//! Tests for the scheduler module.

#[cfg(test)]
mod tests {
    use std::str::FromStr;
    use std::time::Duration;

    use chrono::Utc;
    use cron::Schedule;

    use crate::schema::{AnomalyRule, CommonMetadata, Detection, Schedule as RuleSchedule};
    use crate::scheduler::cron::{is_cron_due, normalize_cron, parse_cooldown};
    use crate::scheduler::RuleScheduler;

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

    // -- normalize_cron ----------------------------------------------------

    #[test]
    fn normalize_cron_5_to_6_fields() {
        assert_eq!(normalize_cron("*/15 * * * *"), "0 */15 * * * *");
        assert_eq!(normalize_cron("0 6 * * 1-5"), "0 0 6 * * 1-5");
        assert_eq!(normalize_cron("30 2 1 * *"), "0 30 2 1 * *");
    }

    #[test]
    fn normalize_cron_already_6_fields() {
        assert_eq!(normalize_cron("0 */15 * * * *"), "0 */15 * * * *");
    }

    #[test]
    fn normalize_cron_trims_whitespace() {
        assert_eq!(normalize_cron("  */5 * * * *  "), "0 */5 * * * *");
    }

    // -- parse_cooldown ----------------------------------------------------

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
        assert_eq!(parse_cooldown("30m15"), None);
    }

    #[test]
    fn parse_cooldown_bare_number_as_seconds() {
        assert_eq!(parse_cooldown("120"), Some(Duration::from_secs(120)));
    }

    // -- should_run: cron window -------------------------------------------

    #[test]
    fn should_run_within_cron_window() {
        let mut sched = RuleScheduler::new();
        let rules = vec![make_rule("r1", "* * * * *", None, true)];
        sched.sync_rules(&rules);

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
        let rules = vec![make_rule("r1", "*/5 * * * *", None, true)];
        sched.sync_rules(&rules);

        let just_after_tick = chrono::DateTime::parse_from_rfc3339("2026-01-15T10:00:01Z")
            .unwrap()
            .with_timezone(&Utc);
        sched.record_trigger_at("r1", just_after_tick);

        let two_min_later = just_after_tick + chrono::Duration::minutes(2);
        assert!(!sched.should_run("r1", two_min_later));

        let five_min_later = just_after_tick + chrono::Duration::minutes(5);
        assert!(sched.should_run("r1", five_min_later));
    }

    // -- should_run: with cooldown -----------------------------------------

    #[test]
    fn should_run_within_cooldown_returns_false() {
        let mut sched = RuleScheduler::new();
        let rules = vec![make_rule("r1", "* * * * *", Some("30m"), true)];
        sched.sync_rules(&rules);

        let now = Utc::now();
        sched.record_trigger_at("r1", now);

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

        let after_cooldown = now + chrono::Duration::minutes(31);
        assert!(sched.should_run("r1", after_cooldown));
    }

    // -- sync_rules --------------------------------------------------------

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

        let trigger_time = Utc::now();
        sched.record_trigger_at("r1", trigger_time);

        let rules = vec![make_rule("r1", "*/5 * * * *", Some("1h"), false)];
        sched.sync_rules(&rules);

        let entry = sched.get("r1").unwrap();
        assert_eq!(entry.cron_expression, "0 */5 * * * *");
        assert_eq!(entry.cooldown, Some(Duration::from_secs(3_600)));
        assert!(!entry.enabled);
        assert_eq!(entry.last_triggered, Some(trigger_time));
    }

    // -- due_rules ---------------------------------------------------------

    #[test]
    fn due_rules_returns_correct_subset() {
        let mut sched = RuleScheduler::new();
        let rules = vec![
            make_rule("always", "* * * * *", None, true),
            make_rule("disabled", "* * * * *", None, false),
            make_rule("cooldown", "* * * * *", Some("1h"), true),
        ];
        sched.sync_rules(&rules);

        let now = Utc::now();
        sched.record_trigger_at("cooldown", now);

        let due = sched.due_rules(now + chrono::Duration::seconds(90));

        assert_eq!(due.len(), 1);
        assert!(due.contains(&"always"));
    }

    // -- record_trigger ----------------------------------------------------

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
        sched.record_trigger("nonexistent");
    }

    // -- is_cron_due -------------------------------------------------------

    #[test]
    fn is_cron_due_never_run_before() {
        let schedule = Schedule::from_str("0 * * * * *").unwrap();
        let now = Utc::now();
        assert!(is_cron_due(&schedule, now, None));
    }

    #[test]
    fn is_cron_due_just_ran() {
        let schedule = Schedule::from_str("0 * * * * *").unwrap();
        let now = Utc::now();
        assert!(!is_cron_due(&schedule, now, Some(now)));
    }
}
