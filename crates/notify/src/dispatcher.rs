//! Routes notifications to configured channels.
//!
//! The dispatcher receives a notification and delivers it to all
//! channels configured for the triggering rule. Individual channel
//! failures don't block other channels.

use std::collections::HashMap;

use crate::traits::{DispatchResult, Notification, Notifier, NotifyError};

/// Dispatches notifications to multiple channels, organized per-rule.
pub struct Dispatcher {
    /// Rule ID → list of notifier channels for that rule.
    rule_channels: HashMap<String, Vec<Box<dyn Notifier>>>,
    /// Fallback channels used when no rule-specific channels exist.
    default_channels: Vec<Box<dyn Notifier>>,
}

impl Dispatcher {
    /// Create a dispatcher with per-rule channel mapping.
    pub fn new(rule_channels: HashMap<String, Vec<Box<dyn Notifier>>>) -> Self {
        Self {
            rule_channels,
            default_channels: Vec::new(),
        }
    }

    /// Create an empty dispatcher.
    pub fn empty() -> Self {
        Self {
            rule_channels: HashMap::new(),
            default_channels: Vec::new(),
        }
    }

    /// Create a simple dispatcher with channels shared across all rules.
    pub fn with_defaults(channels: Vec<Box<dyn Notifier>>) -> Self {
        Self {
            rule_channels: HashMap::new(),
            default_channels: channels,
        }
    }

    /// Replace all channels for a specific rule.
    pub fn set_rule_channels(&mut self, rule_id: String, channels: Vec<Box<dyn Notifier>>) {
        self.rule_channels.insert(rule_id, channels);
    }

    /// Remove channels for a rule (e.g., on rule deletion).
    pub fn remove_rule(&mut self, rule_id: &str) {
        self.rule_channels.remove(rule_id);
    }

    /// Rebuild all rule channels (e.g., after hot-reload).
    pub fn rebuild(&mut self, rule_channels: HashMap<String, Vec<Box<dyn Notifier>>>) {
        self.rule_channels = rule_channels;
    }

    /// Dispatch a notification for a specific rule to all its channels.
    ///
    /// Returns results for each channel delivery. Individual failures
    /// don't block other channels.
    pub async fn dispatch(
        &self,
        rule_id: &str,
        notification: &Notification,
    ) -> Vec<DispatchResult> {
        let channels = self
            .rule_channels
            .get(rule_id)
            .unwrap_or(&self.default_channels);

        if channels.is_empty() {
            tracing::debug!(rule_id, "No notification channels configured");
            return Vec::new();
        }

        let mut results = Vec::with_capacity(channels.len());

        for channel in channels {
            let start = std::time::Instant::now();
            let result = channel.send(notification).await;
            let duration_ms = start.elapsed().as_millis() as u64;

            let (success, error) = match result {
                Ok(()) => {
                    tracing::info!(
                        rule_id,
                        channel = channel.channel_name(),
                        duration_ms,
                        "Notification delivered"
                    );
                    (true, None)
                }
                Err(e) => {
                    tracing::warn!(
                        rule_id,
                        channel = channel.channel_name(),
                        error = %e,
                        duration_ms,
                        "Notification delivery failed"
                    );
                    (false, Some(e.to_string()))
                }
            };

            results.push(DispatchResult {
                channel: channel.channel_name().to_string(),
                entity_key: notification
                    .metadata
                    .get("anomaly_key")
                    .cloned()
                    .unwrap_or_default(),
                success,
                error,
                duration_ms,
            });
        }

        results
    }

    /// Send a test notification to a specific rule's channel by index.
    pub async fn test_notify(
        &self,
        rule_id: &str,
        channel_index: usize,
    ) -> Result<(), NotifyError> {
        let channels = self
            .rule_channels
            .get(rule_id)
            .ok_or_else(|| NotifyError::Config(format!("No channels for rule '{rule_id}'")))?;

        let channel = channels
            .get(channel_index)
            .ok_or_else(|| NotifyError::Config(format!("Channel index {channel_index} out of range")))?;

        channel.test().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    struct MockNotifier {
        name: String,
        send_count: Arc<AtomicUsize>,
        should_fail: bool,
    }

    #[async_trait::async_trait]
    impl Notifier for MockNotifier {
        async fn send(&self, _notification: &Notification) -> Result<(), NotifyError> {
            self.send_count.fetch_add(1, Ordering::SeqCst);
            if self.should_fail {
                Err(NotifyError::Config("mock failure".to_string()))
            } else {
                Ok(())
            }
        }
        fn channel_name(&self) -> &str {
            &self.name
        }
    }

    #[tokio::test]
    async fn dispatch_to_all_channels() {
        let count_a = Arc::new(AtomicUsize::new(0));
        let count_b = Arc::new(AtomicUsize::new(0));

        let channels: Vec<Box<dyn Notifier>> = vec![
            Box::new(MockNotifier {
                name: "a".to_string(),
                send_count: count_a.clone(),
                should_fail: false,
            }),
            Box::new(MockNotifier {
                name: "b".to_string(),
                send_count: count_b.clone(),
                should_fail: false,
            }),
        ];

        let mut dispatcher = Dispatcher::empty();
        dispatcher.set_rule_channels("rule-1".to_string(), channels);

        let notification = Notification {
            subject: "test".to_string(),
            body: "test body".to_string(),
            metadata: HashMap::new(),
        };

        let results = dispatcher.dispatch("rule-1", &notification).await;
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|r| r.success));
        assert_eq!(count_a.load(Ordering::SeqCst), 1);
        assert_eq!(count_b.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn partial_failure_doesnt_block() {
        let count = Arc::new(AtomicUsize::new(0));

        let channels: Vec<Box<dyn Notifier>> = vec![
            Box::new(MockNotifier {
                name: "fail".to_string(),
                send_count: Arc::new(AtomicUsize::new(0)),
                should_fail: true,
            }),
            Box::new(MockNotifier {
                name: "ok".to_string(),
                send_count: count.clone(),
                should_fail: false,
            }),
        ];

        let mut dispatcher = Dispatcher::empty();
        dispatcher.set_rule_channels("rule-1".to_string(), channels);

        let notification = Notification {
            subject: "test".to_string(),
            body: "test body".to_string(),
            metadata: HashMap::new(),
        };

        let results = dispatcher.dispatch("rule-1", &notification).await;
        assert_eq!(results.len(), 2);
        assert!(!results[0].success);
        assert!(results[1].success);
        assert_eq!(count.load(Ordering::SeqCst), 1); // second channel still sent
    }

    #[tokio::test]
    async fn unknown_rule_returns_empty() {
        let dispatcher = Dispatcher::empty();
        let notification = Notification {
            subject: "test".to_string(),
            body: "test".to_string(),
            metadata: HashMap::new(),
        };
        let results = dispatcher.dispatch("nonexistent", &notification).await;
        assert!(results.is_empty());
    }
}
