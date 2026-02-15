//! Integration tests that verify every example YAML rule in
//! `data/rules/examples/` deserializes correctly against the schema.

use stupid_rules::schema::{
    AnomalyRule, ChannelType, Condition, DetectionTemplate, LogicalOperator, NotifyEvent,
    SignalType,
};

/// Resolve the examples directory relative to the workspace root.
/// Integration tests run from the crate directory, so we go up two levels.
fn examples_dir() -> std::path::PathBuf {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest.join("../../data/rules/examples")
}

/// Resolve the default rules directory (root of data/rules/).
fn defaults_dir() -> std::path::PathBuf {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest.join("../../data/rules")
}

fn load_default_rule(filename: &str) -> AnomalyRule {
    let path = defaults_dir().join(filename);
    let yaml = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("Failed to read {}: {}", path.display(), e));
    serde_yaml::from_str(&yaml)
        .unwrap_or_else(|e| panic!("Failed to parse {}: {}", path.display(), e))
}

fn tags(rule: &AnomalyRule) -> Vec<&str> {
    rule.metadata
        .tags
        .as_ref()
        .unwrap()
        .iter()
        .map(|s| s.as_str())
        .collect()
}

fn entity_types(rule: &AnomalyRule) -> Vec<&str> {
    rule.filters
        .as_ref()
        .unwrap()
        .entity_types
        .as_ref()
        .unwrap()
        .iter()
        .map(|s| s.as_str())
        .collect()
}

fn load_rule(filename: &str) -> AnomalyRule {
    let path = examples_dir().join(filename);
    let yaml = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("Failed to read {}: {}", path.display(), e));
    serde_yaml::from_str(&yaml)
        .unwrap_or_else(|e| panic!("Failed to parse {}: {}", path.display(), e))
}

// ── login-spike.yml ─────────────────────────────────────────

#[test]
fn parse_login_spike_example() {
    let rule = load_rule("login-spike.yml");

    assert_eq!(rule.api_version, "v1");
    assert_eq!(rule.kind, "AnomalyRule");
    assert_eq!(rule.metadata.id, "login-spike");
    assert!(!rule.metadata.enabled);
    assert_eq!(tags(&rule), vec!["security", "login", "spike"]);

    // Detection: spike template
    assert_eq!(rule.detection.template, Some(DetectionTemplate::Spike));
    assert!(rule.detection.compose.is_none());

    let params = rule.detection.parse_spike_params().unwrap().unwrap();
    assert_eq!(params.feature, "login_count");
    assert_eq!(params.multiplier, 3.0);
    assert_eq!(params.baseline.as_deref(), Some("cluster_centroid"));

    // Schedule
    assert_eq!(rule.schedule.cron, "*/15 * * * *");

    // Filters
    assert_eq!(entity_types(&rule), vec!["Member"]);
    assert_eq!(rule.filters.as_ref().unwrap().min_score, Some(0.5));

    // Notifications
    assert_eq!(rule.notifications.len(), 1);
    assert_eq!(rule.notifications[0].channel, ChannelType::Webhook);
}

// ── vip-absence.yml ─────────────────────────────────────────

#[test]
fn parse_vip_absence_example() {
    let rule = load_rule("vip-absence.yml");

    assert_eq!(rule.metadata.id, "vip-absence");
    assert!(!rule.metadata.enabled);
    assert_eq!(tags(&rule), vec!["vip", "retention", "absence"]);

    // Detection: absence template
    assert_eq!(rule.detection.template, Some(DetectionTemplate::Absence));
    let params = rule.detection.parse_absence_params().unwrap().unwrap();
    assert_eq!(params.feature, "login_count");
    assert_eq!(params.threshold, 0.0);
    assert_eq!(params.lookback_days, 7);

    // Schedule
    assert_eq!(rule.schedule.cron, "0 9 * * *");
    assert_eq!(rule.schedule.timezone, "Asia/Manila");

    // Filters: where condition
    let filters = rule.filters.as_ref().unwrap();
    let conditions = filters.conditions.as_ref().unwrap();
    let vip_cond = conditions.get("vip_group_numeric").unwrap();
    assert_eq!(vip_cond.gte, Some(4.0));

    // Two notification channels: email + telegram
    assert_eq!(rule.notifications.len(), 2);
    assert_eq!(rule.notifications[0].channel, ChannelType::Email);
    assert_eq!(rule.notifications[1].channel, ChannelType::Telegram);
}

// ── error-burst.yml ─────────────────────────────────────────

#[test]
fn parse_error_burst_example() {
    let rule = load_rule("error-burst.yml");

    assert_eq!(rule.metadata.id, "error-burst");
    assert!(!rule.metadata.enabled);
    assert_eq!(tags(&rule), vec!["errors", "threshold", "operations"]);

    // Detection: threshold template
    assert_eq!(rule.detection.template, Some(DetectionTemplate::Threshold));
    let params = rule.detection.parse_threshold_params().unwrap().unwrap();
    assert_eq!(params.feature, "error_count");
    assert_eq!(params.value, 100.0);

    // Schedule with cooldown
    assert_eq!(rule.schedule.cron, "*/5 * * * *");
    assert_eq!(rule.schedule.cooldown.as_deref(), Some("1h"));

    // Notifications include resolve event
    assert_eq!(rule.notifications.len(), 1);
    assert!(rule.notifications[0].on.contains(&NotifyEvent::Trigger));
    assert!(rule.notifications[0].on.contains(&NotifyEvent::Resolve));
}

// ── multi-signal-fraud.yml ──────────────────────────────────

#[test]
fn parse_multi_signal_fraud_example() {
    let rule = load_rule("multi-signal-fraud.yml");

    assert_eq!(rule.metadata.id, "multi-signal-fraud");
    assert!(!rule.metadata.enabled);
    assert_eq!(tags(&rule), vec!["fraud", "composite", "multi-signal"]);

    // Detection: compose (no template)
    assert!(rule.detection.template.is_none());
    let comp = rule.detection.compose.as_ref().unwrap();
    assert_eq!(comp.operator, LogicalOperator::And);
    assert_eq!(comp.conditions.len(), 2);

    // Second condition is nested OR with 2 sub-conditions
    match &comp.conditions[1] {
        Condition::Nested(inner) => {
            assert_eq!(inner.operator, LogicalOperator::Or);
            assert_eq!(inner.conditions.len(), 2);
        }
        _ => panic!("Expected nested OR composition"),
    }

    // Enrichment
    let enrich = rule.detection.enrich.as_ref().unwrap();
    let os = enrich.opensearch.as_ref().unwrap();
    assert_eq!(os.min_hits, Some(20));
    assert_eq!(os.rate_limit, 30);

    // Schedule with cooldown
    assert_eq!(rule.schedule.cron, "*/30 * * * *");
    assert_eq!(rule.schedule.cooldown.as_deref(), Some("30m"));

    // Filters
    let filters = rule.filters.as_ref().unwrap();
    assert_eq!(filters.min_score, Some(0.7));

    // Two notification channels: webhook + telegram
    assert_eq!(rule.notifications.len(), 2);
    assert_eq!(rule.notifications[0].channel, ChannelType::Webhook);
    assert_eq!(rule.notifications[1].channel, ChannelType::Telegram);
}

// ── Default rules: compute pipeline mirrors ─────────────────

#[test]
fn parse_behavioral_drift_default() {
    let rule = load_default_rule("behavioral-drift.yml");

    assert_eq!(rule.metadata.id, "behavioral-drift");
    assert!(rule.metadata.enabled);
    assert!(tags(&rule).contains(&"compute-default"));

    // Detection: drift template with all 10 features
    assert_eq!(rule.detection.template, Some(DetectionTemplate::Drift));
    let params = rule.detection.parse_drift_params().unwrap().unwrap();
    assert_eq!(params.features.len(), 10);
    assert_eq!(params.method.as_deref(), Some("cosine"));
    assert_eq!(params.threshold, 0.4);

    assert_eq!(entity_types(&rule), vec!["Member"]);
}

#[test]
fn parse_trend_spike_default() {
    let rule = load_default_rule("trend-spike.yml");

    assert_eq!(rule.metadata.id, "trend-spike");
    assert!(rule.metadata.enabled);
    assert!(tags(&rule).contains(&"compute-default"));

    // Detection: compose with z_score signal
    assert!(rule.detection.template.is_none());
    let comp = rule.detection.compose.as_ref().unwrap();
    assert_eq!(comp.operator, LogicalOperator::Or);
    match &comp.conditions[0] {
        Condition::Signal { signal, threshold, .. } => {
            assert_eq!(*signal, SignalType::ZScore);
            assert_eq!(*threshold, 3.0);
        }
        _ => panic!("Expected z_score signal condition"),
    }

    // Hourly schedule
    assert_eq!(rule.schedule.cron, "0 * * * *");
    assert_eq!(rule.schedule.cooldown.as_deref(), Some("1h"));
}

#[test]
fn parse_statistical_outlier_default() {
    let rule = load_default_rule("statistical-outlier.yml");

    assert_eq!(rule.metadata.id, "statistical-outlier");
    assert!(rule.metadata.enabled);
    assert!(tags(&rule).contains(&"compute-default"));

    // Compose: z_score > 2.5
    let comp = rule.detection.compose.as_ref().unwrap();
    match &comp.conditions[0] {
        Condition::Signal { signal, threshold, .. } => {
            assert_eq!(*signal, SignalType::ZScore);
            assert_eq!(*threshold, 2.5);
        }
        _ => panic!("Expected z_score signal condition"),
    }
}

#[test]
fn parse_dbscan_noise_default() {
    let rule = load_default_rule("dbscan-noise.yml");

    assert_eq!(rule.metadata.id, "dbscan-noise");
    assert!(rule.metadata.enabled);
    assert!(tags(&rule).contains(&"compute-default"));

    // Compose: dbscan_noise > 0.6
    let comp = rule.detection.compose.as_ref().unwrap();
    match &comp.conditions[0] {
        Condition::Signal { signal, threshold, .. } => {
            assert_eq!(*signal, SignalType::DbscanNoise);
            assert_eq!(*threshold, 0.6);
        }
        _ => panic!("Expected dbscan_noise signal condition"),
    }

    assert_eq!(rule.schedule.cooldown.as_deref(), Some("30m"));
}

#[test]
fn parse_graph_anomaly_default() {
    let rule = load_default_rule("graph-anomaly.yml");

    assert_eq!(rule.metadata.id, "graph-anomaly");
    assert!(rule.metadata.enabled);
    assert!(tags(&rule).contains(&"compute-default"));

    // Compose: graph_anomaly > 0.4
    let comp = rule.detection.compose.as_ref().unwrap();
    match &comp.conditions[0] {
        Condition::Signal { signal, threshold, .. } => {
            assert_eq!(*signal, SignalType::GraphAnomaly);
            assert_eq!(*threshold, 0.4);
        }
        _ => panic!("Expected graph_anomaly signal condition"),
    }

    assert_eq!(rule.schedule.cron, "*/30 * * * *");
    assert_eq!(rule.schedule.cooldown.as_deref(), Some("1h"));
}

// ── Default rules loaded by RuleLoader ──────────────────────

#[test]
fn rule_loader_loads_all_defaults() {
    let loader = stupid_rules::loader::RuleLoader::new(defaults_dir());
    let results = loader.load_all().unwrap();

    let loaded: Vec<_> = results
        .iter()
        .filter_map(|r| match &r.status {
            stupid_rules::loader::LoadStatus::Loaded { rule_id } => Some(rule_id.as_str()),
            _ => None,
        })
        .collect();

    // All 9 default rules should load (4 original + 5 new compute-default)
    let expected = vec![
        "behavioral-drift",
        "dbscan-noise",
        "error-burst",
        "graph-anomaly",
        "login-spike",
        "multi-signal-fraud",
        "statistical-outlier",
        "trend-spike",
        "vip-absence",
    ];

    let mut sorted = loaded.clone();
    sorted.sort();
    assert_eq!(sorted, expected, "Expected all 9 default rules to load");
}

// ── Round-trip: all examples survive serialize → deserialize ─

#[test]
fn all_examples_round_trip() {
    for filename in &[
        "login-spike.yml",
        "vip-absence.yml",
        "error-burst.yml",
        "multi-signal-fraud.yml",
    ] {
        let rule = load_rule(filename);
        let yaml = serde_yaml::to_string(&rule)
            .unwrap_or_else(|e| panic!("Failed to serialize {}: {}", filename, e));
        let rule2: AnomalyRule = serde_yaml::from_str(&yaml)
            .unwrap_or_else(|e| panic!("Failed to re-parse {}: {}", filename, e));
        assert_eq!(rule, rule2, "Round-trip failed for {}", filename);
    }
}

#[test]
fn all_defaults_round_trip() {
    for filename in &[
        "behavioral-drift.yml",
        "dbscan-noise.yml",
        "graph-anomaly.yml",
        "statistical-outlier.yml",
        "trend-spike.yml",
    ] {
        let rule = load_default_rule(filename);
        let yaml = serde_yaml::to_string(&rule)
            .unwrap_or_else(|e| panic!("Failed to serialize {}: {}", filename, e));
        let rule2: AnomalyRule = serde_yaml::from_str(&yaml)
            .unwrap_or_else(|e| panic!("Failed to re-parse {}: {}", filename, e));
        assert_eq!(rule, rule2, "Round-trip failed for {}", filename);
    }
}
