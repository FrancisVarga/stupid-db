//! Schema and detection validation: apiVersion, kind, metadata, template params, composition.

use crate::schema::*;
use super::ValidationResult;
use super::filter_checks::validate_feature_name;
use super::fuzzy::is_kebab_case;

// ── Schema validation ───────────────────────────────────────────────

pub(super) fn validate_schema(rule: &AnomalyRule, result: &mut ValidationResult) {
    if rule.api_version != "v1" {
        result.error(
            "apiVersion",
            format!("apiVersion must be 'v1', got '{}'", rule.api_version),
        );
    }

    if rule.kind != "AnomalyRule" {
        result.error(
            "kind",
            format!("kind must be 'AnomalyRule', got '{}'", rule.kind),
        );
    }

    // metadata.id must be kebab-case
    if !is_kebab_case(&rule.metadata.id) {
        result.error(
            "metadata.id",
            format!(
                "id must be kebab-case (lowercase alphanumeric + hyphens), got '{}'",
                rule.metadata.id
            ),
        );
    }
}

// ── Detection validation ────────────────────────────────────────────

pub(super) fn validate_detection(rule: &AnomalyRule, result: &mut ValidationResult) {
    let det = &rule.detection;

    // Exactly one of template or compose
    match (&det.template, &det.compose) {
        (Some(_), Some(_)) => {
            result.error(
                "detection",
                "Exactly one of 'template' or 'compose' must be set, but both are present",
            );
        }
        (None, None) => {
            result.error(
                "detection",
                "Exactly one of 'template' or 'compose' must be set, but neither is present",
            );
        }
        (Some(template), None) => {
            validate_template_params(template, det, result);
        }
        (None, Some(composition)) => {
            validate_composition(composition, "detection.compose", result);
        }
    }
}

fn validate_template_params(
    template: &DetectionTemplate,
    det: &Detection,
    result: &mut ValidationResult,
) {
    match template {
        DetectionTemplate::Spike => {
            if let Some(parse_result) = det.parse_spike_params() {
                match parse_result {
                    Ok(params) => {
                        validate_feature_name(&params.feature, "detection.params.feature", result);
                    }
                    Err(e) => {
                        result.error(
                            "detection.params",
                            format!("Invalid spike params: {e}"),
                        );
                    }
                }
            } else {
                result.error("detection.params", "Spike template requires params");
            }
        }
        DetectionTemplate::Drift => {
            if let Some(parse_result) = det.parse_drift_params() {
                match parse_result {
                    Ok(params) => {
                        for (i, feat) in params.features.iter().enumerate() {
                            validate_feature_name(
                                feat,
                                &format!("detection.params.features[{i}]"),
                                result,
                            );
                        }
                    }
                    Err(e) => {
                        result.error(
                            "detection.params",
                            format!("Invalid drift params: {e}"),
                        );
                    }
                }
            } else {
                result.error("detection.params", "Drift template requires params");
            }
        }
        DetectionTemplate::Absence => {
            if let Some(parse_result) = det.parse_absence_params() {
                match parse_result {
                    Ok(params) => {
                        validate_feature_name(&params.feature, "detection.params.feature", result);
                    }
                    Err(e) => {
                        result.error(
                            "detection.params",
                            format!("Invalid absence params: {e}"),
                        );
                    }
                }
            } else {
                result.error("detection.params", "Absence template requires params");
            }
        }
        DetectionTemplate::Threshold => {
            if let Some(parse_result) = det.parse_threshold_params() {
                match parse_result {
                    Ok(params) => {
                        validate_feature_name(&params.feature, "detection.params.feature", result);
                    }
                    Err(e) => {
                        result.error(
                            "detection.params",
                            format!("Invalid threshold params: {e}"),
                        );
                    }
                }
            } else {
                result.error("detection.params", "Threshold template requires params");
            }
        }
    }
}

fn validate_composition(comp: &Composition, path: &str, result: &mut ValidationResult) {
    // NOT must have exactly 1 child
    if comp.operator == LogicalOperator::Not && comp.conditions.len() != 1 {
        result.error(
            path,
            format!(
                "NOT operator must have exactly 1 condition, got {}",
                comp.conditions.len()
            ),
        );
    }

    if comp.conditions.is_empty() {
        result.error(path, "Composition must have at least 1 condition");
    }

    for (i, cond) in comp.conditions.iter().enumerate() {
        match cond {
            Condition::Signal {
                feature, ..
            } => {
                // Signal type is already validated by serde enum deserialization.
                // Validate optional feature reference.
                if let Some(feat) = feature {
                    validate_feature_name(
                        feat,
                        &format!("{path}.conditions[{i}].feature"),
                        result,
                    );
                }
            }
            Condition::Nested(inner) => {
                validate_composition(inner, &format!("{path}.conditions[{i}]"), result);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validation::{validate_rule, validate_yaml};

    fn valid_rule() -> AnomalyRule {
        serde_yaml::from_str(
            r#"
apiVersion: v1
kind: AnomalyRule
metadata:
  id: test-rule
  name: Test Rule
  enabled: true
schedule:
  cron: "*/15 * * * *"
  timezone: UTC
detection:
  template: spike
  params:
    feature: login_count_7d
    multiplier: 3.0
notifications:
  - channel: webhook
    url: "https://hooks.example.com/alerts"
    on: [trigger]
"#,
        )
        .unwrap()
    }

    #[test]
    fn valid_rule_passes() {
        let result = validate_rule(&valid_rule());
        assert!(result.valid, "errors: {:?}", result.errors);
        assert!(result.errors.is_empty());
    }

    #[test]
    fn invalid_api_version() {
        let mut rule = valid_rule();
        rule.api_version = "v2".to_string();
        let result = validate_rule(&rule);
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.path == "apiVersion"));
    }

    #[test]
    fn invalid_kind() {
        let mut rule = valid_rule();
        rule.kind = "Alert".to_string();
        let result = validate_rule(&rule);
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.path == "kind"));
    }

    #[test]
    fn invalid_metadata_id_not_kebab() {
        let mut rule = valid_rule();
        rule.metadata.id = "TestRule".to_string();
        let result = validate_rule(&rule);
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.path == "metadata.id"));
    }

    #[test]
    fn both_template_and_compose() {
        let mut rule = valid_rule();
        rule.detection.compose = Some(Composition {
            operator: LogicalOperator::And,
            conditions: vec![],
        });
        let result = validate_rule(&rule);
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.path == "detection"));
    }

    #[test]
    fn neither_template_nor_compose() {
        let mut rule = valid_rule();
        rule.detection.template = None;
        rule.detection.params = None;
        let result = validate_rule(&rule);
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.path == "detection"));
    }

    #[test]
    fn invalid_feature_with_suggestion() {
        let result = validate_yaml(
            r#"
apiVersion: v1
kind: AnomalyRule
metadata:
  id: test-rule
  name: Test Rule
schedule:
  cron: "*/15 * * * *"
detection:
  template: spike
  params:
    feature: login_count
    multiplier: 3.0
notifications:
  - channel: webhook
    url: "https://example.com/hook"
"#,
        );
        assert!(!result.valid);
        let err = result
            .errors
            .iter()
            .find(|e| e.path == "detection.params.feature")
            .unwrap();
        assert!(err.suggestion.is_some());
        assert!(err.suggestion.as_deref().unwrap().contains("login_count_7d"));
    }

    #[test]
    fn not_operator_wrong_arity() {
        let result = validate_yaml(
            r#"
apiVersion: v1
kind: AnomalyRule
metadata:
  id: test-rule
  name: Test Rule
schedule:
  cron: "*/15 * * * *"
detection:
  compose:
    operator: not
    conditions:
      - signal: z_score
        threshold: 3.0
      - signal: dbscan_noise
        threshold: 0.5
notifications:
  - channel: webhook
    url: "https://example.com/hook"
"#,
        );
        assert!(!result.valid);
        assert!(result
            .errors
            .iter()
            .any(|e| e.message.contains("NOT") && e.message.contains("exactly 1")));
    }

    #[test]
    fn valid_compose_rule_passes() {
        let result = validate_yaml(
            r#"
apiVersion: v1
kind: AnomalyRule
metadata:
  id: multi-signal
  name: Multi Signal
schedule:
  cron: "*/30 * * * *"
detection:
  compose:
    operator: and
    conditions:
      - signal: z_score
        threshold: 3.0
      - signal: dbscan_noise
        threshold: 0.6
notifications:
  - channel: webhook
    url: "https://hooks.example.com/alerts"
"#,
        );
        assert!(result.valid, "errors: {:?}", result.errors);
    }
}
