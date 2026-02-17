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

mod composition;
mod filters;

use std::collections::HashMap;

use crate::schema::{AnomalyRule, SignalType};
use crate::templates::{evaluate_template, ClusterStats, EntityData, RuleMatch};

use composition::evaluate_composition;
use filters::apply_filters;

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
pub(crate) fn signal_type_key(signal: &SignalType) -> &'static str {
    match signal {
        SignalType::ZScore => "z_score",
        SignalType::DbscanNoise => "dbscan_noise",
        SignalType::BehavioralDeviation => "behavioral_deviation",
        SignalType::GraphAnomaly => "graph_anomaly",
    }
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
}
