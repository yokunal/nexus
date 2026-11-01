use crate::rea::{ReaBridge, ReaError};
use std::path::Path;

pub fn execute_rea_inspect(binary_path: &Path, mode: &str) -> Result<String, ReaError> {
    ReaBridge::inspect_binary(binary_path, mode)
}
