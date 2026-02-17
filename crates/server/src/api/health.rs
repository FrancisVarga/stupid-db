//! Health, loading, stats, catalog, queue status, and scheduler metrics endpoints.
//!
//! SRP: server readiness and operational metrics.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use axum::extract::State;
use axum::Json;
use serde::Serialize;

use crate::credential_store::CredentialStore;
use crate::state::AppState;

use super::NotReadyResponse;

// ── Health & Loading ──────────────────────────────────────────────

#[derive(Serialize, utoipa::ToSchema)]
pub struct HealthResponse {
    pub status: &'static str,
    pub version: &'static str,
    pub data_ready: bool,
    pub loading_phase: &'static str,
}

/// Basic health check returning server status and readiness.
#[utoipa::path(
    get,
    path = "/health",
    tag = "Health",
    responses(
        (status = 200, description = "Server is running", body = HealthResponse)
    )
)]
pub async fn health(State(state): State<Arc<AppState>>) -> Json<HealthResponse> {
    let status = state.loading.to_status().await;
    Json(HealthResponse {
        status: "ok",
        version: "0.1.0",
        data_ready: status.is_ready,
        loading_phase: status.phase,
    })
}

/// Current loading progress with phase, progress count, and elapsed time.
#[utoipa::path(
    get,
    path = "/loading",
    tag = "Health",
    responses(
        (status = 200, description = "Loading progress", body = Object)
    )
)]
pub async fn loading(
    State(state): State<Arc<AppState>>,
) -> Json<crate::state::LoadingStatus> {
    Json(state.loading.to_status().await)
}

// ── Stats ─────────────────────────────────────────────────────────

#[derive(Serialize, utoipa::ToSchema)]
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

/// Aggregate statistics: document count, segments, graph nodes/edges, and compute results.
#[utoipa::path(
    get,
    path = "/stats",
    tag = "Health",
    responses(
        (status = 200, description = "Aggregate statistics", body = StatsResponse)
    )
)]
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

// ── Catalog endpoint ───────────────────────────────────────────────

/// Return the entity/schema catalog (available after loading completes).
#[utoipa::path(
    get,
    path = "/catalog",
    tag = "Health",
    responses(
        (status = 200, description = "Entity and schema catalog", body = Object),
        (status = 503, description = "Service not ready", body = NotReadyResponse)
    )
)]
pub async fn catalog(
    State(state): State<Arc<AppState>>,
) -> Result<Json<stupid_catalog::Catalog>, (axum::http::StatusCode, Json<NotReadyResponse>)> {
    super::require_ready(&state).await?;
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

// ── Queue status ──────────────────────────────────────────────

/// Queue consumer metrics: message counts, latency, and connection state.
#[utoipa::path(
    get,
    path = "/queue/status",
    tag = "Health",
    responses(
        (status = 200, description = "Queue consumer metrics", body = Object)
    )
)]
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

/// Compute scheduler metrics: run counts, durations, and next scheduled time.
#[utoipa::path(
    get,
    path = "/scheduler/metrics",
    tag = "Health",
    responses(
        (status = 200, description = "Scheduler execution metrics", body = Object)
    )
)]
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
