use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use serde::Serialize;
use tokio::sync::{broadcast, RwLock};

use stupid_catalog::Catalog;
use stupid_compute::SharedKnowledgeState;
use stupid_graph::GraphStore;
use stupid_llm::QueryGenerator;
use stupid_tool_runtime::AgenticLoop;

pub type SharedGraph = Arc<RwLock<GraphStore>>;
pub type SharedPipeline = Arc<std::sync::Mutex<stupid_compute::Pipeline>>;

/// Handle to the background scheduler thread for shutdown and metrics.
#[allow(dead_code)] // shutdown used for future graceful server shutdown
pub struct SchedulerHandle {
    /// Signal to stop the scheduler loop.
    pub shutdown: Arc<std::sync::atomic::AtomicBool>,
    /// Shared scheduler metrics (read-only from API handlers).
    pub metrics: Arc<std::sync::RwLock<stupid_compute::SchedulerMetrics>>,
}

/// Active segment writer for queue-ingested documents.
/// Tuple: (segment_id, writer) — rotated daily.
pub type QueueWriter = Arc<std::sync::Mutex<Option<(String, stupid_segment::writer::SegmentWriter)>>>;

pub struct AppState {
    pub graph: SharedGraph,
    pub knowledge: SharedKnowledgeState,
    pub pipeline: SharedPipeline,
    pub scheduler: RwLock<Option<SchedulerHandle>>,
    pub catalog: Arc<RwLock<Option<Catalog>>>,
    pub catalog_store: Arc<stupid_catalog::CatalogStore>,
    pub query_generator: Option<QueryGenerator>,
    pub segment_ids: Arc<RwLock<Vec<String>>>,
    pub doc_count: Arc<AtomicU64>,
    pub loading: Arc<LoadingState>,
    pub broadcast: broadcast::Sender<String>,
    pub queue_metrics: Arc<std::sync::RwLock<std::collections::HashMap<String, Arc<QueueMetrics>>>>,
    /// Segment writer for persisting queue-ingested documents to disk.
    pub queue_writer: QueueWriter,
    /// Root data directory for segment storage.
    pub data_dir: PathBuf,
    /// Agent executor for running AI agents.
    pub agent_executor: Option<stupid_agent::AgentExecutor>,
    /// Agentic loop for tool-aware LLM interaction (streaming, tool use).
    pub agentic_loop: Option<AgenticLoop>,
    /// Encrypted connection credential store.
    pub connections: Arc<tokio::sync::RwLock<crate::connections::ConnectionStore>>,
    /// Encrypted queue connection store.
    pub queue_connections: Arc<tokio::sync::RwLock<crate::queue_connections::QueueConnectionStore>>,
    /// Encrypted Athena connection store.
    pub athena_connections: Arc<tokio::sync::RwLock<crate::athena_connections::AthenaConnectionStore>>,
    /// Embedding backend for vector search features.
    pub embedder: Option<Arc<dyn stupid_ingest::embedding::Embedder>>,
    /// Session store for agent chat history persistence.
    pub session_store: Arc<tokio::sync::RwLock<stupid_agent::session::SessionStore>>,
    /// Eisenbahn messaging client for ZMQ-based service routing.
    pub eisenbahn: Option<Arc<crate::eisenbahn_client::EisenbahnClient>>,
    /// Anomaly rule loader (filesystem-backed with hot-reload).
    pub rule_loader: stupid_rules::loader::RuleLoader,
    /// Per-rule trigger history for the `/anomaly-rules/{id}/history` endpoint.
    pub trigger_history: crate::anomaly_rules::SharedTriggerHistory,
    /// Audit log for anomaly rule evaluation.
    pub audit_log: stupid_rules::audit_log::AuditLog,
    /// Per-connection Athena query audit log with cost tracking.
    pub athena_query_log: crate::athena_query_log::AthenaQueryLog,
    /// PostgreSQL connection pool for pgvector embedding storage.
    pub pg_pool: Option<sqlx::PgPool>,
}

/// Lock-free atomic counters for queue consumer observability.
///
/// All fields use `Ordering::Relaxed` — these are monotonic counters
/// where eventual visibility is acceptable for dashboard/status reads.
pub struct QueueMetrics {
    /// Whether queue ingestion is enabled in config.
    pub enabled: AtomicBool,
    /// Whether the consumer is currently connected to the queue.
    pub connected: AtomicBool,
    /// Total messages received from queue polls.
    pub messages_received: AtomicU64,
    /// Messages successfully parsed and ingested.
    pub messages_processed: AtomicU64,
    /// Messages that failed parsing or ingestion.
    pub messages_failed: AtomicU64,
    /// Number of micro-batches flushed to the graph.
    pub batches_processed: AtomicU64,
    /// Cumulative processing time in microseconds (for avg latency calc).
    pub total_processing_time_us: AtomicU64,
    /// Epoch milliseconds of the last successful poll.
    pub last_poll_epoch_ms: AtomicU64,
}

impl QueueMetrics {
    pub fn new() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            connected: AtomicBool::new(false),
            messages_received: AtomicU64::new(0),
            messages_processed: AtomicU64::new(0),
            messages_failed: AtomicU64::new(0),
            batches_processed: AtomicU64::new(0),
            total_processing_time_us: AtomicU64::new(0),
            last_poll_epoch_ms: AtomicU64::new(0),
        }
    }
}

/// Tracks background data loading progress.
pub struct LoadingState {
    pub phase: RwLock<LoadingPhase>,
    /// Number of segments loaded so far.
    pub progress: AtomicU64,
    /// Total segments to load.
    pub total: AtomicU64,
    pub started_at: Instant,
}

impl LoadingState {
    pub fn new() -> Self {
        Self {
            phase: RwLock::new(LoadingPhase::Discovering),
            progress: AtomicU64::new(0),
            total: AtomicU64::new(0),
            started_at: Instant::now(),
        }
    }

    pub async fn set_phase(&self, phase: LoadingPhase) {
        *self.phase.write().await = phase;
    }

    pub fn set_progress(&self, progress: u64, total: u64) {
        self.progress.store(progress, Ordering::Relaxed);
        self.total.store(total, Ordering::Relaxed);
    }

    pub async fn is_ready(&self) -> bool {
        matches!(*self.phase.read().await, LoadingPhase::Ready)
    }

    pub async fn to_status(&self) -> LoadingStatus {
        let phase = self.phase.read().await;
        LoadingStatus {
            phase: phase.label(),
            is_ready: matches!(*phase, LoadingPhase::Ready),
            progress: self.progress.load(Ordering::Relaxed),
            total: self.total.load(Ordering::Relaxed),
            elapsed_seconds: self.started_at.elapsed().as_secs_f64(),
            error: match &*phase {
                LoadingPhase::Failed(msg) => Some(msg.clone()),
                _ => None,
            },
        }
    }
}

#[derive(Debug)]
#[allow(dead_code)] // BuildingCatalog/RunningCompute used by live watcher, not cold start
pub enum LoadingPhase {
    Discovering,
    LoadingSegments,
    BuildingCatalog,
    RunningCompute,
    Ready,
    Failed(String),
}

impl LoadingPhase {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Discovering => "discovering_segments",
            Self::LoadingSegments => "loading_segments",
            Self::BuildingCatalog => "building_catalog",
            Self::RunningCompute => "running_compute",
            Self::Ready => "ready",
            Self::Failed(_) => "failed",
        }
    }
}

/// Serializable loading status for API responses.
#[derive(Debug, Serialize)]
pub struct LoadingStatus {
    pub phase: &'static str,
    pub is_ready: bool,
    pub progress: u64,
    pub total: u64,
    pub elapsed_seconds: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}
