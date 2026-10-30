use std::fs;
use std::path::Path;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum WriteError {
    #[error("Failed to create parent directories: {0}")]
    ParentDir(std::io::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub fn write_file_content(file_path: &Path, content: &str) -> Result<(), WriteError> {
    if let Some(parent) = file_path.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent).map_err(WriteError::ParentDir)?;
        }
    }

    fs::write(file_path, content)?;
    Ok(())
}
