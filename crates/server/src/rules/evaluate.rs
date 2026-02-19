//! Validation and dry-run evaluation endpoints for rules.

use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;

use stupid_rules::schema::{RuleDocument, RuleEnvelope};

use crate::anomaly_rules::MatchSummary;
use crate::state::AppState;

use super::types::{DryRunResult, ValidateError, ValidateSuccess};

/// Validate a raw YAML rule without saving it.
///
/// Runs two-pass deserialization (RuleEnvelope -> RuleDocument) and returns
/// whether the YAML is a valid rule definition. Supports all 6 rule kinds.
#[utoipa::path(
    post,
    path = "/rules/validate",
    tag = "Rules",
    request_body(content = String, content_type = "application/yaml", description = "Rule YAML to validate"),
    responses(
        (status = 200, description = "YAML is valid", body = ValidateSuccess),
        (status = 400, description = "Validation failed", body = ValidateError)
    )
)]
pub(crate) async fn validate_rule(
    body: String,
) -> Result<Json<ValidateSuccess>, (StatusCode, Json<ValidateError>)> {
    // First pass: parse the envelope header.
    let envelope: RuleEnvelope = serde_yaml::from_str(&body).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(ValidateError {
                valid: false,
                errors: vec![format!("Invalid YAML: {}", e)],
            }),
        )
    })?;

    // Second pass: full type-specific deserialization.
    let doc = envelope.parse_full().map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(ValidateError {
                valid: false,
                errors: vec![format!("Failed to parse rule: {}", e)],
            }),
        )
    })?;

    let meta = doc.metadata();
    if meta.id.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ValidateError {
                valid: false,
                errors: vec!["Rule metadata.id must not be empty".to_string()],
            }),
        ));
    }

    Ok(Json(ValidateSuccess {
        valid: true,
        kind: doc.kind(),
        id: meta.id.clone(),
        name: meta.name.clone(),
    }))
}

/// Dry-run a rule against live data without saving it.
///
/// Accepts raw YAML, validates it, and for AnomalyRule kind evaluates against
/// the current entity data and signal scores. Returns matches found and
/// evaluation timing. The rule is never persisted to disk.
#[utoipa::path(
    post,
    path = "/rules/dry-run",
    tag = "Rules",
    request_body(content = String, content_type = "application/yaml", description = "Rule YAML to dry-run"),
    responses(
        (status = 200, description = "Dry-run result", body = DryRunResult),
        (status = 400, description = "Validation failed", body = ValidateError)
    )
)]
pub(crate) async fn dry_run_rule(
    State(state): State<Arc<AppState>>,
    body: String,
) -> Result<Json<DryRunResult>, (StatusCode, Json<ValidateError>)> {
    // Two-pass parse: envelope -> full document.
    let envelope: RuleEnvelope = serde_yaml::from_str(&body).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(ValidateError {
                valid: false,
                errors: vec![format!("Invalid YAML: {}", e)],
            }),
        )
    })?;

    let doc = envelope.parse_full().map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(ValidateError {
                valid: false,
                errors: vec![format!("Failed to parse rule: {}", e)],
            }),
        )
    })?;

    let meta = doc.metadata();
    if meta.id.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ValidateError {
                valid: false,
                errors: vec!["Rule metadata.id must not be empty".to_string()],
            }),
        ));
    }

    let rule_id = meta.id.clone();
    let kind = doc.kind();

    // Only AnomalyRule supports evaluation; other kinds get a validation-only result.
    let rule = match doc {
        RuleDocument::Anomaly(rule) => rule,
        _ => {
            return Ok(Json(DryRunResult {
                rule_id,
                kind,
                matches_found: 0,
                evaluation_ms: 0,
                message: format!(
                    "YAML is valid (kind: {}). Dry-run evaluation is only supported for AnomalyRule.",
                    kind
                ),
                matches: vec![],
            }));
        }
    };

    // Evaluate against live data.
    let start = std::time::Instant::now();
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
                return Ok(Json(DryRunResult {
                    rule_id,
                    kind,
                    matches_found: 0,
                    evaluation_ms,
                    message: format!("Evaluation error: {}", e),
                    matches: vec![],
                }));
            }
        };

    let evaluation_ms = start.elapsed().as_millis() as u64;

    Ok(Json(DryRunResult {
        rule_id,
        kind,
        matches_found,
        evaluation_ms,
        message: format!("{} entities matched", matches_found),
        matches: match_summaries,
    }))
}
