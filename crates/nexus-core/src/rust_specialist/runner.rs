use super::diagnostic::{CheckResult, DiagnosticCode, DiagnosticSpan, RustDiagnostic, SpanLine};
use super::tutor::BorrowTutor;
use std::path::Path;
use std::process::Stdio;
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

#[derive(Error, Debug)]
pub enum RunnerError {
    #[error("IO error executing cargo: {0}")]
    Io(#[from] std::io::Error),
    #[error("Failed to parse diagnostic JSON: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckMode {
    Check,
    Clippy,
    TestNoRun,
}

pub struct RustCheckRunner;

impl RustCheckRunner {
    pub async fn run_check(
        working_dir: &Path,
        mode: CheckMode,
    ) -> Result<CheckResult, RunnerError> {
        let mut cmd = Command::new("cargo");

        match mode {
            CheckMode::Check => {
                cmd.arg("check").arg("--message-format=json");
            }
            CheckMode::Clippy => {
                cmd.arg("clippy").arg("--message-format=json");
            }
            CheckMode::TestNoRun => {
                cmd.arg("test").arg("--no-run").arg("--message-format=json");
            }
        }

        cmd.current_dir(working_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = cmd.spawn()?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| std::io::Error::other("Failed to capture stdout"))?;

        let mut reader = BufReader::new(stdout).lines();
        let mut diagnostics = Vec::new();
        let mut error_count = 0;
        let mut warning_count = 0;
        let mut build_success = true;

        while let Some(line) = reader.next_line().await? {
            let value: serde_json::Value = match serde_json::from_str(&line) {
                Ok(v) => v,
                Err(_) => continue,
            };

            let reason = value["reason"].as_str().unwrap_or_default();

            if reason == "compiler-message" {
                if let Some(msg_val) = value.get("message") {
                    let level = msg_val["level"].as_str().unwrap_or("error").to_string();

                    // Only retain errors and warnings to save context tokens
                    if level != "error" && level != "warning" {
                        continue;
                    }

                    if level == "error" {
                        error_count += 1;
                        build_success = false;
                    } else if level == "warning" {
                        warning_count += 1;
                    }

                    let message = msg_val["message"].as_str().unwrap_or_default().to_string();
                    let rendered = msg_val["rendered"].as_str().map(|s| s.to_string());

                    let code = msg_val.get("code").and_then(|c| {
                        let code_str = c["code"].as_str()?;
                        let explanation = c["explanation"].as_str().map(|s| s.to_string());
                        Some(DiagnosticCode {
                            code: code_str.to_string(),
                            explanation,
                        })
                    });

                    let mut spans = Vec::new();
                    if let Some(spans_arr) = msg_val["spans"].as_array() {
                        for s in spans_arr {
                            let file_name = s["file_name"].as_str().unwrap_or_default().to_string();
                            let byte_start = s["byte_start"].as_u64().unwrap_or(0) as usize;
                            let byte_end = s["byte_end"].as_u64().unwrap_or(0) as usize;
                            let line_start = s["line_start"].as_u64().unwrap_or(0) as usize;
                            let line_end = s["line_end"].as_u64().unwrap_or(0) as usize;
                            let column_start = s["column_start"].as_u64().unwrap_or(0) as usize;
                            let column_end = s["column_end"].as_u64().unwrap_or(0) as usize;
                            let is_primary = s["is_primary"].as_bool().unwrap_or(false);
                            let label = s["label"].as_str().map(|l| l.to_string());
                            let suggested_replacement =
                                s["suggested_replacement"].as_str().map(|r| r.to_string());

                            let mut text_lines = Vec::new();
                            if let Some(text_arr) = s["text"].as_array() {
                                for t in text_arr {
                                    text_lines.push(SpanLine {
                                        text: t["text"].as_str().unwrap_or_default().to_string(),
                                        highlight_start: t["highlight_start"].as_u64().unwrap_or(0)
                                            as usize,
                                        highlight_end: t["highlight_end"].as_u64().unwrap_or(0)
                                            as usize,
                                    });
                                }
                            }

                            spans.push(DiagnosticSpan {
                                file_name,
                                byte_start,
                                byte_end,
                                line_start,
                                line_end,
                                column_start,
                                column_end,
                                is_primary,
                                text: text_lines,
                                label,
                                suggested_replacement,
                            });
                        }
                    }

                    let mut diag = RustDiagnostic {
                        message,
                        code,
                        level,
                        spans,
                        children: Vec::new(),
                        rendered,
                        tutor_advice: None,
                    };

                    // Enrich with BorrowTutor advice
                    diag.tutor_advice = BorrowTutor::advise(&diag);
                    diagnostics.push(diag);
                }
            } else if reason == "build-finished" {
                if let Some(success) = value["success"].as_bool() {
                    build_success = success;
                }
            }
        }

        let _ = child.wait().await?;

        Ok(CheckResult {
            success: build_success && error_count == 0,
            error_count,
            warning_count,
            diagnostics,
        })
    }

    /// Formats the check result into a clean, concise, high-signal diagnostic report.
    pub fn format_report(result: &CheckResult) -> String {
        if result.success {
            if result.warning_count == 0 {
                return "✅ Rust Check Passed: 0 errors, 0 warnings. Code is clean and compiles smoothly.".to_string();
            } else {
                return format!(
                    "⚠️ Rust Check Passed with {} warning(s):\n{}",
                    result.warning_count,
                    Self::format_diagnostics(&result.diagnostics)
                );
            }
        }

        format!(
            "❌ Rust Check FAILED: {} error(s), {} warning(s):\n{}",
            result.error_count,
            result.warning_count,
            Self::format_diagnostics(&result.diagnostics)
        )
    }

    fn format_diagnostics(diagnostics: &[RustDiagnostic]) -> String {
        let mut out = String::new();

        for (i, d) in diagnostics.iter().enumerate() {
            let code_str = d
                .code
                .as_ref()
                .map(|c| format!(" [{}]", c.code))
                .unwrap_or_default();
            out.push_str(&format!(
                "\n--- Diagnostic #{}: {}{} ---\nMessage: {}\n",
                i + 1,
                d.level.to_uppercase(),
                code_str,
                d.message
            ));

            for span in &d.spans {
                let primary_tag = if span.is_primary { " (primary)" } else { "" };
                let label_tag = span
                    .label
                    .as_ref()
                    .map(|l| format!(" -> {}", l))
                    .unwrap_or_default();
                out.push_str(&format!(
                    "  --> {}:{}:{}{}{}\n",
                    span.file_name, span.line_start, span.column_start, primary_tag, label_tag
                ));

                for line in &span.text {
                    out.push_str(&format!("      {}\n", line.text));
                }
            }

            if let Some(advice) = &d.tutor_advice {
                out.push_str(&format!("\n{}\n", advice));
            }
        }

        out
    }
}
