//! Notification channel validation: webhook URLs, email fields, Telegram tokens, secret detection.

use crate::schema::*;
use super::ValidationResult;

pub(super) fn validate_notifications(rule: &AnomalyRule, result: &mut ValidationResult) {
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

#[cfg(test)]
mod tests {
    use crate::validation::{validate_rule, validate_yaml};
    use crate::schema::*;

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
}
