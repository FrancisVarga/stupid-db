//! FeatureConfig rule kind — feature vector definition, encoding maps,
//! event classification keywords, and event compression codes.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::schema::CommonMetadata;

// ── YAML-level types ────────────────────────────────────────────────

/// Top-level FeatureConfig rule document.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FeatureConfigRule {
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    pub kind: String,
    pub metadata: CommonMetadata,
    pub spec: FeatureConfigSpec,
}

/// Specification section of a FeatureConfig rule.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FeatureConfigSpec {
    /// Ordered feature definitions (index = position in vector).
    pub features: Vec<FeatureDefinition>,
    /// VIP group → numeric encoding.
    pub vip_encoding: HashMap<String, f64>,
    /// Fallback strategy for unknown VIP groups.
    #[serde(default = "default_fallback")]
    pub vip_fallback: FallbackStrategy,
    /// Currency → numeric encoding.
    pub currency_encoding: HashMap<String, f64>,
    /// Fallback strategy for unknown currencies.
    #[serde(default = "default_fallback")]
    pub currency_fallback: FallbackStrategy,
    /// Event classification: category → list of substring keywords.
    pub event_classification: HashMap<String, Vec<String>>,
    /// Keywords that indicate a mobile platform.
    pub mobile_keywords: Vec<String>,
    /// Event type → compression code for PrefixSpan.
    pub event_compression: HashMap<String, EventCompressionRule>,
}

/// A single feature in the feature vector.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FeatureDefinition {
    /// Feature name (e.g., "login_count").
    pub name: String,
    /// Zero-based index in the feature vector.
    pub index: usize,
}

/// Fallback strategy for unknown encoding values.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum FallbackStrategy {
    /// Hash-based encoding: hash the string to a value in [0, 1).
    HashBased,
    /// Use a fixed default value.
    Default(f64),
}

fn default_fallback() -> FallbackStrategy {
    FallbackStrategy::HashBased
}

/// Event compression rule for PrefixSpan pattern mining.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EventCompressionRule {
    /// Compression code prefix (e.g., "L", "G", "P", "E").
    pub code: String,
    /// Optional subtype field to append (e.g., "game" → "G:slots").
    #[serde(default)]
    pub subtype_field: Option<String>,
}

// ── Compiled (hot-path) types ───────────────────────────────────────

/// Pre-compiled feature config for O(1) lookups.
#[derive(Debug, Clone)]
pub struct CompiledFeatureConfig {
    /// Feature name → index in the feature vector.
    pub feature_index: HashMap<String, usize>,
    /// Ordered feature names.
    pub feature_names: Vec<String>,
    /// VIP group encoding map (lowercased keys).
    pub vip_encoding: HashMap<String, f64>,
    pub vip_fallback: FallbackStrategy,
    /// Currency encoding map (uppercased keys).
    pub currency_encoding: HashMap<String, f64>,
    pub currency_fallback: FallbackStrategy,
    /// Event classification: category → keywords.
    pub event_classification: HashMap<String, Vec<String>>,
    /// Mobile keywords (lowercased).
    pub mobile_keywords: HashSet<String>,
    /// Event type → compression rule.
    pub event_compression: HashMap<String, EventCompressionRule>,
}

impl CompiledFeatureConfig {
    /// Number of features in the vector.
    pub fn feature_count(&self) -> usize {
        self.feature_names.len()
    }

    /// Look up a feature name's index.
    pub fn feature_index(&self, name: &str) -> Option<usize> {
        self.feature_index.get(name).copied()
    }
}

impl FeatureConfigRule {
    /// Compile the YAML config into optimized lookup structures.
    pub fn compile(&self) -> CompiledFeatureConfig {
        let mut feature_index = HashMap::new();
        let mut feature_names = Vec::new();

        let mut sorted_features = self.spec.features.clone();
        sorted_features.sort_by_key(|f| f.index);

        for feat in &sorted_features {
            feature_index.insert(feat.name.clone(), feat.index);
            feature_names.push(feat.name.clone());
        }

        let vip_encoding: HashMap<String, f64> = self
            .spec
            .vip_encoding
            .iter()
            .map(|(k, v)| (k.to_lowercase(), *v))
            .collect();

        let currency_encoding: HashMap<String, f64> = self
            .spec
            .currency_encoding
            .iter()
            .map(|(k, v)| (k.to_uppercase(), *v))
            .collect();

        let mobile_keywords: HashSet<String> = self
            .spec
            .mobile_keywords
            .iter()
            .map(|k| k.to_lowercase())
            .collect();

        CompiledFeatureConfig {
            feature_index,
            feature_names,
            vip_encoding,
            vip_fallback: self.spec.vip_fallback.clone(),
            currency_encoding,
            currency_fallback: self.spec.currency_fallback.clone(),
            event_classification: self.spec.event_classification.clone(),
            mobile_keywords,
            event_compression: self.spec.event_compression.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_feature_config_yaml() {
        let yaml = include_str!("../../../data/rules/features/feature-config.yml");
        let rule: FeatureConfigRule = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(rule.kind, "FeatureConfig");
        assert_eq!(rule.spec.features.len(), 10);
        assert!(!rule.spec.vip_encoding.is_empty());
        assert!(!rule.spec.currency_encoding.is_empty());
    }

    #[test]
    fn compile_feature_index() {
        let yaml = include_str!("../../../data/rules/features/feature-config.yml");
        let rule: FeatureConfigRule = serde_yaml::from_str(yaml).unwrap();
        let compiled = rule.compile();

        assert_eq!(compiled.feature_count(), 10);
        assert_eq!(compiled.feature_index("login_count"), Some(0));
        assert_eq!(compiled.feature_index("currency"), Some(9));
        assert_eq!(compiled.feature_index("nonexistent"), None);
    }

    #[test]
    fn compile_vip_encoding_case_insensitive() {
        let yaml = include_str!("../../../data/rules/features/feature-config.yml");
        let rule: FeatureConfigRule = serde_yaml::from_str(yaml).unwrap();
        let compiled = rule.compile();

        assert_eq!(compiled.vip_encoding.get("bronze"), Some(&1.0));
        assert_eq!(compiled.vip_encoding.get("vip"), Some(&6.0));
    }

    #[test]
    fn compile_mobile_keywords() {
        let yaml = include_str!("../../../data/rules/features/feature-config.yml");
        let rule: FeatureConfigRule = serde_yaml::from_str(yaml).unwrap();
        let compiled = rule.compile();

        assert!(compiled.mobile_keywords.contains("mobile"));
        assert!(compiled.mobile_keywords.contains("android"));
        assert!(compiled.mobile_keywords.contains("ios"));
    }

    #[test]
    fn round_trip() {
        let yaml = include_str!("../../../data/rules/features/feature-config.yml");
        let rule: FeatureConfigRule = serde_yaml::from_str(yaml).unwrap();
        let serialized = serde_yaml::to_string(&rule).unwrap();
        let rule2: FeatureConfigRule = serde_yaml::from_str(&serialized).unwrap();
        assert_eq!(rule, rule2);
    }
}
