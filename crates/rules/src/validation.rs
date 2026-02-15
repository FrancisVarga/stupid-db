//! Comprehensive rule validation with structured errors and suggestions.
//!
//! Validates all aspects of an [`AnomalyRule`]: schema, detection, schedule,
//! notifications, and filters. Returns a [`ValidationResult`] with errors
//! (block save) and warnings (advisory).

use crate::schema::*;
use serde::{Deserialize, Serialize};

// ── Valid domain values ─────────────────────────────────────────────

/// The 10-dimensional feature vector used by the anomaly engine.
const VALID_FEATURES: &[&str] = &[
    "login_count_7d",
    "game_count_7d",
    "unique_games_7d",
    "error_count_7d",
    "popup_interaction_7d",
    "platform_mobile_ratio",
    "session_count_7d",
    "avg_session_gap_hours",
    "vip_group_numeric",
    "currency_encoded",
];

/// Valid signal names for composition conditions.
/// Kept as documentation — signal types are enforced by `SignalType` enum deserialization.
#[allow(dead_code)]
const VALID_SIGNALS: &[&str] = &[
    "z_score",
    "dbscan_noise",
    "behavioral_deviation",
    "graph_anomaly",
];

/// Valid entity types from `stupid-core`.
const VALID_ENTITY_TYPES: &[&str] = &[
    "Member", "Device", "Game", "Affiliate", "Currency", "VipGroup", "Error",
    "Platform", "Popup", "Provider",
];

/// Valid anomaly classifications from the compute engine.
const VALID_CLASSIFICATIONS: &[&str] = &[
    "Normal",
    "Mild",
    "Anomalous",
    "HighlyAnomalous",
];

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
    fn new() -> Self {
        Self {
            valid: true,
            errors: Vec::new(),
            warnings: Vec::new(),
        }
    }

    fn error(&mut self, path: impl Into<String>, message: impl Into<String>) {
        self.valid = false;
        self.errors.push(ValidationError {
            path: path.into(),
            message: message.into(),
            suggestion: None,
        });
    }

    fn error_with_suggestion(
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

    fn warn(&mut self, path: impl Into<String>, message: impl Into<String>) {
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
    validate_schema(rule, &mut result);
    validate_detection(rule, &mut result);
    validate_schedule(rule, &mut result);
    validate_notifications(rule, &mut result);
    validate_filters(rule, &mut result);
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

// ── 1. Schema validation ────────────────────────────────────────────

fn validate_schema(rule: &AnomalyRule, result: &mut ValidationResult) {
    if rule.api_version != "v1" {
        result.error(
            "apiVersion",
            format!("apiVersion must be 'v1', got '{}'", rule.api_version),
        );
    }

    if rule.kind != "AnomalyRule" {
        result.error(
            "kind",
            format!("kind must be 'AnomalyRule', got '{}'", rule.kind),
        );
    }

    // metadata.id must be kebab-case
    if !is_kebab_case(&rule.metadata.id) {
        result.error(
            "metadata.id",
            format!(
                "id must be kebab-case (lowercase alphanumeric + hyphens), got '{}'",
                rule.metadata.id
            ),
        );
    }
}

// ── 2. Detection validation ─────────────────────────────────────────

fn validate_detection(rule: &AnomalyRule, result: &mut ValidationResult) {
    let det = &rule.detection;

    // Exactly one of template or compose
    match (&det.template, &det.compose) {
        (Some(_), Some(_)) => {
            result.error(
                "detection",
                "Exactly one of 'template' or 'compose' must be set, but both are present",
            );
        }
        (None, None) => {
            result.error(
                "detection",
                "Exactly one of 'template' or 'compose' must be set, but neither is present",
            );
        }
        (Some(template), None) => {
            validate_template_params(template, det, result);
        }
        (None, Some(composition)) => {
            validate_composition(composition, "detection.compose", result);
        }
    }
}

fn validate_template_params(
    template: &DetectionTemplate,
    det: &Detection,
    result: &mut ValidationResult,
) {
    match template {
        DetectionTemplate::Spike => {
            if let Some(parse_result) = det.parse_spike_params() {
                match parse_result {
                    Ok(params) => {
                        validate_feature_name(&params.feature, "detection.params.feature", result);
                    }
                    Err(e) => {
                        result.error(
                            "detection.params",
                            format!("Invalid spike params: {e}"),
                        );
                    }
                }
            } else {
                result.error("detection.params", "Spike template requires params");
            }
        }
        DetectionTemplate::Drift => {
            if let Some(parse_result) = det.parse_drift_params() {
                match parse_result {
                    Ok(params) => {
                        for (i, feat) in params.features.iter().enumerate() {
                            validate_feature_name(
                                feat,
                                &format!("detection.params.features[{i}]"),
                                result,
                            );
                        }
                    }
                    Err(e) => {
                        result.error(
                            "detection.params",
                            format!("Invalid drift params: {e}"),
                        );
                    }
                }
            } else {
                result.error("detection.params", "Drift template requires params");
            }
        }
        DetectionTemplate::Absence => {
            if let Some(parse_result) = det.parse_absence_params() {
                match parse_result {
                    Ok(params) => {
                        validate_feature_name(&params.feature, "detection.params.feature", result);
                    }
                    Err(e) => {
                        result.error(
                            "detection.params",
                            format!("Invalid absence params: {e}"),
                        );
                    }
                }
            } else {
                result.error("detection.params", "Absence template requires params");
            }
        }
        DetectionTemplate::Threshold => {
            if let Some(parse_result) = det.parse_threshold_params() {
                match parse_result {
                    Ok(params) => {
                        validate_feature_name(&params.feature, "detection.params.feature", result);
                    }
                    Err(e) => {
                        result.error(
                            "detection.params",
                            format!("Invalid threshold params: {e}"),
                        );
                    }
                }
            } else {
                result.error("detection.params", "Threshold template requires params");
            }
        }
    }
}

fn validate_composition(comp: &Composition, path: &str, result: &mut ValidationResult) {
    // NOT must have exactly 1 child
    if comp.operator == LogicalOperator::Not && comp.conditions.len() != 1 {
        result.error(
            path,
            format!(
                "NOT operator must have exactly 1 condition, got {}",
                comp.conditions.len()
            ),
        );
    }

    if comp.conditions.is_empty() {
        result.error(path, "Composition must have at least 1 condition");
    }

    for (i, cond) in comp.conditions.iter().enumerate() {
        match cond {
            Condition::Signal {
                feature, ..
            } => {
                // Signal type is already validated by serde enum deserialization.
                // Validate optional feature reference.
                if let Some(feat) = feature {
                    validate_feature_name(
                        feat,
                        &format!("{path}.conditions[{i}].feature"),
                        result,
                    );
                }
            }
            Condition::Nested(inner) => {
                validate_composition(inner, &format!("{path}.conditions[{i}]"), result);
            }
        }
    }
}

// ── 3. Schedule validation ──────────────────────────────────────────

fn validate_schedule(rule: &AnomalyRule, result: &mut ValidationResult) {
    let sched = &rule.schedule;

    // Validate 5-field cron
    validate_cron(&sched.cron, result);

    // Validate timezone (basic check for IANA format)
    validate_timezone(&sched.timezone, result);

    // Validate cooldown duration if present
    if let Some(cooldown) = &sched.cooldown {
        if parse_duration(cooldown).is_none() {
            result.error(
                "schedule.cooldown",
                format!("Invalid duration format '{}', expected e.g. '30m', '1h', '2h30m'", cooldown),
            );
        }
    }
}

fn validate_cron(expr: &str, result: &mut ValidationResult) {
    let fields: Vec<&str> = expr.split_whitespace().collect();
    if fields.len() != 5 {
        result.error(
            "schedule.cron",
            format!(
                "Cron must have exactly 5 fields (min hour dom month dow), got {}",
                fields.len()
            ),
        );
        return;
    }

    // Validate each field against its range
    let ranges: &[(&str, u32, u32)] = &[
        ("minute", 0, 59),
        ("hour", 0, 23),
        ("day-of-month", 1, 31),
        ("month", 1, 12),
        ("day-of-week", 0, 7),
    ];

    for (field, (name, min, max)) in fields.iter().zip(ranges.iter()) {
        if !validate_cron_field(field, *min, *max) {
            result.error(
                "schedule.cron",
                format!("Invalid cron {name} field: '{field}'"),
            );
        }
    }

    // Check minimum interval: cron must not be more frequent than every 1 minute.
    // Every-second patterns aren't possible in 5-field cron, but `* * * * *` (every minute) is ok.
    // We only warn on sub-minute which isn't representable — so nothing to block here.
}

/// Basic cron field validation: supports *, N, N-M, */N, N-M/N, and comma-separated.
fn validate_cron_field(field: &str, min: u32, max: u32) -> bool {
    for part in field.split(',') {
        let part = part.trim();
        if part.is_empty() {
            return false;
        }

        // Split on / for step
        let (range_part, step) = if let Some((r, s)) = part.split_once('/') {
            match s.parse::<u32>() {
                Ok(v) if v > 0 => (r, Some(v)),
                _ => return false,
            }
        } else {
            (part, None)
        };

        if range_part == "*" {
            // Valid: * or */N
            if let Some(s) = step {
                if s > max {
                    return false;
                }
            }
            continue;
        }

        // Range: N-M or single value N
        if let Some((start_s, end_s)) = range_part.split_once('-') {
            match (start_s.parse::<u32>(), end_s.parse::<u32>()) {
                (Ok(s), Ok(e)) if s >= min && e <= max && s <= e => {}
                _ => return false,
            }
        } else {
            match range_part.parse::<u32>() {
                Ok(v) if v >= min && v <= max => {}
                _ => return false,
            }
        }

        let _ = step; // step is valid if we got here
    }
    true
}

fn validate_timezone(tz: &str, result: &mut ValidationResult) {
    // Accept "UTC" and IANA-style "Area/Location" (e.g., "Asia/Manila")
    if tz == "UTC" || tz == "GMT" {
        return;
    }
    if !is_iana_timezone(tz) {
        result.error(
            "schedule.timezone",
            format!("Invalid timezone '{}', expected IANA format (e.g., 'Asia/Manila')", tz),
        );
    }
}

/// Check if a string is valid kebab-case: `^[a-z0-9]+(-[a-z0-9]+)*$`
fn is_kebab_case(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    let mut prev_was_hyphen = true; // treat start as "after separator" to require leading alnum
    for ch in s.chars() {
        if ch == '-' {
            if prev_was_hyphen {
                return false; // double hyphen or leading hyphen
            }
            prev_was_hyphen = true;
        } else if ch.is_ascii_lowercase() || ch.is_ascii_digit() {
            prev_was_hyphen = false;
        } else {
            return false; // uppercase, special chars, etc.
        }
    }
    !prev_was_hyphen // must not end with hyphen
}

/// Basic IANA timezone validation: `Area/Location` with uppercase start per segment.
fn is_iana_timezone(tz: &str) -> bool {
    let parts: Vec<&str> = tz.split('/').collect();
    if parts.len() < 2 {
        return false;
    }
    for part in &parts {
        if part.is_empty() {
            return false;
        }
        let first = part.chars().next().unwrap();
        if !first.is_ascii_uppercase() {
            return false;
        }
        if !part.chars().all(|c| c.is_ascii_alphabetic() || c == '_') {
            return false;
        }
    }
    true
}

/// Parse human-friendly durations like "30m", "1h", "2h30m".
fn parse_duration(s: &str) -> Option<std::time::Duration> {
    let mut total_secs: u64 = 0;
    let mut num_buf = String::new();
    let mut has_unit = false;

    for ch in s.chars() {
        if ch.is_ascii_digit() {
            num_buf.push(ch);
        } else {
            let n: u64 = num_buf.parse().ok()?;
            num_buf.clear();
            match ch {
                'h' => total_secs += n * 3600,
                'm' => total_secs += n * 60,
                's' => total_secs += n,
                'd' => total_secs += n * 86400,
                _ => return None,
            }
            has_unit = true;
        }
    }

    if !num_buf.is_empty() {
        // Trailing digits with no unit
        return None;
    }

    if has_unit && total_secs > 0 {
        Some(std::time::Duration::from_secs(total_secs))
    } else {
        None
    }
}

// ── 4. Notification validation ──────────────────────────────────────

fn validate_notifications(rule: &AnomalyRule, result: &mut ValidationResult) {
    if rule.notifications.is_empty() {
        result.error("notifications", "At least one notification channel must be configured");
        return;
    }

    for (i, notif) in rule.notifications.iter().enumerate() {
        let path = format!("notifications[{i}]");
        match notif.channel {
            ChannelType::Webhook => {
                if notif.url.is_none() {
                    result.error(format!("{path}.url"), "Webhook channel requires 'url'");
                } else if let Some(url) = &notif.url {
                    if !url.starts_with("http://") && !url.starts_with("https://") {
                        result.error(
                            format!("{path}.url"),
                            format!("URL must start with http:// or https://, got '{url}'"),
                        );
                    }
                }
            }
            ChannelType::Email => {
                if notif.smtp_host.is_none() {
                    result.error(format!("{path}.smtp_host"), "Email channel requires 'smtp_host'");
                }
                if notif.smtp_port.is_none() {
                    result.error(format!("{path}.smtp_port"), "Email channel requires 'smtp_port'");
                }
                if notif.from.is_none() {
                    result.error(format!("{path}.from"), "Email channel requires 'from'");
                }
                if notif.to.is_none() {
                    result.error(format!("{path}.to"), "Email channel requires 'to'");
                }
            }
            ChannelType::Telegram => {
                if notif.bot_token.is_none() {
                    result.error(format!("{path}.bot_token"), "Telegram channel requires 'bot_token'");
                }
                if notif.chat_id.is_none() {
                    result.error(format!("{path}.chat_id"), "Telegram channel requires 'chat_id'");
                }
            }
        }

        // Check for raw secrets (not using ${ENV_VAR} syntax)
        check_secret_value(&notif.bot_token, &format!("{path}.bot_token"), result);
        check_secret_value(&notif.url, &format!("{path}.url"), result);
    }
}

/// Warn if a value looks like a raw secret instead of `${ENV_VAR}` reference.
fn check_secret_value(value: &Option<String>, path: &str, result: &mut ValidationResult) {
    if let Some(v) = value {
        if !v.is_empty() && !v.starts_with("${") && looks_like_secret(v) {
            result.warn(
                path,
                format!("Value looks like a raw secret. Consider using '${{ENV_VAR}}' syntax instead"),
            );
        }
    }
}

/// Heuristic: a value "looks like" a secret if it's long enough with mixed chars
/// and doesn't look like a normal URL or text.
fn looks_like_secret(v: &str) -> bool {
    // Skip URLs — they have structured format
    if v.starts_with("http://") || v.starts_with("https://") {
        return false;
    }
    // Token-like strings: 20+ chars with mix of alpha/digit
    if v.len() >= 20 {
        let has_alpha = v.chars().any(|c| c.is_ascii_alphabetic());
        let has_digit = v.chars().any(|c| c.is_ascii_digit());
        let has_special = v.chars().any(|c| ":_-".contains(c));
        return has_alpha && has_digit && has_special;
    }
    false
}

// ── 5. Filter validation ────────────────────────────────────────────

fn validate_filters(rule: &AnomalyRule, result: &mut ValidationResult) {
    let filters = match &rule.filters {
        Some(f) => f,
        None => return,
    };

    // Validate entity types
    if let Some(types) = &filters.entity_types {
        for (i, t) in types.iter().enumerate() {
            if !VALID_ENTITY_TYPES.contains(&t.as_str()) {
                let suggestion = fuzzy_match(t, VALID_ENTITY_TYPES);
                let path = format!("filters.entity_types[{i}]");
                if let Some(s) = suggestion {
                    result.error_with_suggestion(
                        &path,
                        format!("Unknown entity type '{t}'"),
                        format!("Did you mean '{s}'?"),
                    );
                } else {
                    result.error(&path, format!("Unknown entity type '{t}'"));
                }
            }
        }
    }

    // Validate classifications
    if let Some(classes) = &filters.classifications {
        for (i, c) in classes.iter().enumerate() {
            if !VALID_CLASSIFICATIONS.contains(&c.as_str()) {
                let suggestion = fuzzy_match(c, VALID_CLASSIFICATIONS);
                let path = format!("filters.classifications[{i}]");
                if let Some(s) = suggestion {
                    result.error_with_suggestion(
                        &path,
                        format!("Unknown classification '{c}'"),
                        format!("Did you mean '{s}'?"),
                    );
                } else {
                    result.error(&path, format!("Unknown classification '{c}'"));
                }
            }
        }
    }

    // Validate min_score range
    if let Some(score) = filters.min_score {
        if !(0.0..=1.0).contains(&score) {
            result.error(
                "filters.min_score",
                format!("min_score must be between 0.0 and 1.0, got {score}"),
            );
        }
    }

    // Validate where-clause feature references
    if let Some(conditions) = &filters.conditions {
        for key in conditions.keys() {
            validate_feature_name(key, &format!("filters.where.{key}"), result);
        }
    }
}

// ── Fuzzy matching ──────────────────────────────────────────────────

/// Validate a feature name against the known 10-dimensional vector.
fn validate_feature_name(name: &str, path: &str, result: &mut ValidationResult) {
    if !VALID_FEATURES.contains(&name) {
        let suggestion = fuzzy_match(name, VALID_FEATURES);
        if let Some(s) = suggestion {
            result.error_with_suggestion(
                path,
                format!("Unknown feature '{name}'"),
                format!("Did you mean '{s}'?"),
            );
        } else {
            result.error(path, format!("Unknown feature '{name}'"));
        }
    }
}

/// Find the closest match using Levenshtein distance. Returns None if best
/// distance exceeds half the candidate length (too dissimilar).
fn fuzzy_match<'a>(input: &str, candidates: &[&'a str]) -> Option<&'a str> {
    let input_lower = input.to_lowercase();
    let mut best: Option<(&str, usize)> = None;

    for &candidate in candidates {
        let dist = levenshtein(&input_lower, &candidate.to_lowercase());
        match best {
            None => best = Some((candidate, dist)),
            Some((_, best_dist)) if dist < best_dist => best = Some((candidate, dist)),
            _ => {}
        }
    }

    best.and_then(|(name, dist)| {
        // Only suggest if edit distance is reasonable (≤ half the longer string)
        let max_len = input.len().max(name.len());
        if dist <= max_len / 2 {
            Some(name)
        } else {
            None
        }
    })
}

/// Levenshtein edit distance between two strings.
fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let m = a.len();
    let n = b.len();

    let mut prev = (0..=n).collect::<Vec<_>>();
    let mut curr = vec![0; n + 1];

    for i in 1..=m {
        curr[0] = i;
        for j in 1..=n {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            curr[j] = (prev[j] + 1)
                .min(curr[j - 1] + 1)
                .min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }

    prev[n]
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_rule() -> AnomalyRule {
        serde_yaml::from_str(
            r#"
apiVersion: v1
kind: AnomalyRule
metadata:
  id: test-rule
  name: Test Rule
  enabled: true
schedule:
  cron: "*/15 * * * *"
  timezone: UTC
detection:
  template: spike
  params:
    feature: login_count_7d
    multiplier: 3.0
notifications:
  - channel: webhook
    url: "https://hooks.example.com/alerts"
    on: [trigger]
"#,
        )
        .unwrap()
    }

    #[test]
    fn valid_rule_passes() {
        let result = validate_rule(&valid_rule());
        assert!(result.valid, "errors: {:?}", result.errors);
        assert!(result.errors.is_empty());
    }

    #[test]
    fn invalid_api_version() {
        let mut rule = valid_rule();
        rule.api_version = "v2".to_string();
        let result = validate_rule(&rule);
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.path == "apiVersion"));
    }

    #[test]
    fn invalid_kind() {
        let mut rule = valid_rule();
        rule.kind = "Alert".to_string();
        let result = validate_rule(&rule);
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.path == "kind"));
    }

    #[test]
    fn invalid_metadata_id_not_kebab() {
        let mut rule = valid_rule();
        rule.metadata.id = "TestRule".to_string();
        let result = validate_rule(&rule);
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.path == "metadata.id"));
    }

    #[test]
    fn both_template_and_compose() {
        let mut rule = valid_rule();
        rule.detection.compose = Some(Composition {
            operator: LogicalOperator::And,
            conditions: vec![],
        });
        let result = validate_rule(&rule);
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.path == "detection"));
    }

    #[test]
    fn neither_template_nor_compose() {
        let mut rule = valid_rule();
        rule.detection.template = None;
        rule.detection.params = None;
        let result = validate_rule(&rule);
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.path == "detection"));
    }

    #[test]
    fn invalid_feature_with_suggestion() {
        let result = validate_yaml(
            r#"
apiVersion: v1
kind: AnomalyRule
metadata:
  id: test-rule
  name: Test Rule
schedule:
  cron: "*/15 * * * *"
detection:
  template: spike
  params:
    feature: login_count
    multiplier: 3.0
notifications:
  - channel: webhook
    url: "https://example.com/hook"
"#,
        );
        assert!(!result.valid);
        let err = result
            .errors
            .iter()
            .find(|e| e.path == "detection.params.feature")
            .unwrap();
        assert!(err.suggestion.is_some());
        assert!(err.suggestion.as_deref().unwrap().contains("login_count_7d"));
    }

    #[test]
    fn not_operator_wrong_arity() {
        let result = validate_yaml(
            r#"
apiVersion: v1
kind: AnomalyRule
metadata:
  id: test-rule
  name: Test Rule
schedule:
  cron: "*/15 * * * *"
detection:
  compose:
    operator: not
    conditions:
      - signal: z_score
        threshold: 3.0
      - signal: dbscan_noise
        threshold: 0.5
notifications:
  - channel: webhook
    url: "https://example.com/hook"
"#,
        );
        assert!(!result.valid);
        assert!(result
            .errors
            .iter()
            .any(|e| e.message.contains("NOT") && e.message.contains("exactly 1")));
    }

    #[test]
    fn invalid_cron_fields() {
        let mut rule = valid_rule();
        rule.schedule.cron = "*/15 * * *".to_string(); // only 4 fields
        let result = validate_rule(&rule);
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.path == "schedule.cron"));
    }

    #[test]
    fn invalid_timezone() {
        let mut rule = valid_rule();
        rule.schedule.timezone = "not_a_timezone".to_string();
        let result = validate_rule(&rule);
        assert!(!result.valid);
        assert!(result
            .errors
            .iter()
            .any(|e| e.path == "schedule.timezone"));
    }

    #[test]
    fn invalid_cooldown() {
        let mut rule = valid_rule();
        rule.schedule.cooldown = Some("banana".to_string());
        let result = validate_rule(&rule);
        assert!(!result.valid);
        assert!(result
            .errors
            .iter()
            .any(|e| e.path == "schedule.cooldown"));
    }

    #[test]
    fn no_notifications() {
        let mut rule = valid_rule();
        rule.notifications.clear();
        let result = validate_rule(&rule);
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.path == "notifications"));
    }

    #[test]
    fn email_missing_fields() {
        let result = validate_yaml(
            r#"
apiVersion: v1
kind: AnomalyRule
metadata:
  id: test-rule
  name: Test Rule
schedule:
  cron: "*/15 * * * *"
detection:
  template: spike
  params:
    feature: login_count_7d
    multiplier: 3.0
notifications:
  - channel: email
"#,
        );
        assert!(!result.valid);
        assert!(result
            .errors
            .iter()
            .any(|e| e.path.contains("smtp_host")));
        assert!(result.errors.iter().any(|e| e.path.contains("from")));
        assert!(result.errors.iter().any(|e| e.path.contains("to")));
    }

    #[test]
    fn telegram_missing_fields() {
        let result = validate_yaml(
            r#"
apiVersion: v1
kind: AnomalyRule
metadata:
  id: test-rule
  name: Test Rule
schedule:
  cron: "*/15 * * * *"
detection:
  template: spike
  params:
    feature: login_count_7d
    multiplier: 3.0
notifications:
  - channel: telegram
"#,
        );
        assert!(!result.valid);
        assert!(result
            .errors
            .iter()
            .any(|e| e.path.contains("bot_token")));
        assert!(result
            .errors
            .iter()
            .any(|e| e.path.contains("chat_id")));
    }

    #[test]
    fn secret_detection_warning() {
        let result = validate_yaml(
            r#"
apiVersion: v1
kind: AnomalyRule
metadata:
  id: test-rule
  name: Test Rule
schedule:
  cron: "*/15 * * * *"
detection:
  template: spike
  params:
    feature: login_count_7d
    multiplier: 3.0
notifications:
  - channel: telegram
    bot_token: "1234567890:ABCdefGHIjklMNOpqrSTUvwxyz"
    chat_id: "-100123456"
"#,
        );
        assert!(result.warnings.iter().any(|w| w.path.contains("bot_token")));
    }

    #[test]
    fn filter_invalid_entity_type() {
        let mut rule = valid_rule();
        rule.filters = Some(Filters {
            entity_types: Some(vec!["Memer".to_string()]),
            classifications: None,
            min_score: None,
            exclude_keys: None,
            conditions: None,
        });
        let result = validate_rule(&rule);
        assert!(!result.valid);
        let err = result
            .errors
            .iter()
            .find(|e| e.path.contains("entity_types"))
            .unwrap();
        assert!(err.suggestion.as_deref().unwrap().contains("Member"));
    }

    #[test]
    fn filter_min_score_out_of_range() {
        let mut rule = valid_rule();
        rule.filters = Some(Filters {
            entity_types: None,
            classifications: None,
            min_score: Some(1.5),
            exclude_keys: None,
            conditions: None,
        });
        let result = validate_rule(&rule);
        assert!(!result.valid);
        assert!(result
            .errors
            .iter()
            .any(|e| e.path == "filters.min_score"));
    }

    #[test]
    fn filter_where_invalid_feature() {
        let mut rule = valid_rule();
        let mut conds = std::collections::HashMap::new();
        conds.insert(
            "login_freq".to_string(),
            FilterCondition {
                gt: Some(10.0),
                gte: None,
                lt: None,
                lte: None,
                eq: None,
                neq: None,
            },
        );
        rule.filters = Some(Filters {
            entity_types: None,
            classifications: None,
            min_score: None,
            exclude_keys: None,
            conditions: Some(conds),
        });
        let result = validate_rule(&rule);
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.path.contains("filters.where")));
    }

    #[test]
    fn levenshtein_basic() {
        assert_eq!(levenshtein("kitten", "sitting"), 3);
        assert_eq!(levenshtein("", "abc"), 3);
        assert_eq!(levenshtein("abc", "abc"), 0);
    }

    #[test]
    fn fuzzy_match_finds_close() {
        assert_eq!(fuzzy_match("login_count", VALID_FEATURES), Some("login_count_7d"));
        assert_eq!(fuzzy_match("Memer", VALID_ENTITY_TYPES), Some("Member"));
    }

    #[test]
    fn fuzzy_match_rejects_distant() {
        assert_eq!(fuzzy_match("zzzzzzzzzzzzz", VALID_FEATURES), None);
    }

    #[test]
    fn parse_duration_valid() {
        assert_eq!(parse_duration("30m"), Some(std::time::Duration::from_secs(30 * 60)));
        assert_eq!(parse_duration("1h"), Some(std::time::Duration::from_secs(3600)));
        assert_eq!(
            parse_duration("2h30m"),
            Some(std::time::Duration::from_secs(2 * 3600 + 30 * 60))
        );
    }

    #[test]
    fn parse_duration_invalid() {
        assert_eq!(parse_duration("banana"), None);
        assert_eq!(parse_duration("30"), None); // no unit
        assert_eq!(parse_duration(""), None);
    }

    #[test]
    fn cron_validation_edge_cases() {
        let mut result = ValidationResult::new();

        // Valid expressions
        validate_cron("*/15 * * * *", &mut result);
        assert!(result.errors.is_empty());

        validate_cron("0 0 * * 0", &mut result);
        assert!(result.errors.is_empty());

        validate_cron("0,30 9-17 * * 1-5", &mut result);
        assert!(result.errors.is_empty());

        // Invalid
        validate_cron("60 * * * *", &mut result);
        assert!(!result.errors.is_empty());
    }

    #[test]
    fn validate_yaml_parse_error() {
        let result = validate_yaml("not: valid: yaml: {{{{");
        assert!(!result.valid);
        assert!(result.errors[0].message.contains("YAML parse error"));
    }

    #[test]
    fn valid_compose_rule_passes() {
        let result = validate_yaml(
            r#"
apiVersion: v1
kind: AnomalyRule
metadata:
  id: multi-signal
  name: Multi Signal
schedule:
  cron: "*/30 * * * *"
detection:
  compose:
    operator: and
    conditions:
      - signal: z_score
        threshold: 3.0
      - signal: dbscan_noise
        threshold: 0.6
notifications:
  - channel: webhook
    url: "https://hooks.example.com/alerts"
"#,
        );
        assert!(result.valid, "errors: {:?}", result.errors);
    }
}
