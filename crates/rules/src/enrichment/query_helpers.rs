//! Template resolution and hit-bound evaluation helpers.
//!
//! Lightweight string replacement for OpenSearch query JSON and
//! utility to check whether hit counts fall within configured bounds.

use crate::templates::RuleMatch;

/// Simple template variable resolution in OpenSearch query JSON.
///
/// Replaces `{{ anomaly.key }}` -> actual entity key, and similar patterns.
/// This is a lightweight string replacement, not full minijinja rendering.
pub(crate) fn resolve_query_templates(
    query: &serde_json::Value,
    rule_match: &RuleMatch,
) -> serde_json::Value {
    let json_str = serde_json::to_string(query).unwrap_or_default();

    let resolved = json_str
        .replace("{{ anomaly.key }}", &rule_match.entity_key)
        .replace("{{anomaly.key}}", &rule_match.entity_key)
        .replace("{{ anomaly.entity_type }}", &rule_match.entity_type)
        .replace("{{anomaly.entity_type}}", &rule_match.entity_type);

    serde_json::from_str(&resolved).unwrap_or_else(|_| query.clone())
}

/// Evaluate whether the hit count falls within the configured bounds.
pub(crate) fn evaluate_hit_bounds(hit_count: u64, min_hits: Option<u64>, max_hits: Option<u64>) -> bool {
    match (min_hits, max_hits) {
        (Some(min), Some(max)) => hit_count >= min && hit_count <= max,
        (Some(min), None) => hit_count >= min,
        (None, Some(max)) => hit_count <= max,
        (None, None) => hit_count > 0,
    }
}
