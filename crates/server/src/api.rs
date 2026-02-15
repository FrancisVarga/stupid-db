use std::sync::atomic::Ordering;
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::Json;
use serde::Serialize;

use axum::response::sse::{Event, Sse};
use futures::stream;
use serde::Deserialize;
use std::convert::Infallible;
use tokio_stream::wrappers::ReceiverStream;

use crate::connections::{ConnectionCredentials, ConnectionInput, ConnectionSafe};
use crate::athena_connections::{
    AthenaConnectionCredentials, AthenaConnectionInput, AthenaConnectionSafe,
};
use crate::queue_connections::{
    QueueConnectionCredentials, QueueConnectionInput, QueueConnectionSafe,
};
use crate::state::{AppState, LoadingStatus};

// ── Loading guard ─────────────────────────────────────────────────

#[derive(Serialize)]
pub struct NotReadyResponse {
    pub error: &'static str,
    pub loading: LoadingStatus,
}

/// Return 503 with loading progress if data isn't ready yet.
async fn require_ready(
    state: &AppState,
) -> Result<(), (axum::http::StatusCode, Json<NotReadyResponse>)> {
    if state.loading.is_ready().await {
        return Ok(());
    }
    let status = state.loading.to_status().await;
    Err((
        axum::http::StatusCode::SERVICE_UNAVAILABLE,
        Json(NotReadyResponse {
            error: "Data is still loading. Check /loading for progress.",
            loading: status,
        }),
    ))
}

// ── Health & Loading ──────────────────────────────────────────────

#[derive(Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub version: &'static str,
    pub data_ready: bool,
    pub loading_phase: &'static str,
}

pub async fn health(State(state): State<Arc<AppState>>) -> Json<HealthResponse> {
    let status = state.loading.to_status().await;
    Json(HealthResponse {
        status: "ok",
        version: "0.1.0",
        data_ready: status.is_ready,
        loading_phase: status.phase,
    })
}

pub async fn loading(State(state): State<Arc<AppState>>) -> Json<LoadingStatus> {
    Json(state.loading.to_status().await)
}

// ── Stats ─────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct StatsResponse {
    pub doc_count: u64,
    pub segment_count: usize,
    pub segment_ids: Vec<String>,
    pub node_count: usize,
    pub edge_count: usize,
    pub nodes_by_type: std::collections::HashMap<String, usize>,
    pub edges_by_type: std::collections::HashMap<String, usize>,
    pub is_ready: bool,
    pub pagerank_count: usize,
    pub community_count: usize,
    pub degree_count: usize,
}

pub async fn stats(State(state): State<Arc<AppState>>) -> Json<StatsResponse> {
    let graph = state.graph.read().await;
    let gs = graph.stats();
    let segment_ids = state.segment_ids.read().await;
    let (pr_count, comm_count, deg_count) = {
        let k = state.knowledge.read().unwrap();
        (k.pagerank.len(), k.communities.len(), k.degrees.len())
    };
    Json(StatsResponse {
        doc_count: state.doc_count.load(Ordering::Relaxed),
        segment_count: segment_ids.len(),
        segment_ids: segment_ids.clone(),
        node_count: gs.node_count,
        edge_count: gs.edge_count,
        nodes_by_type: gs.nodes_by_type,
        edges_by_type: gs.edges_by_type,
        is_ready: state.loading.is_ready().await,
        pagerank_count: pr_count,
        community_count: comm_count,
        degree_count: deg_count,
    })
}

// ── Graph endpoints ───────────────────────────────────────────────

#[derive(Serialize)]
pub struct NodeResponse {
    pub id: String,
    pub entity_type: String,
    pub key: String,
}

pub async fn graph_nodes(
    State(state): State<Arc<AppState>>,
    axum::extract::Query(params): axum::extract::Query<NodeQueryParams>,
) -> Result<Json<Vec<NodeResponse>>, (axum::http::StatusCode, Json<NotReadyResponse>)> {
    require_ready(&state).await?;

    let graph = state.graph.read().await;
    let limit = params.limit.unwrap_or(100).min(1000);
    let entity_filter = params.entity_type.as_deref();

    let nodes: Vec<NodeResponse> = graph
        .nodes
        .values()
        .filter(|n| {
            entity_filter
                .map(|f| n.entity_type.to_string().eq_ignore_ascii_case(f))
                .unwrap_or(true)
        })
        .take(limit)
        .map(|n| NodeResponse {
            id: n.id.to_string(),
            entity_type: n.entity_type.to_string(),
            key: n.key.clone(),
        })
        .collect();

    Ok(Json(nodes))
}

#[derive(serde::Deserialize)]
pub struct NodeQueryParams {
    pub limit: Option<usize>,
    pub entity_type: Option<String>,
}

#[derive(Serialize)]
pub struct NodeDetailResponse {
    pub id: String,
    pub entity_type: String,
    pub key: String,
    pub neighbors: Vec<NeighborResponse>,
}

#[derive(Serialize)]
pub struct NeighborResponse {
    pub node_id: String,
    pub entity_type: String,
    pub key: String,
    pub edge_type: String,
    pub weight: f64,
}

pub async fn graph_node_by_id(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<NodeDetailResponse>, axum::http::StatusCode> {
    // Allow 503 during loading (no custom body needed for path-based lookups).
    if !state.loading.is_ready().await {
        return Err(axum::http::StatusCode::SERVICE_UNAVAILABLE);
    }

    let node_id: uuid::Uuid = id
        .parse()
        .map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;

    let graph = state.graph.read().await;
    let node = graph
        .nodes
        .get(&node_id)
        .ok_or(axum::http::StatusCode::NOT_FOUND)?;

    let neighbors: Vec<NeighborResponse> = graph
        .neighbors(&node_id)
        .into_iter()
        .map(|(edge, neighbor)| NeighborResponse {
            node_id: neighbor.id.to_string(),
            entity_type: neighbor.entity_type.to_string(),
            key: neighbor.key.clone(),
            edge_type: edge.edge_type.to_string(),
            weight: edge.weight,
        })
        .collect();

    Ok(Json(NodeDetailResponse {
        id: node.id.to_string(),
        entity_type: node.entity_type.to_string(),
        key: node.key.clone(),
        neighbors,
    }))
}

/// Return graph data for D3 force visualization (sampled to keep browser responsive).
#[derive(Serialize)]
pub struct ForceGraphResponse {
    pub nodes: Vec<ForceNode>,
    pub links: Vec<ForceLink>,
}

#[derive(Serialize)]
pub struct ForceNode {
    pub id: String,
    pub entity_type: String,
    pub key: String,
}

#[derive(Serialize)]
pub struct ForceLink {
    pub source: String,
    pub target: String,
    pub edge_type: String,
    pub weight: f64,
}

pub async fn graph_force(
    State(state): State<Arc<AppState>>,
    axum::extract::Query(params): axum::extract::Query<ForceGraphParams>,
) -> Result<Json<ForceGraphResponse>, (axum::http::StatusCode, Json<NotReadyResponse>)> {
    require_ready(&state).await?;

    let graph = state.graph.read().await;
    let limit = params.limit.unwrap_or(200).min(500);

    // Collect nodes up to limit
    let node_set: std::collections::HashSet<uuid::Uuid> = graph
        .nodes
        .keys()
        .take(limit)
        .copied()
        .collect();

    let nodes: Vec<ForceNode> = node_set
        .iter()
        .filter_map(|id| graph.nodes.get(id))
        .map(|n| ForceNode {
            id: n.id.to_string(),
            entity_type: n.entity_type.to_string(),
            key: n.key.clone(),
        })
        .collect();

    let links: Vec<ForceLink> = graph
        .edges
        .values()
        .filter(|e| node_set.contains(&e.source) && node_set.contains(&e.target))
        .map(|e| ForceLink {
            source: e.source.to_string(),
            target: e.target.to_string(),
            edge_type: e.edge_type.to_string(),
            weight: e.weight,
        })
        .collect();

    Ok(Json(ForceGraphResponse { nodes, links }))
}

#[derive(serde::Deserialize)]
pub struct ForceGraphParams {
    pub limit: Option<usize>,
}

// ── Catalog endpoint ───────────────────────────────────────────────

pub async fn catalog(
    State(state): State<Arc<AppState>>,
) -> Result<Json<stupid_catalog::Catalog>, (axum::http::StatusCode, Json<NotReadyResponse>)> {
    require_ready(&state).await?;
    let catalog_lock = state.catalog.read().await;
    match catalog_lock.as_ref() {
        Some(cat) => Ok(Json(cat.clone())),
        None => {
            let status = state.loading.to_status().await;
            Err((
                axum::http::StatusCode::SERVICE_UNAVAILABLE,
                Json(NotReadyResponse {
                    error: "Catalog not yet built.",
                    loading: status,
                }),
            ))
        }
    }
}

// ── Compute endpoints ──────────────────────────────────────────────

#[derive(serde::Deserialize)]
pub struct ComputeQueryParams {
    pub limit: Option<usize>,
}

#[derive(Serialize)]
pub struct PageRankEntry {
    pub id: String,
    pub entity_type: String,
    pub key: String,
    pub score: f64,
}

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

#[derive(Serialize)]
pub struct CommunitySummary {
    pub community_id: u64,
    pub member_count: usize,
    pub top_nodes: Vec<CommunityNode>,
}

#[derive(Serialize)]
pub struct CommunityNode {
    pub id: String,
    pub entity_type: String,
    pub key: String,
}

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

#[derive(Serialize)]
pub struct DegreeEntry {
    pub id: String,
    pub entity_type: String,
    pub key: String,
    pub in_deg: usize,
    pub out_deg: usize,
    pub total: usize,
}

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

// ── Pattern detection endpoints ────────────────────────────────

#[derive(Serialize)]
pub struct PatternResponse {
    pub id: String,
    pub sequence: Vec<String>,
    pub support: f64,
    pub member_count: usize,
    pub avg_duration_secs: f64,
    pub category: String,
    pub description: Option<String>,
}

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

#[derive(serde::Deserialize)]
pub struct CooccurrenceQueryParams {
    pub entity_type_a: Option<String>,
    pub entity_type_b: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Serialize)]
pub struct CooccurrenceEntry {
    pub entity_a: String,
    pub entity_b: String,
    pub count: f64,
    pub pmi: Option<f64>,
}

#[derive(Serialize)]
pub struct CooccurrenceResponse {
    pub entity_type_a: String,
    pub entity_type_b: String,
    pub pairs: Vec<CooccurrenceEntry>,
}

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

#[derive(Serialize)]
pub struct TrendResponse {
    pub metric: String,
    pub current_value: f64,
    pub baseline_mean: f64,
    pub direction: String,
    pub magnitude: f64,
}

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

// ── Anomaly detection endpoints ────────────────────────────────

#[derive(Serialize)]
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

#[derive(Serialize)]
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

#[derive(Serialize)]
pub struct QueryErrorResponse {
    pub error: String,
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

// ── Queue status ──────────────────────────────────────────────

pub async fn queue_status(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let is_empty = state.queue_metrics.read().unwrap().is_empty();

    if is_empty {
        // Even with no active consumers, show enabled=true if queue connections
        // exist in the store — so the dashboard nav link remains visible.
        let has_connections = state
            .queue_connections
            .read()
            .await
            .list()
            .map(|v| !v.is_empty())
            .unwrap_or(false);
        return Json(serde_json::json!({"enabled": has_connections, "queues": {}}));
    }

    let metrics_map = state.queue_metrics.read().unwrap();

    let mut queues = serde_json::Map::new();
    for (id, m) in metrics_map.iter() {
        let batches = m.batches_processed.load(Ordering::Relaxed);
        let total_time_us = m.total_processing_time_us.load(Ordering::Relaxed);
        let avg_latency_ms = if batches > 0 {
            (total_time_us as f64 / batches as f64) / 1000.0
        } else {
            0.0
        };

        queues.insert(id.clone(), serde_json::json!({
            "enabled": m.enabled.load(Ordering::Relaxed),
            "connected": m.connected.load(Ordering::Relaxed),
            "messages_received": m.messages_received.load(Ordering::Relaxed),
            "messages_processed": m.messages_processed.load(Ordering::Relaxed),
            "messages_failed": m.messages_failed.load(Ordering::Relaxed),
            "batches_processed": batches,
            "avg_batch_latency_ms": avg_latency_ms,
            "last_poll_epoch_ms": m.last_poll_epoch_ms.load(Ordering::Relaxed),
        }));
    }

    Json(serde_json::json!({"enabled": true, "queues": queues}))
}

// ── Scheduler metrics ─────────────────────────────────────────

pub async fn scheduler_metrics(
    State(state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    let scheduler_lock = state.scheduler.read().await;
    match scheduler_lock.as_ref() {
        Some(handle) => {
            let metrics = handle.metrics.read().unwrap();
            Json(serde_json::to_value(&*metrics).unwrap_or(serde_json::Value::Null))
        }
        None => Json(serde_json::json!({"status": "scheduler not started"})),
    }
}

// ── Agent endpoints ────────────────────────────────────────────

pub async fn agents_list(
    State(state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    match state.agent_executor.as_ref() {
        Some(executor) => {
            let agents = stupid_agent::config::agents_to_info(&executor.agents);
            Json(serde_json::json!({ "agents": agents }))
        }
        None => Json(serde_json::json!({
            "agents": [],
            "error": "Agent system not configured. Set AGENTS_DIR in config."
        })),
    }
}

#[derive(Deserialize)]
pub struct AgentExecuteRequest {
    pub agent_name: String,
    pub task: String,
    #[serde(default)]
    pub context: serde_json::Value,
}

pub async fn agents_execute(
    State(state): State<Arc<AppState>>,
    Json(req): Json<AgentExecuteRequest>,
) -> Result<Json<stupid_agent::AgentResponse>, (axum::http::StatusCode, Json<QueryErrorResponse>)> {
    let executor = state.agent_executor.as_ref().ok_or_else(|| {
        (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            Json(QueryErrorResponse {
                error: "Agent system not configured.".into(),
            }),
        )
    })?;

    let context = if req.context.is_null() {
        None
    } else {
        Some(&req.context)
    };

    let result = executor
        .execute(&req.agent_name, &req.task, context)
        .await
        .map_err(|e| {
            (
                axum::http::StatusCode::BAD_REQUEST,
                Json(QueryErrorResponse {
                    error: e.to_string(),
                }),
            )
        })?;

    Ok(Json(result))
}

/// SSE streaming endpoint for agent chat.
pub async fn agents_chat(
    State(state): State<Arc<AppState>>,
    Json(req): Json<AgentExecuteRequest>,
) -> Result<
    Sse<impl futures::Stream<Item = Result<Event, Infallible>>>,
    (axum::http::StatusCode, Json<QueryErrorResponse>),
> {
    let executor = state.agent_executor.as_ref().ok_or_else(|| {
        (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            Json(QueryErrorResponse {
                error: "Agent system not configured.".into(),
            }),
        )
    })?;

    let context = if req.context.is_null() {
        None
    } else {
        Some(&req.context)
    };

    // Execute agent and stream the response
    let result = executor
        .execute(&req.agent_name, &req.task, context)
        .await;

    let events = match result {
        Ok(response) => {
            let data = serde_json::to_string(&response).unwrap_or_default();
            vec![
                Ok(Event::default().event("agent_response").data(data)),
                Ok(Event::default().event("done").data("[DONE]")),
            ]
        }
        Err(e) => {
            vec![
                Ok(Event::default()
                    .event("error")
                    .data(serde_json::json!({"error": e.to_string()}).to_string())),
            ]
        }
    };

    Ok(Sse::new(stream::iter(events)))
}

// ── Team endpoints ─────────────────────────────────────────────

#[derive(Deserialize)]
pub struct TeamExecuteRequest {
    pub task: String,
    #[serde(default = "default_strategy")]
    pub strategy: stupid_agent::TeamStrategy,
    #[serde(default)]
    pub context: serde_json::Value,
}

fn default_strategy() -> stupid_agent::TeamStrategy {
    stupid_agent::TeamStrategy::FullHierarchy
}

pub async fn teams_execute(
    State(state): State<Arc<AppState>>,
    Json(req): Json<TeamExecuteRequest>,
) -> Result<Json<stupid_agent::TeamResponse>, (axum::http::StatusCode, Json<QueryErrorResponse>)> {
    let executor = state.agent_executor.as_ref().ok_or_else(|| {
        (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            Json(QueryErrorResponse {
                error: "Agent system not configured.".into(),
            }),
        )
    })?;

    let context = if req.context.is_null() {
        None
    } else {
        Some(&req.context)
    };

    let result = stupid_agent::TeamExecutor::execute(
        executor,
        &req.task,
        req.strategy,
        context,
    )
    .await;

    Ok(Json(result))
}

pub async fn teams_strategies() -> Json<serde_json::Value> {
    let strategies = stupid_agent::TeamExecutor::strategies();
    Json(serde_json::json!({ "strategies": strategies }))
}

// ── Connection CRUD endpoints ─────────────────────────────────

/// List all connections (passwords masked).
pub async fn connections_list(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<ConnectionSafe>>, (axum::http::StatusCode, Json<QueryErrorResponse>)> {
    let store = state.connections.read().await;
    store.list().map(Json).map_err(|e| {
        (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(QueryErrorResponse {
                error: format!("Failed to list connections: {}", e),
            }),
        )
    })
}

/// Add a new connection.
pub async fn connections_add(
    State(state): State<Arc<AppState>>,
    Json(input): Json<ConnectionInput>,
) -> Result<(axum::http::StatusCode, Json<ConnectionSafe>), (axum::http::StatusCode, Json<QueryErrorResponse>)> {
    let store = state.connections.read().await;
    store.add(&input).map(|c| (axum::http::StatusCode::CREATED, Json(c))).map_err(|e| {
        let status = if e.to_string().contains("already exists") {
            axum::http::StatusCode::CONFLICT
        } else {
            axum::http::StatusCode::INTERNAL_SERVER_ERROR
        };
        (status, Json(QueryErrorResponse { error: e.to_string() }))
    })
}

/// Get a single connection (password masked).
pub async fn connections_get(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ConnectionSafe>, axum::http::StatusCode> {
    let store = state.connections.read().await;
    match store.get_safe(&id) {
        Ok(Some(c)) => Ok(Json(c)),
        Ok(None) => Err(axum::http::StatusCode::NOT_FOUND),
        Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR),
    }
}

/// Update an existing connection.
pub async fn connections_update(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(input): Json<ConnectionInput>,
) -> Result<Json<ConnectionSafe>, axum::http::StatusCode> {
    let store = state.connections.read().await;
    match store.update(&id, &input) {
        Ok(Some(c)) => Ok(Json(c)),
        Ok(None) => Err(axum::http::StatusCode::NOT_FOUND),
        Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR),
    }
}

/// Delete a connection.
pub async fn connections_delete(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> axum::http::StatusCode {
    let store = state.connections.read().await;
    match store.delete(&id) {
        Ok(true) => axum::http::StatusCode::NO_CONTENT,
        Ok(false) => axum::http::StatusCode::NOT_FOUND,
        Err(_) => axum::http::StatusCode::INTERNAL_SERVER_ERROR,
    }
}

/// Get decrypted credentials for a connection (used by dashboard pool manager).
pub async fn connections_credentials(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ConnectionCredentials>, axum::http::StatusCode> {
    let store = state.connections.read().await;
    match store.get_credentials(&id) {
        Ok(Some(c)) => Ok(Json(c)),
        Ok(None) => Err(axum::http::StatusCode::NOT_FOUND),
        Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR),
    }
}

// ── Queue Connection CRUD endpoints ──────────────────────────────

/// List all queue connections (credentials masked).
pub async fn queue_connections_list(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<QueueConnectionSafe>>, (axum::http::StatusCode, Json<QueryErrorResponse>)> {
    let store = state.queue_connections.read().await;
    store.list().map(Json).map_err(|e| {
        (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(QueryErrorResponse {
                error: format!("Failed to list queue connections: {}", e),
            }),
        )
    })
}

/// Add a new queue connection.
pub async fn queue_connections_add(
    State(state): State<Arc<AppState>>,
    Json(input): Json<QueueConnectionInput>,
) -> Result<(axum::http::StatusCode, Json<QueueConnectionSafe>), (axum::http::StatusCode, Json<QueryErrorResponse>)> {
    let store = state.queue_connections.read().await;
    store.add(&input).map(|c| (axum::http::StatusCode::CREATED, Json(c))).map_err(|e| {
        let status = if e.to_string().contains("already exists") {
            axum::http::StatusCode::CONFLICT
        } else {
            axum::http::StatusCode::INTERNAL_SERVER_ERROR
        };
        (status, Json(QueryErrorResponse { error: e.to_string() }))
    })
}

/// Get a single queue connection (credentials masked).
pub async fn queue_connections_get(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<QueueConnectionSafe>, axum::http::StatusCode> {
    let store = state.queue_connections.read().await;
    match store.get_safe(&id) {
        Ok(Some(c)) => Ok(Json(c)),
        Ok(None) => Err(axum::http::StatusCode::NOT_FOUND),
        Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR),
    }
}

/// Update an existing queue connection.
pub async fn queue_connections_update(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(input): Json<QueueConnectionInput>,
) -> Result<Json<QueueConnectionSafe>, axum::http::StatusCode> {
    let store = state.queue_connections.read().await;
    match store.update(&id, &input) {
        Ok(Some(c)) => Ok(Json(c)),
        Ok(None) => Err(axum::http::StatusCode::NOT_FOUND),
        Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR),
    }
}

/// Delete a queue connection.
pub async fn queue_connections_delete(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> axum::http::StatusCode {
    let store = state.queue_connections.read().await;
    match store.delete(&id) {
        Ok(true) => axum::http::StatusCode::NO_CONTENT,
        Ok(false) => axum::http::StatusCode::NOT_FOUND,
        Err(_) => axum::http::StatusCode::INTERNAL_SERVER_ERROR,
    }
}

/// Get decrypted credentials for a queue connection (used by SQS consumer).
pub async fn queue_connections_credentials(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<QueueConnectionCredentials>, axum::http::StatusCode> {
    let store = state.queue_connections.read().await;
    match store.get_credentials(&id) {
        Ok(Some(c)) => Ok(Json(c)),
        Ok(None) => Err(axum::http::StatusCode::NOT_FOUND),
        Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR),
    }
}

// ── Athena Connection CRUD endpoints ─────────────────────────────

/// List all Athena connections (credentials masked).
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

        match crate::athena_query::fetch_schema(&creds, &conn).await {
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
pub async fn athena_connections_delete(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> axum::http::StatusCode {
    let store = state.athena_connections.read().await;
    match store.delete(&id) {
        Ok(true) => axum::http::StatusCode::NO_CONTENT,
        Ok(false) => axum::http::StatusCode::NOT_FOUND,
        Err(_) => axum::http::StatusCode::INTERNAL_SERVER_ERROR,
    }
}

/// Get decrypted credentials for an Athena connection.
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

// ── Athena Query SSE endpoint ─────────────────────────────────────

#[derive(Deserialize)]
pub struct AthenaQueryRequest {
    pub sql: String,
    #[serde(default)]
    pub database: Option<String>,
}

/// SSE streaming Athena query execution.
///
/// Submits a SQL query to AWS Athena via the specified connection, polls for
/// status updates, and streams results back as Server-Sent Events.
///
/// Events emitted:
/// - `status`  — query state transitions (QUEUED, RUNNING, SUCCEEDED) with stats
/// - `columns` — column metadata (name + type) sent once before row data
/// - `rows`    — batches of up to 100 result rows
/// - `done`    — final summary (total_rows, data_scanned_bytes, execution_time_ms)
/// - `error`   — terminal error with message
pub async fn athena_query_sse(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<AthenaQueryRequest>,
) -> Result<
    Sse<impl futures::Stream<Item = Result<Event, Infallible>>>,
    (axum::http::StatusCode, Json<QueryErrorResponse>),
> {
    // 1. Get credentials and connection config.
    let store = state.athena_connections.read().await;
    let creds = match store.get_credentials(&id) {
        Ok(Some(c)) => c,
        Ok(None) => {
            return Err((
                axum::http::StatusCode::NOT_FOUND,
                Json(QueryErrorResponse {
                    error: "Connection not found".into(),
                }),
            ))
        }
        Err(e) => {
            return Err((
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(QueryErrorResponse {
                    error: e.to_string(),
                }),
            ))
        }
    };
    let conn = match store.get(&id) {
        Ok(Some(c)) => c,
        _ => {
            return Err((
                axum::http::StatusCode::NOT_FOUND,
                Json(QueryErrorResponse {
                    error: "Connection not found".into(),
                }),
            ))
        }
    };
    drop(store);

    let database = req.database.unwrap_or_else(|| conn.database.clone());
    let workgroup = conn.workgroup.clone();
    let output_location = conn.output_location.clone();
    let sql = req.sql.clone();

    // 2. Create a channel-based stream.
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Event, Infallible>>(32);

    // 3. Spawn background task to execute query and stream events.
    tokio::spawn(async move {
        let client = crate::athena_query::build_athena_client(&creds).await;

        // Start query.
        let query_id = match crate::athena_query::start_query(
            &client,
            &sql,
            &database,
            &workgroup,
            &output_location,
        )
        .await
        {
            Ok(id) => {
                let _ = tx
                    .send(Ok(Event::default().event("status").data(
                        serde_json::json!({"state": "QUEUED", "query_id": &id}).to_string(),
                    )))
                    .await;
                id
            }
            Err(e) => {
                let _ = tx
                    .send(Ok(Event::default().event("error").data(
                        serde_json::json!({"message": e.to_string()}).to_string(),
                    )))
                    .await;
                return;
            }
        };

        // Poll for status updates.
        let timeout = std::time::Duration::from_secs(120);
        let start = std::time::Instant::now();
        let poll_interval = std::time::Duration::from_millis(500);

        loop {
            if start.elapsed() > timeout {
                let _ = tx
                    .send(Ok(Event::default().event("error").data(
                        serde_json::json!({"message": "Query timed out after 120s"}).to_string(),
                    )))
                    .await;
                return;
            }

            let response = match client
                .get_query_execution()
                .query_execution_id(&query_id)
                .send()
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    let _ = tx
                        .send(Ok(Event::default().event("error").data(
                            serde_json::json!({"message": e.to_string()}).to_string(),
                        )))
                        .await;
                    return;
                }
            };

            let execution = match response.query_execution() {
                Some(e) => e,
                None => {
                    tokio::time::sleep(poll_interval).await;
                    continue;
                }
            };

            let state_str = execution
                .status()
                .and_then(|s| s.state())
                .map(|s| s.as_str().to_string())
                .unwrap_or_default();

            let data_scanned = execution
                .statistics()
                .map(|s| s.data_scanned_in_bytes().unwrap_or(0))
                .unwrap_or(0);

            match state_str.as_str() {
                "SUCCEEDED" => {
                    let exec_time_ms = execution
                        .statistics()
                        .map(|s| s.engine_execution_time_in_millis().unwrap_or(0))
                        .unwrap_or(0);

                    let _ = tx
                        .send(Ok(Event::default().event("status").data(
                            serde_json::json!({
                                "state": "SUCCEEDED",
                                "data_scanned_bytes": data_scanned,
                                "execution_time_ms": exec_time_ms
                            })
                            .to_string(),
                        )))
                        .await;

                    // Stream results in batches of 100.
                    let mut next_token: Option<String> = None;
                    let mut is_first_page = true;
                    let mut total_rows = 0u64;

                    loop {
                        let mut request = client
                            .get_query_results()
                            .query_execution_id(&query_id)
                            .max_results(100);

                        if let Some(ref token) = next_token {
                            request = request.next_token(token);
                        }

                        match request.send().await {
                            Ok(result_response) => {
                                if let Some(result_set) = result_response.result_set() {
                                    // Send column metadata on first page only.
                                    if is_first_page {
                                        if let Some(metadata) = result_set.result_set_metadata() {
                                            let columns: Vec<serde_json::Value> = metadata
                                                .column_info()
                                                .iter()
                                                .map(|c| {
                                                    serde_json::json!({
                                                        "name": c.name(),
                                                        "type": c.r#type()
                                                    })
                                                })
                                                .collect();
                                            let _ = tx
                                                .send(Ok(Event::default().event("columns").data(
                                                    serde_json::json!({"columns": columns})
                                                        .to_string(),
                                                )))
                                                .await;
                                        }
                                    }

                                    // Send rows (skip header row on first page).
                                    let mut batch_rows: Vec<Vec<String>> = Vec::new();
                                    for (i, row) in result_set.rows().iter().enumerate() {
                                        if is_first_page && i == 0 {
                                            continue;
                                        }
                                        let row_data: Vec<String> = row
                                            .data()
                                            .iter()
                                            .map(|d| {
                                                d.var_char_value().unwrap_or("").to_string()
                                            })
                                            .collect();
                                        batch_rows.push(row_data);
                                    }

                                    if !batch_rows.is_empty() {
                                        total_rows += batch_rows.len() as u64;
                                        let _ = tx
                                            .send(Ok(Event::default().event("rows").data(
                                                serde_json::json!({"rows": batch_rows})
                                                    .to_string(),
                                            )))
                                            .await;
                                    }
                                }

                                is_first_page = false;
                                next_token =
                                    result_response.next_token().map(|t| t.to_string());
                                if next_token.is_none() {
                                    break;
                                }
                            }
                            Err(e) => {
                                let _ = tx
                                    .send(Ok(Event::default().event("error").data(
                                        serde_json::json!({
                                            "message": format!("Failed to get results: {}", e)
                                        })
                                        .to_string(),
                                    )))
                                    .await;
                                return;
                            }
                        }
                    }

                    // Send done event.
                    let _ = tx
                        .send(Ok(Event::default().event("done").data(
                            serde_json::json!({
                                "total_rows": total_rows,
                                "data_scanned_bytes": data_scanned,
                                "execution_time_ms": exec_time_ms
                            })
                            .to_string(),
                        )))
                        .await;
                    return;
                }
                "FAILED" => {
                    let reason = execution
                        .status()
                        .and_then(|s| s.state_change_reason())
                        .unwrap_or("Unknown error");
                    let _ = tx
                        .send(Ok(Event::default().event("error").data(
                            serde_json::json!({"message": reason}).to_string(),
                        )))
                        .await;
                    return;
                }
                "CANCELLED" => {
                    let _ = tx
                        .send(Ok(Event::default().event("error").data(
                            serde_json::json!({"message": "Query was cancelled"}).to_string(),
                        )))
                        .await;
                    return;
                }
                _ => {
                    // QUEUED or RUNNING — send status update and keep polling.
                    let _ = tx
                        .send(Ok(Event::default().event("status").data(
                            serde_json::json!({
                                "state": state_str,
                                "data_scanned_bytes": data_scanned
                            })
                            .to_string(),
                        )))
                        .await;
                    tokio::time::sleep(poll_interval).await;
                }
            }
        }
    });

    // Convert mpsc receiver to a stream.
    let stream = ReceiverStream::new(rx);
    Ok(Sse::new(stream))
}

/// Get cached schema for an Athena connection.
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

/// Trigger background schema refresh for an Athena connection.
pub async fn athena_connections_schema_refresh(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, Json<QueryErrorResponse>)> {
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
        match crate::athena_query::fetch_schema(&creds, &conn).await {
            Ok(schema) => {
                let store = state_clone.athena_connections.read().await;
                let _ = store.update_schema(&id_clone, schema);
                tracing::info!("Schema refresh complete for Athena connection '{}'", id_clone);
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

