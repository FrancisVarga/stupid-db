//! Signal composition evaluator with AND/OR/NOT tree support.
//!
//! Evaluates composed detection signals using boolean logic trees.
//! Each leaf node references a detection template result; interior
//! nodes combine children with AND, OR, or NOT operators.
//!
//! Two evaluation modes:
//! - **Template mode**: delegates to [`templates::evaluate_template`] for
//!   built-in spike/drift/absence/threshold evaluators.
//! - **Composition mode**: evaluates a boolean expression tree where leaf
//!   nodes check individual signal scores against thresholds.

use std::collections::HashMap;

use crate::schema::{
    AnomalyRule, Composition, Condition, Filters, LogicalOperator, SignalType,
};
use crate::templates::{evaluate_template, ClusterStats, EntityData, RuleMatch};

// ── Rule evaluator ──────────────────────────────────────────────────

/// Evaluates anomaly rules against entity data.
///
/// Supports two detection modes:
/// - `detection.template` → delegates to built-in template evaluators
/// - `detection.compose`  → evaluates boolean expression trees
///
/// After detection, post-filters are applied to narrow results.
pub struct RuleEvaluator;

impl RuleEvaluator {
    /// Evaluate a single rule against a set of entities.
    ///
    /// Returns all entities that matched the rule's detection logic
    /// and passed its post-detection filters.
    pub fn evaluate(
        rule: &AnomalyRule,
        entities: &HashMap<String, EntityData>,
        cluster_stats: &HashMap<usize, ClusterStats>,
        signal_scores: &HashMap<String, SignalScores>,
    ) -> Result<Vec<RuleMatch>, String> {
        if !rule.metadata.enabled {
            return Ok(Vec::new());
        }

        let matches = if let Some(template) = &rule.detection.template {
            // Template-based detection
            let params = rule
                .detection
                .params
                .as_ref()
                .ok_or("Template detection requires `params`")?;
            evaluate_template(template, params, entities, cluster_stats)?
        } else if let Some(composition) = &rule.detection.compose {
            // Composition-based detection
            evaluate_composition(composition, entities, signal_scores)
        } else {
            return Err("Rule must have either `template` or `compose` in detection".to_string());
        };

        // Apply post-detection filters
        let filtered = apply_filters(matches, &rule.filters, entities);

        Ok(filtered)
    }
}

// ── Per-entity signal scores ────────────────────────────────────────

/// Pre-computed signal scores for a single entity, keyed by signal name.
///
/// These come from the compute pipeline's `AnomalyResult.signals` field.
/// The evaluator looks up scores by signal type name.
#[derive(Debug, Clone, Default)]
pub struct SignalScores {
    /// Signal name → raw score (e.g., "z_score" → 3.5).
    pub scores: HashMap<String, f64>,
}

impl SignalScores {
    /// Look up a signal score by type.
    pub fn get(&self, signal: &SignalType) -> Option<f64> {
        let key = signal_type_key(signal);
        self.scores.get(key).copied()
    }
}

/// Map a SignalType enum variant to its lookup key in the scores map.
fn signal_type_key(signal: &SignalType) -> &'static str {
    match signal {
        SignalType::ZScore => "z_score",
        SignalType::DbscanNoise => "dbscan_noise",
        SignalType::BehavioralDeviation => "behavioral_deviation",
        SignalType::GraphAnomaly => "graph_anomaly",
    }
}

// ── Composition evaluation ──────────────────────────────────────────

/// Evaluate a composition tree against all entities, returning matches.
///
/// For each entity, the composition tree is evaluated recursively.
/// Entities that satisfy the top-level composition are returned as matches.
fn evaluate_composition(
    composition: &Composition,
    entities: &HashMap<String, EntityData>,
    signal_scores: &HashMap<String, SignalScores>,
) -> Vec<RuleMatch> {
    let mut matches = Vec::new();

    for (entity_id, data) in entities {
        let scores = signal_scores.get(entity_id);
        let empty = SignalScores::default();
        let scores = scores.unwrap_or(&empty);

        if evaluate_node(composition, scores) {
            // Collect which signals contributed
            let signals = collect_matching_signals(composition, scores);

            matches.push(RuleMatch {
                entity_id: entity_id.clone(),
                entity_key: data.key.clone(),
                entity_type: data.entity_type.clone(),
                score: data.score,
                signals,
                matched_reason: format_composition_reason(composition, scores),
            });
        }
    }

    matches
}

/// Recursively evaluate a composition node against an entity's signal scores.
fn evaluate_node(composition: &Composition, scores: &SignalScores) -> bool {
    match composition.operator {
        LogicalOperator::And => composition
            .conditions
            .iter()
            .all(|c| evaluate_condition(c, scores)),
        LogicalOperator::Or => composition
            .conditions
            .iter()
            .any(|c| evaluate_condition(c, scores)),
        LogicalOperator::Not => {
            // NOT applies to exactly one condition
            composition
                .conditions
                .first()
                .map(|c| !evaluate_condition(c, scores))
                .unwrap_or(true)
        }
    }
}

/// Evaluate a single condition (leaf signal or nested composition).
fn evaluate_condition(condition: &Condition, scores: &SignalScores) -> bool {
    match condition {
        Condition::Signal {
            signal,
            feature: _,
            threshold,
        } => {
            // Check if signal score exceeds threshold
            scores
                .get(signal)
                .map(|score| score > *threshold)
                .unwrap_or(false)
        }
        Condition::Nested(composition) => evaluate_node(composition, scores),
    }
}

/// Collect signal names and values that contributed to a match.
fn collect_matching_signals(
    composition: &Composition,
    scores: &SignalScores,
) -> Vec<(String, f64)> {
    let mut signals = Vec::new();
    for condition in &composition.conditions {
        match condition {
            Condition::Signal {
                signal,
                feature,
                threshold: _,
            } => {
                if let Some(score) = scores.get(signal) {
                    let name = match feature {
                        Some(f) => format!("{}:{}", signal_type_key(signal), f),
                        None => signal_type_key(signal).to_string(),
                    };
                    signals.push((name, score));
                }
            }
            Condition::Nested(inner) => {
                signals.extend(collect_matching_signals(inner, scores));
            }
        }
    }
    signals
}

/// Format a human-readable explanation of which conditions matched.
fn format_composition_reason(composition: &Composition, scores: &SignalScores) -> String {
    let op_str = match composition.operator {
        LogicalOperator::And => "AND",
        LogicalOperator::Or => "OR",
        LogicalOperator::Not => "NOT",
    };

    let parts: Vec<String> = composition
        .conditions
        .iter()
        .map(|c| match c {
            Condition::Signal {
                signal,
                feature: _,
                threshold,
            } => {
                let score = scores.get(signal).unwrap_or(0.0);
                format!("{}={:.3} (>{:.3})", signal_type_key(signal), score, threshold)
            }
            Condition::Nested(inner) => {
                format!("({})", format_composition_reason(inner, scores))
            }
        })
        .collect();

    format!("Composition {}: {}", op_str, parts.join(", "))
}

// ── Post-detection filters ──────────────────────────────────────────

/// Apply post-detection filters to narrow which entities actually trigger.
fn apply_filters(
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

    fn make_signal_scores(scores: &[(&str, f64)]) -> SignalScores {
        SignalScores {
            scores: scores
                .iter()
                .map(|(k, v)| (k.to_string(), *v))
                .collect(),
        }
    }

    // ── Composition tests ───────────────────────────────────────────

    #[test]
    fn and_composition_all_pass() {
        let comp = Composition {
            operator: LogicalOperator::And,
            conditions: vec![
                Condition::Signal {
                    signal: SignalType::ZScore,
                    feature: None,
                    threshold: 2.0,
                },
                Condition::Signal {
                    signal: SignalType::DbscanNoise,
                    feature: None,
                    threshold: 0.5,
                },
            ],
        };

        let scores = make_signal_scores(&[("z_score", 3.0), ("dbscan_noise", 0.8)]);
        assert!(evaluate_node(&comp, &scores));
    }

    #[test]
    fn and_composition_one_fails() {
        let comp = Composition {
            operator: LogicalOperator::And,
            conditions: vec![
                Condition::Signal {
                    signal: SignalType::ZScore,
                    feature: None,
                    threshold: 2.0,
                },
                Condition::Signal {
                    signal: SignalType::DbscanNoise,
                    feature: None,
                    threshold: 0.9, // Higher than the score
                },
            ],
        };

        let scores = make_signal_scores(&[("z_score", 3.0), ("dbscan_noise", 0.8)]);
        assert!(!evaluate_node(&comp, &scores));
    }

    #[test]
    fn or_composition_one_passes() {
        let comp = Composition {
            operator: LogicalOperator::Or,
            conditions: vec![
                Condition::Signal {
                    signal: SignalType::ZScore,
                    feature: None,
                    threshold: 5.0, // Too high
                },
                Condition::Signal {
                    signal: SignalType::DbscanNoise,
                    feature: None,
                    threshold: 0.5, // Passes
                },
            ],
        };

        let scores = make_signal_scores(&[("z_score", 3.0), ("dbscan_noise", 0.8)]);
        assert!(evaluate_node(&comp, &scores));
    }

    #[test]
    fn or_composition_all_fail() {
        let comp = Composition {
            operator: LogicalOperator::Or,
            conditions: vec![
                Condition::Signal {
                    signal: SignalType::ZScore,
                    feature: None,
                    threshold: 5.0,
                },
                Condition::Signal {
                    signal: SignalType::DbscanNoise,
                    feature: None,
                    threshold: 0.9,
                },
            ],
        };

        let scores = make_signal_scores(&[("z_score", 3.0), ("dbscan_noise", 0.8)]);
        assert!(!evaluate_node(&comp, &scores));
    }

    #[test]
    fn not_composition() {
        let comp = Composition {
            operator: LogicalOperator::Not,
            conditions: vec![Condition::Signal {
                signal: SignalType::GraphAnomaly,
                feature: None,
                threshold: 0.5,
            }],
        };

        // Score 0.3 does NOT exceed 0.5, so NOT(false) = true
        let scores = make_signal_scores(&[("graph_anomaly", 0.3)]);
        assert!(evaluate_node(&comp, &scores));

        // Score 0.8 exceeds 0.5, so NOT(true) = false
        let scores_high = make_signal_scores(&[("graph_anomaly", 0.8)]);
        assert!(!evaluate_node(&comp, &scores_high));
    }

    #[test]
    fn nested_composition() {
        // AND(z_score > 2.0, OR(dbscan > 0.5, graph > 0.5))
        let comp = Composition {
            operator: LogicalOperator::And,
            conditions: vec![
                Condition::Signal {
                    signal: SignalType::ZScore,
                    feature: None,
                    threshold: 2.0,
                },
                Condition::Nested(Composition {
                    operator: LogicalOperator::Or,
                    conditions: vec![
                        Condition::Signal {
                            signal: SignalType::DbscanNoise,
                            feature: None,
                            threshold: 0.5,
                        },
                        Condition::Signal {
                            signal: SignalType::GraphAnomaly,
                            feature: None,
                            threshold: 0.5,
                        },
                    ],
                }),
            ],
        };

        // z=3, dbscan=0.3 (fail), graph=0.8 (pass) → AND(true, OR(false, true)) = true
        let scores = make_signal_scores(&[
            ("z_score", 3.0),
            ("dbscan_noise", 0.3),
            ("graph_anomaly", 0.8),
        ]);
        assert!(evaluate_node(&comp, &scores));

        // z=1.5 (fail) → AND(false, ...) = false
        let scores_low_z = make_signal_scores(&[
            ("z_score", 1.5),
            ("dbscan_noise", 0.3),
            ("graph_anomaly", 0.8),
        ]);
        assert!(!evaluate_node(&comp, &scores_low_z));
    }

    #[test]
    fn missing_signal_treated_as_false() {
        let comp = Composition {
            operator: LogicalOperator::And,
            conditions: vec![Condition::Signal {
                signal: SignalType::BehavioralDeviation,
                feature: None,
                threshold: 0.5,
            }],
        };

        // No behavioral_deviation score → false
        let scores = make_signal_scores(&[("z_score", 3.0)]);
        assert!(!evaluate_node(&comp, &scores));
    }

    // ── Full evaluate tests ─────────────────────────────────────────

    #[test]
    fn evaluate_composition_rule() {
        let rule: AnomalyRule = serde_yaml::from_str(
            r#"
apiVersion: v1
kind: AnomalyRule
metadata:
  id: test-compose
  name: Test Compose
  enabled: true
schedule:
  cron: "* * * * *"
detection:
  compose:
    operator: and
    conditions:
      - signal: z_score
        threshold: 2.0
      - signal: dbscan_noise
        threshold: 0.5
"#,
        )
        .unwrap();

        let mut entities = HashMap::new();
        entities.insert("e1".to_string(), make_entity("M001", zero_features()));
        entities.insert("e2".to_string(), make_entity("M002", zero_features()));

        let mut signal_scores = HashMap::new();
        // e1: both signals exceed thresholds
        signal_scores.insert(
            "e1".to_string(),
            make_signal_scores(&[("z_score", 3.0), ("dbscan_noise", 0.8)]),
        );
        // e2: z_score passes but dbscan_noise fails
        signal_scores.insert(
            "e2".to_string(),
            make_signal_scores(&[("z_score", 3.0), ("dbscan_noise", 0.3)]),
        );

        let results =
            RuleEvaluator::evaluate(&rule, &entities, &HashMap::new(), &signal_scores).unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].entity_key, "M001");
    }

    #[test]
    fn evaluate_disabled_rule_returns_empty() {
        let rule: AnomalyRule = serde_yaml::from_str(
            r#"
apiVersion: v1
kind: AnomalyRule
metadata:
  id: disabled
  name: Disabled Rule
  enabled: false
schedule:
  cron: "* * * * *"
detection:
  compose:
    operator: and
    conditions:
      - signal: z_score
        threshold: 0.0
"#,
        )
        .unwrap();

        let mut entities = HashMap::new();
        entities.insert("e1".to_string(), make_entity("M001", zero_features()));

        let mut signal_scores = HashMap::new();
        signal_scores.insert(
            "e1".to_string(),
            make_signal_scores(&[("z_score", 5.0)]),
        );

        let results =
            RuleEvaluator::evaluate(&rule, &entities, &HashMap::new(), &signal_scores).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn evaluate_template_rule() {
        let rule: AnomalyRule = serde_yaml::from_str(
            r#"
apiVersion: v1
kind: AnomalyRule
metadata:
  id: threshold-test
  name: Threshold Test
  enabled: true
schedule:
  cron: "* * * * *"
detection:
  template: threshold
  params:
    feature: login_count
    operator: gte
    value: 50.0
"#,
        )
        .unwrap();

        let mut entities = HashMap::new();
        let mut feat = zero_features();
        feat[0] = 60.0; // login_count
        feat[1] = 10.0;
        entities.insert("e1".to_string(), make_entity("M001", feat));

        let mut feat2 = zero_features();
        feat2[0] = 10.0; // too low
        feat2[1] = 5.0;
        entities.insert("e2".to_string(), make_entity("M002", feat2));

        let results =
            RuleEvaluator::evaluate(&rule, &entities, &HashMap::new(), &HashMap::new()).unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].entity_key, "M001");
    }

    // ── Filter tests ────────────────────────────────────────────────

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

        let results =
            RuleEvaluator::evaluate(&rule, &entities, &HashMap::new(), &HashMap::new()).unwrap();
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

    #[test]
    fn signal_scores_lookup() {
        let scores = make_signal_scores(&[
            ("z_score", 3.5),
            ("dbscan_noise", 0.7),
            ("behavioral_deviation", 0.9),
            ("graph_anomaly", 0.6),
        ]);

        assert_eq!(scores.get(&SignalType::ZScore), Some(3.5));
        assert_eq!(scores.get(&SignalType::DbscanNoise), Some(0.7));
        assert_eq!(scores.get(&SignalType::BehavioralDeviation), Some(0.9));
        assert_eq!(scores.get(&SignalType::GraphAnomaly), Some(0.6));
    }

    #[test]
    fn no_detection_returns_error() {
        let rule: AnomalyRule = serde_yaml::from_str(
            r#"
apiVersion: v1
kind: AnomalyRule
metadata:
  id: empty
  name: No Detection
  enabled: true
schedule:
  cron: "* * * * *"
detection: {}
"#,
        )
        .unwrap();

        let result =
            RuleEvaluator::evaluate(&rule, &HashMap::new(), &HashMap::new(), &HashMap::new());
        assert!(result.is_err());
    }

    #[test]
    fn collect_signals_from_nested_composition() {
        let comp = Composition {
            operator: LogicalOperator::And,
            conditions: vec![
                Condition::Signal {
                    signal: SignalType::ZScore,
                    feature: Some("login_count".to_string()),
                    threshold: 2.0,
                },
                Condition::Nested(Composition {
                    operator: LogicalOperator::Or,
                    conditions: vec![Condition::Signal {
                        signal: SignalType::GraphAnomaly,
                        feature: None,
                        threshold: 0.5,
                    }],
                }),
            ],
        };

        let scores = make_signal_scores(&[("z_score", 3.0), ("graph_anomaly", 0.8)]);
        let signals = collect_matching_signals(&comp, &scores);

        assert_eq!(signals.len(), 2);
        assert_eq!(signals[0].0, "z_score:login_count");
        assert_eq!(signals[0].1, 3.0);
        assert_eq!(signals[1].0, "graph_anomaly");
        assert_eq!(signals[1].1, 0.8);
    }
}
