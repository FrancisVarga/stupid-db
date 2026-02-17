//! Composition tree evaluation for boolean signal logic.
//!
//! Evaluates AND/OR/NOT expression trees where leaf nodes check
//! individual signal scores against thresholds.

use std::collections::HashMap;

use crate::schema::{Composition, Condition, LogicalOperator};
use crate::templates::{EntityData, RuleMatch};

use super::{signal_type_key, SignalScores};

// ── Composition evaluation ──────────────────────────────────────────

/// Evaluate a composition tree against all entities, returning matches.
///
/// For each entity, the composition tree is evaluated recursively.
/// Entities that satisfy the top-level composition are returned as matches.
pub(crate) fn evaluate_composition(
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
pub(crate) fn collect_matching_signals(
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
pub(crate) fn format_composition_reason(
    composition: &Composition,
    scores: &SignalScores,
) -> String {
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

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::*;

    fn make_signal_scores(scores: &[(&str, f64)]) -> SignalScores {
        SignalScores {
            scores: scores
                .iter()
                .map(|(k, v)| (k.to_string(), *v))
                .collect(),
        }
    }

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

        // z=3, dbscan=0.3 (fail), graph=0.8 (pass) -> AND(true, OR(false, true)) = true
        let scores = make_signal_scores(&[
            ("z_score", 3.0),
            ("dbscan_noise", 0.3),
            ("graph_anomaly", 0.8),
        ]);
        assert!(evaluate_node(&comp, &scores));

        // z=1.5 (fail) -> AND(false, ...) = false
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

        // No behavioral_deviation score -> false
        let scores = make_signal_scores(&[("z_score", 3.0)]);
        assert!(!evaluate_node(&comp, &scores));
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
