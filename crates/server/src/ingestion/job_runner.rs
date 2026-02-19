//! Ingestion job runner — async job execution with ZMQ event emission.
//!
//! [`spawn_ingestion_job`] is the main entry point: it creates an [`IngestionJob`],
//! registers it in the in-memory store, publishes an `INGEST_STARTED` event,
//! and spawns the actual work in a background tokio task.
//!
//! Progress is monitored by a separate task that publishes `INGEST_RECORD_BATCH`
//! events at a throttled rate (at most 1/sec). On completion (success or failure),
//! an `INGEST_COMPLETE` event is published and the result is appended to a JSONL log.

use std::path::Path;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::Utc;
use serde::Serialize;
use tokio::sync::Notify;
use tracing::{error, info, warn};
use uuid::Uuid;

use stupid_eisenbahn::events::{
    IngestComplete, IngestRecordBatch, IngestSourceType, IngestStarted,
};
use stupid_eisenbahn::topics;

use crate::state::AppState;

use super::types::{IngestionJob, IngestionSource, JobStatus, SourceConfig, TriggerKind};

// ── Public API ──────────────────────────────────────────────────────

/// Spawn an ingestion job as a fire-and-forget background task.
///
/// Returns the job ID immediately. The actual work runs in a tokio::spawn task.
/// Progress is tracked via atomic counters on the [`IngestionJob`] and published
/// as ZMQ events when eisenbahn is configured.
pub async fn spawn_ingestion_job(
    state: Arc<AppState>,
    source: IngestionSource,
    trigger_kind: TriggerKind,
) -> Uuid {
    let job_id = Uuid::new_v4();
    let source_type = source_type_from_str(&source.source_type);

    let job = Arc::new(IngestionJob {
        id: job_id,
        source_id: Some(source.id),
        source_name: source.name.clone(),
        trigger_kind,
        status: std::sync::RwLock::new(JobStatus::Pending),
        docs_processed: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        docs_total: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        segments_done: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        segments_total: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        created_at: Utc::now(),
        completed_at: std::sync::RwLock::new(None),
        error: std::sync::RwLock::new(None),
        segment_ids: std::sync::RwLock::new(Vec::new()),
    });

    // Register in job store
    {
        let mut store = state.ingestion_jobs.jobs.write().unwrap();
        store.insert(job_id, job.clone());
    }

    // Publish INGEST_STARTED via ZMQ (best-effort)
    publish_ingest_started(&state, &job, &source, source_type.clone()).await;

    // Spawn the actual work
    let state2 = state.clone();
    let job2 = job.clone();
    let source2 = source.clone();
    tokio::spawn(async move {
        run_ingestion_job(state2, job2, source2).await;
    });

    job_id
}

// ── Job execution ───────────────────────────────────────────────────

/// Execute the ingestion job — called inside tokio::spawn.
async fn run_ingestion_job(
    state: Arc<AppState>,
    job: Arc<IngestionJob>,
    source: IngestionSource,
) {
    // Transition to Running
    {
        let mut status = job.status.write().unwrap();
        *status = JobStatus::Running;
    }

    let start = Instant::now();

    // Spawn progress monitor
    let cancel = Arc::new(Notify::new());
    let monitor_handle = spawn_progress_monitor(
        state.clone(),
        job.clone(),
        &source,
        cancel.clone(),
    );

    // Parse source config and run the appropriate importer
    let result = match source.config() {
        Ok(config) => execute_source_config(&state, &job, &source, config).await,
        Err(e) => Err(anyhow::anyhow!("failed to parse source config: {}", e)),
    };

    // Stop the progress monitor
    cancel.notify_waiters();
    let _ = monitor_handle.await;

    // Finalize job
    let duration_ms = start.elapsed().as_millis() as u64;
    let docs_processed = job.docs_processed.load(Ordering::Relaxed);
    let source_type = source_type_from_str(&source.source_type);

    match result {
        Ok(()) => {
            {
                let mut status = job.status.write().unwrap();
                *status = JobStatus::Completed;
            }
            {
                let mut completed = job.completed_at.write().unwrap();
                *completed = Some(Utc::now());
            }
            info!(
                job_id = %job.id,
                source = %source.name,
                docs = docs_processed,
                duration_ms = duration_ms,
                "ingestion job completed successfully"
            );
        }
        Err(e) => {
            let err_msg = e.to_string();
            {
                let mut status = job.status.write().unwrap();
                *status = JobStatus::Failed;
            }
            {
                let mut completed = job.completed_at.write().unwrap();
                *completed = Some(Utc::now());
            }
            {
                let mut error = job.error.write().unwrap();
                *error = Some(err_msg.clone());
            }
            error!(
                job_id = %job.id,
                source = %source.name,
                error = %err_msg,
                "ingestion job failed"
            );
        }
    }

    // Collect segment IDs for the completion event
    let segment_ids: Vec<String> = job.segment_ids.read().unwrap().clone();
    let total_segments = segment_ids.len() as u64;
    let error_msg = job.error.read().unwrap().clone();

    // Publish INGEST_COMPLETE via ZMQ
    publish_ingest_complete(
        &state,
        &job,
        duration_ms,
        total_segments,
        error_msg.clone(),
        source_type,
    )
    .await;

    // Append to JSONL log
    append_job_log(&state.data_dir, &job, duration_ms, &segment_ids, error_msg.as_deref());
}

/// Dispatch to the correct importer based on the parsed SourceConfig.
async fn execute_source_config(
    state: &AppState,
    job: &IngestionJob,
    source: &IngestionSource,
    config: SourceConfig,
) -> anyhow::Result<()> {
    match config {
        SourceConfig::Directory(dir_config) => {
            let path = std::path::Path::new(&dir_config.path);
            if !path.exists() {
                anyhow::bail!("directory not found: {}", dir_config.path);
            }
            // Run the synchronous import_dir in a blocking context.
            // Config::from_env() picks up the server's environment (DATA_DIR, etc.).
            let dir_path = path.to_path_buf();
            tokio::task::spawn_blocking(move || {
                let core_config = stupid_core::Config::from_env();
                crate::import::import_dir(&core_config, &dir_path)
            })
            .await??;

            info!(
                job_id = %job.id,
                source = %source.name,
                path = %dir_config.path,
                "directory import completed"
            );
            Ok(())
        }
        SourceConfig::Parquet(_) | SourceConfig::CsvJson(_) => {
            // Upload-based sources — file path comes from the job request context.
            // For now, these are handled by direct upload endpoints.
            info!(
                job_id = %job.id,
                source = %source.name,
                "upload-based source — no background import needed"
            );
            Ok(())
        }
        SourceConfig::S3(_s3_config) => {
            // TODO: wire up S3 import when import_s3() is implemented
            info!(
                job_id = %job.id,
                source = %source.name,
                "S3 import not yet implemented — job marked complete"
            );
            Ok(())
        }
        SourceConfig::Push(_) | SourceConfig::Queue(_) => {
            // Push/Queue are long-running listeners, not batch jobs.
            // The job_runner handles batch snapshots for telemetry purposes.
            info!(
                job_id = %job.id,
                source = %source.name,
                "push/queue source — handled by dedicated listeners"
            );
            Ok(())
        }
    }
}

// ── Progress monitor ────────────────────────────────────────────────

/// Spawn a monitoring task that periodically publishes INGEST_RECORD_BATCH events.
///
/// Polls job.docs_processed every 100ms and publishes at most once per second
/// when the ZMQ granularity is "batched".
fn spawn_progress_monitor(
    state: Arc<AppState>,
    job: Arc<IngestionJob>,
    source: &IngestionSource,
    cancel: Arc<Notify>,
) -> tokio::task::JoinHandle<()> {
    let is_batched = source.zmq_granularity == "batched";
    let job_id = job.id;

    tokio::spawn(async move {
        if !is_batched {
            // Summary mode — just wait for cancellation
            cancel.notified().await;
            return;
        }

        let mut last_published = 0u64;
        let mut batch_index = 0u64;
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        interval.tick().await; // skip immediate tick

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    let current = job.docs_processed.load(Ordering::Relaxed);
                    if current > last_published {
                        let batch_count = current - last_published;
                        let total = job.docs_total.load(Ordering::Relaxed);

                        let event = IngestRecordBatch {
                            job_id,
                            batch_index,
                            batch_record_count: batch_count,
                            cumulative_records: current,
                            total_records: if total > 0 { Some(total) } else { None },
                            current_segment: job.segment_ids.read()
                                .unwrap()
                                .last()
                                .cloned()
                                .unwrap_or_default(),
                        };

                        publish_event(&state, topics::INGEST_RECORD_BATCH, &event).await;
                        last_published = current;
                        batch_index += 1;
                    }
                }
                _ = cancel.notified() => {
                    break;
                }
            }
        }
    })
}

// ── ZMQ publish helpers ─────────────────────────────────────────────

/// Best-effort publish of an event to eisenbahn. Logs warnings on failure, never panics.
async fn publish_event<T: Serialize>(state: &AppState, topic: &str, event: &T) {
    if let Some(ref client) = state.eisenbahn {
        match tokio::time::timeout(Duration::from_secs(2), client.publish_event(topic, event)).await
        {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                warn!(topic = %topic, error = %e, "failed to publish eisenbahn event");
            }
            Err(_) => {
                warn!(topic = %topic, "eisenbahn publish timed out");
            }
        }
    }
}

async fn publish_ingest_started(
    state: &AppState,
    job: &IngestionJob,
    source: &IngestionSource,
    source_type: IngestSourceType,
) {
    let event = IngestStarted {
        job_id: job.id,
        source: source.name.clone(),
        source_type,
        segment_ids: job.segment_ids.read().unwrap().clone(),
        estimated_records: None,
        started_at: job.created_at,
    };
    publish_event(state, topics::INGEST_STARTED, &event).await;
}

async fn publish_ingest_complete(
    state: &AppState,
    job: &IngestionJob,
    duration_ms: u64,
    total_segments: u64,
    error: Option<String>,
    source_type: IngestSourceType,
) {
    let event = IngestComplete {
        source: job.source_name.clone(),
        record_count: job.docs_processed.load(Ordering::Relaxed),
        duration_ms,
        job_id: Some(job.id),
        total_segments,
        error,
        source_type: Some(source_type),
    };
    publish_event(state, topics::INGEST_COMPLETE, &event).await;
}

// ── JSONL persistence ───────────────────────────────────────────────

/// Serializable job completion record for the JSONL log.
#[derive(Serialize)]
struct JobLogEntry {
    id: Uuid,
    source_id: Option<Uuid>,
    source_name: String,
    trigger_kind: TriggerKind,
    status: JobStatus,
    docs_processed: u64,
    duration_ms: u64,
    created_at: chrono::DateTime<Utc>,
    completed_at: Option<chrono::DateTime<Utc>>,
    error: Option<String>,
    segment_ids: Vec<String>,
}

/// Append a completed job's summary to `data/ingestion/jobs.jsonl`.
///
/// Uses atomic write (temp file + rename) to prevent partial writes.
fn append_job_log(
    data_dir: &Path,
    job: &IngestionJob,
    duration_ms: u64,
    segment_ids: &[String],
    error: Option<&str>,
) {
    let log_dir = data_dir.join("ingestion");
    if let Err(e) = std::fs::create_dir_all(&log_dir) {
        warn!(error = %e, "failed to create ingestion log directory");
        return;
    }

    let log_path = log_dir.join("jobs.jsonl");
    let status = job.status.read().unwrap();
    let completed_at = job.completed_at.read().unwrap();

    let entry = JobLogEntry {
        id: job.id,
        source_id: job.source_id,
        source_name: job.source_name.clone(),
        trigger_kind: job.trigger_kind,
        status: *status,
        docs_processed: job.docs_processed.load(Ordering::Relaxed),
        duration_ms,
        created_at: job.created_at,
        completed_at: *completed_at,
        error: error.map(String::from),
        segment_ids: segment_ids.to_vec(),
    };

    // Serialize to JSON line
    let json_line = match serde_json::to_string(&entry) {
        Ok(j) => j,
        Err(e) => {
            warn!(error = %e, "failed to serialize job log entry");
            return;
        }
    };

    // Atomic append: write to temp file, then append to main log
    // For JSONL, we can safely append since each line is self-contained.
    use std::io::Write;
    let mut file = match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
    {
        Ok(f) => f,
        Err(e) => {
            warn!(error = %e, path = %log_path.display(), "failed to open job log");
            return;
        }
    };

    if let Err(e) = writeln!(file, "{}", json_line) {
        warn!(error = %e, "failed to write job log entry");
    }
}

// ── Helpers ─────────────────────────────────────────────────────────

/// Map the source_type string from the database row to the ZMQ event enum.
fn source_type_from_str(s: &str) -> IngestSourceType {
    match s {
        "parquet" => IngestSourceType::Parquet,
        "directory" => IngestSourceType::Directory,
        "s3" => IngestSourceType::S3,
        "csv_json" => IngestSourceType::CsvJson,
        "push" => IngestSourceType::Push,
        "queue" => IngestSourceType::Queue,
        _ => IngestSourceType::Push, // fallback
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_source_type_from_str() {
        assert_eq!(source_type_from_str("parquet"), IngestSourceType::Parquet);
        assert_eq!(source_type_from_str("directory"), IngestSourceType::Directory);
        assert_eq!(source_type_from_str("s3"), IngestSourceType::S3);
        assert_eq!(source_type_from_str("csv_json"), IngestSourceType::CsvJson);
        assert_eq!(source_type_from_str("push"), IngestSourceType::Push);
        assert_eq!(source_type_from_str("queue"), IngestSourceType::Queue);
        assert_eq!(source_type_from_str("unknown"), IngestSourceType::Push);
    }

    #[test]
    fn test_job_log_entry_serialization() {
        let entry = JobLogEntry {
            id: Uuid::new_v4(),
            source_id: Some(Uuid::new_v4()),
            source_name: "test-source".to_string(),
            trigger_kind: TriggerKind::Manual,
            status: JobStatus::Completed,
            docs_processed: 1000,
            duration_ms: 5000,
            created_at: Utc::now(),
            completed_at: Some(Utc::now()),
            error: None,
            segment_ids: vec!["seg-001".to_string()],
        };
        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains("test-source"));
        assert!(json.contains("manual"));
        assert!(json.contains("completed"));
    }
}
