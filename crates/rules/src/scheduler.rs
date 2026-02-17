//! Per-rule cron scheduling with cooldown support.
//!
//! Manages scheduling state for all loaded anomaly rules. Each rule has its own
//! cron expression and optional cooldown period. The [`RuleScheduler`] tracks
//! when each rule last triggered and determines which rules are due to run.
//!
//! This module does NOT depend on the compute crate. It provides the scheduling
//! building blocks that the server crate wires into the compute scheduler via
//! a `ComputeTask` adapter.

use std::collections::HashMap;
use std::str::FromStr;
use std::time::Duration;

use chrono::{DateTime, Utc};
use cron::Schedule;
use tracing::{debug, warn};

use crate::schema::AnomalyRule;

// ── Per-rule schedule entry ─────────────────────────────────────────

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

// ── Rule scheduler ──────────────────────────────────────────────────

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
        self.entries.retain(|id, _| current_ids.contains(id.as_str()));

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
            if elapsed < chrono::Duration::from_std(cooldown).unwrap_or(chrono::Duration::zero()) {
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

// ── Cron helpers ────────────────────────────────────────────────────

/// Normalize a 5-field cron expression to 6-field by prepending "0 " for seconds.
///
/// The `cron` crate requires 6 fields: `sec min hour day-of-month month day-of-week`.
/// User YAML uses standard 5-field cron: `min hour day-of-month month day-of-week`.
fn normalize_cron(cron_5field: &str) -> String {
    let trimmed = cron_5field.trim();
    let field_count = trimmed.split_whitespace().count();
    if field_count == 5 {
        format!("0 {}", trimmed)
    } else {
        // Already 6-field or non-standard; pass through as-is.
        trimmed.to_string()
    }
}

/// Check if a cron schedule is due at `now`.
///
/// A rule is due if its most recent scheduled tick falls between `last_run`
/// (exclusive) and `now` (inclusive). If `last_run` is `None`, any upcoming
/// tick at or before `now` counts.
fn is_cron_due(schedule: &Schedule, now: DateTime<Utc>, last_run: Option<DateTime<Utc>>) -> bool {
    // Find the most recent scheduled time at or before `now`.
    // `schedule.after()` gives upcoming times, so we check if there is a
    // scheduled time between last_run and now.
    let check_from = last_run.unwrap_or(now - chrono::Duration::days(1));

    // Get the first scheduled time after `check_from`.
    if let Some(next) = schedule.after(&check_from).next() {
        next <= now
    } else {
        false
    }
}

// ── Cooldown parsing ────────────────────────────────────────────────

/// Parse a human-readable duration string into a [`Duration`].
///
/// Supports components: `Xd` (days), `Xh` (hours), `Xm` (minutes), `Xs` (seconds).
/// Components can be combined: "2h30m", "1d12h", "90s".
/// Returns `None` if the string is empty or unparseable.
pub fn parse_cooldown(s: &str) -> Option<Duration> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }

    let mut total_secs: u64 = 0;
    let mut num_buf = String::new();
    let mut found_unit = false;

    for ch in s.chars() {
        if ch.is_ascii_digit() {
            num_buf.push(ch);
        } else {
            let n: u64 = num_buf.parse().ok()?;
            num_buf.clear();
            match ch {
                'd' => total_secs += n * 86_400,
                'h' => total_secs += n * 3_600,
                'm' => total_secs += n * 60,
                's' => total_secs += n,
                _ => return None,
            }
            found_unit = true;
        }
    }

    // Handle trailing number without unit (treat as seconds).
    if !num_buf.is_empty() {
        if found_unit {
            // Ambiguous: "30m15" — ignore trailing digits.
            return None;
        }
        let n: u64 = num_buf.parse().ok()?;
        total_secs += n;
    }

    if total_secs == 0 && !found_unit {
        return None;
    }

    Some(Duration::from_secs(total_secs))
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{
        AnomalyRule, CommonMetadata, Detection, Schedule as RuleSchedule,
    };

    /// Helper to build a minimal AnomalyRule for testing.
    fn make_rule(id: &str, cron: &str, cooldown: Option<&str>, enabled: bool) -> AnomalyRule {
        AnomalyRule {
            api_version: "v1".to_string(),
            kind: "AnomalyRule".to_string(),
            metadata: CommonMetadata {
                id: id.to_string(),
                name: format!("Rule {}", id),
                description: None,
                tags: None,
                enabled,
                extends: None,
            },
            schedule: RuleSchedule {
                cron: cron.to_string(),
                timezone: "UTC".to_string(),
                cooldown: cooldown.map(String::from),
            },
            detection: Detection {
                template: None,
                params: None,
                compose: None,
                enrich: None,
            },
            filters: None,
            notifications: vec![],
        }
    }

    // ── normalize_cron ──────────────────────────────────────────────

    #[test]
    fn normalize_cron_5_to_6_fields() {
        assert_eq!(normalize_cron("*/15 * * * *"), "0 */15 * * * *");
        assert_eq!(normalize_cron("0 6 * * 1-5"), "0 0 6 * * 1-5");
        assert_eq!(normalize_cron("30 2 1 * *"), "0 30 2 1 * *");
    }

    #[test]
    fn normalize_cron_already_6_fields() {
        // Should pass through unchanged.
        assert_eq!(normalize_cron("0 */15 * * * *"), "0 */15 * * * *");
    }

    #[test]
    fn normalize_cron_trims_whitespace() {
        assert_eq!(normalize_cron("  */5 * * * *  "), "0 */5 * * * *");
    }

    // ── parse_cooldown ──────────────────────────────────────────────

    #[test]
    fn parse_cooldown_minutes() {
        assert_eq!(parse_cooldown("30m"), Some(Duration::from_secs(30 * 60)));
    }

    #[test]
    fn parse_cooldown_hours() {
        assert_eq!(parse_cooldown("1h"), Some(Duration::from_secs(3_600)));
    }

    #[test]
    fn parse_cooldown_combined() {
        assert_eq!(
            parse_cooldown("2h30m"),
            Some(Duration::from_secs(2 * 3_600 + 30 * 60))
        );
    }

    #[test]
    fn parse_cooldown_days() {
        assert_eq!(parse_cooldown("1d"), Some(Duration::from_secs(86_400)));
    }

    #[test]
    fn parse_cooldown_seconds() {
        assert_eq!(parse_cooldown("90s"), Some(Duration::from_secs(90)));
    }

    #[test]
    fn parse_cooldown_complex() {
        assert_eq!(
            parse_cooldown("1d2h30m15s"),
            Some(Duration::from_secs(86_400 + 7_200 + 1_800 + 15))
        );
    }

    #[test]
    fn parse_cooldown_empty_returns_none() {
        assert_eq!(parse_cooldown(""), None);
        assert_eq!(parse_cooldown("  "), None);
    }

    #[test]
    fn parse_cooldown_invalid_returns_none() {
        assert_eq!(parse_cooldown("abc"), None);
        assert_eq!(parse_cooldown("30m15"), None); // trailing digits after unit
    }

    #[test]
    fn parse_cooldown_bare_number_as_seconds() {
        assert_eq!(parse_cooldown("120"), Some(Duration::from_secs(120)));
    }

    // ── should_run — cron window ────────────────────────────────────

    #[test]
    fn should_run_within_cron_window() {
        let mut sched = RuleScheduler::new();
        // "* * * * *" = every minute
        let rules = vec![make_rule("r1", "* * * * *", None, true)];
        sched.sync_rules(&rules);

        // Any `now` should be due if never run before, since every minute matches.
        let now = Utc::now();
        assert!(sched.should_run("r1", now));
    }

    #[test]
    fn should_run_disabled_rule_returns_false() {
        let mut sched = RuleScheduler::new();
        let rules = vec![make_rule("r1", "* * * * *", None, false)];
        sched.sync_rules(&rules);

        assert!(!sched.should_run("r1", Utc::now()));
    }

    #[test]
    fn should_run_unknown_rule_returns_false() {
        let sched = RuleScheduler::new();
        assert!(!sched.should_run("nonexistent", Utc::now()));
    }

    #[test]
    fn should_run_after_recent_trigger_respects_last_run() {
        let mut sched = RuleScheduler::new();
        // Every 5 minutes
        let rules = vec![make_rule("r1", "*/5 * * * *", None, true)];
        sched.sync_rules(&rules);

        // Use a fixed time right after a cron tick so the next tick is ~5 min away.
        // 2026-01-15 10:00:01 UTC — just after the 10:00 tick.
        let just_after_tick = chrono::DateTime::parse_from_rfc3339("2026-01-15T10:00:01Z")
            .unwrap()
            .with_timezone(&Utc);
        sched.record_trigger_at("r1", just_after_tick);

        // 2 minutes later (10:02:01): next tick is at 10:05:00, not yet.
        let two_min_later = just_after_tick + chrono::Duration::minutes(2);
        assert!(!sched.should_run("r1", two_min_later));

        // 5 minutes later (10:05:01): the 10:05 tick has passed.
        let five_min_later = just_after_tick + chrono::Duration::minutes(5);
        assert!(sched.should_run("r1", five_min_later));
    }

    // ── should_run — with cooldown ──────────────────────────────────

    #[test]
    fn should_run_within_cooldown_returns_false() {
        let mut sched = RuleScheduler::new();
        // Every minute, 30m cooldown
        let rules = vec![make_rule("r1", "* * * * *", Some("30m"), true)];
        sched.sync_rules(&rules);

        let now = Utc::now();
        sched.record_trigger_at("r1", now);

        // 5 minutes later: cron says yes, but cooldown says no.
        let five_min = now + chrono::Duration::minutes(5);
        assert!(!sched.should_run("r1", five_min));
    }

    #[test]
    fn should_run_after_cooldown_expires() {
        let mut sched = RuleScheduler::new();
        let rules = vec![make_rule("r1", "* * * * *", Some("30m"), true)];
        sched.sync_rules(&rules);

        let now = Utc::now();
        sched.record_trigger_at("r1", now);

        // 31 minutes later: cooldown expired, cron matches.
        let after_cooldown = now + chrono::Duration::minutes(31);
        assert!(sched.should_run("r1", after_cooldown));
    }

    // ── sync_rules ──────────────────────────────────────────────────

    #[test]
    fn sync_rules_adds_new_rules() {
        let mut sched = RuleScheduler::new();
        assert!(sched.is_empty());

        let rules = vec![
            make_rule("r1", "* * * * *", None, true),
            make_rule("r2", "*/5 * * * *", Some("10m"), true),
        ];
        sched.sync_rules(&rules);

        assert_eq!(sched.len(), 2);
        assert!(sched.get("r1").is_some());
        assert!(sched.get("r2").is_some());
    }

    #[test]
    fn sync_rules_removes_deleted_rules() {
        let mut sched = RuleScheduler::new();
        let rules = vec![
            make_rule("r1", "* * * * *", None, true),
            make_rule("r2", "*/5 * * * *", None, true),
        ];
        sched.sync_rules(&rules);
        assert_eq!(sched.len(), 2);

        // Remove r2
        let rules = vec![make_rule("r1", "* * * * *", None, true)];
        sched.sync_rules(&rules);

        assert_eq!(sched.len(), 1);
        assert!(sched.get("r1").is_some());
        assert!(sched.get("r2").is_none());
    }

    #[test]
    fn sync_rules_updates_changed_preserves_last_triggered() {
        let mut sched = RuleScheduler::new();
        let rules = vec![make_rule("r1", "* * * * *", None, true)];
        sched.sync_rules(&rules);

        // Record a trigger.
        let trigger_time = Utc::now();
        sched.record_trigger_at("r1", trigger_time);

        // Sync with updated cron — last_triggered should be preserved.
        let rules = vec![make_rule("r1", "*/5 * * * *", Some("1h"), false)];
        sched.sync_rules(&rules);

        let entry = sched.get("r1").unwrap();
        assert_eq!(entry.cron_expression, "0 */5 * * * *");
        assert_eq!(entry.cooldown, Some(Duration::from_secs(3_600)));
        assert!(!entry.enabled);
        assert_eq!(entry.last_triggered, Some(trigger_time));
    }

    // ── due_rules ───────────────────────────────────────────────────

    #[test]
    fn due_rules_returns_correct_subset() {
        let mut sched = RuleScheduler::new();
        let rules = vec![
            make_rule("always", "* * * * *", None, true),
            make_rule("disabled", "* * * * *", None, false),
            make_rule("cooldown", "* * * * *", Some("1h"), true),
        ];
        sched.sync_rules(&rules);

        // Trigger "cooldown" rule so it enters cooldown.
        let now = Utc::now();
        sched.record_trigger_at("cooldown", now);

        let due = sched.due_rules(now + chrono::Duration::seconds(90));

        // Only "always" should be due. "disabled" is off, "cooldown" is cooling.
        assert_eq!(due.len(), 1);
        assert!(due.contains(&"always"));
    }

    // ── record_trigger ──────────────────────────────────────────────

    #[test]
    fn record_trigger_updates_last_triggered() {
        let mut sched = RuleScheduler::new();
        let rules = vec![make_rule("r1", "* * * * *", None, true)];
        sched.sync_rules(&rules);

        assert!(sched.get("r1").unwrap().last_triggered.is_none());

        sched.record_trigger("r1");
        assert!(sched.get("r1").unwrap().last_triggered.is_some());
    }

    #[test]
    fn record_trigger_at_sets_exact_time() {
        let mut sched = RuleScheduler::new();
        let rules = vec![make_rule("r1", "* * * * *", None, true)];
        sched.sync_rules(&rules);

        let ts = Utc::now() - chrono::Duration::hours(2);
        sched.record_trigger_at("r1", ts);

        assert_eq!(sched.get("r1").unwrap().last_triggered, Some(ts));
    }

    #[test]
    fn record_trigger_unknown_rule_is_noop() {
        let mut sched = RuleScheduler::new();
        sched.record_trigger("nonexistent"); // should not panic
    }

    // ── is_cron_due ─────────────────────────────────────────────────

    #[test]
    fn is_cron_due_never_run_before() {
        let schedule = Schedule::from_str("0 * * * * *").unwrap(); // every minute
        let now = Utc::now();
        assert!(is_cron_due(&schedule, now, None));
    }

    #[test]
    fn is_cron_due_just_ran() {
        let schedule = Schedule::from_str("0 * * * * *").unwrap();
        let now = Utc::now();
        // Just ran — next tick is ~1 minute away.
        assert!(!is_cron_due(&schedule, now, Some(now)));
    }
}
