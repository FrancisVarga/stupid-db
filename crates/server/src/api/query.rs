//! Natural-language query endpoint.
//!
//! SRP: NL query execution via LLM-generated query plans.

use std::sync::Arc;

use axum::extract::State;
use axum::Json;
use serde::Serialize;

use crate::state::AppState;

use super::QueryErrorResponse;

// ── Query endpoint ─────────────────────────────────────────────

#[derive(serde::Deserialize)]
pub struct QueryRequest {
    pub question: String,
}

#[derive(Serialize)]
pub struct QueryResponse {
    pub question: String,
    pub plan: stupid_catalog::QueryPlan,
    pub results: Vec<serde_json::Value>,
}

pub async fn query(
    State(state): State<Arc<AppState>>,
    Json(req): Json<QueryRequest>,
) -> Result<Json<QueryResponse>, (axum::http::StatusCode, Json<QueryErrorResponse>)> {
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
