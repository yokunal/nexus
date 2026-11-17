use crate::errors::NexusV2Error;

pub struct PatchEngine;

impl PatchEngine {
    pub fn apply_patch(original: &str, target: &str, replacement: &str) -> Result<String, NexusV2Error> {
        if target.is_empty() {
            return Err(NexusV2Error::SafetyViolation("Target block to replace cannot be empty".to_string()));
        }

        // Tier 1: Exact Byte Match with Ambiguity Guard
        let exact_count = original.matches(target).count();
        if exact_count > 1 {
            return Err(NexusV2Error::AmbiguousPatch { count: exact_count });
        }
        if exact_count == 1 {
            return Ok(original.replacen(target, replacement, 1));
        }

        // Tier 2: Whitespace-Normalized Match (CRLF vs LF, trailing whitespace)
        let normalized_orig = original.replace("\r\n", "\n");
        let normalized_target = target.replace("\r\n", "\n");
        let norm_count = normalized_orig.matches(&normalized_target).count();
        if norm_count > 1 {
            return Err(NexusV2Error::AmbiguousPatch { count: norm_count });
        }
        if norm_count == 1 {
            return Ok(normalized_orig.replacen(&normalized_target, replacement, 1));
        }

        // Tier 3: Line-Trimmed Anchor Match
        let target_lines: Vec<&str> = target.lines().map(|l| l.trim()).filter(|l| !l.is_empty()).collect();
        if target_lines.is_empty() {
            return Err(NexusV2Error::SafetyViolation("No non-empty target lines found".to_string()));
        }

        let orig_lines: Vec<&str> = original.lines().collect();
        let mut matches = Vec::new();

        for i in 0..orig_lines.len() {
            if i + target_lines.len() <= orig_lines.len() {
                let slice = &orig_lines[i..i + target_lines.len()];
                let matches_all = slice
                    .iter()
                    .zip(target_lines.iter())
                    .all(|(orig_l, tgt_l)| orig_l.trim() == *tgt_l);
                if matches_all {
                    matches.push((i, i + target_lines.len()));
                }
            }
        }

        if matches.len() > 1 {
            return Err(NexusV2Error::AmbiguousPatch { count: matches.len() });
        }

        if let Some((start, end)) = matches.first() {
            let mut result_lines = Vec::new();
            result_lines.extend_from_slice(&orig_lines[0..*start]);
            result_lines.push(replacement);
            result_lines.extend_from_slice(&orig_lines[*end..]);
            return Ok(result_lines.join("\n"));
        }

        Err(NexusV2Error::SafetyViolation(format!(
            "Could not locate unique target block in file (0 matches across 3 tiers)"
        )))
    }
}
