use acore::AgentProvider;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum ProtocolEvent {
    Prompt { 
        text: String, 
        provider: Option<AgentProvider>,
        channel: Option<String>,
    },
    /// エージェントからの回答の断片（チャンク）。
    AgentChunk { 
        chunk: String,
        channel: Option<String>,
    },
    AgentDone {
        channel: Option<String>,
    },
    SystemMessage { 
        msg: String,
        channel: Option<String>,
    },
    StatusUpdate { 
        is_processing: bool,
        channel: Option<String>,
    },
    SyncContext { context: String },
    ProviderSwitched { provider: AgentProvider },
    ModelSwitched { model: String },
}

impl ProtocolEvent {
    pub fn clone_channel(&self) -> Option<String> {
        match self {
            ProtocolEvent::Prompt { channel, .. } => channel.clone(),
            ProtocolEvent::AgentChunk { channel, .. } => channel.clone(),
            ProtocolEvent::AgentDone { channel, .. } => channel.clone(),
            ProtocolEvent::SystemMessage { channel, .. } => channel.clone(),
            ProtocolEvent::StatusUpdate { channel, .. } => channel.clone(),
            ProtocolEvent::SyncContext { .. }
            | ProtocolEvent::ProviderSwitched { .. }
            | ProtocolEvent::ModelSwitched { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ProtocolEvent;
    use acore::AgentProvider;

    #[test]
    fn prompt_deserializes_provider_field() {
        let json = r#"{"Prompt":{"text":"hello","provider":"Gemini","channel":"tui"}}"#;
        let event: ProtocolEvent = serde_json::from_str(json).unwrap();
        match event {
            ProtocolEvent::Prompt { provider, .. } => {
                assert_eq!(provider, Some(AgentProvider::Gemini));
            }
            _ => panic!("expected Prompt"),
        }
    }

    #[test]
    fn provider_switched_serializes_provider_field() {
        let event = ProtocolEvent::ProviderSwitched { provider: AgentProvider::Claude };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""provider":"Claude""#));
        assert!(!json.contains(r#""tool":"Claude""#));
    }

    #[test]
    fn provider_switched_deserializes_provider_field() {
        let json = r#"{"ProviderSwitched":{"provider":"Codex"}}"#;
        let event: ProtocolEvent = serde_json::from_str(json).unwrap();
        match event {
            ProtocolEvent::ProviderSwitched { provider } => {
                assert_eq!(provider, AgentProvider::Codex);
            }
            _ => panic!("expected ProviderSwitched"),
        }
    }
}
