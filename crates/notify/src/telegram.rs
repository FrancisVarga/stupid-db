//! Telegram Bot API notifier with Markdown formatting.
//!
//! Delivers notifications via the Telegram Bot API `sendMessage` endpoint.
//! Supports MarkdownV2 formatting and rate limit handling.

use crate::traits::{Notification, Notifier, NotifyError};

/// Escapes special characters for Telegram MarkdownV2 parse mode.
///
/// Telegram requires these characters to be escaped with a preceding backslash
/// when using MarkdownV2: `_`, `*`, `[`, `]`, `(`, `)`, `~`, `` ` ``, `>`,
/// `#`, `+`, `-`, `=`, `|`, `{`, `}`, `.`, `!`
pub fn escape_markdown_v2(text: &str) -> String {
    let special = ['_', '*', '[', ']', '(', ')', '~', '`', '>', '#', '+', '-', '=', '|', '{', '}', '.', '!'];
    let mut result = String::with_capacity(text.len() * 2);
    for ch in text.chars() {
        if special.contains(&ch) {
            result.push('\\');
        }
        result.push(ch);
    }
    result
}

/// Sends notifications via the Telegram Bot API.
#[derive(Debug)]
pub struct TelegramNotifier {
    bot_token: String,
    chat_id: String,
    parse_mode: Option<String>,
    client: reqwest::Client,
}

impl TelegramNotifier {
    /// Creates a new `TelegramNotifier` from configuration values.
    ///
    /// If `bot_token` starts with `${`, the value between `${` and `}` is
    /// resolved as an environment variable name. Returns
    /// [`NotifyError::Config`] if the token is empty or the env var is missing.
    pub fn from_config(
        bot_token: String,
        chat_id: String,
        parse_mode: Option<String>,
    ) -> Result<Self, NotifyError> {
        let resolved_token = if bot_token.starts_with("${") {
            let var_name = bot_token
                .strip_prefix("${")
                .and_then(|s| s.strip_suffix('}'))
                .ok_or_else(|| {
                    NotifyError::Config(format!(
                        "Malformed env var reference: {bot_token}"
                    ))
                })?;
            std::env::var(var_name).map_err(|_| {
                NotifyError::Config(format!(
                    "Environment variable '{var_name}' is not set"
                ))
            })?
        } else {
            bot_token
        };

        if resolved_token.is_empty() {
            return Err(NotifyError::Config(
                "Telegram bot token must not be empty".to_string(),
            ));
        }

        Ok(Self {
            bot_token: resolved_token,
            chat_id,
            parse_mode,
            client: reqwest::Client::new(),
        })
    }
}

#[async_trait::async_trait]
impl Notifier for TelegramNotifier {
    /// Sends a notification via the Telegram `sendMessage` API.
    async fn send(&self, notification: &Notification) -> Result<(), NotifyError> {
        let url = format!(
            "https://api.telegram.org/bot{}/sendMessage",
            self.bot_token
        );

        let mut body = serde_json::json!({
            "chat_id": self.chat_id,
            "text": notification.body,
        });

        if let Some(ref mode) = self.parse_mode {
            body["parse_mode"] = serde_json::Value::String(mode.clone());
        }

        tracing::debug!(
            chat_id = %self.chat_id,
            parse_mode = ?self.parse_mode,
            "Sending Telegram notification"
        );

        let response = self.client.post(&url).json(&body).send().await?;

        let status = response.status();
        let resp_body: serde_json::Value = response.json().await?;

        if resp_body.get("ok") == Some(&serde_json::Value::Bool(true)) {
            tracing::info!(chat_id = %self.chat_id, "Telegram notification sent");
            return Ok(());
        }

        // Handle rate limiting (HTTP 429).
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            let retry_after = resp_body
                .get("parameters")
                .and_then(|p| p.get("retry_after"))
                .and_then(|v| v.as_u64())
                .unwrap_or(30);
            return Err(NotifyError::RateLimited {
                retry_after_secs: retry_after,
            });
        }

        let description = resp_body
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown Telegram API error");

        Err(NotifyError::Config(format!(
            "Telegram API error: {description}"
        )))
    }

    /// Returns the channel name for this notifier.
    fn channel_name(&self) -> &str {
        "telegram"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_escape_markdown_v2_special_chars() {
        let input = "Hello_World *bold* [link](url) ~strike~ `code` >quote #tag +plus -minus =eq |pipe {brace} .dot !bang";
        let escaped = escape_markdown_v2(input);
        assert_eq!(
            escaped,
            r"Hello\_World \*bold\* \[link\]\(url\) \~strike\~ \`code\` \>quote \#tag \+plus \-minus \=eq \|pipe \{brace\} \.dot \!bang"
        );
    }

    #[test]
    fn test_escape_markdown_v2_no_special_chars() {
        let input = "Hello World 123";
        assert_eq!(escape_markdown_v2(input), input);
    }

    #[test]
    fn test_escape_markdown_v2_empty() {
        assert_eq!(escape_markdown_v2(""), "");
    }

    #[test]
    fn test_env_var_resolution() {
        std::env::set_var("TEST_TG_BOT_TOKEN", "123:ABC");
        let notifier = TelegramNotifier::from_config(
            "${TEST_TG_BOT_TOKEN}".to_string(),
            "12345".to_string(),
            None,
        )
        .expect("should resolve env var");
        assert_eq!(notifier.bot_token, "123:ABC");
        assert_eq!(notifier.chat_id, "12345");
        std::env::remove_var("TEST_TG_BOT_TOKEN");
    }

    #[test]
    fn test_env_var_missing() {
        let result = TelegramNotifier::from_config(
            "${NONEXISTENT_VAR_TELEGRAM_XYZ}".to_string(),
            "12345".to_string(),
            None,
        );
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("NONEXISTENT_VAR_TELEGRAM_XYZ"));
    }

    #[test]
    fn test_empty_token_rejected() {
        let result = TelegramNotifier::from_config(
            String::new(),
            "12345".to_string(),
            None,
        );
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("must not be empty"));
    }

    #[test]
    fn test_channel_name() {
        let notifier = TelegramNotifier::from_config(
            "test-token".to_string(),
            "12345".to_string(),
            Some("MarkdownV2".to_string()),
        )
        .unwrap();
        assert_eq!(notifier.channel_name(), "telegram");
    }

    #[test]
    fn test_literal_token_accepted() {
        let notifier = TelegramNotifier::from_config(
            "123456:ABC-DEF".to_string(),
            "-100123".to_string(),
            Some("HTML".to_string()),
        )
        .unwrap();
        assert_eq!(notifier.bot_token, "123456:ABC-DEF");
        assert_eq!(notifier.chat_id, "-100123");
        assert_eq!(notifier.parse_mode.as_deref(), Some("HTML"));
    }
}
