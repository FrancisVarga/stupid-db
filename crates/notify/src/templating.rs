//! Minijinja template rendering for notification messages.
//!
//! Renders notification subject and body templates using minijinja,
//! with access to rule metadata, detection results, and entity context.
//!
//! Templates are arbitrary strings (not pre-registered), so a fresh
//! [`minijinja::Environment`] is created per render call.

use std::collections::HashMap;

use crate::traits::NotifyError;

/// Context data available to notification templates.
#[derive(Debug, Clone, serde::Serialize)]
pub struct TemplateContext {
    /// Rule metadata that triggered the notification.
    pub rule: RuleContext,
    /// Anomaly detection results.
    pub anomaly: AnomalyContext,
    /// Event type: `"trigger"` or `"resolve"`.
    pub event: String,
    /// Current timestamp in ISO 8601 format.
    pub now: String,
}

/// Rule metadata exposed to templates.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RuleContext {
    /// Unique rule identifier.
    pub id: String,
    /// Human-readable rule name.
    pub name: String,
    /// Optional description of the rule.
    pub description: Option<String>,
    /// Tags associated with the rule.
    pub tags: Vec<String>,
}

/// Anomaly detection results exposed to templates.
#[derive(Debug, Clone, serde::Serialize)]
pub struct AnomalyContext {
    /// Entity key that triggered the anomaly.
    pub key: String,
    /// Anomaly score (higher = more anomalous).
    pub score: f64,
    /// Classification label (e.g., `"high"`, `"critical"`).
    pub classification: String,
    /// Entity type (e.g., `"player"`, `"session"`).
    pub entity_type: String,
    /// Optional cluster assignment.
    pub cluster_id: Option<u64>,
    /// Signal names and their values.
    pub signals: Vec<(String, f64)>,
    /// Feature map used for detection.
    pub features: HashMap<String, f64>,
}

/// Renders notification templates using minijinja.
///
/// A fresh [`minijinja::Environment`] is created per render call since
/// templates are dynamic strings, not pre-registered files.
#[derive(Debug)]
pub struct TemplateRenderer {
    _private: (),
}

impl TemplateRenderer {
    /// Create a new template renderer.
    pub fn new() -> Self {
        Self { _private: () }
    }

    /// Build a configured minijinja environment with custom filters and globals.
    fn build_env() -> minijinja::Environment<'static> {
        let mut env = minijinja::Environment::new();

        // Register custom filters
        env.add_filter("round", round_filter);

        // `lower` and `upper` are built-in with the "builtins" feature,
        // but we register explicit versions to guarantee availability.
        env.add_filter("lower", lower_filter);
        env.add_filter("upper", upper_filter);

        // Register global `env()` function for environment variable access
        env.add_function("env", env_function);

        env
    }

    /// Render a template string with the given context.
    ///
    /// # Errors
    ///
    /// Returns [`NotifyError::Template`] if the template is invalid or
    /// rendering fails (e.g., type errors, undefined variables in strict mode).
    pub fn render(&self, template_str: &str, ctx: &TemplateContext) -> Result<String, NotifyError> {
        let env = Self::build_env();
        env.render_str(template_str, ctx)
            .map_err(|e| NotifyError::Template(e.to_string()))
    }

    /// Validate that a template string parses without errors.
    ///
    /// This does not evaluate the template — it only checks syntax.
    ///
    /// # Errors
    ///
    /// Returns [`NotifyError::Template`] if the template has syntax errors.
    pub fn validate(&self, template_str: &str) -> Result<(), NotifyError> {
        let env = Self::build_env();
        // Parse the template to check for syntax errors without evaluating.
        env.template_from_str(template_str)
            .map_err(|e| NotifyError::Template(e.to_string()))?;
        Ok(())
    }
}

impl Default for TemplateRenderer {
    fn default() -> Self {
        Self::new()
    }
}

/// Custom filter: round a float to N decimal places.
fn round_filter(value: f64, decimals: Option<u32>) -> String {
    let n = decimals.unwrap_or(0);
    format!("{:.prec$}", value, prec = n as usize)
}

/// Custom filter: lowercase a string.
fn lower_filter(value: String) -> String {
    value.to_lowercase()
}

/// Custom filter: uppercase a string.
fn upper_filter(value: String) -> String {
    value.to_uppercase()
}

/// Global function: read an environment variable by name.
///
/// Returns the variable value, or an empty string if not found
/// (with a warning logged via tracing).
fn env_function(name: String) -> String {
    match std::env::var(&name) {
        Ok(val) => val,
        Err(_) => {
            tracing::warn!(var = %name, "Environment variable not found, returning empty string");
            String::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a sample context for testing.
    fn sample_context() -> TemplateContext {
        let mut features = HashMap::new();
        features.insert("login_count".to_string(), 42.0);
        features.insert("avg_session".to_string(), 3.14159);

        TemplateContext {
            rule: RuleContext {
                id: "rule-001".to_string(),
                name: "High Login Frequency".to_string(),
                description: Some("Detects abnormal login patterns".to_string()),
                tags: vec!["security".to_string(), "login".to_string()],
            },
            anomaly: AnomalyContext {
                key: "player-12345".to_string(),
                score: 0.987654,
                classification: "Critical".to_string(),
                entity_type: "player".to_string(),
                cluster_id: Some(7),
                signals: vec![
                    ("rapid_login".to_string(), 0.95),
                    ("geo_anomaly".to_string(), 0.88),
                ],
                features,
            },
            event: "trigger".to_string(),
            now: "2026-02-16T12:00:00Z".to_string(),
        }
    }

    #[test]
    fn render_basic_template() {
        let renderer = TemplateRenderer::new();
        let ctx = sample_context();

        let template = "Alert: {{ rule.name }} triggered for {{ anomaly.key }}";
        let result = renderer.render(template, &ctx).unwrap();
        assert_eq!(result, "Alert: High Login Frequency triggered for player-12345");
    }

    #[test]
    fn render_nested_feature_access() {
        let renderer = TemplateRenderer::new();
        let ctx = sample_context();

        let template = "Login count: {{ anomaly.features.login_count }}";
        let result = renderer.render(template, &ctx).unwrap();
        assert_eq!(result, "Login count: 42.0");
    }

    #[test]
    fn render_round_filter() {
        let renderer = TemplateRenderer::new();
        let ctx = sample_context();

        let template = "Score: {{ anomaly.score | round(2) }}";
        let result = renderer.render(template, &ctx).unwrap();
        assert_eq!(result, "Score: 0.99");
    }

    #[test]
    fn render_round_filter_no_decimals() {
        let renderer = TemplateRenderer::new();
        let ctx = sample_context();

        let template = "Score: {{ anomaly.score | round }}";
        let result = renderer.render(template, &ctx).unwrap();
        assert_eq!(result, "Score: 1");
    }

    #[test]
    fn render_upper_lower_filters() {
        let renderer = TemplateRenderer::new();
        let ctx = sample_context();

        let upper_tpl = "{{ anomaly.classification | upper }}";
        let lower_tpl = "{{ anomaly.classification | lower }}";

        assert_eq!(renderer.render(upper_tpl, &ctx).unwrap(), "CRITICAL");
        assert_eq!(renderer.render(lower_tpl, &ctx).unwrap(), "critical");
    }

    #[test]
    fn render_env_function() {
        // Set a test env var
        std::env::set_var("STUPID_NOTIFY_TEST_VAR", "hello_notify");

        let renderer = TemplateRenderer::new();
        let ctx = sample_context();

        let template = "Env: {{ env('STUPID_NOTIFY_TEST_VAR') }}";
        let result = renderer.render(template, &ctx).unwrap();
        assert_eq!(result, "Env: hello_notify");

        std::env::remove_var("STUPID_NOTIFY_TEST_VAR");
    }

    #[test]
    fn render_env_missing_returns_empty() {
        let renderer = TemplateRenderer::new();
        let ctx = sample_context();

        let template = "Env: [{{ env('DEFINITELY_NOT_SET_XYZ') }}]";
        let result = renderer.render(template, &ctx).unwrap();
        assert_eq!(result, "Env: []");
    }

    #[test]
    fn invalid_template_produces_error() {
        let renderer = TemplateRenderer::new();
        let ctx = sample_context();

        let template = "{{ unclosed";
        let result = renderer.render(template, &ctx);
        assert!(result.is_err());

        match result.unwrap_err() {
            NotifyError::Template(msg) => {
                assert!(!msg.is_empty(), "Error message should not be empty");
            }
            other => panic!("Expected Template error, got: {:?}", other),
        }
    }

    #[test]
    fn validate_valid_template() {
        let renderer = TemplateRenderer::new();
        assert!(renderer.validate("Hello {{ rule.name }}").is_ok());
    }

    #[test]
    fn validate_invalid_template() {
        let renderer = TemplateRenderer::new();
        let result = renderer.validate("{{ unclosed");
        assert!(result.is_err());
    }

    #[test]
    fn render_optional_fields() {
        let renderer = TemplateRenderer::new();
        let mut ctx = sample_context();
        ctx.rule.description = None;
        ctx.anomaly.cluster_id = None;

        // None values should render without error
        let template = "Desc: {{ rule.description }}";
        let result = renderer.render(template, &ctx).unwrap();
        assert_eq!(result, "Desc: none");
    }

    #[test]
    fn render_event_and_timestamp() {
        let renderer = TemplateRenderer::new();
        let ctx = sample_context();

        let template = "[{{ now }}] Event: {{ event }}";
        let result = renderer.render(template, &ctx).unwrap();
        assert_eq!(result, "[2026-02-16T12:00:00Z] Event: trigger");
    }

    #[test]
    fn render_signals_iteration() {
        let renderer = TemplateRenderer::new();
        let ctx = sample_context();

        let template = "{% for s in anomaly.signals %}{{ s[0] }}={{ s[1] | round(1) }} {% endfor %}";
        let result = renderer.render(template, &ctx).unwrap();
        assert_eq!(result, "rapid_login=0.9 geo_anomaly=0.9 ");
    }

    #[test]
    fn render_tags_iteration() {
        let renderer = TemplateRenderer::new();
        let ctx = sample_context();

        let template = "Tags: {% for t in rule.tags %}{{ t }}{% if not loop.last %}, {% endif %}{% endfor %}";
        let result = renderer.render(template, &ctx).unwrap();
        assert_eq!(result, "Tags: security, login");
    }
}
