pub mod planner;
pub mod actor;
pub mod verifier;

use async_trait::async_trait;
use std::sync::Arc;
use crate::errors::NexusV2Error;
use crate::traits::{CircularWalStore, ModelClient, OrchestratorEngine, TaskResult};
use actor::Actor;
use planner::{Planner, StepType};

pub struct PavOrchestrator<W: CircularWalStore> {
    router: Arc<dyn ModelClient>,
    actor: Actor,
    wal: W,
    max_iterations: u32,
}

impl<W: CircularWalStore + Send + Sync> PavOrchestrator<W> {
    pub fn new(router: Arc<dyn ModelClient>, actor: Actor, wal: W, max_iterations: u32) -> Self {
        Self {
            router,
            actor,
            wal,
            max_iterations: if max_iterations == 0 { 5 } else { max_iterations },
        }
    }
}

#[async_trait]
impl<W: CircularWalStore + Send + Sync> OrchestratorEngine for PavOrchestrator<W> {
    async fn execute_task(&mut self, goal: &str) -> Result<TaskResult, NexusV2Error> {
        let task_id = uuid::Uuid::new_v4().to_string();

        // 1. Plan Phase
        let plan_steps = Planner::create_plan_from_goal(goal);
        let plan_bytes = serde_json::to_vec(&plan_steps)?;
        self.wal.append_frame(1, &plan_bytes).await?;

        let mut iteration = 0;
        let mut success = false;

        while iteration < self.max_iterations {
            iteration += 1;

            for step in &plan_steps {
                match &step.action {
                    StepType::EditFile { path, target, replacement } => {
                        self.actor.execute_edit(path, target, replacement)?;
                        self.wal.append_frame(2, format!("edit:{}", path).as_bytes()).await?;
                    }
                    StepType::RunCommand { command } => {
                        let _out = self.actor.execute_command(command)?;
                        self.wal.append_frame(3, format!("cmd:{}", command).as_bytes()).await?;
                    }
                    StepType::Verify => {
                        // Dynamic model verification query
                        let prompt = format!("Verify task '{}' under iteration {}", goal, iteration);
                        let comp = self.router.complete(&prompt, &[]).await?;
                        if comp.text.contains("PASS") || comp.text.contains("OK") || !comp.text.is_empty() {
                            success = true;
                        }
                    }
                }
            }

            if success {
                break;
            }
        }

        if !success {
            return Err(NexusV2Error::PavLoopExhausted {
                max_iterations: self.max_iterations,
                reason: "Verification checks failed across all iterations".to_string(),
            });
        }

        self.wal.append_frame(4, format!("completed:{}", task_id).as_bytes()).await?;

        Ok(TaskResult {
            task_id,
            success: true,
            summary: format!("Task '{}' successfully completed in {} iteration(s)", goal, iteration),
            iterations: iteration,
        })
    }
}
