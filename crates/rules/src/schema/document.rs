//! Multi-kind rule document container and accessors.

use super::{AnomalyRule, CommonMetadata, RuleKind};

/// A fully deserialized rule of any supported kind.
#[derive(Debug, Clone, PartialEq)]
pub enum RuleDocument {
    /// Anomaly detection rule (spike, drift, absence, threshold, compose).
    Anomaly(AnomalyRule),
    /// Entity schema -- field mappings, extraction plans, embedding templates.
    EntitySchema(crate::entity_schema::EntitySchemaRule),
    /// Feature config -- feature vector, encodings, event classification.
    FeatureConfig(crate::feature_config::FeatureConfigRule),
    /// Scoring config -- anomaly weights, thresholds, graph params.
    ScoringConfig(crate::scoring_config::ScoringConfigRule),
    /// Trend config -- z-score thresholds, window defaults, severity.
    TrendConfig(crate::trend_config::TrendConfigRule),
    /// Pattern config -- PrefixSpan defaults, classification rules.
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

    /// Get mutable reference to the rule's metadata regardless of kind.
    pub fn metadata_mut(&mut self) -> &mut CommonMetadata {
        match self {
            RuleDocument::Anomaly(rule) => &mut rule.metadata,
            RuleDocument::EntitySchema(rule) => &mut rule.metadata,
            RuleDocument::FeatureConfig(rule) => &mut rule.metadata,
            RuleDocument::ScoringConfig(rule) => &mut rule.metadata,
            RuleDocument::TrendConfig(rule) => &mut rule.metadata,
            RuleDocument::PatternConfig(rule) => &mut rule.metadata,
        }
    }

    /// Serialize this document to JSON, delegating to the inner type.
    ///
    /// Each inner type already has `apiVersion`, `kind`, and `metadata` fields,
    /// so the JSON output naturally identifies the rule kind.
    pub fn to_json(&self) -> serde_json::Result<serde_json::Value> {
        match self {
            RuleDocument::Anomaly(r) => serde_json::to_value(r),
            RuleDocument::EntitySchema(r) => serde_json::to_value(r),
            RuleDocument::FeatureConfig(r) => serde_json::to_value(r),
            RuleDocument::ScoringConfig(r) => serde_json::to_value(r),
            RuleDocument::TrendConfig(r) => serde_json::to_value(r),
            RuleDocument::PatternConfig(r) => serde_json::to_value(r),
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
