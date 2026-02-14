use thiserror::Error;

#[derive(Error, Debug)]
pub enum StupidError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialize(String),

    #[error("Parquet error: {0}")]
    Parquet(String),

    #[error("Segment not found: {0}")]
    SegmentNotFound(String),

    #[error("Document not found at offset {0}")]
    DocumentNotFound(u64),

    #[error("Node not found: {0}")]
    NodeNotFound(String),

    #[error("Storage error: {0}")]
    Storage(String),

    #[error("{0}")]
    Other(String),
}
