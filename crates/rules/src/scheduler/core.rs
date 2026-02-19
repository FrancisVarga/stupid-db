//! [`RuleScheduler`] — manages scheduling state for all loaded anomaly rules.

use std::collections::HashMap;
use std::str::FromStr;

use chrono::{DateTime, Utc};
use cron::Schedule;
use tracing::{debug, warn};

use crate::schema::AnomalyRule;

use super::cron::{is_cron_due, normalize_cron, parse_cooldown};
use super::entry::RuleScheduleEntry;

/// Manages scheduling state for all loaded anomaly rules.
///
/// Call [`sync_rules`](RuleScheduler::sync_rules) whenever the rule set changes
/// (e.g., after hot-reload). Use [`due_rules`](RuleScheduler::due_rules) from
/// the scheduler tick loop to find which rules should execute.
pub struct RuleScheduler {
    entries: HashMap<String, RuleScheduleEntry>,
}

impl RuleScheduler {
    /// Create a new empty scheduler.
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// Synchronize scheduling entries with the current set of loaded rules.
    ///
    /// - Adds entries for new rules.
    /// - Updates cron/cooldown/enabled for changed rules (preserves `last_triggered`).
    /// - Removes entries for rules no longer present.
    pub fn sync_rules(&mut self, rules: &[AnomalyRule]) {
        let current_ids: std::collections::HashSet<&str> =
            rules.iter().map(|r| r.metadata.id.as_str()).collect();

        // Remove entries for deleted rules.
        self.entries
            .retain(|id, _| current_ids.contains(id.as_str()));

        // Add or update entries.
        for rule in rules {
            let id = &rule.metadata.id;
            let cron_expr = normalize_cron(&rule.schedule.cron);
            let cooldown = rule
                .schedule
                .cooldown
                .as_deref()
                .and_then(parse_cooldown);

            match self.entries.get_mut(id) {
                Some(entry) => {
                    // Update mutable fields; preserve last_triggered.
                    entry.cron_expression = cron_expr;
                    entry.timezone = rule.schedule.timezone.clone();
                    entry.cooldown = cooldown;
                    entry.enabled = rule.metadata.enabled;
                }
                None => {
                    self.entries.insert(
                        id.clone(),
                        RuleScheduleEntry {
                            rule_id: id.clone(),
                            cron_expression: cron_expr,
                            timezone: rule.schedule.timezone.clone(),
                            cooldown,
                            last_triggered: None,
                            enabled: rule.metadata.enabled,
                        },
                    );
                }
            }
        }
    }

    /// Check whether a single rule should run at the given instant.
    ///
    /// Returns `false` if the rule is unknown, disabled, its cron expression
    /// is invalid, the cron window has not arrived, or the cooldown has not
    /// elapsed since the last trigger.
    pub fn should_run(&self, rule_id: &str, now: DateTime<Utc>) -> bool {
        let entry = match self.entries.get(rule_id) {
            Some(e) => e,
            None => return false,
        };

        if !entry.enabled {
            return false;
        }

        // Check cooldown first (cheaper than cron parse).
        if let (Some(cooldown), Some(last)) = (entry.cooldown, entry.last_triggered) {
            let elapsed = now.signed_duration_since(last);
            if elapsed
                < chrono::Duration::from_std(cooldown).unwrap_or(chrono::Duration::zero())
            {
                debug!(
                    rule_id = %rule_id,
                    "rule still in cooldown ({:.0}s remaining)",
                    cooldown.as_secs_f64() - elapsed.num_seconds() as f64,
                );
                return false;
            }
        }

        // Parse cron and check if `now` falls within the most recent tick window.
        match Schedule::from_str(&entry.cron_expression) {
            Ok(schedule) => is_cron_due(&schedule, now, entry.last_triggered),
            Err(e) => {
                warn!(
                    rule_id = %rule_id,
                    cron = %entry.cron_expression,
                    error = %e,
                    "invalid cron expression"
                );
                false
            }
        }
    }

    /// Record that a rule was triggered, updating `last_triggered` to now.
    pub fn record_trigger(&mut self, rule_id: &str) {
        if let Some(entry) = self.entries.get_mut(rule_id) {
            entry.last_triggered = Some(Utc::now());
        }
    }

    /// Record that a rule was triggered at a specific timestamp.
    ///
    /// Useful for testing and deterministic replay.
    pub fn record_trigger_at(&mut self, rule_id: &str, at: DateTime<Utc>) {
        if let Some(entry) = self.entries.get_mut(rule_id) {
            entry.last_triggered = Some(at);
        }
    }

    /// Return the IDs of all rules that should run at the given instant.
    pub fn due_rules(&self, now: DateTime<Utc>) -> Vec<&str> {
        self.entries
            .keys()
            .filter(|id| self.should_run(id, now))
            .map(String::as_str)
            .collect()
    }

    /// Get a reference to a scheduling entry by rule ID.
    pub fn get(&self, rule_id: &str) -> Option<&RuleScheduleEntry> {
        self.entries.get(rule_id)
    }

    /// Number of tracked rules.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the scheduler has no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl Default for RuleScheduler {
    fn default() -> Self {
        Self::new()
    }
}
