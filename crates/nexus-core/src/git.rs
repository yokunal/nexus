use std::path::Path;
use std::process::Command;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum GitError {
    #[error("Git command failed: {0}")]
    CommandFailed(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TurnSnapshot {
    pub turn_id: String,
    pub commit_or_stash_sha: String,
    pub had_uncommitted_changes: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub struct GitSnapshotManager;

impl GitSnapshotManager {
    /// Checks whether the given workspace path is inside an active git repository.
    pub fn is_git_repo(workspace: &Path) -> bool {
        Command::new("git")
            .arg("rev-parse")
            .arg("--is-inside-work-tree")
            .current_dir(workspace)
            .output()
            .map(|out| out.status.success())
            .unwrap_or(false)
    }

    /// Checks if the working tree has uncommitted modifications or untracked files.
    pub fn has_uncommitted_changes(workspace: &Path) -> Result<bool, GitError> {
        let output = Command::new("git")
            .arg("status")
            .arg("--porcelain")
            .current_dir(workspace)
            .output()?;

        if !output.status.success() {
            return Err(GitError::CommandFailed(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(!stdout.trim().is_empty())
    }

    /// Creates a non-destructive turn snapshot before an agent turn executes tools.
    /// Uses `git stash create` or `HEAD` SHA.
    ///
    /// Compass Rule: 1 snapshot per agent turn (not per patch), zero cost on non-git repos.
    pub fn create_turn_snapshot(
        workspace: &Path,
        turn_id: &str,
    ) -> Result<Option<TurnSnapshot>, GitError> {
        if !Self::is_git_repo(workspace) {
            tracing::warn!("Workspace is not a git repository. Turn snapshots disabled.");
            return Ok(None);
        }

        let is_dirty = Self::has_uncommitted_changes(workspace)?;
        if is_dirty {
            tracing::warn!(
                "Working tree has uncommitted changes prior to turn '{}'.",
                turn_id
            );
        }

        // Try `git stash create` which generates a commit object representing
        // the current worktree state without modifying HEAD or index.
        let stash_output = Command::new("git")
            .arg("stash")
            .arg("create")
            .arg(format!("nexus-turn-{}", turn_id))
            .current_dir(workspace)
            .output()?;

        let stash_sha = String::from_utf8_lossy(&stash_output.stdout)
            .trim()
            .to_string();

        let ref_sha = if !stash_sha.is_empty() {
            stash_sha
        } else {
            // Clean worktree: grab current HEAD commit SHA
            let head_output = Command::new("git")
                .arg("rev-parse")
                .arg("HEAD")
                .current_dir(workspace)
                .output()?;

            if !head_output.status.success() {
                return Err(GitError::CommandFailed(
                    "Failed to resolve HEAD commit".to_string(),
                ));
            }
            String::from_utf8_lossy(&head_output.stdout)
                .trim()
                .to_string()
        };

        Ok(Some(TurnSnapshot {
            turn_id: turn_id.to_string(),
            commit_or_stash_sha: ref_sha,
            had_uncommitted_changes: is_dirty,
            created_at: chrono::Utc::now(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_git_repo_detection() {
        // /home/ubuntu/nexus is an active git repo
        let nexus_dir = Path::new("/home/ubuntu/nexus");
        assert!(GitSnapshotManager::is_git_repo(nexus_dir));

        let non_git = std::env::temp_dir();
        // Temp dir may or may not be git repo, but won't crash
        let _ = GitSnapshotManager::is_git_repo(&non_git);
    }

    #[test]
    fn test_create_turn_snapshot() {
        let nexus_dir = Path::new("/home/ubuntu/nexus");
        let snapshot =
            GitSnapshotManager::create_turn_snapshot(nexus_dir, "test-turn-123").unwrap();
        assert!(snapshot.is_some());
        let snap = snapshot.unwrap();
        assert_eq!(snap.turn_id, "test-turn-123");
        assert!(!snap.commit_or_stash_sha.is_empty());
    }
}
