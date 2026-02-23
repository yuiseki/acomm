use acore::AgentTool;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum ProtocolEvent {
    Prompt { 
        text: String, 
        tool: Option<AgentTool>,
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
    ToolSwitched { tool: AgentTool },
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
            | ProtocolEvent::ToolSwitched { .. }
            | ProtocolEvent::ModelSwitched { .. } => None,
        }
    }
}
