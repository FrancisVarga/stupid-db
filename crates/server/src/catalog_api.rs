//! Catalog API sub-router.
//!
//! Exposes the full catalog surface: merged catalog, segment partials,
//! external SQL sources, snapshots, and query plan execution.
//!
//! Mount via `.merge(catalog_router())` in main.rs.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::api::QueryErrorResponse;
use crate::state::AppState;

// ── Response types ──────────────────────────────────────────────

#[derive(Serialize, utoipa::ToSchema)]
pub struct SegmentListResponse {
    pub segment_ids: Vec<String>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct RebuildResponse {
    pub total_nodes: usize,
    pub total_edges: usize,
    pub segment_count: usize,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct SnapshotResponse {
    pub filename: String,
}

/// Schema type for OpenAPI documentation of the query plan request body.
#[derive(Serialize, Deserialize, utoipa::ToSchema)]
#[allow(dead_code)]
pub struct QueryExecuteRequest {
    /// Ordered list of query steps (filter, traversal, aggregate).
    pub steps: Vec<serde_json::Value>,
}

// ── Helpers ─────────────────────────────────────────────────────

fn store_err(e: impl std::fmt::Display) -> (StatusCode, Json<QueryErrorResponse>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(QueryErrorResponse {
            error: format!("Catalog store error: {e}"),
        }),
    )
}

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

// ── External sources ────────────────────────────────────────────

/// List all external SQL sources (Athena, Trino, etc.).
#[utoipa::path(
    get,
    path = "/catalog/externals",
    tag = "Catalog",
    responses(
        (status = 200, description = "List of external sources", body = [Object]),
        (status = 500, description = "Store error", body = QueryErrorResponse)
    )
)]
pub(crate) async fn list_externals(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<stupid_catalog::ExternalSource>>, (StatusCode, Json<QueryErrorResponse>)> {
    let sources = state.catalog_store.list_external_sources().map_err(store_err)?;
    Ok(Json(sources))
}

/// Get a specific external source by kind and connection ID.
#[utoipa::path(
    get,
    path = "/catalog/externals/{kind}/{connection_id}",
    tag = "Catalog",
    params(
        ("kind" = String, Path, description = "Source kind (e.g. athena, trino, postgres)"),
        ("connection_id" = String, Path, description = "Connection identifier")
    ),
    responses(
        (status = 200, description = "External source details", body = Object),
        (status = 404, description = "External source not found"),
        (status = 500, description = "Store error", body = QueryErrorResponse)
    )
)]
pub(crate) async fn get_external(
    State(state): State<Arc<AppState>>,
    Path((kind, connection_id)): Path<(String, String)>,
) -> Result<Json<stupid_catalog::ExternalSource>, (StatusCode, Json<QueryErrorResponse>)> {
    match state.catalog_store.load_external_source(&kind, &connection_id) {
        Ok(Some(s)) => Ok(Json(s)),
        Ok(None) => Err((
            StatusCode::NOT_FOUND,
            Json(QueryErrorResponse {
                error: format!("External source '{kind}-{connection_id}' not found."),
            }),
        )),
        Err(e) => Err(store_err(e)),
    }
}

/// Add or update an external SQL source.
#[utoipa::path(
    post,
    path = "/catalog/externals",
    tag = "Catalog",
    request_body = Object,
    responses(
        (status = 201, description = "External source saved"),
        (status = 500, description = "Store error", body = QueryErrorResponse)
    )
)]
pub(crate) async fn add_external(
    State(state): State<Arc<AppState>>,
    Json(source): Json<stupid_catalog::ExternalSource>,
) -> Result<StatusCode, (StatusCode, Json<QueryErrorResponse>)> {
    state
        .catalog_store
        .save_external_source(&source)
        .map_err(store_err)?;

    info!(
        "External source '{}-{}' saved via API",
        source.kind, source.connection_id
    );

    Ok(StatusCode::CREATED)
}

/// Remove an external source by kind and connection ID.
#[utoipa::path(
    delete,
    path = "/catalog/externals/{kind}/{connection_id}",
    tag = "Catalog",
    params(
        ("kind" = String, Path, description = "Source kind"),
        ("connection_id" = String, Path, description = "Connection identifier")
    ),
    responses(
        (status = 204, description = "External source removed"),
        (status = 500, description = "Store error", body = QueryErrorResponse)
    )
)]
pub(crate) async fn delete_external(
    State(state): State<Arc<AppState>>,
    Path((kind, connection_id)): Path<(String, String)>,
) -> Result<StatusCode, (StatusCode, Json<QueryErrorResponse>)> {
    state
        .catalog_store
        .remove_external_source(&kind, &connection_id)
        .map_err(store_err)?;

    info!("External source '{kind}-{connection_id}' removed via API");

    Ok(StatusCode::NO_CONTENT)
}

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

// ── Query execution ─────────────────────────────────────────────

/// Execute a structured query plan against the knowledge graph.
#[utoipa::path(
    post,
    path = "/catalog/query",
    tag = "Catalog",
    request_body = QueryExecuteRequest,
    responses(
        (status = 200, description = "Query results", body = [Object]),
        (status = 400, description = "Invalid query plan", body = QueryErrorResponse),
        (status = 503, description = "Service not ready", body = crate::api::NotReadyResponse)
    )
)]
pub(crate) async fn execute_query(
    State(state): State<Arc<AppState>>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<Vec<serde_json::Value>>, (StatusCode, Json<QueryErrorResponse>)> {
    // Require graph to be loaded.
    crate::api::require_ready(&state).await.map_err(|(status, body)| {
        (
            status,
            Json(QueryErrorResponse {
                error: body.error.to_string(),
            }),
        )
    })?;

    // Parse the query plan from the JSON body.
    let plan: stupid_catalog::plan::QueryPlan =
        serde_json::from_value(body).map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                Json(QueryErrorResponse {
                    error: format!("Invalid query plan: {e}"),
                }),
            )
        })?;

    // Execute against the graph.
    let graph = state.graph.read().await;
    let results =
        stupid_catalog::QueryExecutor::execute(&plan, &graph).map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                Json(QueryErrorResponse {
                    error: format!("Query execution failed: {e}"),
                }),
            )
        })?;

    Ok(Json(results))
}

// ── Router ──────────────────────────────────────────────────────

/// Build the catalog sub-router.
///
/// Mount on the main router with `.merge(catalog_router())`.
pub fn catalog_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/catalog", get(get_catalog))
        .route("/catalog/manifest", get(get_manifest))
        .route("/catalog/rebuild", post(rebuild_catalog))
        .route("/catalog/segments", get(list_segments))
        .route(
            "/catalog/segments/{id}",
            get(get_segment).delete(delete_segment),
        )
        .route(
            "/catalog/externals",
            get(list_externals).post(add_external),
        )
        .route(
            "/catalog/externals/{kind}/{connection_id}",
            get(get_external).delete(delete_external),
        )
        .route("/catalog/snapshots", post(create_snapshot))
        .route("/catalog/query", post(execute_query))
}
