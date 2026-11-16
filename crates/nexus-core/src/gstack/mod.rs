use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GStackReviewMode {
    CeoReview,
    EngReview,
    QaLoop,
    ShipReview,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CeoScopePosture {
    ScopeExpansion,
    SelectiveExpansion,
    HoldScope,
    ScopeReduction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchitectureInvariants {
    pub interface_first: bool,
    pub explicit_error_domains: bool,
    pub shallow_wrapper_guard: bool,
    pub test_coverage_contract: bool,
    pub persistence_wal_first: bool,
    pub custom_rules: Vec<String>,
}

impl Default for ArchitectureInvariants {
    fn default() -> Self {
        Self {
            interface_first: true,
            explicit_error_domains: true,
            shallow_wrapper_guard: true,
            test_coverage_contract: true,
            persistence_wal_first: true,
            custom_rules: Vec::new(),
        }
    }
}

pub struct GStackEngine;

impl GStackEngine {
    /// Loads custom invariants from `AGENTS.md` in the workspace if present,
    /// otherwise returns default declarative architecture standards.
    pub fn load_invariants(workspace: &Path) -> ArchitectureInvariants {
        let agents_file = workspace.join("AGENTS.md");
        let mut invariants = ArchitectureInvariants::default();

        if agents_file.exists() {
            if let Ok(content) = fs::read_to_string(&agents_file) {
                for line in content.lines() {
                    let trimmed = line.trim();
                    if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
                        invariants.custom_rules.push(trimmed[2..].to_string());
                    }
                }
            }
        }

        invariants
    }

    /// Formats the prompt section for declarative architecture invariants.
    pub fn format_architecture_prompt(invariants: &ArchitectureInvariants) -> String {
        let mut prompt = String::from("## Declarative Architecture Invariants\n");
        if invariants.interface_first {
            prompt.push_str(
                "- **Interface-First Contract**: Define traits, interfaces, and public types before writing concrete implementation bodies.\n",
            );
        }
        if invariants.explicit_error_domains {
            prompt.push_str(
                "- **Explicit Error Domains**: Use dedicated `thiserror` error enums. Never call `.unwrap()` or `.expect()` in non-test production code.\n",
            );
        }
        if invariants.shallow_wrapper_guard {
            prompt.push_str(
                "- **Modularity Invariant**: Deep modules over shallow wrappers. Encapsulate related business logic cleanly.\n",
            );
        }
        if invariants.test_coverage_contract {
            prompt.push_str(
                "- **Test Coverage Contract**: Every new public interface must be accompanied by unit or integration tests verifying happy and failure paths.\n",
            );
        }
        if invariants.persistence_wal_first {
            prompt.push_str(
                "- **Persistence & Storage Architecture Invariant**: Never implement naive full-file rewrite or truncation persistence (`truncate(true)`). Always build production-grade Append-Only Write-Ahead Logs (WAL) or transactional SQLite with WAL journaling. Every storage engine must guarantee:\n  1. Atomic sequential append-only writes with fsync flushing.\n  2. Crash-recovery sequential replay on startup that reconstructs in-memory state.\n  3. Background or explicit log compaction to prune obsolete tombstones, expired entries, and overwritten keys.\n",
            );
        }

        if !invariants.custom_rules.is_empty() {
            prompt.push_str("\n### Project Specific Rules (from AGENTS.md):\n");
            for rule in &invariants.custom_rules {
                prompt.push_str(&format!("- {}\n", rule));
            }
        }

        prompt
    }

    /// Generates specialized prompt instructions for a given gstack review mode.
    pub fn format_review_prompt(mode: GStackReviewMode) -> String {
        match mode {
            GStackReviewMode::CeoReview => {
                r#"## gstack: CEO Review Mode
You are acting as the Founder/CEO reviewing this product or feature plan.
Your mission is to evaluate:
1. The 10-Star Product: What would turn this from an ordinary tool into a magical experience?
2. Question Premises: Challenge unexamined assumptions. Are we solving the root problem?
3. Strategic Posture: Recommend one of 4 scope postures:
   - SCOPE EXPANSION: Dream big and expand scope to capture unfair competitive advantage.
   - SELECTIVE EXPANSION: Hold the core scope steady, cherry-pick 1-2 high-leverage delight features.
   - HOLD SCOPE: Deliver the planned scope with maximum engineering rigor.
   - SCOPE REDUCTION: Strip to the absolute simplest working MVP.
Output a structured review with recommendations and next steps."#.to_string()
            }
            GStackReviewMode::EngReview => {
                r#"## gstack: Engineering Architecture Review Mode
You are acting as the Principal Engineer/Engineering Manager reviewing the execution architecture.
Evaluate the code and plans against these criteria:
1. Single Source of Truth: Does each entity have exactly one canonical owner?
2. Failure Domains: Are error states cleanly isolated and propagated?
3. Interface Contracts: Are interfaces minimal, cohesive, and easy to mock in tests?
4. Performance & Memory: Zero unnecessary allocations, optimal runtime footprint.
5. Invariants: Are system constraints guaranteed at compile time where possible?
Identify architectural risks and provide concrete code refactoring advice."#.to_string()
            }
            GStackReviewMode::QaLoop => {
                r#"## gstack: QA Verify-Fix Loop Mode
You are running the automated verification and repair cycle:
1. Run compilation checks and test suites (`cargo_check`, `run_command`).
2. If failures occur: pinpoint the exact failing assertion or compiler error span.
3. Formulate an atomic fix targeting root cause, NOT symptom suppression.
4. Apply the patch via `edit_file` (ensuring ambiguity guards pass).
5. Re-run verification until all checks pass cleanly.
Never mark a task complete without positive verification evidence."#.to_string()
            }
            GStackReviewMode::ShipReview => {
                r#"## gstack: Ship Review & Pre-Landing Gate
You are acting as the Release Gatekeeper preparing changes to ship:
1. Verify working tree is clean and unstaged modifications are intentional.
2. Confirm all unit tests pass (`cargo test`) with 100% success.
3. Confirm linter and compiler pass with zero warnings (`cargo clippy -- -D warnings`).
4. Ensure documentation, versioning, and CHANGELOG updates accurately reflect user-facing changes.
Provide a clear Go / No-Go landing verdict."#.to_string()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_invariants() {
        let invariants = ArchitectureInvariants::default();
        let prompt = GStackEngine::format_architecture_prompt(&invariants);
        assert!(prompt.contains("Interface-First Contract"));
        assert!(prompt.contains("Explicit Error Domains"));
        assert!(prompt.contains("Test Coverage Contract"));
        assert!(prompt.contains("Persistence & Storage Architecture Invariant"));
        assert!(prompt.contains("Append-Only Write-Ahead Logs"));
    }

    #[test]
    fn test_custom_invariants_loading() {
        let temp_dir = std::env::temp_dir().join("nexus_gstack_test");
        let _ = fs::create_dir_all(&temp_dir);
        let agents_md = temp_dir.join("AGENTS.md");
        fs::write(
            &agents_md,
            "# Rules\n- No unsafe code allowed\n- Prefer async channels over shared mutexes\n",
        )
        .unwrap();

        let invariants = GStackEngine::load_invariants(&temp_dir);
        assert_eq!(invariants.custom_rules.len(), 2);
        assert!(invariants.custom_rules[0].contains("No unsafe code allowed"));

        let prompt = GStackEngine::format_architecture_prompt(&invariants);
        assert!(prompt.contains("No unsafe code allowed"));
        assert!(prompt.contains("Prefer async channels"));
    }

    #[test]
    fn test_review_modes_prompt() {
        let ceo_prompt = GStackEngine::format_review_prompt(GStackReviewMode::CeoReview);
        assert!(ceo_prompt.contains("CEO Review Mode"));
        assert!(ceo_prompt.contains("SCOPE EXPANSION"));

        let eng_prompt = GStackEngine::format_review_prompt(GStackReviewMode::EngReview);
        assert!(eng_prompt.contains("Engineering Architecture Review Mode"));

        let qa_prompt = GStackEngine::format_review_prompt(GStackReviewMode::QaLoop);
        assert!(qa_prompt.contains("QA Verify-Fix Loop Mode"));

        let ship_prompt = GStackEngine::format_review_prompt(GStackReviewMode::ShipReview);
        assert!(ship_prompt.contains("Ship Review"));
    }
}
