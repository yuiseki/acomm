/**
 * Slack Socket Mode adapter for acomm bridge.
 *
 * Connects to Slack's Socket Mode WebSocket endpoint to receive events,
 * forwards them to the acomm bridge as ProtocolEvent::Prompt,
 * and replies via the Slack Web API (chat.postMessage) when the agent finishes.
 *
 * Required environment variables:
 *   SLACK_APP_TOKEN  — xapp-... App-Level Token with connections:write scope
 *   SLACK_BOT_TOKEN  — xoxb-... Bot Token with chat:write scope
 *
 * Required bot scopes: app_mentions:read, channels:history, chat:write
 * Required event subscriptions: message.channels (or app_mention)
 */

use crate::protocol::ProtocolEvent;
use std::collections::HashMap;
use std::error::Error;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

const SOCKET_PATH: &str = "/tmp/acomm.sock";
const SLACK_API_BASE: &str = "https://slack.com/api";

// ─── Slack Socket Mode payload types ──────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct SocketModeEnvelope {
    #[serde(rename = "type")]
    envelope_type: String,
    #[serde(default)]
    envelope_id: String,
    #[serde(default)]
    payload: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub struct SlackMessageEvent {
    pub channel: String,
    pub user: Option<String>,
    pub text: Option<String>,
    /// Present when the message is from a bot
    pub bot_id: Option<String>,
    pub subtype: Option<String>,
}

// ─── Public adapter entry point ───────────────────────────────────────────────

pub async fn start_slack_adapter() -> Result<(), Box<dyn Error>> {
    let app_token = std::env::var("SLACK_APP_TOKEN")
        .map_err(|_| "SLACK_APP_TOKEN environment variable not set (xapp-...)")?;
    let bot_token = std::env::var("SLACK_BOT_TOKEN")
        .map_err(|_| "SLACK_BOT_TOKEN environment variable not set (xoxb-...)")?;

    println!("Slack Socket Mode adapter starting...");

    // Connect to acomm bridge
    let bridge_stream = UnixStream::connect(SOCKET_PATH).await.map_err(|e| {
        format!(
            "Bridge is not running. Please start it with 'acomm --bridge'. Error: {}",
            e
        )
    })?;
    let (bridge_reader, mut bridge_writer) = tokio::io::split(bridge_stream);
    let mut bridge_lines = BufReader::new(bridge_reader).lines();

    // Obtain WebSocket URL from Slack
    let ws_url = open_socket_mode_connection(&app_token).await?;
    println!("Connecting to Slack Socket Mode WebSocket...");

    let (ws_stream, _) = connect_async(&ws_url).await?;
    let (mut ws_sink, mut ws_stream) = ws_stream.split();

    println!("Connected to Slack Socket Mode.");

    let mut reply_buffers: HashMap<String, String> = HashMap::new();

    loop {
        tokio::select! {
            // Slack Socket Mode messages
            ws_msg = ws_stream.next() => {
                let msg = match ws_msg {
                    Some(Ok(m)) => m,
                    Some(Err(e)) => return Err(format!("WebSocket error: {}", e).into()),
                    None => return Err("Slack Socket Mode disconnected".into()),
                };

                let text = match msg {
                    Message::Text(t) => t,
                    Message::Ping(data) => {
                        ws_sink.send(Message::Pong(data)).await?;
                        continue;
                    }
                    Message::Close(_) => return Err("Slack closed the WebSocket connection".into()),
                    _ => continue,
                };

                let envelope: SocketModeEnvelope = match serde_json::from_str(&text) {
                    Ok(e) => e,
                    Err(_) => continue,
                };

                match envelope.envelope_type.as_str() {
                    "hello" => {
                        println!("Slack Socket Mode hello received.");
                    }
                    "events_api" => {
                        // Acknowledge the event immediately to avoid retries
                        if !envelope.envelope_id.is_empty() {
                            let ack = json!({ "envelope_id": envelope.envelope_id });
                            ws_sink.send(Message::Text(serde_json::to_string(&ack)?.into())).await?;
                        }

                        if let Some(payload) = envelope.payload {
                            if let Ok(event) = serde_json::from_value::<SlackMessageEvent>(
                                payload["event"].clone(),
                            ) {
                                handle_slack_event(event, &mut bridge_writer).await?;
                            }
                        }
                    }
                    "disconnect" => {
                        return Err("Slack requested disconnect".into());
                    }
                    _ => {}
                }
            }

            // Bridge protocol events
            line_res = bridge_lines.next_line() => {
                let line = match line_res? {
                    Some(l) => l,
                    None => break,
                };
                if let Ok(event) = serde_json::from_str::<ProtocolEvent>(&line) {
                    match event {
                        ProtocolEvent::Prompt { channel: Some(ref ch), .. }
                            if ch.starts_with("slack:") =>
                        {
                            reply_buffers.insert(ch.clone(), String::new());
                        }
                        ProtocolEvent::AgentChunk { ref chunk, channel: Some(ref ch) }
                            if ch.starts_with("slack:") =>
                        {
                            reply_buffers.entry(ch.clone()).or_default().push_str(chunk);
                        }
                        ProtocolEvent::AgentDone { channel: Some(ref ch) }
                            if ch.starts_with("slack:") =>
                        {
                            // Channel format: "slack:<user_id>:<channel_id>"
                            let parts: Vec<&str> = ch.splitn(3, ':').collect();
                            let slack_channel = parts.get(2).unwrap_or(&"");
                            let key = ch.to_string();
                            if let Some(content) = reply_buffers.remove(&key) {
                                if !content.is_empty() {
                                    send_slack_message(&bot_token, slack_channel, &content).await?;
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

// ─── Helpers ──────────────────────────────────────────────────────────────────

/// Call apps.connections.open to get a fresh WebSocket URL.
async fn open_socket_mode_connection(app_token: &str) -> Result<String, Box<dyn Error>> {
    let client = reqwest::Client::new();
    let res: Value = client
        .post(format!("{}/apps.connections.open", SLACK_API_BASE))
        .header("Authorization", format!("Bearer {}", app_token))
        .header("Content-Type", "application/x-www-form-urlencoded")
        .send()
        .await?
        .json()
        .await?;

    if res["ok"].as_bool() != Some(true) {
        return Err(format!("apps.connections.open failed: {}", res).into());
    }
    res["url"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "Missing WebSocket URL in Slack response".into())
}

/// Process a Slack message event and forward it to the bridge if appropriate.
async fn handle_slack_event<W>(
    event: SlackMessageEvent,
    bridge_writer: &mut W,
) -> Result<(), Box<dyn Error>>
where
    W: AsyncWriteExt + Unpin,
{
    // Skip bot messages, subtypes (edits, joins, etc.), and empty messages
    if event.bot_id.is_some() { return Ok(()); }
    if event.subtype.is_some() { return Ok(()); }
    let text = match event.text {
        Some(ref t) if !t.is_empty() => t.clone(),
        _ => return Ok(()),
    };
    let user_id = event.user.as_deref().unwrap_or("unknown");
    let protocol_event = transform_slack_message(&text, user_id, &event.channel);
    let j = serde_json::to_string(&protocol_event)?;
    bridge_writer.write_all(format!("{}\n", j).as_bytes()).await?;
    Ok(())
}

/// Send a message to a Slack channel via chat.postMessage.
async fn send_slack_message(
    bot_token: &str,
    channel: &str,
    text: &str,
) -> Result<(), Box<dyn Error>> {
    let client = reqwest::Client::new();
    client
        .post(format!("{}/chat.postMessage", SLACK_API_BASE))
        .header("Authorization", format!("Bearer {}", bot_token))
        .json(&json!({ "channel": channel, "text": text }))
        .send()
        .await?;
    Ok(())
}

// ─── Public transformation helpers ────────────────────────────────────────────

/// Convert a Slack message event to a ProtocolEvent::Prompt for the bridge.
///
/// Channel format: `slack:<user_id>:<slack_channel_id>`
pub fn transform_slack_message(text: &str, user_id: &str, slack_channel: &str) -> ProtocolEvent {
    ProtocolEvent::Prompt {
        text: text.to_string(),
        tool: None,
        channel: Some(format!("slack:{}:{}", user_id, slack_channel)),
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transform_slack_message() {
        let event = transform_slack_message("hello執事", "U12345", "C98765");
        if let ProtocolEvent::Prompt { text, channel, tool } = event {
            assert_eq!(text, "hello執事");
            assert_eq!(channel, Some("slack:U12345:C98765".to_string()));
            assert!(tool.is_none());
        } else {
            panic!("Transform failed to produce a Prompt event");
        }
    }

    #[test]
    fn test_transform_slack_message_channel_prefix() {
        let event = transform_slack_message("test", "Uabc", "Cdef");
        if let ProtocolEvent::Prompt { channel, .. } = event {
            let ch = channel.unwrap();
            assert!(ch.starts_with("slack:"), "Channel must start with 'slack:'");
            let parts: Vec<&str> = ch.splitn(3, ':').collect();
            assert_eq!(parts.len(), 3, "Channel must have 3 parts: slack:user_id:channel_id");
            assert_eq!(parts[1], "Uabc");
            assert_eq!(parts[2], "Cdef");
        } else {
            panic!("Not a Prompt event");
        }
    }

    #[test]
    fn test_transform_slack_message_unknown_user() {
        let event = transform_slack_message("hi", "unknown", "C001");
        if let ProtocolEvent::Prompt { channel, .. } = event {
            assert_eq!(channel, Some("slack:unknown:C001".to_string()));
        } else {
            panic!("Not a Prompt event");
        }
    }

    #[test]
    fn test_transform_slack_message_preserves_cjk() {
        let event = transform_slack_message("おはようございます！", "U999", "C888");
        if let ProtocolEvent::Prompt { text, .. } = event {
            assert_eq!(text, "おはようございます！");
        } else {
            panic!("Not a Prompt event");
        }
    }
}
