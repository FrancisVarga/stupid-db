//! Graph topology endpoints: node listing, detail, and D3 force layout.
//!
//! SRP: graph structure queries for visualization.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::Json;
use serde::Serialize;

use crate::state::AppState;

use super::NotReadyResponse;

// ── Graph endpoints ───────────────────────────────────────────────

#[derive(Serialize, utoipa::ToSchema)]
pub struct NodeResponse {
    pub id: String,
    pub entity_type: String,
    pub key: String,
}

/// List graph nodes with optional entity type filter and limit.
#[utoipa::path(
    get,
    path = "/graph/nodes",
    tag = "Graph",
    params(NodeQueryParams),
    responses(
        (status = 200, description = "List of graph nodes", body = Vec<NodeResponse>),
        (status = 503, description = "Service not ready", body = NotReadyResponse)
    )
)]
pub async fn graph_nodes(
    State(state): State<Arc<AppState>>,
    axum::extract::Query(params): axum::extract::Query<NodeQueryParams>,
) -> Result<Json<Vec<NodeResponse>>, (axum::http::StatusCode, Json<NotReadyResponse>)> {
    super::require_ready(&state).await?;

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

#[derive(serde::Deserialize, utoipa::IntoParams)]
pub struct NodeQueryParams {
    /// Maximum number of nodes to return (default 100, max 1000).
    pub limit: Option<usize>,
    /// Filter by entity type (case-insensitive).
    pub entity_type: Option<String>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct NodeDetailResponse {
    pub id: String,
    pub entity_type: String,
    pub key: String,
    pub neighbors: Vec<NeighborResponse>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct NeighborResponse {
    pub node_id: String,
    pub entity_type: String,
    pub key: String,
    pub edge_type: String,
    pub weight: f64,
}

/// Get a single node by UUID with its immediate neighbors.
#[utoipa::path(
    get,
    path = "/graph/nodes/{id}",
    tag = "Graph",
    params(
        ("id" = String, Path, description = "Node UUID")
    ),
    responses(
        (status = 200, description = "Node detail with neighbors", body = NodeDetailResponse),
        (status = 400, description = "Invalid UUID format"),
        (status = 404, description = "Node not found"),
        (status = 503, description = "Service not ready")
    )
)]
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
#[derive(Serialize, utoipa::ToSchema)]
pub struct ForceGraphResponse {
    pub nodes: Vec<ForceNode>,
    pub links: Vec<ForceLink>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct ForceNode {
    pub id: String,
    pub entity_type: String,
    pub key: String,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct ForceLink {
    pub source: String,
    pub target: String,
    pub edge_type: String,
    pub weight: f64,
}

/// Sampled graph data for D3 force-directed layout visualization.
#[utoipa::path(
    get,
    path = "/graph/force",
    tag = "Graph",
    params(ForceGraphParams),
    responses(
        (status = 200, description = "Force graph nodes and links", body = ForceGraphResponse),
        (status = 503, description = "Service not ready", body = NotReadyResponse)
    )
)]
pub async fn graph_force(
    State(state): State<Arc<AppState>>,
    axum::extract::Query(params): axum::extract::Query<ForceGraphParams>,
) -> Result<Json<ForceGraphResponse>, (axum::http::StatusCode, Json<NotReadyResponse>)> {
    super::require_ready(&state).await?;

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

#[derive(serde::Deserialize, utoipa::IntoParams)]
pub struct ForceGraphParams {
    /// Maximum number of nodes to include (default 200, max 500).
    pub limit: Option<usize>,
}
