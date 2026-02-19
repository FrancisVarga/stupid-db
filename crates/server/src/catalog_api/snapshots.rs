//! Catalog snapshot endpoints.

use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use tracing::info;

use crate::api::QueryErrorResponse;
use crate::state::AppState;
use super::types::{store_err, SnapshotResponse};

// ── Snapshots ───────────────────────────────────────────────────

/// Create a timestamped snapshot of the current catalog.
#[utoipa::path(
    post,
    path = "/catalog/snapshots",
    tag = "Catalog",
    responses(
        (status = 201, description = "Snapshot created", body = SnapshotResponse),
        (status = 503, description = "Service not ready", body = crate::api::NotReadyResponse),
        (status = 500, description = "Store error", body = QueryErrorResponse)
    )
)]
pub(crate) async fn create_snapshot(
    State(state): State<Arc<AppState>>,
) -> Result<(StatusCode, Json<SnapshotResponse>), (StatusCode, Json<QueryErrorResponse>)> {
    let catalog_lock = state.catalog.read().await;
    let catalog = catalog_lock.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(QueryErrorResponse {
                error: "Catalog not yet built — cannot snapshot.".into(),
            }),
        )
    })?;

    let filename = state
        .catalog_store
        .save_snapshot(catalog)
        .map_err(store_err)?;

    info!("Catalog snapshot created: {filename}");

    Ok((StatusCode::CREATED, Json(SnapshotResponse { filename })))
}
