//! Shared types for the catalog API: response structs, query parameters,
//! path parameters, and error helpers.

use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::api::QueryErrorResponse;

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

pub(crate) fn store_err(e: impl std::fmt::Display) -> (StatusCode, Json<QueryErrorResponse>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(QueryErrorResponse {
            error: format!("Catalog store error: {e}"),
        }),
    )
}
