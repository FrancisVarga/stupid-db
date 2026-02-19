//! Anomaly detection endpoint (DBSCAN clustering).

use std::sync::Arc;

use axum::extract::State;
use axum::Json;
use serde::Serialize;

use crate::state::AppState;
use super::ComputeQueryParams;

#[derive(Serialize, utoipa::ToSchema)]
pub struct FeatureDimension {
    pub name: &'static str,
    pub value: f64,
}

/// Feature dimension labels in the same order as `MemberFeatures::to_feature_vector`.
const FEATURE_NAMES: [&str; 10] = [
    "login_count",
    "game_count",
    "unique_games",
    "error_count",
    "popup_interactions",
    "mobile_ratio",
    "session_count",
    "avg_session_gap_hrs",
    "vip_group",
    "currency",
];

#[derive(Serialize, utoipa::ToSchema)]
pub struct AnomalyEntry {
    pub id: String,
    pub entity_type: String,
    pub key: String,
    pub score: f64,
    pub is_anomalous: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub features: Option<Vec<FeatureDimension>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cluster_id: Option<u64>,
}

/// Anomaly scores from DBSCAN clustering, sorted by score descending.
#[utoipa::path(
    get,
    path = "/compute/anomalies",
    tag = "Compute",
    params(ComputeQueryParams),
    responses(
        (status = 200, description = "Anomaly entries with optional features", body = Vec<AnomalyEntry>)
    )
)]
pub async fn compute_anomalies(
    State(state): State<Arc<AppState>>,
    axum::extract::Query(params): axum::extract::Query<ComputeQueryParams>,
) -> Json<Vec<AnomalyEntry>> {
    let limit = params.limit.unwrap_or(50).min(500);

    // Extract anomaly data + cluster assignments from knowledge
    // (std::sync lock — must not hold across .await).
    let sorted_anomalies: Vec<(uuid::Uuid, f64, bool, Option<u64>)> = {
        let knowledge = state.knowledge.read().unwrap();
        if knowledge.anomalies.is_empty() {
            return Json(vec![]);
        }
        let mut entries: Vec<(uuid::Uuid, f64, bool, Option<u64>)> = knowledge
            .anomalies
            .iter()
            .map(|(&id, score)| {
                let cluster_id = knowledge.clusters.get(&id).copied();
                (id, score.score, score.is_anomalous, cluster_id)
            })
            .collect();
        entries.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        entries.into_iter().take(limit).collect()
    };

    // Resolve member codes and feature vectors from the pipeline's reverse mapping.
    // Pipeline NodeIds use FNV-hash UUIDs which differ from graph's random UUIDs,
    // so we look up member_key directly from pipeline features.
    let pipeline = state.pipeline.lock().unwrap();
    let result: Vec<AnomalyEntry> = sorted_anomalies
        .into_iter()
        .filter_map(|(node_id, score, is_anomalous, cluster_id)| {
            let key = pipeline.features.member_key(&node_id)?;
            let features = pipeline.features.to_feature_vector(&node_id).map(|vec| {
                vec.into_iter()
                    .enumerate()
                    .map(|(i, value)| FeatureDimension {
                        name: FEATURE_NAMES[i],
                        value,
                    })
                    .collect()
            });
            Some(AnomalyEntry {
                id: node_id.to_string(),
                entity_type: "Member".to_string(),
                key: key.to_owned(),
                score,
                is_anomalous,
                features,
                cluster_id,
            })
        })
        .collect();

    Json(result)
}
