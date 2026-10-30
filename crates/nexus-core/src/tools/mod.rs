pub mod edit_file;
pub mod read_file;
pub mod run_command;
pub mod write_file;

use crate::sandbox::{Sandbox, SandboxError};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use thiserror::Error;

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
}

impl ToolManager {
    pub fn new(sandbox: Sandbox) -> Self {
        Self { sandbox }
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
        ]
    }

    /// Dispatches and executes a tool call safely within the sandbox.
    pub async fn execute(&self, call: &ToolCall) -> ToolResult {
        let res = match call.name.as_str() {
            "read_file" => self.handle_read(call).await,
            "write_file" => self.handle_write(call).await,
            "edit_file" => self.handle_edit(call).await,
            "run_command" => self.handle_command(call).await,
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

        edit_file::safe_replace(&full_path, target, replacement)?;
        Ok(format!("Successfully edited {}", path_str))
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
        let write_call = ToolCall {
            id: "call_write".to_string(),
            name: "write_file".to_string(),
            arguments: serde_json::json!({
                "path": "test.txt",
                "content": "Line 1\nLine 2\nLine 3"
            }),
        };
        let res = manager.execute(&write_call).await;
        assert!(!res.is_error);

        // 2. Read file
        let read_call = ToolCall {
            id: "call_read".to_string(),
            name: "read_file".to_string(),
            arguments: serde_json::json!({
                "path": "test.txt",
                "start_line": 2,
                "end_line": 2
            }),
        };
        let res = manager.execute(&read_call).await;
        assert!(!res.is_error);
        assert!(res.output.contains("Line 2"));

        // 3. Edit file (exact match)
        let edit_call = ToolCall {
            id: "call_edit".to_string(),
            name: "edit_file".to_string(),
            arguments: serde_json::json!({
                "path": "test.txt",
                "target": "Line 2",
                "replacement": "Line Two Updated"
            }),
        };
        let res = manager.execute(&edit_call).await;
        assert!(!res.is_error);

        // 4. Verify updated content
        let read_updated = manager
            .execute(&ToolCall {
                id: "call_read_2".to_string(),
                name: "read_file".to_string(),
                arguments: serde_json::json!({ "path": "test.txt" }),
            })
            .await;
        assert!(read_updated.output.contains("Line Two Updated"));

        // 5. Test path traversal block
        let traversal_call = ToolCall {
            id: "call_traversal".to_string(),
            name: "write_file".to_string(),
            arguments: serde_json::json!({
                "path": "../../outside.txt",
                "content": "hacked"
            }),
        };
        let res = manager.execute(&traversal_call).await;
        assert!(res.is_error);
        assert!(res.output.contains("Path traversal detected"));

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
