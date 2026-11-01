use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use std::sync::mpsc as std_mpsc;
use std::time::Duration;
use tokio::sync::mpsc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkillChangeEvent {
    SkillModified(PathBuf),
    SkillCreated(PathBuf),
    SkillRemoved(PathBuf),
    ReloadRequired,
}

pub struct SkillWatcher {
    _watcher: RecommendedWatcher,
}

impl SkillWatcher {
    /// Starts watching workspace skill directories (.nexus/skills and .nexus/staging/skills).
    /// Emits events onto `rx` when SKILL.md changes occur.
    pub fn start(
        workspace: &Path,
        tx: mpsc::Sender<SkillChangeEvent>,
    ) -> Result<Self, notify::Error> {
        let nexus_dir = workspace.join(".nexus");
        let skills_dir = nexus_dir.join("skills");
        let staging_dir = nexus_dir.join("staging").join("skills");

        let _ = std::fs::create_dir_all(&skills_dir);
        let _ = std::fs::create_dir_all(&staging_dir);

        let (std_tx, std_rx) = std_mpsc::channel();

        let mut watcher = notify::recommended_watcher(move |res: notify::Result<Event>| {
            if let Ok(event) = res {
                let _ = std_tx.send(event);
            }
        })?;

        watcher.watch(&nexus_dir, RecursiveMode::Recursive)?;

        // Forward and filter events in background task
        tokio::spawn(async move {
            let mut last_event_time: Option<tokio::time::Instant> = None;

            loop {
                // Non-blocking poll from std_rx
                match std_rx.try_recv() {
                    Ok(event) => {
                        let is_skill_md = event.paths.iter().any(|p| {
                            p.to_string_lossy().contains("SKILL.md")
                                || p.file_name().and_then(|n| n.to_str()) == Some("SKILL.md")
                        });

                        if is_skill_md {
                            let now = tokio::time::Instant::now();
                            let should_fire = match last_event_time {
                                Some(last) => now.duration_since(last) > Duration::from_millis(50),
                                None => true,
                            };

                            if should_fire {
                                last_event_time = Some(now);
                                let change = match event.kind {
                                    EventKind::Create(_) => SkillChangeEvent::SkillCreated(
                                        event.paths.first().cloned().unwrap_or_default(),
                                    ),
                                    EventKind::Remove(_) => SkillChangeEvent::SkillRemoved(
                                        event.paths.first().cloned().unwrap_or_default(),
                                    ),
                                    EventKind::Modify(_) => SkillChangeEvent::SkillModified(
                                        event.paths.first().cloned().unwrap_or_default(),
                                    ),
                                    _ => SkillChangeEvent::ReloadRequired,
                                };
                                let _ = tx.send(change).await;
                            }
                        }
                    }
                    Err(std_mpsc::TryRecvError::Empty) => {
                        tokio::time::sleep(Duration::from_millis(20)).await;
                    }
                    Err(std_mpsc::TryRecvError::Disconnected) => {
                        break;
                    }
                }
            }
        });

        Ok(Self { _watcher: watcher })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[tokio::test]
    async fn test_watcher_detection() {
        let temp_dir = std::env::temp_dir().join("nexus_watcher_test");
        let _ = fs::remove_dir_all(&temp_dir);
        let _ = fs::create_dir_all(&temp_dir);

        let (tx, mut rx) = mpsc::channel(16);
        let _watcher = SkillWatcher::start(&temp_dir, tx).unwrap();

        // Brief delay to allow the OS file-watcher subsystem to establish directory watches
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Write a test skill file in staging
        let skill_path = temp_dir
            .join(".nexus")
            .join("staging")
            .join("skills")
            .join("sample")
            .join("SKILL.md");
        fs::create_dir_all(skill_path.parent().unwrap()).unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        fs::write(&skill_path, "---\nname: sample\n---\nTest").unwrap();

        // Wait for event
        let event = tokio::time::timeout(Duration::from_secs(3), rx.recv()).await;
        assert!(
            event.is_ok(),
            "Expected watcher to detect SKILL.md creation"
        );
    }
}
