//! Notifier trait definition and shared error types.

use std::collections::HashMap;

/// Errors that can occur during notification delivery.
#[derive(Debug, thiserror::Error)]
pub enum NotifyError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    #[error("SMTP delivery failed: {0}")]
    Smtp(String),

    #[error("Template rendering failed: {0}")]
    Template(String),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Rate limited: retry after {retry_after_secs}s")]
    RateLimited { retry_after_secs: u64 },
}

/// A rendered notification ready for delivery.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Notification {
    /// The rendered subject/title.
    pub subject: String,
    /// The rendered body content.
    pub body: String,
    /// Additional metadata (e.g., severity, rule name).
    pub metadata: HashMap<String, String>,
}

/// Rich context passed to notifiers for template rendering and delivery.
#[derive(Debug, Clone, serde::Serialize)]
pub struct NotificationContext {
    pub rule_id: String,
    pub rule_name: String,
    pub rule_description: Option<String>,
    pub rule_tags: Vec<String>,
    pub anomaly_key: String,
    pub anomaly_score: f64,
    pub anomaly_classification: String,
    pub anomaly_signals: Vec<(String, f64)>,
    pub anomaly_features: HashMap<String, f64>,
    pub anomaly_entity_type: String,
    pub anomaly_cluster_id: Option<u64>,
    pub event: String,
    pub timestamp: String,
    pub enrichment_hits: Option<u64>,
}

/// Trait for notification channel implementations.
#[async_trait::async_trait]
pub trait Notifier: Send + Sync {
    /// Deliver a notification through this channel.
    async fn send(&self, notification: &Notification) -> Result<(), NotifyError>;

    /// Test connectivity with a sample notification.
    async fn test(&self) -> Result<(), NotifyError> {
        let test_notification = Notification {
            subject: "[TEST] Anomaly Detection Test".to_string(),
            body: "This is a test notification from stupid-db anomaly detection.".to_string(),
            metadata: HashMap::from([
                ("rule_id".to_string(), "test-rule".to_string()),
                ("event".to_string(), "trigger".to_string()),
            ]),
        };
        self.send(&test_notification).await
    }

    /// Human-readable name for this channel (e.g., "webhook", "email").
    fn channel_name(&self) -> &str;
}

/// Result of dispatching a notification to a single channel.
#[derive(Debug)]
pub struct DispatchResult {
    pub channel: String,
    pub entity_key: String,
    pub success: bool,
    pub error: Option<String>,
    pub duration_ms: u64,
}
