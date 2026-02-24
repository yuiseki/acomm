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
 * Optional environment variables:
 *   DISCORD_ALLOWED_USER_IDS — comma-separated Discord user IDs to allow.
 *   If set, messages from other users are ignored.
 *
 * Required bot intents (Gateway subscribe):
 *   GUILD_MESSAGES (1 << 9) = 512
 *   DIRECT_MESSAGES (1 << 12) = 4096
 *
 * Optional (for reading guild message content reliably):
 *   MESSAGE_CONTENT (1 << 15) = 32768
 */

use crate::protocol::ProtocolEvent;
use std::collections::{HashMap, HashSet};
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
const DISCORD_SAFE_MESSAGE_LIMIT: usize = 1900;
const DEFAULT_DISCORD_PROVIDER_NAME: &str = "gemini";
const DEFAULT_DISCORD_MODEL_NAME: &str = "auto-gemini-3";

/// Gateway opcodes
const OP_DISPATCH: u64 = 0;
const OP_HEARTBEAT: u64 = 1;
const OP_IDENTIFY: u64 = 2;
const OP_PRESENCE_UPDATE: u64 = 3;
const OP_HELLO: u64 = 10;
const OP_HEARTBEAT_ACK: u64 = 11;

const DISCORD_PRESENCE_ONLINE: &str = "online";
const DISCORD_PRESENCE_DND: &str = "dnd";
const DISCORD_PRESENCE_INVISIBLE: &str = "invisible";

/// Gateway intents: GUILD_MESSAGES | DIRECT_MESSAGES
///
/// MESSAGE_CONTENT is intentionally omitted here so bots can connect without
/// enabling the privileged intent. DM text content is still available.
const GATEWAY_INTENTS: u64 = (1 << 9) | (1 << 12);

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

#[derive(Debug, Clone)]
struct DiscordReplyBuffer {
    content: String,
    provider: String,
    model: String,
}

fn build_identify_payload(token: &str) -> GatewayPayload {
    GatewayPayload {
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
    }
}

fn build_heartbeat_payload(sequence: Option<u64>) -> GatewayPayload {
    GatewayPayload {
        op: OP_HEARTBEAT,
        d: Some(sequence.map_or(Value::Null, |s| json!(s))),
        s: None,
        t: None,
    }
}

fn build_presence_update_payload(status: &str) -> GatewayPayload {
    let status = match status {
        DISCORD_PRESENCE_ONLINE | "idle" | DISCORD_PRESENCE_DND | DISCORD_PRESENCE_INVISIBLE => {
            status
        }
        _ => DISCORD_PRESENCE_ONLINE,
    };
    GatewayPayload {
        op: OP_PRESENCE_UPDATE,
        d: Some(json!({
            "since": Value::Null,
            "activities": [],
            "status": status,
            "afk": false,
        })),
        s: None,
        t: None,
    }
}

fn parse_allowed_discord_user_ids(raw: &str) -> HashSet<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

fn load_allowed_discord_user_ids_from_env() -> Option<HashSet<String>> {
    let raw = std::env::var("DISCORD_ALLOWED_USER_IDS").ok()?;
    let ids = parse_allowed_discord_user_ids(&raw);
    if ids.is_empty() { None } else { Some(ids) }
}

fn should_forward_discord_message(
    msg: &DiscordMessage,
    bot_user_id: Option<&str>,
    allowed_user_ids: Option<&HashSet<String>>,
) -> bool {
    if let Some(bot_id) = bot_user_id {
        if msg.author.id == bot_id {
            return false;
        }
    }
    if msg.author.bot.unwrap_or(false) {
        return false;
    }
    if msg.content.trim().is_empty() {
        return false;
    }
    if let Some(ids) = allowed_user_ids {
        if !ids.contains(&msg.author.id) {
            return false;
        }
    }
    true
}

fn default_model_for_provider_name(provider_name: &str) -> Option<&'static str> {
    match provider_name {
        "gemini" => Some(DEFAULT_DISCORD_MODEL_NAME),
        "claude" => Some("claude-sonnet-4-6"),
        "codex" => Some("gpt-5.3-codex"),
        "dummy" => Some("echo"),
        "mock" => Some("mock-model"),
        _ => None,
    }
}

fn discord_channel_id_from_bridge_channel(channel: &str) -> Option<&str> {
    let mut parts = channel.splitn(3, ':');
    match (parts.next(), parts.next()) {
        (Some("discord"), Some(channel_id)) if !channel_id.is_empty() => Some(channel_id),
        _ => None,
    }
}

fn truncate_for_discord(content: &str) -> String {
    let trimmed = content.trim_end();
    if trimmed.chars().count() <= DISCORD_SAFE_MESSAGE_LIMIT {
        return trimmed.to_string();
    }

    let mut out = String::new();
    for (idx, ch) in trimmed.chars().enumerate() {
        if idx >= DISCORD_SAFE_MESSAGE_LIMIT.saturating_sub(1) {
            break;
        }
        out.push(ch);
    }
    out.push('…');
    out
}

fn format_discord_agent_reply_with_status(content: &str, provider: &str, model: &str) -> String {
    let provider = provider.trim();
    let provider = if provider.is_empty() {
        DEFAULT_DISCORD_PROVIDER_NAME
    } else {
        provider
    };
    let model = model.trim();
    let model = if model.is_empty() {
        default_model_for_provider_name(provider).unwrap_or("unknown")
    } else {
        model
    };

    let suffix = format!("__{}:{}__", provider, model);
    let body = content.trim_end();
    if body.is_empty() {
        return truncate_for_discord(&suffix);
    }

    let separator = "\n\n";
    let reserved = suffix.chars().count() + separator.chars().count();
    if reserved >= DISCORD_SAFE_MESSAGE_LIMIT {
        return truncate_for_discord(&suffix);
    }

    let body_budget = DISCORD_SAFE_MESSAGE_LIMIT - reserved;
    let body_chars = body.chars().count();
    let body_part = if body_chars <= body_budget {
        body.to_string()
    } else if body_budget <= 1 {
        "…".to_string()
    } else {
        let mut truncated = String::new();
        for (idx, ch) in body.chars().enumerate() {
            if idx >= body_budget - 1 {
                break;
            }
            truncated.push(ch);
        }
        truncated.push('…');
        truncated
    };

    format!("{body_part}{separator}{suffix}")
}

/// Send a proactive agent notification to a Discord channel.
///
/// Required environment variables:
///   DISCORD_BOT_TOKEN         — bot token
///   DISCORD_NOTIFY_CHANNEL_ID — target channel ID for agent-initiated messages
pub async fn notify_discord(text: &str) -> Result<(), Box<dyn Error>> {
    let token = std::env::var("DISCORD_BOT_TOKEN")
        .map_err(|_| "DISCORD_BOT_TOKEN environment variable not set")?;
    let channel_id = std::env::var("DISCORD_NOTIFY_CHANNEL_ID")
        .map_err(|_| "DISCORD_NOTIFY_CHANNEL_ID environment variable not set")?;
    send_discord_message(&token, &channel_id, text).await
}

pub async fn start_discord_adapter() -> Result<(), Box<dyn Error>> {
    let token = std::env::var("DISCORD_BOT_TOKEN")
        .map_err(|_| "DISCORD_BOT_TOKEN environment variable not set")?;
    let allowed_user_ids = load_allowed_discord_user_ids_from_env();

    println!("Discord adapter starting...");
    if let Some(ids) = &allowed_user_ids {
        println!("Discord author allowlist enabled: {} user id(s)", ids.len());
    }

    let bridge_stream = UnixStream::connect(SOCKET_PATH).await.map_err(|e| {
        format!(
            "Bridge is not running. Please start it with 'acomm --bridge'. Error: {}",
            e
        )
    })?;
    println!("Connected to acomm bridge.");
    let (bridge_reader, mut bridge_writer) = tokio::io::split(bridge_stream);
    let mut bridge_lines = BufReader::new(bridge_reader).lines();

    println!("Connecting to Discord Gateway: {}...", DISCORD_GATEWAY_URL);
    let (ws_stream, _) = connect_async(DISCORD_GATEWAY_URL).await?;
    let (mut ws_sink, mut ws_stream) = ws_stream.split();

    println!("Connected to Discord Gateway.");

    let mut heartbeat_interval_ms: u64 = 41250; // default fallback
    let mut sequence: Option<u64> = None;
    let mut bot_user_id: Option<String> = None;
    let mut active_provider_name = DEFAULT_DISCORD_PROVIDER_NAME.to_string();
    let mut active_model_name = DEFAULT_DISCORD_MODEL_NAME.to_string();
    let mut reply_buffers: HashMap<String, DiscordReplyBuffer> = HashMap::new();
    let mut typing_tasks: HashMap<String, tokio::task::JoinHandle<()>> = HashMap::new();
    let mut bridge_sync_done = false;
    let mut discord_gateway_ready = false;
    let mut discord_presence_status = DISCORD_PRESENCE_ONLINE.to_string();

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
                    Message::Close(frame) => {
                        if let Some(frame) = frame {
                            return Err(format!(
                                "Discord Gateway closed connection: code={} reason={}",
                                frame.code, frame.reason
                            ).into());
                        }
                        return Err("Discord Gateway closed connection".into());
                    }
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
                        let identify = build_identify_payload(&token);
                        ws_sink.send(Message::Text(serde_json::to_string(&identify)?.into())).await?;
                        println!("Sent IDENTIFY to Discord Gateway.");
                    }
                    OP_HEARTBEAT_ACK => {
                        // Heartbeat acknowledged — connection is healthy.
                    }
                    OP_HEARTBEAT => {
                        // Server-requested heartbeat
                        let hb = build_heartbeat_payload(sequence);
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
                                let presence = build_presence_update_payload(DISCORD_PRESENCE_ONLINE);
                                ws_sink
                                    .send(Message::Text(serde_json::to_string(&presence)?.into()))
                                    .await?;
                                discord_gateway_ready = true;
                                discord_presence_status = DISCORD_PRESENCE_ONLINE.to_string();
                                println!("Discord presence set to {}.", DISCORD_PRESENCE_ONLINE);
                            }
                            Some("MESSAGE_CREATE") => {
                                if let Some(d) = &payload.d {
                                    if let Ok(msg) = serde_json::from_value::<DiscordMessage>(d.clone()) {
                                        let is_allowed_sender = allowed_user_ids
                                            .as_ref()
                                            .map(|ids| ids.contains(&msg.author.id))
                                            .unwrap_or(true);
                                        if !should_forward_discord_message(
                                            &msg,
                                            bot_user_id.as_deref(),
                                            allowed_user_ids.as_ref(),
                                        ) {
                                            if !is_allowed_sender && !msg.author.bot.unwrap_or(false) {
                                                println!(
                                                    "Ignoring Discord message from non-allowed user: {} ({})",
                                                    msg.author.username, msg.author.id
                                                );
                                            }
                                            continue;
                                        }

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
                let hb = build_heartbeat_payload(sequence);
                ws_sink.send(Message::Text(serde_json::to_string(&hb)?.into())).await?;
            }

            // Bridge protocol events
            line_res = bridge_lines.next_line() => {
                let line = match line_res? {
                    Some(l) => l,
                    None => {
                        if discord_gateway_ready {
                            let presence = build_presence_update_payload(DISCORD_PRESENCE_INVISIBLE);
                            let _ = ws_sink
                                .send(Message::Text(
                                    serde_json::to_string(&presence)?.into(),
                                ))
                                .await;
                            println!(
                                "Discord presence set to {} before adapter shutdown.",
                                DISCORD_PRESENCE_INVISIBLE
                            );
                        }
                        break;
                    }
                };
                if let Ok(event) = serde_json::from_str::<ProtocolEvent>(&line) {
                    if let ProtocolEvent::ProviderSwitched { ref provider } = event {
                        active_provider_name = provider.command_name().to_string();
                        if let Some(model) = default_model_for_provider_name(&active_provider_name) {
                            active_model_name = model.to_string();
                        }
                    }
                    if let ProtocolEvent::ModelSwitched { ref model } = event {
                        active_model_name = model.clone();
                    }
                    if !bridge_sync_done {
                        if matches!(event, ProtocolEvent::BridgeSyncDone { .. }) {
                            bridge_sync_done = true;
                            println!("Bridge initial sync complete (backlog ignored for Discord outbound replay safety).");
                        }
                        continue;
                    }
                    match event {
                        ProtocolEvent::Prompt { provider, channel: Some(ref ch), .. }
                            if ch.starts_with("discord:") =>
                        {
                            let should_switch_presence_to_dnd = reply_buffers.is_empty()
                                && discord_gateway_ready
                                && discord_presence_status != DISCORD_PRESENCE_DND;
                            let key = ch.to_string();
                            let provider_name = provider
                                .as_ref()
                                .map(|p| p.command_name().to_string())
                                .unwrap_or_else(|| active_provider_name.clone());
                            let model_name = if active_model_name.trim().is_empty() {
                                default_model_for_provider_name(&provider_name)
                                    .unwrap_or("unknown")
                                    .to_string()
                            } else {
                                active_model_name.clone()
                            };
                            reply_buffers.insert(
                                key.clone(),
                                DiscordReplyBuffer {
                                    content: String::new(),
                                    provider: provider_name,
                                    model: model_name,
                                },
                            );
                            // Start typing indicator while agent processes.
                            if let Some(discord_channel_id) = discord_channel_id_from_bridge_channel(ch).map(str::to_string) {
                                let token_clone = token.clone();
                                let handle = tokio::spawn(async move {
                                    loop {
                                        let _ = trigger_discord_typing(&token_clone, &discord_channel_id).await;
                                        tokio::time::sleep(std::time::Duration::from_secs(8)).await;
                                    }
                                });
                                if let Some(old) = typing_tasks.insert(key, handle) {
                                    old.abort();
                                }
                            }
                            if should_switch_presence_to_dnd {
                                let presence = build_presence_update_payload(DISCORD_PRESENCE_DND);
                                ws_sink
                                    .send(Message::Text(serde_json::to_string(&presence)?.into()))
                                    .await?;
                                discord_presence_status = DISCORD_PRESENCE_DND.to_string();
                                println!("Discord presence set to {}.", DISCORD_PRESENCE_DND);
                            }
                        }
                        ProtocolEvent::AgentChunk { ref chunk, channel: Some(ref ch) }
                            if ch.starts_with("discord:") =>
                        {
                            if let Some(buf) = reply_buffers.get_mut(ch) {
                                buf.content.push_str(chunk);
                            }
                        }
                        ProtocolEvent::AgentDone { channel: Some(ref ch) }
                            if ch.starts_with("discord:") =>
                        {
                            // Stop typing indicator.
                            if let Some(handle) = typing_tasks.remove(ch.as_str()) {
                                handle.abort();
                            }
                            let key = ch.to_string();
                            if let Some(buf) = reply_buffers.remove(&key) {
                                if !buf.content.is_empty() {
                                    let answer = extract_discord_answer(&buf.content);
                                    let formatted = format_discord_agent_reply_with_status(
                                        &answer,
                                        &buf.provider,
                                        &buf.model,
                                    );
                                    if let Some(discord_channel_id) = discord_channel_id_from_bridge_channel(ch) {
                                        send_discord_message(&token, discord_channel_id, &formatted).await?;
                                    }
                                }
                            }
                            if discord_gateway_ready
                                && reply_buffers.is_empty()
                                && discord_presence_status != DISCORD_PRESENCE_ONLINE
                            {
                                let presence = build_presence_update_payload(DISCORD_PRESENCE_ONLINE);
                                ws_sink
                                    .send(Message::Text(serde_json::to_string(&presence)?.into()))
                                    .await?;
                                discord_presence_status = DISCORD_PRESENCE_ONLINE.to_string();
                                println!("Discord presence set to {}.", DISCORD_PRESENCE_ONLINE);
                            }
                        }
                        ProtocolEvent::SystemMessage { msg, channel: Some(ref ch) }
                            if ch.starts_with("discord:") =>
                        {
                            if let Some(discord_channel_id) = discord_channel_id_from_bridge_channel(ch) {
                                let formatted = format_discord_agent_reply_with_status(
                                    &msg,
                                    &active_provider_name,
                                    &active_model_name,
                                );
                                send_discord_message(&token, discord_channel_id, &formatted).await?;
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
    // Keep a safety margin below Discord's 2000-char limit and truncate by chars.
    let truncated = truncate_for_discord(content);

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

/// POST /channels/{channel_id}/typing to show the typing indicator in Discord.
/// The indicator lasts ~10 seconds; this should be called every ~8 seconds while
/// the agent is processing.
async fn trigger_discord_typing(token: &str, channel_id: &str) -> Result<(), Box<dyn Error>> {
    let client = reqwest::Client::new();
    let url = format!("{}/channels/{}/typing", DISCORD_API_BASE, channel_id);
    client
        .post(&url)
        .header("Authorization", format!("Bot {}", token))
        .send()
        .await?;
    Ok(())
}

/// Extract the final answer from an agent's full output for Discord delivery.
///
/// Agent outputs include intermediate tool-call narration followed by the final
/// answer. This function walks backwards through double-newline separators to find
/// the last substantive paragraph (≥ 30 Unicode chars) that fits within Discord's
/// 1900-char limit. Uses character counts (not byte lengths) so multi-byte Unicode
/// is handled correctly. If no usable separator is found, the last 1899 chars are
/// returned with a leading ellipsis.
pub fn extract_discord_answer(content: &str) -> String {
    const DISCORD_LIMIT: usize = 1900;
    let trimmed = content.trim_end();

    if trimmed.chars().count() <= DISCORD_LIMIT {
        return trimmed.to_string();
    }

    // Walk backwards through double-newline separators to find the last
    // substantive block (≥ 30 chars) that fits within the Discord limit.
    let mut search = trimmed;
    while let Some(pos) = search.rfind("\n\n") {
        let candidate = search[pos + 2..].trim();
        let char_count = candidate.chars().count();
        if char_count >= 30 {
            if char_count <= DISCORD_LIMIT {
                return candidate.to_string();
            }
            // Candidate itself too long — take the last (DISCORD_LIMIT - 1) chars.
            let chars: Vec<char> = candidate.chars().collect();
            let start = chars.len().saturating_sub(DISCORD_LIMIT - 1);
            let truncated: String = chars[start..].iter().collect();
            return format!("…{}", truncated);
        }
        // Candidate too short — look for an earlier separator.
        search = &search[..pos];
    }

    // No usable separator found — take the last (DISCORD_LIMIT - 1) chars.
    let chars: Vec<char> = trimmed.chars().collect();
    let start = chars.len().saturating_sub(DISCORD_LIMIT - 1);
    let truncated: String = chars[start..].iter().collect();
    format!("…{}", truncated)
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
        provider: None,
        channel: Some(format!("discord:{}:{}", channel_id, message_id)),
    }
}

#[cfg_attr(not(test), allow(dead_code))]
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
        if let ProtocolEvent::Prompt { text, channel, provider } = event {
            assert_eq!(text, "Hello 執事！");
            assert_eq!(channel, Some("discord:987654321:111222333".to_string()));
            assert!(provider.is_none());
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

    #[test]
    fn test_format_discord_agent_reply_with_status_appends_suffix() {
        let reply = format_discord_agent_reply_with_status(
            "pong",
            "gemini",
            "auto-gemini-3",
        );
        assert!(reply.starts_with("pong"));
        assert!(reply.ends_with("__gemini:auto-gemini-3__"));
        assert!(reply.contains("\n\n__gemini:auto-gemini-3__"));
        assert!(reply.chars().count() <= 1900);
    }

    #[test]
    fn test_format_discord_agent_reply_with_status_preserves_suffix_when_truncated() {
        let body = "あ".repeat(2500);
        let reply = format_discord_agent_reply_with_status(
            &body,
            "claude",
            "claude-sonnet-4-6",
        );
        assert!(reply.ends_with("__claude:claude-sonnet-4-6__"));
        assert!(reply.chars().count() <= 1900);
    }

    #[test]
    fn test_gateway_intents_include_direct_messages_for_dm_support() {
        const DIRECT_MESSAGES_INTENT: u64 = 1 << 12;
        assert_ne!(
            GATEWAY_INTENTS & DIRECT_MESSAGES_INTENT,
            0,
            "Discord DM MESSAGE_CREATE requires DIRECT_MESSAGES intent",
        );
    }

    #[test]
    fn test_identify_payload_uses_discord_properties_keys() {
        let payload = build_identify_payload("dummy-token");
        let d = payload.d.expect("identify payload must include d");
        let props = d.get("properties").expect("identify payload must include properties");
        assert!(props.get("os").is_some(), "Discord IDENTIFY requires os");
        assert!(props.get("browser").is_some(), "Discord IDENTIFY requires browser");
        assert!(props.get("device").is_some(), "Discord IDENTIFY requires device");
        assert!(props.get("$os").is_none(), "$os key should not be sent");
        assert!(props.get("$browser").is_none(), "$browser key should not be sent");
        assert!(props.get("$device").is_none(), "$device key should not be sent");
    }

    #[test]
    fn test_heartbeat_payload_includes_null_d_when_sequence_absent() {
        let payload = build_heartbeat_payload(None);
        let json = serde_json::to_string(&payload).expect("heartbeat payload must serialize");
        assert!(json.contains(r#""op":1"#));
        assert!(json.contains(r#""d":null"#), "Discord heartbeat must include d:null before first sequence");
    }

    #[test]
    fn test_presence_update_payload_uses_discord_gateway_schema() {
        let payload = build_presence_update_payload("dnd");
        assert_eq!(payload.op, OP_PRESENCE_UPDATE);
        let d = payload.d.expect("presence update payload must include d");
        assert_eq!(d.get("status").and_then(Value::as_str), Some("dnd"));
        assert_eq!(d.get("afk").and_then(Value::as_bool), Some(false));
        assert_eq!(d.get("since"), Some(&Value::Null));
        assert_eq!(
            d.get("activities")
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(0),
            "presence activities should default to empty list"
        );
    }

    fn sample_message(author_id: &str) -> DiscordMessage {
        DiscordMessage {
            id: "msg1".to_string(),
            channel_id: "ch1".to_string(),
            content: "hello".to_string(),
            author: DiscordUser {
                id: author_id.to_string(),
                username: "user".to_string(),
                bot: Some(false),
            },
        }
    }

    // env var を書き換えるテストは並列実行すると競合するため 1 関数にまとめて順序実行する。
    #[tokio::test]
    async fn test_notify_discord_env_var_validation() {
        let token_backup = std::env::var("DISCORD_BOT_TOKEN").ok();
        let channel_backup = std::env::var("DISCORD_NOTIFY_CHANNEL_ID").ok();

        // Case 1: DISCORD_BOT_TOKEN が未設定
        unsafe {
            std::env::remove_var("DISCORD_BOT_TOKEN");
            std::env::remove_var("DISCORD_NOTIFY_CHANNEL_ID");
        }
        let result = notify_discord("test").await;
        assert!(result.is_err(), "should fail when DISCORD_BOT_TOKEN is missing");
        assert!(
            format!("{}", result.unwrap_err()).contains("DISCORD_BOT_TOKEN"),
            "error should mention DISCORD_BOT_TOKEN"
        );

        // Case 2: DISCORD_BOT_TOKEN は設定済み、DISCORD_NOTIFY_CHANNEL_ID が未設定
        unsafe {
            std::env::set_var("DISCORD_BOT_TOKEN", "dummy-token");
            std::env::remove_var("DISCORD_NOTIFY_CHANNEL_ID");
        }
        let result = notify_discord("test").await;
        assert!(result.is_err(), "should fail when DISCORD_NOTIFY_CHANNEL_ID is missing");
        assert!(
            format!("{}", result.unwrap_err()).contains("DISCORD_NOTIFY_CHANNEL_ID"),
            "error should mention DISCORD_NOTIFY_CHANNEL_ID"
        );

        // 復元
        unsafe {
            match token_backup {
                Some(v) => std::env::set_var("DISCORD_BOT_TOKEN", v),
                None => std::env::remove_var("DISCORD_BOT_TOKEN"),
            }
            if let Some(v) = channel_backup { std::env::set_var("DISCORD_NOTIFY_CHANNEL_ID", v); }
        }
    }

    // ─── extract_discord_answer tests ──────────────────────────────────────────

    #[test]
    fn test_extract_discord_answer_short_content_unchanged() {
        let short = "Hello, 天気は晴れです。";
        assert_eq!(extract_discord_answer(short), short);
    }

    #[test]
    fn test_extract_discord_answer_exactly_at_limit_unchanged() {
        let content = "a".repeat(1900);
        assert_eq!(extract_discord_answer(&content), content);
    }

    #[test]
    fn test_extract_discord_answer_extracts_last_paragraph() {
        // Simulate agent output: tool narration + final answer separated by \n\n.
        // Padding must be large enough so total chars exceed 1900.
        let thinking = "I will search for the information.\n\nLooking at the files...\n\n";
        let answer = "本日の天気カレンダーを日本語に修正いたしました。修正内容は以下の通りです。";
        let padding = "x".repeat(2000); // ensures total > 1900
        let full = format!("{}{}\n\n{}", padding, thinking.trim(), answer);
        assert!(full.chars().count() > 1900, "Precondition: full content must exceed 1900 chars");
        let result = extract_discord_answer(&full);
        assert_eq!(result, answer, "Should extract the last paragraph as the final answer");
    }

    #[test]
    fn test_extract_discord_answer_skips_short_trailing_block() {
        // If the last paragraph is too short (< 30 chars), look earlier.
        let early_answer = "本日の天気カレンダーを日本語に修正いたしました。詳細は以下の通りです。正常に完了いたしました。";
        let padding = "x".repeat(2000); // ensures total > 1900
        // Last block is only "OK" (too short), penultimate block is the real answer.
        let full = format!("{}\n\n{}\n\nOK", padding, early_answer);
        assert!(full.chars().count() > 1900, "Precondition: full content must exceed 1900 chars");
        let result = extract_discord_answer(&full);
        assert_eq!(result, early_answer, "Should skip short trailing block and use earlier paragraph");
    }

    #[test]
    fn test_extract_discord_answer_truncates_when_no_separator() {
        // No double-newline — falls back to last 1899 chars with ellipsis prefix.
        // Discord limits are character-based, so we check chars().count().
        let content = "a".repeat(2000);
        let result = extract_discord_answer(&content);
        assert!(result.starts_with('…'), "Should start with ellipsis when truncated");
        assert!(result.chars().count() <= 1900, "Result must fit within Discord 1900-char limit");
    }

    #[test]
    fn test_extract_discord_answer_trims_trailing_whitespace() {
        let content = format!("short answer\n\n\n   ");
        assert_eq!(extract_discord_answer(&content), "short answer");
    }

    // ─── parse_allowed_discord_user_ids tests ──────────────────────────────────

    #[test]
    fn test_parse_allowed_discord_user_ids_trims_and_dedups() {
        let ids = parse_allowed_discord_user_ids(" 123 , , 456,123 ");
        assert_eq!(ids.len(), 2);
        assert!(ids.contains("123"));
        assert!(ids.contains("456"));
    }

    #[test]
    fn test_should_forward_discord_message_rejects_unlisted_user_when_allowlist_enabled() {
        let msg = sample_message("user-2");
        let allowed = parse_allowed_discord_user_ids("user-1");
        assert!(
            !should_forward_discord_message(&msg, Some("bot-1"), Some(&allowed)),
            "messages from users outside allowlist should be ignored",
        );
    }

    #[test]
    fn test_should_forward_discord_message_accepts_listed_user_when_allowlist_enabled() {
        let msg = sample_message("user-1");
        let allowed = parse_allowed_discord_user_ids("user-1,user-2");
        assert!(
            should_forward_discord_message(&msg, Some("bot-1"), Some(&allowed)),
            "messages from allowed users should be forwarded",
        );
    }
}
