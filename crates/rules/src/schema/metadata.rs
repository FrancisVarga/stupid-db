//! Common metadata shared across all rule kinds.

use serde::{Deserialize, Serialize};

/// Shared metadata for all rule kinds (anomaly, entity schema, feature config, etc.).
///
/// The `extends` field enables rule inheritance: a child rule references a parent
/// by ID and deep-merges the parent's fields, with the child's values winning.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CommonMetadata {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Parent rule ID for inheritance. The loader deep-merges the parent's
    /// spec into this rule, with child fields taking precedence.
    #[serde(default)]
    pub extends: Option<String>,
}

/// Type alias for backward compatibility with existing code that references `RuleMetadata`.
pub type RuleMetadata = CommonMetadata;

pub(crate) fn default_true() -> bool {
    true
}
