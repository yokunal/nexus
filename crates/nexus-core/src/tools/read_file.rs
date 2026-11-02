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
    let default_end = if start_line.is_none() && end_line.is_none() && lines.len() > 400 {
        400
    } else {
        lines.len()
    };
    let end = end_line.unwrap_or(default_end).min(lines.len());

    if start >= lines.len() {
        return Ok(String::new());
    }

    let mut selected: Vec<String> = lines[start..end]
        .iter()
        .enumerate()
        .map(|(idx, line)| format!("{:4}: {}", start + idx + 1, line))
        .collect();

    if start_line.is_none() && end_line.is_none() && lines.len() > 400 {
        selected.push(format!(
            "\n... [Truncated for token efficiency: lines 1-400 of {} shown. Use start_line and end_line to inspect specific sections]",
            lines.len()
        ));
    }

    Ok(selected.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_file_truncation() {
        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join(format!("test_read_truncation_{}.txt", uuid::Uuid::new_v4()));
        let content: Vec<String> = (1..=600).map(|i| format!("line {}", i)).collect();
        fs::write(&file_path, content.join("\n")).unwrap();

        let read = read_file_lines(&file_path, None, None).unwrap();
        assert!(read.contains("line 1"));
        assert!(read.contains("line 400"));
        assert!(!read.contains(" 401: line 401"));
        assert!(read.contains("Truncated for token efficiency"));

        let _ = fs::remove_file(&file_path);
    }
}
