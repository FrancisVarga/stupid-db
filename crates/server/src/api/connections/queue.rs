//! Queue connection CRUD handlers.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::Json;

use crate::credential_store::CredentialStore;
use crate::queue_connections::{
    QueueConnectionCredentials, QueueConnectionInput, QueueConnectionSafe,
};
use crate::state::AppState;

use crate::api::QueryErrorResponse;

// ── Queue Connection CRUD ────────────────────────────────────────

/// List all queue connections (credentials masked).
#[utoipa::path(
    get,
    path = "/queue-connections",
    tag = "Queue Connections",
    responses(
        (status = 200, description = "List of queue connections", body = Vec<QueueConnectionSafe>),
        (status = 500, description = "Internal error", body = QueryErrorResponse)
    )
)]
pub async fn queue_connections_list(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<QueueConnectionSafe>>, (axum::http::StatusCode, Json<QueryErrorResponse>)> {
    let store = state.queue_connections.read().await;
    store.list().map(Json).map_err(|e| {
        (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(QueryErrorResponse {
                error: format!("Failed to list queue connections: {}", e),
            }),
        )
    })
}

/// Add a new queue connection.
#[utoipa::path(
    post,
    path = "/queue-connections",
    tag = "Queue Connections",
    request_body = QueueConnectionInput,
    responses(
        (status = 201, description = "Queue connection created", body = QueueConnectionSafe),
        (status = 409, description = "Queue connection already exists", body = QueryErrorResponse),
        (status = 500, description = "Internal error", body = QueryErrorResponse)
    )
)]
pub async fn queue_connections_add(
    State(state): State<Arc<AppState>>,
    Json(input): Json<QueueConnectionInput>,
) -> Result<(axum::http::StatusCode, Json<QueueConnectionSafe>), (axum::http::StatusCode, Json<QueryErrorResponse>)> {
    let store = state.queue_connections.read().await;
    store.add(&input).map(|c| (axum::http::StatusCode::CREATED, Json(c))).map_err(|e| {
        let status = if e.to_string().contains("already exists") {
            axum::http::StatusCode::CONFLICT
        } else {
            axum::http::StatusCode::INTERNAL_SERVER_ERROR
        };
        (status, Json(QueryErrorResponse { error: e.to_string() }))
    })
}

/// Get a single queue connection (credentials masked).
#[utoipa::path(
    get,
    path = "/queue-connections/{id}",
    tag = "Queue Connections",
    params(("id" = String, Path, description = "Queue connection ID")),
    responses(
        (status = 200, description = "Queue connection details", body = QueueConnectionSafe),
        (status = 404, description = "Not found"),
        (status = 500, description = "Internal error")
    )
)]
pub async fn queue_connections_get(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<QueueConnectionSafe>, axum::http::StatusCode> {
    let store = state.queue_connections.read().await;
    match store.get_safe(&id) {
        Ok(Some(c)) => Ok(Json(c)),
        Ok(None) => Err(axum::http::StatusCode::NOT_FOUND),
        Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR),
    }
}

/// Update an existing queue connection.
#[utoipa::path(
    put,
    path = "/queue-connections/{id}",
    tag = "Queue Connections",
    params(("id" = String, Path, description = "Queue connection ID")),
    request_body = QueueConnectionInput,
    responses(
        (status = 200, description = "Queue connection updated", body = QueueConnectionSafe),
        (status = 404, description = "Not found"),
        (status = 500, description = "Internal error")
    )
)]
pub async fn queue_connections_update(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(input): Json<QueueConnectionInput>,
) -> Result<Json<QueueConnectionSafe>, axum::http::StatusCode> {
    let store = state.queue_connections.read().await;
    match store.update(&id, &input) {
        Ok(Some(c)) => Ok(Json(c)),
        Ok(None) => Err(axum::http::StatusCode::NOT_FOUND),
        Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR),
    }
}

/// Delete a queue connection.
#[utoipa::path(
    delete,
    path = "/queue-connections/{id}",
    tag = "Queue Connections",
    params(("id" = String, Path, description = "Queue connection ID")),
    responses(
        (status = 204, description = "Queue connection deleted"),
        (status = 404, description = "Not found"),
        (status = 500, description = "Internal error")
    )
)]
pub async fn queue_connections_delete(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> axum::http::StatusCode {
    let store = state.queue_connections.read().await;
    match store.delete(&id) {
        Ok(true) => axum::http::StatusCode::NO_CONTENT,
        Ok(false) => axum::http::StatusCode::NOT_FOUND,
        Err(_) => axum::http::StatusCode::INTERNAL_SERVER_ERROR,
    }
}

/// Get decrypted credentials for a queue connection (used by SQS consumer).
#[utoipa::path(
    get,
    path = "/queue-connections/{id}/credentials",
    tag = "Queue Connections",
    params(("id" = String, Path, description = "Queue connection ID")),
    responses(
        (status = 200, description = "Decrypted queue credentials", body = QueueConnectionCredentials),
        (status = 404, description = "Not found"),
        (status = 500, description = "Internal error")
    )
)]
pub async fn queue_connections_credentials(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<QueueConnectionCredentials>, axum::http::StatusCode> {
    let store = state.queue_connections.read().await;
    match store.get_credentials(&id) {
        Ok(Some(c)) => Ok(Json(c)),
        Ok(None) => Err(axum::http::StatusCode::NOT_FOUND),
        Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR),
    }
}
