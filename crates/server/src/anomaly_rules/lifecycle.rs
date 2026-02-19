//! Lifecycle endpoints for anomaly rules: start, pause, run, test-notify,
//! history, and audit logs.

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use tracing::warn;

use stupid_rules::audit_log::{LogEntry, LogQueryParams};
use stupid_rules::schema::AnomalyRule;

use crate::state::AppState;

use super::types::{
    HistoryParams, MatchSummary, RunResult, TestNotifyResult, TriggerEntry,
};

/// Start (enable) an anomaly rule.
///
/// Sets `metadata.enabled = true` and persists the change to disk.
#[utoipa::path(
    post,
    path = "/anomaly-rules/{id}/start",
    tag = "Anomaly Rules",
    params(
        ("id" = String, Path, description = "Anomaly rule ID")
    ),
    responses(
        (status = 200, description = "Anomaly rule started", body = Object),
        (status = 404, description = "Rule not found", body = String)
    )
)]
pub(crate) async fn start_anomaly_rule(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<AnomalyRule>, (StatusCode, String)> {
    toggle_rule_enabled(&state, &id, true)
}

/// Pause (disable) an anomaly rule.
///
/// Sets `metadata.enabled = false` and persists the change to disk.
#[utoipa::path(
    post,
    path = "/anomaly-rules/{id}/pause",
    tag = "Anomaly Rules",
    params(
        ("id" = String, Path, description = "Anomaly rule ID")
    ),
    responses(
        (status = 200, description = "Anomaly rule paused", body = Object),
        (status = 404, description = "Rule not found", body = String)
    )
)]
pub(crate) async fn pause_anomaly_rule(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<AnomalyRule>, (StatusCode, String)> {
    toggle_rule_enabled(&state, &id, false)
}

/// Toggle a rule's `metadata.enabled` flag and persist to disk.
fn toggle_rule_enabled(
    state: &AppState,
    id: &str,
    enabled: bool,
) -> Result<Json<AnomalyRule>, (StatusCode, String)> {
    let rules = state.rule_loader.rules();
    let mut rule = {
        let guard = rules.read().expect("rules lock poisoned");
        guard
            .get(id)
            .cloned()
            .ok_or_else(|| (StatusCode::NOT_FOUND, format!("Rule '{}' not found", id)))?
    };

    rule.metadata.enabled = enabled;

    state.rule_loader.write_rule(&rule).map_err(|e| {
        warn!(error = %e, "Failed to update anomaly rule enabled state");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to persist rule: {}", e),
        )
    })?;

    Ok(Json(rule))
}

/// Run a rule immediately against current knowledge state.
///
/// Builds entity data and signal scores from the compute pipeline,
/// evaluates the rule via `RuleEvaluator`, and records the trigger
/// in history.
#[utoipa::path(
    post,
    path = "/anomaly-rules/{id}/run",
    tag = "Anomaly Rules",
    params(
        ("id" = String, Path, description = "Anomaly rule ID")
    ),
    responses(
        (status = 200, description = "Rule evaluation result", body = RunResult),
        (status = 404, description = "Rule not found", body = String)
    )
)]
pub(crate) async fn run_anomaly_rule(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<RunResult>, (StatusCode, String)> {
    let rules = state.rule_loader.rules();
    let guard = rules.read().expect("rules lock poisoned");
    if !guard.contains_key(&id) {
        return Err((StatusCode::NOT_FOUND, format!("Rule '{}' not found", id)));
    }
    drop(guard);

    let start = std::time::Instant::now();

    let rules = state.rule_loader.rules();
    let guard = rules.read().expect("rules lock poisoned");
    let rule = guard.get(&id).cloned().unwrap();
    drop(guard);

    let (entities, cluster_stats, signal_scores) =
        crate::rule_runner::build_evaluation_context(&state);

    let (matches_found, match_summaries) =
        match stupid_rules::evaluator::RuleEvaluator::evaluate(
            &rule,
            &entities,
            &cluster_stats,
            &signal_scores,
        ) {
            Ok(mut matches) => {
                let count = matches.len();
                // Sort by score descending and keep top 50 for history.
                matches.sort_by(|a, b| {
                    b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal)
                });
                let summaries: Vec<MatchSummary> = matches
                    .iter()
                    .take(50)
                    .map(|m| MatchSummary {
                        entity_key: m.entity_key.clone(),
                        entity_type: m.entity_type.clone(),
                        score: m.score,
                        reason: m.matched_reason.clone(),
                    })
                    .collect();
                (count, summaries)
            }
            Err(e) => {
                let evaluation_ms = start.elapsed().as_millis() as u64;
                return Ok(Json(RunResult {
                    rule_id: id,
                    matches_found: 0,
                    evaluation_ms,
                    message: format!("Evaluation error: {}", e),
                }));
            }
        };

    let evaluation_ms = start.elapsed().as_millis() as u64;

    // Record in trigger history.
    {
        let mut history = state.trigger_history.write().expect("trigger_history lock");
        let entry = TriggerEntry {
            timestamp: chrono::Utc::now().to_rfc3339(),
            matches_found,
            evaluation_ms,
            matches: match_summaries,
        };
        let deque = history
            .entry(id.clone())
            .or_insert_with(|| std::collections::VecDeque::with_capacity(500));
        deque.push_back(entry);
        while deque.len() > 500 {
            deque.pop_front();
        }
    }

    Ok(Json(RunResult {
        rule_id: id,
        matches_found,
        evaluation_ms,
        message: format!("{} entities matched", matches_found),
    }))
}

/// Send a test notification through all channels configured on a rule.
///
/// TODO: Full dispatch requires `Dispatcher` in `AppState`, which will be
/// added when the notification subsystem is integrated. This stub returns
/// a synthetic result for each configured channel.
#[utoipa::path(
    post,
    path = "/anomaly-rules/{id}/test-notify",
    tag = "Anomaly Rules",
    params(
        ("id" = String, Path, description = "Anomaly rule ID")
    ),
    responses(
        (status = 200, description = "Test notification results", body = Vec<TestNotifyResult>),
        (status = 404, description = "Rule not found", body = String)
    )
)]
pub(crate) async fn test_notify_rule(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Vec<TestNotifyResult>>, (StatusCode, String)> {
    let rules = state.rule_loader.rules();
    let guard = rules.read().expect("rules lock poisoned");
    let rule = guard
        .get(&id)
        .cloned()
        .ok_or_else(|| (StatusCode::NOT_FOUND, format!("Rule '{}' not found", id)))?;
    drop(guard);

    // TODO: Build a Notification from test data and dispatch via
    // Dispatcher::dispatch(). For now, return a stub result per channel.
    let results: Vec<TestNotifyResult> = rule
        .notifications
        .iter()
        .map(|ch| {
            let channel_name = serde_json::to_value(&ch.channel)
                .ok()
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_else(|| format!("{:?}", ch.channel));

            TestNotifyResult {
                channel: channel_name,
                success: false,
                error: Some("Stub: dispatcher not yet integrated".to_string()),
                response_ms: 0,
            }
        })
        .collect();

    Ok(Json(results))
}

/// Get trigger history for a rule.
///
/// Returns the most recent trigger entries (newest first), limited by
/// the optional `limit` query parameter.
#[utoipa::path(
    get,
    path = "/anomaly-rules/{id}/history",
    tag = "Anomaly Rules",
    params(
        ("id" = String, Path, description = "Anomaly rule ID"),
        HistoryParams
    ),
    responses(
        (status = 200, description = "Trigger history entries", body = Vec<TriggerEntry>),
        (status = 404, description = "Rule not found", body = String)
    )
)]
pub(crate) async fn rule_history(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(params): Query<HistoryParams>,
) -> Result<Json<Vec<TriggerEntry>>, (StatusCode, String)> {
    // Verify rule exists.
    {
        let rules = state.rule_loader.rules();
        let guard = rules.read().expect("rules lock poisoned");
        if !guard.contains_key(&id) {
            return Err((StatusCode::NOT_FOUND, format!("Rule '{}' not found", id)));
        }
    }

    let limit = params.limit.unwrap_or(100) as usize;
    let history = state.trigger_history.read().expect("trigger_history lock poisoned");
    let entries: Vec<TriggerEntry> = history
        .get(&id)
        .map(|deque| deque.iter().rev().take(limit).cloned().collect())
        .unwrap_or_default();

    Ok(Json(entries))
}

/// Get audit log entries for a rule.
///
/// Returns filtered log entries (newest first), controlled by optional
/// `level`, `phase`, `limit`, and `since` query parameters.
#[utoipa::path(
    get,
    path = "/anomaly-rules/{id}/logs",
    tag = "Anomaly Rules",
    params(
        ("id" = String, Path, description = "Anomaly rule ID"),
        ("level" = Option<String>, Query, description = "Minimum log level filter"),
        ("phase" = Option<String>, Query, description = "Execution phase filter"),
        ("limit" = Option<u32>, Query, description = "Maximum number of entries"),
        ("since" = Option<String>, Query, description = "Only entries at or after this ISO 8601 timestamp")
    ),
    responses(
        (status = 200, description = "Audit log entries", body = Vec<Object>),
        (status = 404, description = "Rule not found", body = String)
    )
)]
pub(crate) async fn rule_logs(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(params): Query<LogQueryParams>,
) -> Result<Json<Vec<LogEntry>>, (StatusCode, String)> {
    // Verify rule exists.
    {
        let rules = state.rule_loader.rules();
        let guard = rules.read().expect("rules lock poisoned");
        if !guard.contains_key(&id) {
            return Err((StatusCode::NOT_FOUND, format!("Rule '{}' not found", id)));
        }
    }

    let entries = state.audit_log.query(&id, &params);
    Ok(Json(entries))
}
