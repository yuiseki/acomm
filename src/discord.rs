/**
 * Discord adapter for acomm bridge.
 *
 * Connects to the Discord Gateway WebSocket API to receive messages,
 * forwards them to the acomm bridge as ProtocolEvent::Prompt,
 * and replies via the Discord REST API when the agent finishes.
 *
 * Required environment variables:
 *   DISCORD_BOT_TOKEN — bot token from the Discord Developer Portal
 *
 * Required bot intents (set in Developer Portal):
 *   GUILD_MESSAGES (1 << 9) = 512
 *   MESSAGE_CONTENT (1 << 15) = 32768
 */

use crate::protocol::ProtocolEvent;
use std::collections::HashMap;
use std::error::Error;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

const SOCKET_PATH: &str = "/tmp/acomm.sock";
const DISCORD_GATEWAY_URL: &str = "wss://gateway.discord.gg/?v=10&encoding=json";
const DISCORD_API_BASE: &str = "https://discord.com/api/v10";

/// Gateway opcodes
const OP_DISPATCH: u64 = 0;
const OP_HEARTBEAT: u64 = 1;
const OP_IDENTIFY: u64 = 2;
const OP_HELLO: u64 = 10;
const OP_HEARTBEAT_ACK: u64 = 11;

/// Gateway intents: GUILD_MESSAGES | MESSAGE_CONTENT
const GATEWAY_INTENTS: u64 = (1 << 9) | (1 << 15);

#[derive(Debug, Serialize, Deserialize)]
struct GatewayPayload {
    op: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    d: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    s: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    t: Option<String>,
}

/// Represents a minimal Discord message event.
#[derive(Debug, Serialize, Deserialize)]
pub struct DiscordMessage {
    pub id: String,
    pub channel_id: String,
    pub content: String,
    pub author: DiscordUser,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DiscordUser {
    pub id: String,
    pub username: String,
    pub bot: Option<bool>,
}

pub async fn start_discord_adapter() -> Result<(), Box<dyn Error>> {
    let token = std::env::var("DISCORD_BOT_TOKEN")
        .map_err(|_| "DISCORD_BOT_TOKEN environment variable not set")?;

    println!("Discord adapter starting...");

    let bridge_stream = UnixStream::connect(SOCKET_PATH).await.map_err(|e| {
        format!(
            "Bridge is not running. Please start it with 'acomm --bridge'. Error: {}",
            e
        )
    })?;
    let (bridge_reader, mut bridge_writer) = tokio::io::split(bridge_stream);
    let mut bridge_lines = BufReader::new(bridge_reader).lines();

    let (ws_stream, _) = connect_async(DISCORD_GATEWAY_URL).await?;
    let (mut ws_sink, mut ws_stream) = ws_stream.split();

    println!("Connected to Discord Gateway.");

    let mut heartbeat_interval_ms: u64 = 41250; // default fallback
    let mut sequence: Option<u64> = None;
    let mut bot_user_id: Option<String> = None;
    let mut reply_buffers: HashMap<String, String> = HashMap::new();

    // Heartbeat ticker (fires after first HELLO)
    let mut heartbeat_ticker: Option<tokio::time::Interval> = None;

    loop {
        tokio::select! {
            // Discord Gateway messages
            ws_msg = ws_stream.next() => {
                let msg = match ws_msg {
                    Some(Ok(m)) => m,
                    Some(Err(e)) => return Err(format!("WebSocket error: {}", e).into()),
                    None => return Err("Discord Gateway disconnected".into()),
                };

                let text = match msg {
                    Message::Text(t) => t,
                    Message::Close(_) => return Err("Discord Gateway closed connection".into()),
                    _ => continue,
                };

                let payload: GatewayPayload = match serde_json::from_str(&text) {
                    Ok(p) => p,
                    Err(_) => continue,
                };

                match payload.op {
                    OP_HELLO => {
                        if let Some(d) = &payload.d {
                            if let Some(interval) = d["heartbeat_interval"].as_u64() {
                                heartbeat_interval_ms = interval;
                            }
                        }
                        // Start heartbeat
                        heartbeat_ticker = Some(tokio::time::interval(
                            std::time::Duration::from_millis(heartbeat_interval_ms),
                        ));
                        // Send IDENTIFY
                        let identify = GatewayPayload {
                            op: OP_IDENTIFY,
                            d: Some(json!({
                                "token": token,
                                "intents": GATEWAY_INTENTS,
                                "properties": {
                                    "os": "linux",
                                    "browser": "acomm",
                                    "device": "acomm"
                                }
                            })),
                            s: None,
                            t: None,
                        };
                        ws_sink.send(Message::Text(serde_json::to_string(&identify)?.into())).await?;
                        println!("Sent IDENTIFY to Discord Gateway.");
                    }
                    OP_HEARTBEAT_ACK => {
                        // Heartbeat acknowledged — connection is healthy.
                    }
                    OP_HEARTBEAT => {
                        // Server-requested heartbeat
                        let hb = GatewayPayload { op: OP_HEARTBEAT, d: sequence.map(|s| json!(s)), s: None, t: None };
                        ws_sink.send(Message::Text(serde_json::to_string(&hb)?.into())).await?;
                    }
                    OP_DISPATCH => {
                        sequence = payload.s;
                        match payload.t.as_deref() {
                            Some("READY") => {
                                if let Some(d) = &payload.d {
                                    if let Some(uid) = d["user"]["id"].as_str() {
                                        bot_user_id = Some(uid.to_string());
                                        println!("Discord READY. Bot user id: {}", uid);
                                    }
                                }
                            }
                            Some("MESSAGE_CREATE") => {
                                if let Some(d) = &payload.d {
                                    if let Ok(msg) = serde_json::from_value::<DiscordMessage>(d.clone()) {
                                        // Ignore messages from the bot itself
                                        if let Some(ref bot_id) = bot_user_id {
                                            if &msg.author.id == bot_id { continue; }
                                        }
                                        // Ignore messages from other bots
                                        if msg.author.bot.unwrap_or(false) { continue; }

                                        if msg.content.is_empty() { continue; }

                                        let event = transform_discord_message(
                                            &msg.content,
                                            &msg.channel_id,
                                            &msg.id,
                                        );
                                        let j = serde_json::to_string(&event)?;
                                        bridge_writer.write_all(format!("{}\n", j).as_bytes()).await?;
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }

            // Heartbeat timer
            _ = async {
                if let Some(ref mut ticker) = heartbeat_ticker {
                    ticker.tick().await
                } else {
                    // If heartbeat not yet set up, wait forever
                    std::future::pending::<tokio::time::Instant>().await
                }
            } => {
                let hb = GatewayPayload {
                    op: OP_HEARTBEAT,
                    d: sequence.map(|s| json!(s)),
                    s: None,
                    t: None,
                };
                ws_sink.send(Message::Text(serde_json::to_string(&hb)?.into())).await?;
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
                            if ch.starts_with("discord:") =>
                        {
                            let key = ch.to_string();
                            reply_buffers.insert(key, String::new());
                        }
                        ProtocolEvent::AgentChunk { ref chunk, channel: Some(ref ch) }
                            if ch.starts_with("discord:") =>
                        {
                            reply_buffers.entry(ch.clone()).or_default().push_str(chunk);
                        }
                        ProtocolEvent::AgentDone { channel: Some(ref ch) }
                            if ch.starts_with("discord:") =>
                        {
                            // channel format: "discord:<channel_id>:<message_id>"
                            let parts: Vec<&str> = ch.splitn(3, ':').collect();
                            let discord_channel_id = parts.get(1).unwrap_or(&"");
                            let key = ch.to_string();
                            if let Some(content) = reply_buffers.remove(&key) {
                                if !content.is_empty() {
                                    send_discord_message(&token, discord_channel_id, &content).await?;
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

/// Send a message to a Discord channel via REST API.
async fn send_discord_message(
    token: &str,
    channel_id: &str,
    content: &str,
) -> Result<(), Box<dyn Error>> {
    // Discord messages have a 2000-char limit; truncate if needed
    let truncated = if content.len() > 1900 {
        format!("{}…", &content[..1900])
    } else {
        content.to_string()
    };

    let client = reqwest::Client::new();
    let url = format!("{}/channels/{}/messages", DISCORD_API_BASE, channel_id);
    client
        .post(&url)
        .header("Authorization", format!("Bot {}", token))
        .header("Content-Type", "application/json")
        .json(&json!({ "content": truncated }))
        .send()
        .await?;
    Ok(())
}

/// Transform a Discord message event into a ProtocolEvent::Prompt for the bridge.
///
/// Channel format: `discord:<channel_id>:<message_id>`
/// This encodes both the channel (needed for replies) and the message id (for deduplication).
pub fn transform_discord_message(
    content: &str,
    channel_id: &str,
    message_id: &str,
) -> ProtocolEvent {
    ProtocolEvent::Prompt {
        text: content.to_string(),
        tool: None,
        channel: Some(format!("discord:{}:{}", channel_id, message_id)),
    }
}

/// Format a bot reply with a prefix tag (for readability in Discord).
pub fn format_discord_reply(content: &str) -> String {
    format!("**[YuiClaw]** {}", content)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transform_discord_message() {
        let event = transform_discord_message("Hello 執事！", "987654321", "111222333");
        if let ProtocolEvent::Prompt { text, channel, tool } = event {
            assert_eq!(text, "Hello 執事！");
            assert_eq!(channel, Some("discord:987654321:111222333".to_string()));
            assert!(tool.is_none());
        } else {
            panic!("Transform failed to produce a Prompt event");
        }
    }

    #[test]
    fn test_transform_discord_message_channel_prefix() {
        let event = transform_discord_message("test", "ch123", "msg456");
        if let ProtocolEvent::Prompt { channel, .. } = event {
            let ch = channel.unwrap();
            assert!(ch.starts_with("discord:"), "Channel must start with 'discord:'");
            let parts: Vec<&str> = ch.splitn(3, ':').collect();
            assert_eq!(parts.len(), 3, "Channel must have 3 parts: discord:channel_id:message_id");
            assert_eq!(parts[1], "ch123");
            assert_eq!(parts[2], "msg456");
        } else {
            panic!("Not a Prompt event");
        }
    }

    #[test]
    fn test_transform_discord_message_empty_content() {
        let event = transform_discord_message("", "ch1", "msg1");
        if let ProtocolEvent::Prompt { text, .. } = event {
            assert_eq!(text, "");
        } else {
            panic!("Not a Prompt event");
        }
    }

    #[test]
    fn test_format_discord_reply() {
        let reply = format_discord_reply("こんにちは！");
        assert!(reply.contains("こんにちは！"));
        assert!(reply.starts_with("**[YuiClaw]**"));
    }

    #[test]
    fn test_format_discord_reply_non_empty() {
        let reply = format_discord_reply("test message");
        assert!(!reply.is_empty());
        assert!(reply.contains("test message"));
    }
}
