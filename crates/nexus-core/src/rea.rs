use std::path::{Path, PathBuf};
use std::process::Command;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ReaError {
    #[error("REA CLI not found on host. REA is an opt-in external reverse-engineering plugin.")]
    ReaNotInstalled,
    #[error("REA command failed: {0}")]
    CommandFailed(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub struct ReaBridge;

impl ReaBridge {
    /// Detects if `rea` binary is available on the system.
    pub fn detect_rea() -> Option<PathBuf> {
        let candidates = [
            "/home/ubuntu/.local/bin/rea",
            "/usr/local/bin/rea",
            "/usr/bin/rea",
        ];

        for path in &candidates {
            let p = PathBuf::from(path);
            if p.exists() {
                return Some(p);
            }
        }

        // Try `which rea`
        if let Ok(output) = Command::new("which").arg("rea").output() {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let trimmed = stdout.trim();
                if !trimmed.is_empty() {
                    return Some(PathBuf::from(trimmed));
                }
            }
        }

        None
    }

    pub fn is_available() -> bool {
        Self::detect_rea().is_some()
    }

    /// Runs REA inspection on a target binary.
    /// Preserves the sub-10MB core binary footprint by executing REA as an external process.
    pub fn inspect_binary(target_path: &Path, mode: &str) -> Result<String, ReaError> {
        let rea_path = Self::detect_rea().ok_or(ReaError::ReaNotInstalled)?;

        let mut cmd = Command::new(rea_path);
        match mode {
            "version" | "--version" => {
                cmd.arg("--version");
            }
            "overview" => {
                cmd.arg("binary-overview").arg(target_path);
            }
            "objdump" => {
                cmd.arg("inspect-with-objdump").arg(target_path);
            }
            _ => {
                cmd.arg("--help");
            }
        }

        let output = cmd.output()?;
        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            Err(ReaError::CommandFailed(format!("{}\n{}", stdout, stderr)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rea_detection() {
        // REA is installed at /home/ubuntu/.local/bin/rea
        assert!(ReaBridge::is_available());
        let path = ReaBridge::detect_rea().unwrap();
        assert!(path.exists());
    }

    #[test]
    fn test_rea_version_check() {
        let version_out = ReaBridge::inspect_binary(Path::new("dummy"), "version").unwrap();
        assert!(version_out.contains("6.7") || version_out.contains("rea"));
    }
}
