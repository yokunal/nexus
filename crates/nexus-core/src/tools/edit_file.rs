use std::fs;
use std::path::Path;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum EditError {
    #[error("File not found: {0}")]
    NotFound(String),
    #[error("Target string not found in file: {0}")]
    TargetNotFound(String),
    #[error("Ambiguous match: found {occurrences} occurrences of target block in {file}. Provide more surrounding context lines.")]
    AmbiguousMatch { file: String, occurrences: usize },
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Safely replaces an exact target chunk in a file with a replacement chunk.
/// Fails if the target is missing or if it matches more than once (Ambiguity Guard).
pub fn safe_replace(file_path: &Path, target: &str, replacement: &str) -> Result<(), EditError> {
    if !file_path.exists() {
        return Err(EditError::NotFound(file_path.display().to_string()));
    }

    let content = fs::read_to_string(file_path)?;

    // Check occurrences
    let occurrences = content.matches(target).count();

    if occurrences == 0 {
        // Attempt a whitespace-normalized match fallback
        let normalized_target = target.trim();
        let normalized_occurrences = content
            .lines()
            .map(|l| l.trim())
            .filter(|l| l.contains(normalized_target))
            .count();

        if normalized_occurrences > 1 {
            return Err(EditError::AmbiguousMatch {
                file: file_path.display().to_string(),
                occurrences: normalized_occurrences,
            });
        }

        return Err(EditError::TargetNotFound(target.to_string()));
    }

    if occurrences > 1 {
        return Err(EditError::AmbiguousMatch {
            file: file_path.display().to_string(),
            occurrences,
        });
    }

    // Exact single match
    let updated = content.replacen(target, replacement, 1);
    fs::write(file_path, updated)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_ambiguity_guard() {
        let temp_file = std::env::temp_dir().join("nexus_edit_ambiguous_test.txt");
        let content = "fn main() {\n    let x = 10;\n    let x = 10;\n}\n";
        fs::write(&temp_file, content).unwrap();

        // Target matches twice -> Must fail with AmbiguousMatch!
        let result = safe_replace(&temp_file, "let x = 10;", "let x = 20;");
        assert!(matches!(
            result,
            Err(EditError::AmbiguousMatch { occurrences: 2, .. })
        ));

        // Target with unique context -> Must succeed!
        let unique_target = "fn main() {\n    let x = 10;";
        let unique_replacement = "fn main() {\n    let x = 20;";
        let result = safe_replace(&temp_file, unique_target, unique_replacement);
        assert!(result.is_ok());

        let new_content = fs::read_to_string(&temp_file).unwrap();
        assert!(new_content.contains("let x = 20;"));
    }
}
