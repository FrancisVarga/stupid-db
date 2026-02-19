//! External SQL source endpoints: CRUD + drill-down (databases, tables, columns).

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use tracing::info;

use crate::api::QueryErrorResponse;
use crate::state::AppState;
use super::types::{
    store_err, ColumnFilterQuery, DatabasePathParams, DatabaseSummary, DepthQuery,
    ExternalSourceSummary, SearchQuery, TablePathParams, TableSummary,
};

// ── Helpers ─────────────────────────────────────────────────────

/// Helper: load an external source or return 404.
pub(super) fn load_source(
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
pub(super) fn find_database<'a>(
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
pub(super) fn find_table<'a>(
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

// ── External source CRUD ────────────────────────────────────────

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
