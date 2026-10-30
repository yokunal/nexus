use std::path::Path;
use std::process::Stdio;
use std::time::Duration;
use thiserror::Error;
use tokio::process::Command;
use tokio::time::timeout;

#[derive(Error, Debug)]
pub enum CommandError {
    #[error("Command execution timed out after {0:?}")]
    Timeout(Duration),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CommandOutput {
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

pub async fn execute_command(
    command_line: &str,
    working_dir: &Path,
    timeout_duration: Duration,
) -> Result<CommandOutput, CommandError> {
    let mut cmd = Command::new("bash");
    cmd.arg("-c")
        .arg(command_line)
        .current_dir(working_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let child = cmd.spawn()?;

    let output = match timeout(timeout_duration, child.wait_with_output()).await {
        Ok(res) => res?,
        Err(_) => return Err(CommandError::Timeout(timeout_duration)),
    };

    Ok(CommandOutput {
        exit_code: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    })
}
