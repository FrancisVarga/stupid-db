//! Query execution endpoint for the catalog knowledge graph.

use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;

use crate::api::QueryErrorResponse;
use crate::state::AppState;

// ── Query execution ─────────────────────────────────────────────

/// Execute a structured query plan against the knowledge graph.
#[utoipa::path(
    post,
    path = "/catalog/query",
    tag = "Catalog",
    request_body = super::types::QueryExecuteRequest,
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
    // Route through eisenbahn if available.
    if let Some(ref eb) = state.eisenbahn {
        let steps = match body.get("steps") {
            Some(s) => serde_json::from_value::<Vec<serde_json::Value>>(s.clone())
                .unwrap_or_default(),
            None => vec![body.clone()],
        };
        let svc_req = stupid_eisenbahn::services::CatalogQueryRequest { steps };
        let resp = eb
            .catalog_query(svc_req, std::time::Duration::from_secs(30))
            .await
            .map_err(|e| eb_catalog_error(e))?;
        return Ok(Json(resp.results));
    }

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

/// Map an eisenbahn error to an HTTP error response for catalog queries.
fn eb_catalog_error(e: stupid_eisenbahn::EisenbahnError) -> (StatusCode, Json<QueryErrorResponse>) {
    let status = match &e {
        stupid_eisenbahn::EisenbahnError::Timeout(_) => StatusCode::GATEWAY_TIMEOUT,
        _ => StatusCode::BAD_GATEWAY,
    };
    (status, Json(QueryErrorResponse { error: e.to_string() }))
}
