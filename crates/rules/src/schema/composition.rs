//! Boolean signal composition types for combining detection signals.

use serde::{Deserialize, Serialize};

/// Boolean composition tree for combining detection signals.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Composition {
    pub operator: LogicalOperator,
    pub conditions: Vec<Condition>,
}

/// Logical operators for signal composition.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum LogicalOperator {
    And,
    Or,
    Not,
}

/// A condition leaf or nested composition node.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum Condition {
    /// A direct signal check against a threshold.
    Signal {
        signal: SignalType,
        #[serde(default)]
        feature: Option<String>,
        threshold: f64,
    },
    /// A nested composition for recursive boolean logic.
    Nested(Composition),
}

/// Signal types available for composition conditions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SignalType {
    ZScore,
    DbscanNoise,
    BehavioralDeviation,
    GraphAnomaly,
}
