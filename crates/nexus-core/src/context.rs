use crate::git::GitSnapshotManager;
use crate::session::Message;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Context budget tracker for managing model window limits and auto-compaction thresholds.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextBudget {
    /// Total token capacity of the model context window (default: 128,000)
    pub window_capacity: u32,
    /// Percentage threshold at which auto-compaction triggers (e.g. 0.75 for 75%)
    pub compact_threshold_pct: f64,
}

impl Default for ContextBudget {
    fn default() -> Self {
        Self {
            window_capacity: 128_000,
            compact_threshold_pct: 0.75, // 75% capacity threshold
        }
    }
}

impl ContextBudget {
    pub fn new(window_capacity: u32, threshold_pct: f64) -> Self {
        Self {
            window_capacity,
            compact_threshold_pct: threshold_pct.clamp(0.1, 0.95),
        }
    }

    /// Auto-compaction threshold in tokens
    pub fn threshold_tokens(&self) -> u32 {
        (self.window_capacity as f64 * self.compact_threshold_pct) as u32
    }

    /// Returns true if current estimated tokens exceeds the auto-compaction ceiling
    pub fn is_above_threshold(&self, current_tokens: u32) -> bool {
        current_tokens >= self.threshold_tokens()
    }

    /// Current context usage percentage (0.0 to 100.0%)
    pub fn usage_percentage(&self, current_tokens: u32) -> f64 {
        if self.window_capacity == 0 {
            0.0
        } else {
            (current_tokens as f64 / self.window_capacity as f64) * 100.0
        }
    }

    /// Formats a clean status pill for the terminal UI: e.g. "14.2k / 128k (11.1%)"
    pub fn format_status_pill(&self, current_tokens: u32) -> String {
        let cur_k = current_tokens as f64 / 1000.0;
        let win_k = self.window_capacity as f64 / 1000.0;
        let pct = self.usage_percentage(current_tokens);
        format!("{:.1}k / {:.0}k ({:.1}%)", cur_k, win_k, pct)
    }

    /// Formats a compact chip for prompt bar: e.g. "14.2k/128k"
    pub fn format_short_pill(&self, current_tokens: u32) -> String {
        let cur_k = current_tokens as f64 / 1000.0;
        let win_k = self.window_capacity as f64 / 1000.0;
        format!("{:.1}k/{:.0}k", cur_k, win_k)
    }
}

/// Structured summary generated during conversation compaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextSummary {
    pub goal_and_task: String,
    pub key_decisions: Vec<String>,
    pub files_touched: Vec<String>,
    pub pending_steps: Vec<String>,
    pub pre_compact_tokens: u32,
    pub post_compact_tokens: u32,
    pub compacted_turns_count: usize,
}

impl ContextSummary {
    /// Computes percentage of tokens saved by compaction.
    pub fn reduction_percentage(&self) -> f64 {
        if self.pre_compact_tokens > 0 {
            (self.pre_compact_tokens.saturating_sub(self.post_compact_tokens) as f64
                / self.pre_compact_tokens as f64)
                * 100.0
        } else {
            0.0
        }
    }

    /// Formats the summary into a standardized <context_summary> XML fence for the model.
    pub fn to_markdown(&self) -> String {
        let mut out = String::new();
        out.push_str("<context_summary>\n");
        out.push_str("This conversation was compacted to preserve context efficiency.\n\n");
        out.push_str("## Core Objective & User Intent\n");
        out.push_str(&self.goal_and_task);
        out.push_str("\n\n");

        if !self.key_decisions.is_empty() {
            out.push_str("## Key Architectural Decisions & Discoveries\n");
            for d in &self.key_decisions {
                out.push_str(&format!("- {}\n", d));
            }
            out.push('\n');
        }

        if !self.files_touched.is_empty() {
            out.push_str("## Touched Files & System State\n");
            for f in &self.files_touched {
                out.push_str(&format!("- {}\n", f));
            }
            out.push('\n');
        }

        if !self.pending_steps.is_empty() {
            out.push_str("## Current State & Pending Next Steps\n");
            for s in &self.pending_steps {
                out.push_str(&format!("- {}\n", s));
            }
            out.push('\n');
        }

        out.push_str(&format!(
            "<!-- Compacted {} turns: {} tokens -> {} tokens ({:.1}% reduction) -->\n",
            self.compacted_turns_count,
            self.pre_compact_tokens,
            self.post_compact_tokens,
            if self.pre_compact_tokens > 0 {
                (self.pre_compact_tokens.saturating_sub(self.post_compact_tokens) as f64
                    / self.pre_compact_tokens as f64)
                    * 100.0
            } else {
                0.0
            }
        ));
        out.push_str("</context_summary>");
        out
    }
}

/// Macro-compactor responsible for history partitioning, git probing, and context boundary synthesis.
pub struct MacroCompactor;

impl MacroCompactor {
    /// Deterministic BPE / word+char token counter calibrated to Claude tokenizer (~3.75 chars/token).
    pub fn estimate_tokens(text: &str) -> u32 {
        if text.is_empty() {
            return 0;
        }
        let mut count = 0;
        for word in text.split_whitespace() {
            if word.len() <= 4 {
                count += 1;
            } else {
                count += word.len().div_ceil(4);
            }
        }
        let punct_count = text
            .chars()
            .filter(|c| "{}[],:;\"'=><()!/\\#*`-_".contains(*c))
            .count();
        let char_floor = text.len().div_ceil(4);
        (count.max(char_floor).max((punct_count * 2 / 3) + 1)) as u32
    }

    /// Estimates total tokens for a single message including tool calls and results.
    pub fn estimate_message_tokens(msg: &Message) -> u32 {
        let mut total = Self::estimate_tokens(&msg.content);
        if let Some(calls) = &msg.tool_calls {
            for call in calls {
                total += Self::estimate_tokens(&call.name);
                total += Self::estimate_tokens(&call.arguments.to_string());
            }
        }
        if let Some(results) = &msg.tool_results {
            for res in results {
                total += Self::estimate_tokens(&res.output);
            }
        }
        total + 4 // role framing overhead
    }

    /// Estimates total cumulative tokens across the entire conversation history.
    pub fn estimate_history_tokens(history: &[Message]) -> u32 {
        history.iter().map(Self::estimate_message_tokens).sum()
    }

    /// Returns true if the conversation exceeds the auto-compaction budget threshold.
    pub fn should_auto_compact(history: &[Message], budget: &ContextBudget) -> bool {
        let current = Self::estimate_history_tokens(history);
        budget.is_above_threshold(current)
    }

    /// Partitions history and synthesizes a CompactBoundary message.
    ///
    /// Preserved messages: The last `preserve_count` turns (default: 2, keeping the active goal/tool result verbatim).
    /// Compacted messages: Older turns synthesized into a structured `<context_summary>`.
    pub fn compact_history(
        history: &[Message],
        workspace_dir: &Path,
        custom_instructions: Option<&str>,
        preserve_count: usize,
    ) -> (Vec<Message>, ContextSummary) {
        let total_msgs = history.len();
        let preserve_msgs = preserve_count.max(2).min(total_msgs);
        let split_idx = total_msgs.saturating_sub(preserve_msgs);

        let to_compact = &history[..split_idx];
        let preserved = &history[split_idx..];

        let pre_tokens = Self::estimate_history_tokens(history);

        // 1. Extract goals from initial user prompts
        let mut user_goals = Vec::new();
        for msg in to_compact.iter().filter(|m| m.role == "user") {
            if !msg.content.trim().is_empty() && !msg.content.contains("<context_summary>") {
                user_goals.push(msg.content.trim().to_string());
            }
        }

        let goal_summary = if let Some(instructions) = custom_instructions {
            format!("{}\n(User Focus: {})", user_goals.first().cloned().unwrap_or_else(|| "General development task".to_string()), instructions)
        } else if let Some(first_goal) = user_goals.first() {
            first_goal.clone()
        } else {
            "Autonomous workspace tasks and refactoring.".to_string()
        };

        // 2. Extract decisions & key findings from assistant replies
        let mut decisions = Vec::new();
        for msg in to_compact.iter().filter(|m| m.role == "assistant") {
            if !msg.content.trim().is_empty() {
                let first_line = msg.content.lines().next().unwrap_or("").trim();
                if !first_line.is_empty() && first_line.len() > 10 {
                    decisions.push(first_line.to_string());
                }
            }
            if let Some(calls) = &msg.tool_calls {
                for c in calls {
                    if c.name == "edit_file" || c.name == "write_file" {
                        if let Some(p) = c.arguments.get("path").and_then(|v| v.as_str()) {
                            decisions.push(format!("Applied modifications to `{}` via `{}`", p, c.name));
                        }
                    }
                }
            }
        }
        decisions.dedup();
        decisions.truncate(6);

        // 3. Extract touched files from Git
        let mut files_touched = Vec::new();
        if let Ok(git_summary) = GitSnapshotManager::get_diff_summary(workspace_dir) {
            for line in git_summary.lines() {
                let trimmed = line.trim();
                if !trimmed.is_empty() && !trimmed.starts_with("Modified/Untracked") && !trimmed.starts_with("Diff stat") {
                    files_touched.push(trimmed.to_string());
                }
            }
        }
        files_touched.truncate(8);

        // 4. Extract pending next steps
        let mut pending_steps = Vec::new();
        if let Some(last_compacted) = to_compact.last() {
            if !last_compacted.content.is_empty() {
                let tail = last_compacted.content.lines().rev().take(2).collect::<Vec<_>>();
                for line in tail.into_iter().rev() {
                    let t = line.trim();
                    if !t.is_empty() {
                        pending_steps.push(t.to_string());
                    }
                }
            }
        }
        if pending_steps.is_empty() {
            pending_steps.push("Proceed with user's next active instruction in the preserved context.".to_string());
        }

        // Build summary object
        let mut summary = ContextSummary {
            goal_and_task: goal_summary,
            key_decisions: decisions,
            files_touched,
            pending_steps,
            pre_compact_tokens: pre_tokens,
            post_compact_tokens: 0,
            compacted_turns_count: split_idx,
        };

        let summary_text = summary.to_markdown();
        let compact_boundary_msg = Message {
            role: "user".to_string(),
            content: summary_text,
            tool_calls: None,
            tool_results: None,
            timestamp: Utc::now(),
        };

        // Assemble new history: [CompactBoundary, ...preserved]
        let mut new_history = Vec::with_capacity(preserved.len() + 1);
        new_history.push(compact_boundary_msg);
        new_history.extend_from_slice(preserved);

        let post_tokens = Self::estimate_history_tokens(&new_history);
        summary.post_compact_tokens = post_tokens;

        (new_history, summary)
    }
}

/// Manages recovery of interrupted token generation (Ctrl+C or network drops).
pub struct InterruptedOutputManager;

impl InterruptedOutputManager {
    /// Fences partial output in <interrupted-output> tags with continuation instructions.
    pub fn wrap_interrupted_output(partial: &str) -> String {
        let trimmed = partial.trim();
        if trimmed.is_empty() {
            return String::new();
        }

        format!(
            "Continuing an interrupted response. Text before the interruption follows:\n\n<interrupted-output>\n{}\n</interrupted-output>\n\nContinue from exactly where you left off without repeating the prior output.",
            trimmed
        )
    }

    /// Extracts the partial text from an interrupted output fence if present.
    pub fn extract_interrupted_output(content: &str) -> Option<String> {
        let start_tag = "<interrupted-output>";
        let end_tag = "</interrupted-output>";

        if let Some(start_idx) = content.find(start_tag) {
            let after_start = &content[start_idx + start_tag.len()..];
            if let Some(end_idx) = after_start.find(end_tag) {
                return Some(after_start[..end_idx].trim().to_string());
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_budget_threshold() {
        let budget = ContextBudget::new(100_000, 0.75);
        assert_eq!(budget.threshold_tokens(), 75_000);
        assert!(!budget.is_above_threshold(74_999));
        assert!(budget.is_above_threshold(75_000));
        assert_eq!(budget.usage_percentage(50_000), 50.0);
        assert!(budget.format_status_pill(50_000).contains("50.0k / 100k (50.0%)"));
    }

    #[test]
    fn test_macro_compaction_partitions_history() {
        let mut history = Vec::new();
        for i in 1..=10 {
            history.push(Message {
                role: if i % 2 == 1 { "user".to_string() } else { "assistant".to_string() },
                content: format!(
                    "Turn {} message content with a large detailed block of code and compilation logs.\n\
                     fn process_step_{}() {{\n    \
                         println!(\"Executing step {}\");\n    \
                         let mut buffer = Vec::new();\n    \
                         for item in 0..100 {{\n        \
                             buffer.push(format!(\"item_{{}}\", item));\n    \
                         }}\n    \
                         assert_eq!(buffer.len(), 100);\n\
                     }}\n\
                     Stack trace and compiler diagnostic info for turn {}: zero errors found, proceeding with analysis.",
                    i, i, i, i
                ),
                tool_calls: None,
                tool_results: None,
                timestamp: Utc::now(),
            });
        }

        let temp_dir = std::env::temp_dir();
        let (new_history, summary) = MacroCompactor::compact_history(&history, &temp_dir, Some("Fix parser bug"), 2);

        // Preserved last 2 turns + 1 compact boundary = 3 messages
        assert_eq!(new_history.len(), 3);
        assert!(new_history[0].content.contains("<context_summary>"));
        assert!(new_history[0].content.contains("Fix parser bug"));
        assert_eq!(new_history[1].content, history[8].content);
        assert_eq!(new_history[2].content, history[9].content);
        assert_eq!(summary.compacted_turns_count, 8);
        assert!(summary.pre_compact_tokens > summary.post_compact_tokens);
    }

    #[test]
    fn test_interrupted_output_fence_and_extraction() {
        let partial = "let total = a + b;";
        let wrapped = InterruptedOutputManager::wrap_interrupted_output(partial);
        assert!(wrapped.contains("<interrupted-output>"));
        assert!(wrapped.contains("let total = a + b;"));

        let extracted = InterruptedOutputManager::extract_interrupted_output(&wrapped);
        assert_eq!(extracted, Some("let total = a + b;".to_string()));
    }
}
