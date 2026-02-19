//! Session CRUD endpoints: list, create, get, update, delete.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::Json;

use crate::state::AppState;

use super::super::QueryErrorResponse;
use super::types::{SessionCreateRequest, SessionUpdateRequest};

/// List all sessions
///
/// Returns summaries of all sessions, sorted by updated_at descending.
#[utoipa::path(
    get,
    path = "/sessions",
    tag = "Sessions",
    responses(
        (status = 200, description = "List of session summaries", body = Object),
        (status = 500, description = "Internal error", body = QueryErrorResponse)
    )
)]
pub async fn sessions_list(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<stupid_agent::session::SessionSummary>>, (axum::http::StatusCode, Json<QueryErrorResponse>)> {
    let store = state.session_store.read().await;
    store.list().map(Json).map_err(|e| {
        (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(QueryErrorResponse {
                error: format!("Failed to list sessions: {}", e),
            }),
        )
    })
}

/// Create a new session
///
/// Creates an empty session with an optional name.
#[utoipa::path(
    post,
    path = "/sessions",
    tag = "Sessions",
    request_body = SessionCreateRequest,
    responses(
        (status = 201, description = "Session created", body = Object),
        (status = 500, description = "Internal error", body = QueryErrorResponse)
    )
)]
pub async fn sessions_create(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SessionCreateRequest>,
) -> Result<(axum::http::StatusCode, Json<stupid_agent::session::Session>), (axum::http::StatusCode, Json<QueryErrorResponse>)> {
    let store = state.session_store.write().await;
    store
        .create(req.name.as_deref())
        .map(|s| (axum::http::StatusCode::CREATED, Json(s)))
        .map_err(|e| {
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(QueryErrorResponse {
                    error: format!("Failed to create session: {}", e),
                }),
            )
        })
}

/// Get a session by ID
///
/// Returns the full session including all messages.
#[utoipa::path(
    get,
    path = "/sessions/{id}",
    tag = "Sessions",
    params(
        ("id" = String, Path, description = "Session ID")
    ),
    responses(
        (status = 200, description = "Full session with messages", body = Object),
        (status = 404, description = "Session not found", body = QueryErrorResponse),
        (status = 500, description = "Internal error", body = QueryErrorResponse)
    )
)]
pub async fn sessions_get(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<stupid_agent::session::Session>, (axum::http::StatusCode, Json<QueryErrorResponse>)> {
    let store = state.session_store.read().await;
    match store.get(&id) {
        Ok(Some(session)) => Ok(Json(session)),
        Ok(None) => Err((
            axum::http::StatusCode::NOT_FOUND,
            Json(QueryErrorResponse {
                error: format!("Session not found: {}", id),
            }),
        )),
        Err(e) => Err((
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(QueryErrorResponse {
                error: format!("Failed to get session: {}", e),
            }),
        )),
    }
}

/// Rename a session
///
/// Updates the session name.
#[utoipa::path(
    put,
    path = "/sessions/{id}",
    tag = "Sessions",
    params(
        ("id" = String, Path, description = "Session ID")
    ),
    request_body = SessionUpdateRequest,
    responses(
        (status = 200, description = "Session updated", body = Object),
        (status = 404, description = "Session not found", body = QueryErrorResponse),
        (status = 500, description = "Internal error", body = QueryErrorResponse)
    )
)]
pub async fn sessions_update(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<SessionUpdateRequest>,
) -> Result<Json<stupid_agent::session::Session>, (axum::http::StatusCode, Json<QueryErrorResponse>)> {
    let store = state.session_store.write().await;
    match store.rename(&id, &req.name) {
        Ok(Some(session)) => Ok(Json(session)),
        Ok(None) => Err((
            axum::http::StatusCode::NOT_FOUND,
            Json(QueryErrorResponse {
                error: format!("Session not found: {}", id),
            }),
        )),
        Err(e) => Err((
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(QueryErrorResponse {
                error: format!("Failed to rename session: {}", e),
            }),
        )),
    }
}

/// Delete a session
///
/// Permanently removes a session and all its messages.
#[utoipa::path(
    delete,
    path = "/sessions/{id}",
    tag = "Sessions",
    params(
        ("id" = String, Path, description = "Session ID")
    ),
    responses(
        (status = 204, description = "Session deleted"),
        (status = 404, description = "Session not found", body = QueryErrorResponse),
        (status = 500, description = "Internal error", body = QueryErrorResponse)
    )
)]
pub async fn sessions_delete(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<axum::http::StatusCode, (axum::http::StatusCode, Json<QueryErrorResponse>)> {
    let store = state.session_store.write().await;
    match store.delete(&id) {
        Ok(true) => Ok(axum::http::StatusCode::NO_CONTENT),
        Ok(false) => Err((
            axum::http::StatusCode::NOT_FOUND,
            Json(QueryErrorResponse {
                error: format!("Session not found: {}", id),
            }),
        )),
        Err(e) => Err((
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(QueryErrorResponse {
                error: format!("Failed to delete session: {}", e),
            }),
        )),
    }
}
