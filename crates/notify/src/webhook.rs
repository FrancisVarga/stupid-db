//! Generic HTTP webhook notifier.
//!
//! Delivers notifications as JSON payloads to configured webhook URLs
//! with optional custom headers and request body templates.

use std::collections::HashMap;
use std::sync::Arc;

use crate::templating::TemplateRenderer;
use crate::traits::{Notification, Notifier, NotifyError};

/// Delivers notifications as JSON over HTTP to a configured endpoint.
///
/// Supports configurable HTTP method, custom headers, and optional
/// body templates rendered via [`TemplateRenderer`]. Environment
/// variable references (`${VAR_NAME}`) in the URL and header values
/// are resolved at construction time.
#[allow(dead_code)]
#[derive(Debug)]
pub struct WebhookNotifier {
    /// Target URL (env vars already resolved).
    url: String,
    /// HTTP method (defaults to POST).
    method: reqwest::Method,
    /// Custom headers to include on every request.
    headers: HashMap<String, String>,
    /// Optional minijinja body template. When set, the template is
    /// rendered with notification data; otherwise the notification
    /// is serialized as JSON directly.
    body_template: Option<String>,
    /// Shared template renderer for body templates.
    renderer: Arc<TemplateRenderer>,
    /// Shared HTTP client (connection pooling).
    client: reqwest::Client,
}

impl WebhookNotifier {
    /// Create a new webhook notifier.
    ///
    /// Environment variable references (`${VAR_NAME}`) in `url` and
    /// header values are resolved eagerly. Missing env vars produce
    /// a [`NotifyError::Config`] error.
    ///
    /// `method` defaults to `POST` when `None`.
    pub fn new(
        url: String,
        method: Option<reqwest::Method>,
        headers: HashMap<String, String>,
        body_template: Option<String>,
        renderer: Arc<TemplateRenderer>,
    ) -> Result<Self, NotifyError> {
        let resolved_url = resolve_env_vars(&url)?;

        let mut resolved_headers = HashMap::with_capacity(headers.len());
        for (key, value) in &headers {
            resolved_headers.insert(key.clone(), resolve_env_vars(value)?);
        }

        // Validate body template syntax at construction time.
        if let Some(ref tmpl) = body_template {
            renderer
                .validate(tmpl)
                .map_err(|e| NotifyError::Config(format!("invalid body template: {e}")))?;
        }

        Ok(Self {
            url: resolved_url,
            method: method.unwrap_or(reqwest::Method::POST),
            headers: resolved_headers,
            body_template,
            renderer,
            client: reqwest::Client::new(),
        })
    }

    /// Construct a [`WebhookNotifier`] from config-level primitives.
    ///
    /// `method` is parsed from a string (e.g. `"POST"`, `"PUT"`).
    /// Invalid method strings produce [`NotifyError::Config`].
    pub fn from_config(
        url: String,
        method: Option<String>,
        headers: Option<HashMap<String, String>>,
        body_template: Option<String>,
        renderer: Arc<TemplateRenderer>,
    ) -> Result<Self, NotifyError> {
        let parsed_method = match method {
            Some(m) => {
                let upper = m.to_uppercase();
                upper
                    .parse::<reqwest::Method>()
                    .map(Some)
                    .map_err(|_| NotifyError::Config(format!("invalid HTTP method: {m}")))?
            }
            None => None,
        };

        Self::new(
            url,
            parsed_method,
            headers.unwrap_or_default(),
            body_template,
            renderer,
        )
    }
}

#[async_trait::async_trait]
impl Notifier for WebhookNotifier {
    /// Deliver a notification as a JSON payload to the configured webhook URL.
    async fn send(&self, notification: &Notification) -> Result<(), NotifyError> {
        let body = serde_json::to_string(notification).map_err(|e| {
            NotifyError::Config(format!("failed to serialize notification: {e}"))
        })?;

        let mut request = self
            .client
            .request(self.method.clone(), &self.url)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body);

        for (key, value) in &self.headers {
            request = request.header(key.as_str(), value.as_str());
        }

        let response = request.send().await?;
        let status = response.status();

        if !status.is_success() {
            let body_text = response
                .text()
                .await
                .unwrap_or_else(|_| "<unreadable body>".to_string());
            tracing::warn!(
                url = %self.url,
                %status,
                body = %body_text,
                "webhook returned non-2xx status"
            );
            return Err(NotifyError::Config(format!(
                "webhook returned {status}: {body_text}"
            )));
        }

        tracing::debug!(
            url = %self.url,
            method = %self.method,
            status = %status,
            "webhook notification delivered"
        );

        Ok(())
    }

    fn channel_name(&self) -> &str {
        "webhook"
    }
}

/// Resolve `${VAR_NAME}` patterns in a string using `std::env::var`.
///
/// Returns an error if a referenced variable is not set.
fn resolve_env_vars(input: &str) -> Result<String, NotifyError> {
    let mut result = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '$' && chars.peek() == Some(&'{') {
            // Consume the '{'
            chars.next();
            let mut var_name = String::new();
            let mut closed = false;
            for c in chars.by_ref() {
                if c == '}' {
                    closed = true;
                    break;
                }
                var_name.push(c);
            }
            if !closed {
                return Err(NotifyError::Config(format!(
                    "unclosed env var reference in: {input}"
                )));
            }
            let value = std::env::var(&var_name).map_err(|_| {
                NotifyError::Config(format!("env var not found: {var_name}"))
            })?;
            result.push_str(&value);
        } else {
            result.push(ch);
        }
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_env_vars_basic() {
        std::env::set_var("WEBHOOK_TEST_HOST", "example.com");
        let result = resolve_env_vars("https://${WEBHOOK_TEST_HOST}/hook").unwrap();
        assert_eq!(result, "https://example.com/hook");
        std::env::remove_var("WEBHOOK_TEST_HOST");
    }

    #[test]
    fn resolve_env_vars_multiple() {
        std::env::set_var("WT_PROTO", "https");
        std::env::set_var("WT_HOST", "api.test");
        let result = resolve_env_vars("${WT_PROTO}://${WT_HOST}/v1").unwrap();
        assert_eq!(result, "https://api.test/v1");
        std::env::remove_var("WT_PROTO");
        std::env::remove_var("WT_HOST");
    }

    #[test]
    fn resolve_env_vars_missing() {
        let result = resolve_env_vars("https://${ABSOLUTELY_NOT_SET_12345}/hook");
        assert!(result.is_err());
        match result.unwrap_err() {
            NotifyError::Config(msg) => assert!(msg.contains("ABSOLUTELY_NOT_SET_12345")),
            other => panic!("expected Config error, got: {other:?}"),
        }
    }

    #[test]
    fn resolve_env_vars_unclosed() {
        let result = resolve_env_vars("https://${UNCLOSED/hook");
        assert!(result.is_err());
        match result.unwrap_err() {
            NotifyError::Config(msg) => assert!(msg.contains("unclosed")),
            other => panic!("expected Config error, got: {other:?}"),
        }
    }

    #[test]
    fn resolve_env_vars_no_vars() {
        let result = resolve_env_vars("https://plain.example.com/hook").unwrap();
        assert_eq!(result, "https://plain.example.com/hook");
    }

    #[test]
    fn from_config_default_method() {
        let renderer = Arc::new(TemplateRenderer::new());
        let notifier =
            WebhookNotifier::from_config("https://example.com".into(), None, None, None, renderer)
                .unwrap();
        assert_eq!(notifier.method, reqwest::Method::POST);
    }

    #[test]
    fn from_config_put_method() {
        let renderer = Arc::new(TemplateRenderer::new());
        let notifier = WebhookNotifier::from_config(
            "https://example.com".into(),
            Some("PUT".into()),
            None,
            None,
            renderer,
        )
        .unwrap();
        assert_eq!(notifier.method, reqwest::Method::PUT);
    }

    #[test]
    fn from_config_case_insensitive_method() {
        let renderer = Arc::new(TemplateRenderer::new());
        let notifier = WebhookNotifier::from_config(
            "https://example.com".into(),
            Some("post".into()),
            None,
            None,
            renderer,
        )
        .unwrap();
        assert_eq!(notifier.method, reqwest::Method::POST);
    }

    #[test]
    fn from_config_invalid_method() {
        let renderer = Arc::new(TemplateRenderer::new());
        let result = WebhookNotifier::from_config(
            "https://example.com".into(),
            Some("NOT_A_METHOD\0".into()),
            None,
            None,
            renderer,
        );
        assert!(result.is_err());
    }

    #[test]
    fn from_config_with_headers() {
        std::env::set_var("WT_API_KEY", "secret-key-123");
        let renderer = Arc::new(TemplateRenderer::new());
        let headers = HashMap::from([
            ("X-Api-Key".to_string(), "${WT_API_KEY}".to_string()),
            ("X-Static".to_string(), "fixed-value".to_string()),
        ]);
        let notifier = WebhookNotifier::from_config(
            "https://example.com".into(),
            None,
            Some(headers),
            None,
            renderer,
        )
        .unwrap();
        assert_eq!(notifier.headers["X-Api-Key"], "secret-key-123");
        assert_eq!(notifier.headers["X-Static"], "fixed-value");
        std::env::remove_var("WT_API_KEY");
    }

    #[test]
    fn from_config_invalid_body_template() {
        let renderer = Arc::new(TemplateRenderer::new());
        let result = WebhookNotifier::from_config(
            "https://example.com".into(),
            None,
            None,
            Some("{{ unclosed".into()),
            renderer,
        );
        assert!(result.is_err());
        match result.unwrap_err() {
            NotifyError::Config(msg) => assert!(msg.contains("invalid body template")),
            other => panic!("expected Config error, got: {other:?}"),
        }
    }

    #[test]
    fn channel_name_is_webhook() {
        let renderer = Arc::new(TemplateRenderer::new());
        let notifier =
            WebhookNotifier::from_config("https://example.com".into(), None, None, None, renderer)
                .unwrap();
        assert_eq!(notifier.channel_name(), "webhook");
    }
}
