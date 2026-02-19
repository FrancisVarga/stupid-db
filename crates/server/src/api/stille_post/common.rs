//! Shared helpers and type aliases for Stille Post endpoints.

use axum::Json;

use crate::state::AppState;

use super::super::QueryErrorResponse;

// ── Type alias ──────────────────────────────────────────────────

pub(crate) type ApiResult<T> = Result<T, (axum::http::StatusCode, Json<QueryErrorResponse>)>;

// ── Helpers ─────────────────────────────────────────────────────

pub(crate) fn require_pg(state: &AppState) -> ApiResult<&sqlx::PgPool> {
    state.pg_pool.as_ref().ok_or_else(|| {
        (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            Json(QueryErrorResponse {
                error: "PostgreSQL not configured".into(),
            }),
        )
    })
}

pub(crate) fn internal_error(e: impl std::fmt::Display) -> (axum::http::StatusCode, Json<QueryErrorResponse>) {
    (
        axum::http::StatusCode::INTERNAL_SERVER_ERROR,
        Json(QueryErrorResponse {
            error: e.to_string(),
        }),
    )
}

pub(crate) fn not_found(resource: &str, id: sqlx::types::Uuid) -> (axum::http::StatusCode, Json<QueryErrorResponse>) {
    (
        axum::http::StatusCode::NOT_FOUND,
        Json(QueryErrorResponse {
            error: format!("{} not found: {}", resource, id),
        }),
    )
}

pub(crate) fn bad_request(msg: impl Into<String>) -> (axum::http::StatusCode, Json<QueryErrorResponse>) {
    (
        axum::http::StatusCode::BAD_REQUEST,
        Json(QueryErrorResponse { error: msg.into() }),
    )
}

pub(crate) fn default_json_object() -> serde_json::Value {
    serde_json::json!({})
}
