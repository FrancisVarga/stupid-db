//! Queue error types.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum QueueError {
    #[error("connection error: {0}")]
    Connection(String),

    #[error("message parse error: {0}")]
    Parse(String),

    #[error("acknowledge error: {0}")]
    Ack(String),

    #[error("timeout after {0}ms")]
    Timeout(u64),

    #[error("queue not found: {0}")]
    NotFound(String),

    #[error("authentication error: {0}")]
    Auth(String),

    #[error("provider error: {0}")]
    Provider(String),
}
