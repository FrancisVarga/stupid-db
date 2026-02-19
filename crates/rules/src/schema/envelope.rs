//! Rule envelope for lightweight first-pass deserialization.

use serde::{Deserialize, Serialize};

use super::{CommonMetadata, RuleDocument, RuleKind};

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
                let rule: super::AnomalyRule =
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
