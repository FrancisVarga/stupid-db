//! Graph-based compute endpoints: PageRank, communities, and degree centrality.

use std::sync::Arc;

use axum::extract::State;
use axum::Json;
use serde::Serialize;

use crate::state::AppState;
use super::ComputeQueryParams;

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
