//! ScoringConfig rule kind — anomaly signal weights, classification
//! thresholds, z-score normalization, and graph anomaly parameters.

use serde::{Deserialize, Serialize};

use crate::schema::CommonMetadata;

// ── YAML-level types ────────────────────────────────────────────────

/// Top-level ScoringConfig rule document.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ScoringConfigRule {
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    pub kind: String,
    pub metadata: CommonMetadata,
    pub spec: ScoringConfigSpec,
}

/// Specification section of a ScoringConfig rule.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ScoringConfigSpec {
    /// Weights for the four anomaly detection signals (must sum to ~1.0).
    pub multi_signal_weights: MultiSignalWeights,
    /// Thresholds for anomaly classification buckets.
    pub classification_thresholds: ClassificationThresholds,
    /// Z-score normalization parameters.
    pub z_score_normalization: ZScoreNormalization,
    /// Graph anomaly scoring parameters.
    pub graph_anomaly: GraphAnomalyParams,
    /// Default anomaly threshold for cluster-based z-score scoring.
    #[serde(default = "default_anomaly_threshold")]
    pub default_anomaly_threshold: f64,
}

fn default_anomaly_threshold() -> f64 {
    2.0
}

/// Weights for the four multi-signal anomaly detectors.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct MultiSignalWeights {
    pub statistical: f64,
    pub dbscan_noise: f64,
    pub behavioral: f64,
    pub graph: f64,
}

/// Classification thresholds — ascending boundaries for Normal/Mild/Anomalous/HighlyAnomalous.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ClassificationThresholds {
    /// Scores below this are Normal.
    pub mild: f64,
    /// Scores between mild and this are Mild.
    pub anomalous: f64,
    /// Scores between anomalous and this are Anomalous; above is HighlyAnomalous.
    pub highly_anomalous: f64,
}

/// Z-score normalization parameters.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ZScoreNormalization {
    /// Divisor for normalizing max z-score to [0, 1] range.
    pub divisor: f64,
}

/// Graph anomaly scoring parameters.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct GraphAnomalyParams {
    /// Multiplier for avg_neighbor_count to determine "high connectivity" threshold.
    pub neighbor_multiplier: f64,
    /// Score addition when connectivity exceeds threshold.
    pub high_connectivity_score: f64,
    /// Community count threshold for "multi-community" bonus.
    pub community_threshold: u64,
    /// Score addition when community count exceeds threshold.
    pub multi_community_score: f64,
}

// ── Compiled (hot-path) types ───────────────────────────────────────

/// Pre-compiled scoring config — all fields are already typed, no lookup needed.
/// Kept as a separate type for consistency with the compiled pattern.
pub type CompiledScoringConfig = ScoringConfigSpec;

impl ScoringConfigRule {
    /// Compile the YAML config (trivial — spec is already typed).
    pub fn compile(&self) -> CompiledScoringConfig {
        self.spec.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_scoring_config_yaml() {
        let yaml = include_str!("../../../data/rules/scoring/scoring-config.yml");
        let rule: ScoringConfigRule = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(rule.kind, "ScoringConfig");

        let weights = &rule.spec.multi_signal_weights;
        let sum = weights.statistical + weights.dbscan_noise + weights.behavioral + weights.graph;
        assert!((sum - 1.0).abs() < 1e-10, "Weights should sum to 1.0");
    }

    #[test]
    fn classification_thresholds_ascending() {
        let yaml = include_str!("../../../data/rules/scoring/scoring-config.yml");
        let rule: ScoringConfigRule = serde_yaml::from_str(yaml).unwrap();
        let t = &rule.spec.classification_thresholds;
        assert!(t.mild < t.anomalous);
        assert!(t.anomalous < t.highly_anomalous);
    }

    #[test]
    fn extends_override_single_weight() {
        let yaml = r#"
apiVersion: v1
kind: ScoringConfig
metadata:
  id: scoring-custom
  name: Custom Scoring
  extends: scoring-default
  enabled: true
spec:
  multi_signal_weights:
    statistical: 0.25
    dbscan_noise: 0.25
    behavioral: 0.25
    graph: 0.25
  classification_thresholds:
    mild: 0.3
    anomalous: 0.5
    highly_anomalous: 0.7
  z_score_normalization:
    divisor: 5.0
  graph_anomaly:
    neighbor_multiplier: 3.0
    high_connectivity_score: 0.5
    community_threshold: 3
    multi_community_score: 0.3
  default_anomaly_threshold: 2.0
"#;
        let rule: ScoringConfigRule = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(rule.metadata.extends.as_deref(), Some("scoring-default"));
        assert_eq!(rule.spec.multi_signal_weights.statistical, 0.25);
    }

    #[test]
    fn round_trip() {
        let yaml = include_str!("../../../data/rules/scoring/scoring-config.yml");
        let rule: ScoringConfigRule = serde_yaml::from_str(yaml).unwrap();
        let serialized = serde_yaml::to_string(&rule).unwrap();
        let rule2: ScoringConfigRule = serde_yaml::from_str(&serialized).unwrap();
        assert_eq!(rule, rule2);
    }
}
