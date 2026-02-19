//! Error types and load result structures for the rule loader.

use std::path::PathBuf;

/// Errors that can occur during rule loading and management.
#[derive(Debug, thiserror::Error)]
pub enum RuleError {
    /// Filesystem I/O error.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// YAML parse/deserialization error.
    #[error("YAML parse error: {0}")]
    Parse(#[from] serde_yaml::Error),

    /// Rule validation error (e.g. missing required fields, duplicate IDs).
    #[error("Validation error: {0}")]
    Validation(String),

    /// Filesystem watcher error.
    #[error("Notify watcher error: {0}")]
    Notify(#[from] notify::Error),
}

/// Result alias for rule operations.
pub type Result<T> = std::result::Result<T, RuleError>;

/// Outcome of loading a single rule file.
#[derive(Debug)]
pub struct LoadResult {
    /// Path to the file that was loaded.
    pub path: PathBuf,
    /// Status of the load attempt.
    pub status: LoadStatus,
}

/// Status of a single file load attempt.
#[derive(Debug)]
pub enum LoadStatus {
    /// Rule was successfully loaded.
    Loaded { rule_id: String },
    /// File was skipped (dotfile, non-YAML, etc.).
    Skipped { reason: String },
    /// Parse or validation error occurred.
    Failed { error: String },
}
