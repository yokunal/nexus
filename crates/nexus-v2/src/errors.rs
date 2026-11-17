use thiserror::Error;

#[derive(Error, Debug)]
pub enum NexusV2Error {
    #[error("Plan-Act-Verify loop failed after {max_iterations} iterations: {reason}")]
    PavLoopExhausted { max_iterations: u32, reason: String },

    #[error("Primary and fallback models both failed: primary={primary_err}, fallback={fallback_err}")]
    DualModelFailure { primary_err: String, fallback_err: String },

    #[error("AST Semantic Safety violation: {0}")]
    SafetyViolation(String),

    #[error("Ambiguous patch match detected (count={count}): target requires unique placement")]
    AmbiguousPatch { count: usize },

    #[error("Path containment violation: path {0} escapes workspace root")]
    PathEscape(String),

    #[error("Circular WAL storage error: {0}")]
    StorageError(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}
