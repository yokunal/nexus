use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpanLine {
    pub text: String,
    pub highlight_start: usize,
    pub highlight_end: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticSpan {
    pub file_name: String,
    pub byte_start: usize,
    pub byte_end: usize,
    pub line_start: usize,
    pub line_end: usize,
    pub column_start: usize,
    pub column_end: usize,
    pub is_primary: bool,
    pub text: Vec<SpanLine>,
    pub label: Option<String>,
    pub suggested_replacement: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticCode {
    pub code: String,
    pub explanation: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RustDiagnostic {
    pub message: String,
    pub code: Option<DiagnosticCode>,
    pub level: String, // "error", "warning", "note", "help"
    pub spans: Vec<DiagnosticSpan>,
    pub children: Vec<RustDiagnosticChild>,
    pub rendered: Option<String>,
    pub tutor_advice: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RustDiagnosticChild {
    pub message: String,
    pub level: String,
    pub spans: Vec<DiagnosticSpan>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckResult {
    pub success: bool,
    pub error_count: usize,
    pub warning_count: usize,
    pub diagnostics: Vec<RustDiagnostic>,
}

impl RustDiagnostic {
    pub fn is_error(&self) -> bool {
        self.level == "error"
    }

    pub fn is_borrow_or_lifetime(&self) -> bool {
        if let Some(c) = &self.code {
            matches!(
                c.code.as_str(),
                "E0382"
                    | "E0499"
                    | "E0502"
                    | "E0503"
                    | "E0505"
                    | "E0506"
                    | "E0515"
                    | "E0597"
                    | "E0716"
                    | "E0621"
                    | "E0106"
                    | "E0310"
                    | "E0477"
            )
        } else {
            false
        }
    }
}
