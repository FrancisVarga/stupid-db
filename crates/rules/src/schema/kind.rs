//! Rule kind enum for two-pass deserialization dispatch.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// Supported rule kinds for two-pass deserialization dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RuleKind {
    AnomalyRule,
    EntitySchema,
    FeatureConfig,
    ScoringConfig,
    TrendConfig,
    PatternConfig,
}

impl fmt::Display for RuleKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RuleKind::AnomalyRule => write!(f, "AnomalyRule"),
            RuleKind::EntitySchema => write!(f, "EntitySchema"),
            RuleKind::FeatureConfig => write!(f, "FeatureConfig"),
            RuleKind::ScoringConfig => write!(f, "ScoringConfig"),
            RuleKind::TrendConfig => write!(f, "TrendConfig"),
            RuleKind::PatternConfig => write!(f, "PatternConfig"),
        }
    }
}

impl FromStr for RuleKind {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "AnomalyRule" => Ok(RuleKind::AnomalyRule),
            "EntitySchema" => Ok(RuleKind::EntitySchema),
            "FeatureConfig" => Ok(RuleKind::FeatureConfig),
            "ScoringConfig" => Ok(RuleKind::ScoringConfig),
            "TrendConfig" => Ok(RuleKind::TrendConfig),
            "PatternConfig" => Ok(RuleKind::PatternConfig),
            other => Err(format!("unknown rule kind: '{}'", other)),
        }
    }
}
