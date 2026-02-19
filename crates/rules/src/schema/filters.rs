//! Post-detection filters for narrowing alert triggers.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Post-detection filters to narrow which entities trigger alerts.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Filters {
    #[serde(default)]
    pub entity_types: Option<Vec<String>>,
    #[serde(default)]
    pub classifications: Option<Vec<String>>,
    #[serde(default)]
    pub min_score: Option<f64>,
    #[serde(default)]
    pub exclude_keys: Option<Vec<String>>,
    #[serde(default, rename = "where")]
    pub conditions: Option<HashMap<String, FilterCondition>>,
}

/// Numeric comparison conditions for entity feature filtering.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FilterCondition {
    #[serde(default)]
    pub gt: Option<f64>,
    #[serde(default)]
    pub gte: Option<f64>,
    #[serde(default)]
    pub lt: Option<f64>,
    #[serde(default)]
    pub lte: Option<f64>,
    #[serde(default)]
    pub eq: Option<f64>,
    #[serde(default)]
    pub neq: Option<f64>,
}

impl FilterCondition {
    /// Check if a value passes all conditions.
    pub fn matches(&self, value: f64) -> bool {
        if let Some(v) = self.gt {
            if value <= v {
                return false;
            }
        }
        if let Some(v) = self.gte {
            if value < v {
                return false;
            }
        }
        if let Some(v) = self.lt {
            if value >= v {
                return false;
            }
        }
        if let Some(v) = self.lte {
            if value > v {
                return false;
            }
        }
        if let Some(v) = self.eq {
            if (value - v).abs() > f64::EPSILON {
                return false;
            }
        }
        if let Some(v) = self.neq {
            if (value - v).abs() <= f64::EPSILON {
                return false;
            }
        }
        true
    }
}
