use nexus_lsp::{LspClient, LspError};
use std::path::Path;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::Mutex;

#[derive(Error, Debug)]
pub enum ToolLspError {
    #[error("LSP error: {0}")]
    Lsp(#[from] LspError),
    #[error("Language server is not running or unavailable: {0}")]
    Unavailable(String),
}

/// Helper to format hover information into markdown for the agent.
pub async fn execute_hover(
    client: Option<&Arc<Mutex<LspClient>>>,
    file_path: &Path,
    line: u32,
    column: u32,
) -> Result<String, ToolLspError> {
    let client_mutex = client.ok_or_else(|| {
        ToolLspError::Unavailable(
            "rust-analyzer is not active. Ensure rust-analyzer is installed.".to_string(),
        )
    })?;

    let client = client_mutex.lock().await;
    match client.hover(file_path, line, column).await? {
        Some(info) if !info.contents.trim().is_empty() => Ok(format!(
            "### LSP Hover Info ({}:{}):\n\n{}",
            line, column, info.contents
        )),
        _ => Ok(format!(
            "No hover documentation available at {}:{}:{}",
            file_path.display(),
            line,
            column
        )),
    }
}

/// Helper to format symbol definition locations.
pub async fn execute_definition(
    client: Option<&Arc<Mutex<LspClient>>>,
    file_path: &Path,
    line: u32,
    column: u32,
) -> Result<String, ToolLspError> {
    let client_mutex = client.ok_or_else(|| {
        ToolLspError::Unavailable(
            "rust-analyzer is not active. Ensure rust-analyzer is installed.".to_string(),
        )
    })?;

    let client = client_mutex.lock().await;
    let locations = client.definition(file_path, line, column).await?;

    if locations.is_empty() {
        return Ok(format!(
            "No definition found for symbol at {}:{}:{}",
            file_path.display(),
            line,
            column
        ));
    }

    let mut out = format!("### Symbol Definition(s) (found {}):\n", locations.len());
    for (i, loc) in locations.iter().enumerate() {
        out.push_str(&format!(
            "{}. `{}` (lines {}:{}-{}:{})\n",
            i + 1,
            loc.uri,
            loc.range.start.line + 1,
            loc.range.start.character,
            loc.range.end.line + 1,
            loc.range.end.character
        ));
    }
    Ok(out)
}

/// Helper to format document symbols into an outline tree.
pub async fn execute_symbols(
    client: Option<&Arc<Mutex<LspClient>>>,
    file_path: &Path,
) -> Result<String, ToolLspError> {
    let client_mutex = client.ok_or_else(|| {
        ToolLspError::Unavailable(
            "rust-analyzer is not active. Ensure rust-analyzer is installed.".to_string(),
        )
    })?;

    let client = client_mutex.lock().await;
    let symbols = client.document_symbols(file_path).await?;

    if symbols.is_empty() {
        return Ok(format!("No symbols found in {}", file_path.display()));
    }

    let mut out = format!(
        "### Document Symbols in `{}` ({} items):\n",
        file_path.display(),
        symbols.len()
    );
    for s in symbols {
        let container = s
            .container_name
            .as_ref()
            .map(|c| format!(" (in {})", c))
            .unwrap_or_default();
        out.push_str(&format!(
            "- **[{}]** `{}`{}\n",
            s.kind_string(),
            s.name,
            container
        ));
    }
    Ok(out)
}
