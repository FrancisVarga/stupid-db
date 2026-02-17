//! Post-detection filters for narrowing rule matches.
//!
//! After detection (template or composition), filters remove entities
//! that don't meet additional criteria: entity type, minimum score,
//! excluded keys, or feature-level where conditions.

use std::collections::HashMap;

use crate::schema::Filters;
use crate::templates::{EntityData, RuleMatch};

// ── Post-detection filters ──────────────────────────────────────────

/// Apply post-detection filters to narrow which entities actually trigger.
pub(super) fn apply_filters(
    matches: Vec<RuleMatch>,
    filters: &Option<Filters>,
    entities: &HashMap<String, EntityData>,
) -> Vec<RuleMatch> {
    let filters = match filters {
        Some(f) => f,
        None => return matches,
    };

    matches
        .into_iter()
        .filter(|m| {
            // Filter by entity type
            if let Some(types) = &filters.entity_types {
                if !types.contains(&m.entity_type) {
                    return false;
                }
            }

            // Filter by classification (not available in RuleMatch directly,
            // so we skip this filter for now — it would need AnomalyResult data)
            // TODO: Add classification to RuleMatch or filter input

            // Filter by minimum score
            if let Some(min) = filters.min_score {
                if m.score < min {
                    return false;
                }
            }

            // Filter by excluded keys
            if let Some(excluded) = &filters.exclude_keys {
                if excluded.contains(&m.entity_key) {
                    return false;
                }
            }

            // Filter by feature conditions (where clause)
            if let Some(conditions) = &filters.conditions {
                if let Some(data) = entities.get(&m.entity_id) {
                    for (feature_name, condition) in conditions {
                        let idx = crate::templates::feature_index(feature_name);
                        let value = idx.and_then(|i| data.features.get(i).copied());
                        match value {
                            Some(v) => {
                                if !condition.matches(v) {
                                    return false;
                                }
                            }
                            None => return false, // Unknown feature → exclude
                        }
                    }
                }
            }

            true
        })
        .collect()
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::*;
    use crate::templates::{EntityData, FEATURE_COUNT};

    fn zero_features() -> Vec<f64> {
        vec![0.0; FEATURE_COUNT]
    }

    fn make_entity(key: &str, features: Vec<f64>) -> EntityData {
        EntityData {
            key: key.to_string(),
            entity_type: "Member".to_string(),
            features,
            score: 0.7,
            cluster_id: None,
        }
    }

    #[test]
    fn filter_by_entity_type() {
        let rule: AnomalyRule = serde_yaml::from_str(
            r#"
apiVersion: v1
kind: AnomalyRule
metadata:
  id: filtered
  name: Filtered Rule
  enabled: true
schedule:
  cron: "* * * * *"
detection:
  template: threshold
  params:
    feature: login_count
    operator: gte
    value: 10.0
filters:
  entity_types: [Device]
"#,
        )
        .unwrap();

        let mut entities = HashMap::new();
        let mut feat = zero_features();
        feat[0] = 50.0;
        feat[1] = 10.0;
        // EntityData has entity_type "Member" which doesn't match filter
        entities.insert("e1".to_string(), make_entity("M001", feat));

        let results = crate::evaluator::RuleEvaluator::evaluate(
            &rule,
            &entities,
            &HashMap::new(),
            &HashMap::new(),
        )
        .unwrap();
        assert!(results.is_empty(), "Member should be filtered out when filter requires Device");
    }

    #[test]
    fn filter_by_min_score() {
        let matches = vec![
            RuleMatch {
                entity_id: "e1".to_string(),
                entity_key: "M001".to_string(),
                entity_type: "Member".to_string(),
                score: 0.8,
                signals: vec![],
                matched_reason: "test".to_string(),
            },
            RuleMatch {
                entity_id: "e2".to_string(),
                entity_key: "M002".to_string(),
                entity_type: "Member".to_string(),
                score: 0.3,
                signals: vec![],
                matched_reason: "test".to_string(),
            },
        ];

        let filters = Some(Filters {
            entity_types: None,
            classifications: None,
            min_score: Some(0.5),
            exclude_keys: None,
            conditions: None,
        });

        let filtered = apply_filters(matches, &filters, &HashMap::new());
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].entity_key, "M001");
    }

    #[test]
    fn filter_by_exclude_keys() {
        let matches = vec![
            RuleMatch {
                entity_id: "e1".to_string(),
                entity_key: "SYSTEM".to_string(),
                entity_type: "Member".to_string(),
                score: 0.9,
                signals: vec![],
                matched_reason: "test".to_string(),
            },
            RuleMatch {
                entity_id: "e2".to_string(),
                entity_key: "M002".to_string(),
                entity_type: "Member".to_string(),
                score: 0.9,
                signals: vec![],
                matched_reason: "test".to_string(),
            },
        ];

        let filters = Some(Filters {
            entity_types: None,
            classifications: None,
            min_score: None,
            exclude_keys: Some(vec!["SYSTEM".to_string()]),
            conditions: None,
        });

        let filtered = apply_filters(matches, &filters, &HashMap::new());
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].entity_key, "M002");
    }

    #[test]
    fn filter_by_where_conditions() {
        let mut entities = HashMap::new();
        let mut feat = zero_features();
        feat[0] = 100.0; // login_count
        feat[1] = 5.0;
        entities.insert("e1".to_string(), make_entity("M001", feat));

        let mut feat2 = zero_features();
        feat2[0] = 5.0; // login_count too low
        feat2[1] = 5.0;
        entities.insert("e2".to_string(), make_entity("M002", feat2));

        let matches = vec![
            RuleMatch {
                entity_id: "e1".to_string(),
                entity_key: "M001".to_string(),
                entity_type: "Member".to_string(),
                score: 0.9,
                signals: vec![],
                matched_reason: "test".to_string(),
            },
            RuleMatch {
                entity_id: "e2".to_string(),
                entity_key: "M002".to_string(),
                entity_type: "Member".to_string(),
                score: 0.9,
                signals: vec![],
                matched_reason: "test".to_string(),
            },
        ];

        let mut conditions = HashMap::new();
        conditions.insert(
            "login_count".to_string(),
            FilterCondition {
                gt: Some(50.0),
                gte: None,
                lt: None,
                lte: None,
                eq: None,
                neq: None,
            },
        );

        let filters = Some(Filters {
            entity_types: None,
            classifications: None,
            min_score: None,
            exclude_keys: None,
            conditions: Some(conditions),
        });

        let filtered = apply_filters(matches, &filters, &entities);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].entity_key, "M001");
    }
}
