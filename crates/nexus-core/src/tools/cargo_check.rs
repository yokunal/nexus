use crate::polyglot::{PolyglotDoctor, PolyglotDetector, PolyglotLanguage};
use crate::rust_specialist::{CheckMode, RustCheckRunner};
use std::path::Path;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum CargoCheckError {
    #[error("Runner error: {0}")]
    Runner(#[from] crate::rust_specialist::runner::RunnerError),
    #[error("Polyglot error: {0}")]
    Polyglot(#[from] crate::polyglot::PolyglotError),
}

pub async fn execute_cargo_check(
    workspace_root: &Path,
    mode_str: Option<&str>,
) -> Result<String, CargoCheckError> {
    if !workspace_root.join("Cargo.toml").exists() {
        let lang = PolyglotDetector::detect(workspace_root);
        if lang != PolyglotLanguage::Unknown {
            let res = PolyglotDoctor::run_check(workspace_root).await?;
            return Ok(PolyglotDoctor::format_report(&res));
        }
    }

    let mode = match mode_str {
        Some("clippy") => CheckMode::Clippy,
        Some("test") => CheckMode::TestNoRun,
        _ => CheckMode::Check,
    };

    let result = RustCheckRunner::run_check(workspace_root, mode).await?;
    Ok(RustCheckRunner::format_report(&result))
}
