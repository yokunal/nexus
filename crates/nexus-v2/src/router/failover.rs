use async_trait::async_trait;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use crate::errors::NexusV2Error;
use crate::traits::{CompletionResponse, Message, ModelClient};

pub struct DualModelRouter {
    primary: Arc<dyn ModelClient>,
    fallback: Arc<dyn ModelClient>,
    failover_count: Arc<AtomicUsize>,
}

impl DualModelRouter {
    pub fn new(primary: Arc<dyn ModelClient>, fallback: Arc<dyn ModelClient>) -> Self {
        Self {
            primary,
            fallback,
            failover_count: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub fn get_failover_count(&self) -> usize {
        self.failover_count.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl ModelClient for DualModelRouter {
    async fn complete(&self, prompt: &str, context: &[Message]) -> Result<CompletionResponse, NexusV2Error> {
        match self.primary.complete(prompt, context).await {
            Ok(resp) => Ok(resp),
            Err(primary_err) => {
                // Increment failover counter and transparently invoke fallback model
                self.failover_count.fetch_add(1, Ordering::SeqCst);
                match self.fallback.complete(prompt, context).await {
                    Ok(resp) => Ok(resp),
                    Err(fallback_err) => Err(NexusV2Error::DualModelFailure {
                        primary_err: primary_err.to_string(),
                        fallback_err: fallback_err.to_string(),
                    }),
                }
            }
        }
    }

    async fn health_check(&self) -> Result<bool, NexusV2Error> {
        let p_ok = self.primary.health_check().await.unwrap_or(false);
        let f_ok = self.fallback.health_check().await.unwrap_or(false);
        Ok(p_ok || f_ok)
    }
}
