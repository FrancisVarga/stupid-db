//! DB connection CRUD handlers.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::Json;

use crate::connections::{ConnectionCredentials, ConnectionInput, ConnectionSafe};
use crate::credential_store::CredentialStore;
use crate::state::AppState;

use crate::api::QueryErrorResponse;

// ── DB Connection CRUD ───────────────────────────────────────────

/// List all connections (passwords masked).
#[utoipa::path(
    get,
    path = "/connections",
    tag = "DB Connections",
    responses(
        (status = 200, description = "List of connections", body = Vec<ConnectionSafe>),
        (status = 500, description = "Internal error", body = QueryErrorResponse)
    )
)]
pub async fn connections_list(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<ConnectionSafe>>, (axum::http::StatusCode, Json<QueryErrorResponse>)> {
    let store = state.connections.read().await;
    store.list().map(Json).map_err(|e| {
        (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(QueryErrorResponse {
                error: format!("Failed to list connections: {}", e),
            }),
        )
    })
}

/// Add a new connection.
#[utoipa::path(
    post,
    path = "/connections",
    tag = "DB Connections",
    request_body = ConnectionInput,
    responses(
        (status = 201, description = "Connection created", body = ConnectionSafe),
        (status = 409, description = "Connection already exists", body = QueryErrorResponse),
        (status = 500, description = "Internal error", body = QueryErrorResponse)
    )
)]
pub async fn connections_add(
    State(state): State<Arc<AppState>>,
    Json(input): Json<ConnectionInput>,
) -> Result<(axum::http::StatusCode, Json<ConnectionSafe>), (axum::http::StatusCode, Json<QueryErrorResponse>)> {
    let store = state.connections.read().await;
    store.add(&input).map(|c| (axum::http::StatusCode::CREATED, Json(c))).map_err(|e| {
        let status = if e.to_string().contains("already exists") {
            axum::http::StatusCode::CONFLICT
        } else {
            axum::http::StatusCode::INTERNAL_SERVER_ERROR
        };
        (status, Json(QueryErrorResponse { error: e.to_string() }))
    })
}

/// Get a single connection (password masked).
#[utoipa::path(
    get,
    path = "/connections/{id}",
    tag = "DB Connections",
    params(("id" = String, Path, description = "Connection ID")),
    responses(
        (status = 200, description = "Connection details", body = ConnectionSafe),
        (status = 404, description = "Not found"),
        (status = 500, description = "Internal error")
    )
)]
pub async fn connections_get(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ConnectionSafe>, axum::http::StatusCode> {
    let store = state.connections.read().await;
    match store.get_safe(&id) {
        Ok(Some(c)) => Ok(Json(c)),
        Ok(None) => Err(axum::http::StatusCode::NOT_FOUND),
        Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR),
    }
}

/// Update an existing connection.
#[utoipa::path(
    put,
    path = "/connections/{id}",
    tag = "DB Connections",
    params(("id" = String, Path, description = "Connection ID")),
    request_body = ConnectionInput,
    responses(
        (status = 200, description = "Connection updated", body = ConnectionSafe),
        (status = 404, description = "Not found"),
        (status = 500, description = "Internal error")
    )
)]
pub async fn connections_update(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(input): Json<ConnectionInput>,
) -> Result<Json<ConnectionSafe>, axum::http::StatusCode> {
    let store = state.connections.read().await;
    match store.update(&id, &input) {
        Ok(Some(c)) => Ok(Json(c)),
        Ok(None) => Err(axum::http::StatusCode::NOT_FOUND),
        Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR),
    }
}

/// Delete a connection.
#[utoipa::path(
    delete,
    path = "/connections/{id}",
    tag = "DB Connections",
    params(("id" = String, Path, description = "Connection ID")),
    responses(
        (status = 204, description = "Connection deleted"),
        (status = 404, description = "Not found"),
        (status = 500, description = "Internal error")
    )
)]
pub async fn connections_delete(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> axum::http::StatusCode {
    let store = state.connections.read().await;
    match store.delete(&id) {
        Ok(true) => axum::http::StatusCode::NO_CONTENT,
        Ok(false) => axum::http::StatusCode::NOT_FOUND,
        Err(_) => axum::http::StatusCode::INTERNAL_SERVER_ERROR,
    }
}

/// Get decrypted credentials for a connection (used by dashboard pool manager).
#[utoipa::path(
    get,
    path = "/connections/{id}/credentials",
    tag = "DB Connections",
    params(("id" = String, Path, description = "Connection ID")),
    responses(
        (status = 200, description = "Decrypted credentials", body = ConnectionCredentials),
        (status = 404, description = "Not found"),
        (status = 500, description = "Internal error")
    )
)]
pub async fn connections_credentials(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ConnectionCredentials>, axum::http::StatusCode> {
    let store = state.connections.read().await;
    match store.get_credentials(&id) {
        Ok(Some(c)) => Ok(Json(c)),
        Ok(None) => Err(axum::http::StatusCode::NOT_FOUND),
        Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR),
    }
}
