use crate::protocol::ProtocolEvent;
use std::error::Error;
use tokio::net::UnixStream;

const SOCKET_PATH: &str = "/tmp/acomm.sock";

pub async fn start_slack_adapter() -> Result<(), Box<dyn Error>> {
    println!("Slack adapter starting...");
    
    // Bridge への接続確認
    let _stream = UnixStream::connect(SOCKET_PATH).await.map_err(|e| {
        format!("Bridge is not running. Please start it with 'acomm --bridge'. Error: {}", e)
    })?;

    println!("Slack Socket Mode connection not yet implemented.");
    
    Ok(())
}

/// Slack のメッセージイベントを ProtocolEvent::Prompt に変換します。
#[allow(dead_code)]
pub fn transform_slack_message(text: &str, user_id: &str) -> ProtocolEvent {
    ProtocolEvent::Prompt {
        text: text.to_string(),
        tool: None,
        channel: Some(format!("slack:{}", user_id)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transform_slack_message() {
        let event = transform_slack_message("hello执事", "U12345");
        if let ProtocolEvent::Prompt { text, channel, .. } = event {
            assert_eq!(text, "hello执事");
            assert_eq!(channel, Some("slack:U12345".to_string()));
        } else {
            panic!("Transform failed to produce a Prompt event");
        }
    }
}
