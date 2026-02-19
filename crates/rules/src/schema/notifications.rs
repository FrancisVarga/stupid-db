//! Notification channel types for rule alerting.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A notification channel configuration within a rule.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NotificationChannel {
    pub channel: ChannelType,
    #[serde(default = "default_on_events")]
    pub on: Vec<NotifyEvent>,
    #[serde(default)]
    pub template: Option<String>,
    // Channel-specific configuration
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub method: Option<String>,
    #[serde(default)]
    pub headers: Option<HashMap<String, String>>,
    #[serde(default)]
    pub body_template: Option<String>,
    // Email fields
    #[serde(default)]
    pub smtp_host: Option<String>,
    #[serde(default)]
    pub smtp_port: Option<u16>,
    #[serde(default)]
    pub tls: Option<bool>,
    #[serde(default)]
    pub from: Option<String>,
    #[serde(default)]
    pub to: Option<Vec<String>>,
    #[serde(default)]
    pub subject: Option<String>,
    // Telegram fields
    #[serde(default)]
    pub bot_token: Option<String>,
    #[serde(default)]
    pub chat_id: Option<String>,
    #[serde(default)]
    pub parse_mode: Option<String>,
}

fn default_on_events() -> Vec<NotifyEvent> {
    vec![NotifyEvent::Trigger]
}

/// Notification channel types.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ChannelType {
    Webhook,
    Email,
    Telegram,
}

/// Events that can trigger notifications.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum NotifyEvent {
    Trigger,
    Resolve,
}
