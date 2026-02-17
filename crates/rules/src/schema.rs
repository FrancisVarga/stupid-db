//! YAML DSL schema types with serde deserialization.
//!
//! Defines the complete type hierarchy for rule documents:
//! - `RuleEnvelope`: lightweight first-pass header (apiVersion, kind, metadata)
//! - `RuleDocument`: enum dispatching to kind-specific types
//! - `AnomalyRule`: anomaly detection rules with templates and signal composition
//!
//! New rule kinds (EntitySchema, FeatureConfig, etc.) are added as `RuleDocument` variants.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;

// ── Rule kind enum ──────────────────────────────────────────────────

/// Supported rule kinds for two-pass deserialization dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RuleKind {
    AnomalyRule,
    EntitySchema,
    FeatureConfig,
    ScoringConfig,
    TrendConfig,
    PatternConfig,
}

impl fmt::Display for RuleKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RuleKind::AnomalyRule => write!(f, "AnomalyRule"),
            RuleKind::EntitySchema => write!(f, "EntitySchema"),
            RuleKind::FeatureConfig => write!(f, "FeatureConfig"),
            RuleKind::ScoringConfig => write!(f, "ScoringConfig"),
            RuleKind::TrendConfig => write!(f, "TrendConfig"),
            RuleKind::PatternConfig => write!(f, "PatternConfig"),
        }
    }
}

impl FromStr for RuleKind {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "AnomalyRule" => Ok(RuleKind::AnomalyRule),
            "EntitySchema" => Ok(RuleKind::EntitySchema),
            "FeatureConfig" => Ok(RuleKind::FeatureConfig),
            "ScoringConfig" => Ok(RuleKind::ScoringConfig),
            "TrendConfig" => Ok(RuleKind::TrendConfig),
            "PatternConfig" => Ok(RuleKind::PatternConfig),
            other => Err(format!("unknown rule kind: '{}'", other)),
        }
    }
}

// ── Rule envelope (first-pass) ──────────────────────────────────────

/// Lightweight first-pass deserializer that reads only the header fields.
///
/// Used during two-pass loading: first extract `kind` to determine the
/// concrete type, then deserialize the full document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleEnvelope {
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    pub kind: String,
    pub metadata: CommonMetadata,
    /// Remaining fields captured as raw YAML for second-pass deserialization.
    #[serde(flatten)]
    pub rest: serde_yaml::Value,
}

impl RuleEnvelope {
    /// Parse the `kind` field into a typed [`RuleKind`].
    pub fn rule_kind(&self) -> std::result::Result<RuleKind, String> {
        self.kind.parse()
    }

    /// Two-pass: reconstruct the full YAML and deserialize into the concrete type.
    pub fn parse_full(&self) -> std::result::Result<RuleDocument, String> {
        match self.rule_kind()? {
            RuleKind::AnomalyRule => {
                let yaml = serde_yaml::to_string(self).map_err(|e| e.to_string())?;
                let rule: AnomalyRule =
                    serde_yaml::from_str(&yaml).map_err(|e| e.to_string())?;
                Ok(RuleDocument::Anomaly(rule))
            }
            RuleKind::EntitySchema => {
                let yaml = serde_yaml::to_string(self).map_err(|e| e.to_string())?;
                let rule: crate::entity_schema::EntitySchemaRule =
                    serde_yaml::from_str(&yaml).map_err(|e| e.to_string())?;
                Ok(RuleDocument::EntitySchema(rule))
            }
            RuleKind::FeatureConfig => {
                let yaml = serde_yaml::to_string(self).map_err(|e| e.to_string())?;
                let rule: crate::feature_config::FeatureConfigRule =
                    serde_yaml::from_str(&yaml).map_err(|e| e.to_string())?;
                Ok(RuleDocument::FeatureConfig(rule))
            }
            RuleKind::ScoringConfig => {
                let yaml = serde_yaml::to_string(self).map_err(|e| e.to_string())?;
                let rule: crate::scoring_config::ScoringConfigRule =
                    serde_yaml::from_str(&yaml).map_err(|e| e.to_string())?;
                Ok(RuleDocument::ScoringConfig(rule))
            }
            RuleKind::TrendConfig => {
                let yaml = serde_yaml::to_string(self).map_err(|e| e.to_string())?;
                let rule: crate::trend_config::TrendConfigRule =
                    serde_yaml::from_str(&yaml).map_err(|e| e.to_string())?;
                Ok(RuleDocument::TrendConfig(rule))
            }
            RuleKind::PatternConfig => {
                let yaml = serde_yaml::to_string(self).map_err(|e| e.to_string())?;
                let rule: crate::pattern_config::PatternConfigRule =
                    serde_yaml::from_str(&yaml).map_err(|e| e.to_string())?;
                Ok(RuleDocument::PatternConfig(rule))
            }
        }
    }
}

// ── Rule document (multi-kind container) ────────────────────────────

/// A fully deserialized rule of any supported kind.
#[derive(Debug, Clone, PartialEq)]
pub enum RuleDocument {
    /// Anomaly detection rule (spike, drift, absence, threshold, compose).
    Anomaly(AnomalyRule),
    /// Entity schema — field mappings, extraction plans, embedding templates.
    EntitySchema(crate::entity_schema::EntitySchemaRule),
    /// Feature config — feature vector, encodings, event classification.
    FeatureConfig(crate::feature_config::FeatureConfigRule),
    /// Scoring config — anomaly weights, thresholds, graph params.
    ScoringConfig(crate::scoring_config::ScoringConfigRule),
    /// Trend config — z-score thresholds, window defaults, severity.
    TrendConfig(crate::trend_config::TrendConfigRule),
    /// Pattern config — PrefixSpan defaults, classification rules.
    PatternConfig(crate::pattern_config::PatternConfigRule),
}

impl RuleDocument {
    /// Get the rule's metadata regardless of kind.
    pub fn metadata(&self) -> &CommonMetadata {
        match self {
            RuleDocument::Anomaly(rule) => &rule.metadata,
            RuleDocument::EntitySchema(rule) => &rule.metadata,
            RuleDocument::FeatureConfig(rule) => &rule.metadata,
            RuleDocument::ScoringConfig(rule) => &rule.metadata,
            RuleDocument::TrendConfig(rule) => &rule.metadata,
            RuleDocument::PatternConfig(rule) => &rule.metadata,
        }
    }

    /// Get the rule kind.
    pub fn kind(&self) -> RuleKind {
        match self {
            RuleDocument::Anomaly(_) => RuleKind::AnomalyRule,
            RuleDocument::EntitySchema(_) => RuleKind::EntitySchema,
            RuleDocument::FeatureConfig(_) => RuleKind::FeatureConfig,
            RuleDocument::ScoringConfig(_) => RuleKind::ScoringConfig,
            RuleDocument::TrendConfig(_) => RuleKind::TrendConfig,
            RuleDocument::PatternConfig(_) => RuleKind::PatternConfig,
        }
    }

    /// Try to extract as an `AnomalyRule` reference.
    pub fn as_anomaly(&self) -> Option<&AnomalyRule> {
        match self {
            RuleDocument::Anomaly(rule) => Some(rule),
            _ => None,
        }
    }

    /// Try to extract as an `EntitySchemaRule` reference.
    pub fn as_entity_schema(&self) -> Option<&crate::entity_schema::EntitySchemaRule> {
        match self {
            RuleDocument::EntitySchema(rule) => Some(rule),
            _ => None,
        }
    }

    /// Try to extract as a `FeatureConfigRule` reference.
    pub fn as_feature_config(&self) -> Option<&crate::feature_config::FeatureConfigRule> {
        match self {
            RuleDocument::FeatureConfig(rule) => Some(rule),
            _ => None,
        }
    }

    /// Try to extract as a `ScoringConfigRule` reference.
    pub fn as_scoring_config(&self) -> Option<&crate::scoring_config::ScoringConfigRule> {
        match self {
            RuleDocument::ScoringConfig(rule) => Some(rule),
            _ => None,
        }
    }

    /// Try to extract as a `TrendConfigRule` reference.
    pub fn as_trend_config(&self) -> Option<&crate::trend_config::TrendConfigRule> {
        match self {
            RuleDocument::TrendConfig(rule) => Some(rule),
            _ => None,
        }
    }

    /// Try to extract as a `PatternConfigRule` reference.
    pub fn as_pattern_config(&self) -> Option<&crate::pattern_config::PatternConfigRule> {
        match self {
            RuleDocument::PatternConfig(rule) => Some(rule),
            _ => None,
        }
    }

    /// Serialize this document to YAML, delegating to the inner type.
    pub fn to_yaml(&self) -> std::result::Result<String, serde_yaml::Error> {
        match self {
            RuleDocument::Anomaly(r) => serde_yaml::to_string(r),
            RuleDocument::EntitySchema(r) => serde_yaml::to_string(r),
            RuleDocument::FeatureConfig(r) => serde_yaml::to_string(r),
            RuleDocument::ScoringConfig(r) => serde_yaml::to_string(r),
            RuleDocument::TrendConfig(r) => serde_yaml::to_string(r),
            RuleDocument::PatternConfig(r) => serde_yaml::to_string(r),
        }
    }
}

// ── Root anomaly rule document ──────────────────────────────────────

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

/// Shared metadata for all rule kinds (anomaly, entity schema, feature config, etc.).
///
/// The `extends` field enables rule inheritance: a child rule references a parent
/// by ID and deep-merges the parent's fields, with the child's values winning.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CommonMetadata {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Parent rule ID for inheritance. The loader deep-merges the parent's
    /// spec into this rule, with child fields taking precedence.
    #[serde(default)]
    pub extends: Option<String>,
}

/// Type alias for backward compatibility with existing code that references `RuleMetadata`.
pub type RuleMetadata = CommonMetadata;

fn default_true() -> bool {
    true
}

// ── Schedule ─────────────────────────────────────────────────────────

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

// ── Detection ────────────────────────────────────────────────────────

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

// ── Signal composition ───────────────────────────────────────────────

/// Boolean composition tree for combining detection signals.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Composition {
    pub operator: LogicalOperator,
    pub conditions: Vec<Condition>,
}

/// Logical operators for signal composition.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum LogicalOperator {
    And,
    Or,
    Not,
}

/// A condition leaf or nested composition node.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum Condition {
    /// A direct signal check against a threshold.
    Signal {
        signal: SignalType,
        #[serde(default)]
        feature: Option<String>,
        threshold: f64,
    },
    /// A nested composition for recursive boolean logic.
    Nested(Composition),
}

/// Signal types available for composition conditions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SignalType {
    ZScore,
    DbscanNoise,
    BehavioralDeviation,
    GraphAnomaly,
}

// ── Enrichment ───────────────────────────────────────────────────────

/// Optional enrichment step that runs after detection signals fire.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Enrichment {
    #[serde(default)]
    pub opensearch: Option<OpenSearchEnrichment>,
}

/// OpenSearch query enrichment configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct OpenSearchEnrichment {
    pub query: serde_json::Value,
    #[serde(default)]
    pub min_hits: Option<u64>,
    #[serde(default)]
    pub max_hits: Option<u64>,
    #[serde(default = "default_rate_limit")]
    pub rate_limit: u32,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

fn default_rate_limit() -> u32 {
    60
}

// ── Filters ──────────────────────────────────────────────────────────

/// Post-detection filters to narrow which entities trigger alerts.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Filters {
    #[serde(default)]
    pub entity_types: Option<Vec<String>>,
    #[serde(default)]
    pub classifications: Option<Vec<String>>,
    #[serde(default)]
    pub min_score: Option<f64>,
    #[serde(default)]
    pub exclude_keys: Option<Vec<String>>,
    #[serde(default, rename = "where")]
    pub conditions: Option<HashMap<String, FilterCondition>>,
}

/// Numeric comparison conditions for entity feature filtering.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FilterCondition {
    #[serde(default)]
    pub gt: Option<f64>,
    #[serde(default)]
    pub gte: Option<f64>,
    #[serde(default)]
    pub lt: Option<f64>,
    #[serde(default)]
    pub lte: Option<f64>,
    #[serde(default)]
    pub eq: Option<f64>,
    #[serde(default)]
    pub neq: Option<f64>,
}

impl FilterCondition {
    /// Check if a value passes all conditions.
    pub fn matches(&self, value: f64) -> bool {
        if let Some(v) = self.gt {
            if value <= v {
                return false;
            }
        }
        if let Some(v) = self.gte {
            if value < v {
                return false;
            }
        }
        if let Some(v) = self.lt {
            if value >= v {
                return false;
            }
        }
        if let Some(v) = self.lte {
            if value > v {
                return false;
            }
        }
        if let Some(v) = self.eq {
            if (value - v).abs() > f64::EPSILON {
                return false;
            }
        }
        if let Some(v) = self.neq {
            if (value - v).abs() <= f64::EPSILON {
                return false;
            }
        }
        true
    }
}

// ── Notifications ────────────────────────────────────────────────────

/// A notification channel configuration within a rule.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NotificationChannel {
    pub channel: ChannelType,
    #[serde(default = "default_on_events")]
    pub on: Vec<NotifyEvent>,
    #[serde(default)]
    pub template: Option<String>,
    // Channel-specific configuration
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub method: Option<String>,
    #[serde(default)]
    pub headers: Option<HashMap<String, String>>,
    #[serde(default)]
    pub body_template: Option<String>,
    // Email fields
    #[serde(default)]
    pub smtp_host: Option<String>,
    #[serde(default)]
    pub smtp_port: Option<u16>,
    #[serde(default)]
    pub tls: Option<bool>,
    #[serde(default)]
    pub from: Option<String>,
    #[serde(default)]
    pub to: Option<Vec<String>>,
    #[serde(default)]
    pub subject: Option<String>,
    // Telegram fields
    #[serde(default)]
    pub bot_token: Option<String>,
    #[serde(default)]
    pub chat_id: Option<String>,
    #[serde(default)]
    pub parse_mode: Option<String>,
}

fn default_on_events() -> Vec<NotifyEvent> {
    vec![NotifyEvent::Trigger]
}

/// Notification channel types.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ChannelType {
    Webhook,
    Email,
    Telegram,
}

/// Events that can trigger notifications.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum NotifyEvent {
    Trigger,
    Resolve,
}

// ── Helper: parse template params ────────────────────────────────────

impl Detection {
    /// Parse template-specific parameters from the `params` field.
    pub fn parse_spike_params(&self) -> Option<Result<SpikeParams, serde_yaml::Error>> {
        self.params
            .as_ref()
            .map(|v| serde_yaml::from_value(v.clone()))
    }

    pub fn parse_drift_params(&self) -> Option<Result<DriftParams, serde_yaml::Error>> {
        self.params
            .as_ref()
            .map(|v| serde_yaml::from_value(v.clone()))
    }

    pub fn parse_absence_params(&self) -> Option<Result<AbsenceParams, serde_yaml::Error>> {
        self.params
            .as_ref()
            .map(|v| serde_yaml::from_value(v.clone()))
    }

    pub fn parse_threshold_params(&self) -> Option<Result<ThresholdParams, serde_yaml::Error>> {
        self.params
            .as_ref()
            .map(|v| serde_yaml::from_value(v.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPIKE_RULE_YAML: &str = r#"
apiVersion: v1
kind: AnomalyRule
metadata:
  id: login-spike
  name: Login Spike Detection
  description: Detect members with abnormal login frequency
  tags: [security, login]
  enabled: true
schedule:
  cron: "*/15 * * * *"
  timezone: Asia/Manila
  cooldown: "30m"
detection:
  template: spike
  params:
    feature: login_count
    multiplier: 3.0
    baseline: cluster_centroid
    min_samples: 5
filters:
  entity_types: [Member]
  min_score: 0.5
notifications:
  - channel: webhook
    on: [trigger]
    url: "https://hooks.example.com/alerts"
    method: POST
"#;

    const COMPOSE_RULE_YAML: &str = r#"
apiVersion: v1
kind: AnomalyRule
metadata:
  id: multi-signal-fraud
  name: Multi-Signal Fraud Detection
  enabled: true
schedule:
  cron: "*/30 * * * *"
  timezone: UTC
detection:
  compose:
    operator: and
    conditions:
      - signal: z_score
        threshold: 3.0
      - operator: or
        conditions:
          - signal: dbscan_noise
            threshold: 0.6
          - signal: graph_anomaly
            threshold: 0.5
  enrich:
    opensearch:
      query: {"bool": {"must": [{"range": {"@timestamp": {"gte": "now-1h"}}}]}}
      min_hits: 20
      rate_limit: 30
      timeout_ms: 5000
notifications:
  - channel: telegram
    on: [trigger, resolve]
    bot_token: "${TELEGRAM_BOT_TOKEN}"
    chat_id: "-100123456"
    parse_mode: MarkdownV2
"#;

    #[test]
    fn parse_spike_rule() {
        let rule: AnomalyRule = serde_yaml::from_str(SPIKE_RULE_YAML).unwrap();
        assert_eq!(rule.api_version, "v1");
        assert_eq!(rule.metadata.id, "login-spike");
        assert_eq!(rule.detection.template, Some(DetectionTemplate::Spike));
        assert!(rule.detection.compose.is_none());

        let params = rule.detection.parse_spike_params().unwrap().unwrap();
        assert_eq!(params.feature, "login_count");
        assert_eq!(params.multiplier, 3.0);
        assert_eq!(params.baseline.as_deref(), Some("cluster_centroid"));
    }

    #[test]
    fn parse_compose_rule() {
        let rule: AnomalyRule = serde_yaml::from_str(COMPOSE_RULE_YAML).unwrap();
        assert_eq!(rule.metadata.id, "multi-signal-fraud");
        assert!(rule.detection.template.is_none());
        let comp = rule.detection.compose.as_ref().unwrap();
        assert_eq!(comp.operator, LogicalOperator::And);
        assert_eq!(comp.conditions.len(), 2);
        // Second condition is a nested OR
        match &comp.conditions[1] {
            Condition::Nested(inner) => {
                assert_eq!(inner.operator, LogicalOperator::Or);
                assert_eq!(inner.conditions.len(), 2);
            }
            _ => panic!("Expected nested composition"),
        }
        // Enrichment present
        let enrich = rule.detection.enrich.as_ref().unwrap();
        let os = enrich.opensearch.as_ref().unwrap();
        assert_eq!(os.min_hits, Some(20));
        assert_eq!(os.rate_limit, 30);
    }

    #[test]
    fn round_trip() {
        let rule: AnomalyRule = serde_yaml::from_str(SPIKE_RULE_YAML).unwrap();
        let yaml = serde_yaml::to_string(&rule).unwrap();
        let rule2: AnomalyRule = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(rule, rule2);
    }

    #[test]
    fn malformed_yaml_errors() {
        // Missing required field
        let missing_meta = r#"
apiVersion: v1
kind: AnomalyRule
schedule:
  cron: "* * * * *"
detection:
  template: spike
"#;
        assert!(serde_yaml::from_str::<AnomalyRule>(missing_meta).is_err());

        // Unknown template type
        let bad_template = r#"
apiVersion: v1
kind: AnomalyRule
metadata:
  id: test
  name: Test
  enabled: true
schedule:
  cron: "* * * * *"
detection:
  template: nonexistent
"#;
        assert!(serde_yaml::from_str::<AnomalyRule>(bad_template).is_err());

        // Unknown field in strict struct
        let unknown_field = r#"
apiVersion: v1
kind: AnomalyRule
metadata:
  id: test
  name: Test
  enabled: true
  bogus_field: oops
schedule:
  cron: "* * * * *"
detection:
  template: spike
"#;
        assert!(serde_yaml::from_str::<AnomalyRule>(unknown_field).is_err());
    }

    #[test]
    fn filter_condition_matches() {
        let cond = FilterCondition {
            gt: None,
            gte: Some(4.0),
            lt: None,
            lte: None,
            eq: None,
            neq: None,
        };
        assert!(cond.matches(4.0));
        assert!(cond.matches(5.0));
        assert!(!cond.matches(3.9));
    }

    #[test]
    fn threshold_params_deserialize() {
        let yaml = r#"
feature: error_count
operator: gte
value: 100.0
"#;
        let params: ThresholdParams = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(params.feature, "error_count");
        assert_eq!(params.operator, ThresholdOperator::Gte);
        assert_eq!(params.value, 100.0);
    }

    // ── RuleKind / RuleEnvelope / RuleDocument tests ────────────────

    #[test]
    fn rule_kind_from_str() {
        assert_eq!("AnomalyRule".parse::<RuleKind>().unwrap(), RuleKind::AnomalyRule);
        assert_eq!("EntitySchema".parse::<RuleKind>().unwrap(), RuleKind::EntitySchema);
        assert_eq!("FeatureConfig".parse::<RuleKind>().unwrap(), RuleKind::FeatureConfig);
        assert_eq!("ScoringConfig".parse::<RuleKind>().unwrap(), RuleKind::ScoringConfig);
        assert_eq!("TrendConfig".parse::<RuleKind>().unwrap(), RuleKind::TrendConfig);
        assert_eq!("PatternConfig".parse::<RuleKind>().unwrap(), RuleKind::PatternConfig);
        assert!("UnknownKind".parse::<RuleKind>().is_err());
    }

    #[test]
    fn rule_kind_display() {
        assert_eq!(RuleKind::AnomalyRule.to_string(), "AnomalyRule");
        assert_eq!(RuleKind::EntitySchema.to_string(), "EntitySchema");
    }

    #[test]
    fn rule_envelope_parses_anomaly_rule() {
        let envelope: RuleEnvelope = serde_yaml::from_str(SPIKE_RULE_YAML).unwrap();
        assert_eq!(envelope.api_version, "v1");
        assert_eq!(envelope.kind, "AnomalyRule");
        assert_eq!(envelope.metadata.id, "login-spike");
        assert_eq!(envelope.rule_kind().unwrap(), RuleKind::AnomalyRule);
    }

    #[test]
    fn rule_envelope_unknown_kind_errors() {
        let yaml = r#"
apiVersion: v1
kind: UnknownKind
metadata:
  id: test
  name: Test
  enabled: true
"#;
        let envelope: RuleEnvelope = serde_yaml::from_str(yaml).unwrap();
        assert!(envelope.rule_kind().is_err());
    }

    #[test]
    fn rule_envelope_parse_full_anomaly() {
        let envelope: RuleEnvelope = serde_yaml::from_str(SPIKE_RULE_YAML).unwrap();
        let doc = envelope.parse_full().unwrap();

        assert_eq!(doc.kind(), RuleKind::AnomalyRule);
        assert_eq!(doc.metadata().id, "login-spike");

        let rule = doc.as_anomaly().unwrap();
        assert_eq!(rule.detection.template, Some(DetectionTemplate::Spike));
    }

    #[test]
    fn rule_document_metadata_accessor() {
        let rule: AnomalyRule = serde_yaml::from_str(SPIKE_RULE_YAML).unwrap();
        let doc = RuleDocument::Anomaly(rule.clone());
        assert_eq!(doc.metadata().id, rule.metadata.id);
        assert_eq!(doc.metadata().name, rule.metadata.name);
    }

    #[test]
    fn common_metadata_with_extends() {
        let yaml = r#"
apiVersion: v1
kind: AnomalyRule
metadata:
  id: child-rule
  name: Child Rule
  extends: parent-rule
  enabled: true
schedule:
  cron: "*/15 * * * *"
detection:
  template: spike
  params:
    feature: login_count
    multiplier: 3.0
"#;
        let rule: AnomalyRule = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(rule.metadata.extends.as_deref(), Some("parent-rule"));
    }
}
