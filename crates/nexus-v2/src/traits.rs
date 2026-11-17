use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::Path;
use crate::errors::NexusV2Error;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
}

impl Message {
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".to_string(),
            content: content.into(),
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: "assistant".to_string(),
            content: content.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionResponse {
    pub text: String,
    pub token_usage: TokenUsage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResult {
    pub task_id: String,
    pub success: bool,
    pub summary: String,
    pub iterations: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WalRecord {
    pub sequence: u64,
    pub timestamp: u64,
    pub event_type: u8,
    pub payload: Vec<u8>,
}

#[async_trait]
pub trait ModelClient: Send + Sync {
    async fn complete(&self, prompt: &str, context: &[Message]) -> Result<CompletionResponse, NexusV2Error>;
    async fn health_check(&self) -> Result<bool, NexusV2Error>;
}

#[async_trait]
pub trait OrchestratorEngine: Send + Sync {
    async fn execute_task(&mut self, goal: &str) -> Result<TaskResult, NexusV2Error>;
}

#[async_trait]
pub trait SafetyFilter: Send + Sync {
    fn validate_patch(&self, target_path: &Path, original: &str, patch: &str) -> Result<(), NexusV2Error>;
    fn validate_command(&self, cmd: &str) -> Result<(), NexusV2Error>;
}

#[async_trait]
pub trait CircularWalStore: Send + Sync {
    async fn append_frame(&mut self, event_type: u8, payload: &[u8]) -> Result<u64, NexusV2Error>;
    async fn recover(&mut self) -> Result<Vec<WalRecord>, NexusV2Error>;
    async fn compact(&mut self) -> Result<(), NexusV2Error>;
}
