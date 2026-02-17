//! Computed analytics endpoints: PageRank, communities, degrees,
//! patterns, co-occurrence, trends, and anomalies.
//!
//! SRP: exposing precomputed knowledge-store results via REST.

use std::sync::Arc;

use axum::extract::State;
use axum::Json;
use serde::Serialize;

use crate::state::AppState;

// ── Shared query params ──────────────────────────────────────────

#[derive(serde::Deserialize, utoipa::IntoParams)]
pub struct ComputeQueryParams {
    /// Maximum number of results to return (default 50, max 500).
    pub limit: Option<usize>,
}

// ── PageRank ─────────────────────────────────────────────────────

#[derive(Serialize, utoipa::ToSchema)]
pub struct PageRankEntry {
    pub id: String,
    pub entity_type: String,
    pub key: String,
    pub score: f64,
}

/// Top nodes by PageRank score, sorted descending.
#[utoipa::path(
    get,
    path = "/compute/pagerank",
    tag = "Compute",
    params(ComputeQueryParams),
    responses(
        (status = 200, description = "PageRank scores", body = Vec<PageRankEntry>)
    )
)]
pub async fn compute_pagerank(
    State(state): State<Arc<AppState>>,
    axum::extract::Query(params): axum::extract::Query<ComputeQueryParams>,
) -> Json<Vec<PageRankEntry>> {
    let limit = params.limit.unwrap_or(50).min(500);

    // Clone sorted scores out of knowledge (std::sync lock — must not hold across .await).
    let sorted_scores: Vec<(uuid::Uuid, f64)> = {
        let knowledge = state.knowledge.read().unwrap();
        if knowledge.pagerank.is_empty() {
            return Json(vec![]);
        }
        let mut entries: Vec<(uuid::Uuid, f64)> = knowledge.pagerank.iter().map(|(&k, &v)| (k, v)).collect();
        entries.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        entries.into_iter().take(limit).collect()
    };

    let graph = state.graph.read().await;
    let result: Vec<PageRankEntry> = sorted_scores
        .into_iter()
        .filter_map(|(node_id, score)| {
            let node = graph.nodes.get(&node_id)?;
            Some(PageRankEntry {
                id: node_id.to_string(),
                entity_type: node.entity_type.to_string(),
                key: node.key.clone(),
                score,
            })
        })
        .collect();

    Json(result)
}

// ── Communities ───────────────────────────────────────────────────

#[derive(Serialize, utoipa::ToSchema)]
pub struct CommunitySummary {
    pub community_id: u64,
    pub member_count: usize,
    pub top_nodes: Vec<CommunityNode>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct CommunityNode {
    pub id: String,
    pub entity_type: String,
    pub key: String,
}

/// Louvain community detection results sorted by member count descending.
#[utoipa::path(
    get,
    path = "/compute/communities",
    tag = "Compute",
    responses(
        (status = 200, description = "Community summaries", body = Vec<CommunitySummary>)
    )
)]
pub async fn compute_communities(
    State(state): State<Arc<AppState>>,
) -> Json<Vec<CommunitySummary>> {
    // Extract community data from knowledge (std::sync lock — must not hold across .await).
    let community_members: std::collections::HashMap<u64, Vec<uuid::Uuid>> = {
        let knowledge = state.knowledge.read().unwrap();
        if knowledge.communities.is_empty() {
            return Json(vec![]);
        }
        let mut members: std::collections::HashMap<u64, Vec<uuid::Uuid>> =
            std::collections::HashMap::new();
        for (&node_id, &community_id) in &knowledge.communities {
            members.entry(community_id).or_default().push(node_id);
        }
        members
    };

    let graph = state.graph.read().await;

    let mut summaries: Vec<CommunitySummary> = community_members
        .into_iter()
        .map(|(community_id, members)| {
            let top_nodes: Vec<CommunityNode> = members
                .iter()
                .take(5)
                .filter_map(|id| {
                    let node = graph.nodes.get(id)?;
                    Some(CommunityNode {
                        id: id.to_string(),
                        entity_type: node.entity_type.to_string(),
                        key: node.key.clone(),
                    })
                })
                .collect();

            CommunitySummary {
                community_id,
                member_count: members.len(),
                top_nodes,
            }
        })
        .collect();

    summaries.sort_by(|a, b| b.member_count.cmp(&a.member_count));
    Json(summaries)
}

// ── Degrees ──────────────────────────────────────────────────────

#[derive(Serialize, utoipa::ToSchema)]
pub struct DegreeEntry {
    pub id: String,
    pub entity_type: String,
    pub key: String,
    pub in_deg: usize,
    pub out_deg: usize,
    pub total: usize,
}

/// Node degree centrality sorted by total degree descending.
#[utoipa::path(
    get,
    path = "/compute/degrees",
    tag = "Compute",
    params(ComputeQueryParams),
    responses(
        (status = 200, description = "Degree entries", body = Vec<DegreeEntry>)
    )
)]
pub async fn compute_degrees(
    State(state): State<Arc<AppState>>,
    axum::extract::Query(params): axum::extract::Query<ComputeQueryParams>,
) -> Json<Vec<DegreeEntry>> {
    let limit = params.limit.unwrap_or(50).min(500);

    // Extract degree data from knowledge (std::sync lock — must not hold across .await).
    let sorted_degrees: Vec<(uuid::Uuid, stupid_compute::DegreeInfo)> = {
        let knowledge = state.knowledge.read().unwrap();
        if knowledge.degrees.is_empty() {
            return Json(vec![]);
        }
        let mut entries: Vec<(uuid::Uuid, stupid_compute::DegreeInfo)> =
            knowledge.degrees.iter().map(|(&k, v)| (k, v.clone())).collect();
        entries.sort_by(|a, b| b.1.total.cmp(&a.1.total));
        entries.into_iter().take(limit).collect()
    };

    let graph = state.graph.read().await;

    let result: Vec<DegreeEntry> = sorted_degrees
        .into_iter()
        .filter_map(|(node_id, deg)| {
            let node = graph.nodes.get(&node_id)?;
            Some(DegreeEntry {
                id: node_id.to_string(),
                entity_type: node.entity_type.to_string(),
                key: node.key.clone(),
                in_deg: deg.in_deg,
                out_deg: deg.out_deg,
                total: deg.total,
            })
        })
        .collect();

    Json(result)
}

// ── Pattern detection ────────────────────────────────────────────

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

// ── Co-occurrence ────────────────────────────────────────────────

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

// ── Trends ───────────────────────────────────────────────────────

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

// ── Anomaly detection ────────────────────────────────────────────

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
