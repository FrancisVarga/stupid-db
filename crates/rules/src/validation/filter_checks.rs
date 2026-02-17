//! Filter validation: entity types, classifications, score ranges, and feature references.

use crate::schema::*;
use super::ValidationResult;
use super::fuzzy::fuzzy_match;

// ── Valid domain values ─────────────────────────────────────────────

/// The 10-dimensional feature vector used by the anomaly engine.
const VALID_FEATURES: &[&str] = &[
    "login_count_7d",
    "game_count_7d",
    "unique_games_7d",
    "error_count_7d",
    "popup_interaction_7d",
    "platform_mobile_ratio",
    "session_count_7d",
    "avg_session_gap_hours",
    "vip_group_numeric",
    "currency_encoded",
];

/// Valid signal names for composition conditions.
/// Kept as documentation — signal types are enforced by `SignalType` enum deserialization.
#[allow(dead_code)]
const VALID_SIGNALS: &[&str] = &[
    "z_score",
    "dbscan_noise",
    "behavioral_deviation",
    "graph_anomaly",
];

/// Valid entity types from `stupid-core`.
const VALID_ENTITY_TYPES: &[&str] = &[
    "Member", "Device", "Game", "Affiliate", "Currency", "VipGroup", "Error",
    "Platform", "Popup", "Provider",
];

/// Valid anomaly classifications from the compute engine.
const VALID_CLASSIFICATIONS: &[&str] = &[
    "Normal",
    "Mild",
    "Anomalous",
    "HighlyAnomalous",
];

// ── Filter validation ───────────────────────────────────────────────

pub(super) fn validate_filters(rule: &AnomalyRule, result: &mut ValidationResult) {
    let filters = match &rule.filters {
        Some(f) => f,
        None => return,
    };

    // Validate entity types
    if let Some(types) = &filters.entity_types {
        for (i, t) in types.iter().enumerate() {
            if !VALID_ENTITY_TYPES.contains(&t.as_str()) {
                let suggestion = fuzzy_match(t, VALID_ENTITY_TYPES);
                let path = format!("filters.entity_types[{i}]");
                if let Some(s) = suggestion {
                    result.error_with_suggestion(
                        &path,
                        format!("Unknown entity type '{t}'"),
                        format!("Did you mean '{s}'?"),
                    );
                } else {
                    result.error(&path, format!("Unknown entity type '{t}'"));
                }
            }
        }
    }

    // Validate classifications
    if let Some(classes) = &filters.classifications {
        for (i, c) in classes.iter().enumerate() {
            if !VALID_CLASSIFICATIONS.contains(&c.as_str()) {
                let suggestion = fuzzy_match(c, VALID_CLASSIFICATIONS);
                let path = format!("filters.classifications[{i}]");
                if let Some(s) = suggestion {
                    result.error_with_suggestion(
                        &path,
                        format!("Unknown classification '{c}'"),
                        format!("Did you mean '{s}'?"),
                    );
                } else {
                    result.error(&path, format!("Unknown classification '{c}'"));
                }
            }
        }
    }

    // Validate min_score range
    if let Some(score) = filters.min_score {
        if !(0.0..=1.0).contains(&score) {
            result.error(
                "filters.min_score",
                format!("min_score must be between 0.0 and 1.0, got {score}"),
            );
        }
    }

    // Validate where-clause feature references
    if let Some(conditions) = &filters.conditions {
        for key in conditions.keys() {
            validate_feature_name(key, &format!("filters.where.{key}"), result);
        }
    }
}

/// Validate a feature name against the known 10-dimensional vector.
pub(super) fn validate_feature_name(name: &str, path: &str, result: &mut ValidationResult) {
    if !VALID_FEATURES.contains(&name) {
        let suggestion = fuzzy_match(name, VALID_FEATURES);
        if let Some(s) = suggestion {
            result.error_with_suggestion(
                path,
                format!("Unknown feature '{name}'"),
                format!("Did you mean '{s}'?"),
            );
        } else {
            result.error(path, format!("Unknown feature '{name}'"));
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::validation::validate_rule;
    use crate::schema::*;

    fn valid_rule() -> AnomalyRule {
        serde_yaml::from_str(
            r#"
apiVersion: v1
kind: AnomalyRule
metadata:
  id: test-rule
  name: Test Rule
  enabled: true
schedule:
  cron: "*/15 * * * *"
  timezone: UTC
detection:
  template: spike
  params:
    feature: login_count_7d
    multiplier: 3.0
notifications:
  - channel: webhook
    url: "https://hooks.example.com/alerts"
    on: [trigger]
"#,
        )
        .unwrap()
    }

    #[test]
    fn filter_invalid_entity_type() {
        let mut rule = valid_rule();
        rule.filters = Some(Filters {
            entity_types: Some(vec!["Memer".to_string()]),
            classifications: None,
            min_score: None,
            exclude_keys: None,
            conditions: None,
        });
        let result = validate_rule(&rule);
        assert!(!result.valid);
        let err = result
            .errors
            .iter()
            .find(|e| e.path.contains("entity_types"))
            .unwrap();
        assert!(err.suggestion.as_deref().unwrap().contains("Member"));
    }

    #[test]
    fn filter_min_score_out_of_range() {
        let mut rule = valid_rule();
        rule.filters = Some(Filters {
            entity_types: None,
            classifications: None,
            min_score: Some(1.5),
            exclude_keys: None,
            conditions: None,
        });
        let result = validate_rule(&rule);
        assert!(!result.valid);
        assert!(result
            .errors
            .iter()
            .any(|e| e.path == "filters.min_score"));
    }

    #[test]
    fn filter_where_invalid_feature() {
        let mut rule = valid_rule();
        let mut conds = std::collections::HashMap::new();
        conds.insert(
            "login_freq".to_string(),
            FilterCondition {
                gt: Some(10.0),
                gte: None,
                lt: None,
                lte: None,
                eq: None,
                neq: None,
            },
        );
        rule.filters = Some(Filters {
            entity_types: None,
            classifications: None,
            min_score: None,
            exclude_keys: None,
            conditions: Some(conds),
        });
        let result = validate_rule(&rule);
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.path.contains("filters.where")));
    }
}
