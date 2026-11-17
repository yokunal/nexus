use std::path::Path;
use crate::errors::NexusV2Error;
use crate::safety::patch_engine::PatchEngine;
use crate::safety::sandbox::Sandbox;
use crate::traits::SafetyFilter;

pub struct Actor {
    sandbox: Sandbox,
}

impl Actor {
    pub fn new(sandbox: Sandbox) -> Self {
        Self { sandbox }
    }

    pub fn execute_edit(
        &self,
        file_path: impl AsRef<Path>,
        target: &str,
        replacement: &str,
    ) -> Result<String, NexusV2Error> {
        let full_path = self.sandbox.resolve_path(file_path)?;
        let original = std::fs::read_to_string(&full_path)?;
        let updated = PatchEngine::apply_patch(&original, target, replacement)?;
        std::fs::write(&full_path, &updated)?;
        Ok(updated)
    }

    pub fn execute_command(&self, cmd: &str) -> Result<String, NexusV2Error> {
        self.sandbox.validate_command(cmd)?;

        let output = std::process::Command::new("bash")
            .arg("-c")
            .arg(cmd)
            .output()?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        if !output.status.success() {
            return Err(NexusV2Error::SafetyViolation(format!(
                "Command failed with exit code {:?}: {}",
                output.status.code(),
                stderr
            )));
        }

        Ok(stdout)
    }
}
