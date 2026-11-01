pub mod watcher;

pub use watcher::{SkillChangeEvent, SkillWatcher};

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum SkillError {
    #[error("Skill '{0}' not found in staging or permanent storage")]
    NotFound(String),
    #[error("Skill name '{0}' is invalid (use lowercase alphanumeric and hyphens)")]
    InvalidName(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Watch error: {0}")]
    Watch(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub instructions: String,
    pub path: PathBuf,
    pub is_staged: bool,
}

pub struct SkillManager;

impl SkillManager {
    /// Validates skill name to kebab-case letters, numbers, and hyphens.
    pub fn is_valid_name(name: &str) -> bool {
        !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    }

    /// Returns the permanent skills directory: `<workspace>/.nexus/skills`
    pub fn permanent_dir(workspace: &Path) -> PathBuf {
        workspace.join(".nexus").join("skills")
    }

    /// Returns the staging skills directory: `<workspace>/.nexus/staging/skills`
    pub fn staging_dir(workspace: &Path) -> PathBuf {
        workspace.join(".nexus").join("staging").join("skills")
    }

    /// Scans both staging and permanent storage, returning all loaded skills.
    pub fn load_all(workspace: &Path) -> Result<Vec<Skill>, SkillError> {
        let mut skills = Vec::new();

        // 1. Permanent skills
        let perm_dir = Self::permanent_dir(workspace);
        if perm_dir.exists() {
            Self::scan_directory(&perm_dir, false, &mut skills)?;
        }

        // 2. Staging skills
        let stage_dir = Self::staging_dir(workspace);
        if stage_dir.exists() {
            Self::scan_directory(&stage_dir, true, &mut skills)?;
        }

        Ok(skills)
    }

    fn scan_directory(
        dir: &Path,
        is_staged: bool,
        skills: &mut Vec<Skill>,
    ) -> Result<(), SkillError> {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                let skill_md = path.join("SKILL.md");
                if skill_md.exists() {
                    if let Ok(skill) = Self::parse_skill_file(&skill_md, is_staged) {
                        skills.push(skill);
                    }
                }
            }
        }
        Ok(())
    }

    /// Parses a `SKILL.md` file with optional YAML frontmatter.
    pub fn parse_skill_file(path: &Path, is_staged: bool) -> Result<Skill, SkillError> {
        let content = fs::read_to_string(path)?;
        let mut name = path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();
        let mut description = "Specialized AI skill".to_string();
        let mut instructions = content.clone();

        if content.starts_with("---") {
            let parts: Vec<&str> = content.splitn(3, "---").collect();
            if parts.len() >= 3 {
                let frontmatter = parts[1];
                instructions = parts[2].trim().to_string();

                for line in frontmatter.lines() {
                    let trimmed = line.trim();
                    if let Some(rest) = trimmed.strip_prefix("name:") {
                        name = rest.trim().trim_matches('"').to_string();
                    } else if let Some(rest) = trimmed.strip_prefix("description:") {
                        description = rest.trim().trim_matches('"').to_string();
                    }
                }
            }
        }

        Ok(Skill {
            name,
            description,
            instructions,
            path: path.to_path_buf(),
            is_staged,
        })
    }

    /// Synthesizes a new skill into the staging quarantine sandbox.
    pub fn synthesize_skill(
        workspace: &Path,
        name: &str,
        description: &str,
        instructions: &str,
    ) -> Result<Skill, SkillError> {
        if !Self::is_valid_name(name) {
            return Err(SkillError::InvalidName(name.to_string()));
        }

        let stage_skill_dir = Self::staging_dir(workspace).join(name);
        fs::create_dir_all(&stage_skill_dir)?;

        let skill_file = stage_skill_dir.join("SKILL.md");
        let content = format!(
            "---\nname: \"{}\"\ndescription: \"{}\"\n---\n\n# {}\n\n{}\n",
            name, description, name, instructions
        );

        fs::write(&skill_file, content)?;

        Ok(Skill {
            name: name.to_string(),
            description: description.to_string(),
            instructions: instructions.to_string(),
            path: skill_file,
            is_staged: true,
        })
    }

    /// Promotes a staged skill into permanent storage (.nexus/skills/<name>/).
    pub fn promote_skill(workspace: &Path, name: &str) -> Result<(), SkillError> {
        let stage_skill_dir = Self::staging_dir(workspace).join(name);
        if !stage_skill_dir.exists() {
            return Err(SkillError::NotFound(name.to_string()));
        }

        let perm_skill_dir = Self::permanent_dir(workspace).join(name);
        fs::create_dir_all(Self::permanent_dir(workspace))?;

        if perm_skill_dir.exists() {
            fs::remove_dir_all(&perm_skill_dir)?;
        }

        fs::rename(&stage_skill_dir, &perm_skill_dir)?;
        Ok(())
    }

    /// Discards a staged skill, cleaning up without bloat.
    pub fn discard_skill(workspace: &Path, name: &str) -> Result<(), SkillError> {
        let stage_skill_dir = Self::staging_dir(workspace).join(name);
        if !stage_skill_dir.exists() {
            return Err(SkillError::NotFound(name.to_string()));
        }

        fs::remove_dir_all(&stage_skill_dir)?;
        Ok(())
    }

    /// Formats the skills section for the LLM system prompt.
    pub fn format_skills_prompt(skills: &[Skill]) -> String {
        if skills.is_empty() {
            return String::new();
        }

        let mut prompt = String::from("## Available Specialized Skills\n");
        for s in skills {
            let status = if s.is_staged {
                "[STAGED TEST-DRIVE]"
            } else {
                "[ACTIVE]"
            };
            prompt.push_str(&format!("- **{}** {}: {}\n", s.name, status, s.description));
        }
        prompt.push_str(
            "\nYou can invoke or follow instructions from any active or staged skills when executing tasks.\n",
        );
        prompt
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_skill_lifecycle_synthesize_promote_discard() {
        let temp_dir = std::env::temp_dir().join("nexus_skill_test");
        let _ = fs::remove_dir_all(&temp_dir);
        let _ = fs::create_dir_all(&temp_dir);

        // 1. Synthesize into staging
        let skill = SkillManager::synthesize_skill(
            &temp_dir,
            "rust-benchmarker",
            "Runs criterion benchmarks and charts throughput",
            "Execute cargo bench and summarize latency distributions.",
        )
        .unwrap();

        assert_eq!(skill.name, "rust-benchmarker");
        assert!(skill.is_staged);
        assert!(skill.path.exists());

        // 2. Load all skills
        let loaded = SkillManager::load_all(&temp_dir).unwrap();
        assert_eq!(loaded.len(), 1);
        assert!(loaded[0].is_staged);

        let prompt = SkillManager::format_skills_prompt(&loaded);
        assert!(prompt.contains("rust-benchmarker"));
        assert!(prompt.contains("[STAGED TEST-DRIVE]"));

        // 3. Promote skill to permanent
        SkillManager::promote_skill(&temp_dir, "rust-benchmarker").unwrap();
        let loaded_after_promote = SkillManager::load_all(&temp_dir).unwrap();
        assert_eq!(loaded_after_promote.len(), 1);
        assert!(!loaded_after_promote[0].is_staged);

        // 4. Synthesize another skill and discard it
        SkillManager::synthesize_skill(
            &temp_dir,
            "temp-helper",
            "Temporary helper",
            "Do temp work",
        )
        .unwrap();
        assert_eq!(SkillManager::load_all(&temp_dir).unwrap().len(), 2);

        SkillManager::discard_skill(&temp_dir, "temp-helper").unwrap();
        assert_eq!(SkillManager::load_all(&temp_dir).unwrap().len(), 1);
    }
}
