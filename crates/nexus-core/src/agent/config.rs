use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentRole {
    Scout,
    Planner,
    Reviewer,
    Worker,
}

impl Default for AgentRole {
    fn default() -> Self {
        Self::Worker
    }
}

impl std::fmt::Display for AgentRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Scout => write!(f, "scout"),
            Self::Planner => write!(f, "planner"),
            Self::Reviewer => write!(f, "reviewer"),
            Self::Worker => write!(f, "worker"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentSource {
    Builtin,
    User,
    Project,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub name: String,
    pub role: AgentRole,
    pub description: String,
    pub system_prompt: String,
    pub model: Option<String>,
    pub tools: Vec<String>,
    pub source: AgentSource,
    pub max_turns: usize,
}

impl AgentConfig {
    pub fn is_read_only(&self) -> bool {
        matches!(
            self.role,
            AgentRole::Scout | AgentRole::Planner | AgentRole::Reviewer
        )
    }

    pub fn allows_tool(&self, tool_name: &str) -> bool {
        // Subagents are never permitted to invoke subagents (prevent fork bombs)
        if tool_name == "invoke_subagent" {
            return false;
        }

        if self.tools.is_empty() {
            // Default full tools for Worker, read-only for Scout/Planner/Reviewer
            if self.is_read_only() {
                return matches!(
                    tool_name,
                    "read_file"
                        | "search_memory"
                        | "list_agents"
                        | "cargo_check"
                        | "lsp_hover"
                        | "lsp_definition"
                        | "lsp_references"
                );
            }
            return true;
        }
        self.tools.iter().any(|t| t == tool_name)
    }

    pub fn builtin_scout() -> Self {
        Self {
            name: "scout".to_string(),
            role: AgentRole::Scout,
            description: "Fast codebase reconnaissance with read-only exploration tools".to_string(),
            system_prompt: "You are Scout, an elite read-only reconnaissance subagent. Your mission is to rapidly explore the repository, inspect file structures, locate relevant modules, and return a high-density, accurate technical summary. You cannot modify files or run destructive commands. Return only distilled facts and exact file paths.".to_string(),
            model: None,
            tools: vec![
                "read_file".to_string(),
                "list_agents".to_string(),
                "search_memory".to_string(),
                "lsp_hover".to_string(),
                "lsp_definition".to_string(),
                "lsp_references".to_string(),
            ],
            source: AgentSource::Builtin,
            max_turns: 6,
        }
    }

    pub fn builtin_planner() -> Self {
        Self {
            name: "planner".to_string(),
            role: AgentRole::Planner,
            description: "Architectural planning, interface design, and step-by-step roadmaps".to_string(),
            system_prompt: "You are Planner, an architectural design subagent. Your mission is to analyze the codebase structure, review interface contracts and data models, and synthesize an actionable, phased implementation roadmap. Do not make edits yourself; produce clean, structured implementation specifications.".to_string(),
            model: None,
            tools: vec![
                "read_file".to_string(),
                "list_agents".to_string(),
                "search_memory".to_string(),
                "lsp_hover".to_string(),
                "lsp_definition".to_string(),
                "lsp_references".to_string(),
            ],
            source: AgentSource::Builtin,
            max_turns: 6,
        }
    }

    pub fn builtin_reviewer() -> Self {
        Self {
            name: "reviewer".to_string(),
            role: AgentRole::Reviewer,
            description: "Code review, regression verification, and compiler diagnostics".to_string(),
            system_prompt: "You are Reviewer, a quality and safety verification subagent. Your mission is to audit proposed changes, verify compiler soundness via cargo_check, spot borrow-checker and concurrency hazards, and check test coverage. Report issues concisely with line locations and tutor suggestions.".to_string(),
            model: None,
            tools: vec![
                "read_file".to_string(),
                "list_agents".to_string(),
                "cargo_check".to_string(),
                "search_memory".to_string(),
            ],
            source: AgentSource::Builtin,
            max_turns: 6,
        }
    }

    pub fn builtin_worker() -> Self {
        Self {
            name: "worker".to_string(),
            role: AgentRole::Worker,
            description: "General-purpose implementation agent with full read-write tool suite".to_string(),
            system_prompt: "You are Worker, an elite autonomous implementation subagent. Your mission is to implement, refactor, and fix code with precision edits. Always inspect files before editing, write complete production-grade code (never placeholders or // TODO stubs), and verify your changes with cargo_check or test commands.".to_string(),
            model: None,
            tools: vec![
                "read_file".to_string(),
                "write_file".to_string(),
                "edit_file".to_string(),
                "cargo_check".to_string(),
                "run_command".to_string(),
                "search_memory".to_string(),
                "distill_memory".to_string(),
            ],
            source: AgentSource::Builtin,
            max_turns: 8,
        }
    }
}

pub struct AgentDiscovery;

impl AgentDiscovery {
    /// Loads all available agents: Builtins, overridden by User (~/.nexus/agents/*.md),
    /// overridden by Project (<workspace>/.nexus/agents/*.md).
    pub fn discover_agents(workspace_dir: &Path) -> Vec<AgentConfig> {
        let mut map: HashMap<String, AgentConfig> = HashMap::new();

        // 1. Built-in defaults
        let builtins = vec![
            AgentConfig::builtin_scout(),
            AgentConfig::builtin_planner(),
            AgentConfig::builtin_reviewer(),
            AgentConfig::builtin_worker(),
        ];
        for b in builtins {
            map.insert(b.name.clone(), b);
        }

        // 2. User-level agents (~/.nexus/agents/*.md)
        if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
            let user_agents_dir = home.join(".nexus").join("agents");
            Self::scan_markdown_dir(&user_agents_dir, AgentSource::User, &mut map);
        }

        // 3. Project-level agents (<workspace>/.nexus/agents/*.md)
        let project_agents_dir = workspace_dir.join(".nexus").join("agents");
        Self::scan_markdown_dir(&project_agents_dir, AgentSource::Project, &mut map);

        let mut configs: Vec<AgentConfig> = map.into_values().collect();
        configs.sort_by(|a, b| a.name.cmp(&b.name));
        configs
    }

    /// Finds an agent by name
    pub fn find_agent(workspace_dir: &Path, name: &str) -> Option<AgentConfig> {
        Self::discover_agents(workspace_dir)
            .into_iter()
            .find(|a| a.name.eq_ignore_ascii_case(name))
    }

    fn scan_markdown_dir(
        dir: &Path,
        source: AgentSource,
        map: &mut HashMap<String, AgentConfig>,
    ) {
        if !dir.exists() || !dir.is_dir() {
            return;
        }

        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("md") {
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        if let Some(config) = Self::parse_markdown_agent(&content, source) {
                            map.insert(config.name.clone(), config);
                        }
                    }
                }
            }
        }
    }

    /// Parses YAML frontmatter and markdown body
    pub fn parse_markdown_agent(content: &str, source: AgentSource) -> Option<AgentConfig> {
        let trimmed = content.trim_start();
        if !trimmed.starts_with("---") {
            return None;
        }

        let after_first = &trimmed[3..];
        let end_idx = after_first.find("---")?;
        let frontmatter = &after_first[..end_idx];
        let body = after_first[end_idx + 3..].trim();

        let mut name = String::new();
        let mut role = AgentRole::Worker;
        let mut description = String::new();
        let mut model = None;
        let mut tools = Vec::new();
        let mut max_turns = 6;
        let mut in_tools_list = false;

        for raw_line in frontmatter.lines() {
            let line = raw_line.trim();
            if line.is_empty() {
                continue;
            }

            if in_tools_list {
                if let Some(item) = line.strip_prefix('-') {
                    let tool = item.trim().trim_matches('"').trim_matches('\'').to_string();
                    if !tool.is_empty() {
                        tools.push(tool);
                    }
                    continue;
                } else if !line.contains(':') {
                    continue;
                } else {
                    in_tools_list = false;
                }
            }

            if let Some((k, v)) = line.split_once(':') {
                let key = k.trim().to_lowercase();
                let val = v.trim().trim_matches('"').trim_matches('\'');
                match key.as_str() {
                    "name" => name = val.to_string(),
                    "role" => {
                        role = match val.to_lowercase().as_str() {
                            "scout" => AgentRole::Scout,
                            "planner" => AgentRole::Planner,
                            "reviewer" => AgentRole::Reviewer,
                            _ => AgentRole::Worker,
                        };
                    }
                    "description" => description = val.to_string(),
                    "model" => {
                        if !val.is_empty() {
                            model = Some(val.to_string());
                        }
                    }
                    "tools" => {
                        let cleaned = val.trim_start_matches('[').trim_end_matches(']');
                        if cleaned.is_empty() {
                            in_tools_list = true;
                        } else {
                            tools = cleaned
                                .split(',')
                                .map(|s| s.trim().trim_matches('"').trim_matches('\'').to_string())
                                .filter(|s| !s.is_empty())
                                .collect();
                        }
                    }
                    "max_turns" => {
                        if let Ok(n) = val.parse::<usize>() {
                            max_turns = n;
                        }
                    }
                    _ => {}
                }
            }
        }

        if name.is_empty() {
            return None;
        }

        Some(AgentConfig {
            name,
            role,
            description,
            system_prompt: body.to_string(),
            model,
            tools,
            source,
            max_turns,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtin_agents() {
        let scout = AgentConfig::builtin_scout();
        assert_eq!(scout.name, "scout");
        assert!(scout.is_read_only());
        assert!(scout.allows_tool("read_file"));
        assert!(!scout.allows_tool("edit_file"));

        let worker = AgentConfig::builtin_worker();
        assert_eq!(worker.name, "worker");
        assert!(!worker.is_read_only());
        assert!(worker.allows_tool("edit_file"));
    }

    #[test]
    fn test_parse_markdown_agent() {
        let md = r#"---
name: custom-sec-auditor
role: reviewer
description: Audits unsafe blocks and CVE vulnerabilities
model: claude-3-7-sonnet
tools: read_file, cargo_check
max_turns: 10
---
You are a security auditor. Inspect all unsafe blocks and report CVE risks.
"#;
        let parsed = AgentDiscovery::parse_markdown_agent(md, AgentSource::Project).unwrap();
        assert_eq!(parsed.name, "custom-sec-auditor");
        assert_eq!(parsed.role, AgentRole::Reviewer);
        assert_eq!(parsed.description, "Audits unsafe blocks and CVE vulnerabilities");
        assert_eq!(parsed.model, Some("claude-3-7-sonnet".to_string()));
        assert_eq!(parsed.tools, vec!["read_file", "cargo_check"]);
        assert_eq!(parsed.max_turns, 10);
        assert!(parsed.system_prompt.contains("You are a security auditor"));
    }
}
