use super::config::AgentDiscovery;
use super::handback::{SubagentHandback, SubagentStatus};
use super::worktree::GitWorktreeManager;
use crate::events::{NexusEvent, TokenUsage};
use crate::provider::LlmProvider;
use crate::sandbox::Sandbox;
use crate::session::Message;
use crate::tools::ToolManager;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{mpsc, Semaphore};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum WorkspaceMode {
    #[default]
    Inherit,
    Worktree,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTask {
    pub agent: String,
    pub task: String,
    pub workspace_mode: Option<WorkspaceMode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum AgentTopology {
    Single(AgentTask),
    Parallel { tasks: Vec<AgentTask> },
    Chain { chain: Vec<AgentTask> },
}

pub struct AgentRunner;

impl AgentRunner {
    /// Executes a single subagent task autonomously in an isolated context window.
    pub async fn run_single(
        workspace_dir: PathBuf,
        task: AgentTask,
        provider: Arc<dyn LlmProvider>,
    ) -> SubagentHandback {
        let start_time = std::time::Instant::now();
        let agent_opt = AgentDiscovery::find_agent(&workspace_dir, &task.agent);

        let agent = match agent_opt {
            Some(a) => a,
            None => {
                return SubagentHandback {
                    agent_name: task.agent.clone(),
                    role: super::config::AgentRole::Worker,
                    task: task.task.clone(),
                    status: SubagentStatus::Failed(format!("Agent '{}' not found in built-ins or .nexus/agents/", task.agent)),
                    distilled_output: String::new(),
                    tools_used: Vec::new(),
                    turns_taken: 0,
                    token_usage: TokenUsage::default(),
                    duration_ms: start_time.elapsed().as_millis() as u64,
                };
            }
        };

        let agent_id = uuid::Uuid::new_v4().to_string()[..8].to_string();
        let use_worktree = matches!(task.workspace_mode, Some(WorkspaceMode::Worktree));

        let (target_workspace, branch_name) = if use_worktree {
            match GitWorktreeManager::create_worktree(&workspace_dir, &agent_id) {
                Ok((path, branch)) => (path, branch),
                Err(_) => (workspace_dir.clone(), None),
            }
        } else {
            (workspace_dir.clone(), None)
        };

        let sandbox = match Sandbox::new(&target_workspace, true) {
            Ok(s) => s,
            Err(e) => {
                if use_worktree {
                    let _ = GitWorktreeManager::cleanup_worktree(&workspace_dir, &target_workspace, branch_name.as_deref());
                }
                return SubagentHandback {
                    agent_name: agent.name,
                    role: agent.role,
                    task: task.task,
                    status: SubagentStatus::Failed(format!("Sandbox initialization failed: {}", e)),
                    distilled_output: String::new(),
                    tools_used: Vec::new(),
                    turns_taken: 0,
                    token_usage: TokenUsage::default(),
                    duration_ms: start_time.elapsed().as_millis() as u64,
                };
            }
        };

        let tool_manager = ToolManager::new(sandbox);
        let all_tool_defs = ToolManager::tool_definitions();
        let allowed_tool_defs: Vec<serde_json::Value> = all_tool_defs
            .into_iter()
            .filter(|t| {
                t.get("name")
                    .and_then(|n| n.as_str())
                    .map(|name| agent.allows_tool(name))
                    .unwrap_or(false)
            })
            .collect();

        let mut messages = vec![Message {
            role: "user".to_string(),
            content: task.task.clone(),
            tool_calls: None,
            tool_results: None,
            timestamp: Utc::now(),
        }];

        let mut accumulated_tokens = TokenUsage::default();
        let mut tools_used = Vec::new();
        let mut turns_taken = 0;
        let mut final_output = String::new();
        let mut subagent_status = SubagentStatus::Success;

        for _ in 0..agent.max_turns {
            turns_taken += 1;
            let (tx, mut rx) = mpsc::channel(64);
            let provider_clone = provider.clone();
            let sys_prompt = agent.system_prompt.clone();
            let history_clone = messages.clone();
            let tools_clone = allowed_tool_defs.clone();

            let handle = tokio::spawn(async move {
                provider_clone
                    .stream_turn(&sys_prompt, &history_clone, &tools_clone, tx)
                    .await
            });

            while let Some(event) = rx.recv().await {
                if let NexusEvent::TokenUsageRecorded(u) = event {
                    accumulated_tokens.input_tokens += u.input_tokens;
                    accumulated_tokens.output_tokens += u.output_tokens;
                    accumulated_tokens.cache_read_tokens += u.cache_read_tokens;
                    accumulated_tokens.cache_creation_tokens += u.cache_creation_tokens;
                }
            }

            let assistant_msg = match handle.await {
                Ok(Ok(msg)) => msg,
                Ok(Err(err)) => {
                    subagent_status = SubagentStatus::Failed(err.to_string());
                    break;
                }
                Err(err) => {
                    subagent_status = SubagentStatus::Failed(err.to_string());
                    break;
                }
            };

            messages.push(assistant_msg.clone());

            if let Some(calls) = &assistant_msg.tool_calls {
                let mut results = Vec::new();
                for call in calls {
                    if !tools_used.contains(&call.name) {
                        tools_used.push(call.name.clone());
                    }

                    if call.name == "invoke_subagent" {
                        results.push(crate::tools::ToolResult {
                            call_id: call.id.clone(),
                            output: "Permission denied: subagents cannot spawn nested subagents.".to_string(),
                            is_error: true,
                        });
                        continue;
                    }

                    if !agent.allows_tool(&call.name) {
                        results.push(crate::tools::ToolResult {
                            call_id: call.id.clone(),
                            output: format!("Permission denied: tool '{}' is restricted for agent role '{}'", call.name, agent.role),
                            is_error: true,
                        });
                        continue;
                    }

                    let res = tool_manager.execute(call).await;
                    results.push(res);
                }

                messages.push(Message {
                    role: "user".to_string(),
                    content: String::new(),
                    tool_calls: None,
                    tool_results: Some(results),
                    timestamp: Utc::now(),
                });
            } else {
                final_output = assistant_msg.content;
                break;
            }
        }

        if final_output.trim().is_empty() && subagent_status == SubagentStatus::Success {
            if let Some(last) = messages.iter().rev().find(|m| m.role == "assistant" && !m.content.trim().is_empty()) {
                final_output = last.content.clone();
            } else {
                let tools_list = if tools_used.is_empty() {
                    "none".to_string()
                } else {
                    tools_used.join(", ")
                };
                final_output = format!(
                    "Subagent '{}' completed {} turn(s) with tools: [{}].",
                    agent.name,
                    turns_taken,
                    tools_list
                );
            }
        }

        if use_worktree {
            if let Ok(Some(preserved_branch)) = GitWorktreeManager::cleanup_worktree(&workspace_dir, &target_workspace, branch_name.as_deref()) {
                final_output.push_str(&format!(
                    "\n\n> 🌿 **Worktree Preserved**: Subagent committed changes to branch `{}`. Run `git merge {}` to integrate.",
                    preserved_branch, preserved_branch
                ));
            }
        }

        SubagentHandback {
            agent_name: agent.name,
            role: agent.role,
            task: task.task,
            status: subagent_status,
            distilled_output: final_output,
            tools_used,
            turns_taken,
            token_usage: accumulated_tokens,
            duration_ms: start_time.elapsed().as_millis() as u64,
        }
    }

    /// Executes multiple subagents concurrently with a semaphore bounding parallelism (max 4 concurrent).
    pub async fn run_parallel(
        workspace_dir: PathBuf,
        tasks: Vec<AgentTask>,
        provider: Arc<dyn LlmProvider>,
        max_concurrency: usize,
    ) -> Vec<SubagentHandback> {
        let semaphore = Arc::new(Semaphore::new(max_concurrency.clamp(1, 8)));
        let mut handles = Vec::new();

        for task in tasks {
            let sem_clone = semaphore.clone();
            let ws_clone = workspace_dir.clone();
            let provider_clone = provider.clone();

            let handle = tokio::spawn(async move {
                let _permit = sem_clone.acquire().await.unwrap();
                Self::run_single(ws_clone, task, provider_clone).await
            });

            handles.push(handle);
        }

        let mut results = Vec::new();
        for h in handles {
            if let Ok(res) = h.await {
                results.push(res);
            }
        }
        results
    }

    /// Executes a sequential pipeline of subagents where step N's output replaces '{previous}' in step N+1.
    pub async fn run_chain(
        workspace_dir: PathBuf,
        chain: Vec<AgentTask>,
        provider: Arc<dyn LlmProvider>,
    ) -> Vec<SubagentHandback> {
        let mut results = Vec::new();
        let mut previous_output = String::new();

        for (i, step) in chain.into_iter().enumerate() {
            let mut resolved_task = step.task;
            if i > 0 && resolved_task.contains("{previous}") {
                resolved_task = resolved_task.replace("{previous}", &previous_output);
            }

            let task = AgentTask {
                agent: step.agent,
                task: resolved_task,
                workspace_mode: step.workspace_mode,
            };

            let handback = Self::run_single(workspace_dir.clone(), task, provider.clone()).await;
            previous_output = handback.distilled_output.clone();
            let is_err = matches!(handback.status, SubagentStatus::Failed(_));
            results.push(handback);

            if is_err {
                // Break pipeline on error like Pi chain mode
                break;
            }
        }

        results
    }
}
