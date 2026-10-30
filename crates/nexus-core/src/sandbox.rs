use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum SandboxError {
    #[error("Path traversal detected: {path} is outside the workspace {workspace}")]
    PathTraversal { path: String, workspace: String },
    #[error("File not found: {0}")]
    NotFound(String),
    #[error("Permission denied: {0}")]
    PermissionDenied(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Security sandbox that confines all file operations to the designated workspace.
#[derive(Clone, Debug)]
pub struct Sandbox {
    workspace_root: PathBuf,
    allow_unrestricted_shell: bool,
}

impl Sandbox {
    pub fn new(workspace_root: impl Into<PathBuf>, allow_unrestricted_shell: bool) -> Result<Self, SandboxError> {
        let root = workspace_root.into();
        let canonical_root = root.canonicalize().unwrap_or(root);
        Ok(Self {
            workspace_root: canonical_root,
            allow_unrestricted_shell,
        })
    }

    pub fn workspace_root(&self) -> &Path {
        &self.workspace_root
    }

    pub fn is_unrestricted_shell_allowed(&self) -> bool {
        self.allow_unrestricted_shell
    }

    /// Resolves and validates that a relative or absolute path stays strictly inside workspace root.
    pub fn resolve_path(&self, requested: impl AsRef<Path>) -> Result<PathBuf, SandboxError> {
        let req = requested.as_ref();
        let target = if req.is_absolute() {
            req.to_path_buf()
        } else {
            self.workspace_root.join(req)
        };

        // Normalize path
        let mut normalized = PathBuf::new();
        for component in target.components() {
            match component {
                std::path::Component::Prefix(p) => normalized.push(p.as_os_str()),
                std::path::Component::RootDir => normalized.push("/"),
                std::path::Component::CurDir => {}
                std::path::Component::ParentDir => {
                    normalized.pop();
                }
                std::path::Component::Normal(c) => normalized.push(c),
            }
        }

        // Check if normalized path starts with workspace root
        if !normalized.starts_with(&self.workspace_root) {
            return Err(SandboxError::PathTraversal {
                path: req.display().to_string(),
                workspace: self.workspace_root.display().to_string(),
            });
        }

        Ok(normalized)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_sandbox_path_traversal() {
        let temp_dir = std::env::temp_dir().join("nexus_sandbox_test");
        let _ = fs::create_dir_all(&temp_dir);
        let sandbox = Sandbox::new(&temp_dir, false).unwrap();

        // Safe path inside workspace
        assert!(sandbox.resolve_path("sub/file.txt").is_ok());

        // Traversal attempt outside workspace
        assert!(sandbox.resolve_path("../../etc/passwd").is_err());
        assert!(sandbox.resolve_path("/etc/shadow").is_err());
    }
}
