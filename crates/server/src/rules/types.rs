//! Request/response types for the generic rule CRUD API.

use serde::{Deserialize, Serialize};

use stupid_rules::schema::RuleKind;

use crate::anomaly_rules::MatchSummary;

// ── Response Types ──────────────────────────────────────────────────

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

/// Response for a successful validation.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ValidateSuccess {
    pub valid: bool,
    #[schema(value_type = String)]
    pub kind: RuleKind,
    pub id: String,
    pub name: String,
}

/// Response for a failed validation.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ValidateError {
    pub valid: bool,
    pub errors: Vec<String>,
}

/// Result of a dry-run evaluation.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct DryRunResult {
    pub rule_id: String,
    #[schema(value_type = String)]
    pub kind: RuleKind,
    pub matches_found: usize,
    pub evaluation_ms: u64,
    pub message: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub matches: Vec<MatchSummary>,
}

// ── Query Parameters ────────────────────────────────────────────────

/// Query parameters for GET /rules/recent-triggers.
#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct RecentTriggersParams {
    pub limit: Option<u32>,
}

/// Query parameters for GET /rules.
#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct RulesQueryParams {
    pub kind: Option<String>,
}
