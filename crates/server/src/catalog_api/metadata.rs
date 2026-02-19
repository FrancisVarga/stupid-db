//! Catalog metadata endpoints: get catalog, get manifest, rebuild.

use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use tracing::info;

use crate::api::QueryErrorResponse;
use crate::state::AppState;
use super::types::{store_err, RebuildResponse};

// ── Catalog metadata ────────────────────────────────────────────

/// Return the current merged entity/schema catalog.
#[utoipa::path(
    get,
    path = "/catalog",
    tag = "Catalog",
    responses(
        (status = 200, description = "Current merged catalog", body = Object),
        (status = 503, description = "Service not ready", body = crate::api::NotReadyResponse)
    )
)]
pub(crate) async fn get_catalog(
    State(state): State<Arc<AppState>>,
) -> Result<Json<stupid_catalog::Catalog>, (StatusCode, Json<crate::api::NotReadyResponse>)> {
    crate::api::require_ready(&state).await?;
    let catalog_lock = state.catalog.read().await;
    match catalog_lock.as_ref() {
        Some(cat) => Ok(Json(cat.clone())),
        None => {
            let status = state.loading.to_status().await;
            Err((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(crate::api::NotReadyResponse {
                    error: "Catalog not yet built.",
                    loading: status,
                }),
            ))
        }
    }
}

/// Return the catalog manifest (segment IDs, hash, timestamp).
#[utoipa::path(
    get,
    path = "/catalog/manifest",
    tag = "Catalog",
    responses(
        (status = 200, description = "Current catalog manifest", body = Object),
        (status = 404, description = "No manifest found"),
        (status = 500, description = "Store error", body = QueryErrorResponse)
    )
)]
pub(crate) async fn get_manifest(
    State(state): State<Arc<AppState>>,
) -> Result<Json<stupid_catalog::CatalogManifest>, (StatusCode, Json<QueryErrorResponse>)> {
    match state.catalog_store.load_manifest() {
        Ok(Some(m)) => Ok(Json(m)),
        Ok(None) => Err((
            StatusCode::NOT_FOUND,
            Json(QueryErrorResponse {
                error: "No catalog manifest found.".into(),
            }),
        )),
        Err(e) => Err(store_err(e)),
    }
}

/// Rebuild the merged catalog from all persisted segment partials.
#[utoipa::path(
    post,
    path = "/catalog/rebuild",
    tag = "Catalog",
    responses(
        (status = 200, description = "Catalog rebuilt", body = RebuildResponse),
        (status = 500, description = "Rebuild failed", body = QueryErrorResponse)
    )
)]
pub(crate) async fn rebuild_catalog(
    State(state): State<Arc<AppState>>,
) -> Result<Json<RebuildResponse>, (StatusCode, Json<QueryErrorResponse>)> {
    let catalog = state.catalog_store.rebuild_from_partials().map_err(store_err)?;

    let segment_count = state.catalog_store.list_partials().unwrap_or_default().len();

    // Update in-memory catalog.
    {
        let mut lock = state.catalog.write().await;
        *lock = Some(catalog.clone());
    }

    info!(
        "Catalog rebuilt via API: {} nodes, {} edges from {} segments",
        catalog.total_nodes, catalog.total_edges, segment_count
    );

    Ok(Json(RebuildResponse {
        total_nodes: catalog.total_nodes,
        total_edges: catalog.total_edges,
        segment_count,
    }))
}
