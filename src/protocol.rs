use acore::AgentTool;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum ProtocolEvent {
    Prompt { 
        text: String, 
        tool: Option<AgentTool>,
        channel: Option<String>,
    },
    AgentLine { line: String },
    AgentDone,
    SystemMessage { 
        msg: String,
        channel: Option<String>,
    },
    StatusUpdate { is_processing: bool },
    SyncContext { context: String },
    ToolSwitched { tool: AgentTool },
}

impl ProtocolEvent {
    pub fn clone_channel(&self) -> Option<String> {
        match self {
            ProtocolEvent::Prompt { channel, .. } => channel.clone(),
            ProtocolEvent::SystemMessage { channel, .. } => channel.clone(),
            _ => None,
        }
    }
}
