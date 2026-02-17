//! Anomaly rule CRUD and lifecycle API endpoints.
//!
//! Provides REST endpoints for managing anomaly detection rules stored as
//! YAML files on disk via [`stupid_rules::loader::RuleLoader`], plus
//! lifecycle operations (start, pause, run, test-notify, history).

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, RwLock};

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use tracing::warn;

use stupid_rules::audit_log::{LogEntry, LogQueryParams};
use stupid_rules::schema::AnomalyRule;

use crate::state::AppState;

/// Lightweight summary returned by the list endpoint.
#[derive(Debug, Serialize)]
pub struct RuleSummary {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub template: Option<String>,
    pub cron: String,
    pub channel_count: usize,
    /// ISO-8601 timestamp of the most recent trigger, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_triggered: Option<String>,
    /// How many times this rule has been triggered.
    pub trigger_count: usize,
}

impl RuleSummary {
    /// Build a summary from a rule, enriched with trigger history.
    fn from_rule_with_history(rule: &AnomalyRule, history: &HashMap<String, VecDeque<TriggerEntry>>) -> Self {
        let template = rule.detection.template.as_ref().map(|t| {
            serde_json::to_value(t)
                .ok()
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_else(|| format!("{:?}", t).to_lowercase())
        });

        let (last_triggered, trigger_count) = match history.get(&rule.metadata.id) {
            Some(deque) => (
                deque.back().map(|e| e.timestamp.clone()),
                deque.len(),
            ),
            None => (None, 0),
        };

        Self {
            id: rule.metadata.id.clone(),
            name: rule.metadata.name.clone(),
            enabled: rule.metadata.enabled,
            template,
            cron: rule.schedule.cron.clone(),
            channel_count: rule.notifications.len(),
            last_triggered,
            trigger_count,
        }
    }
}

/// List all anomaly rules as lightweight summaries.
async fn list_anomaly_rules(
    State(state): State<Arc<AppState>>,
) -> Json<Vec<RuleSummary>> {
    let rules = state.rule_loader.rules();
    let guard = rules.read().expect("rules lock poisoned");
    let history = state.trigger_history.read().expect("trigger_history lock");
    let mut summaries: Vec<RuleSummary> = guard
        .values()
        .map(|r| RuleSummary::from_rule_with_history(r, &history))
        .collect();
    summaries.sort_by(|a, b| a.id.cmp(&b.id));
    Json(summaries)
}

/// Create a new anomaly rule from a YAML body.
///
/// Returns 201 on success, 400 on parse/validation error, 409 if the rule ID
/// already exists.
async fn create_anomaly_rule(
    State(state): State<Arc<AppState>>,
    body: String,
) -> Result<(StatusCode, Json<AnomalyRule>), (StatusCode, String)> {
    let rule: AnomalyRule = serde_yaml::from_str(&body).map_err(|e| {
        (StatusCode::BAD_REQUEST, format!("Invalid YAML: {}", e))
    })?;

    if rule.metadata.id.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "Rule metadata.id must not be empty".to_string()));
    }

    // Check uniqueness.
    {
        let rules = state.rule_loader.rules();
        let guard = rules.read().expect("rules lock poisoned");
        if guard.contains_key(&rule.metadata.id) {
            return Err((
                StatusCode::CONFLICT,
                format!("Rule with id '{}' already exists", rule.metadata.id),
            ));
        }
    }

    // Persist to disk (also updates the in-memory map).
    state.rule_loader.write_rule(&rule).map_err(|e| {
        warn!(error = %e, "Failed to write anomaly rule");
        (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to write rule: {}", e))
    })?;

    Ok((StatusCode::CREATED, Json(rule)))
}

/// Get a single anomaly rule by ID.
async fn get_anomaly_rule(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<AnomalyRule>, StatusCode> {
    let rules = state.rule_loader.rules();
    let guard = rules.read().expect("rules lock poisoned");
    guard
        .get(&id)
        .cloned()
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

/// Update an existing anomaly rule by ID.
///
/// The rule ID in the path must match the `metadata.id` in the YAML body
/// (or the body ID is overwritten to match the path).
async fn update_anomaly_rule(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    body: String,
) -> Result<Json<AnomalyRule>, (StatusCode, String)> {
    let mut rule: AnomalyRule = serde_yaml::from_str(&body).map_err(|e| {
        (StatusCode::BAD_REQUEST, format!("Invalid YAML: {}", e))
    })?;

    // Ensure the rule ID matches the path parameter.
    rule.metadata.id = id.clone();

    // Verify the rule exists before overwriting.
    {
        let rules = state.rule_loader.rules();
        let guard = rules.read().expect("rules lock poisoned");
        if !guard.contains_key(&id) {
            return Err((StatusCode::NOT_FOUND, format!("Rule '{}' not found", id)));
        }
    }

    state.rule_loader.write_rule(&rule).map_err(|e| {
        warn!(error = %e, "Failed to update anomaly rule");
        (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to update rule: {}", e))
    })?;

    Ok(Json(rule))
}

/// Delete an anomaly rule by ID.
async fn delete_anomaly_rule(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> StatusCode {
    match state.rule_loader.delete_rule(&id) {
        Ok(()) => StatusCode::NO_CONTENT,
        Err(e) => {
            // Validation error means not found; anything else is internal.
            if matches!(e, stupid_rules::loader::RuleError::Validation(_)) {
                StatusCode::NOT_FOUND
            } else {
                warn!(error = %e, "Failed to delete anomaly rule");
                StatusCode::INTERNAL_SERVER_ERROR
            }
        }
    }
}

// ── Lifecycle types ───────────────────────────────────────────────────

/// Result of an immediate rule evaluation via `POST /anomaly-rules/{id}/run`.
#[derive(Debug, Serialize)]
pub struct RunResult {
    pub rule_id: String,
    pub matches_found: usize,
    pub evaluation_ms: u64,
    pub message: String,
}

/// Result of a test notification dispatch via `POST /anomaly-rules/{id}/test-notify`.
#[derive(Debug, Serialize)]
pub struct TestNotifyResult {
    pub channel: String,
    pub success: bool,
    pub error: Option<String>,
    pub response_ms: u64,
}

/// Compact match summary stored in trigger history.
#[derive(Debug, Clone, Serialize)]
pub struct MatchSummary {
    pub entity_key: String,
    pub entity_type: String,
    pub score: f64,
    pub reason: String,
}

/// A single trigger history entry stored per rule.
#[derive(Debug, Clone, Serialize)]
pub struct TriggerEntry {
    pub timestamp: String,
    pub matches_found: usize,
    pub evaluation_ms: u64,
    /// Top matches (capped at 50) sorted by score descending.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub matches: Vec<MatchSummary>,
}

/// Query parameters for the history endpoint.
#[derive(Debug, Deserialize)]
pub struct HistoryParams {
    pub limit: Option<u32>,
}

/// Shared trigger history map: rule_id -> deque of recent trigger entries.
pub type SharedTriggerHistory = Arc<RwLock<HashMap<String, VecDeque<TriggerEntry>>>>;

// ── Lifecycle endpoints ──────────────────────────────────────────────

/// Start (enable) an anomaly rule.
///
/// Sets `metadata.enabled = true` and persists the change to disk.
async fn start_anomaly_rule(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<AnomalyRule>, (StatusCode, String)> {
    toggle_rule_enabled(&state, &id, true)
}

/// Pause (disable) an anomaly rule.
///
/// Sets `metadata.enabled = false` and persists the change to disk.
async fn pause_anomaly_rule(
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
async fn run_anomaly_rule(
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
async fn test_notify_rule(
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
async fn rule_history(
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
async fn rule_logs(
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

/// Build the anomaly rules sub-router.
///
/// Mount this on the main router with `.merge(anomaly_rules_router())` or
/// `.nest("/", anomaly_rules_router())`.
pub fn anomaly_rules_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/anomaly-rules", get(list_anomaly_rules).post(create_anomaly_rule))
        .route(
            "/anomaly-rules/{id}",
            get(get_anomaly_rule)
                .put(update_anomaly_rule)
                .delete(delete_anomaly_rule),
        )
        .route("/anomaly-rules/{id}/start", post(start_anomaly_rule))
        .route("/anomaly-rules/{id}/pause", post(pause_anomaly_rule))
        .route("/anomaly-rules/{id}/run", post(run_anomaly_rule))
        .route("/anomaly-rules/{id}/test-notify", post(test_notify_rule))
        .route("/anomaly-rules/{id}/history", get(rule_history))
        .route("/anomaly-rules/{id}/logs", get(rule_logs))
}
