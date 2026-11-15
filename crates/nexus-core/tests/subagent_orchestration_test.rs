use async_trait::async_trait;
use nexus_core::agent::{
    AgentDiscovery, AgentRole, AgentRunner, AgentSource, AgentTask, GitWorktreeManager,
    SubagentHandback, SubagentStatus,
};
use nexus_core::events::{NexusEvent, TokenUsage};
use nexus_core::provider::{LlmProvider, ProviderError};
use nexus_core::sandbox::Sandbox;
use nexus_core::session::Message;
use nexus_core::tools::{ToolCall, ToolManager};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use uuid::Uuid;
use tokio::sync::mpsc::Sender;

struct TempTestDir {
    pub path: PathBuf,
}

impl TempTestDir {
    fn new(prefix: &str) -> Self {
        let path = std::env::temp_dir().join(format!("{}_{}", prefix, Uuid::new_v4()));
        fs::create_dir_all(&path).unwrap();
        Self { path }
    }
}

impl Drop for TempTestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// Mock LLM provider for zero-cost offline testing of multi-agent orchestration
struct MockSubagentProvider {
    turn_counter: AtomicUsize,
}

impl MockSubagentProvider {
    fn new() -> Self {
        Self {
            turn_counter: AtomicUsize::new(0),
        }
    }
}

#[async_trait]
impl LlmProvider for MockSubagentProvider {
    async fn stream_turn(
        &self,
        system_prompt: &str,
        history: &[Message],
        _tools: &[serde_json::Value],
        event_tx: Sender<NexusEvent>,
    ) -> Result<Message, ProviderError> {
        let count = self.turn_counter.fetch_add(1, Ordering::SeqCst);
        let last_prompt = history
            .last()
            .map(|m| m.content.as_str())
            .unwrap_or("default");

        let response_text = format!(
            "Mock Agent Response #{} for prompt [{}]. System role: {}",
            count + 1,
            last_prompt,
            if system_prompt.contains("scout") {
                "Scout"
            } else if system_prompt.contains("reviewer") {
                "Reviewer"
            } else {
                "Worker"
            }
        );

        // Stream mock tokens
        let _ = event_tx
            .send(NexusEvent::TokenChunk(response_text.clone()))
            .await;
        let _ = event_tx
            .send(NexusEvent::TokenUsageRecorded(TokenUsage {
                input_tokens: 120,
                output_tokens: 40,
                cache_read_tokens: 250,
                cache_creation_tokens: 0,
            }))
            .await;

        Ok(Message {
            role: "assistant".to_string(),
            content: response_text,
            tool_calls: None,
            tool_results: None,
            timestamp: chrono::Utc::now(),
        })
    }
}

#[test]
fn test_agent_discovery_builtins() {
    let temp = TempTestDir::new("nexus_disc_test");
    let agents = AgentDiscovery::discover_agents(&temp.path);

    assert!(
        agents.len() >= 4,
        "Must discover at least 4 built-in subagents"
    );

    let scout = agents.iter().find(|a| a.name == "scout").unwrap();
    assert_eq!(scout.role, AgentRole::Scout);
    assert_eq!(scout.source, AgentSource::Builtin);
    assert!(scout.allows_tool("read_file"));
    assert!(scout.allows_tool("list_agents"));
    assert!(!scout.allows_tool("write_file"));
    assert!(!scout.allows_tool("run_command"));
    assert!(!scout.allows_tool("invoke_subagent"));

    let reviewer = agents.iter().find(|a| a.name == "reviewer").unwrap();
    assert_eq!(reviewer.role, AgentRole::Reviewer);
    assert!(reviewer.allows_tool("read_file"));
    assert!(reviewer.allows_tool("cargo_check"));
    assert!(!reviewer.allows_tool("write_file"));
    assert!(!reviewer.allows_tool("edit_file"));

    let worker = agents.iter().find(|a| a.name == "worker").unwrap();
    assert_eq!(worker.role, AgentRole::Worker);
    assert!(worker.allows_tool("read_file"));
    assert!(worker.allows_tool("write_file"));
    assert!(worker.allows_tool("edit_file"));
    assert!(worker.allows_tool("run_command"));
    assert!(!worker.allows_tool("invoke_subagent")); // Fork bomb prevention
}

#[test]
fn test_agent_discovery_markdown_frontmatter() {
    let temp = TempTestDir::new("nexus_frontmatter_test");
    let agents_dir = temp.path.join(".nexus").join("agents");
    fs::create_dir_all(&agents_dir).unwrap();

    let custom_agent_md = r#"---
name: secops
role: reviewer
description: Security vulnerability and invariant auditor
tools:
  - read_file
  - cargo_check
---
You are a senior security researcher specializing in memory safety and zero token leakage.
"#;
    fs::write(agents_dir.join("secops.md"), custom_agent_md).unwrap();

    let agents = AgentDiscovery::discover_agents(&temp.path);
    let secops = agents
        .iter()
        .find(|a| a.name == "secops")
        .expect("Must discover secops from markdown");

    assert_eq!(secops.name, "secops");
    assert_eq!(secops.role, AgentRole::Reviewer);
    assert_eq!(secops.source, AgentSource::Project);
    assert_eq!(
        secops.description,
        "Security vulnerability and invariant auditor"
    );
    assert!(secops.system_prompt.contains("senior security researcher"));
    assert!(secops.allows_tool("read_file"));
    assert!(secops.allows_tool("cargo_check"));
    assert!(!secops.allows_tool("write_file"));
}

#[test]
fn test_subagent_handback_formatting() {
    let handback = SubagentHandback {
        agent_name: "scout".to_string(),
        role: AgentRole::Scout,
        task: "Inspect architecture invariants in AGENTS.md".to_string(),
        status: SubagentStatus::Success,
        distilled_output: "All 5 architecture invariants confirmed intact.".to_string(),
        tools_used: vec!["read_file".to_string()],
        turns_taken: 1,
        token_usage: TokenUsage {
            input_tokens: 150,
            output_tokens: 35,
            cache_read_tokens: 500,
            cache_creation_tokens: 0,
        },
        duration_ms: 120,
    };

    let card = handback.format_handback_card();
    assert!(card.contains("<subagent_handback"));
    assert!(card.contains(r#"agent="scout""#));
    assert!(card.contains(r#"role="scout""#));
    assert!(card.contains("COMPLETED"));
    assert!(card.contains("All 5 architecture invariants confirmed intact."));
    assert!(card.contains("150 in / 35 out tokens"));
    assert!(card.contains("</subagent_handback>"));
}

#[tokio::test]
async fn test_git_worktree_fallback_manager() {
    let temp = TempTestDir::new("nexus_worktree_test");
    let agent_id = "test-worker-alpha";

    let worktree_res = GitWorktreeManager::create_worktree(&temp.path, agent_id);
    assert!(
        worktree_res.is_ok(),
        "Worktree creation must succeed even in non-git directory"
    );

    let (worktree_path, branch_name) = worktree_res.unwrap();
    assert!(worktree_path.exists(), "Worktree folder must exist on disk");

    let cleanup_res =
        GitWorktreeManager::cleanup_worktree(&temp.path, &worktree_path, branch_name.as_deref());
    assert!(cleanup_res.is_ok(), "Worktree cleanup must succeed");
    assert!(!worktree_path.exists(), "Worktree folder must be cleanly deleted");
}

#[tokio::test]
async fn test_agent_runner_single() {
    let temp = TempTestDir::new("nexus_runner_single_test");
    let provider: Arc<dyn LlmProvider> = Arc::new(MockSubagentProvider::new());

    let task = AgentTask {
        agent: "scout".to_string(),
        task: "Survey crates and module layout".to_string(),
        workspace_mode: None,
    };

    let handback = AgentRunner::run_single(temp.path.clone(), task, provider).await;

    assert_eq!(handback.status, SubagentStatus::Success);
    assert_eq!(handback.agent_name, "scout");
    assert_eq!(handback.role, AgentRole::Scout);
    assert!(handback.distilled_output.contains("Mock Agent Response"));
    assert!(handback.token_usage.output_tokens > 0);
}

#[tokio::test]
async fn test_agent_runner_parallel() {
    let temp = TempTestDir::new("nexus_runner_parallel_test");
    let provider: Arc<dyn LlmProvider> = Arc::new(MockSubagentProvider::new());

    let tasks = vec![
        AgentTask {
            agent: "scout".to_string(),
            task: "Scan crate 1".to_string(),
            workspace_mode: None,
        },
        AgentTask {
            agent: "reviewer".to_string(),
            task: "Audit crate 2".to_string(),
            workspace_mode: None,
        },
        AgentTask {
            agent: "worker".to_string(),
            task: "Benchmark crate 3".to_string(),
            workspace_mode: None,
        },
    ];

    let handbacks = AgentRunner::run_parallel(temp.path.clone(), tasks, provider, 3).await;

    assert_eq!(handbacks.len(), 3, "Must return 3 handbacks from parallel run");
    for h in handbacks {
        assert_eq!(h.status, SubagentStatus::Success);
        assert!(h.distilled_output.contains("Mock Agent Response"));
    }
}

#[tokio::test]
async fn test_agent_runner_chain() {
    let temp = TempTestDir::new("nexus_runner_chain_test");
    let provider: Arc<dyn LlmProvider> = Arc::new(MockSubagentProvider::new());

    let tasks = vec![
        AgentTask {
            agent: "scout".to_string(),
            task: "Phase 1: Discover API surface".to_string(),
            workspace_mode: None,
        },
        AgentTask {
            agent: "reviewer".to_string(),
            task: "Phase 2: Review previous findings: {previous}".to_string(),
            workspace_mode: None,
        },
    ];

    let handbacks = AgentRunner::run_chain(temp.path.clone(), tasks, provider).await;

    assert_eq!(handbacks.len(), 2, "Must return 2 handbacks for 2-step chain");
    assert_eq!(handbacks[0].status, SubagentStatus::Success);
    assert_eq!(handbacks[1].status, SubagentStatus::Success);
    assert!(handbacks[1].distilled_output.contains("Mock Agent Response"));
}

#[tokio::test]
async fn test_tool_manager_invoke_subagent_and_list() {
    let temp = TempTestDir::new("nexus_tools_subagent_test");
    let sandbox = Sandbox::new(&temp.path, true).unwrap();
    let provider: Arc<dyn LlmProvider> = Arc::new(MockSubagentProvider::new());

    let tool_mgr = ToolManager::new(sandbox).with_provider(provider);

    // 1. Test list_agents
    let list_call = ToolCall {
        id: "call_list_1".to_string(),
        name: "list_agents".to_string(),
        arguments: serde_json::json!({}),
    };
    let list_result = tool_mgr.execute(&list_call).await;
    assert!(!list_result.is_error);
    assert!(list_result.output.contains("scout"));
    assert!(list_result.output.contains("reviewer"));
    assert!(list_result.output.contains("worker"));

    // 2. Test invoke_subagent single
    let invoke_single_call = ToolCall {
        id: "call_invoke_1".to_string(),
        name: "invoke_subagent".to_string(),
        arguments: serde_json::json!({
            "agent": "scout",
            "task": "Perform read-only security scan",
            "topology": "single"
        }),
    };
    let single_result = tool_mgr.execute(&invoke_single_call).await;
    assert!(!single_result.is_error);
    assert!(single_result.output.contains("<subagent_handback"));
    assert!(single_result.output.contains(r#"agent="scout""#));

    // 3. Test invoke_subagent parallel
    let invoke_parallel_call = ToolCall {
        id: "call_invoke_2".to_string(),
        name: "invoke_subagent".to_string(),
        arguments: serde_json::json!({
            "topology": "parallel",
            "tasks": [
                { "agent": "scout", "task": "Check license headers" },
                { "agent": "reviewer", "task": "Check code comments" }
            ]
        }),
    };
    let parallel_result = tool_mgr.execute(&invoke_parallel_call).await;
    assert!(!parallel_result.is_error);
    assert!(parallel_result.output.contains("Parallel Subagent Execution Report (2 tasks)"));
    assert!(parallel_result.output.contains(r#"agent="scout""#));
    assert!(parallel_result.output.contains(r#"agent="reviewer""#));
}
