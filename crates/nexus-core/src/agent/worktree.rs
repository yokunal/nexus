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

    /// Cleans up a temporary git worktree. If changes or commits were made,
    /// auto-commits them and preserves the branch so work is never lost.
    /// If no changes or commits were made, safely deletes the ephemeral branch.
    pub fn cleanup_worktree(
        repo_dir: &Path,
        worktree_dir: &Path,
        branch_name: Option<&str>,
    ) -> Result<Option<String>, String> {
        let mut preserved_branch: Option<String> = None;

        if let Some(branch) = branch_name {
            // Check if there are uncommitted changes in the worktree
            let is_dirty = Command::new("git")
                .args(["status", "--porcelain"])
                .current_dir(worktree_dir)
                .output()
                .map(|o| !o.stdout.is_empty())
                .unwrap_or(false);

            if is_dirty {
                // Auto-commit any subagent work on this isolated branch
                let _ = Command::new("git")
                    .args(["add", "-A"])
                    .current_dir(worktree_dir)
                    .output();

                let _ = Command::new("git")
                    .args(["commit", "-m", &format!("nexus(subagent): work completed on {}", branch)])
                    .current_dir(worktree_dir)
                    .output();
            }

            // Check if branch has new commits compared to HEAD in repo_dir
            let has_new_commits = Command::new("git")
                .args(["rev-list", "--count", &format!("HEAD..{}", branch)])
                .current_dir(repo_dir)
                .output()
                .map(|o| {
                    let out = String::from_utf8_lossy(&o.stdout);
                    out.trim().parse::<usize>().unwrap_or(0) > 0
                })
                .unwrap_or(false);

            // Remove the worktree directory from git
            let _ = Command::new("git")
                .args(["worktree", "remove", "--force", worktree_dir.to_str().unwrap_or("")])
                .current_dir(repo_dir)
                .output();

            if has_new_commits {
                // Preserve branch so authoring subagent work is never destroyed!
                preserved_branch = Some(branch.to_string());
            } else {
                // Purely scratch/read-only: prune empty branch
                let _ = Command::new("git")
                    .args(["branch", "-D", branch])
                    .current_dir(repo_dir)
                    .output();
            }
        }

        if worktree_dir.exists() {
            let _ = std::fs::remove_dir_all(worktree_dir);
        }

        Ok(preserved_branch)
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

        let preserved = GitWorktreeManager::cleanup_worktree(&temp_dir, &wt_path, branch.as_deref()).unwrap();
        assert!(preserved.is_none());
        assert!(!wt_path.exists());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_worktree_preserves_dirty_branch_in_git_repo() {
        let repo_dir = std::env::temp_dir().join(format!("nexus_git_wt_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&repo_dir).unwrap();

        // Initialize a dummy git repo with an initial commit
        let _ = Command::new("git").args(["init"]).current_dir(&repo_dir).output();
        let _ = Command::new("git").args(["config", "user.name", "Nexus Test"]).current_dir(&repo_dir).output();
        let _ = Command::new("git").args(["config", "user.email", "test@nexus.local"]).current_dir(&repo_dir).output();
        std::fs::write(repo_dir.join("init.txt"), "initial").unwrap();
        let _ = Command::new("git").args(["add", "init.txt"]).current_dir(&repo_dir).output();
        let _ = Command::new("git").args(["commit", "-m", "init"]).current_dir(&repo_dir).output();

        let agent_id = "agent-unit-test";
        let (wt_path, branch) = GitWorktreeManager::create_worktree(&repo_dir, agent_id).unwrap();
        assert!(branch.is_some());
        let branch_name = branch.unwrap();

        // Write a change inside the worktree
        std::fs::write(wt_path.join("agent_work.txt"), "subagent produced this!").unwrap();

        // Cleanup worktree: should detect dirty tree, auto-commit, and preserve the branch
        let preserved = GitWorktreeManager::cleanup_worktree(&repo_dir, &wt_path, Some(&branch_name)).unwrap();
        assert_eq!(preserved, Some(branch_name.clone()));
        assert!(!wt_path.exists());

        // Verify the branch still exists in repo
        let branch_check = Command::new("git")
            .args(["rev-parse", "--verify", &branch_name])
            .current_dir(&repo_dir)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        assert!(branch_check, "Branch must be preserved in git repo");

        let _ = std::fs::remove_dir_all(&repo_dir);
    }
}
