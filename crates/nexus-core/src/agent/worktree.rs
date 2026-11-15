use std::path::{Path, PathBuf};
use std::process::Command;

pub struct GitWorktreeManager;

impl GitWorktreeManager {
    /// Creates an isolated git worktree for a subagent.
    /// If not a git repo, gracefully falls back to an isolated temporary directory.
    pub fn create_worktree(repo_dir: &Path, agent_id: &str) -> Result<(PathBuf, Option<String>), String> {
        let worktree_dir = repo_dir.join(".nexus").join("worktrees").join(agent_id);
        let branch_name = format!("nexus-agent-{}", agent_id);

        // Check if repo_dir is actually a git repository
        let is_git = Command::new("git")
            .args(["rev-parse", "--is-inside-work-tree"])
            .current_dir(repo_dir)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

        if !is_git {
            let temp_dir = std::env::temp_dir().join(format!("nexus_subagent_{}", agent_id));
            std::fs::create_dir_all(&temp_dir)
                .map_err(|e| format!("Failed to create temporary sandbox: {}", e))?;
            return Ok((temp_dir, None));
        }

        // Clean up any stale directory if it exists
        if worktree_dir.exists() {
            let _ = std::fs::remove_dir_all(&worktree_dir);
        }

        let output = Command::new("git")
            .args([
                "worktree",
                "add",
                "-f",
                worktree_dir.to_str().unwrap_or(""),
                "-b",
                &branch_name,
            ])
            .current_dir(repo_dir)
            .output()
            .map_err(|e| format!("Failed to invoke git worktree: {}", e))?;

        if !output.status.success() {
            // If worktree add fails (e.g. branch already exists), fallback to non-worktree temp copy
            let temp_dir = std::env::temp_dir().join(format!("nexus_subagent_{}", agent_id));
            std::fs::create_dir_all(&temp_dir)
                .map_err(|e| format!("Failed to create fallback sandbox: {}", e))?;
            return Ok((temp_dir, None));
        }

        Ok((worktree_dir, Some(branch_name)))
    }

    /// Cleans up and deletes a temporary git worktree and its ephemeral branch.
    pub fn cleanup_worktree(
        repo_dir: &Path,
        worktree_dir: &Path,
        branch_name: Option<&str>,
    ) -> Result<(), String> {
        if let Some(branch) = branch_name {
            let _ = Command::new("git")
                .args(["worktree", "remove", "--force", worktree_dir.to_str().unwrap_or("")])
                .current_dir(repo_dir)
                .output();

            let _ = Command::new("git")
                .args(["branch", "-D", branch])
                .current_dir(repo_dir)
                .output();
        }

        if worktree_dir.exists() {
            let _ = std::fs::remove_dir_all(worktree_dir);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_worktree_fallback_in_temp() {
        let temp_dir = std::env::temp_dir().join(format!("nexus_wt_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let (wt_path, branch) = GitWorktreeManager::create_worktree(&temp_dir, "test-agent").unwrap();
        assert!(wt_path.exists());
        assert!(branch.is_none()); // non-git fallback

        GitWorktreeManager::cleanup_worktree(&temp_dir, &wt_path, branch.as_deref()).unwrap();
        assert!(!wt_path.exists());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
