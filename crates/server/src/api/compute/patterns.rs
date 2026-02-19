//! Sequential pattern detection endpoint (PrefixSpan).

use std::sync::Arc;

use axum::extract::State;
use axum::Json;
use serde::Serialize;

use crate::state::AppState;
use super::ComputeQueryParams;

#[derive(Serialize, utoipa::ToSchema)]
pub struct PatternResponse {
    pub id: String,
    pub sequence: Vec<String>,
    pub support: f64,
    pub member_count: usize,
    pub avg_duration_secs: f64,
    pub category: String,
    pub description: Option<String>,
}

/// Detected sequential patterns (PrefixSpan) sorted by support.
#[utoipa::path(
    get,
    path = "/compute/patterns",
    tag = "Compute",
    params(ComputeQueryParams),
    responses(
        (status = 200, description = "Sequential patterns", body = Vec<PatternResponse>)
    )
)]
pub async fn compute_patterns(
    State(state): State<Arc<AppState>>,
    axum::extract::Query(params): axum::extract::Query<ComputeQueryParams>,
) -> Json<Vec<PatternResponse>> {
    let limit = params.limit.unwrap_or(50).min(500);

    let knowledge = state.knowledge.read().unwrap();
    let result: Vec<PatternResponse> = knowledge
        .prefixspan_patterns
        .iter()
        .take(limit)
        .map(|p| PatternResponse {
            id: p.id.clone(),
            sequence: p.sequence.iter().map(|e| e.0.clone()).collect(),
            support: p.support,
            member_count: p.member_count,
            avg_duration_secs: p.avg_duration_secs,
            category: format!("{:?}", p.category),
            description: p.description.clone(),
        })
        .collect();

    Json(result)
}
