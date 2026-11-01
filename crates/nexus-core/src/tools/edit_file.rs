use std::fs;
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PatchTier {
    Tier1Exact,
    Tier2WhitespaceNormalized,
    Tier3AnchorContext,
}

#[derive(Error, Debug, PartialEq, Eq)]
pub enum EditError {
    #[error("File not found: {0}")]
    NotFound(String),
    #[error("Target block is empty")]
    EmptyTarget,
    #[error("Target string not found in file (exhausted Tier 1 Exact, Tier 2 Normalized, and Tier 3 Context Hunk)")]
    TargetNotFound,
    #[error("Ambiguous match in {file} via {tier:?}: found {occurrences} occurrences of target block. Provide more surrounding context lines.")]
    AmbiguousMatch {
        file: String,
        occurrences: usize,
        tier: PatchTier,
    },
    #[error("IO error: {0}")]
    Io(String),
}

impl From<std::io::Error> for EditError {
    fn from(err: std::io::Error) -> Self {
        EditError::Io(err.to_string())
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PatchReport {
    pub tier: PatchTier,
    pub start_line: usize,
    pub end_line: usize,
    pub occurrences_checked: usize,
}

/// Safely replaces target content using a 3-tier matching engine:
/// 1. Tier 1: Exact string match with ambiguity guard.
/// 2. Tier 2: Whitespace-normalized line matching with ambiguity guard.
/// 3. Tier 3: Context hunk / anchor matching with ambiguity guard.
///
/// In all tiers, if more than 1 match is found, editing FAILS with AmbiguousMatch
/// to prevent silent corruption.
pub fn safe_replace(
    file_path: &Path,
    target: &str,
    replacement: &str,
) -> Result<PatchReport, EditError> {
    if !file_path.exists() {
        return Err(EditError::NotFound(file_path.display().to_string()));
    }

    if target.is_empty() {
        return Err(EditError::EmptyTarget);
    }

    let content = fs::read_to_string(file_path)?;

    // =========================================================================
    // Tier 1: Exact Match
    // =========================================================================
    let exact_occurrences = content.matches(target).count();
    if exact_occurrences > 1 {
        return Err(EditError::AmbiguousMatch {
            file: file_path.display().to_string(),
            occurrences: exact_occurrences,
            tier: PatchTier::Tier1Exact,
        });
    }

    if exact_occurrences == 1 {
        let (start_line, end_line) = calculate_line_span(&content, target);
        let updated = content.replacen(target, replacement, 1);
        fs::write(file_path, updated)?;
        return Ok(PatchReport {
            tier: PatchTier::Tier1Exact,
            start_line,
            end_line,
            occurrences_checked: 1,
        });
    }

    // =========================================================================
    // Tier 2: Whitespace-Normalized Line Match
    // =========================================================================
    if let Some(report) = try_whitespace_normalized(file_path, &content, target, replacement)? {
        return Ok(report);
    }

    // =========================================================================
    // Tier 3: Multi-Line Anchor / Context Hunk Match
    // =========================================================================
    if let Some(report) = try_anchor_context(file_path, &content, target, replacement)? {
        return Ok(report);
    }

    Err(EditError::TargetNotFound)
}

fn calculate_line_span(content: &str, target: &str) -> (usize, usize) {
    if let Some(idx) = content.find(target) {
        let start_line = content[..idx].lines().count().max(1);
        let target_line_count = target.lines().count().max(1);
        let end_line = start_line + target_line_count - 1;
        (start_line, end_line)
    } else {
        (1, 1)
    }
}

/// Tier 2: Normalizes leading and trailing whitespace on each line,
/// scanning a sliding window over file lines.
fn try_whitespace_normalized(
    file_path: &Path,
    content: &str,
    target: &str,
    replacement: &str,
) -> Result<Option<PatchReport>, EditError> {
    let file_lines: Vec<&str> = content.lines().collect();
    let target_lines: Vec<&str> = target.lines().collect();

    if target_lines.is_empty() || file_lines.len() < target_lines.len() {
        return Ok(None);
    }

    let norm_target: Vec<String> = target_lines.iter().map(|l| l.trim().to_string()).collect();

    let mut matched_windows: Vec<usize> = Vec::new();
    let window_size = target_lines.len();

    for i in 0..=(file_lines.len() - window_size) {
        let mut matches = true;
        for k in 0..window_size {
            if file_lines[i + k].trim() != norm_target[k] {
                matches = false;
                break;
            }
        }
        if matches {
            matched_windows.push(i);
        }
    }

    if matched_windows.len() > 1 {
        return Err(EditError::AmbiguousMatch {
            file: file_path.display().to_string(),
            occurrences: matched_windows.len(),
            tier: PatchTier::Tier2WhitespaceNormalized,
        });
    }

    if matched_windows.len() == 1 {
        let start_line_idx = matched_windows[0];
        let end_line_idx = start_line_idx + window_size;

        // Reconstruct file: lines before + replacement + lines after
        let mut updated_lines: Vec<&str> = Vec::new();
        updated_lines.extend_from_slice(&file_lines[..start_line_idx]);

        let rep_lines: Vec<&str> = replacement.lines().collect();
        updated_lines.extend_from_slice(&rep_lines);

        if end_line_idx < file_lines.len() {
            updated_lines.extend_from_slice(&file_lines[end_line_idx..]);
        }

        let mut updated_content = updated_lines.join("\n");
        if content.ends_with('\n') {
            updated_content.push('\n');
        }

        fs::write(file_path, updated_content)?;

        return Ok(Some(PatchReport {
            tier: PatchTier::Tier2WhitespaceNormalized,
            start_line: start_line_idx + 1,
            end_line: end_line_idx,
            occurrences_checked: 1,
        }));
    }

    Ok(None)
}

/// Tier 3: Anchor / Context Hunk Matching.
/// Uses head and tail anchor lines to identify candidate replacement region.
fn try_anchor_context(
    file_path: &Path,
    content: &str,
    target: &str,
    replacement: &str,
) -> Result<Option<PatchReport>, EditError> {
    let file_lines: Vec<&str> = content.lines().collect();
    let target_lines: Vec<&str> = target.lines().collect();

    // Requires at least 3 lines to have meaningful head and tail anchors
    if target_lines.len() < 3 || file_lines.len() < target_lines.len() {
        return Ok(None);
    }

    let head_anchor = target_lines[0].trim();
    let tail_anchor = target_lines[target_lines.len() - 1].trim();

    if head_anchor.is_empty() || tail_anchor.is_empty() {
        return Ok(None);
    }

    let mut candidate_spans: Vec<(usize, usize)> = Vec::new();

    for (h_idx, line) in file_lines.iter().enumerate() {
        if line.trim() == head_anchor {
            // Search ahead within a reasonable window (target length ± 4 lines)
            let min_tail = h_idx + target_lines.len().saturating_sub(4).max(1);
            let max_tail = (h_idx + target_lines.len() + 4).min(file_lines.len());

            for t_idx in min_tail..max_tail {
                if file_lines[t_idx].trim() == tail_anchor {
                    // Check similarity of interior lines
                    let interior_file = &file_lines[h_idx + 1..t_idx];
                    let interior_target = &target_lines[1..target_lines.len() - 1];

                    if is_interior_similar(interior_file, interior_target) {
                        candidate_spans.push((h_idx, t_idx));
                    }
                }
            }
        }
    }

    if candidate_spans.len() > 1 {
        return Err(EditError::AmbiguousMatch {
            file: file_path.display().to_string(),
            occurrences: candidate_spans.len(),
            tier: PatchTier::Tier3AnchorContext,
        });
    }

    if candidate_spans.len() == 1 {
        let (start_idx, end_idx) = candidate_spans[0];
        let end_line_exclusive = end_idx + 1;

        let mut updated_lines: Vec<&str> = Vec::new();
        updated_lines.extend_from_slice(&file_lines[..start_idx]);

        let rep_lines: Vec<&str> = replacement.lines().collect();
        updated_lines.extend_from_slice(&rep_lines);

        if end_line_exclusive < file_lines.len() {
            updated_lines.extend_from_slice(&file_lines[end_line_exclusive..]);
        }

        let mut updated_content = updated_lines.join("\n");
        if content.ends_with('\n') {
            updated_content.push('\n');
        }

        fs::write(file_path, updated_content)?;

        return Ok(Some(PatchReport {
            tier: PatchTier::Tier3AnchorContext,
            start_line: start_idx + 1,
            end_line: end_line_exclusive,
            occurrences_checked: 1,
        }));
    }

    Ok(None)
}

/// Checks if two slices of lines share sufficient line overlap (>= 60% match)
fn is_interior_similar(file_slice: &[&str], target_slice: &[&str]) -> bool {
    if file_slice.is_empty() && target_slice.is_empty() {
        return true;
    }
    if file_slice.is_empty() || target_slice.is_empty() {
        return false;
    }

    let mut matches = 0;
    for t_line in target_slice {
        let t_trimmed = t_line.trim();
        if file_slice.iter().any(|f| f.trim() == t_trimmed) {
            matches += 1;
        }
    }

    let max_len = file_slice.len().max(target_slice.len());
    (matches as f32 / max_len as f32) >= 0.5
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_tier1_exact_match() {
        let temp_file = std::env::temp_dir().join("nexus_tier1_test.txt");
        let content = "fn main() {\n    let answer = 41;\n    println!(\"{}\", answer);\n}\n";
        fs::write(&temp_file, content).unwrap();

        let report = safe_replace(&temp_file, "let answer = 41;", "let answer = 42;").unwrap();
        assert_eq!(report.tier, PatchTier::Tier1Exact);
        assert_eq!(report.start_line, 2);

        let updated = fs::read_to_string(&temp_file).unwrap();
        assert!(updated.contains("let answer = 42;"));
    }

    #[test]
    fn test_tier1_ambiguity_guard() {
        let temp_file = std::env::temp_dir().join("nexus_ambig_test.txt");
        let content = "let x = 1;\nlet x = 1;\n";
        fs::write(&temp_file, content).unwrap();

        let err = safe_replace(&temp_file, "let x = 1;", "let x = 2;").unwrap_err();
        assert_eq!(
            err,
            EditError::AmbiguousMatch {
                file: temp_file.display().to_string(),
                occurrences: 2,
                tier: PatchTier::Tier1Exact,
            }
        );
    }

    #[test]
    fn test_tier2_whitespace_normalized() {
        let temp_file = std::env::temp_dir().join("nexus_tier2_test.txt");
        // Indentation in file is 8 spaces
        let content = "fn test() {\n        let val = 100;\n        val + 1\n}\n";
        fs::write(&temp_file, content).unwrap();

        // Model provided 4 spaces
        let target = "    let val = 100;\n    val + 1";
        let replacement = "        let val = 200;\n        val + 2";

        let report = safe_replace(&temp_file, target, replacement).unwrap();
        assert_eq!(report.tier, PatchTier::Tier2WhitespaceNormalized);

        let updated = fs::read_to_string(&temp_file).unwrap();
        assert!(updated.contains("let val = 200;"));
    }

    #[test]
    fn test_tier2_ambiguity_guard() {
        let temp_file = std::env::temp_dir().join("nexus_tier2_ambig_test.txt");
        let content = "  let x = 1;\n    let x = 1;\n";
        fs::write(&temp_file, content).unwrap();

        // 6 spaces does not exist as an exact substring in content,
        // so Tier 1 has 0 matches. Tier 2 normalizes both lines to "let x = 1;"
        // and detects the ambiguity!
        let target = "      let x = 1;";
        let err = safe_replace(&temp_file, target, "let x = 2;").unwrap_err();
        assert_eq!(
            err,
            EditError::AmbiguousMatch {
                file: temp_file.display().to_string(),
                occurrences: 2,
                tier: PatchTier::Tier2WhitespaceNormalized,
            }
        );
    }

    #[test]
    fn test_tier3_anchor_context() {
        let temp_file = std::env::temp_dir().join("nexus_tier3_test.txt");
        let content = "fn compute() {\n    // start computation\n    let a = 1;\n    let b = 2;\n    // finish\n}\n";
        fs::write(&temp_file, content).unwrap();

        // Model got head and tail right, but missed the middle comment
        let target = "fn compute() {\n    let a = 1;\n    let b = 2;\n    // finish\n}";
        let replacement = "fn compute() {\n    let a = 10;\n    let b = 20;\n    // finish\n}";

        let report = safe_replace(&temp_file, target, replacement).unwrap();
        assert_eq!(report.tier, PatchTier::Tier3AnchorContext);

        let updated = fs::read_to_string(&temp_file).unwrap();
        assert!(updated.contains("let a = 10;"));
    }
}
