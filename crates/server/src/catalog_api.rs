//! Catalog API sub-router.
//!
//! Exposes the full catalog surface: merged catalog, segment partials,
//! external SQL sources, snapshots, and query plan execution.
//!
//! Mount via `.merge(catalog_router())` in main.rs.

use std::sync::Arc;

use axum::extract::{Path, Query, State};
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

// ── External source drill-down response types ───────────────────

/// Lightweight summary of an external source (no nested databases/tables/columns).
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ExternalSourceSummary {
    /// Human-readable name.
    pub name: String,
    /// Source kind (e.g. "athena", "trino", "postgres").
    pub kind: String,
    /// Connection identifier for routing queries.
    pub connection_id: String,
    /// Number of databases in this source.
    pub database_count: usize,
}

/// Lightweight summary of a database within an external source.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DatabaseSummary {
    /// Database name.
    pub name: String,
    /// Number of tables in this database.
    pub table_count: usize,
}

/// Lightweight summary of a table within a database.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct TableSummary {
    /// Table name.
    pub name: String,
    /// Number of columns in this table.
    pub column_count: usize,
}

// ── Query parameter types ───────────────────────────────────────

/// Search filter for list endpoints.
#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct SearchQuery {
    /// Case-insensitive substring match on name.
    #[serde(default)]
    pub search: Option<String>,
}

/// Depth control for single-source endpoint.
#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct DepthQuery {
    /// Response depth: "shallow" (default) returns summary only,
    /// "full" returns the complete nested tree.
    #[serde(default)]
    pub depth: Option<String>,
}

/// Filter parameters for column listing.
#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct ColumnFilterQuery {
    /// Case-insensitive substring match on column name.
    #[serde(default)]
    pub search: Option<String>,
    /// Exact match on column data type (e.g. "bigint", "timestamp").
    #[serde(default)]
    pub data_type: Option<String>,
}

/// Path parameters for database-level endpoints.
#[derive(Debug, Deserialize)]
pub struct DatabasePathParams {
    pub kind: String,
    pub connection_id: String,
    pub db_name: String,
}

/// Path parameters for table-level endpoints.
#[derive(Debug, Deserialize)]
pub struct TablePathParams {
    pub kind: String,
    pub connection_id: String,
    pub db_name: String,
    pub table_name: String,
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

/// Helper: load an external source or return 404.
fn load_source(
    store: &stupid_catalog::CatalogStore,
    kind: &str,
    connection_id: &str,
) -> Result<stupid_catalog::ExternalSource, (StatusCode, Json<QueryErrorResponse>)> {
    match store.load_external_source(kind, connection_id) {
        Ok(Some(s)) => Ok(s),
        Ok(None) => Err((
            StatusCode::NOT_FOUND,
            Json(QueryErrorResponse {
                error: format!("External source '{kind}-{connection_id}' not found."),
            }),
        )),
        Err(e) => Err(store_err(e)),
    }
}

/// Helper: find a database within a source or return 404.
fn find_database<'a>(
    source: &'a stupid_catalog::ExternalSource,
    db_name: &str,
) -> Result<&'a stupid_catalog::ExternalDatabase, (StatusCode, Json<QueryErrorResponse>)> {
    source
        .databases
        .iter()
        .find(|db| db.name == db_name)
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(QueryErrorResponse {
                    error: format!(
                        "Database '{}' not found in source '{}-{}'.",
                        db_name, source.kind, source.connection_id
                    ),
                }),
            )
        })
}

/// Helper: find a table within a database or return 404.
fn find_table<'a>(
    db: &'a stupid_catalog::ExternalDatabase,
    table_name: &str,
    source: &stupid_catalog::ExternalSource,
) -> Result<&'a stupid_catalog::ExternalTable, (StatusCode, Json<QueryErrorResponse>)> {
    db.tables
        .iter()
        .find(|t| t.name == table_name)
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(QueryErrorResponse {
                    error: format!(
                        "Table '{}' not found in database '{}' of source '{}-{}'.",
                        table_name, db.name, source.kind, source.connection_id
                    ),
                }),
            )
        })
}

/// List all external SQL sources as lightweight summaries.
///
/// Returns name, kind, connection_id, and database_count — no nested data.
/// Use `?search=` to filter by name (case-insensitive substring match).
#[utoipa::path(
    get,
    path = "/catalog/externals",
    tag = "Catalog",
    params(SearchQuery),
    responses(
        (status = 200, description = "Lightweight list of external sources", body = [ExternalSourceSummary]),
        (status = 500, description = "Store error", body = QueryErrorResponse)
    )
)]
pub(crate) async fn list_externals(
    State(state): State<Arc<AppState>>,
    Query(params): Query<SearchQuery>,
) -> Result<Json<Vec<ExternalSourceSummary>>, (StatusCode, Json<QueryErrorResponse>)> {
    let sources = state.catalog_store.list_external_sources().map_err(store_err)?;
    let search = params.search.map(|s| s.to_lowercase());

    let summaries: Vec<ExternalSourceSummary> = sources
        .into_iter()
        .filter(|s| {
            search
                .as_ref()
                .map_or(true, |q| s.name.to_lowercase().contains(q))
        })
        .map(|s| ExternalSourceSummary {
            database_count: s.databases.len(),
            name: s.name,
            kind: s.kind,
            connection_id: s.connection_id,
        })
        .collect();

    Ok(Json(summaries))
}

/// Get a specific external source by kind and connection ID.
///
/// By default returns a lightweight summary. Use `?depth=full` to get
/// the complete nested tree (databases, tables, columns).
#[utoipa::path(
    get,
    path = "/catalog/externals/{kind}/{connection_id}",
    tag = "Catalog",
    params(
        ("kind" = String, Path, description = "Source kind (e.g. athena, trino, postgres)"),
        ("connection_id" = String, Path, description = "Connection identifier"),
        DepthQuery,
    ),
    responses(
        (status = 200, description = "External source (summary or full tree)", body = Object),
        (status = 404, description = "External source not found"),
        (status = 500, description = "Store error", body = QueryErrorResponse)
    )
)]
pub(crate) async fn get_external(
    State(state): State<Arc<AppState>>,
    Path((kind, connection_id)): Path<(String, String)>,
    Query(params): Query<DepthQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<QueryErrorResponse>)> {
    let source = load_source(&state.catalog_store, &kind, &connection_id)?;

    let is_full = params
        .depth
        .as_deref()
        .map_or(false, |d| d.eq_ignore_ascii_case("full"));

    if is_full {
        Ok(Json(serde_json::to_value(&source).unwrap()))
    } else {
        let summary = ExternalSourceSummary {
            database_count: source.databases.len(),
            name: source.name,
            kind: source.kind,
            connection_id: source.connection_id,
        };
        Ok(Json(serde_json::to_value(&summary).unwrap()))
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

// ── External source drill-down endpoints ────────────────────────

/// List databases for a specific external source.
///
/// Returns database names with table counts. Use `?search=` to filter.
#[utoipa::path(
    get,
    path = "/catalog/externals/{kind}/{connection_id}/databases",
    tag = "Catalog",
    params(
        ("kind" = String, Path, description = "Source kind (e.g. athena, trino, postgres)"),
        ("connection_id" = String, Path, description = "Connection identifier"),
        SearchQuery,
    ),
    responses(
        (status = 200, description = "List of databases with table counts", body = [DatabaseSummary]),
        (status = 404, description = "External source not found"),
        (status = 500, description = "Store error", body = QueryErrorResponse)
    )
)]
pub(crate) async fn list_databases(
    State(state): State<Arc<AppState>>,
    Path((kind, connection_id)): Path<(String, String)>,
    Query(params): Query<SearchQuery>,
) -> Result<Json<Vec<DatabaseSummary>>, (StatusCode, Json<QueryErrorResponse>)> {
    let source = load_source(&state.catalog_store, &kind, &connection_id)?;
    let search = params.search.map(|s| s.to_lowercase());

    let databases: Vec<DatabaseSummary> = source
        .databases
        .iter()
        .filter(|db| {
            search
                .as_ref()
                .map_or(true, |q| db.name.to_lowercase().contains(q))
        })
        .map(|db| DatabaseSummary {
            name: db.name.clone(),
            table_count: db.tables.len(),
        })
        .collect();

    Ok(Json(databases))
}

/// List tables for a specific database within an external source.
///
/// Returns table names with column counts. Use `?search=` to filter.
#[utoipa::path(
    get,
    path = "/catalog/externals/{kind}/{connection_id}/databases/{db_name}/tables",
    tag = "Catalog",
    params(
        ("kind" = String, Path, description = "Source kind"),
        ("connection_id" = String, Path, description = "Connection identifier"),
        ("db_name" = String, Path, description = "Database name"),
        SearchQuery,
    ),
    responses(
        (status = 200, description = "List of tables with column counts", body = [TableSummary]),
        (status = 404, description = "Source or database not found"),
        (status = 500, description = "Store error", body = QueryErrorResponse)
    )
)]
pub(crate) async fn list_tables(
    State(state): State<Arc<AppState>>,
    Path(params): Path<DatabasePathParams>,
    Query(search): Query<SearchQuery>,
) -> Result<Json<Vec<TableSummary>>, (StatusCode, Json<QueryErrorResponse>)> {
    let source = load_source(&state.catalog_store, &params.kind, &params.connection_id)?;
    let db = find_database(&source, &params.db_name)?;
    let search_lc = search.search.map(|s| s.to_lowercase());

    let tables: Vec<TableSummary> = db
        .tables
        .iter()
        .filter(|t| {
            search_lc
                .as_ref()
                .map_or(true, |q| t.name.to_lowercase().contains(q))
        })
        .map(|t| TableSummary {
            name: t.name.clone(),
            column_count: t.columns.len(),
        })
        .collect();

    Ok(Json(tables))
}

/// List columns for a specific table within a database.
///
/// Returns column names and data types. Use `?search=` for name filtering
/// and `?data_type=` for exact type matching.
#[utoipa::path(
    get,
    path = "/catalog/externals/{kind}/{connection_id}/databases/{db_name}/tables/{table_name}/columns",
    tag = "Catalog",
    params(
        ("kind" = String, Path, description = "Source kind"),
        ("connection_id" = String, Path, description = "Connection identifier"),
        ("db_name" = String, Path, description = "Database name"),
        ("table_name" = String, Path, description = "Table name"),
        ColumnFilterQuery,
    ),
    responses(
        (status = 200, description = "List of columns with data types", body = [Object]),
        (status = 404, description = "Source, database, or table not found"),
        (status = 500, description = "Store error", body = QueryErrorResponse)
    )
)]
pub(crate) async fn list_columns(
    State(state): State<Arc<AppState>>,
    Path(params): Path<TablePathParams>,
    Query(filter): Query<ColumnFilterQuery>,
) -> Result<Json<Vec<stupid_catalog::ExternalColumn>>, (StatusCode, Json<QueryErrorResponse>)> {
    let source = load_source(&state.catalog_store, &params.kind, &params.connection_id)?;
    let db = find_database(&source, &params.db_name)?;
    let table = find_table(db, &params.table_name, &source)?;

    let search_lc = filter.search.map(|s| s.to_lowercase());

    let columns: Vec<stupid_catalog::ExternalColumn> = table
        .columns
        .iter()
        .filter(|c| {
            search_lc
                .as_ref()
                .map_or(true, |q| c.name.to_lowercase().contains(q))
        })
        .filter(|c| {
            filter
                .data_type
                .as_ref()
                .map_or(true, |dt| c.data_type == *dt)
        })
        .cloned()
        .collect();

    Ok(Json(columns))
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
        // External sources: list (lightweight) + create
        .route(
            "/catalog/externals",
            get(list_externals).post(add_external),
        )
        // External sources: get (shallow/full) + delete
        .route(
            "/catalog/externals/{kind}/{connection_id}",
            get(get_external).delete(delete_external),
        )
        // Drill-down: databases
        .route(
            "/catalog/externals/{kind}/{connection_id}/databases",
            get(list_databases),
        )
        // Drill-down: tables
        .route(
            "/catalog/externals/{kind}/{connection_id}/databases/{db_name}/tables",
            get(list_tables),
        )
        // Drill-down: columns
        .route(
            "/catalog/externals/{kind}/{connection_id}/databases/{db_name}/tables/{table_name}/columns",
            get(list_columns),
        )
        .route("/catalog/snapshots", post(create_snapshot))
        .route("/catalog/query", post(execute_query))
}

// ── Tests ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use stupid_catalog::{ExternalColumn, ExternalDatabase, ExternalSource, ExternalTable};

    fn sample_source() -> ExternalSource {
        ExternalSource {
            name: "Production Data Lake".into(),
            kind: "athena".into(),
            connection_id: "prod-lake".into(),
            databases: vec![
                ExternalDatabase {
                    name: "analytics".into(),
                    tables: vec![
                        ExternalTable {
                            name: "events".into(),
                            columns: vec![
                                ExternalColumn { name: "id".into(), data_type: "bigint".into() },
                                ExternalColumn { name: "user_id".into(), data_type: "bigint".into() },
                                ExternalColumn { name: "ts".into(), data_type: "timestamp".into() },
                                ExternalColumn { name: "event_type".into(), data_type: "varchar".into() },
                            ],
                        },
                        ExternalTable {
                            name: "users".into(),
                            columns: vec![
                                ExternalColumn { name: "id".into(), data_type: "bigint".into() },
                                ExternalColumn { name: "username".into(), data_type: "varchar".into() },
                            ],
                        },
                    ],
                },
                ExternalDatabase {
                    name: "raw".into(),
                    tables: vec![ExternalTable {
                        name: "logs".into(),
                        columns: vec![
                            ExternalColumn { name: "line".into(), data_type: "text".into() },
                        ],
                    }],
                },
            ],
        }
    }

    fn sample_sources() -> Vec<ExternalSource> {
        vec![
            sample_source(),
            ExternalSource {
                name: "Staging Lake".into(),
                kind: "athena".into(),
                connection_id: "staging".into(),
                databases: vec![],
            },
            ExternalSource {
                name: "Production Postgres".into(),
                kind: "postgres".into(),
                connection_id: "prod-pg".into(),
                databases: vec![ExternalDatabase {
                    name: "app".into(),
                    tables: vec![],
                }],
            },
        ]
    }

    // ── ExternalSourceSummary mapping ─────────────────────────

    #[test]
    fn source_summary_has_correct_database_count() {
        let src = sample_source();
        let summary = ExternalSourceSummary {
            database_count: src.databases.len(),
            name: src.name.clone(),
            kind: src.kind.clone(),
            connection_id: src.connection_id.clone(),
        };
        assert_eq!(summary.database_count, 2);
        assert_eq!(summary.name, "Production Data Lake");
    }

    #[test]
    fn source_summary_excludes_nested_data() {
        let src = sample_source();
        let summary = ExternalSourceSummary {
            database_count: src.databases.len(),
            name: src.name,
            kind: src.kind,
            connection_id: src.connection_id,
        };
        let json = serde_json::to_value(&summary).unwrap();
        assert!(json.get("databases").is_none());
        assert!(json.get("tables").is_none());
        assert!(json.get("columns").is_none());
        assert_eq!(json["database_count"], 2);
    }

    // ── Search filtering ──────────────────────────────────────

    #[test]
    fn search_filters_sources_case_insensitive() {
        let sources = sample_sources();
        let search = Some("production".to_lowercase());

        let filtered: Vec<_> = sources
            .iter()
            .filter(|s| {
                search
                    .as_ref()
                    .map_or(true, |q| s.name.to_lowercase().contains(q))
            })
            .collect();

        assert_eq!(filtered.len(), 2); // "Production Data Lake" + "Production Postgres"
    }

    #[test]
    fn search_returns_all_when_none() {
        let sources = sample_sources();
        let search: Option<String> = None;

        let filtered: Vec<_> = sources
            .iter()
            .filter(|s| {
                search
                    .as_ref()
                    .map_or(true, |q| s.name.to_lowercase().contains(q))
            })
            .collect();

        assert_eq!(filtered.len(), 3);
    }

    // ── DatabaseSummary mapping ───────────────────────────────

    #[test]
    fn database_summary_has_correct_table_count() {
        let src = sample_source();
        let summaries: Vec<DatabaseSummary> = src
            .databases
            .iter()
            .map(|db| DatabaseSummary {
                name: db.name.clone(),
                table_count: db.tables.len(),
            })
            .collect();

        assert_eq!(summaries.len(), 2);
        assert_eq!(summaries[0].name, "analytics");
        assert_eq!(summaries[0].table_count, 2);
        assert_eq!(summaries[1].name, "raw");
        assert_eq!(summaries[1].table_count, 1);
    }

    #[test]
    fn database_search_filters_by_name() {
        let src = sample_source();
        let search = Some("ana".to_string());

        let filtered: Vec<_> = src
            .databases
            .iter()
            .filter(|db| {
                search
                    .as_ref()
                    .map_or(true, |q| db.name.to_lowercase().contains(&q.to_lowercase()))
            })
            .collect();

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].name, "analytics");
    }

    // ── TableSummary mapping ──────────────────────────────────

    #[test]
    fn table_summary_has_correct_column_count() {
        let src = sample_source();
        let db = &src.databases[0]; // analytics

        let summaries: Vec<TableSummary> = db
            .tables
            .iter()
            .map(|t| TableSummary {
                name: t.name.clone(),
                column_count: t.columns.len(),
            })
            .collect();

        assert_eq!(summaries.len(), 2);
        assert_eq!(summaries[0].name, "events");
        assert_eq!(summaries[0].column_count, 4);
        assert_eq!(summaries[1].name, "users");
        assert_eq!(summaries[1].column_count, 2);
    }

    // ── Column filtering ──────────────────────────────────────

    #[test]
    fn column_filter_by_name() {
        let src = sample_source();
        let table = &src.databases[0].tables[0]; // events
        let search = Some("user".to_lowercase());

        let filtered: Vec<_> = table
            .columns
            .iter()
            .filter(|c| {
                search
                    .as_ref()
                    .map_or(true, |q| c.name.to_lowercase().contains(q))
            })
            .collect();

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].name, "user_id");
    }

    #[test]
    fn column_filter_by_data_type() {
        let src = sample_source();
        let table = &src.databases[0].tables[0]; // events
        let data_type = Some("bigint".to_string());

        let filtered: Vec<_> = table
            .columns
            .iter()
            .filter(|c| {
                data_type
                    .as_ref()
                    .map_or(true, |dt| c.data_type == *dt)
            })
            .collect();

        assert_eq!(filtered.len(), 2); // id + user_id
    }

    #[test]
    fn column_filter_combined_search_and_type() {
        let src = sample_source();
        let table = &src.databases[0].tables[0]; // events
        let search = Some("id".to_lowercase());
        let data_type = Some("bigint".to_string());

        let filtered: Vec<_> = table
            .columns
            .iter()
            .filter(|c| {
                search
                    .as_ref()
                    .map_or(true, |q| c.name.to_lowercase().contains(q))
            })
            .filter(|c| {
                data_type
                    .as_ref()
                    .map_or(true, |dt| c.data_type == *dt)
            })
            .collect();

        assert_eq!(filtered.len(), 2); // id + user_id (both contain "id" and are bigint)
    }

    #[test]
    fn column_filter_no_match() {
        let src = sample_source();
        let table = &src.databases[0].tables[0]; // events

        let filtered: Vec<_> = table
            .columns
            .iter()
            .filter(|c| c.data_type == "boolean")
            .collect();

        assert!(filtered.is_empty());
    }

    // ── find_database / find_table helpers ─────────────────────

    #[test]
    fn find_database_returns_ok_for_existing() {
        let src = sample_source();
        let result = find_database(&src, "analytics");
        assert!(result.is_ok());
        assert_eq!(result.unwrap().name, "analytics");
    }

    #[test]
    fn find_database_returns_404_for_missing() {
        let src = sample_source();
        let result = find_database(&src, "nonexistent");
        assert!(result.is_err());
        let (status, body) = result.unwrap_err();
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert!(body.error.contains("nonexistent"));
    }

    #[test]
    fn find_table_returns_ok_for_existing() {
        let src = sample_source();
        let db = &src.databases[0];
        let result = find_table(db, "events", &src);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().name, "events");
    }

    #[test]
    fn find_table_returns_404_for_missing() {
        let src = sample_source();
        let db = &src.databases[0];
        let result = find_table(db, "nonexistent", &src);
        assert!(result.is_err());
        let (status, body) = result.unwrap_err();
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert!(body.error.contains("nonexistent"));
    }

    // ── Depth param ───────────────────────────────────────────

    #[test]
    fn depth_full_returns_complete_tree() {
        let src = sample_source();
        let is_full = Some("full".to_string())
            .as_deref()
            .map_or(false, |d| d.eq_ignore_ascii_case("full"));
        assert!(is_full);

        let json = serde_json::to_value(&src).unwrap();
        assert!(json.get("databases").is_some());
    }

    #[test]
    fn depth_shallow_returns_summary() {
        let is_full = Some("shallow".to_string())
            .as_deref()
            .map_or(false, |d| d.eq_ignore_ascii_case("full"));
        assert!(!is_full);

        let is_full_none: bool = None::<String>
            .as_deref()
            .map_or(false, |d: &str| d.eq_ignore_ascii_case("full"));
        assert!(!is_full_none);
    }

    // ── JSON round-trip ───────────────────────────────────────

    #[test]
    fn summary_types_serialize_correctly() {
        let summary = ExternalSourceSummary {
            name: "Lake".into(),
            kind: "athena".into(),
            connection_id: "prod".into(),
            database_count: 3,
        };
        let json = serde_json::to_string(&summary).unwrap();
        let restored: ExternalSourceSummary = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.name, "Lake");
        assert_eq!(restored.database_count, 3);

        let db_summary = DatabaseSummary { name: "db1".into(), table_count: 5 };
        let json = serde_json::to_string(&db_summary).unwrap();
        let restored: DatabaseSummary = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.table_count, 5);

        let table_summary = TableSummary { name: "t1".into(), column_count: 10 };
        let json = serde_json::to_string(&table_summary).unwrap();
        let restored: TableSummary = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.column_count, 10);
    }
}
