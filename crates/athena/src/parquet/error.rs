//! Error types for Parquet conversion.

/// Errors that can occur during Parquet conversion or writing.
#[derive(Debug, thiserror::Error)]
pub enum ParquetError {
    /// Failed to build Arrow arrays from result data.
    #[error("Arrow conversion error: {0}")]
    Arrow(#[from] arrow::error::ArrowError),

    /// Failed to write Parquet file.
    #[error("Parquet write error: {0}")]
    Write(#[from] parquet::errors::ParquetError),

    /// I/O error when creating/writing the output file.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}
