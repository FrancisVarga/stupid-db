//! Segment partial catalog endpoints: list, get, delete.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use tracing::info;

use crate::api::QueryErrorResponse;
use crate::state::AppState;
use super::types::{store_err, RebuildResponse, SegmentListResponse};

// ── Segment partials ────────────────────────────────────────────

/// List all segment IDs with persisted partial catalogs.
#[utoipa::path(
    get,
    path = "/catalog/segments",
    tag = "Catalog",
    responses(
        (status = 200, description = "List of segment IDs", body = SegmentListResponse),
        (status = 500, description = "Store error", body = QueryErrorResponse)
    )
)]
pub(crate) async fn list_segments(
    State(state): State<Arc<AppState>>,
) -> Result<Json<SegmentListResponse>, (StatusCode, Json<QueryErrorResponse>)> {
    let segment_ids = state.catalog_store.list_partials().map_err(store_err)?;
    Ok(Json(SegmentListResponse { segment_ids }))
}

/// Get the partial catalog for a specific segment.
#[utoipa::path(
    get,
    path = "/catalog/segments/{id}",
    tag = "Catalog",
    params(
        ("id" = String, Path, description = "Segment ID (URL-encoded if contains /)")
    ),
    responses(
        (status = 200, description = "Partial catalog for segment", body = Object),
        (status = 404, description = "Segment not found"),
        (status = 500, description = "Store error", body = QueryErrorResponse)
    )
)]
pub(crate) async fn get_segment(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<stupid_catalog::PartialCatalog>, (StatusCode, Json<QueryErrorResponse>)> {
    let decoded = urlencoding::decode(&id).map(|c| c.into_owned()).unwrap_or(id);
    match state.catalog_store.load_partial(&decoded) {
        Ok(Some(p)) => Ok(Json(p)),
        Ok(None) => Err((
            StatusCode::NOT_FOUND,
            Json(QueryErrorResponse {
                error: format!("Segment '{decoded}' not found."),
            }),
        )),
        Err(e) => Err(store_err(e)),
    }
}

/// Remove a segment's partial catalog and rebuild the merged catalog.
#[utoipa::path(
    delete,
    path = "/catalog/segments/{id}",
    tag = "Catalog",
    params(
        ("id" = String, Path, description = "Segment ID (URL-encoded if contains /)")
    ),
    responses(
        (status = 200, description = "Segment removed, catalog rebuilt", body = RebuildResponse),
        (status = 500, description = "Store error", body = QueryErrorResponse)
    )
)]
pub(crate) async fn delete_segment(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<RebuildResponse>, (StatusCode, Json<QueryErrorResponse>)> {
    let decoded = urlencoding::decode(&id).map(|c| c.into_owned()).unwrap_or(id);
    let catalog = state
        .catalog_store
        .remove_segment(&decoded)
        .map_err(store_err)?;

    let segment_count = state.catalog_store.list_partials().unwrap_or_default().len();

    // Update in-memory catalog.
    {
        let mut lock = state.catalog.write().await;
        *lock = Some(catalog.clone());
    }

    info!("Segment '{}' removed via API, catalog rebuilt", decoded);

    Ok(Json(RebuildResponse {
        total_nodes: catalog.total_nodes,
        total_edges: catalog.total_edges,
        segment_count,
    }))
}
