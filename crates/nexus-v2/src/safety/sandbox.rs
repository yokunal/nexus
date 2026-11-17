use std::path::{Path, PathBuf};
use crate::errors::NexusV2Error;
use crate::traits::SafetyFilter;

pub struct Sandbox {
    workspace_root: PathBuf,
}

impl Sandbox {
    pub fn new(workspace_root: impl AsRef<Path>) -> Self {
        Self {
            workspace_root: std::fs::canonicalize(workspace_root.as_ref())
                .unwrap_or_else(|_| workspace_root.as_ref().to_path_buf()),
        }
    }

    pub fn resolve_path(&self, relative_path: impl AsRef<Path>) -> Result<PathBuf, NexusV2Error> {
        let p = relative_path.as_ref();
        let path_str = p.to_string_lossy();

        if path_str.contains("..") || path_str.starts_with('/') {
            return Err(NexusV2Error::PathEscape(path_str.to_string()));
        }

        let candidate = self.workspace_root.join(p);
        Ok(candidate)
    }
}

impl SafetyFilter for Sandbox {
    fn validate_patch(&self, target_path: &Path, original: &str, patch: &str) -> Result<(), NexusV2Error> {
        let _resolved = self.resolve_path(target_path)?;
        if patch.is_empty() {
            return Err(NexusV2Error::SafetyViolation("Patch cannot be empty".to_string()));
        }
        // Run patch through dry-run validation
        super::patch_engine::PatchEngine::apply_patch(original, patch, "")?;
        Ok(())
    }

    fn validate_command(&self, cmd: &str) -> Result<(), NexusV2Error> {
        let trimmed = cmd.trim();

        let blacklist = [
            "rm -rf /",
            "rm -rf /*",
            ":(){ :|:& };:",
            ":(){",
            "mkfs.",
            "dd if=/dev/zero",
            "chmod -R 777 /",
            "> /dev/sda",
        ];

        for pattern in &blacklist {
            if trimmed.contains(pattern) {
                return Err(NexusV2Error::SafetyViolation(format!(
                    "Forbidden destructive command pattern detected: '{}'",
                    pattern
                )));
            }
        }

        Ok(())
    }
}
