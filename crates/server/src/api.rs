use std::sync::atomic::Ordering;
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::Json;
use serde::Serialize;

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
    let metrics = &state.queue_metrics;
    let enabled = metrics.enabled.load(Ordering::Relaxed);

    if !enabled {
        return Json(serde_json::json!({"enabled": false}));
    }

    let processed = metrics.messages_processed.load(Ordering::Relaxed);
    let batches = metrics.batches_processed.load(Ordering::Relaxed);
    let total_time_us = metrics.total_processing_time_us.load(Ordering::Relaxed);
    let avg_latency_ms = if batches > 0 {
        (total_time_us as f64 / batches as f64) / 1000.0
    } else {
        0.0
    };

    Json(serde_json::json!({
        "enabled": true,
        "connected": metrics.connected.load(Ordering::Relaxed),
        "messages_received": metrics.messages_received.load(Ordering::Relaxed),
        "messages_processed": processed,
        "messages_failed": metrics.messages_failed.load(Ordering::Relaxed),
        "batches_processed": batches,
        "avg_batch_latency_ms": avg_latency_ms,
        "last_poll_epoch_ms": metrics.last_poll_epoch_ms.load(Ordering::Relaxed),
    }))
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

