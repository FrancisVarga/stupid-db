//! Metric trend detection endpoint.

use std::sync::Arc;

use axum::extract::State;
use axum::Json;
use serde::Serialize;

use crate::state::AppState;

#[derive(Serialize, utoipa::ToSchema)]
pub struct TrendResponse {
    pub metric: String,
    pub current_value: f64,
    pub baseline_mean: f64,
    pub direction: String,
    pub magnitude: f64,
}

/// Detected metric trends sorted by magnitude descending.
#[utoipa::path(
    get,
    path = "/compute/trends",
    tag = "Compute",
    responses(
        (status = 200, description = "Metric trends", body = Vec<TrendResponse>)
    )
)]
pub async fn compute_trends(
    State(state): State<Arc<AppState>>,
) -> Json<Vec<TrendResponse>> {
    let knowledge = state.knowledge.read().unwrap();

    let mut trends: Vec<TrendResponse> = knowledge
        .trends
        .values()
        .map(|t| TrendResponse {
            metric: t.metric_name.clone(),
            current_value: t.current,
            baseline_mean: t.baseline,
            direction: format!("{:?}", t.direction),
            magnitude: t.magnitude,
        })
        .collect();

    // Sort by magnitude descending.
    trends.sort_by(|a, b| b.magnitude.partial_cmp(&a.magnitude).unwrap_or(std::cmp::Ordering::Equal));

    Json(trends)
}
