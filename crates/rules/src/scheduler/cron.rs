//! Cron normalization, due-check, and cooldown parsing helpers.

use std::time::Duration;

use chrono::{DateTime, Utc};
use cron::Schedule;

/// Normalize a 5-field cron expression to 6-field by prepending "0 " for seconds.
///
/// The `cron` crate requires 6 fields: `sec min hour day-of-month month day-of-week`.
/// User YAML uses standard 5-field cron: `min hour day-of-month month day-of-week`.
pub(crate) fn normalize_cron(cron_5field: &str) -> String {
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
pub(crate) fn is_cron_due(
    schedule: &Schedule,
    now: DateTime<Utc>,
    last_run: Option<DateTime<Utc>>,
) -> bool {
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
            // Ambiguous: "30m15" -- ignore trailing digits.
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
