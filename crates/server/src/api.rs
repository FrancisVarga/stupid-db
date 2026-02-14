use std::sync::Arc;

use axum::extract::{Path, State};
use axum::Json;
use serde::Serialize;

use crate::state::AppState;

#[derive(Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub version: &'static str,
}

pub async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        version: "0.1.0",
    })
}

#[derive(Serialize)]
pub struct StatsResponse {
    pub doc_count: u64,
    pub segment_count: usize,
    pub segment_ids: Vec<String>,
    pub node_count: usize,
    pub edge_count: usize,
    pub nodes_by_type: std::collections::HashMap<String, usize>,
    pub edges_by_type: std::collections::HashMap<String, usize>,
}

pub async fn stats(State(state): State<Arc<AppState>>) -> Json<StatsResponse> {
    let graph = state.graph.read().await;
    let gs = graph.stats();
    Json(StatsResponse {
        doc_count: state.doc_count,
        segment_count: state.segment_ids.len(),
        segment_ids: state.segment_ids.clone(),
        node_count: gs.node_count,
        edge_count: gs.edge_count,
        nodes_by_type: gs.nodes_by_type,
        edges_by_type: gs.edges_by_type,
    })
}

#[derive(Serialize)]
pub struct NodeResponse {
    pub id: String,
    pub entity_type: String,
    pub key: String,
}

pub async fn graph_nodes(
    State(state): State<Arc<AppState>>,
    axum::extract::Query(params): axum::extract::Query<NodeQueryParams>,
) -> Json<Vec<NodeResponse>> {
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

    Json(nodes)
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
) -> Json<ForceGraphResponse> {
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

    Json(ForceGraphResponse { nodes, links })
}

#[derive(serde::Deserialize)]
pub struct ForceGraphParams {
    pub limit: Option<usize>,
}

// ── Catalog endpoint ───────────────────────────────────────────────

pub async fn catalog(
    State(state): State<Arc<AppState>>,
) -> Json<stupid_catalog::Catalog> {
    Json(state.catalog.clone())
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
    let compute_lock = state.compute.read().await;
    let Some(compute) = compute_lock.as_ref() else {
        return Json(vec![]);  // Compute still running
    };

    let limit = params.limit.unwrap_or(50).min(500);
    let graph = state.graph.read().await;

    let mut entries: Vec<(&uuid::Uuid, &f64)> = compute.pagerank.iter().collect();
    entries.sort_by(|a, b| b.1.partial_cmp(a.1).unwrap_or(std::cmp::Ordering::Equal));

    let result: Vec<PageRankEntry> = entries
        .into_iter()
        .take(limit)
        .filter_map(|(&node_id, &score)| {
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
    let compute_lock = state.compute.read().await;
    let Some(compute) = compute_lock.as_ref() else {
        return Json(vec![]);  // Compute still running
    };

    let graph = state.graph.read().await;

    // Group nodes by community
    let mut community_members: std::collections::HashMap<u64, Vec<uuid::Uuid>> =
        std::collections::HashMap::new();
    for (&node_id, &community_id) in &compute.communities {
        community_members
            .entry(community_id)
            .or_default()
            .push(node_id);
    }

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
    let compute_lock = state.compute.read().await;
    let Some(compute) = compute_lock.as_ref() else {
        return Json(vec![]);  // Compute still running
    };

    let limit = params.limit.unwrap_or(50).min(500);
    let graph = state.graph.read().await;

    let mut entries: Vec<(&uuid::Uuid, &stupid_compute::DegreeInfo)> =
        compute.degrees.iter().collect();
    entries.sort_by(|a, b| b.1.total.cmp(&a.1.total));

    let result: Vec<DegreeEntry> = entries
        .into_iter()
        .take(limit)
        .filter_map(|(&node_id, deg)| {
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
    let qg = state.query_generator.as_ref().ok_or_else(|| {
        (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            Json(QueryErrorResponse {
                error: "LLM query generator not configured. Set LLM_PROVIDER and API keys.".into(),
            }),
        )
    })?;

    let graph = state.graph.read().await;

    let result = qg
        .ask(&req.question, &state.catalog, &graph)
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
