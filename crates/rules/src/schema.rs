//! YAML DSL schema types with serde deserialization.
//!
//! Defines the complete type hierarchy for anomaly detection rules:
//! `AnomalyRule` → `Detection` → `Template` / `Composition` → `Signal`

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ── Root rule document ───────────────────────────────────────────────

/// Top-level anomaly rule definition parsed from YAML.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AnomalyRule {
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    pub kind: String,
    pub metadata: RuleMetadata,
    pub schedule: Schedule,
    pub detection: Detection,
    pub filters: Option<Filters>,
    #[serde(default)]
    pub notifications: Vec<NotificationChannel>,
}

/// Rule identity and lifecycle metadata.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RuleMetadata {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

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
}
