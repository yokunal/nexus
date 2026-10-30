use crate::tools::{ToolCall, ToolResult};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NexusEvent {
    /// User input delivered to the agent
    UserPrompt(String),
    /// Streaming token chunk received from LLM
    TokenChunk(String),
    /// Agent decided to invoke a tool
    ToolCallStarted(ToolCall),
    /// Tool execution completed
    ToolCallFinished(ToolResult),
    /// Current conversation turn is fully completed
    TurnComplete,
    /// An error occurred during the turn
    Error(String),
}
