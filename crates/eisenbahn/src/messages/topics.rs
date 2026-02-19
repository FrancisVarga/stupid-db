//! Topic constants for PUB/SUB routing.
//!
//! Topics follow the pattern `eisenbahn.<domain>.<event>` for consistent
//! namespace-qualified routing across all components.

// ── Event topics ──────────────────────────────────────────────────────────

/// Fired when an ingest job begins processing a source.
pub const INGEST_STARTED: &str = "eisenbahn.ingest.started";

/// Fired when an ingest batch finishes writing to storage.
pub const INGEST_COMPLETE: &str = "eisenbahn.ingest.complete";

/// Fired after each record batch is processed during ingest.
pub const INGEST_RECORD_BATCH: &str = "eisenbahn.ingest.record_batch";

/// Fired when a new ingestion source is registered.
pub const INGEST_SOURCE_REGISTERED: &str = "eisenbahn.ingest.source_registered";

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

// ── Service request/reply topics ─────────────────────────────────────────

/// Query service request.
pub const SVC_QUERY_REQUEST: &str = "eisenbahn.svc.query.request";
/// Query service response.
pub const SVC_QUERY_RESPONSE: &str = "eisenbahn.svc.query.response";

/// Agent service request.
pub const SVC_AGENT_REQUEST: &str = "eisenbahn.svc.agent.request";
/// Agent service response.
pub const SVC_AGENT_RESPONSE: &str = "eisenbahn.svc.agent.response";

/// Athena service request.
pub const SVC_ATHENA_REQUEST: &str = "eisenbahn.svc.athena.request";
/// Athena service response.
pub const SVC_ATHENA_RESPONSE: &str = "eisenbahn.svc.athena.response";
/// Athena streaming chunk.
pub const SVC_ATHENA_STREAM: &str = "eisenbahn.svc.athena.stream";
/// Athena query completion signal.
pub const SVC_ATHENA_DONE: &str = "eisenbahn.svc.athena.done";

/// Catalog query service request.
pub const SVC_CATALOG_REQUEST: &str = "eisenbahn.svc.catalog.request";
/// Catalog query service response.
pub const SVC_CATALOG_RESPONSE: &str = "eisenbahn.svc.catalog.response";
