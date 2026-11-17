use async_trait::async_trait;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use crate::errors::NexusV2Error;
use crate::traits::{CompletionResponse, Message, ModelClient, TokenUsage};

pub struct MockModelClient {
    name: String,
    should_fail: Arc<AtomicBool>,
    response_text: String,
}

impl MockModelClient {
    pub fn new(name: impl Into<String>, response_text: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            should_fail: Arc::new(AtomicBool::new(false)),
            response_text: response_text.into(),
        }
    }

    pub fn set_failing(&self, fail: bool) {
        self.should_fail.store(fail, Ordering::SeqCst);
    }
}

#[async_trait]
impl ModelClient for MockModelClient {
    async fn complete(&self, _prompt: &str, _context: &[Message]) -> Result<CompletionResponse, NexusV2Error> {
        if self.should_fail.load(Ordering::SeqCst) {
            return Err(NexusV2Error::SafetyViolation(format!(
                "Model '{}' simulated rate-limit or upstream failure",
                self.name
            )));
        }

        Ok(CompletionResponse {
            text: self.response_text.clone(),
            token_usage: TokenUsage {
                prompt_tokens: 15,
                completion_tokens: 25,
            },
        })
    }

    async fn health_check(&self) -> Result<bool, NexusV2Error> {
        Ok(!self.should_fail.load(Ordering::SeqCst))
    }
}
