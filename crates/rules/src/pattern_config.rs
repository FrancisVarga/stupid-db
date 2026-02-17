//! PatternConfig rule kind — PrefixSpan defaults and declarative
//! pattern classification rules.

use serde::{Deserialize, Serialize};

use crate::schema::CommonMetadata;

// ── YAML-level types ────────────────────────────────────────────────

/// Top-level PatternConfig rule document.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PatternConfigRule {
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    pub kind: String,
    pub metadata: CommonMetadata,
    pub spec: PatternConfigSpec,
}

/// Specification section of a PatternConfig rule.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PatternConfigSpec {
    /// PrefixSpan algorithm defaults.
    pub prefixspan_defaults: PrefixSpanDefaults,
    /// Declarative pattern classification rules (evaluated in order).
    pub classification_rules: Vec<ClassificationRule>,
}

/// Default parameters for the PrefixSpan algorithm.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PrefixSpanDefaults {
    /// Minimum support ratio (fraction of sequences containing the pattern).
    pub min_support: f64,
    /// Maximum pattern length.
    pub max_length: usize,
    /// Minimum number of member sequences required.
    pub min_members: usize,
}

/// A declarative classification rule for patterns.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ClassificationRule {
    /// Pattern category to assign (e.g., "ErrorChain", "Churn", "Funnel").
    pub category: String,
    /// Condition that must be met.
    pub condition: ClassificationCondition,
}

/// Condition for a classification rule.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ClassificationCondition {
    /// Check type: "count_gte", "sequence_match", "has_then_absent".
    pub check: String,
    /// Event code prefix to match (e.g., "E" for errors).
    #[serde(default)]
    pub event_code: Option<String>,
    /// Minimum count for "count_gte" checks.
    #[serde(default)]
    pub min_count: Option<usize>,
    /// Ordered event codes for "sequence_match" (e.g., ["L", "G"]).
    #[serde(default)]
    pub sequence: Option<Vec<String>>,
    /// Event code that must be present for "has_then_absent" checks.
    #[serde(default)]
    pub present_code: Option<String>,
    /// Event code that must be absent after `present_code`.
    #[serde(default)]
    pub absent_code: Option<String>,
}

// ── Compiled type ───────────────────────────────────────────────────

/// Pre-compiled pattern config (trivial — spec is already typed).
pub type CompiledPatternConfig = PatternConfigSpec;

impl PatternConfigRule {
    /// Compile the YAML config.
    pub fn compile(&self) -> CompiledPatternConfig {
        self.spec.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_pattern_config_yaml() {
        let yaml = include_str!("../../../data/rules/patterns/pattern-config.yml");
        let rule: PatternConfigRule = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(rule.kind, "PatternConfig");
        assert_eq!(rule.spec.prefixspan_defaults.min_support, 0.01);
        assert_eq!(rule.spec.prefixspan_defaults.max_length, 10);
        assert_eq!(rule.spec.prefixspan_defaults.min_members, 50);
    }

    #[test]
    fn classification_rules_order_preserved() {
        let yaml = include_str!("../../../data/rules/patterns/pattern-config.yml");
        let rule: PatternConfigRule = serde_yaml::from_str(yaml).unwrap();
        // ErrorChain should come first (highest priority).
        assert_eq!(rule.spec.classification_rules[0].category, "ErrorChain");
    }

    #[test]
    fn round_trip() {
        let yaml = include_str!("../../../data/rules/patterns/pattern-config.yml");
        let rule: PatternConfigRule = serde_yaml::from_str(yaml).unwrap();
        let serialized = serde_yaml::to_string(&rule).unwrap();
        let rule2: PatternConfigRule = serde_yaml::from_str(&serialized).unwrap();
        assert_eq!(rule, rule2);
    }
}
