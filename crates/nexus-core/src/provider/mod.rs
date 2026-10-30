pub mod anthropic;

use async_trait::async_trait;
use crate::events::NexusEvent;
use crate::session::Message;
use thiserror::Error;
use tokio::sync::mpsc::Sender;

#[derive(Error, Debug)]
pub enum ProviderError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("API error ({status}): {message}")]
    Api { status: u16, message: String },
    #[error("Stream error: {0}")]
    Stream(String),
    #[error("Serialization error: {0}")]
    Serde(#[from] serde_json::Error),
}

#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Stream a response from the LLM provider, emitting NexusEvents to the event_tx channel.
    async fn stream_turn(
        &self,
        system_prompt: &str,
        history: &[Message],
        tools: &[serde_json::Value],
        event_tx: Sender<NexusEvent>,
    ) -> Result<Message, ProviderError>;
}
