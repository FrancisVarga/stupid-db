//! Connection CRUD endpoints for DB, Queue, and Athena connections.
//!
//! SRP: connection credential management (18 handlers total).

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::Json;

use crate::connections::{ConnectionCredentials, ConnectionInput, ConnectionSafe};
use crate::credential_store::CredentialStore;
use crate::athena_connections::{
    AthenaConnectionCredentials, AthenaConnectionInput, AthenaConnectionSafe,
};
use crate::queue_connections::{
    QueueConnectionCredentials, QueueConnectionInput, QueueConnectionSafe,
};
use crate::state::AppState;

use super::QueryErrorResponse;

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

// ── Athena Connection CRUD ───────────────────────────────────────

/// List all Athena connections (credentials masked).
#[utoipa::path(
    get,
    path = "/athena-connections",
    tag = "Athena Connections",
    responses(
        (status = 200, description = "List of Athena connections", body = Vec<AthenaConnectionSafe>),
        (status = 500, description = "Internal error", body = QueryErrorResponse)
    )
)]
pub async fn athena_connections_list(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<AthenaConnectionSafe>>, (axum::http::StatusCode, Json<QueryErrorResponse>)> {
    let store = state.athena_connections.read().await;
    store.list().map(Json).map_err(|e| {
        (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(QueryErrorResponse {
                error: format!("Failed to list Athena connections: {}", e),
            }),
        )
    })
}

/// Add a new Athena connection.
///
/// After persisting the connection, spawns a background task to fetch the
/// Athena schema (databases/tables/columns) so it is available for queries.
#[utoipa::path(
    post,
    path = "/athena-connections",
    tag = "Athena Connections",
    request_body = AthenaConnectionInput,
    responses(
        (status = 201, description = "Athena connection created", body = AthenaConnectionSafe),
        (status = 409, description = "Athena connection already exists", body = QueryErrorResponse),
        (status = 500, description = "Internal error", body = QueryErrorResponse)
    )
)]
pub async fn athena_connections_add(
    State(state): State<Arc<AppState>>,
    Json(input): Json<AthenaConnectionInput>,
) -> Result<(axum::http::StatusCode, Json<AthenaConnectionSafe>), (axum::http::StatusCode, Json<QueryErrorResponse>)> {
    let safe = {
        let store = state.athena_connections.read().await;
        store.add(&input).map_err(|e| {
            let status = if e.to_string().contains("already exists") {
                axum::http::StatusCode::CONFLICT
            } else {
                axum::http::StatusCode::INTERNAL_SERVER_ERROR
            };
            (status, Json(QueryErrorResponse { error: e.to_string() }))
        })?
    };

    // Spawn background schema fetch for the newly created connection.
    let id = safe.id.clone();
    let state_clone = state.clone();
    tokio::spawn(async move {
        // Retrieve credentials and config for schema fetch.
        let (creds, conn) = {
            let store = state_clone.athena_connections.read().await;
            let creds = match store.get_credentials(&id) {
                Ok(Some(c)) => c,
                _ => return,
            };
            let conn = match store.get(&id) {
                Ok(Some(c)) => c,
                _ => return,
            };
            (creds, conn)
        };

        {
            let store = state_clone.athena_connections.read().await;
            let _ = store.update_schema_status(&id, "fetching");
        }

        match crate::athena_query::fetch_schema(&creds, &conn, Some(&state_clone.athena_query_log)).await {
            Ok(schema) => {
                let store = state_clone.athena_connections.read().await;
                let _ = store.update_schema(&id, schema);
                tracing::info!("Schema fetch complete for new Athena connection '{}'", id);
            }
            Err(e) => {
                let store = state_clone.athena_connections.read().await;
                let _ = store.update_schema_status(&id, &format!("failed: {}", e));
                tracing::warn!("Schema fetch failed for new Athena connection '{}': {}", id, e);
            }
        }
    });

    Ok((axum::http::StatusCode::CREATED, Json(safe)))
}

/// Get a single Athena connection (credentials masked).
#[utoipa::path(
    get,
    path = "/athena-connections/{id}",
    tag = "Athena Connections",
    params(("id" = String, Path, description = "Athena connection ID")),
    responses(
        (status = 200, description = "Athena connection details", body = AthenaConnectionSafe),
        (status = 404, description = "Not found"),
        (status = 500, description = "Internal error")
    )
)]
pub async fn athena_connections_get(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<AthenaConnectionSafe>, axum::http::StatusCode> {
    let store = state.athena_connections.read().await;
    match store.get_safe(&id) {
        Ok(Some(c)) => Ok(Json(c)),
        Ok(None) => Err(axum::http::StatusCode::NOT_FOUND),
        Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR),
    }
}

/// Update an existing Athena connection.
#[utoipa::path(
    put,
    path = "/athena-connections/{id}",
    tag = "Athena Connections",
    params(("id" = String, Path, description = "Athena connection ID")),
    request_body = AthenaConnectionInput,
    responses(
        (status = 200, description = "Athena connection updated", body = AthenaConnectionSafe),
        (status = 404, description = "Not found"),
        (status = 500, description = "Internal error")
    )
)]
pub async fn athena_connections_update(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(input): Json<AthenaConnectionInput>,
) -> Result<Json<AthenaConnectionSafe>, axum::http::StatusCode> {
    let store = state.athena_connections.read().await;
    match store.update(&id, &input) {
        Ok(Some(c)) => Ok(Json(c)),
        Ok(None) => Err(axum::http::StatusCode::NOT_FOUND),
        Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR),
    }
}

/// Delete an Athena connection.
#[utoipa::path(
    delete,
    path = "/athena-connections/{id}",
    tag = "Athena Connections",
    params(("id" = String, Path, description = "Athena connection ID")),
    responses(
        (status = 204, description = "Athena connection deleted"),
        (status = 404, description = "Not found"),
        (status = 500, description = "Internal error")
    )
)]
pub async fn athena_connections_delete(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> axum::http::StatusCode {
    let store = state.athena_connections.read().await;
    match store.delete(&id) {
        Ok(true) => {
            // Clean up query log entries for the deleted connection.
            state.athena_query_log.clear(&id);
            axum::http::StatusCode::NO_CONTENT
        }
        Ok(false) => axum::http::StatusCode::NOT_FOUND,
        Err(_) => axum::http::StatusCode::INTERNAL_SERVER_ERROR,
    }
}

/// Get decrypted credentials for an Athena connection.
#[utoipa::path(
    get,
    path = "/athena-connections/{id}/credentials",
    tag = "Athena Connections",
    params(("id" = String, Path, description = "Athena connection ID")),
    responses(
        (status = 200, description = "Decrypted Athena credentials", body = AthenaConnectionCredentials),
        (status = 404, description = "Not found"),
        (status = 500, description = "Internal error")
    )
)]
pub async fn athena_connections_credentials(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<AthenaConnectionCredentials>, axum::http::StatusCode> {
    let store = state.athena_connections.read().await;
    match store.get_credentials(&id) {
        Ok(Some(c)) => Ok(Json(c)),
        Ok(None) => Err(axum::http::StatusCode::NOT_FOUND),
        Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR),
    }
}
