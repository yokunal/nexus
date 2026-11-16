use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolyglotDiagnostic {
    pub file: String,
    pub line: Option<usize>,
    pub column: Option<usize>,
    pub level: String, // "error", "warning"
    pub message: String,
    pub code: Option<String>,
    pub suggestion: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolyglotCheckResult {
    pub language: crate::polyglot::PolyglotLanguage,
    pub success: bool,
    pub diagnostics: Vec<PolyglotDiagnostic>,
    pub raw_output: String,
}
