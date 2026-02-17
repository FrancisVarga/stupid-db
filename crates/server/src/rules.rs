//! Generic rule CRUD API endpoints for all rule kinds.
//!
//! Provides REST endpoints for managing any [`RuleDocument`] variant stored
//! as YAML files on disk via [`stupid_rules::loader::RuleLoader`].
//! Complements the anomaly-specific lifecycle endpoints in [`crate::anomaly_rules`].

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::{header, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use tracing::warn;

use stupid_rules::schema::{RuleEnvelope, RuleKind};

use crate::anomaly_rules::MatchSummary;
use crate::state::AppState;

// ── Types ────────────────────────────────────────────────────────────

/// A recent trigger entry enriched with rule metadata for the dashboard feed.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct RecentTrigger {
    pub rule_id: String,
    pub rule_name: String,
    #[schema(value_type = String)]
    pub kind: RuleKind,
    pub timestamp: String,
    pub matches_found: usize,
    pub evaluation_ms: u64,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub matches: Vec<MatchSummary>,
}

/// Query parameters for GET /rules/recent-triggers.
#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct RecentTriggersParams {
    pub limit: Option<u32>,
}

/// Lightweight summary returned by GET /rules.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct GenericRuleSummary {
    pub id: String,
    pub name: String,
    #[schema(value_type = String)]
    pub kind: RuleKind,
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
}

/// Query parameters for GET /rules.
#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct RulesQueryParams {
    pub kind: Option<String>,
}

// ── Endpoints ────────────────────────────────────────────────────────

/// List all rule documents as lightweight summaries.
#[utoipa::path(
    get,
    path = "/rules",
    tag = "Rules",
    params(RulesQueryParams),
    responses(
        (status = 200, description = "List of rule summaries", body = Vec<GenericRuleSummary>)
    )
)]
pub(crate) async fn list_rules(
    State(state): State<Arc<AppState>>,
    Query(params): Query<RulesQueryParams>,
) -> Json<Vec<GenericRuleSummary>> {
    let docs = state.rule_loader.documents();
    let guard = docs.read().expect("documents lock poisoned");

    let kind_filter: Option<RuleKind> = params
        .kind
        .as_deref()
        .and_then(|k| k.parse().ok());

    let mut summaries: Vec<GenericRuleSummary> = guard
        .values()
        .filter(|doc| match kind_filter {
            Some(k) => doc.kind() == k,
            None => true,
        })
        .map(|doc| {
            let meta = doc.metadata();
            GenericRuleSummary {
                id: meta.id.clone(),
                name: meta.name.clone(),
                kind: doc.kind(),
                enabled: meta.enabled,
                description: meta.description.clone(),
                tags: meta.tags.clone(),
            }
        })
        .collect();

    summaries.sort_by(|a, b| a.id.cmp(&b.id));
    Json(summaries)
}

/// Get a single rule document as JSON.
#[utoipa::path(
    get,
    path = "/rules/{id}",
    tag = "Rules",
    params(
        ("id" = String, Path, description = "Rule ID")
    ),
    responses(
        (status = 200, description = "Rule document", body = Object),
        (status = 404, description = "Rule not found")
    )
)]
pub(crate) async fn get_rule(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, StatusCode> {
    let docs = state.rule_loader.documents();
    let guard = docs.read().expect("documents lock poisoned");
    let doc = guard.get(&id).ok_or(StatusCode::NOT_FOUND)?;
    let json = doc.to_json().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(json))
}

/// Get raw YAML for a rule (reads from disk for round-trip fidelity).
#[utoipa::path(
    get,
    path = "/rules/{id}/yaml",
    tag = "Rules",
    params(
        ("id" = String, Path, description = "Rule ID")
    ),
    responses(
        (status = 200, description = "Rule YAML source", content_type = "text/plain", body = String),
        (status = 404, description = "Rule not found")
    )
)]
pub(crate) async fn get_rule_yaml(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, StatusCode> {
    // Verify rule exists in memory.
    {
        let docs = state.rule_loader.documents();
        let guard = docs.read().expect("documents lock poisoned");
        if !guard.contains_key(&id) {
            return Err(StatusCode::NOT_FOUND);
        }
    }

    // Read from disk for round-trip fidelity (preserves comments, formatting).
    let rules_dir = state.rule_loader.rules_dir();
    let yml_path = rules_dir.join(format!("{}.yml", id));
    let yaml_path = rules_dir.join(format!("{}.yaml", id));

    let content = if yml_path.exists() {
        std::fs::read_to_string(&yml_path).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    } else if yaml_path.exists() {
        std::fs::read_to_string(&yaml_path).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    } else {
        // Fall back to serialization if file not found on expected path.
        let docs = state.rule_loader.documents();
        let guard = docs.read().expect("documents lock poisoned");
        guard
            .get(&id)
            .ok_or(StatusCode::NOT_FOUND)?
            .to_yaml()
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    };

    Ok(([(header::CONTENT_TYPE, "text/plain; charset=utf-8")], content))
}

/// Create a new rule from a YAML body. Supports all 6 rule kinds.
#[utoipa::path(
    post,
    path = "/rules",
    tag = "Rules",
    request_body(content = String, content_type = "application/yaml", description = "Rule definition in YAML format"),
    responses(
        (status = 201, description = "Rule created", body = Object),
        (status = 400, description = "Invalid YAML", body = String),
        (status = 409, description = "Rule already exists", body = String)
    )
)]
pub(crate) async fn create_rule(
    State(state): State<Arc<AppState>>,
    body: String,
) -> Result<(StatusCode, impl IntoResponse), (StatusCode, String)> {
    // Two-pass parse: envelope -> full document.
    let envelope: RuleEnvelope = serde_yaml::from_str(&body).map_err(|e| {
        (StatusCode::BAD_REQUEST, format!("Invalid YAML: {}", e))
    })?;

    let doc = envelope.parse_full().map_err(|e| {
        (StatusCode::BAD_REQUEST, format!("Failed to parse rule: {}", e))
    })?;

    let meta = doc.metadata();
    if meta.id.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "Rule metadata.id must not be empty".to_string()));
    }

    // Check uniqueness.
    {
        let docs = state.rule_loader.documents();
        let guard = docs.read().expect("documents lock poisoned");
        if guard.contains_key(&meta.id) {
            return Err((
                StatusCode::CONFLICT,
                format!("Rule with id '{}' already exists", meta.id),
            ));
        }
    }

    state.rule_loader.write_document(&doc).map_err(|e| {
        warn!(error = %e, "Failed to write rule document");
        (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to write rule: {}", e))
    })?;

    let json = doc.to_json().map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to serialize: {}", e))
    })?;

    Ok((StatusCode::CREATED, Json(json)))
}

/// Update an existing rule by ID from a YAML body.
#[utoipa::path(
    put,
    path = "/rules/{id}",
    tag = "Rules",
    params(
        ("id" = String, Path, description = "Rule ID")
    ),
    request_body(content = String, content_type = "application/yaml", description = "Updated rule definition in YAML format"),
    responses(
        (status = 200, description = "Rule updated", body = Object),
        (status = 400, description = "Invalid YAML", body = String),
        (status = 404, description = "Rule not found", body = String)
    )
)]
pub(crate) async fn update_rule(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    body: String,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let envelope: RuleEnvelope = serde_yaml::from_str(&body).map_err(|e| {
        (StatusCode::BAD_REQUEST, format!("Invalid YAML: {}", e))
    })?;

    let mut doc = envelope.parse_full().map_err(|e| {
        (StatusCode::BAD_REQUEST, format!("Failed to parse rule: {}", e))
    })?;

    // Ensure the rule ID matches the path parameter.
    doc.metadata_mut().id = id.clone();

    // Verify the rule exists before overwriting.
    {
        let docs = state.rule_loader.documents();
        let guard = docs.read().expect("documents lock poisoned");
        if !guard.contains_key(&id) {
            return Err((StatusCode::NOT_FOUND, format!("Rule '{}' not found", id)));
        }
    }

    state.rule_loader.write_document(&doc).map_err(|e| {
        warn!(error = %e, "Failed to update rule document");
        (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to update rule: {}", e))
    })?;

    let json = doc.to_json().map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to serialize: {}", e))
    })?;

    Ok(Json(json))
}

/// Delete a rule by ID. Works for any rule kind.
#[utoipa::path(
    delete,
    path = "/rules/{id}",
    tag = "Rules",
    params(
        ("id" = String, Path, description = "Rule ID")
    ),
    responses(
        (status = 204, description = "Rule deleted"),
        (status = 404, description = "Rule not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub(crate) async fn delete_rule(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> StatusCode {
    match state.rule_loader.delete_rule(&id) {
        Ok(()) => StatusCode::NO_CONTENT,
        Err(e) => {
            if matches!(e, stupid_rules::loader::RuleError::Validation(_)) {
                StatusCode::NOT_FOUND
            } else {
                warn!(error = %e, "Failed to delete rule");
                StatusCode::INTERNAL_SERVER_ERROR
            }
        }
    }
}

/// Toggle a rule's `metadata.enabled` flag. Works for any rule kind.
#[utoipa::path(
    post,
    path = "/rules/{id}/toggle",
    tag = "Rules",
    params(
        ("id" = String, Path, description = "Rule ID")
    ),
    responses(
        (status = 200, description = "Rule toggled", body = Object),
        (status = 404, description = "Rule not found", body = String)
    )
)]
pub(crate) async fn toggle_rule(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let docs = state.rule_loader.documents();
    let mut doc = {
        let guard = docs.read().expect("documents lock poisoned");
        guard
            .get(&id)
            .cloned()
            .ok_or_else(|| (StatusCode::NOT_FOUND, format!("Rule '{}' not found", id)))?
    };

    let meta = doc.metadata_mut();
    meta.enabled = !meta.enabled;

    state.rule_loader.write_document(&doc).map_err(|e| {
        warn!(error = %e, "Failed to toggle rule enabled state");
        (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to persist rule: {}", e))
    })?;

    let json = doc.to_json().map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to serialize: {}", e))
    })?;

    Ok(Json(json))
}

/// Get recent trigger events across all rules, sorted by timestamp descending.
///
/// Merges all per-rule trigger histories and enriches each entry with rule
/// metadata (name, kind) for display in the dashboard feed.
#[utoipa::path(
    get,
    path = "/rules/recent-triggers",
    tag = "Rules",
    params(RecentTriggersParams),
    responses(
        (status = 200, description = "Recent trigger events", body = Vec<RecentTrigger>)
    )
)]
pub(crate) async fn recent_triggers(
    State(state): State<Arc<AppState>>,
    Query(params): Query<RecentTriggersParams>,
) -> Json<Vec<RecentTrigger>> {
    let limit = params.limit.unwrap_or(50).min(200) as usize;

    // Build a name+kind lookup from documents.
    let docs = state.rule_loader.documents();
    let docs_guard = docs.read().expect("documents lock poisoned");
    let meta_map: std::collections::HashMap<&str, (&str, RuleKind)> = docs_guard
        .iter()
        .map(|(id, doc)| (id.as_str(), (doc.metadata().name.as_str(), doc.kind())))
        .collect();

    // Merge all trigger histories.
    let history = state.trigger_history.read().expect("trigger_history lock");
    let mut all: Vec<RecentTrigger> = history
        .iter()
        .flat_map(|(rule_id, deque)| {
            let (name, kind) = meta_map
                .get(rule_id.as_str())
                .copied()
                .unwrap_or(("(unknown)", RuleKind::AnomalyRule));
            deque.iter().map(move |entry| RecentTrigger {
                rule_id: rule_id.clone(),
                rule_name: name.to_string(),
                kind,
                timestamp: entry.timestamp.clone(),
                matches_found: entry.matches_found,
                evaluation_ms: entry.evaluation_ms,
                matches: entry.matches.clone(),
            })
        })
        .collect();

    // Sort by timestamp descending (RFC-3339 is lexicographically sortable).
    all.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    all.truncate(limit);

    Json(all)
}

/// Build the generic rules sub-router.
pub fn rules_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/rules", get(list_rules).post(create_rule))
        .route("/rules/recent-triggers", get(recent_triggers))
        .route("/rules/{id}", get(get_rule).put(update_rule).delete(delete_rule))
        .route("/rules/{id}/yaml", get(get_rule_yaml))
        .route("/rules/{id}/toggle", post(toggle_rule))
}
