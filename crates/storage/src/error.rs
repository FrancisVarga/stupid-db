use thiserror::Error;

#[derive(Error, Debug)]
pub enum StorageError {
    #[error("object store error: {0}")]
    ObjectStore(#[from] object_store::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("core error: {0}")]
    Core(#[from] stupid_core::StupidError),

    #[error("parquet error: {0}")]
    Parquet(String),

    #[error("not configured: {0}")]
    NotConfigured(String),

    #[error("{0}")]
    Other(String),
}
