use std::collections::HashSet;
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
    #[error(
        "Blacklisted destructive command detected: {0}. (Defense-in-depth guard blocked execution)"
    )]
    BlockedCommand(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CommandOutput {
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

/// NOTE: Blacklist is defense-in-depth, NOT proof of absence.
/// Obfuscated shell tricks (e.g., `echo ... | base64 -d | bash`, `printf '%s'`, `$0` indirection)
/// cannot be comprehensively caught by regular expressions alone.
/// The interactive user permission prompt (Tier 3 Gate) remains the true security boundary.
pub fn check_destructive_blacklist(command_line: &str) -> Result<(), CommandError> {
    let lower = command_line.trim().to_lowercase();

    // High-risk destructive command patterns
    let dangerous_patterns = [
        "rm -rf /",
        "rm -rf /*",
        "rm -fr /",
        "mkfs",
        ":(){ :|:& };:", // forkbomb
        ":(){:|:&};:",
        "> /dev/sda",
        "> /dev/nvme",
        "dd if=/dev/zero of=/dev/",
        "dd if=/dev/urandom of=/dev/",
        "chmod -R 777 /",
        "chown -R",
    ];

    for pattern in &dangerous_patterns {
        if lower.contains(pattern) {
            return Err(CommandError::BlockedCommand(pattern.to_string()));
        }
    }

    Ok(())
}

/// Scrub sensitive authentication tokens and proxy variables from the child execution environment.
pub fn get_scrubbed_env() -> Vec<(String, String)> {
    let sensitive_keys: HashSet<&str> = [
        "ANTHROPIC_API_KEY",
        "OPENAI_API_KEY",
        "GEMINI_API_KEY",
        "AWS_ACCESS_KEY_ID",
        "AWS_SECRET_ACCESS_KEY",
        "AWS_SESSION_TOKEN",
        "GITHUB_TOKEN",
        "GH_TOKEN",
        "SSH_AUTH_SOCK",
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "http_proxy",
        "https_proxy",
        "SSL_CERT_FILE",
        "SSL_CERT_DIR",
    ]
    .iter()
    .cloned()
    .collect();

    std::env::vars()
        .filter(|(k, _)| !sensitive_keys.contains(k.as_str()))
        .collect()
}

/// Compact command outputs with intelligent head+tail truncation if output exceeds max_lines.
pub fn compact_output_stream(output: &str, max_lines: usize) -> String {
    let lines: Vec<&str> = output.lines().collect();
    if lines.len() <= max_lines {
        return output.to_string();
    }

    let half = max_lines / 2;
    let head: Vec<&str> = lines.iter().take(half).copied().collect();
    let tail: Vec<&str> = lines.iter().rev().take(half).rev().copied().collect();
    let omitted = lines.len().saturating_sub(max_lines);

    format!(
        "{}\n[... {} lines omitted for token efficiency ...]\n{}",
        head.join("\n"),
        omitted,
        tail.join("\n")
    )
}

pub async fn execute_command(
    command_line: &str,
    working_dir: &Path,
    timeout_duration: Duration,
) -> Result<CommandOutput, CommandError> {
    // Defense-in-depth blacklist check
    check_destructive_blacklist(command_line)?;

    let mut cmd = Command::new("bash");
    cmd.arg("-c")
        .arg(command_line)
        .current_dir(working_dir)
        .env_clear()
        .envs(get_scrubbed_env())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let child = cmd.spawn()?;

    let output = match timeout(timeout_duration, child.wait_with_output()).await {
        Ok(res) => res?,
        Err(_) => return Err(CommandError::Timeout(timeout_duration)),
    };

    let stdout_str = String::from_utf8_lossy(&output.stdout);
    let stderr_str = String::from_utf8_lossy(&output.stderr);

    Ok(CommandOutput {
        exit_code: output.status.code(),
        stdout: compact_output_stream(&stdout_str, 80),
        stderr: compact_output_stream(&stderr_str, 80),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_blacklist_destructive_commands() {
        assert!(check_destructive_blacklist("rm -rf /").is_err());
        assert!(check_destructive_blacklist("rm -rf /*").is_err());
        assert!(check_destructive_blacklist(":(){ :|:& };:").is_err());
        assert!(check_destructive_blacklist("dd if=/dev/zero of=/dev/sda").is_err());
        assert!(check_destructive_blacklist("cargo check").is_ok());
        assert!(check_destructive_blacklist("git status").is_ok());
    }

    #[test]
    fn test_env_scrubbing() {
        std::env::set_var("GITHUB_TOKEN", "super_secret_github");
        std::env::set_var("SSH_AUTH_SOCK", "/tmp/ssh.sock");
        std::env::set_var("HTTP_PROXY", "http://proxy.internal:8080");

        let envs = get_scrubbed_env();
        let keys: Vec<String> = envs.into_iter().map(|(k, _)| k).collect();

        assert!(!keys.contains(&"GITHUB_TOKEN".to_string()));
        assert!(!keys.contains(&"SSH_AUTH_SOCK".to_string()));
        assert!(!keys.contains(&"HTTP_PROXY".to_string()));
    }
}
