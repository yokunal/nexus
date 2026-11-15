use super::config::AgentRole;
use crate::events::TokenUsage;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SubagentStatus {
    Success,
    Failed(String),
    Interrupted,
}

impl std::fmt::Display for SubagentStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Success => write!(f, "success"),
            Self::Failed(err) => write!(f, "failed: {}", err),
            Self::Interrupted => write!(f, "interrupted"),
        }
    }
}

/// Structured handback contract returned by a subagent upon task completion.
/// Inspired by Claude Code's SubagentHandback and Pi's SingleResult.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubagentHandback {
    pub agent_name: String,
    pub role: AgentRole,
    pub task: String,
    pub status: SubagentStatus,
    pub distilled_output: String,
    pub tools_used: Vec<String>,
    pub turns_taken: usize,
    pub token_usage: TokenUsage,
    pub duration_ms: u64,
}

impl SubagentHandback {
    /// Formats the handback as a clean XML/Markdown briefing for the parent coordinator.
    /// This prevents intermediate tool spam from entering the coordinator's context window.
    pub fn format_handback_card(&self) -> String {
        let status_str = match &self.status {
            SubagentStatus::Success => "✓ COMPLETED",
            SubagentStatus::Failed(e) => &format!("✗ FAILED: {}", e),
            SubagentStatus::Interrupted => "⏸ INTERRUPTED",
        };

        let tools_str = if self.tools_used.is_empty() {
            "none".to_string()
        } else {
            self.tools_used.join(", ")
        };

        format!(
            "<subagent_handback agent=\"{}\" role=\"{}\" status=\"{}\">\n\
             ## Task Objective\n{}\n\n\
             ## Distilled Output & Findings\n{}\n\n\
             <!-- Subagent Metrics: {} turns, {} in / {} out tokens, tools: [{}], duration: {}ms -->\n\
             </subagent_handback>",
            self.agent_name,
            self.role,
            status_str,
            self.task,
            self.distilled_output.trim(),
            self.turns_taken,
            self.token_usage.input_tokens,
            self.token_usage.output_tokens,
            tools_str,
            self.duration_ms
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_handback_card() {
        let handback = SubagentHandback {
            agent_name: "scout".to_string(),
            role: AgentRole::Scout,
            task: "Find all database connections in crates/nexus-core".to_string(),
            status: SubagentStatus::Success,
            distilled_output: "Database connections found in crates/nexus-core/src/session.rs using rusqlite::Connection.".to_string(),
            tools_used: vec!["read_file".to_string()],
            turns_taken: 2,
            token_usage: TokenUsage {
                input_tokens: 420,
                output_tokens: 85,
                cache_read_tokens: 0,
                cache_creation_tokens: 0,
            },
            duration_ms: 125,
        };

        let card = handback.format_handback_card();
        assert!(card.contains("<subagent_handback agent=\"scout\""));
        assert!(card.contains("✓ COMPLETED"));
        assert!(card.contains("Database connections found in crates/nexus-core/src/session.rs"));
        assert!(card.contains("tools: [read_file]"));
    }
}
