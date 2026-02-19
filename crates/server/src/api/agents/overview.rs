//! Bundeswehr fleet overview endpoint.
//!
//! Aggregates telemetry and agent-store data into a single fleet-level
//! summary for the overview dashboard tab.

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::State;
use axum::Json;
use serde::Serialize;

use stupid_agent::types::AgentTier;

use crate::state::AppState;

use super::super::QueryErrorResponse;
use super::types::require_agent_store;

// ── Response types ──────────────────────────────────────────────

/// Top-level fleet overview returned by `GET /api/bundeswehr/overview`.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct BundeswehrOverview {
    pub total_agents: usize,
    pub agents_by_tier: TierCounts,
    pub total_executions: usize,
    pub avg_error_rate: f64,
    pub top_agents: Vec<AgentExecSummary>,
    pub worst_agents: Vec<AgentErrorSummary>,
}

/// Agent counts broken down by tier.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct TierCounts {
    pub architect: usize,
    pub lead: usize,
    pub specialist: usize,
}

/// Compact view of an agent's execution count (for "top agents" ranking).
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct AgentExecSummary {
    pub name: String,
    pub executions: usize,
}

/// Compact view of an agent's error rate (for "worst agents" ranking).
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct AgentErrorSummary {
    pub name: String,
    pub error_rate: f64,
}

// ── Handler ─────────────────────────────────────────────────────

/// Fleet-level Bundeswehr overview
///
/// Aggregates agent tier counts from the agent store and execution
/// metrics from the telemetry store into a single dashboard payload.
#[utoipa::path(
    get,
    path = "/api/bundeswehr/overview",
    tag = "Bundeswehr",
    responses(
        (status = 200, description = "Fleet overview", body = BundeswehrOverview),
        (status = 500, description = "Internal error", body = QueryErrorResponse),
        (status = 503, description = "Agent store not configured", body = QueryErrorResponse)
    )
)]
pub async fn bundeswehr_overview(
    State(state): State<Arc<AppState>>,
) -> Result<Json<BundeswehrOverview>, (axum::http::StatusCode, Json<QueryErrorResponse>)> {
    let store = require_agent_store(&state)?;

    // ── Tier counts from agent store ────────────────────────────
    let agents = store.list().await;
    let total_agents = agents.len();

    let mut tier_counts: HashMap<&str, usize> = HashMap::new();
    for agent in &agents {
        let key = match agent.tier {
            AgentTier::Architect => "architect",
            AgentTier::Lead => "lead",
            AgentTier::Specialist => "specialist",
        };
        *tier_counts.entry(key).or_insert(0) += 1;
    }

    let agents_by_tier = TierCounts {
        architect: tier_counts.get("architect").copied().unwrap_or(0),
        lead: tier_counts.get("lead").copied().unwrap_or(0),
        specialist: tier_counts.get("specialist").copied().unwrap_or(0),
    };

    // ── Execution metrics from telemetry store ──────────────────
    let telemetry = state.telemetry_store.read().await;
    let stats = telemetry.overview().map_err(|e| {
        (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(QueryErrorResponse {
                error: format!("telemetry overview failed: {e}"),
            }),
        )
    })?;

    let total_executions: usize = stats.iter().map(|s| s.total_executions).sum();

    // Weighted average error rate: sum(errors) / sum(executions)
    let avg_error_rate = if total_executions > 0 {
        let total_errors: usize = stats.iter().map(|s| s.error_count).sum();
        total_errors as f64 / total_executions as f64
    } else {
        0.0
    };

    // Top 5 agents by execution count (already sorted desc by overview())
    let top_agents: Vec<AgentExecSummary> = stats
        .iter()
        .take(5)
        .map(|s| AgentExecSummary {
            name: s.agent_name.clone(),
            executions: s.total_executions,
        })
        .collect();

    // Worst 5 agents by error rate (only include agents with ≥1 execution)
    let mut by_error_rate: Vec<_> = stats
        .iter()
        .filter(|s| s.total_executions > 0)
        .collect();
    by_error_rate.sort_by(|a, b| {
        b.error_rate
            .partial_cmp(&a.error_rate)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let worst_agents: Vec<AgentErrorSummary> = by_error_rate
        .iter()
        .take(5)
        .map(|s| AgentErrorSummary {
            name: s.agent_name.clone(),
            error_rate: s.error_rate,
        })
        .collect();

    Ok(Json(BundeswehrOverview {
        total_agents,
        agents_by_tier,
        total_executions,
        avg_error_rate,
        top_agents,
        worst_agents,
    }))
}
