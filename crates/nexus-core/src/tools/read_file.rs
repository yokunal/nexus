use std::fs;
use std::path::Path;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ReadError {
    #[error("File not found: {0}")]
    NotFound(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub fn read_file_lines(
    file_path: &Path,
    start_line: Option<usize>,
    end_line: Option<usize>,
) -> Result<String, ReadError> {
    if !file_path.exists() {
        return Err(ReadError::NotFound(file_path.display().to_string()));
    }

    let content = fs::read_to_string(file_path)?;
    let lines: Vec<&str> = content.lines().collect();

    let start = start_line.unwrap_or(1).saturating_sub(1);
    let end = end_line.unwrap_or(lines.len()).min(lines.len());

    if start >= lines.len() {
        return Ok(String::new());
    }

    let selected: Vec<String> = lines[start..end]
        .iter()
        .enumerate()
        .map(|(idx, line)| format!("{:4}: {}", start + idx + 1, line))
        .collect();

    Ok(selected.join("\n"))
}
