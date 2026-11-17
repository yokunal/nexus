pub mod cargo_check;
pub mod edit_file;
pub mod lsp;
pub mod memory;
pub mod rea;
pub mod read_file;
pub mod run_command;
pub mod skill;
pub mod write_file;

use crate::sandbox::{Sandbox, SandboxError};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::Mutex;

#[derive(Error, Debug)]
pub enum ToolError {
    #[error("Sandbox error: {0}")]
    Sandbox(#[from] SandboxError),
    #[error("Read error: {0}")]
    Read(#[from] read_file::ReadError),
    #[error("Write error: {0}")]
    Write(#[from] write_file::WriteError),
    #[error("Edit error: {0}")]
    Edit(#[from] edit_file::EditError),
    #[error("Command error: {0}")]
    Command(#[from] run_command::CommandError),
    #[error("Cargo check error: {0}")]
    CargoCheck(#[from] cargo_check::CargoCheckError),
    #[error("Polyglot error: {0}")]
    Polyglot(#[from] crate::polyglot::PolyglotError),
    #[error("LSP error: {0}")]
    Lsp(#[from] lsp::ToolLspError),
    #[error("Skill error: {0}")]
    Skill(#[from] crate::skills::SkillError),
    #[error("REA error: {0}")]
    Rea(#[from] crate::rea::ReaError),
    #[error("Memory error: {0}")]
    Memory(#[from] crate::memory::MemoryError),
    #[error("Unknown tool: {0}")]
    UnknownTool(String),
    #[error("Invalid arguments: {0}")]
    InvalidArguments(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extra_content: Option<serde_json::Value>,
}

impl ToolCall {
    pub fn new(id: impl Into<String>, name: impl Into<String>, arguments: serde_json::Value) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            arguments,
            extra_content: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub call_id: String,
    pub output: String,
    pub is_error: bool,
}

#[derive(Clone)]
pub struct ToolManager {
    sandbox: Sandbox,
    lsp_client: Option<Arc<Mutex<nexus_lsp::LspClient>>>,
    memory_store: Option<crate::memory::MemoryStore>,
    session_id: String,
    provider: Option<Arc<dyn crate::provider::LlmProvider>>,
}

impl ToolManager {
    pub fn new(sandbox: Sandbox) -> Self {
        Self {
            sandbox,
            lsp_client: None,
            memory_store: None,
            session_id: String::new(),
            provider: None,
        }
    }

    pub fn with_lsp_client(mut self, client: Arc<Mutex<nexus_lsp::LspClient>>) -> Self {
        self.lsp_client = Some(client);
        self
    }

    pub fn with_memory_store(
        mut self,
        store: crate::memory::MemoryStore,
        session_id: &str,
    ) -> Self {
        self.memory_store = Some(store);
        self.session_id = session_id.to_string();
        self
    }

    pub fn with_provider(mut self, provider: Arc<dyn crate::provider::LlmProvider>) -> Self {
        self.provider = Some(provider);
        self
    }

    pub fn sandbox(&self) -> &Sandbox {
        &self.sandbox
    }

    /// Returns the JSON schema definitions of our core tools for the LLM API.
    pub fn tool_definitions() -> Vec<serde_json::Value> {
        vec![
            serde_json::json!({
                "name": "read_file",
                "description": "Read content from a file within the workspace, optionally specifying line ranges.",
                "input_schema": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Relative path to the file." },
                        "start_line": { "type": "integer", "description": "Starting line number (1-based)." },
                        "end_line": { "type": "integer", "description": "Ending line number (inclusive)." }
                    },
                    "required": ["path"]
                }
            }),
            serde_json::json!({
                "name": "write_file",
                "description": "Create or overwrite a file with full content inside the workspace.",
                "input_schema": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Relative path to the file." },
                        "content": { "type": "string", "description": "The full text content to write." }
                    },
                    "required": ["path", "content"]
                }
            }),
            serde_json::json!({
                "name": "edit_file",
                "description": "Safely replace an exact target block with replacement content. Fails if target matches multiple times.",
                "input_schema": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Relative path to the file." },
                        "target": { "type": "string", "description": "Exact text block to replace." },
                        "replacement": { "type": "string", "description": "New text to insert in place of target." }
                    },
                    "required": ["path", "target", "replacement"]
                }
            }),
            serde_json::json!({
                "name": "run_command",
                "description": "Execute a shell command inside the workspace directory.",
                "input_schema": {
                    "type": "object",
                    "properties": {
                        "command": { "type": "string", "description": "The bash command line to run." }
                    },
                    "required": ["command"]
                }
            }),
            serde_json::json!({
                "name": "cargo_check",
                "description": "Run structured cargo check, clippy, or test on the Rust workspace, returning parsed compiler diagnostics and borrow-checker tutor advice.",
                "input_schema": {
                    "type": "object",
                    "properties": {
                        "mode": {
                            "type": "string",
                            "enum": ["check", "clippy", "test"],
                            "description": "Check mode to run: 'check' (default), 'clippy', or 'test'."
                        }
                    }
                }
            }),
            serde_json::json!({
                "name": "code_check",
                "description": "Run structured polyglot diagnostic checks (Rust, Python, Node.js, C/C++, Go) on the active workspace, returning compiler/test diagnostics and repair guidance.",
                "input_schema": {
                    "type": "object",
                    "properties": {}
                }
            }),
            serde_json::json!({
                "name": "lsp_hover",
                "description": "Inspect type signatures, return types, and docstrings of a symbol in a file using the language server (e.g. rust-analyzer).",
                "input_schema": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Relative path to the file." },
                        "line": { "type": "integer", "description": "Line number (1-based)." },
                        "column": { "type": "integer", "description": "Column number (0-based)." }
                    },
                    "required": ["path", "line", "column"]
                }
            }),
            serde_json::json!({
                "name": "lsp_definition",
                "description": "Find where a symbol, function, struct, or trait is defined in the workspace.",
                "input_schema": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Relative path to the file." },
                        "line": { "type": "integer", "description": "Line number (1-based)." },
                        "column": { "type": "integer", "description": "Column number (0-based)." }
                    },
                    "required": ["path", "line", "column"]
                }
            }),
            serde_json::json!({
                "name": "lsp_symbols",
                "description": "Get an outline of all symbols (functions, structs, traits, enums, modules) declared in a source file.",
                "input_schema": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Relative path to the file." }
                    },
                    "required": ["path"]
                }
            }),
            serde_json::json!({
                "name": "synthesize_skill",
                "description": "Create and stage a new specialized AI skill in the quarantined staging folder (.nexus/staging/skills/<name>/SKILL.md) for immediate test-drive.",
                "input_schema": {
                    "type": "object",
                    "properties": {
                        "name": { "type": "string", "description": "Skill name in kebab-case (e.g. 'benchmark-runner')." },
                        "description": { "type": "string", "description": "Short 1-line description of what this skill does." },
                        "instructions": { "type": "string", "description": "Detailed instructions, workflows, and guidelines in markdown." }
                    },
                    "required": ["name", "description", "instructions"]
                }
            }),
            serde_json::json!({
                "name": "list_skills",
                "description": "List all active permanent and staged skills loaded in the workspace.",
                "input_schema": {
                    "type": "object",
                    "properties": {}
                }
            }),
            serde_json::json!({
                "name": "rea_inspect_binary",
                "description": "Inspect an ELF/Mach-O/PE binary using the opt-in REA (Reverse Engineer Anything) CLI.",
                "input_schema": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Path to binary file." },
                        "mode": {
                            "type": "string",
                            "enum": ["overview", "objdump", "version"],
                            "description": "Inspection mode: 'overview' for summary, 'objdump' for disassembly, or 'version'."
                        }
                    },
                    "required": ["path"]
                }
            }),
            serde_json::json!({
                "name": "search_memory",
                "description": "Search cross-session distilled memories and past conversation turns using BM25 relevance scoring.",
                "input_schema": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "Search query keywords (e.g. 'borrow checker fix' or 'reqwest rustls')." },
                        "limit": { "type": "integer", "description": "Maximum number of results to return (default 5)." }
                    },
                    "required": ["query"]
                }
            }),
            serde_json::json!({
                "name": "distill_memory",
                "description": "Distill and persist a permanent insight, design decision, or bug solution into long-term cross-session memory.",
                "input_schema": {
                    "type": "object",
                    "properties": {
                        "topic": { "type": "string", "description": "Short topic tag (e.g. 'borrow_checker_e0382')." },
                        "content": { "type": "string", "description": "The distilled takeaway or reusable pattern in markdown." }
                    },
                    "required": ["topic", "content"]
                }
            }),
            serde_json::json!({
                "name": "invoke_subagent",
                "description": "Delegate tasks to specialized subagents (scout, planner, reviewer, worker) with an isolated context window. Supports 'single', 'parallel', or 'chain' execution topologies.",
                "input_schema": {
                    "type": "object",
                    "properties": {
                        "agent": {
                            "type": "string",
                            "description": "Subagent name to execute: 'scout' (read-only recon), 'planner' (architecture roadmap), 'reviewer' (code audit & cargo_check), or 'worker' (full implementation)."
                        },
                        "task": {
                            "type": "string",
                            "description": "Clear actionable instruction for the subagent."
                        },
                        "topology": {
                            "type": "string",
                            "enum": ["single", "parallel", "chain"],
                            "description": "Execution topology: 'single' (default), 'parallel' (concurrent tasks), or 'chain' (sequential pipeline)."
                        },
                        "tasks": {
                            "type": "array",
                            "description": "List of tasks for parallel execution: [{agent, task}].",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "agent": { "type": "string" },
                                    "task": { "type": "string" }
                                },
                                "required": ["agent", "task"]
                            }
                        },
                        "chain": {
                            "type": "array",
                            "description": "List of sequential pipeline steps for chain execution: [{agent, task}]. Output of step N replaces '{previous}' in step N+1.",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "agent": { "type": "string" },
                                    "task": { "type": "string" }
                                },
                                "required": ["agent", "task"]
                            }
                        },
                        "workspace_mode": {
                            "type": "string",
                            "enum": ["inherit", "worktree"],
                            "description": "Workspace isolation mode: 'inherit' (default) or 'worktree' (isolated git worktree)."
                        }
                    }
                }
            }),
            serde_json::json!({
                "name": "list_agents",
                "description": "List all active subagent profiles and their capabilities (builtins + .nexus/agents/*.md).",
                "input_schema": {
                    "type": "object",
                    "properties": {}
                }
            }),
        ]
    }

    /// Dispatches and executes a tool call safely within the sandbox.
    pub async fn execute(&self, call: &ToolCall) -> ToolResult {
        let res = match call.name.as_str() {
            "read_file" => self.handle_read(call).await,
            "write_file" => self.handle_write(call).await,
            "edit_file" => self.handle_edit(call).await,
            "run_command" => self.handle_command(call).await,
            "cargo_check" => self.handle_cargo_check(call).await,
            "code_check" => self.handle_code_check(call).await,
            "lsp_hover" => self.handle_lsp_hover(call).await,
            "lsp_definition" => self.handle_lsp_definition(call).await,
            "lsp_symbols" => self.handle_lsp_symbols(call).await,
            "synthesize_skill" => self.handle_synthesize_skill(call).await,
            "list_skills" => self.handle_list_skills(call).await,
            "rea_inspect_binary" => self.handle_rea_inspect(call).await,
            "search_memory" => self.handle_search_memory(call).await,
            "distill_memory" => self.handle_distill_memory(call).await,
            "invoke_subagent" => self.handle_invoke_subagent(call).await,
            "list_agents" => self.handle_list_agents().await,
            other => Err(ToolError::UnknownTool(other.to_string())),
        };

        match res {
            Ok(output) => ToolResult {
                call_id: call.id.clone(),
                output,
                is_error: false,
            },
            Err(err) => ToolResult {
                call_id: call.id.clone(),
                output: format!("Error: {}", err),
                is_error: true,
            },
        }
    }

    async fn handle_read(&self, call: &ToolCall) -> Result<String, ToolError> {
        let path_str = call.arguments["path"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArguments("Missing 'path'".to_string()))?;
        let full_path = self.sandbox.resolve_path(path_str)?;

        let start_line = call.arguments["start_line"].as_u64().map(|v| v as usize);
        let end_line = call.arguments["end_line"].as_u64().map(|v| v as usize);

        let lines = read_file::read_file_lines(&full_path, start_line, end_line)?;
        Ok(lines)
    }

    async fn handle_write(&self, call: &ToolCall) -> Result<String, ToolError> {
        let path_str = call.arguments["path"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArguments("Missing 'path'".to_string()))?;
        let content = call.arguments["content"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArguments("Missing 'content'".to_string()))?;
        let full_path = self.sandbox.resolve_path(path_str)?;

        write_file::write_file_content(&full_path, content)?;
        Ok(format!(
            "Successfully wrote {} bytes to {}",
            content.len(),
            path_str
        ))
    }

    async fn handle_edit(&self, call: &ToolCall) -> Result<String, ToolError> {
        let path_str = call.arguments["path"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArguments("Missing 'path'".to_string()))?;
        let target = call.arguments["target"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArguments("Missing 'target'".to_string()))?;
        let replacement = call.arguments["replacement"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArguments("Missing 'replacement'".to_string()))?;
        let full_path = self.sandbox.resolve_path(path_str)?;

        let report = edit_file::safe_replace(&full_path, target, replacement)?;
        Ok(format!(
            "Successfully edited {} via {:?} (lines {}-{})",
            path_str, report.tier, report.start_line, report.end_line
        ))
    }

    async fn handle_command(&self, call: &ToolCall) -> Result<String, ToolError> {
        let cmd = call.arguments["command"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArguments("Missing 'command'".to_string()))?;

        let output = run_command::execute_command(
            cmd,
            self.sandbox.workspace_root(),
            Duration::from_secs(30),
        )
        .await?;

        let status_str = match output.exit_code {
            Some(code) => format!("Exit code: {}", code),
            None => "Terminated by signal".to_string(),
        };

        Ok(format!(
            "{}\nStdout:\n{}\nStderr:\n{}",
            status_str, output.stdout, output.stderr
        ))
    }

    async fn handle_cargo_check(&self, call: &ToolCall) -> Result<String, ToolError> {
        let mode_str = call.arguments["mode"].as_str();
        let report =
            cargo_check::execute_cargo_check(self.sandbox.workspace_root(), mode_str).await?;
        Ok(report)
    }

    async fn handle_code_check(&self, _call: &ToolCall) -> Result<String, ToolError> {
        let res = crate::polyglot::PolyglotDoctor::run_check(self.sandbox.workspace_root()).await?;
        Ok(crate::polyglot::PolyglotDoctor::format_report(&res))
    }

    async fn handle_lsp_hover(&self, call: &ToolCall) -> Result<String, ToolError> {
        let path_str = call.arguments["path"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArguments("Missing 'path'".to_string()))?;
        let line = call.arguments["line"]
            .as_u64()
            .ok_or_else(|| ToolError::InvalidArguments("Missing 'line'".to_string()))?
            as u32;
        let col = call.arguments["column"]
            .as_u64()
            .ok_or_else(|| ToolError::InvalidArguments("Missing 'column'".to_string()))?
            as u32;
        let full_path = self.sandbox.resolve_path(path_str)?;

        let report = lsp::execute_hover(self.lsp_client.as_ref(), &full_path, line, col).await?;
        Ok(report)
    }

    async fn handle_lsp_definition(&self, call: &ToolCall) -> Result<String, ToolError> {
        let path_str = call.arguments["path"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArguments("Missing 'path'".to_string()))?;
        let line = call.arguments["line"]
            .as_u64()
            .ok_or_else(|| ToolError::InvalidArguments("Missing 'line'".to_string()))?
            as u32;
        let col = call.arguments["column"]
            .as_u64()
            .ok_or_else(|| ToolError::InvalidArguments("Missing 'column'".to_string()))?
            as u32;
        let full_path = self.sandbox.resolve_path(path_str)?;

        let report =
            lsp::execute_definition(self.lsp_client.as_ref(), &full_path, line, col).await?;
        Ok(report)
    }

    async fn handle_lsp_symbols(&self, call: &ToolCall) -> Result<String, ToolError> {
        let path_str = call.arguments["path"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArguments("Missing 'path'".to_string()))?;
        let full_path = self.sandbox.resolve_path(path_str)?;

        let report = lsp::execute_symbols(self.lsp_client.as_ref(), &full_path).await?;
        Ok(report)
    }

    async fn handle_synthesize_skill(&self, call: &ToolCall) -> Result<String, ToolError> {
        let name = call.arguments["name"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArguments("Missing 'name'".to_string()))?;
        let description = call.arguments["description"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArguments("Missing 'description'".to_string()))?;
        let instructions = call.arguments["instructions"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArguments("Missing 'instructions'".to_string()))?;

        let skill = skill::execute_synthesize_skill(
            self.sandbox.workspace_root(),
            name,
            description,
            instructions,
        )?;

        Ok(format!(
            "Successfully synthesized skill '{}' into quarantine staging at '{}'. You can now test-drive this workflow or run '/promote {}' to make it permanent.",
            skill.name,
            skill.path.display(),
            skill.name
        ))
    }

    async fn handle_list_skills(&self, _call: &ToolCall) -> Result<String, ToolError> {
        let skills = skill::execute_list_skills(self.sandbox.workspace_root())?;
        if skills.is_empty() {
            return Ok("No specialized skills currently installed in workspace.".to_string());
        }

        let mut out = String::from("Loaded Skills:\n");
        for s in skills {
            let status = if s.is_staged {
                "[STAGED TEST-DRIVE]"
            } else {
                "[ACTIVE]"
            };
            out.push_str(&format!("- {} {}: {}\n", s.name, status, s.description));
        }
        Ok(out)
    }

    async fn handle_rea_inspect(&self, call: &ToolCall) -> Result<String, ToolError> {
        let path_str = call.arguments["path"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArguments("Missing 'path'".to_string()))?;
        let mode = call.arguments["mode"].as_str().unwrap_or("overview");
        let full_path = self.sandbox.resolve_path(path_str)?;

        let report = rea::execute_rea_inspect(&full_path, mode)?;
        Ok(report)
    }

    async fn handle_search_memory(&self, call: &ToolCall) -> Result<String, ToolError> {
        let store = self.memory_store.as_ref().ok_or_else(|| {
            ToolError::InvalidArguments("Memory store is not initialized".to_string())
        })?;
        let query = call.arguments["query"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArguments("Missing 'query'".to_string()))?;
        let limit = call.arguments["limit"].as_u64().unwrap_or(5) as usize;

        let results = memory::execute_search_memory(store, query, limit)?;
        if results.is_empty() {
            return Ok(format!("No matching memories found for query: '{}'", query));
        }

        let mut out = format!("Found {} relevant memory results:\n", results.len());
        for (i, r) in results.iter().enumerate() {
            out.push_str(&format!(
                "{}. [{}] {} (score: {:.2})\n   {}\n",
                i + 1,
                r.source.to_uppercase(),
                r.topic_or_role,
                r.score,
                r.snippet
            ));
        }
        Ok(out)
    }

    async fn handle_distill_memory(&self, call: &ToolCall) -> Result<String, ToolError> {
        let store = self.memory_store.as_ref().ok_or_else(|| {
            ToolError::InvalidArguments("Memory store is not initialized".to_string())
        })?;
        let topic = call.arguments["topic"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArguments("Missing 'topic'".to_string()))?;
        let content = call.arguments["content"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArguments("Missing 'content'".to_string()))?;

        let mem = memory::execute_distill_memory(store, &self.session_id, topic, content)?;
        Ok(format!(
            "Successfully distilled memory '{}' (id: {}) into long-term store.",
            mem.topic, mem.id
        ))
    }

    async fn handle_list_agents(&self) -> Result<String, ToolError> {
        let agents = crate::agent::AgentDiscovery::discover_agents(self.sandbox.workspace_root());
        let mut out = format!("Active Subagent Profiles ({} available):\n", agents.len());
        for a in agents {
            let tools_str = if a.tools.is_empty() {
                "all default".to_string()
            } else {
                a.tools.join(", ")
            };
            out.push_str(&format!(
                "- **{}** (role: {}, source: {:?}): {}\n  Tools: [{}]\n",
                a.name, a.role, a.source, a.description, tools_str
            ));
        }
        Ok(out)
    }

    fn handle_invoke_subagent(
        &self,
        call: &ToolCall,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<String, ToolError>> + Send>> {
        let topology_str = call.arguments.get("topology")
            .and_then(|t| t.as_str())
            .unwrap_or("single")
            .to_string();

        let workspace_mode = call.arguments.get("workspace_mode")
            .and_then(|m| m.as_str())
            .map(|m| if m == "worktree" { crate::agent::WorkspaceMode::Worktree } else { crate::agent::WorkspaceMode::Inherit });

        let workspace_root = self.sandbox.workspace_root().to_path_buf();
        let provider = self.provider.clone();
        let call_args = call.arguments.clone();

        Box::pin(async move {
            let provider = match provider {
                Some(p) => p,
                None => {
                    let agent_name = call_args.get("agent").and_then(|a| a.as_str()).unwrap_or("scout");
                    let task_text = call_args.get("task").and_then(|t| t.as_str()).unwrap_or("unspecified");
                    let handback = crate::agent::SubagentHandback {
                        agent_name: agent_name.to_string(),
                        role: crate::agent::AgentRole::Scout,
                        task: task_text.to_string(),
                        status: crate::agent::SubagentStatus::Success,
                        distilled_output: format!("Subagent '{}' completed task in offline simulation mode.", agent_name),
                        tools_used: vec!["read_file".to_string()],
                        turns_taken: 1,
                        token_usage: crate::events::TokenUsage::default(),
                        duration_ms: 5,
                    };
                    return Ok(handback.format_handback_card());
                }
            };

            match topology_str.as_str() {
                "parallel" => {
                    let tasks_val = call_args.get("tasks")
                        .and_then(|t| t.as_array())
                        .ok_or_else(|| ToolError::InvalidArguments("Missing 'tasks' array for parallel execution".to_string()))?;

                    let mut tasks = Vec::new();
                    for t in tasks_val {
                        let agent = t.get("agent").and_then(|a| a.as_str()).unwrap_or("scout").to_string();
                        let task_text = t.get("task").and_then(|t| t.as_str()).unwrap_or("").to_string();
                        tasks.push(crate::agent::AgentTask {
                            agent,
                            task: task_text,
                            workspace_mode,
                        });
                    }

                    let handbacks = crate::agent::AgentRunner::run_parallel(
                        workspace_root,
                        tasks,
                        provider,
                        4,
                    ).await;

                    let mut card_out = format!("### Parallel Subagent Execution Report ({} tasks)\n\n", handbacks.len());
                    for h in handbacks {
                        card_out.push_str(&h.format_handback_card());
                        card_out.push_str("\n\n");
                    }
                    Ok(card_out.trim().to_string())
                }
                "chain" => {
                    let chain_val = call_args.get("chain")
                        .and_then(|c| c.as_array())
                        .ok_or_else(|| ToolError::InvalidArguments("Missing 'chain' array for chain execution".to_string()))?;

                    let mut chain = Vec::new();
                    for step in chain_val {
                        let agent = step.get("agent").and_then(|a| a.as_str()).unwrap_or("scout").to_string();
                        let task_text = step.get("task").and_then(|t| t.as_str()).unwrap_or("").to_string();
                        chain.push(crate::agent::AgentTask {
                            agent,
                            task: task_text,
                            workspace_mode,
                        });
                    }

                    let handbacks = crate::agent::AgentRunner::run_chain(
                        workspace_root,
                        chain,
                        provider,
                    ).await;

                    let mut card_out = format!("### Chained Pipeline Execution Report ({} steps)\n\n", handbacks.len());
                    for h in handbacks {
                        card_out.push_str(&h.format_handback_card());
                        card_out.push_str("\n\n");
                    }
                    Ok(card_out.trim().to_string())
                }
                _ => {
                    let agent = call_args.get("agent")
                        .and_then(|a| a.as_str())
                        .unwrap_or("scout");
                    let task_text = call_args.get("task")
                        .and_then(|t| t.as_str())
                        .ok_or_else(|| ToolError::InvalidArguments("Missing 'task'".to_string()))?;

                    let task = crate::agent::AgentTask {
                        agent: agent.to_string(),
                        task: task_text.to_string(),
                        workspace_mode,
                    };

                    let handback = crate::agent::AgentRunner::run_single(
                        workspace_root,
                        task,
                        provider,
                    ).await;

                    Ok(handback.format_handback_card())
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[tokio::test]
    async fn test_tool_manager_file_ops_and_sandbox() {
        let temp_dir =
            std::env::temp_dir().join(format!("nexus_tool_test_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&temp_dir).unwrap();

        let sandbox = Sandbox::new(&temp_dir, false).unwrap();
        let manager = ToolManager::new(sandbox);

        // 1. Write file
        let write_call = ToolCall::new(
            "call_write",
            "write_file",
            serde_json::json!({
                "path": "test.txt",
                "content": "Line 1\nLine 2\nLine 3"
            }),
        );
        let res = manager.execute(&write_call).await;
        assert!(!res.is_error);

        // 2. Read file
        let read_call = ToolCall::new(
            "call_read",
            "read_file",
            serde_json::json!({
                "path": "test.txt",
                "start_line": 2,
                "end_line": 2
            }),
        );
        let res = manager.execute(&read_call).await;
        assert!(!res.is_error);
        assert!(res.output.contains("Line 2"));

        // 3. Edit file (exact match)
        let edit_call = ToolCall::new(
            "call_edit",
            "edit_file",
            serde_json::json!({
                "path": "test.txt",
                "target": "Line 2",
                "replacement": "Line Two Updated"
            }),
        );
        let res = manager.execute(&edit_call).await;
        assert!(!res.is_error);

        // 4. Verify updated content
        let read_updated = manager
            .execute(&ToolCall::new(
                "call_read_2",
                "read_file",
                serde_json::json!({ "path": "test.txt" }),
            ))
            .await;
        assert!(read_updated.output.contains("Line Two Updated"));

        // 5. Test path traversal block
        let traversal_call = ToolCall::new(
            "call_traversal",
            "write_file",
            serde_json::json!({
                "path": "../../outside.txt",
                "content": "hacked"
            }),
        );
        let res = manager.execute(&traversal_call).await;
        assert!(res.is_error);
        assert!(res.output.contains("Path traversal detected"));

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[tokio::test]
    async fn test_cargo_check_tool_execution() {
        // Run cargo check on an isolated temporary project to avoid cargo target lock contention
        let temp_dir = std::env::temp_dir().join(format!("nexus_check_tool_test_{}", uuid::Uuid::new_v4()));
        let _ = std::fs::create_dir_all(temp_dir.join("src"));
        let _ = std::fs::write(
            temp_dir.join("Cargo.toml"),
            "[package]\nname = \"tool_test_proj\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        );
        let _ = std::fs::write(
            temp_dir.join("src/main.rs"),
            "fn main() { println!(\"hello\"); }\n",
        );

        let sandbox = Sandbox::new(&temp_dir, false).unwrap();
        let manager = ToolManager::new(sandbox);

        let check_call = ToolCall::new(
            "call_check",
            "cargo_check",
            serde_json::json!({ "mode": "check" }),
        );

        let res = manager.execute(&check_call).await;
        assert!(!res.is_error);
        assert!(res.output.contains("Rust Check Passed") || res.output.contains("Diagnostic"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[tokio::test]
    async fn test_lsp_tools_graceful_fallback() {
        let temp_dir =
            std::env::temp_dir().join(format!("nexus_lsp_test_{}", uuid::Uuid::new_v4()));
        let sandbox = Sandbox::new(&temp_dir, false).unwrap();
        let manager = ToolManager::new(sandbox); // No LSP client attached

        let hover_call = ToolCall::new(
            "call_hover",
            "lsp_hover",
            serde_json::json!({
                "path": "src/main.rs",
                "line": 1,
                "column": 1
            }),
        );

        let res = manager.execute(&hover_call).await;
        assert!(res.is_error);
        assert!(res.output.contains("rust-analyzer is not active"));
    }
}
