use crate::tools::{ToolCall, ToolResult};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub cache_read_tokens: u32,
    pub cache_creation_tokens: u32,
}

impl TokenUsage {
    pub fn cache_hit_rate(&self) -> f64 {
        let total = self.input_tokens + self.cache_read_tokens;
        if total == 0 {
            0.0
        } else {
            (self.cache_read_tokens as f64 / total as f64) * 100.0
        }
    }
}

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
    /// Token usage metrics for the turn
    TokenUsageRecorded(TokenUsage),
    /// Current conversation turn is fully completed
    TurnComplete,
    /// An error occurred during the turn
    Error(String),
}
