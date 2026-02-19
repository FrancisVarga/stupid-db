//! Anomaly rule types: root document, schedule, detection, and template parameters.

use serde::{Deserialize, Serialize};

use super::{CommonMetadata, Composition, Enrichment, Filters, NotificationChannel};

/// Top-level anomaly rule definition parsed from YAML.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AnomalyRule {
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    pub kind: String,
    pub metadata: CommonMetadata,
    pub schedule: Schedule,
    pub detection: Detection,
    pub filters: Option<Filters>,
    #[serde(default)]
    pub notifications: Vec<NotificationChannel>,
}

/// Cron-based execution schedule with timezone and cooldown.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Schedule {
    pub cron: String,
    #[serde(default = "default_timezone")]
    pub timezone: String,
    #[serde(default)]
    pub cooldown: Option<String>,
}

fn default_timezone() -> String {
    "UTC".to_string()
}

/// Detection configuration: exactly one of `template` or `compose`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Detection {
    #[serde(default)]
    pub template: Option<DetectionTemplate>,
    #[serde(default)]
    pub params: Option<serde_yaml::Value>,
    #[serde(default)]
    pub compose: Option<Composition>,
    #[serde(default)]
    pub enrich: Option<Enrichment>,
}

/// Built-in detection template types.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum DetectionTemplate {
    Spike,
    Drift,
    Absence,
    Threshold,
}

// ── Template parameters ──────────────────────────────────────────────

/// Parameters for the `spike` detection template.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SpikeParams {
    pub feature: String,
    pub multiplier: f64,
    #[serde(default)]
    pub baseline: Option<String>,
    #[serde(default)]
    pub min_samples: Option<usize>,
}

/// Parameters for the `drift` detection template.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DriftParams {
    pub features: Vec<String>,
    #[serde(default)]
    pub method: Option<String>,
    pub threshold: f64,
    #[serde(default)]
    pub window: Option<String>,
    #[serde(default)]
    pub baseline_window: Option<String>,
}

/// Parameters for the `absence` detection template.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AbsenceParams {
    pub feature: String,
    pub threshold: f64,
    pub lookback_days: u32,
    #[serde(default)]
    pub compare_to: Option<String>,
}

/// Parameters for the `threshold` detection template.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ThresholdParams {
    pub feature: String,
    pub operator: ThresholdOperator,
    pub value: f64,
}

/// Comparison operators for threshold detection.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ThresholdOperator {
    Gt,
    Gte,
    Lt,
    Lte,
    Eq,
    Neq,
}

// ── Helper: parse template params ────────────────────────────────────

impl Detection {
    /// Parse template-specific parameters from the `params` field.
    pub fn parse_spike_params(&self) -> Option<Result<SpikeParams, serde_yaml::Error>> {
        self.params
            .as_ref()
            .map(|v| serde_yaml::from_value(v.clone()))
    }

    /// Parse drift detection parameters from the `params` field.
    pub fn parse_drift_params(&self) -> Option<Result<DriftParams, serde_yaml::Error>> {
        self.params
            .as_ref()
            .map(|v| serde_yaml::from_value(v.clone()))
    }

    /// Parse absence detection parameters from the `params` field.
    pub fn parse_absence_params(&self) -> Option<Result<AbsenceParams, serde_yaml::Error>> {
        self.params
            .as_ref()
            .map(|v| serde_yaml::from_value(v.clone()))
    }

    /// Parse threshold detection parameters from the `params` field.
    pub fn parse_threshold_params(&self) -> Option<Result<ThresholdParams, serde_yaml::Error>> {
        self.params
            .as_ref()
            .map(|v| serde_yaml::from_value(v.clone()))
    }
}
