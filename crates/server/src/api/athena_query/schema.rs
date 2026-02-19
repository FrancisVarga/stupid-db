//! Schema introspection and refresh endpoints for Athena connections.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::Json;

use crate::credential_store::CredentialStore;
use crate::state::AppState;

use super::helpers::eb_athena_error;
use crate::api::QueryErrorResponse;

/// Get cached schema for an Athena connection
///
/// Returns the cached database/table/column schema and its fetch status.
#[utoipa::path(
    get,
    path = "/athena-connections/{id}/schema",
    tag = "Athena Queries",
    params(("id" = String, Path, description = "Athena connection ID")),
    responses(
        (status = 200, description = "Schema and status", body = Object),
        (status = 404, description = "Connection not found")
    )
)]
pub async fn athena_connections_schema(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, axum::http::StatusCode> {
    let store = state.athena_connections.read().await;
    match store.get(&id) {
        Ok(Some(conn)) => {
            Ok(Json(serde_json::json!({
                "schema_status": conn.schema_status,
                "schema": conn.schema,
            })))
        }
        Ok(None) => Err(axum::http::StatusCode::NOT_FOUND),
        Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR),
    }
}

/// Trigger background schema refresh for an Athena connection
///
/// Sets the schema status to "fetching" and spawns a background task to
/// introspect Athena catalogs, databases, tables, and columns.
#[utoipa::path(
    post,
    path = "/athena-connections/{id}/schema/refresh",
    tag = "Athena Queries",
    params(("id" = String, Path, description = "Athena connection ID")),
    responses(
        (status = 200, description = "Refresh started", body = Object),
        (status = 404, description = "Connection not found", body = QueryErrorResponse)
    )
)]
pub async fn athena_connections_schema_refresh(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, Json<QueryErrorResponse>)> {
    // Route through eisenbahn if available (fire-and-forget style).
    if let Some(ref eb) = state.eisenbahn {
        let svc_req = stupid_eisenbahn::services::AthenaServiceRequest::SchemaRefresh {
            connection_id: id.clone(),
        };
        // Use a short timeout — the refresh happens async on the worker side.
        let _resp = eb
            .athena_query_stream(svc_req)
            .await
            .map_err(|e| eb_athena_error(e))?;
        // Don't wait for the full stream — the worker does the work async.
        return Ok(Json(serde_json::json!({ "status": "fetching", "message": "Schema refresh started via eisenbahn" })));
    }

    // Get credentials and connection config.
    let (creds, conn) = {
        let store = state.athena_connections.read().await;
        let creds = match store.get_credentials(&id) {
            Ok(Some(c)) => c,
            Ok(None) => return Err((axum::http::StatusCode::NOT_FOUND, Json(QueryErrorResponse { error: "Not found".into() }))),
            Err(e) => return Err((axum::http::StatusCode::INTERNAL_SERVER_ERROR, Json(QueryErrorResponse { error: e.to_string() }))),
        };
        let conn = match store.get(&id) {
            Ok(Some(c)) => c,
            _ => return Err((axum::http::StatusCode::NOT_FOUND, Json(QueryErrorResponse { error: "Not found".into() }))),
        };
        (creds, conn)
    };

    // Update status to "fetching".
    {
        let store = state.athena_connections.read().await;
        let _ = store.update_schema_status(&id, "fetching");
    }

    // Spawn background schema fetch.
    let state_clone = state.clone();
    let id_clone = id.clone();
    tokio::spawn(async move {
        match crate::athena_query::fetch_schema(&creds, &conn, Some(&state_clone.athena_query_log)).await {
            Ok(schema) => {
                let store = state_clone.athena_connections.read().await;
                let _ = store.update_schema(&id_clone, schema);
                tracing::info!("Schema refresh complete for Athena connection '{}'", id_clone);
                drop(store);

                // Rebuild catalog external sources from all Athena connections.
                rebuild_catalog_external_sources(&state_clone).await;
            }
            Err(e) => {
                let store = state_clone.athena_connections.read().await;
                let _ = store.update_schema_status(&id_clone, &format!("failed: {}", e));
                tracing::warn!("Schema refresh failed for '{}': {}", id_clone, e);
            }
        }
    });

    Ok(Json(serde_json::json!({ "status": "fetching", "message": "Schema refresh started" })))
}

/// Rebuild the catalog's external SQL sources from all enabled Athena connections.
///
/// Reads all Athena connections with cached schemas, converts them to
/// `ExternalSource` entries, merges into the in-memory catalog, and
/// persists `current.json` to the catalog store.
async fn rebuild_catalog_external_sources(state: &Arc<AppState>) {
    let athena_store = state.athena_connections.read().await;
    let conns = match athena_store.list() {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("Failed to list Athena connections for catalog update: {}", e);
            return;
        }
    };

    let sources: Vec<stupid_catalog::ExternalSource> = conns
        .iter()
        .filter(|c| c.enabled && c.schema.is_some())
        .map(|c| {
            let schema = c.schema.as_ref().unwrap();
            stupid_catalog::ExternalSource {
                name: c.name.clone(),
                kind: "athena".to_string(),
                connection_id: c.id.clone(),
                databases: schema
                    .databases
                    .iter()
                    .map(|db| stupid_catalog::ExternalDatabase {
                        name: db.name.clone(),
                        tables: db
                            .tables
                            .iter()
                            .map(|t| stupid_catalog::ExternalTable {
                                name: t.name.clone(),
                                columns: t
                                    .columns
                                    .iter()
                                    .map(|col| stupid_catalog::ExternalColumn {
                                        name: col.name.clone(),
                                        data_type: col.data_type.clone(),
                                    })
                                    .collect(),
                            })
                            .collect(),
                    })
                    .collect(),
            }
        })
        .collect();
    drop(athena_store);

    // Persist each external source to catalog/external/{kind}-{id}.json
    for source in &sources {
        if let Err(e) = state.catalog_store.save_external_source(source) {
            tracing::warn!("Failed to persist external source '{}': {}", source.connection_id, e);
        }
    }

    // Update the in-memory catalog with refreshed external sources.
    let mut catalog_lock = state.catalog.write().await;
    if let Some(ref mut cat) = *catalog_lock {
        cat.external_sources = sources;
        tracing::info!(
            "Catalog updated with {} external source(s) and persisted to catalog/external/",
            cat.external_sources.len()
        );
    } else {
        tracing::debug!("Catalog not yet built — skipping external source update");
    }
}
