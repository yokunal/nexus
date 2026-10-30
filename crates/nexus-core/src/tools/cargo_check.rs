use crate::rust_specialist::{CheckMode, RustCheckRunner};
use std::path::Path;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum CargoCheckError {
    #[error("Runner error: {0}")]
    Runner(#[from] crate::rust_specialist::runner::RunnerError),
}

pub async fn execute_cargo_check(
    workspace_root: &Path,
    mode_str: Option<&str>,
) -> Result<String, CargoCheckError> {
    let mode = match mode_str {
        Some("clippy") => CheckMode::Clippy,
        Some("test") => CheckMode::TestNoRun,
        _ => CheckMode::Check,
    };

    let result = RustCheckRunner::run_check(workspace_root, mode).await?;
    Ok(RustCheckRunner::format_report(&result))
}
