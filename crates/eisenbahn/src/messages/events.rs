//! Domain event message payloads.
//!
//! These are the inner payloads carried by [`Message`](crate::Message) envelopes.
//! Each type represents a specific event that components publish via PUB/SUB.

use serde::{Deserialize, Serialize};

/// Emitted when an ingest batch finishes processing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IngestComplete {
    /// Data source identifier (e.g. file path, stream name).
    pub source: String,
    /// Number of records successfully ingested.
    pub record_count: u64,
    /// Wall-clock duration of the ingest in milliseconds.
    pub duration_ms: u64,
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
        };
        assert_eq!(roundtrip(&msg), msg);
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
