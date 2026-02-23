use crate::protocol::ProtocolEvent;
use std::error::Error;
use tokio::net::UnixStream;
use tokio::io::AsyncWriteExt;
use serde::{Deserialize, Serialize};
use futures_util::StreamExt;

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

    // Bridge への接続
    let mut bridge_stream = UnixStream::connect(SOCKET_PATH).await.map_err(|e| {
        format!("Bridge is not running. Please start it with 'acomm --bridge'. Error: {}", e)
    })?;

    let url = format!("https://ntfy.sh/{}/json", topic);
    let client = reqwest::Client::new();
    let mut ntfy_stream = client.get(&url).send().await?.bytes_stream();

    println!("Subscribed to ntfy.sh topic: {}", topic);

    while let Some(item) = ntfy_stream.next().await {
        let bytes = item?;
        let line = String::from_utf8_lossy(&bytes);
        
        for json_line in line.lines() {
            if let Ok(msg) = serde_json::from_str::<NtfyMessage>(json_line) {
                if msg.event == "message" {
                    if let Some(text) = msg.message {
                        println!("Received from ntfy: {}", text);
                        let event = transform_ntfy_message(&text, &msg.id);
                        let j = serde_json::to_string(&event)?;
                        bridge_stream.write_all(format!("{}\n", j).as_bytes()).await?;
                    }
                }
            }
        }
    }

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
