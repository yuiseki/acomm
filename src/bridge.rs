use crate::protocol::ProtocolEvent;
use acore::{AgentExecutor, AgentProvider, SessionManager};
use std::{collections::VecDeque, error::Error, path::Path, sync::Arc};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{broadcast, Mutex};

const SOCKET_PATH: &str = "/tmp/acomm.sock";
const MAX_BACKLOG: usize = 100;
const DEFAULT_PROVIDER: AgentProvider = AgentProvider::Gemini;
const DEFAULT_GEMINI_MODEL: &str = "auto-gemini-3";
const DEFAULT_CLAUDE_MODEL: &str = "claude-sonnet-4-6";
const DEFAULT_CODEX_MODEL: &str = "gpt-5.3-codex";

#[derive(Clone, Debug, PartialEq, Eq)]
struct ProviderPreset {
    provider: AgentProvider,
    model: &'static str,
}

fn default_model_for_provider(provider: &AgentProvider) -> Option<&'static str> {
    match provider {
        AgentProvider::Gemini => Some(DEFAULT_GEMINI_MODEL),
        AgentProvider::Claude => Some(DEFAULT_CLAUDE_MODEL),
        AgentProvider::Codex => Some(DEFAULT_CODEX_MODEL),
        AgentProvider::Dummy => Some("echo"),
        AgentProvider::Mock => Some("mock-model"),
        AgentProvider::OpenCode => None,
    }
}

fn discord_magic_provider_preset(text: &str, channel: Option<&str>) -> Option<ProviderPreset> {
    if !channel.unwrap_or_default().starts_with("discord:") {
        return None;
    }

    match text.trim() {
        "p-gemini" => Some(ProviderPreset {
            provider: AgentProvider::Gemini,
            model: DEFAULT_GEMINI_MODEL,
        }),
        "p-codex" => Some(ProviderPreset {
            provider: AgentProvider::Codex,
            model: DEFAULT_CODEX_MODEL,
        }),
        "p-claude" => Some(ProviderPreset {
            provider: AgentProvider::Claude,
            model: DEFAULT_CLAUDE_MODEL,
        }),
        _ => None,
    }
}

fn apply_provider_preset(
    tx: &Arc<broadcast::Sender<ProtocolEvent>>,
    channel: Option<String>,
    preset: ProviderPreset,
) {
    let provider_name = preset.provider.command_name().to_string();
    let _ = tx.send(ProtocolEvent::ProviderSwitched {
        provider: preset.provider.clone(),
    });
    let _ = tx.send(ProtocolEvent::ModelSwitched {
        model: preset.model.to_string(),
    });
    let _ = tx.send(ProtocolEvent::SystemMessage {
        msg: format!("Switched to {}:{}.", provider_name, preset.model),
        channel,
    });
}

pub struct BridgeState {
    pub active_provider: AgentProvider,
    pub active_model: Option<String>,
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
        active_provider: DEFAULT_PROVIDER,
        active_model: default_model_for_provider(&DEFAULT_PROVIDER).map(str::to_string),
        backlog: VecDeque::new(),
        session_manager: SessionManager::new(),
    }));

    let mut manager_rx = tx.subscribe();
    let state_for_manager = Arc::clone(&state);
    tokio::spawn(async move {
        while let Ok(event) = manager_rx.recv().await {
            let mut s = state_for_manager.lock().await;
            if matches!(event,
                ProtocolEvent::Prompt { .. }
                | ProtocolEvent::AgentChunk { .. }
                | ProtocolEvent::AgentDone { .. }
                | ProtocolEvent::SystemMessage { .. }
                | ProtocolEvent::ProviderSwitched { .. }
                | ProtocolEvent::ModelSwitched { .. }
            ) {
                s.backlog.push_back(event.clone());
                if s.backlog.len() > MAX_BACKLOG {
                    s.backlog.pop_front();
                }
            }
            if let ProtocolEvent::ProviderSwitched { ref provider } = event {
                s.active_provider = provider.clone();
                // Reset model selection when provider changes
                s.active_model = default_model_for_provider(provider).map(str::to_string);
            }
            if let ProtocolEvent::ModelSwitched { ref model } = event {
                s.active_model = Some(model.clone());
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
        let provider_event = ProtocolEvent::ProviderSwitched { provider: s.active_provider.clone() };
        initial_payload.push_str(&serde_json::to_string(&provider_event)?);
        initial_payload.push('\n');
        if let Some(ref model) = s.active_model {
            let model_event = ProtocolEvent::ModelSwitched { model: model.clone() };
            initial_payload.push_str(&serde_json::to_string(&model_event)?);
            initial_payload.push('\n');
        }
        for event in &s.backlog {
            initial_payload.push_str(&serde_json::to_string(event)?);
            initial_payload.push('\n');
        }
        let sync_done = ProtocolEvent::BridgeSyncDone {};
        initial_payload.push_str(&serde_json::to_string(&sync_done)?);
        initial_payload.push('\n');
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
                        ProtocolEvent::Prompt { ref text, ref provider, .. } => {
                            let channel = event.clone_channel();
                            if let Some(preset) = discord_magic_provider_preset(text, channel.as_deref()) {
                                apply_provider_preset(&tx_loop, channel, preset);
                                continue;
                            }
                            if text.starts_with('/') {
                                handle_command(text, &tx_loop, &state).await?;
                            } else {
                                let (active_provider, active_model, manager) = {
                                    let s = state.lock().await;
                                    let selected_provider = match provider {
                                        Some(t) => t.clone(),
                                        None => s.active_provider.clone(),
                                    };
                                    let selected_model = if selected_provider == s.active_provider {
                                        s.active_model.clone()
                                    } else {
                                        default_model_for_provider(&selected_provider).map(str::to_string)
                                    };
                                    (selected_provider, selected_model, s.session_manager.clone())
                                };
                                let _ = tx_loop.send(ProtocolEvent::Prompt { 
                                    text: text.clone(), 
                                    provider: Some(active_provider.clone()), 
                                    channel: channel.clone()
                                });
                                let _ = tx_loop.send(ProtocolEvent::StatusUpdate { is_processing: true, channel: channel.clone() });
                                
                                let tx_inner = Arc::clone(&tx_loop);
                                let text_inner = text.clone();
                                let channel_inner = channel.clone();
                                let active_model_inner = active_model.clone();
                                
                                tokio::spawn(async move {
                                    let tx_chunk = Arc::clone(&tx_inner);
                                    let tx_err = Arc::clone(&tx_inner);
                                    let ch_chunk = channel_inner.clone();
                                    match manager.execute_with_resume_with_model(
                                        active_provider,
                                        active_model_inner,
                                        &text_inner,
                                        move |chunk| {
                                        let _ = tx_chunk.send(ProtocolEvent::AgentChunk { chunk, channel: ch_chunk.clone() });
                                    }).await {
                                        Ok(_) => {},
                                        Err(e) => {
                                            let _ = tx_err.send(ProtocolEvent::SystemMessage { 
                                                msg: format!("Agent execution failed: {}", e), 
                                                channel: channel_inner.clone()
                                            });
                                        }
                                    }
                                    let _ = tx_inner.send(ProtocolEvent::AgentDone { channel: channel_inner.clone() });
                                    let _ = tx_inner.send(ProtocolEvent::StatusUpdate { is_processing: false, channel: channel_inner });
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
            let _ = tx.send(ProtocolEvent::SystemMessage { msg: format!("Search results:\n{result}"), channel: Some("bridge".into()) });
        }
        "today" => {
            let output = std::process::Command::new("amem").arg("today").output()?;
            let result = String::from_utf8_lossy(&output.stdout).to_string();
            let _ = tx.send(ProtocolEvent::SystemMessage { msg: format!("Today:\n{result}"), channel: Some("bridge".into()) });
        }
        "provider" => {
            if let Some(name) = parts.get(1) {
                let provider = match *name {
                    "gemini" => AgentProvider::Gemini,
                    "claude" => AgentProvider::Claude,
                    "codex" => AgentProvider::Codex,
                    "opencode" => AgentProvider::OpenCode,
                    "dummy" | "dummy-bot" | "dummybot" => AgentProvider::Dummy,
                    "mock" => AgentProvider::Mock,
                    _ => return Ok(()),
                };
                let default_model = default_model_for_provider(&provider).map(str::to_string);
                let _ = tx.send(ProtocolEvent::ProviderSwitched { provider });
                if let Some(model) = default_model {
                    let _ = tx.send(ProtocolEvent::ModelSwitched { model });
                }
            }
        }
        "model" => {
            if let Some(model_name) = parts.get(1) {
                let _ = tx.send(ProtocolEvent::ModelSwitched { model: model_name.to_string() });
            }
        }
        "clear" => {
            let mut s = state.lock().await;
            s.backlog.clear();
            s.session_manager = SessionManager::new();
            s.active_model = default_model_for_provider(&s.active_provider).map(str::to_string);
            let cleared_model = s.active_model.clone();
            let _ = tx.send(ProtocolEvent::SystemMessage { msg: "Cleared.".into(), channel: Some("bridge".into()) });
            if let Some(model) = cleared_model {
                let _ = tx.send(ProtocolEvent::ModelSwitched { model });
            }
        }
        _ => {}
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

    // ブリッジテストは同じソケットパスを使うため並列実行すると競合する。
    // static Mutex で排他制御し、常に1テストずつ実行する。
    static BRIDGE_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[tokio::test]
    async fn test_bridge_mock_flow() {
        let _guard = BRIDGE_TEST_LOCK.lock().unwrap();
        let _ = std::fs::remove_file(SOCKET_PATH);
        tokio::spawn(async { let _ = start_bridge().await; });
        tokio::time::sleep(Duration::from_millis(500)).await;
        
        let stream = UnixStream::connect(SOCKET_PATH).await.expect("Failed to connect");
        let (reader, mut writer) = tokio::io::split(stream);
        let mut lines = BufReader::new(reader).lines();
        
        while let Ok(Ok(Some(line))) = tokio::time::timeout(Duration::from_millis(200), lines.next_line()).await {
            let _ = serde_json::from_str::<ProtocolEvent>(&line);
        }

        let prompt = ProtocolEvent::Prompt { 
            text: "hello mock".into(), 
            provider: Some(AgentProvider::Mock), 
            channel: Some("test_channel".into()) 
        };
        writer.write_all(format!("{}\n", serde_json::to_string(&prompt).unwrap()).as_bytes()).await.unwrap();
        
        let mut received = Vec::new();
        let start = std::time::Instant::now();
        while start.elapsed() < Duration::from_secs(5) {
            if let Ok(Ok(Some(line))) = tokio::time::timeout(Duration::from_millis(500), lines.next_line()).await {
                let ev: ProtocolEvent = serde_json::from_str(&line).unwrap();
                received.push(ev);
            }
        }
        
        assert!(received.iter().any(|e| matches!(e, ProtocolEvent::StatusUpdate { channel: Some(c), .. } if c == "test_channel")));
        assert!(received.iter().any(|e| matches!(e, ProtocolEvent::AgentChunk { channel: Some(c), .. } if c == "test_channel")));
        assert!(received.iter().any(|e| matches!(e, ProtocolEvent::AgentDone { channel: Some(c), .. } if c == "test_channel")));
    }

    #[tokio::test]
    async fn test_bridge_initial_sync_emits_completion_marker() {
        let _guard = BRIDGE_TEST_LOCK.lock().unwrap();
        let _ = std::fs::remove_file(SOCKET_PATH);
        tokio::spawn(async { let _ = start_bridge().await; });
        tokio::time::sleep(Duration::from_millis(500)).await;

        let stream = UnixStream::connect(SOCKET_PATH).await.expect("Failed to connect");
        let (reader, _) = tokio::io::split(stream);
        let mut lines = BufReader::new(reader).lines();

        let mut saw_marker = false;
        let start = std::time::Instant::now();
        while start.elapsed() < Duration::from_secs(2) {
            if let Ok(Ok(Some(line))) = tokio::time::timeout(Duration::from_millis(200), lines.next_line()).await {
                if line.contains("\"BridgeSyncDone\"") {
                    saw_marker = true;
                    break;
                }
            } else {
                break;
            }
        }

        assert!(saw_marker, "bridge should emit BridgeSyncDone after initial sync payload");
    }

    #[tokio::test]
    async fn test_bridge_initial_sync_emits_gemini_default_provider_and_model() {
        let _guard = BRIDGE_TEST_LOCK.lock().unwrap();
        let _ = std::fs::remove_file(SOCKET_PATH);
        tokio::spawn(async { let _ = start_bridge().await; });
        tokio::time::sleep(Duration::from_millis(500)).await;

        let stream = UnixStream::connect(SOCKET_PATH).await.expect("Failed to connect");
        let (reader, _) = tokio::io::split(stream);
        let mut lines = BufReader::new(reader).lines();

        let mut saw_provider = false;
        let mut saw_model = false;
        let start = std::time::Instant::now();
        while start.elapsed() < Duration::from_secs(2) {
            let line = match tokio::time::timeout(Duration::from_millis(200), lines.next_line()).await {
                Ok(Ok(Some(line))) => line,
                _ => break,
            };

            let ev: ProtocolEvent = match serde_json::from_str(&line) {
                Ok(ev) => ev,
                Err(_) => continue,
            };

            match ev {
                ProtocolEvent::ProviderSwitched { provider } if provider == AgentProvider::Gemini => saw_provider = true,
                ProtocolEvent::ModelSwitched { model } if model == "auto-gemini-3" => saw_model = true,
                ProtocolEvent::BridgeSyncDone {} => break,
                _ => {}
            }
        }

        assert!(saw_provider, "initial sync should include Gemini default provider");
        assert!(saw_model, "initial sync should include auto-gemini-3 default model");
    }

    #[tokio::test]
    async fn test_handle_command_provider_dummy_switches_provider() {
        let (tx, mut rx) = broadcast::channel(8);
        let tx = Arc::new(tx);
        let state = Mutex::new(BridgeState {
            active_provider: AgentProvider::Gemini,
            active_model: None,
            backlog: VecDeque::new(),
            session_manager: SessionManager::new(),
        });

        handle_command("/provider dummy", &tx, &state).await.unwrap();

        let ev = rx.recv().await.unwrap();
        assert!(matches!(ev, ProtocolEvent::ProviderSwitched { provider: AgentProvider::Dummy }));
    }

    #[tokio::test]
    async fn test_handle_command_provider_codex_emits_default_model() {
        let (tx, mut rx) = broadcast::channel(8);
        let tx = Arc::new(tx);
        let state = Mutex::new(BridgeState {
            active_provider: AgentProvider::Gemini,
            active_model: Some("auto-gemini-3".into()),
            backlog: VecDeque::new(),
            session_manager: SessionManager::new(),
        });

        handle_command("/provider codex", &tx, &state).await.unwrap();

        let ev1 = rx.recv().await.unwrap();
        let ev2 = rx.recv().await.unwrap();
        assert!(matches!(ev1, ProtocolEvent::ProviderSwitched { provider: AgentProvider::Codex }));
        assert!(matches!(ev2, ProtocolEvent::ModelSwitched { model } if model == "gpt-5.3-codex"));
    }

    #[test]
    fn test_discord_magic_provider_preset_for_gemini() {
        let preset = discord_magic_provider_preset("p-gemini", Some("discord:1:2"))
            .expect("p-gemini should map to a preset");
        assert_eq!(preset.provider, AgentProvider::Gemini);
        assert_eq!(preset.model, "auto-gemini-3");
    }

    #[test]
    fn test_discord_magic_provider_preset_for_codex_and_claude() {
        let codex = discord_magic_provider_preset("p-codex", Some("discord:1:2"))
            .expect("p-codex should map to codex preset");
        assert_eq!(codex.provider, AgentProvider::Codex);
        assert_eq!(codex.model, "gpt-5.3-codex");

        let claude = discord_magic_provider_preset("p-claude", Some("discord:1:2"))
            .expect("p-claude should map to claude preset");
        assert_eq!(claude.provider, AgentProvider::Claude);
        assert_eq!(claude.model, "claude-sonnet-4-6");
    }

    #[test]
    fn test_discord_magic_provider_preset_ignores_non_discord_or_unknown_text() {
        assert!(discord_magic_provider_preset("p-gemini", Some("tui")).is_none());
        assert!(discord_magic_provider_preset("p-unknown", Some("discord:1:2")).is_none());
        assert!(discord_magic_provider_preset("hello", Some("discord:1:2")).is_none());
    }
}
