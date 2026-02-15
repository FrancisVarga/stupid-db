//! Queue consumer trait and types.

use std::fmt;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::QueueError;

/// A raw message received from a queue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueMessage {
    /// Unique message identifier from the queue provider.
    pub id: String,
    /// Raw message body (JSON string).
    pub body: String,
    /// Provider-specific handle for ack/nack (e.g., SQS receipt handle).
    pub receipt_handle: String,
    /// When the message was sent to the queue.
    pub timestamp: DateTime<Utc>,
    /// Number of times this message has been received (for retry tracking).
    pub attempt_count: u32,
}

/// Health status of a queue connection.
#[derive(Debug, Clone, Serialize)]
pub struct QueueHealth {
    /// Whether the queue is reachable.
    pub connected: bool,
    /// Approximate number of messages waiting in the queue.
    pub approximate_message_count: Option<u64>,
    /// Queue provider name (e.g., "sqs", "redis").
    pub provider: String,
}

impl fmt::Display for QueueHealth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "QueueHealth {{ connected: {}, messages: {:?}, provider: {} }}",
            self.connected, self.approximate_message_count, self.provider
        )
    }
}

/// Trait for queue consumer backends.
///
/// Implementations handle the specifics of polling, acknowledging, and
/// managing messages for a particular queue provider (SQS, Redis, MQTT).
#[async_trait]
pub trait QueueConsumer: Send + Sync {
    /// Poll up to `max_messages` from the queue.
    ///
    /// May block for up to the provider's long-poll timeout (e.g., 20s for SQS).
    /// Returns an empty vec if no messages are available.
    async fn poll_batch(&self, max_messages: u32) -> Result<Vec<QueueMessage>, QueueError>;

    /// Acknowledge successful processing — removes the message from the queue.
    async fn ack(&self, receipt_handle: &str) -> Result<(), QueueError>;

    /// Negative-acknowledge — returns the message to the queue for retry.
    ///
    /// For SQS: sets visibility timeout to 0 so message is immediately available.
    async fn nack(&self, receipt_handle: &str) -> Result<(), QueueError>;

    /// Check queue connectivity and return health status.
    async fn health_check(&self) -> Result<QueueHealth, QueueError>;

    /// Get approximate depth of the dead-letter queue (if configured).
    async fn dlq_depth(&self) -> Result<Option<u64>, QueueError> {
        Ok(None) // Default: DLQ not supported
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_queue_message_serde_roundtrip() {
        let msg = QueueMessage {
            id: "msg-123".to_string(),
            body: r#"{"event_type":"Login","memberCode":"M001"}"#.to_string(),
            receipt_handle: "handle-abc".to_string(),
            timestamp: Utc::now(),
            attempt_count: 1,
        };

        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: QueueMessage = serde_json::from_str(&json).unwrap();

        assert_eq!(msg.id, deserialized.id);
        assert_eq!(msg.body, deserialized.body);
        assert_eq!(msg.receipt_handle, deserialized.receipt_handle);
        assert_eq!(msg.attempt_count, deserialized.attempt_count);
    }

    #[test]
    fn test_queue_health_display() {
        let health = QueueHealth {
            connected: true,
            approximate_message_count: Some(42),
            provider: "sqs".to_string(),
        };
        let display = format!("{}", health);
        assert!(display.contains("connected: true"));
        assert!(display.contains("42"));
    }

    #[test]
    fn test_queue_message_clone() {
        let msg = QueueMessage {
            id: "msg-456".to_string(),
            body: "{}".to_string(),
            receipt_handle: "handle-xyz".to_string(),
            timestamp: Utc::now(),
            attempt_count: 3,
        };
        let cloned = msg.clone();
        assert_eq!(msg.id, cloned.id);
        assert_eq!(msg.attempt_count, cloned.attempt_count);
    }
}
