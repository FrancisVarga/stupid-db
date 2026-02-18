//! Topic constants for PUB/SUB routing.
//!
//! Topics follow the pattern `eisenbahn.<domain>.<event>` for consistent
//! namespace-qualified routing across all components.

// ── Event topics ──────────────────────────────────────────────────────────

/// Fired when an ingest batch finishes writing to storage.
pub const INGEST_COMPLETE: &str = "eisenbahn.ingest.complete";

/// Fired when an anomaly rule triggers above its threshold.
pub const ANOMALY_DETECTED: &str = "eisenbahn.anomaly.detected";

/// Fired when a rule is created, updated, or deleted.
pub const RULE_CHANGED: &str = "eisenbahn.rule.changed";

/// Fired when a compute batch finishes feature extraction.
pub const COMPUTE_COMPLETE: &str = "eisenbahn.compute.complete";

/// Periodic worker health heartbeat.
pub const WORKER_HEALTH: &str = "eisenbahn.worker.health";

// ── Pipeline topics ───────────────────────────────────────────────────────

/// Raw records pushed into the ingest pipeline.
pub const INGEST_BATCH: &str = "eisenbahn.pipeline.ingest";

/// Computed features flowing out of the compute pipeline.
pub const COMPUTE_RESULT: &str = "eisenbahn.pipeline.compute";

/// Entity/edge updates flowing into the graph store.
pub const GRAPH_UPDATE: &str = "eisenbahn.pipeline.graph";
