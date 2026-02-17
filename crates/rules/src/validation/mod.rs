//! Comprehensive rule validation with structured errors and suggestions.
//!
//! Validates all aspects of rule documents: AnomalyRule (schema, detection,
//! schedule, notifications, filters) and config rule kinds (EntitySchema,
//! FeatureConfig, ScoringConfig, TrendConfig, PatternConfig).
//! Returns a [`ValidationResult`] with errors (block save) and warnings (advisory).

pub(crate) mod config_checks;
mod filter_checks;
mod notification_checks;
mod schedule_checks;
mod schema_checks;

pub mod fuzzy;

use crate::schema::*;
use serde::{Deserialize, Serialize};

// ── Result types ────────────────────────────────────────────────────

/// Overall validation outcome.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    pub valid: bool,
    pub errors: Vec<ValidationError>,
    pub warnings: Vec<ValidationWarning>,
}

/// A blocking validation error.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationError {
    /// JSON-path-like location, e.g. `"detection.params.feature"`.
    pub path: String,
    pub message: String,
    /// Optional "Did you mean …?" suggestion.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<String>,
}

/// A non-blocking advisory warning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationWarning {
    pub path: String,
    pub message: String,
}

impl ValidationResult {
    pub(crate) fn new() -> Self {
        Self {
            valid: true,
            errors: Vec::new(),
            warnings: Vec::new(),
        }
    }

    pub(crate) fn error(&mut self, path: impl Into<String>, message: impl Into<String>) {
        self.valid = false;
        self.errors.push(ValidationError {
            path: path.into(),
            message: message.into(),
            suggestion: None,
        });
    }

    pub(crate) fn error_with_suggestion(
        &mut self,
        path: impl Into<String>,
        message: impl Into<String>,
        suggestion: impl Into<String>,
    ) {
        self.valid = false;
        self.errors.push(ValidationError {
            path: path.into(),
            message: message.into(),
            suggestion: Some(suggestion.into()),
        });
    }

    pub(crate) fn warn(&mut self, path: impl Into<String>, message: impl Into<String>) {
        self.warnings.push(ValidationWarning {
            path: path.into(),
            message: message.into(),
        });
    }
}

// ── Public API ──────────────────────────────────────────────────────

/// Validate a parsed [`AnomalyRule`].
pub fn validate_rule(rule: &AnomalyRule) -> ValidationResult {
    let mut result = ValidationResult::new();
    schema_checks::validate_schema(rule, &mut result);
    schema_checks::validate_detection(rule, &mut result);
    schedule_checks::validate_schedule(rule, &mut result);
    notification_checks::validate_notifications(rule, &mut result);
    filter_checks::validate_filters(rule, &mut result);
    result
}

/// Validate any [`RuleDocument`] variant, dispatching to the appropriate validator.
pub fn validate_document(doc: &RuleDocument) -> ValidationResult {
    let mut result = ValidationResult::new();
    match doc {
        RuleDocument::Anomaly(rule) => {
            schema_checks::validate_schema(rule, &mut result);
            schema_checks::validate_detection(rule, &mut result);
            schedule_checks::validate_schedule(rule, &mut result);
            notification_checks::validate_notifications(rule, &mut result);
            filter_checks::validate_filters(rule, &mut result);
        }
        RuleDocument::EntitySchema(rule) => {
            config_checks::validate_entity_schema(rule, &mut result);
        }
        RuleDocument::FeatureConfig(rule) => {
            config_checks::validate_feature_config(rule, &mut result);
        }
        RuleDocument::ScoringConfig(rule) => {
            config_checks::validate_scoring_config(rule, &mut result);
        }
        RuleDocument::TrendConfig(rule) => {
            config_checks::validate_trend_config(rule, &mut result);
        }
        RuleDocument::PatternConfig(rule) => {
            config_checks::validate_pattern_config(rule, &mut result);
        }
    }
    result
}

/// Parse raw YAML and validate. Returns parse errors merged with validation errors.
pub fn validate_yaml(yaml: &str) -> ValidationResult {
    match serde_yaml::from_str::<AnomalyRule>(yaml) {
        Ok(rule) => validate_rule(&rule),
        Err(e) => {
            let mut result = ValidationResult::new();
            result.error("", format!("YAML parse error: {e}"));
            result
        }
    }
}
