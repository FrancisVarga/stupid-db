//! Entity co-occurrence matrix endpoint with optional PMI scores.

use std::sync::Arc;

use axum::extract::State;
use axum::Json;
use serde::Serialize;

use crate::state::AppState;

#[derive(serde::Deserialize, utoipa::IntoParams)]
pub struct CooccurrenceQueryParams {
    /// Filter by first entity type (case-insensitive).
    pub entity_type_a: Option<String>,
    /// Filter by second entity type (case-insensitive).
    pub entity_type_b: Option<String>,
    /// Maximum pairs per entity-type combination (default 50, max 500).
    pub limit: Option<usize>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct CooccurrenceEntry {
    pub entity_a: String,
    pub entity_b: String,
    pub count: f64,
    pub pmi: Option<f64>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct CooccurrenceResponse {
    pub entity_type_a: String,
    pub entity_type_b: String,
    pub pairs: Vec<CooccurrenceEntry>,
}

/// Entity co-occurrence matrices with optional PMI scores.
#[utoipa::path(
    get,
    path = "/compute/cooccurrence",
    tag = "Compute",
    params(CooccurrenceQueryParams),
    responses(
        (status = 200, description = "Co-occurrence matrices", body = Vec<CooccurrenceResponse>)
    )
)]
pub async fn compute_cooccurrence(
    State(state): State<Arc<AppState>>,
    axum::extract::Query(params): axum::extract::Query<CooccurrenceQueryParams>,
) -> Json<Vec<CooccurrenceResponse>> {
    let limit = params.limit.unwrap_or(50).min(500);

    let knowledge = state.knowledge.read().unwrap();

    let mut responses = Vec::new();

    // If PMI matrices are available, use them; otherwise fall back to raw counts.
    if !knowledge.cooccurrence_pmi.is_empty() {
        for ((type_a, type_b), matrix) in &knowledge.cooccurrence_pmi {
            let type_a_str = type_a.to_string();
            let type_b_str = type_b.to_string();

            // Apply entity type filter if specified.
            if let Some(ref filter_a) = params.entity_type_a {
                if !type_a_str.eq_ignore_ascii_case(filter_a) && !type_b_str.eq_ignore_ascii_case(filter_a) {
                    continue;
                }
            }
            if let Some(ref filter_b) = params.entity_type_b {
                if !type_a_str.eq_ignore_ascii_case(filter_b) && !type_b_str.eq_ignore_ascii_case(filter_b) {
                    continue;
                }
            }

            let mut pairs: Vec<CooccurrenceEntry> = matrix
                .counts
                .entries
                .iter()
                .map(|((a, b), &count)| {
                    let pmi = matrix.pmi.get(&(a.clone(), b.clone())).copied();
                    CooccurrenceEntry {
                        entity_a: a.clone(),
                        entity_b: b.clone(),
                        count,
                        pmi,
                    }
                })
                .collect();

            // Sort by PMI descending (or by count if no PMI).
            pairs.sort_by(|a, b| {
                let pmi_a = a.pmi.unwrap_or(0.0);
                let pmi_b = b.pmi.unwrap_or(0.0);
                pmi_b.partial_cmp(&pmi_a).unwrap_or(std::cmp::Ordering::Equal)
            });
            pairs.truncate(limit);

            responses.push(CooccurrenceResponse {
                entity_type_a: type_a_str,
                entity_type_b: type_b_str,
                pairs,
            });
        }
    } else {
        // Fall back to raw co-occurrence matrices.
        for ((type_a, type_b), matrix) in &knowledge.cooccurrence {
            let type_a_str = type_a.to_string();
            let type_b_str = type_b.to_string();

            if let Some(ref filter_a) = params.entity_type_a {
                if !type_a_str.eq_ignore_ascii_case(filter_a) && !type_b_str.eq_ignore_ascii_case(filter_a) {
                    continue;
                }
            }
            if let Some(ref filter_b) = params.entity_type_b {
                if !type_a_str.eq_ignore_ascii_case(filter_b) && !type_b_str.eq_ignore_ascii_case(filter_b) {
                    continue;
                }
            }

            let mut pairs: Vec<CooccurrenceEntry> = matrix
                .entries
                .iter()
                .map(|((a, b), &count)| CooccurrenceEntry {
                    entity_a: a.clone(),
                    entity_b: b.clone(),
                    count,
                    pmi: None,
                })
                .collect();

            pairs.sort_by(|a, b| b.count.partial_cmp(&a.count).unwrap_or(std::cmp::Ordering::Equal));
            pairs.truncate(limit);

            responses.push(CooccurrenceResponse {
                entity_type_a: type_a_str,
                entity_type_b: type_b_str,
                pairs,
            });
        }
    }

    Json(responses)
}
