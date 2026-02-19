//! Handlers for listing and inspecting in-memory ingestion jobs.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::ingestion::types::IngestionJob;
use crate::state::AppState;

/// Serialize an `IngestionJob` to a JSON Value.
///
/// Manual construction is required because the job uses `RwLock` and `AtomicU64`
/// fields that don't implement `Serialize`.
fn job_to_json(job: &IngestionJob) -> Value {
    let status = job.status.read().unwrap();
    let completed_at = job.completed_at.read().unwrap();
    let error = job.error.read().unwrap();

    json!({
        "id": job.id,
        "source_id": job.source_id,
        "source_name": job.source_name,
        "trigger_kind": job.trigger_kind,
        "status": *status,
        "docs_processed": job.docs_processed.load(Ordering::Relaxed),
        "docs_total": job.docs_total.load(Ordering::Relaxed),
        "segments_done": job.segments_done.load(Ordering::Relaxed),
        "segments_total": job.segments_total.load(Ordering::Relaxed),
        "created_at": job.created_at,
        "completed_at": *completed_at,
        "error": error.clone(),
    })
}

/// GET /api/ingestion/jobs — list all active/recent jobs.
pub async fn ingestion_jobs_list(
    State(state): State<Arc<AppState>>,
) -> Json<Value> {
    let store = state.ingestion_jobs.jobs.read().unwrap();
    let jobs: Vec<Value> = store.values().map(|job| job_to_json(job)).collect();
    Json(json!(jobs))
}

/// GET /api/ingestion/jobs/{id} — get a single job by UUID.
pub async fn ingestion_jobs_get(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let store = state.ingestion_jobs.jobs.read().unwrap();
    let job = store.get(&id).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("ingestion job not found: {}", id) })),
        )
    })?;
    Ok(Json(job_to_json(job)))
}
