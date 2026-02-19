//! Tests for the rule loader module.

use std::collections::HashMap;
use std::fs;

use tempfile::TempDir;

use super::*;
use crate::schema::RuleDocument;

const VALID_RULE_YAML: &str = r#"
apiVersion: v1
kind: AnomalyRule
metadata:
  id: test-rule
  name: Test Rule
  enabled: true
schedule:
  cron: "*/15 * * * *"
detection:
  template: spike
  params:
    feature: login_count
    multiplier: 3.0
"#;

fn temp_loader() -> (TempDir, RuleLoader) {
    let dir = TempDir::new().expect("create tempdir");
    let loader = RuleLoader::new(dir.path().to_path_buf());
    (dir, loader)
}

#[test]
fn load_rule_from_file() {
    let (dir, loader) = temp_loader();
    let rule_path = dir.path().join("test-rule.yml");
    fs::write(&rule_path, VALID_RULE_YAML).unwrap();

    let doc = loader.load_file(&rule_path).unwrap();
    assert_eq!(doc.metadata().id, "test-rule");
    assert_eq!(doc.metadata().name, "Test Rule");
    assert!(doc.as_anomaly().is_some());
}

#[test]
fn load_all_skips_dotfiles_and_non_yaml() {
    let (dir, loader) = temp_loader();

    // Valid YAML
    fs::write(dir.path().join("rule1.yml"), VALID_RULE_YAML).unwrap();

    // Dotfile (should be skipped)
    fs::write(dir.path().join(".hidden.yml"), VALID_RULE_YAML).unwrap();

    // Non-YAML (should be skipped)
    fs::write(dir.path().join("readme.txt"), "not a rule").unwrap();

    let results = loader.load_all().unwrap();

    let loaded: Vec<_> = results
        .iter()
        .filter(|r| matches!(r.status, LoadStatus::Loaded { .. }))
        .collect();
    let skipped: Vec<_> = results
        .iter()
        .filter(|r| matches!(r.status, LoadStatus::Skipped { .. }))
        .collect();

    assert_eq!(loaded.len(), 1);
    assert_eq!(skipped.len(), 2);

    // Verify rule is in both maps
    let rules = loader.rules();
    let guard = rules.read().unwrap();
    assert!(guard.contains_key("test-rule"));

    let docs = loader.documents();
    let guard = docs.read().unwrap();
    assert!(guard.contains_key("test-rule"));
}

#[test]
fn load_all_recursive_subdirectories() {
    let (dir, loader) = temp_loader();

    // Rule in root
    fs::write(dir.path().join("rule1.yml"), VALID_RULE_YAML).unwrap();

    // Rule in subdirectory
    let sub = dir.path().join("subdir");
    fs::create_dir(&sub).unwrap();
    let sub_yaml = VALID_RULE_YAML.replace("test-rule", "sub-rule");
    fs::write(sub.join("sub-rule.yml"), sub_yaml).unwrap();

    let results = loader.load_all().unwrap();

    let loaded: Vec<_> = results
        .iter()
        .filter_map(|r| match &r.status {
            LoadStatus::Loaded { rule_id } => Some(rule_id.as_str()),
            _ => None,
        })
        .collect();

    assert_eq!(loaded.len(), 2, "Should load rules from root and subdirectory");

    let docs = loader.documents();
    let guard = docs.read().unwrap();
    assert!(guard.contains_key("test-rule"));
    assert!(guard.contains_key("sub-rule"));
}

#[test]
fn load_file_two_pass_non_anomaly() {
    let (dir, loader) = temp_loader();
    let yaml = r#"
apiVersion: v1
kind: TrendConfig
metadata:
  id: trend-test
  name: Trend Test
  enabled: true
spec:
  default_window_size: 168
  min_data_points: 3
  z_score_trigger: 2.0
  direction_thresholds:
    up: 0.5
    down: 0.5
  severity_thresholds:
    notable: 2.0
    significant: 3.0
    critical: 4.0
"#;
    let path = dir.path().join("trend-test.yml");
    fs::write(&path, yaml).unwrap();

    let doc = loader.load_file(&path).unwrap();
    assert_eq!(doc.kind(), crate::schema::RuleKind::TrendConfig);
    assert_eq!(doc.metadata().id, "trend-test");
    assert!(doc.as_trend_config().is_some());
    // Should NOT be in anomaly_rules
    assert!(doc.as_anomaly().is_none());
}

#[test]
fn load_all_multi_kind() {
    let (dir, loader) = temp_loader();

    // AnomalyRule
    fs::write(dir.path().join("anomaly.yml"), VALID_RULE_YAML).unwrap();

    // TrendConfig in subdirectory
    let sub = dir.path().join("scoring");
    fs::create_dir(&sub).unwrap();
    fs::write(sub.join("trend.yml"), r#"
apiVersion: v1
kind: TrendConfig
metadata:
  id: trend-cfg
  name: Trend Config
  enabled: true
spec:
  default_window_size: 168
  min_data_points: 3
  z_score_trigger: 2.0
  direction_thresholds:
    up: 0.5
    down: 0.5
  severity_thresholds:
    notable: 2.0
    significant: 3.0
    critical: 4.0
"#).unwrap();

    let results = loader.load_all().unwrap();
    let loaded: Vec<_> = results
        .iter()
        .filter(|r| matches!(r.status, LoadStatus::Loaded { .. }))
        .collect();
    assert_eq!(loaded.len(), 2);

    // Both in documents map
    let docs = loader.documents();
    let guard = docs.read().unwrap();
    assert_eq!(guard.len(), 2);

    // Only anomaly rule in the backward-compat map
    let rules = loader.rules();
    let guard = rules.read().unwrap();
    assert_eq!(guard.len(), 1);
    assert!(guard.contains_key("test-rule"));
}

#[test]
fn write_and_read_back() {
    let (_dir, loader) = temp_loader();

    let rule: crate::schema::AnomalyRule = serde_yaml::from_str(VALID_RULE_YAML).unwrap();
    let path = loader.write_rule(&rule).unwrap();

    assert!(path.exists());
    assert!(path.file_name().unwrap().to_str().unwrap() == "test-rule.yml");

    // Read it back
    let loaded = loader.load_file(&path).unwrap();
    assert_eq!(loaded.metadata().id, rule.metadata.id);
    assert_eq!(loaded.metadata().name, rule.metadata.name);
}

#[test]
fn write_document_non_anomaly() {
    let (dir, loader) = temp_loader();

    let yaml = include_str!("../../../../data/rules/scoring/trend-config.yml");
    let rule: crate::trend_config::TrendConfigRule = serde_yaml::from_str(yaml).unwrap();
    let doc = RuleDocument::TrendConfig(rule);

    let path = loader.write_document(&doc).unwrap();
    assert!(path.exists());

    // Should be in documents but not in anomaly_rules
    let docs = loader.documents();
    let guard = docs.read().unwrap();
    assert!(guard.contains_key("trend-config-default"));

    let rules = loader.rules();
    let guard = rules.read().unwrap();
    assert!(!guard.contains_key("trend-config-default"));

    // Read back
    let loaded = loader.load_file(&path).unwrap();
    assert_eq!(loaded.kind(), crate::schema::RuleKind::TrendConfig);

    // Clean up
    let _ = fs::remove_file(dir.path().join("trend-config-default.yml"));
}

#[test]
fn delete_rule_removes_file_and_entry() {
    let (_dir, loader) = temp_loader();

    let rule: crate::schema::AnomalyRule = serde_yaml::from_str(VALID_RULE_YAML).unwrap();
    let path = loader.write_rule(&rule).unwrap();
    assert!(path.exists());

    loader.delete_rule("test-rule").unwrap();
    assert!(!path.exists());

    // Removed from both maps
    let rules = loader.rules();
    let guard = rules.read().unwrap();
    assert!(!guard.contains_key("test-rule"));

    let docs = loader.documents();
    let guard = docs.read().unwrap();
    assert!(!guard.contains_key("test-rule"));
}

#[test]
fn delete_nonexistent_rule_errors() {
    let (_dir, loader) = temp_loader();
    let err = loader.delete_rule("no-such-rule").unwrap_err();
    assert!(matches!(err, RuleError::Validation(_)));
}

#[test]
fn invalid_yaml_produces_error_not_panic() {
    let (dir, loader) = temp_loader();
    let bad_path = dir.path().join("bad.yml");
    fs::write(&bad_path, "this: is: not: valid: yaml: [[[").unwrap();

    let result = loader.load_file(&bad_path);
    assert!(result.is_err());
}

#[test]
fn empty_id_fails_validation() {
    let (dir, loader) = temp_loader();
    let yaml = r#"
apiVersion: v1
kind: AnomalyRule
metadata:
  id: ""
  name: Empty ID Rule
  enabled: true
schedule:
  cron: "* * * * *"
detection:
  template: spike
"#;
    let path = dir.path().join("empty-id.yml");
    fs::write(&path, yaml).unwrap();

    let result = loader.load_file(&path);
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), RuleError::Validation(_)));
}

#[test]
fn load_all_reports_failed_files() {
    let (dir, loader) = temp_loader();

    // One valid, one invalid
    fs::write(dir.path().join("good.yml"), VALID_RULE_YAML).unwrap();
    fs::write(dir.path().join("bad.yml"), "not valid yaml: [[[").unwrap();

    let results = loader.load_all().unwrap();

    let loaded = results
        .iter()
        .filter(|r| matches!(r.status, LoadStatus::Loaded { .. }))
        .count();
    let failed = results
        .iter()
        .filter(|r| matches!(r.status, LoadStatus::Failed { .. }))
        .count();

    assert_eq!(loaded, 1);
    assert_eq!(failed, 1);
}

#[test]
fn new_creates_missing_directory() {
    let dir = TempDir::new().unwrap();
    let nested = dir.path().join("sub").join("rules");
    assert!(!nested.exists());

    let _loader = RuleLoader::new(nested.clone());
    assert!(nested.exists());
}

// ── Deep-merge tests ────────────────────────────────────────────

#[test]
fn deep_merge_child_scalar_wins() {
    let parent: serde_yaml::Value = serde_yaml::from_str("a: 1\nb: 2").unwrap();
    let child: serde_yaml::Value = serde_yaml::from_str("b: 99").unwrap();
    let merged = deep_merge(&parent, &child);
    let m = merged.as_mapping().unwrap();
    assert_eq!(
        m.get(&serde_yaml::Value::String("a".into())).and_then(|v| v.as_i64()),
        Some(1)
    );
    assert_eq!(
        m.get(&serde_yaml::Value::String("b".into())).and_then(|v| v.as_i64()),
        Some(99)
    );
}

#[test]
fn deep_merge_nested_maps() {
    let parent: serde_yaml::Value =
        serde_yaml::from_str("spec:\n  a: 1\n  b: 2").unwrap();
    let child: serde_yaml::Value =
        serde_yaml::from_str("spec:\n  b: 99\n  c: 3").unwrap();
    let merged = deep_merge(&parent, &child);
    let spec = merged
        .as_mapping()
        .unwrap()
        .get(&serde_yaml::Value::String("spec".into()))
        .unwrap()
        .as_mapping()
        .unwrap();
    assert_eq!(spec.get(&serde_yaml::Value::String("a".into())).and_then(|v| v.as_i64()), Some(1));
    assert_eq!(spec.get(&serde_yaml::Value::String("b".into())).and_then(|v| v.as_i64()), Some(99));
    assert_eq!(spec.get(&serde_yaml::Value::String("c".into())).and_then(|v| v.as_i64()), Some(3));
}

#[test]
fn deep_merge_arrays_replace_entirely() {
    let parent: serde_yaml::Value =
        serde_yaml::from_str("tags:\n  - a\n  - b").unwrap();
    let child: serde_yaml::Value =
        serde_yaml::from_str("tags:\n  - x").unwrap();
    let merged = deep_merge(&parent, &child);
    let tags = merged
        .as_mapping()
        .unwrap()
        .get(&serde_yaml::Value::String("tags".into()))
        .unwrap()
        .as_sequence()
        .unwrap();
    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0].as_str(), Some("x"));
}

#[test]
fn resolve_extends_simple_chain() {
    let parent: serde_yaml::Value = serde_yaml::from_str(
        "metadata:\n  id: parent\n  name: Parent\n  enabled: true\nspec:\n  a: 1\n  b: 2"
    ).unwrap();
    let child: serde_yaml::Value = serde_yaml::from_str(
        "metadata:\n  id: child\n  name: Child\n  extends: parent\n  enabled: true\nspec:\n  b: 99"
    ).unwrap();

    let mut raw = HashMap::new();
    raw.insert("parent".to_string(), parent);
    raw.insert("child".to_string(), child);

    let resolved = resolve_extends(&raw).unwrap();
    let child_resolved = resolved.get("child").unwrap().as_mapping().unwrap();
    let spec = child_resolved
        .get(&serde_yaml::Value::String("spec".into()))
        .unwrap()
        .as_mapping()
        .unwrap();

    // Inherited from parent.
    assert_eq!(spec.get(&serde_yaml::Value::String("a".into())).and_then(|v| v.as_i64()), Some(1));
    // Overridden by child.
    assert_eq!(spec.get(&serde_yaml::Value::String("b".into())).and_then(|v| v.as_i64()), Some(99));
}

#[test]
fn resolve_extends_circular_detected() {
    let a: serde_yaml::Value = serde_yaml::from_str(
        "metadata:\n  id: a\n  name: A\n  extends: b\n  enabled: true"
    ).unwrap();
    let b: serde_yaml::Value = serde_yaml::from_str(
        "metadata:\n  id: b\n  name: B\n  extends: a\n  enabled: true"
    ).unwrap();

    let mut raw = HashMap::new();
    raw.insert("a".to_string(), a);
    raw.insert("b".to_string(), b);

    let result = resolve_extends(&raw);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("circular"));
}

#[test]
fn resolve_extends_missing_parent_errors() {
    let child: serde_yaml::Value = serde_yaml::from_str(
        "metadata:\n  id: child\n  name: Child\n  extends: nonexistent\n  enabled: true"
    ).unwrap();

    let mut raw = HashMap::new();
    raw.insert("child".to_string(), child);

    let result = resolve_extends(&raw);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("not found"));
}

#[test]
fn resolve_extends_no_extends_passthrough() {
    let rule: serde_yaml::Value = serde_yaml::from_str(
        "metadata:\n  id: standalone\n  name: Standalone\n  enabled: true\nspec:\n  x: 42"
    ).unwrap();

    let mut raw = HashMap::new();
    raw.insert("standalone".to_string(), rule.clone());

    let resolved = resolve_extends(&raw).unwrap();
    assert_eq!(resolved.get("standalone").unwrap(), &rule);
}
