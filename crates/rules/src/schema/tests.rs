//! Tests for schema types.

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
