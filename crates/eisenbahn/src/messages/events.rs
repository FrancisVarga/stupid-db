//! Domain event message payloads.
//!
//! These are the inner payloads carried by [`Message`](crate::Message) envelopes.
//! Each type represents a specific event that components publish via PUB/SUB.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// The type of ingestion source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IngestSourceType {
    Parquet,
    Directory,
    S3,
    CsvJson,
    Push,
    Queue,
}

/// Emitted when an ingest job begins processing a source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IngestStarted {
    /// Unique identifier for this ingest job.
    pub job_id: Uuid,
    /// Data source identifier (e.g. file path, stream name).
    pub source: String,
    /// Type of the ingestion source.
    pub source_type: IngestSourceType,
    /// Segment IDs targeted by this ingest job.
    pub segment_ids: Vec<String>,
    /// Estimated total records (if known).
    pub estimated_records: Option<u64>,
    /// When the ingest job started.
    pub started_at: DateTime<Utc>,
}

/// Emitted after each record batch is processed during ingest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IngestRecordBatch {
    /// Ingest job this batch belongs to.
    pub job_id: Uuid,
    /// Zero-based index of this batch within the job.
    pub batch_index: u64,
    /// Number of records in this batch.
    pub batch_record_count: u64,
    /// Cumulative records processed so far in the job.
    pub cumulative_records: u64,
    /// Total records expected (if known).
    pub total_records: Option<u64>,
    /// Segment currently being written to.
    pub current_segment: String,
}

/// Emitted when a new ingestion source is registered.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IngestSourceRegistered {
    /// Unique identifier for the registered source.
    pub source_id: Uuid,
    /// Human-readable name for the source.
    pub name: String,
    /// Type of the ingestion source.
    pub source_type: IngestSourceType,
    /// Source-specific configuration.
    pub config: serde_json::Value,
}

/// Emitted when an ingest batch finishes processing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IngestComplete {
    /// Data source identifier (e.g. file path, stream name).
    pub source: String,
    /// Number of records successfully ingested.
    pub record_count: u64,
    /// Wall-clock duration of the ingest in milliseconds.
    pub duration_ms: u64,
    /// Unique identifier for the ingest job (if tracked).
    #[serde(default)]
    pub job_id: Option<Uuid>,
    /// Number of segments written during the ingest.
    #[serde(default)]
    pub total_segments: u64,
    /// Error message if the ingest failed.
    #[serde(default)]
    pub error: Option<String>,
    /// Type of the ingestion source (if known).
    #[serde(default)]
    pub source_type: Option<IngestSourceType>,
}

/// Emitted when an anomaly rule fires above its threshold.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnomalyDetected {
    /// The rule that triggered the anomaly.
    pub rule_id: String,
    /// The entity that exhibited anomalous behavior.
    pub entity_id: String,
    /// Anomaly score (higher = more anomalous).
    pub score: f64,
}

/// What happened to a rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuleAction {
    Created,
    Updated,
    Deleted,
}

/// Emitted when a rule is created, updated, or deleted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleChanged {
    /// The rule that was modified.
    pub rule_id: String,
    /// What happened to the rule.
    pub action: RuleAction,
}

/// Emitted when a compute batch finishes feature extraction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComputeComplete {
    /// Identifier for the compute batch.
    pub batch_id: String,
    /// Number of features computed in this batch.
    pub features_computed: u64,
}

/// Worker health status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkerStatus {
    Healthy,
    Degraded,
    Unhealthy,
}

/// Periodic heartbeat reporting worker health metrics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkerHealth {
    /// Unique identifier for the worker.
    pub worker_id: String,
    /// Current health status.
    pub status: WorkerStatus,
    /// CPU utilization percentage (0.0–100.0).
    pub cpu_pct: f64,
    /// Memory usage in bytes.
    pub mem_bytes: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip<T>(val: &T) -> T
    where
        T: Serialize + for<'de> Deserialize<'de> + std::fmt::Debug + PartialEq,
    {
        let bytes = rmp_serde::to_vec(val).expect("serialize");
        rmp_serde::from_slice(&bytes).expect("deserialize")
    }

    #[test]
    fn roundtrip_ingest_complete() {
        let msg = IngestComplete {
            source: "data/sample.parquet".into(),
            record_count: 42_000,
            duration_ms: 1234,
            job_id: None,
            total_segments: 0,
            error: None,
            source_type: None,
        };
        assert_eq!(roundtrip(&msg), msg);
    }

    #[test]
    fn roundtrip_ingest_complete_with_new_fields() {
        let msg = IngestComplete {
            source: "data/sample.parquet".into(),
            record_count: 42_000,
            duration_ms: 1234,
            job_id: Some(Uuid::new_v4()),
            total_segments: 3,
            error: None,
            source_type: Some(IngestSourceType::Parquet),
        };
        assert_eq!(roundtrip(&msg), msg);
    }

    #[test]
    fn roundtrip_ingest_started() {
        let msg = IngestStarted {
            job_id: Uuid::new_v4(),
            source: "data/sample.parquet".into(),
            source_type: IngestSourceType::Parquet,
            segment_ids: vec!["seg-001".into(), "seg-002".into()],
            estimated_records: Some(100_000),
            started_at: Utc::now(),
        };
        assert_eq!(roundtrip(&msg), msg);
    }

    #[test]
    fn roundtrip_ingest_record_batch() {
        let msg = IngestRecordBatch {
            job_id: Uuid::new_v4(),
            batch_index: 5,
            batch_record_count: 1_000,
            cumulative_records: 6_000,
            total_records: Some(10_000),
            current_segment: "seg-001".into(),
        };
        assert_eq!(roundtrip(&msg), msg);
    }

    #[test]
    fn roundtrip_ingest_source_registered() {
        let msg = IngestSourceRegistered {
            source_id: Uuid::new_v4(),
            name: "daily-parquet-feed".into(),
            source_type: IngestSourceType::Directory,
            config: serde_json::json!({"path": "/data/feed", "pattern": "*.parquet"}),
        };
        assert_eq!(roundtrip(&msg), msg);
    }

    #[test]
    fn ingest_source_type_serde_snake_case() {
        // Verify that IngestSourceType serializes/deserializes as snake_case
        let json = serde_json::to_string(&IngestSourceType::CsvJson).unwrap();
        assert_eq!(json, "\"csv_json\"");
        let parsed: IngestSourceType = serde_json::from_str("\"csv_json\"").unwrap();
        assert_eq!(parsed, IngestSourceType::CsvJson);
    }

    #[test]
    fn ingest_complete_backward_compat() {
        // Old messages without new fields should deserialize with defaults
        let old_json = r#"{"source":"test.parquet","record_count":100,"duration_ms":50}"#;
        let parsed: IngestComplete = serde_json::from_str(old_json).unwrap();
        assert_eq!(parsed.source, "test.parquet");
        assert_eq!(parsed.record_count, 100);
        assert_eq!(parsed.job_id, None);
        assert_eq!(parsed.total_segments, 0);
        assert_eq!(parsed.error, None);
        assert_eq!(parsed.source_type, None);
    }

    #[test]
    fn roundtrip_anomaly_detected() {
        let msg = AnomalyDetected {
            rule_id: "rule-001".into(),
            entity_id: "entity-abc".into(),
            score: 0.95,
        };
        assert_eq!(roundtrip(&msg), msg);
    }

    #[test]
    fn roundtrip_rule_changed() {
        for action in [RuleAction::Created, RuleAction::Updated, RuleAction::Deleted] {
            let msg = RuleChanged {
                rule_id: "rule-002".into(),
                action,
            };
            assert_eq!(roundtrip(&msg), msg);
        }
    }

    #[test]
    fn roundtrip_compute_complete() {
        let msg = ComputeComplete {
            batch_id: "batch-xyz".into(),
            features_computed: 128,
        };
        assert_eq!(roundtrip(&msg), msg);
    }

    #[test]
    fn roundtrip_worker_health() {
        let msg = WorkerHealth {
            worker_id: "worker-01".into(),
            status: WorkerStatus::Healthy,
            cpu_pct: 42.5,
            mem_bytes: 1_073_741_824,
        };
        assert_eq!(roundtrip(&msg), msg);
    }
}
