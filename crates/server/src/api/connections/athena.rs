//! Athena connection CRUD handlers.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::Json;

use crate::athena_connections::{
    AthenaConnectionCredentials, AthenaConnectionInput, AthenaConnectionSafe,
};
use crate::credential_store::CredentialStore;
use crate::state::AppState;

use crate::api::QueryErrorResponse;

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
