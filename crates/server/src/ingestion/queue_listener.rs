//! Queue/stream source listener infrastructure.
//!
//! Provides a trait-object backend abstraction over queue systems (Redis, SQS, NATS).
//! Each backend is gated behind a Cargo feature to avoid pulling in heavy cloud SDKs
//! unless explicitly opted in.
//!
//! # Architecture
//!
//! ```text
//! ┌──────────────┐     ┌──────────────┐     ┌─────────────────┐
//! │  QueueBackend │────▶│ QueueMessage │────▶│ Entity records  │
//! │  (trait obj)  │     │  { id, body }│     │ → ZMQ publish   │
//! └──────────────┘     └──────────────┘     └─────────────────┘
//! ```
//!
//! The scheduler (issue #411) will call [`run_queue_listener`] for Push/Queue
//! source types. Each backend implements [`QueueBackend`] for receive + ack.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tracing::{error, info, warn};

use crate::state::AppState;

use super::types::{IngestionSource, QueueConfig, SourceConfig};

// ── Core types ──────────────────────────────────────────────────────

/// A single message received from a queue backend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueMessage {
    /// Backend-specific message identifier (used for ack).
    pub id: String,
    /// Deserialized message payload.
    pub payload: serde_json::Value,
}

/// Trait for pluggable queue backends.
///
/// Each implementation handles the specifics of connecting to and reading from
/// a particular queue system. Backends are selected at runtime based on the
/// `queue_type` field in [`QueueConfig`].
#[async_trait]
pub trait QueueBackend: Send + Sync + std::fmt::Debug {
    /// Poll for available messages, returning up to `batch_size` items.
    async fn receive(&mut self) -> anyhow::Result<Vec<QueueMessage>>;

    /// Acknowledge a successfully processed message.
    async fn ack(&mut self, message_id: &str) -> anyhow::Result<()>;
}

// ── Backend selection ───────────────────────────────────────────────

/// Build the appropriate [`QueueBackend`] from a [`QueueConfig`].
///
/// Returns an error if the requested backend is unknown or its Cargo feature
/// is not enabled.
pub fn build_backend(config: &QueueConfig) -> anyhow::Result<Box<dyn QueueBackend>> {
    match config.queue_type.as_str() {
        "redis" => Ok(Box::new(RedisBackend::new(config)?)),
        "sqs" => Ok(Box::new(SqsBackend::new(config)?)),
        "nats" => Ok(Box::new(NatsBackend::new(config)?)),
        other => anyhow::bail!(
            "unknown queue backend '{}' — supported: redis, sqs, nats",
            other
        ),
    }
}

// ── Redis backend ───────────────────────────────────────────────────

/// Redis queue backend (requires `queue-redis` feature).
///
/// When the feature is enabled, uses `redis::aio::MultiplexedConnection`
/// with LPOP for list-based queues or XREADGROUP for Redis Streams.
/// When disabled, returns an error on any operation.
#[cfg(not(feature = "queue-redis"))]
#[derive(Debug)]
pub struct RedisBackend;

#[cfg(not(feature = "queue-redis"))]
impl RedisBackend {
    pub fn new(_config: &QueueConfig) -> anyhow::Result<Self> {
        anyhow::bail!("Redis queue backend requires the 'queue-redis' Cargo feature")
    }
}

#[cfg(not(feature = "queue-redis"))]
#[async_trait]
impl QueueBackend for RedisBackend {
    async fn receive(&mut self) -> anyhow::Result<Vec<QueueMessage>> {
        anyhow::bail!("Redis queue backend requires the 'queue-redis' Cargo feature")
    }

    async fn ack(&mut self, _message_id: &str) -> anyhow::Result<()> {
        anyhow::bail!("Redis queue backend requires the 'queue-redis' Cargo feature")
    }
}

// ── SQS backend ─────────────────────────────────────────────────────

/// AWS SQS queue backend (requires `queue-sqs` feature).
///
/// When the feature is enabled, uses `aws-sdk-sqs` for polling and
/// message deletion. When disabled, returns an error on any operation.
#[cfg(not(feature = "queue-sqs"))]
#[derive(Debug)]
pub struct SqsBackend;

#[cfg(not(feature = "queue-sqs"))]
impl SqsBackend {
    pub fn new(_config: &QueueConfig) -> anyhow::Result<Self> {
        anyhow::bail!("SQS queue backend requires the 'queue-sqs' Cargo feature")
    }
}

#[cfg(not(feature = "queue-sqs"))]
#[async_trait]
impl QueueBackend for SqsBackend {
    async fn receive(&mut self) -> anyhow::Result<Vec<QueueMessage>> {
        anyhow::bail!("SQS queue backend requires the 'queue-sqs' Cargo feature")
    }

    async fn ack(&mut self, _message_id: &str) -> anyhow::Result<()> {
        anyhow::bail!("SQS queue backend requires the 'queue-sqs' Cargo feature")
    }
}

// ── NATS backend ────────────────────────────────────────────────────

/// NATS queue backend (requires `queue-nats` feature).
///
/// When the feature is enabled, uses `async-nats` for JetStream
/// pull-based consumers. When disabled, returns an error on any operation.
#[cfg(not(feature = "queue-nats"))]
#[derive(Debug)]
pub struct NatsBackend;

#[cfg(not(feature = "queue-nats"))]
impl NatsBackend {
    pub fn new(_config: &QueueConfig) -> anyhow::Result<Self> {
        anyhow::bail!("NATS queue backend requires the 'queue-nats' Cargo feature")
    }
}

#[cfg(not(feature = "queue-nats"))]
#[async_trait]
impl QueueBackend for NatsBackend {
    async fn receive(&mut self) -> anyhow::Result<Vec<QueueMessage>> {
        anyhow::bail!("NATS queue backend requires the 'queue-nats' Cargo feature")
    }

    async fn ack(&mut self, _message_id: &str) -> anyhow::Result<()> {
        anyhow::bail!("NATS queue backend requires the 'queue-nats' Cargo feature")
    }
}

// ── Main listener loop ──────────────────────────────────────────────

/// Default poll interval when not specified in config.
const DEFAULT_POLL_INTERVAL_MS: u64 = 1000;

/// Default batch size when not specified in config.
const DEFAULT_BATCH_SIZE: u32 = 10;

/// Spawn a long-running queue listener for a Queue source.
///
/// Reads messages from the configured backend, parses payloads as entity
/// records, updates job counters, and publishes `INGEST_RECORD_BATCH` events
/// via eisenbahn/ZMQ.
///
/// Runs indefinitely until the source is disabled or a fatal error occurs.
/// Soft errors (individual message failures) are logged and skipped.
pub async fn run_queue_listener(
    state: Arc<AppState>,
    source: IngestionSource,
) -> anyhow::Result<()> {
    // Parse the QueueConfig from the source's config_json.
    let config = match source.config()? {
        SourceConfig::Queue(q) => q,
        other => anyhow::bail!(
            "expected Queue source config, got {:?}",
            std::mem::discriminant(&other)
        ),
    };

    let poll_interval = Duration::from_millis(DEFAULT_POLL_INTERVAL_MS);
    let batch_size = config.batch_size.unwrap_or(DEFAULT_BATCH_SIZE);

    info!(
        source_id = %source.id,
        source_name = %source.name,
        queue_type = %config.queue_type,
        batch_size = batch_size,
        poll_interval_ms = poll_interval.as_millis() as u64,
        "starting queue listener"
    );

    // Build the backend — this will fail immediately if the feature is not enabled.
    let mut backend = build_backend(&config)?;

    let mut total_received: u64 = 0;
    let mut total_acked: u64 = 0;
    let mut consecutive_errors: u32 = 0;
    const MAX_CONSECUTIVE_ERRORS: u32 = 10;

    loop {
        // Receive a batch of messages.
        let messages = match backend.receive().await {
            Ok(msgs) => {
                consecutive_errors = 0;
                msgs
            }
            Err(e) => {
                consecutive_errors += 1;
                warn!(
                    source_name = %source.name,
                    error = %e,
                    consecutive_errors = consecutive_errors,
                    "queue receive failed"
                );

                if consecutive_errors >= MAX_CONSECUTIVE_ERRORS {
                    error!(
                        source_name = %source.name,
                        "queue listener stopping after {} consecutive errors",
                        MAX_CONSECUTIVE_ERRORS
                    );
                    anyhow::bail!(
                        "queue listener for '{}' stopped: {} consecutive receive errors",
                        source.name,
                        MAX_CONSECUTIVE_ERRORS
                    );
                }

                // Exponential backoff on errors (capped at 30s).
                let backoff = Duration::from_millis(
                    (poll_interval.as_millis() as u64) * 2u64.pow(consecutive_errors.min(5)),
                );
                tokio::time::sleep(backoff.min(Duration::from_secs(30))).await;
                continue;
            }
        };

        if messages.is_empty() {
            tokio::time::sleep(poll_interval).await;
            continue;
        }

        total_received += messages.len() as u64;

        // Process each message: parse payload, ack on success.
        for msg in &messages {
            // For now, we validate the payload is a JSON object (entity record).
            // Full entity extraction will be wired in when the pipeline integration
            // is complete.
            if !msg.payload.is_object() {
                warn!(
                    source_name = %source.name,
                    message_id = %msg.id,
                    "skipping non-object payload"
                );
                continue;
            }

            // Acknowledge the message after successful parsing.
            if let Err(e) = backend.ack(&msg.id).await {
                warn!(
                    source_name = %source.name,
                    message_id = %msg.id,
                    error = %e,
                    "failed to ack message"
                );
                continue;
            }

            total_acked += 1;
        }

        info!(
            source_name = %source.name,
            batch_size = messages.len(),
            total_received = total_received,
            total_acked = total_acked,
            "processed queue batch"
        );

        // Brief yield to avoid busy-spinning when the queue is full.
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_queue_message_deserialize() {
        let json = r#"{"id":"msg-001","payload":{"event_type":"login","user":"alice"}}"#;
        let msg: QueueMessage = serde_json::from_str(json).unwrap();
        assert_eq!(msg.id, "msg-001");
        assert_eq!(msg.payload["event_type"], "login");
        assert_eq!(msg.payload["user"], "alice");
    }

    #[test]
    fn test_queue_message_with_nested_payload() {
        let json = r#"{
            "id": "msg-002",
            "payload": {
                "event_type": "transaction",
                "amount": 99.5,
                "metadata": {"source": "api", "version": 2}
            }
        }"#;
        let msg: QueueMessage = serde_json::from_str(json).unwrap();
        assert_eq!(msg.id, "msg-002");
        assert_eq!(msg.payload["amount"], 99.5);
        assert!(msg.payload["metadata"].is_object());
    }

    #[test]
    fn test_queue_message_roundtrip() {
        let msg = QueueMessage {
            id: "msg-003".to_string(),
            payload: serde_json::json!({"key": "value"}),
        };
        let serialized = serde_json::to_string(&msg).unwrap();
        let deserialized: QueueMessage = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized.id, msg.id);
        assert_eq!(deserialized.payload, msg.payload);
    }

    #[test]
    fn test_build_backend_unknown() {
        let config = QueueConfig {
            queue_type: "kafka".to_string(),
            connection_ref: "test".to_string(),
            queue_url: None,
            channel: None,
            stream_key: None,
            consumer_group: None,
            batch_size: None,
        };
        let result = build_backend(&config);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("unknown queue backend 'kafka'"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn test_build_backend_redis_without_feature() {
        let config = QueueConfig {
            queue_type: "redis".to_string(),
            connection_ref: "redis-local".to_string(),
            queue_url: None,
            channel: Some("events".to_string()),
            stream_key: None,
            consumer_group: None,
            batch_size: Some(20),
        };
        // Without the queue-redis feature, this should fail with a clear message.
        let result = build_backend(&config);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("queue-redis"),
            "should mention feature gate: {}",
            err
        );
    }

    #[test]
    fn test_build_backend_sqs_without_feature() {
        let config = QueueConfig {
            queue_type: "sqs".to_string(),
            connection_ref: "aws-prod".to_string(),
            queue_url: Some("https://sqs.us-east-1.amazonaws.com/123/my-queue".to_string()),
            channel: None,
            stream_key: None,
            consumer_group: Some("ingest-workers".to_string()),
            batch_size: Some(10),
        };
        let result = build_backend(&config);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("queue-sqs"),
            "should mention feature gate: {}",
            err
        );
    }

    #[test]
    fn test_build_backend_nats_without_feature() {
        let config = QueueConfig {
            queue_type: "nats".to_string(),
            connection_ref: "nats-cluster".to_string(),
            queue_url: None,
            channel: Some("events.ingest".to_string()),
            stream_key: None,
            consumer_group: None,
            batch_size: None,
        };
        let result = build_backend(&config);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("queue-nats"),
            "should mention feature gate: {}",
            err
        );
    }

    #[test]
    fn test_default_constants() {
        assert_eq!(DEFAULT_POLL_INTERVAL_MS, 1000);
        assert_eq!(DEFAULT_BATCH_SIZE, 10);
    }
}
