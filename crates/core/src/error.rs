use thiserror::Error;

/// Domain errors shared across all SYNAPSEED crates.
#[derive(Debug, Error)]
pub enum SynapseedError {
    #[error("Parse error in '{file}': {reason}")]
    Parse { file: String, reason: String },

    #[error("Symbol not found: {0}")]
    SymbolNotFound(String),

    #[error("Security violation: {0}")]
    SecurityViolation(String),

    #[error("Policy denied command: {command}")]
    PolicyDenied { command: String },

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("{0}")]
    Internal(String),
}

pub type Result<T> = std::result::Result<T, SynapseedError>;
