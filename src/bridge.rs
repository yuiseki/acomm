use crate::protocol::ProtocolEvent;
use acore::{AgentExecutor, AgentTool, SessionManager};
use std::{collections::VecDeque, error::Error, path::Path, sync::Arc};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{broadcast, Mutex};

const SOCKET_PATH: &str = "/tmp/acomm.sock";
const MAX_BACKLOG: usize = 100;

pub struct BridgeState {
    pub active_tool: AgentTool,
    pub backlog: VecDeque<ProtocolEvent>,
    pub session_manager: SessionManager,
}

pub async fn start_bridge() -> Result<(), Box<dyn Error>> {
    if Path::new(SOCKET_PATH).exists() {
        let _ = std::fs::remove_file(SOCKET_PATH);
    }
    let listener = UnixListener::bind(SOCKET_PATH)?;
    
    let (tx, _rx) = broadcast::channel(100);
    let tx = Arc::new(tx);
    
    let state = Arc::new(Mutex::new(BridgeState {
        active_tool: AgentTool::Gemini,
        backlog: VecDeque::new(),
        session_manager: SessionManager::new(),
    }));

    let mut manager_rx = tx.subscribe();
    let state_for_manager = Arc::clone(&state);
    tokio::spawn(async move {
        while let Ok(event) = manager_rx.recv().await {
            let mut s = state_for_manager.lock().await;
            if matches!(event, ProtocolEvent::Prompt { .. } | ProtocolEvent::AgentLine { .. } | ProtocolEvent::AgentDone | ProtocolEvent::SystemMessage { .. } | ProtocolEvent::ToolSwitched { .. }) {
                s.backlog.push_back(event.clone());
                if s.backlog.len() > MAX_BACKLOG {
                    s.backlog.pop_front();
                }
            }
            if let ProtocolEvent::ToolSwitched { ref tool } = event {
                s.active_tool = tool.clone();
            }
        }
    });

    println!("acomm bridge started at {}", SOCKET_PATH);

    loop {
        let (stream, _) = listener.accept().await?;
        let tx = Arc::clone(&tx);
        let state = Arc::clone(&state);
        tokio::spawn(async move {
            if let Err(e) = handle_bridge_connection(stream, tx, state).await {
                let msg = e.to_string();
                if !msg.contains("Broken pipe") {
                    eprintln!("Bridge connection error: {}", e);
                }
            }
        });
    }
}

async fn handle_bridge_connection(
    mut stream: UnixStream,
    broadcast_tx: Arc<broadcast::Sender<ProtocolEvent>>,
    state: Arc<Mutex<BridgeState>>,
) -> Result<(), Box<dyn Error>> {
    let mut broadcast_rx = broadcast_tx.subscribe();
    let (reader, mut writer) = stream.split();
    let mut lines = BufReader::new(reader).lines();

    {
        let s = state.lock().await;
        let context = AgentExecutor::fetch_context().await;
        let mut initial_payload = String::new();
        if !context.is_empty() {
            let event = ProtocolEvent::SyncContext { context };
            initial_payload.push_str(&serde_json::to_string(&event)?);
            initial_payload.push('\n');
        }
        let tool_event = ProtocolEvent::ToolSwitched { tool: s.active_tool.clone() };
        initial_payload.push_str(&serde_json::to_string(&tool_event)?);
        initial_payload.push('\n');
        for event in &s.backlog {
            initial_payload.push_str(&serde_json::to_string(event)?);
            initial_payload.push('\n');
        }
        let _ = writer.write_all(initial_payload.as_bytes()).await;
    }

    loop {
        let tx_loop = Arc::clone(&broadcast_tx);
        tokio::select! {
            line_res = lines.next_line() => {
                let line = match line_res {
                    Ok(Some(l)) => l,
                    _ => break,
                };
                if let Ok(event) = serde_json::from_str::<ProtocolEvent>(&line) {
                    match event {
                        ProtocolEvent::Prompt { ref text, ref tool, .. } => {
                            if text.starts_with('/') {
                                handle_command(text, &tx_loop, &state).await?;
                            } else {
                                let active_tool = match tool {
                                    Some(t) => t.clone(),
                                    None => state.lock().await.active_tool.clone(),
                                };
                                
                                let _ = tx_loop.send(ProtocolEvent::Prompt { 
                                    text: text.clone(), 
                                    tool: Some(active_tool.clone()), 
                                    channel: event.clone_channel()
                                });
                                
                                let _ = tx_loop.send(ProtocolEvent::StatusUpdate { is_processing: true });
                                
                                let tx_inner = Arc::clone(&tx_loop);
                                let state_inner = Arc::clone(&state);
                                let text_inner = text.clone();
                                
                                // SessionManager をロック外で利用するためにクローン
                                let manager = state_inner.lock().await.session_manager.clone();
                                
                                tokio::spawn(async move {
                                    let tx_line = Arc::clone(&tx_inner);
                                    let tx_err = Arc::clone(&tx_inner);
                                    
                                    // Resume 付き実行
                                    match manager.execute_with_resume(active_tool, &text_inner, move |line| {
                                        let _ = tx_line.send(ProtocolEvent::AgentLine { line });
                                    }).await {
                                        Ok(_) => {},
                                        Err(e) => {
                                            let _ = tx_err.send(ProtocolEvent::SystemMessage { 
                                                msg: format!("Agent execution failed: {}", e), 
                                                channel: Some("bridge".into()) 
                                            });
                                        }
                                    }
                                    let _ = tx_inner.send(ProtocolEvent::AgentDone);
                                    let _ = tx_inner.send(ProtocolEvent::StatusUpdate { is_processing: false });
                                });
                            }
                        }
                        ProtocolEvent::SystemMessage { .. } => {
                            let _ = tx_loop.send(event);
                        }
                        _ => {}
                    }
                }
            }
            event_res = broadcast_rx.recv() => {
                match event_res {
                    Ok(event) => {
                        if let Ok(j) = serde_json::to_string(&event) {
                            if let Err(_) = writer.write_all(format!("{}\n", j).as_bytes()).await {
                                break;
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => { continue; }
                    Err(_) => break,
                }
            }
        }
    }
    Ok(())
}

async fn handle_command(
    text: &str,
    tx: &Arc<broadcast::Sender<ProtocolEvent>>,
    state: &Mutex<BridgeState>,
) -> Result<(), Box<dyn Error>> {
    let parts: Vec<&str> = text[1..].split_whitespace().collect();
    let cmd = parts.get(0).unwrap_or(&"");

    match *cmd {
        "search" => {
            let query = parts[1..].join(" ");
            let output = std::process::Command::new("amem").arg("search").arg(&query).output()?;
            let result = String::from_utf8_lossy(&output.stdout).to_string();
            let _ = tx.send(ProtocolEvent::SystemMessage { 
                msg: format!("Search results for '{}':\n{}", query, result), 
                channel: Some("bridge".into()) 
            });
        }
        "today" => {
            let output = std::process::Command::new("amem").arg("today").output()?;
            let result = String::from_utf8_lossy(&output.stdout).to_string();
            let _ = tx.send(ProtocolEvent::SystemMessage { 
                msg: format!("Today's summary:\n{}", result), 
                channel: Some("bridge".into()) 
            });
        }
        "tool" => {
            if let Some(tool_name) = parts.get(1) {
                let tool = match *tool_name {
                    "gemini" => AgentTool::Gemini,
                    "claude" => AgentTool::Claude,
                    "codex" => AgentTool::Codex,
                    "opencode" => AgentTool::OpenCode,
                    _ => return Ok(()),
                };
                let _ = tx.send(ProtocolEvent::ToolSwitched { tool });
            }
        }
        "clear" => {
            let mut s = state.lock().await;
            s.backlog.clear();
            s.session_manager = SessionManager::new(); // セッションIDもリセット
            let _ = tx.send(ProtocolEvent::SystemMessage { msg: "Backlog and session ID cleared.".to_string(), channel: Some("bridge".into()) });
        }
        "help" => {
            let help = "/search <query> - Search memory\n/today - Show today's summary\n/tool <name> - Switch active CLI\n/clear - Clear backlog and session\n/help - Show this message";
            let _ = tx.send(ProtocolEvent::SystemMessage { msg: help.to_string(), channel: Some("bridge".into()) });
        }
        _ => {
            let _ = tx.send(ProtocolEvent::SystemMessage { msg: format!("Unknown command: /{}", cmd), channel: Some("bridge".into()) });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::ProtocolEvent;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::UnixStream;
    use std::time::Duration;

    #[tokio::test]
    async fn test_bridge_prompt_flow() {
        tokio::spawn(async {
            let _ = start_bridge().await;
        });
        tokio::time::sleep(Duration::from_millis(500)).await;

        let stream = UnixStream::connect(SOCKET_PATH).await.expect("Failed to connect to bridge");
        let (reader, mut writer) = tokio::io::split(stream);
        let mut lines = BufReader::new(reader).lines();

        while let Ok(Ok(Some(_line))) = tokio::time::timeout(Duration::from_millis(500), lines.next_line()).await {
            // 同期中
        }

        let prompt = ProtocolEvent::Prompt {
            text: "一言だけ「pong」と返してください。挨拶は不要です。".to_string(),
            tool: Some(AgentTool::Gemini),
            channel: Some("test".to_string()),
        };
        let j = serde_json::to_string(&prompt).unwrap();
        writer.write_all(format!("{}\n", j).as_bytes()).await.unwrap();

        let mut received_events = Vec::new();
        let start = std::time::Instant::now();
        while start.elapsed() < Duration::from_secs(15) {
            if let Ok(Ok(Some(line))) = tokio::time::timeout(Duration::from_millis(500), lines.next_line()).await {
                let event: ProtocolEvent = serde_json::from_str(&line).unwrap();
                println!("Test received: {:?}", event);
                received_events.push(event);
            }
        }

        assert!(received_events.iter().any(|e| matches!(e, ProtocolEvent::Prompt { .. })), "Prompt not broadcasted");
        assert!(received_events.iter().any(|e| matches!(e, ProtocolEvent::StatusUpdate { is_processing: true })), "Thinking not broadcasted");
        assert!(received_events.iter().any(|e| matches!(e, ProtocolEvent::AgentDone)), "AgentDone not broadcasted");
    }
}
