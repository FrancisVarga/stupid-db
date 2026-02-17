//! Validation for non-anomaly rule kinds: EntitySchema, FeatureConfig,
//! ScoringConfig, TrendConfig, PatternConfig.

use std::collections::HashSet;

use super::fuzzy::is_kebab_case;
use super::ValidationResult;

use crate::entity_schema::EntitySchemaRule;
use crate::feature_config::FeatureConfigRule;
use crate::pattern_config::PatternConfigRule;
use crate::scoring_config::ScoringConfigRule;
use crate::trend_config::TrendConfigRule;

// ── Common metadata validation ──────────────────────────────────────

fn validate_common_metadata(
    api_version: &str,
    kind: &str,
    expected_kind: &str,
    id: &str,
    result: &mut ValidationResult,
) {
    if api_version != "v1" {
        result.error(
            "apiVersion",
            format!("apiVersion must be 'v1', got '{}'", api_version),
        );
    }
    if kind != expected_kind {
        result.error(
            "kind",
            format!("kind must be '{}', got '{}'", expected_kind, kind),
        );
    }
    if !is_kebab_case(id) {
        result.error(
            "metadata.id",
            format!(
                "id must be kebab-case (lowercase alphanumeric + hyphens), got '{}'",
                id
            ),
        );
    }
}

// ── EntitySchema validation ─────────────────────────────────────────

pub fn validate_entity_schema(rule: &EntitySchemaRule, result: &mut ValidationResult) {
    validate_common_metadata(
        &rule.api_version,
        &rule.kind,
        "EntitySchema",
        &rule.metadata.id,
        result,
    );

    // Entity type names must be unique.
    let mut seen_types = HashSet::new();
    for et in &rule.spec.entity_types {
        if !seen_types.insert(&et.name) {
            result.error(
                "spec.entity_types",
                format!("duplicate entity type name '{}'", et.name),
            );
        }
    }

    // Edge types must reference valid entity types.
    let valid_types: HashSet<&str> = rule.spec.entity_types.iter().map(|et| et.name.as_str()).collect();
    for edge in &rule.spec.edge_types {
        if !valid_types.contains(edge.from.as_str()) {
            result.error(
                "spec.edge_types",
                format!(
                    "edge '{}' references unknown source entity type '{}'",
                    edge.name, edge.from
                ),
            );
        }
        if !valid_types.contains(edge.to.as_str()) {
            result.error(
                "spec.edge_types",
                format!(
                    "edge '{}' references unknown target entity type '{}'",
                    edge.name, edge.to
                ),
            );
        }
    }

    // Field mappings must reference valid entity types.
    for fm in &rule.spec.field_mappings {
        if !valid_types.contains(fm.entity_type.as_str()) {
            result.error(
                "spec.field_mappings",
                format!(
                    "field '{}' maps to unknown entity type '{}'",
                    fm.field, fm.entity_type
                ),
            );
        }
    }
}

// ── FeatureConfig validation ────────────────────────────────────────

pub fn validate_feature_config(rule: &FeatureConfigRule, result: &mut ValidationResult) {
    validate_common_metadata(
        &rule.api_version,
        &rule.kind,
        "FeatureConfig",
        &rule.metadata.id,
        result,
    );

    // Feature indices should be sequential starting from 0.
    let spec = &rule.spec;
    if !spec.features.is_empty() {
        let max_index = spec.features.iter().map(|f| f.index).max().unwrap_or(0);
        if max_index >= spec.features.len() {
            result.warn(
                "spec.features",
                format!(
                    "feature indices are not dense: max index {} but only {} features defined",
                    max_index,
                    spec.features.len()
                ),
            );
        }

        // Check for duplicate indices.
        let mut seen_indices = HashSet::new();
        for feat in &spec.features {
            if !seen_indices.insert(feat.index) {
                result.error(
                    "spec.features",
                    format!("duplicate feature index {} for '{}'", feat.index, feat.name),
                );
            }
        }
    }
}

// ── ScoringConfig validation ────────────────────────────────────────

pub fn validate_scoring_config(rule: &ScoringConfigRule, result: &mut ValidationResult) {
    validate_common_metadata(
        &rule.api_version,
        &rule.kind,
        "ScoringConfig",
        &rule.metadata.id,
        result,
    );

    let spec = &rule.spec;

    // Weights should sum to approximately 1.0.
    let weight_sum = spec.multi_signal_weights.statistical
        + spec.multi_signal_weights.dbscan_noise
        + spec.multi_signal_weights.behavioral
        + spec.multi_signal_weights.graph;
    if (weight_sum - 1.0).abs() > 0.01 {
        result.warn(
            "spec.multi_signal_weights",
            format!(
                "signal weights sum to {:.3} (expected ~1.0)",
                weight_sum
            ),
        );
    }

    // Classification thresholds must be ascending.
    let t = &spec.classification_thresholds;
    if !(t.mild <= t.anomalous && t.anomalous <= t.highly_anomalous) {
        result.error(
            "spec.classification_thresholds",
            format!(
                "thresholds must be ascending: mild({}) <= anomalous({}) <= highly_anomalous({})",
                t.mild, t.anomalous, t.highly_anomalous
            ),
        );
    }
}

// ── TrendConfig validation ──────────────────────────────────────────

pub fn validate_trend_config(rule: &TrendConfigRule, result: &mut ValidationResult) {
    validate_common_metadata(
        &rule.api_version,
        &rule.kind,
        "TrendConfig",
        &rule.metadata.id,
        result,
    );

    let spec = &rule.spec;

    // Severity thresholds must be ascending: notable < significant < critical.
    let s = &spec.severity_thresholds;
    if !(s.notable < s.significant && s.significant < s.critical) {
        result.error(
            "spec.severity_thresholds",
            format!(
                "severity thresholds must be ascending: notable({}) < significant({}) < critical({})",
                s.notable, s.significant, s.critical
            ),
        );
    }

    // min_data_points must be at least 2 for meaningful stddev.
    if spec.min_data_points < 2 {
        result.warn(
            "spec.min_data_points",
            "min_data_points should be at least 2 for meaningful standard deviation".to_string(),
        );
    }
}

// ── PatternConfig validation ────────────────────────────────────────

pub fn validate_pattern_config(rule: &PatternConfigRule, result: &mut ValidationResult) {
    validate_common_metadata(
        &rule.api_version,
        &rule.kind,
        "PatternConfig",
        &rule.metadata.id,
        result,
    );

    // Classification rules should have non-empty categories.
    for (i, cr) in rule.spec.classification_rules.iter().enumerate() {
        if cr.category.is_empty() {
            result.error(
                &format!("spec.classification_rules[{}].category", i),
                "classification rule must have a non-empty category".to_string(),
            );
        }
        if cr.condition.check.is_empty() {
            result.warn(
                &format!("spec.classification_rules[{}].condition", i),
                "classification rule has an empty check type".to_string(),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load_entity_schema() -> EntitySchemaRule {
        let yaml = include_str!("../../../../data/rules/schema/entity-schema.yml");
        serde_yaml::from_str(yaml).unwrap()
    }

    fn load_feature_config() -> FeatureConfigRule {
        let yaml = include_str!("../../../../data/rules/features/feature-config.yml");
        serde_yaml::from_str(yaml).unwrap()
    }

    fn load_scoring_config() -> ScoringConfigRule {
        let yaml = include_str!("../../../../data/rules/scoring/scoring-config.yml");
        serde_yaml::from_str(yaml).unwrap()
    }

    fn load_trend_config() -> TrendConfigRule {
        let yaml = include_str!("../../../../data/rules/scoring/trend-config.yml");
        serde_yaml::from_str(yaml).unwrap()
    }

    fn load_pattern_config() -> PatternConfigRule {
        let yaml = include_str!("../../../../data/rules/patterns/pattern-config.yml");
        serde_yaml::from_str(yaml).unwrap()
    }

    #[test]
    fn entity_schema_valid() {
        let rule = load_entity_schema();
        let mut result = ValidationResult::new();
        validate_entity_schema(&rule, &mut result);
        assert!(result.valid, "Default entity schema should be valid: {:?}", result.errors);
    }

    #[test]
    fn feature_config_valid() {
        let rule = load_feature_config();
        let mut result = ValidationResult::new();
        validate_feature_config(&rule, &mut result);
        assert!(result.valid, "Default feature config should be valid: {:?}", result.errors);
    }

    #[test]
    fn scoring_config_valid() {
        let rule = load_scoring_config();
        let mut result = ValidationResult::new();
        validate_scoring_config(&rule, &mut result);
        assert!(result.valid, "Default scoring config should be valid: {:?}", result.errors);
    }

    #[test]
    fn trend_config_valid() {
        let rule = load_trend_config();
        let mut result = ValidationResult::new();
        validate_trend_config(&rule, &mut result);
        assert!(result.valid, "Default trend config should be valid: {:?}", result.errors);
    }

    #[test]
    fn pattern_config_valid() {
        let rule = load_pattern_config();
        let mut result = ValidationResult::new();
        validate_pattern_config(&rule, &mut result);
        assert!(result.valid, "Default pattern config should be valid: {:?}", result.errors);
    }

    #[test]
    fn entity_schema_duplicate_type() {
        let mut rule = load_entity_schema();
        rule.spec.entity_types.push(crate::entity_schema::EntityTypeDef {
            name: "Member".to_string(),
            key_prefix: "member2:".to_string(),
        });
        let mut result = ValidationResult::new();
        validate_entity_schema(&rule, &mut result);
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.message.contains("duplicate")));
    }

    #[test]
    fn scoring_weights_warn_on_bad_sum() {
        let mut rule = load_scoring_config();
        rule.spec.multi_signal_weights.statistical = 0.5;
        let mut result = ValidationResult::new();
        validate_scoring_config(&rule, &mut result);
        // Should still be valid (warning, not error) but have a warning.
        assert!(result.warnings.iter().any(|w| w.message.contains("sum to")));
    }

    #[test]
    fn trend_config_bad_severity_order() {
        let mut rule = load_trend_config();
        // Swap notable and critical to make them out of order.
        rule.spec.severity_thresholds.notable = 5.0;
        rule.spec.severity_thresholds.critical = 1.0;
        let mut result = ValidationResult::new();
        validate_trend_config(&rule, &mut result);
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.message.contains("ascending")));
    }
}
