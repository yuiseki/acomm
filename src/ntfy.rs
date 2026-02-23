use crate::protocol::ProtocolEvent;
use std::error::Error;
use tokio::net::UnixStream;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use serde::{Deserialize, Serialize};
use futures_util::StreamExt;
use std::collections::HashMap;

const SOCKET_PATH: &str = "/tmp/acomm.sock";

#[derive(Debug, Serialize, Deserialize)]
struct NtfyMessage {
    id: String,
    time: u64,
    event: String,
    topic: String,
    message: Option<String>,
    title: Option<String>,
}

pub async fn start_ntfy_adapter() -> Result<(), Box<dyn Error>> {
    let topic = std::env::var("NTFY_TOPIC").map_err(|_| "NTFY_TOPIC environment variable not set")?;
    println!("ntfy adapter starting for topic: {}", topic);

    // Bridge への双方向接続
    let stream = UnixStream::connect(SOCKET_PATH).await.map_err(|e| {
        format!("Bridge is not running. Please start it with 'acomm --bridge'. Error: {}", e)
    })?;
    let (reader, mut writer) = tokio::io::split(stream);
    let mut bridge_lines = BufReader::new(reader).lines();

    // ntfy.sh 購読ストリーム
    let url = format!("https://ntfy.sh/{}/json", topic);
    let client = reqwest::Client::new();
    let mut ntfy_stream = client.get(&url).send().await?.bytes_stream();

    println!("Subscribed to ntfy.sh topic: {}", topic);

    // 回答のバッファ管理 (msg_id -> content)
    let mut reply_buffers: HashMap<String, String> = HashMap::new();

    loop {
        tokio::select! {
            // お嬢様からの命令を受信 (ntfy -> Bridge)
            Some(item) = ntfy_stream.next() => {
                let bytes = item?;
                let line = String::from_utf8_lossy(&bytes);
                for json_line in line.lines() {
                    if let Ok(msg) = serde_json::from_str::<NtfyMessage>(json_line) {
                        if msg.event == "message" {
                            if let Some(text) = msg.message {
                                // 執事自身の通知（返信）を無限ループしないよう除外
                                if text.starts_with("[bot]") { continue; }
                                
                                println!("Received from ntfy: {}", text);
                                let event = transform_ntfy_message(&text, &msg.id);
                                let j = serde_json::to_string(&event)?;
                                writer.write_all(format!("{}\n", j).as_bytes()).await?;
                            }
                        }
                    }
                }
            }
            // 執事からの回答を受信 (Bridge -> ntfy)
            line_res = bridge_lines.next_line() => {
                let line = match line_res? {
                    Some(l) => l,
                    None => break,
                };
                if let Ok(event) = serde_json::from_str::<ProtocolEvent>(&line) {
                    match event {
                        ProtocolEvent::AgentChunk { chunk } => {
                            // ntfy チャネル由来のイベントか判定
                            // TODO: Bridge 側で Prompt の channel を AgentChunk に引き継ぐ実装が必要
                            // 現状はアクティブな ntfy バッファがあればそこに追記する暫定対応
                            if let Some(buf) = reply_buffers.values_mut().next() {
                                buf.push_str(&chunk);
                            }
                        }
                        ProtocolEvent::Prompt { channel: Some(ref ch), .. } if ch.starts_with("ntfy:") => {
                            let msg_id = ch.replace("ntfy:", "");
                            reply_buffers.insert(msg_id, String::new());
                        }
                        ProtocolEvent::AgentDone => {
                            // 全ての ntfy バッファを送信してクリア
                            let ids: Vec<String> = reply_buffers.keys().cloned().collect();
                            for id in ids {
                                if let Some(content) = reply_buffers.remove(&id) {
                                    if !content.is_empty() {
                                        send_to_ntfy(&topic, &content).await?;
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    Ok(())
}

async fn send_to_ntfy(topic: &str, message: &str) -> Result<(), Box<dyn Error>> {
    let client = reqwest::Client::new();
    let url = format!("https://ntfy.sh/{}", topic);
    // 無料枠を尊重し、プレフィックスを付けて送信
    let payload = format!("[bot] {}", message);
    client.post(&url).body(payload).send().await?;
    Ok(())
}

pub fn transform_ntfy_message(text: &str, msg_id: &str) -> ProtocolEvent {
    ProtocolEvent::Prompt {
        text: text.to_string(),
        tool: None,
        channel: Some(format!("ntfy:{}", msg_id)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transform_ntfy_message() {
        let event = transform_ntfy_message("hello", "msg123");
        if let ProtocolEvent::Prompt { text, channel, .. } = event {
            assert_eq!(text, "hello");
            assert_eq!(channel, Some("ntfy:msg123".to_string()));
        } else {
            panic!("Failed to transform ntfy message");
        }
    }
}
