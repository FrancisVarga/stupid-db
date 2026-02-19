//! CRUD handlers for ingestion sources (PostgreSQL-backed).

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::ingestion::source_store::{
    CreateIngestionSource, IngestionSourceStore, UpdateIngestionSource,
};
use crate::ingestion::types::TriggerKind;
use crate::ingestion::job_runner::spawn_ingestion_job;
use crate::state::AppState;

/// Helper: extract pg_pool or return 503.
fn require_pg(state: &AppState) -> Result<&sqlx::PgPool, (StatusCode, Json<Value>)> {
    state.pg_pool.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "error": "PostgreSQL not configured" })),
        )
    })
}

/// Map an IngestionStoreError to an HTTP response.
fn store_err(
    e: crate::ingestion::source_store::IngestionStoreError,
) -> (StatusCode, Json<Value>) {
    let status =
        StatusCode::from_u16(e.status_code()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    (status, Json(json!({ "error": e.to_string() })))
}

/// GET /api/ingestion/sources
pub async fn ingestion_sources_list(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let pool = require_pg(&state)?;
    let sources = IngestionSourceStore::list(pool).await.map_err(store_err)?;
    Ok(Json(serde_json::to_value(sources).unwrap_or_default()))
}

/// POST /api/ingestion/sources
pub async fn ingestion_sources_create(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateIngestionSource>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, Json<Value>)> {
    let pool = require_pg(&state)?;
    let source = IngestionSourceStore::create(pool, req)
        .await
        .map_err(store_err)?;
    Ok((
        StatusCode::CREATED,
        Json(serde_json::to_value(source).unwrap_or_default()),
    ))
}

/// GET /api/ingestion/sources/{id}
pub async fn ingestion_sources_get(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let pool = require_pg(&state)?;
    let source = IngestionSourceStore::get(pool, id)
        .await
        .map_err(store_err)?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": format!("ingestion source not found: {}", id) })),
            )
        })?;
    Ok(Json(serde_json::to_value(source).unwrap_or_default()))
}

/// PUT /api/ingestion/sources/{id}
pub async fn ingestion_sources_update(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateIngestionSource>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let pool = require_pg(&state)?;
    let source = IngestionSourceStore::update(pool, id, req)
        .await
        .map_err(store_err)?;
    Ok(Json(serde_json::to_value(source).unwrap_or_default()))
}

/// DELETE /api/ingestion/sources/{id}
pub async fn ingestion_sources_delete(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<Value>)> {
    let pool = require_pg(&state)?;
    IngestionSourceStore::delete(pool, id)
        .await
        .map_err(store_err)?;
    Ok(StatusCode::NO_CONTENT)
}

/// POST /api/ingestion/sources/{id}/trigger — manually trigger an ingestion job.
pub async fn ingestion_sources_trigger(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let pool = require_pg(&state)?;
    let source = IngestionSourceStore::get(pool, id)
        .await
        .map_err(store_err)?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": format!("ingestion source not found: {}", id) })),
            )
        })?;

    let job_id = spawn_ingestion_job(state.clone(), source, TriggerKind::Manual).await;
    Ok(Json(json!({ "job_id": job_id })))
}
