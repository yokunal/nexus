use std::path::Path;

#[derive(Debug, Clone)]
pub struct VerificationReport {
    pub success: bool,
    pub diagnostics: Vec<String>,
}

pub struct Verifier;

impl Verifier {
    pub fn verify_file_content(path: impl AsRef<Path>, expected_substring: &str) -> VerificationReport {
        match std::fs::read_to_string(path) {
            Ok(content) => {
                if content.contains(expected_substring) {
                    VerificationReport {
                        success: true,
                        diagnostics: Vec::new(),
                    }
                } else {
                    VerificationReport {
                        success: false,
                        diagnostics: vec![format!("Expected substring '{}' was missing", expected_substring)],
                    }
                }
            }
            Err(e) => VerificationReport {
                success: false,
                diagnostics: vec![format!("Failed to read target file: {}", e)],
            },
        }
    }
}
