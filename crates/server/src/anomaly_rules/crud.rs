//! CRUD endpoints for anomaly rules: list, create, get, update, delete.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use tracing::warn;

use stupid_rules::schema::AnomalyRule;

use crate::state::AppState;

use super::types::RuleSummary;

/// List all anomaly rules as lightweight summaries.
#[utoipa::path(
    get,
    path = "/anomaly-rules",
    tag = "Anomaly Rules",
    responses(
        (status = 200, description = "List of anomaly rule summaries", body = Vec<RuleSummary>)
    )
)]
pub(crate) async fn list_anomaly_rules(
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
#[utoipa::path(
    post,
    path = "/anomaly-rules",
    tag = "Anomaly Rules",
    request_body(content = String, content_type = "application/yaml", description = "Anomaly rule definition in YAML format"),
    responses(
        (status = 201, description = "Anomaly rule created", body = Object),
        (status = 400, description = "Invalid YAML", body = String),
        (status = 409, description = "Rule already exists", body = String)
    )
)]
pub(crate) async fn create_anomaly_rule(
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
#[utoipa::path(
    get,
    path = "/anomaly-rules/{id}",
    tag = "Anomaly Rules",
    params(
        ("id" = String, Path, description = "Anomaly rule ID")
    ),
    responses(
        (status = 200, description = "Anomaly rule details", body = Object),
        (status = 404, description = "Rule not found")
    )
)]
pub(crate) async fn get_anomaly_rule(
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
#[utoipa::path(
    put,
    path = "/anomaly-rules/{id}",
    tag = "Anomaly Rules",
    params(
        ("id" = String, Path, description = "Anomaly rule ID")
    ),
    request_body(content = String, content_type = "application/yaml", description = "Updated anomaly rule definition in YAML format"),
    responses(
        (status = 200, description = "Anomaly rule updated", body = Object),
        (status = 400, description = "Invalid YAML", body = String),
        (status = 404, description = "Rule not found", body = String)
    )
)]
pub(crate) async fn update_anomaly_rule(
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
#[utoipa::path(
    delete,
    path = "/anomaly-rules/{id}",
    tag = "Anomaly Rules",
    params(
        ("id" = String, Path, description = "Anomaly rule ID")
    ),
    responses(
        (status = 204, description = "Anomaly rule deleted"),
        (status = 404, description = "Rule not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub(crate) async fn delete_anomaly_rule(
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
