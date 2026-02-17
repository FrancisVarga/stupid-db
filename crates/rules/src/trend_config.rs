//! TrendConfig rule kind — z-score thresholds, direction thresholds,
//! window defaults, and severity classification for trend detection.

use serde::{Deserialize, Serialize};

use crate::schema::CommonMetadata;

// ── YAML-level types ────────────────────────────────────────────────

/// Top-level TrendConfig rule document.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TrendConfigRule {
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    pub kind: String,
    pub metadata: CommonMetadata,
    pub spec: TrendConfigSpec,
}

/// Specification section of a TrendConfig rule.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TrendConfigSpec {
    /// Default sliding window size (number of data points).
    pub default_window_size: usize,
    /// Minimum data points required for z-score calculation.
    pub min_data_points: usize,
    /// Z-score threshold that triggers trend detection.
    pub z_score_trigger: f64,
    /// Direction thresholds for classifying Up/Down/Stable.
    pub direction_thresholds: DirectionThresholds,
    /// Severity thresholds — ascending z-score boundaries.
    pub severity_thresholds: SeverityThresholds,
}

/// Thresholds for determining trend direction from z-score.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DirectionThresholds {
    /// z_score > this → Up trend.
    pub up: f64,
    /// z_score < -this → Down trend (stored as positive value).
    pub down: f64,
}

/// Severity thresholds — ascending z-score absolute values.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SeverityThresholds {
    /// |z| above this → Notable.
    pub notable: f64,
    /// |z| above this → Significant.
    pub significant: f64,
    /// |z| above this → Critical.
    pub critical: f64,
}

// ── Compiled type ───────────────────────────────────────────────────

/// Pre-compiled trend config (trivial — spec is already typed).
pub type CompiledTrendConfig = TrendConfigSpec;

impl TrendConfigRule {
    /// Compile the YAML config.
    pub fn compile(&self) -> CompiledTrendConfig {
        self.spec.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_trend_config_yaml() {
        let yaml = include_str!("../../../data/rules/scoring/trend-config.yml");
        let rule: TrendConfigRule = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(rule.kind, "TrendConfig");
        assert_eq!(rule.spec.default_window_size, 168);
    }

    #[test]
    fn severity_thresholds_ascending() {
        let yaml = include_str!("../../../data/rules/scoring/trend-config.yml");
        let rule: TrendConfigRule = serde_yaml::from_str(yaml).unwrap();
        let t = &rule.spec.severity_thresholds;
        assert!(t.notable < t.significant);
        assert!(t.significant < t.critical);
    }

    #[test]
    fn round_trip() {
        let yaml = include_str!("../../../data/rules/scoring/trend-config.yml");
        let rule: TrendConfigRule = serde_yaml::from_str(yaml).unwrap();
        let serialized = serde_yaml::to_string(&rule).unwrap();
        let rule2: TrendConfigRule = serde_yaml::from_str(&serialized).unwrap();
        assert_eq!(rule, rule2);
    }
}
