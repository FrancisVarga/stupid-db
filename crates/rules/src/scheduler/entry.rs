//! Per-rule schedule entry type.

use std::time::Duration;

use chrono::{DateTime, Utc};

/// Scheduling state for a single rule.
#[derive(Debug, Clone)]
pub struct RuleScheduleEntry {
    /// Rule identifier (matches `AnomalyRule.metadata.id`).
    pub rule_id: String,
    /// Normalized 6-field cron expression (seconds prepended).
    pub cron_expression: String,
    /// IANA timezone string (e.g., "UTC", "Asia/Manila").
    pub timezone: String,
    /// Minimum interval between successive triggers.
    pub cooldown: Option<Duration>,
    /// Timestamp of the last successful trigger.
    pub last_triggered: Option<DateTime<Utc>>,
    /// Whether the rule is enabled for evaluation.
    pub enabled: bool,
}
