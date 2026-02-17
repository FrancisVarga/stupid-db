//! Schedule validation: cron expressions, timezones, and duration parsing.

use crate::schema::*;
use super::ValidationResult;

pub(super) fn validate_schedule(rule: &AnomalyRule, result: &mut ValidationResult) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validation::{validate_rule, validate_yaml};

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
    fn validate_yaml_parse_error() {
        let result = validate_yaml("not: valid: yaml: {{{{");
        assert!(!result.valid);
        assert!(result.errors[0].message.contains("YAML parse error"));
    }
}
