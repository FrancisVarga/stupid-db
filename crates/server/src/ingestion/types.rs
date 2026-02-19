//! Type definitions for the Ingestion Manager.
//!
//! Covers source configuration (tagged union), scheduling, DB row mapping,
//! job tracking, and the in-memory job store.

use std::sync::atomic::AtomicU64;
use std::sync::{Arc, RwLock};

use chrono::{DateTime, Utc};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ── ZMQ granularity ──────────────────────────────────────────────────

/// Controls how ingested documents are published over ZMQ.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ZmqGranularity {
    /// Publish a single summary message per batch.
    Summary,
    /// Publish one message per document in the batch.
    Batched,
}

impl Default for ZmqGranularity {
    fn default() -> Self {
        Self::Summary
    }
}

// ── Source configuration (tagged union) ──────────────────────────────

/// Source-specific configuration, tagged by `type` field.
///
/// Stored as JSONB in the `config_json` column of `ingestion_sources`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SourceConfig {
    Parquet(ParquetConfig),
    Directory(DirectoryConfig),
    S3(S3Config),
    CsvJson(CsvJsonConfig),
    Push(PushConfig),
    Queue(QueueConfig),
}

/// Parquet file ingestion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParquetConfig {
    /// Event type label applied to all documents from this source.
    pub event_type: String,
    /// Optional fixed segment ID (auto-generated if omitted).
    pub segment_id: Option<String>,
}

/// Directory-based file ingestion with optional watch mode.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectoryConfig {
    /// Filesystem path to watch/scan.
    pub path: String,
    /// Whether to recurse into subdirectories.
    #[serde(default = "default_true")]
    pub recursive: bool,
    /// Glob pattern for matching files.
    #[serde(default = "default_file_pattern")]
    pub file_pattern: String,
    /// Enable filesystem watch mode.
    pub watch: Option<bool>,
    /// Polling interval for watch mode (seconds).
    pub watch_interval_secs: Option<u32>,
}

/// S3 bucket ingestion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct S3Config {
    /// S3 bucket name.
    pub bucket: String,
    /// Key prefix to filter objects.
    pub prefix: String,
    /// AWS region.
    pub region: String,
    /// Reference to stored credentials (by connection ID).
    pub credentials_ref: Option<String>,
}

/// CSV/JSON file ingestion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CsvJsonConfig {
    /// Event type label applied to all documents from this source.
    pub event_type: String,
    /// Optional fixed segment ID.
    pub segment_id: Option<String>,
    /// CSV delimiter character.
    #[serde(default = "default_delimiter")]
    pub delimiter: String,
    /// Name of the timestamp field in the data.
    #[serde(default = "default_timestamp_field")]
    pub timestamp_field: String,
}

/// Push-based ingestion (HTTP POST receiver).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushConfig {
    /// Event type label applied to pushed documents.
    pub event_type: String,
    /// Prefix for auto-generated segment IDs.
    pub segment_id_prefix: Option<String>,
    /// Restrict accepted content types.
    pub allowed_content_types: Option<Vec<String>>,
}

/// Queue-based ingestion (SQS, Redis, Kafka, etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueConfig {
    /// Queue provider type (e.g. "sqs", "redis", "kafka").
    pub queue_type: String,
    /// Reference to stored queue connection credentials.
    pub connection_ref: String,
    /// Full queue URL (SQS).
    pub queue_url: Option<String>,
    /// Channel/topic name (Redis Pub/Sub, Kafka).
    pub channel: Option<String>,
    /// Stream key (Redis Streams).
    pub stream_key: Option<String>,
    /// Consumer group name.
    pub consumer_group: Option<String>,
    /// Max messages per poll batch.
    pub batch_size: Option<u32>,
}

// ── Default value functions ──────────────────────────────────────────

fn default_true() -> bool {
    true
}

fn default_file_pattern() -> String {
    "*.parquet".to_string()
}

fn default_delimiter() -> String {
    ",".to_string()
}

fn default_timestamp_field() -> String {
    "@timestamp".to_string()
}

fn default_timezone() -> String {
    "UTC".to_string()
}

// ── Schedule ─────────────────────────────────────────────────────────

/// Cron-based schedule for periodic ingestion runs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestionSchedule {
    /// Cron expression (e.g. "0 */5 * * * *").
    pub cron: String,
    /// IANA timezone name.
    #[serde(default = "default_timezone")]
    pub timezone: String,
}

// ── Database row ─────────────────────────────────────────────────────

/// Row from the `ingestion_sources` table.
#[derive(Debug, Clone, serde::Serialize, sqlx::FromRow)]
pub struct IngestionSource {
    pub id: Uuid,
    pub name: String,
    pub source_type: String,
    pub config_json: serde_json::Value,
    pub zmq_granularity: String,
    pub schedule_json: Option<serde_json::Value>,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_run_at: Option<DateTime<Utc>>,
    pub next_run_at: Option<DateTime<Utc>>,
}

impl IngestionSource {
    /// Deserialize the stored `config_json` into a typed [`SourceConfig`].
    pub fn config(&self) -> Result<SourceConfig, serde_json::Error> {
        serde_json::from_value(self.config_json.clone())
    }

    /// Deserialize the optional `schedule_json` into an [`IngestionSchedule`].
    ///
    /// Returns `None` if no schedule is set, `Some(Err)` if parsing fails.
    pub fn schedule(&self) -> Option<Result<IngestionSchedule, serde_json::Error>> {
        self.schedule_json
            .as_ref()
            .map(|v| serde_json::from_value(v.clone()))
    }

    /// Whether this source type supports scheduled runs.
    ///
    /// Only `directory`, `s3`, and `queue` sources are schedulable;
    /// `parquet` and `csv_json` are one-shot, and `push` is event-driven.
    pub fn is_schedulable(&self) -> bool {
        matches!(
            self.source_type.as_str(),
            "directory" | "s3" | "queue"
        )
    }
}

// ── Job tracking ─────────────────────────────────────────────────────

/// How a job was triggered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TriggerKind {
    /// Manually triggered via API.
    Manual,
    /// Triggered by cron schedule.
    Scheduled,
    /// Triggered by incoming push data.
    Push,
    /// Triggered by filesystem watcher.
    Watch,
}

/// Current status of an ingestion job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

/// An in-memory ingestion job with atomic progress counters.
///
/// Counters use `Arc<AtomicU64>` for lock-free updates from async tasks.
/// Mutable completion fields use `RwLock` since they change once at job end.
#[derive(Debug)]
pub struct IngestionJob {
    pub id: Uuid,
    pub source_id: Option<Uuid>,
    pub source_name: String,
    pub trigger_kind: TriggerKind,
    pub status: RwLock<JobStatus>,
    pub docs_processed: Arc<AtomicU64>,
    pub docs_total: Arc<AtomicU64>,
    pub segments_done: Arc<AtomicU64>,
    pub segments_total: Arc<AtomicU64>,
    pub created_at: DateTime<Utc>,
    pub completed_at: RwLock<Option<DateTime<Utc>>>,
    pub error: RwLock<Option<String>>,
    pub segment_ids: RwLock<Vec<String>>,
}

/// In-memory store for active and recent ingestion jobs.
///
/// Uses `IndexMap` to preserve insertion order (newest last) while
/// allowing O(1) lookups by job ID.
#[derive(Debug)]
pub struct IngestionJobStore {
    pub jobs: Arc<RwLock<IndexMap<Uuid, Arc<IngestionJob>>>>,
}

impl IngestionJobStore {
    /// Create an empty job store.
    pub fn new() -> Self {
        Self {
            jobs: Arc::new(RwLock::new(IndexMap::new())),
        }
    }
}

impl Default for IngestionJobStore {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_source_config_parquet_roundtrip() {
        let config = SourceConfig::Parquet(ParquetConfig {
            event_type: "w88_event".to_string(),
            segment_id: Some("seg-001".to_string()),
        });
        let json = serde_json::to_string(&config).unwrap();
        let parsed: SourceConfig = serde_json::from_str(&json).unwrap();
        if let SourceConfig::Parquet(c) = parsed {
            assert_eq!(c.event_type, "w88_event");
            assert_eq!(c.segment_id.as_deref(), Some("seg-001"));
        } else {
            panic!("expected Parquet variant");
        }
    }

    #[test]
    fn test_source_config_directory_roundtrip() {
        let json = r#"{"type":"directory","path":"/data/logs","recursive":false,"file_pattern":"*.json"}"#;
        let config: SourceConfig = serde_json::from_str(json).unwrap();
        if let SourceConfig::Directory(c) = &config {
            assert_eq!(c.path, "/data/logs");
            assert!(!c.recursive);
            assert_eq!(c.file_pattern, "*.json");
        } else {
            panic!("expected Directory variant");
        }
        // Round-trip
        let json2 = serde_json::to_string(&config).unwrap();
        let _: SourceConfig = serde_json::from_str(&json2).unwrap();
    }

    #[test]
    fn test_source_config_directory_defaults() {
        let json = r#"{"type":"directory","path":"/data"}"#;
        let config: SourceConfig = serde_json::from_str(json).unwrap();
        if let SourceConfig::Directory(c) = config {
            assert!(c.recursive);
            assert_eq!(c.file_pattern, "*.parquet");
            assert!(c.watch.is_none());
        } else {
            panic!("expected Directory variant");
        }
    }

    #[test]
    fn test_source_config_s3_roundtrip() {
        let config = SourceConfig::S3(S3Config {
            bucket: "my-bucket".to_string(),
            prefix: "logs/".to_string(),
            region: "us-east-1".to_string(),
            credentials_ref: Some("aws-prod".to_string()),
        });
        let json = serde_json::to_string(&config).unwrap();
        let parsed: SourceConfig = serde_json::from_str(&json).unwrap();
        if let SourceConfig::S3(c) = parsed {
            assert_eq!(c.bucket, "my-bucket");
            assert_eq!(c.prefix, "logs/");
            assert_eq!(c.credentials_ref.as_deref(), Some("aws-prod"));
        } else {
            panic!("expected S3 variant");
        }
    }

    #[test]
    fn test_source_config_csv_json_defaults() {
        let json = r#"{"type":"csv_json","event_type":"access_log"}"#;
        let config: SourceConfig = serde_json::from_str(json).unwrap();
        if let SourceConfig::CsvJson(c) = config {
            assert_eq!(c.event_type, "access_log");
            assert_eq!(c.delimiter, ",");
            assert_eq!(c.timestamp_field, "@timestamp");
            assert!(c.segment_id.is_none());
        } else {
            panic!("expected CsvJson variant");
        }
    }

    #[test]
    fn test_source_config_push_roundtrip() {
        let config = SourceConfig::Push(PushConfig {
            event_type: "webhook".to_string(),
            segment_id_prefix: Some("push-".to_string()),
            allowed_content_types: Some(vec!["application/json".to_string()]),
        });
        let json = serde_json::to_string(&config).unwrap();
        let parsed: SourceConfig = serde_json::from_str(&json).unwrap();
        if let SourceConfig::Push(c) = parsed {
            assert_eq!(c.event_type, "webhook");
            assert_eq!(c.segment_id_prefix.as_deref(), Some("push-"));
            assert_eq!(c.allowed_content_types.as_ref().unwrap().len(), 1);
        } else {
            panic!("expected Push variant");
        }
    }

    #[test]
    fn test_source_config_queue_roundtrip() {
        let config = SourceConfig::Queue(QueueConfig {
            queue_type: "sqs".to_string(),
            connection_ref: "aws-prod".to_string(),
            queue_url: Some("https://sqs.us-east-1.amazonaws.com/123/my-queue".to_string()),
            channel: None,
            stream_key: None,
            consumer_group: Some("ingest-workers".to_string()),
            batch_size: Some(10),
        });
        let json = serde_json::to_string(&config).unwrap();
        let parsed: SourceConfig = serde_json::from_str(&json).unwrap();
        if let SourceConfig::Queue(c) = parsed {
            assert_eq!(c.queue_type, "sqs");
            assert_eq!(c.connection_ref, "aws-prod");
            assert!(c.queue_url.is_some());
            assert_eq!(c.consumer_group.as_deref(), Some("ingest-workers"));
            assert_eq!(c.batch_size, Some(10));
        } else {
            panic!("expected Queue variant");
        }
    }

    #[test]
    fn test_zmq_granularity_default() {
        assert_eq!(ZmqGranularity::default(), ZmqGranularity::Summary);
    }

    #[test]
    fn test_zmq_granularity_serde() {
        let json = r#""batched""#;
        let g: ZmqGranularity = serde_json::from_str(json).unwrap();
        assert_eq!(g, ZmqGranularity::Batched);

        let json = serde_json::to_string(&ZmqGranularity::Summary).unwrap();
        assert_eq!(json, r#""summary""#);
    }

    #[test]
    fn test_schedule_defaults() {
        let json = r#"{"cron":"0 */5 * * * *"}"#;
        let sched: IngestionSchedule = serde_json::from_str(json).unwrap();
        assert_eq!(sched.cron, "0 */5 * * * *");
        assert_eq!(sched.timezone, "UTC");
    }

    #[test]
    fn test_trigger_kind_serde() {
        let json = r#""scheduled""#;
        let tk: TriggerKind = serde_json::from_str(json).unwrap();
        assert_eq!(tk, TriggerKind::Scheduled);

        let json = serde_json::to_string(&TriggerKind::Watch).unwrap();
        assert_eq!(json, r#""watch""#);
    }

    #[test]
    fn test_job_status_serde() {
        for (variant, expected) in [
            (JobStatus::Pending, "pending"),
            (JobStatus::Running, "running"),
            (JobStatus::Completed, "completed"),
            (JobStatus::Failed, "failed"),
            (JobStatus::Cancelled, "cancelled"),
        ] {
            let json = serde_json::to_string(&variant).unwrap();
            assert_eq!(json, format!("\"{}\"", expected));
            let parsed: JobStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, variant);
        }
    }

    #[test]
    fn test_ingestion_source_is_schedulable() {
        let make_source = |source_type: &str| IngestionSource {
            id: Uuid::new_v4(),
            name: "test".to_string(),
            source_type: source_type.to_string(),
            config_json: serde_json::json!({}),
            zmq_granularity: "summary".to_string(),
            schedule_json: None,
            enabled: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            last_run_at: None,
            next_run_at: None,
        };

        assert!(!make_source("parquet").is_schedulable());
        assert!(make_source("directory").is_schedulable());
        assert!(make_source("s3").is_schedulable());
        assert!(!make_source("csv_json").is_schedulable());
        assert!(!make_source("push").is_schedulable());
        assert!(make_source("queue").is_schedulable());
    }

    #[test]
    fn test_ingestion_source_config_parse() {
        let source = IngestionSource {
            id: Uuid::new_v4(),
            name: "test-parquet".to_string(),
            source_type: "parquet".to_string(),
            config_json: serde_json::json!({
                "type": "parquet",
                "event_type": "w88_event",
            }),
            zmq_granularity: "summary".to_string(),
            schedule_json: Some(serde_json::json!({
                "cron": "0 0 * * *",
            })),
            enabled: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            last_run_at: None,
            next_run_at: None,
        };

        let config = source.config().unwrap();
        assert!(matches!(config, SourceConfig::Parquet(_)));

        let schedule = source.schedule().unwrap().unwrap();
        assert_eq!(schedule.cron, "0 0 * * *");
        assert_eq!(schedule.timezone, "UTC");
    }

    #[test]
    fn test_ingestion_source_no_schedule() {
        let source = IngestionSource {
            id: Uuid::new_v4(),
            name: "test".to_string(),
            source_type: "push".to_string(),
            config_json: serde_json::json!({"type": "push", "event_type": "hook"}),
            zmq_granularity: "summary".to_string(),
            schedule_json: None,
            enabled: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            last_run_at: None,
            next_run_at: None,
        };

        assert!(source.schedule().is_none());
    }

    #[test]
    fn test_job_store_new() {
        let store = IngestionJobStore::new();
        let jobs = store.jobs.read().unwrap();
        assert!(jobs.is_empty());
    }
}
