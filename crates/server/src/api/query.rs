//! Natural-language query endpoint.
//!
//! SRP: NL query execution via LLM-generated query plans.

use std::sync::Arc;
use std::time::Duration;

use axum::extract::State;
use axum::Json;
use serde::Serialize;

use crate::state::AppState;

use super::QueryErrorResponse;

// ── Query endpoint ─────────────────────────────────────────────

#[derive(serde::Deserialize, utoipa::ToSchema)]
pub struct QueryRequest {
    pub question: String,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct QueryResponse {
    pub question: String,
    #[schema(value_type = Object)]
    pub plan: stupid_catalog::QueryPlan,
    #[schema(value_type = Vec<Object>)]
    pub results: Vec<serde_json::Value>,
}

/// Execute a natural-language query
///
/// Sends the question to the LLM query generator which produces a query plan,
/// executes it against the in-memory graph and catalog, and returns results.
#[utoipa::path(
    post,
    path = "/query",
    tag = "Query",
    request_body = QueryRequest,
    responses(
        (status = 200, description = "Query results with execution plan", body = QueryResponse),
        (status = 503, description = "Service not ready", body = QueryErrorResponse)
    )
)]
pub async fn query(
    State(state): State<Arc<AppState>>,
    Json(req): Json<QueryRequest>,
) -> Result<Json<QueryResponse>, (axum::http::StatusCode, Json<QueryErrorResponse>)> {
    // Route through eisenbahn if available.
    if let Some(ref eb) = state.eisenbahn {
        let svc_req = stupid_eisenbahn::services::QueryServiceRequest {
            question: req.question.clone(),
        };
        let resp = eb
            .query(svc_req, Duration::from_secs(30))
            .await
            .map_err(|e| eb_error(e))?;
        let plan = serde_json::from_value(resp.plan).map_err(|e| {
            (
                axum::http::StatusCode::BAD_GATEWAY,
                Json(QueryErrorResponse {
                    error: format!("Failed to decode query plan from service: {e}"),
                }),
            )
        })?;
        return Ok(Json(QueryResponse {
            question: req.question,
            plan,
            results: resp.results,
        }));
    }

    // Require data to be loaded before accepting queries.
    if !state.loading.is_ready().await {
        return Err((
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            Json(QueryErrorResponse {
                error: "Data is still loading. Check /loading for progress.".into(),
            }),
        ));
    }

    let qg = state.query_generator.as_ref().ok_or_else(|| {
        (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            Json(QueryErrorResponse {
                error: "LLM query generator not configured. Set LLM_PROVIDER and API keys.".into(),
            }),
        )
    })?;

    let catalog_lock = state.catalog.read().await;
    let cat = catalog_lock.as_ref().ok_or_else(|| {
        (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            Json(QueryErrorResponse {
                error: "Catalog not yet built.".into(),
            }),
        )
    })?;

    let graph = state.graph.read().await;

    let result = qg
        .ask(&req.question, cat, &graph)
        .await
        .map_err(|e| {
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(QueryErrorResponse {
                    error: e.to_string(),
                }),
            )
        })?;

    Ok(Json(QueryResponse {
        question: result.question,
        plan: result.plan,
        results: result.results,
    }))
}

/// Map an eisenbahn error to an HTTP error response.
fn eb_error(e: stupid_eisenbahn::EisenbahnError) -> (axum::http::StatusCode, Json<QueryErrorResponse>) {
    let status = match &e {
        stupid_eisenbahn::EisenbahnError::Timeout(_) => axum::http::StatusCode::GATEWAY_TIMEOUT,
        _ => axum::http::StatusCode::BAD_GATEWAY,
    };
    (status, Json(QueryErrorResponse { error: e.to_string() }))
}
