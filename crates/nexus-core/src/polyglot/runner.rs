use super::detector::{PolyglotDetector, PolyglotLanguage};
use super::diagnostic::{PolyglotCheckResult, PolyglotDiagnostic};
use crate::rust_specialist::{CheckMode, RustCheckRunner};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use thiserror::Error;
use tokio::process::Command;

#[derive(Error, Debug)]
pub enum PolyglotError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Runner error: {0}")]
    Runner(String),
}

pub struct PolyglotDoctor;

impl PolyglotDoctor {
    pub async fn run_check(workspace_dir: &Path) -> Result<PolyglotCheckResult, PolyglotError> {
        let language = PolyglotDetector::detect(workspace_dir);
        match language {
            PolyglotLanguage::Rust => Self::check_rust(workspace_dir).await,
            PolyglotLanguage::Python => Self::check_python(workspace_dir).await,
            PolyglotLanguage::JavaScript | PolyglotLanguage::TypeScript => {
                Self::check_javascript(workspace_dir, language).await
            }
            PolyglotLanguage::C | PolyglotLanguage::Cpp => {
                Self::check_c_cpp(workspace_dir, language).await
            }
            PolyglotLanguage::Go => Self::check_go(workspace_dir).await,
            PolyglotLanguage::Unknown => Ok(PolyglotCheckResult {
                language: PolyglotLanguage::Unknown,
                success: true,
                diagnostics: Vec::new(),
                raw_output: "No recognized language project files found.".to_string(),
            }),
        }
    }

    async fn check_rust(workspace_dir: &Path) -> Result<PolyglotCheckResult, PolyglotError> {
        let rust_res = RustCheckRunner::run_check(workspace_dir, CheckMode::Check)
            .await
            .map_err(|e| PolyglotError::Runner(e.to_string()))?;

        let mut diagnostics = Vec::new();
        for d in &rust_res.diagnostics {
            let primary_span = d.spans.iter().find(|s| s.is_primary);
            let file = primary_span
                .map(|s| s.file_name.clone())
                .unwrap_or_else(|| "unknown".to_string());
            let line = primary_span.map(|s| s.line_start);
            let column = primary_span.map(|s| s.column_start);

            let suggestion = d.tutor_advice.clone().or_else(|| {
                primary_span.and_then(|s| s.suggested_replacement.clone())
            });

            diagnostics.push(PolyglotDiagnostic {
                file,
                line,
                column,
                level: d.level.clone(),
                message: d.message.clone(),
                code: d.code.as_ref().map(|c| c.code.clone()),
                suggestion,
            });
        }

        Ok(PolyglotCheckResult {
            language: PolyglotLanguage::Rust,
            success: rust_res.success,
            diagnostics,
            raw_output: RustCheckRunner::format_report(&rust_res),
        })
    }

    async fn check_python(workspace_dir: &Path) -> Result<PolyglotCheckResult, PolyglotError> {
        let py_files = Self::collect_files(workspace_dir, &["py"]);
        if py_files.is_empty() {
            return Ok(PolyglotCheckResult {
                language: PolyglotLanguage::Python,
                success: true,
                diagnostics: Vec::new(),
                raw_output: "No python source files found.".to_string(),
            });
        }

        // 1. Syntax check via python3 -m py_compile
        let mut compile_cmd = Command::new("python3");
        compile_cmd.arg("-m").arg("py_compile");
        for f in &py_files {
            compile_cmd.arg(f);
        }
        compile_cmd
            .current_dir(workspace_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let compile_out = compile_cmd.output().await?;
        if !compile_out.status.success() {
            let stderr = String::from_utf8_lossy(&compile_out.stderr).to_string();
            let diags = Self::parse_python_syntax_errors(&stderr);
            return Ok(PolyglotCheckResult {
                language: PolyglotLanguage::Python,
                success: false,
                diagnostics: diags,
                raw_output: stderr,
            });
        }

        // 2. Test runner check if test files exist
        let has_tests = py_files
            .iter()
            .any(|f| f.to_string_lossy().contains("test"));
        if has_tests {
            let mut test_cmd = Command::new("python3");
            test_cmd
                .arg("-m")
                .arg("unittest")
                .arg("discover")
                .arg("-s")
                .arg(".")
                .arg("-p")
                .arg("*test*.py")
                .current_dir(workspace_dir)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());

            let test_out = test_cmd.output().await?;
            let stdout = String::from_utf8_lossy(&test_out.stdout).to_string();
            let stderr = String::from_utf8_lossy(&test_out.stderr).to_string();
            let combined = format!("{}\n{}", stdout, stderr);

            if !test_out.status.success() {
                let diags = Self::parse_python_test_failures(&combined);
                return Ok(PolyglotCheckResult {
                    language: PolyglotLanguage::Python,
                    success: false,
                    diagnostics: diags,
                    raw_output: combined,
                });
            }
        }

        Ok(PolyglotCheckResult {
            language: PolyglotLanguage::Python,
            success: true,
            diagnostics: Vec::new(),
            raw_output: "All Python syntax and tests passed cleanly.".to_string(),
        })
    }

    async fn check_javascript(
        workspace_dir: &Path,
        lang: PolyglotLanguage,
    ) -> Result<PolyglotCheckResult, PolyglotError> {
        let js_files = Self::collect_files(workspace_dir, &["js", "mjs", "cjs"]);

        // 1. Syntax check via node --check
        for f in &js_files {
            let mut chk = Command::new("node");
            chk.arg("--check")
                .arg(f)
                .current_dir(workspace_dir)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            let out = chk.output().await?;
            if !out.status.success() {
                let stderr = String::from_utf8_lossy(&out.stderr).to_string();
                let diags = Self::parse_node_syntax_error(&stderr, f);
                return Ok(PolyglotCheckResult {
                    language: lang,
                    success: false,
                    diagnostics: diags,
                    raw_output: stderr,
                });
            }
        }

        // 2. Test runner check: if test files exist, run node --test
        let has_tests = js_files
            .iter()
            .any(|f| f.to_string_lossy().contains("test"));
        if has_tests {
            let mut test_cmd = Command::new("node");
            test_cmd
                .arg("--test")
                .current_dir(workspace_dir)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());

            let test_out = test_cmd.output().await?;
            let stdout = String::from_utf8_lossy(&test_out.stdout).to_string();
            let stderr = String::from_utf8_lossy(&test_out.stderr).to_string();
            let combined = format!("{}\n{}", stdout, stderr);

            if !test_out.status.success() {
                let diags = Self::parse_node_test_failures(&combined);
                return Ok(PolyglotCheckResult {
                    language: lang,
                    success: false,
                    diagnostics: diags,
                    raw_output: combined,
                });
            }
        }

        Ok(PolyglotCheckResult {
            language: lang,
            success: true,
            diagnostics: Vec::new(),
            raw_output: "All JavaScript syntax and test suites passed cleanly.".to_string(),
        })
    }

    async fn check_c_cpp(
        workspace_dir: &Path,
        lang: PolyglotLanguage,
    ) -> Result<PolyglotCheckResult, PolyglotError> {
        // If Makefile exists with check/test target, invoke make check
        if workspace_dir.join("Makefile").exists() {
            let mut mk = Command::new("make");
            mk.arg("-n")
                .arg("check")
                .current_dir(workspace_dir)
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            if let Ok(st) = mk.status().await {
                if st.success() {
                    let mut run_mk = Command::new("make");
                    run_mk
                        .arg("check")
                        .current_dir(workspace_dir)
                        .stdout(Stdio::piped())
                        .stderr(Stdio::piped());
                    let out = run_mk.output().await?;
                    let combined = format!(
                        "{}\n{}",
                        String::from_utf8_lossy(&out.stdout),
                        String::from_utf8_lossy(&out.stderr)
                    );
                    if !out.status.success() {
                        let diags = Self::parse_gcc_diagnostics(&combined);
                        return Ok(PolyglotCheckResult {
                            language: lang,
                            success: false,
                            diagnostics: diags,
                            raw_output: combined,
                        });
                    }
                }
            }
        }

        // Check syntax with gcc -fsyntax-only
        let c_files = Self::collect_files(workspace_dir, &["c", "cpp", "cc"]);
        if !c_files.is_empty() {
            let compiler = if lang == PolyglotLanguage::Cpp { "g++" } else { "gcc" };
            let mut comp_cmd = Command::new(compiler);
            comp_cmd
                .arg("-fsyntax-only")
                .arg("-Wall")
                .arg("-Wextra");
            for f in &c_files {
                comp_cmd.arg(f);
            }
            comp_cmd
                .current_dir(workspace_dir)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());

            let comp_out = comp_cmd.output().await?;
            let stderr = String::from_utf8_lossy(&comp_out.stderr).to_string();
            if !comp_out.status.success() {
                let diags = Self::parse_gcc_diagnostics(&stderr);
                return Ok(PolyglotCheckResult {
                    language: lang,
                    success: false,
                    diagnostics: diags,
                    raw_output: stderr,
                });
            }
        }

        Ok(PolyglotCheckResult {
            language: lang,
            success: true,
            diagnostics: Vec::new(),
            raw_output: "All C/C++ syntax and compiler gates passed cleanly.".to_string(),
        })
    }

    async fn check_go(workspace_dir: &Path) -> Result<PolyglotCheckResult, PolyglotError> {
        let mut cmd = Command::new("go");
        cmd.arg("vet")
            .arg("./...")
            .current_dir(workspace_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let out = cmd.output().await?;
        let stderr = String::from_utf8_lossy(&out.stderr).to_string();
        if !out.status.success() {
            return Ok(PolyglotCheckResult {
                language: PolyglotLanguage::Go,
                success: false,
                diagnostics: vec![PolyglotDiagnostic {
                    file: "go.mod".to_string(),
                    line: None,
                    column: None,
                    level: "error".to_string(),
                    message: stderr.clone(),
                    code: None,
                    suggestion: None,
                }],
                raw_output: stderr,
            });
        }

        Ok(PolyglotCheckResult {
            language: PolyglotLanguage::Go,
            success: true,
            diagnostics: Vec::new(),
            raw_output: "Go vet passed cleanly.".to_string(),
        })
    }

    fn collect_files(dir: &Path, exts: &[&str]) -> Vec<PathBuf> {
        let mut list = Vec::new();
        Self::walk_dir(dir, exts, &mut list, 0);
        list
    }

    fn walk_dir(dir: &Path, exts: &[&str], out: &mut Vec<PathBuf>, depth: usize) {
        if depth > 4 {
            return;
        }
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_file() {
                    if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
                        if exts.iter().any(|&e| e.eq_ignore_ascii_case(ext)) {
                            out.push(p);
                        }
                    }
                } else if p.is_dir() {
                    let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    if name != ".git"
                        && name != "node_modules"
                        && name != "target"
                        && name != ".venv"
                        && name != "venv"
                        && name != "__pycache__"
                    {
                        Self::walk_dir(&p, exts, out, depth + 1);
                    }
                }
            }
        }
    }

    fn parse_python_syntax_errors(stderr: &str) -> Vec<PolyglotDiagnostic> {
        let mut diags = Vec::new();
        let mut cur_file = "unknown".to_string();
        let mut cur_line = None;

        for line in stderr.lines() {
            if let Some(idx) = line.find("File \"") {
                let rest = &line[idx + 6..];
                if let Some(end_quote) = rest.find('"') {
                    cur_file = rest[..end_quote].to_string();
                    if let Some(line_kw) = rest.find("line ") {
                        let num_part = rest[line_kw + 5..]
                            .chars()
                            .take_while(|c| c.is_ascii_digit())
                            .collect::<String>();
                        cur_line = num_part.parse::<usize>().ok();
                    }
                }
            } else if line.contains("SyntaxError:") || line.contains("IndentationError:") {
                diags.push(PolyglotDiagnostic {
                    file: cur_file.clone(),
                    line: cur_line,
                    column: None,
                    level: "error".to_string(),
                    message: line.trim().to_string(),
                    code: Some("PYTHON_SYNTAX".to_string()),
                    suggestion: Some(format!("Check syntax or indentation near line {:?}", cur_line)),
                });
            }
        }

        if diags.is_empty() && !stderr.trim().is_empty() {
            diags.push(PolyglotDiagnostic {
                file: cur_file,
                line: cur_line,
                column: None,
                level: "error".to_string(),
                message: stderr.trim().lines().take(3).collect::<Vec<_>>().join(" "),
                code: None,
                suggestion: None,
            });
        }

        diags
    }

    fn parse_python_test_failures(output: &str) -> Vec<PolyglotDiagnostic> {
        let mut diags = Vec::new();
        for line in output.lines() {
            if line.starts_with("FAIL:") || line.starts_with("ERROR:") {
                diags.push(PolyglotDiagnostic {
                    file: "test_runner".to_string(),
                    line: None,
                    column: None,
                    level: "error".to_string(),
                    message: line.trim().to_string(),
                    code: Some("TEST_FAILURE".to_string()),
                    suggestion: Some("Review test assertion or runtime exception.".to_string()),
                });
            }
        }
        if diags.is_empty() && output.contains("FAILED") {
            diags.push(PolyglotDiagnostic {
                file: "tests".to_string(),
                line: None,
                column: None,
                level: "error".to_string(),
                message: "One or more Python unit tests failed.".to_string(),
                code: Some("TEST_FAILURE".to_string()),
                suggestion: None,
            });
        }
        diags
    }

    fn parse_node_syntax_error(stderr: &str, file_path: &Path) -> Vec<PolyglotDiagnostic> {
        let mut line_num = None;
        let mut msg = "Syntax error".to_string();

        for line in stderr.lines() {
            if line.contains("SyntaxError:") {
                msg = line.trim().to_string();
            }
            if line.contains(':') {
                let parts: Vec<&str> = line.split(':').collect();
                if parts.len() >= 2 {
                    if let Ok(num) = parts[1].trim().parse::<usize>() {
                        line_num = Some(num);
                    }
                }
            }
        }

        vec![PolyglotDiagnostic {
            file: file_path.display().to_string(),
            line: line_num,
            column: None,
            level: "error".to_string(),
            message: msg,
            code: Some("NODE_SYNTAX".to_string()),
            suggestion: Some("Correct JavaScript syntax error near specified line.".to_string()),
        }]
    }

    fn parse_node_test_failures(output: &str) -> Vec<PolyglotDiagnostic> {
        let mut diags = Vec::new();
        for line in output.lines() {
            if line.contains("not ok") || line.contains("ERR_ASSERTION") || line.contains("AssertionError") {
                diags.push(PolyglotDiagnostic {
                    file: "node_test".to_string(),
                    line: None,
                    column: None,
                    level: "error".to_string(),
                    message: line.trim().to_string(),
                    code: Some("TEST_FAILURE".to_string()),
                    suggestion: Some("Inspect assertion expectation in test spec.".to_string()),
                });
            }
        }
        if diags.is_empty() && output.contains("fail") {
            diags.push(PolyglotDiagnostic {
                file: "node_test".to_string(),
                line: None,
                column: None,
                level: "error".to_string(),
                message: "Node test suite failed.".to_string(),
                code: Some("TEST_FAILURE".to_string()),
                suggestion: None,
            });
        }
        diags
    }

    fn parse_gcc_diagnostics(output: &str) -> Vec<PolyglotDiagnostic> {
        let mut diags = Vec::new();
        for line in output.lines() {
            // Format: file:line:col: error: message
            let parts: Vec<&str> = line.splitn(5, ':').collect();
            if parts.len() >= 4 {
                let file = parts[0].trim().to_string();
                let line_num = parts[1].trim().parse::<usize>().ok();
                let col_num = parts[2].trim().parse::<usize>().ok();
                let rest = parts[3..].join(":");
                let level = if rest.contains("error:") {
                    "error".to_string()
                } else if rest.contains("warning:") {
                    "warning".to_string()
                } else {
                    continue;
                };

                diags.push(PolyglotDiagnostic {
                    file,
                    line: line_num,
                    column: col_num,
                    level,
                    message: rest.trim().to_string(),
                    code: Some("GCC_DIAGNOSTIC".to_string()),
                    suggestion: None,
                });
            }
        }
        diags
    }

    pub fn format_report(result: &PolyglotCheckResult) -> String {
        if result.success {
            format!(
                "╭─ 🩺 Nexus Polyglot Doctor: {} Diagnostics ─────────────────\n│ Status: Healthy (0 errors, 0 warnings)\n╰─ ⟡ All compiler syntax and test gates passing cleanly!\n",
                result.language
            )
        } else {
            let mut out = format!(
                "╭─ 🩺 Nexus Polyglot Doctor: {} Diagnostics ─────────────────\n│ Status: Failed ({} diagnostic issue(s) detected)\n│\n",
                result.language,
                result.diagnostics.len()
            );

            for (i, d) in result.diagnostics.iter().enumerate() {
                let loc = match (d.line, d.column) {
                    (Some(l), Some(c)) => format!("{}:{}:{}", d.file, l, c),
                    (Some(l), None) => format!("{}:{}", d.file, l),
                    _ => d.file.clone(),
                };
                let tag = if d.level == "warning" { "\x1b[1;33m[WARN]\x1b[0m" } else { "\x1b[1;31m[ERROR]\x1b[0m" };
                out.push_str(&format!("│ {}. {} {} - {}\n", i + 1, tag, loc, d.message));
                if let Some(ref sugg) = d.suggestion {
                    out.push_str(&format!("│    \x1b[1;32mAdvice:\x1b[0m {}\n", sugg));
                }
            }

            out.push_str("╰────────────────────────────────────────────────────────────\n");
            out
        }
    }
}
