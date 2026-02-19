//! Cron-based background scheduler for periodic ingestion jobs.
//!
//! Polls `ingestion_sources` every 30 seconds for enabled sources with a
//! `schedule_json` whose `next_run_at` is NULL or in the past, then spawns
//! an ingestion job and advances `next_run_at` to the next cron fire time.

use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use cron::Schedule;
use std::str::FromStr;
use tracing::{info, warn};

use crate::state::AppState;

use super::job_runner::spawn_ingestion_job;
use super::source_store::IngestionSourceStore;
use super::types::TriggerKind;

/// Poll interval for the scheduler loop.
const POLL_INTERVAL: Duration = Duration::from_secs(30);

/// Run the ingestion scheduler in an infinite loop.
///
/// For each enabled, scheduled source whose `next_run_at <= now` (or is NULL):
/// 1. Parse the cron expression from `schedule_json`
/// 2. Spawn an ingestion job via [`spawn_ingestion_job`]
/// 3. Compute the next fire time and update `next_run_at`
/// 4. Update `last_run_at` to now
///
/// Silently returns if `pg_pool` is `None` (PostgreSQL not configured).
pub async fn run_ingestion_scheduler(state: Arc<AppState>) {
    let pool = match state.pg_pool.as_ref() {
        Some(p) => p,
        None => return, // no PG — scheduler disabled
    };

    info!("ingestion scheduler started (poll interval: {}s)", POLL_INTERVAL.as_secs());

    loop {
        tokio::time::sleep(POLL_INTERVAL).await;

        let now = Utc::now();
        let due_sources = match IngestionSourceStore::find_due_scheduled(pool, now).await {
            Ok(sources) => sources,
            Err(e) => {
                warn!(error = %e, "scheduler: failed to query due sources");
                continue;
            }
        };

        for source in due_sources {
            // Parse the schedule from the source row.
            let schedule = match source.schedule() {
                Some(Ok(s)) => s,
                Some(Err(e)) => {
                    warn!(
                        source_id = %source.id,
                        source_name = %source.name,
                        error = %e,
                        "scheduler: failed to parse schedule_json — skipping"
                    );
                    continue;
                }
                None => continue, // no schedule — shouldn't happen (query filters), but be safe
            };

            // Parse the cron expression.
            let cron_schedule = match parse_cron(&schedule.cron) {
                Ok(s) => s,
                Err(e) => {
                    warn!(
                        source_id = %source.id,
                        source_name = %source.name,
                        cron = %schedule.cron,
                        error = %e,
                        "scheduler: invalid cron expression — skipping"
                    );
                    continue;
                }
            };

            info!(
                source_id = %source.id,
                source_name = %source.name,
                trigger = "scheduled",
                "scheduler: triggering ingestion job"
            );

            // Spawn the ingestion job.
            spawn_ingestion_job(state.clone(), source.clone(), TriggerKind::Scheduled).await;

            // Compute next fire time and update timestamps.
            if let Some(next_fire) = cron_schedule.upcoming(Utc).next() {
                if let Err(e) = IngestionSourceStore::update_next_run_at(pool, source.id, next_fire).await {
                    warn!(
                        source_id = %source.id,
                        error = %e,
                        "scheduler: failed to update next_run_at"
                    );
                }
            }

            if let Err(e) = IngestionSourceStore::update_last_run_at(pool, source.id, Utc::now()).await {
                warn!(
                    source_id = %source.id,
                    error = %e,
                    "scheduler: failed to update last_run_at"
                );
            }
        }
    }
}

/// Parse a cron expression, auto-prepending "0 " for 5-field expressions.
///
/// The `cron` crate requires 6 fields (sec min hr dom mon dow), but users
/// typically write 5-field cron (min hr dom mon dow). We detect and adapt.
fn parse_cron(expr: &str) -> Result<Schedule, cron::error::Error> {
    let parts: Vec<&str> = expr.split_whitespace().collect();
    if parts.len() == 5 {
        // Standard 5-field cron — prepend seconds field
        let six_field = format!("0 {}", expr);
        Schedule::from_str(&six_field)
    } else {
        Schedule::from_str(expr)
    }
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_cron_six_field() {
        // 6-field: every 5 minutes
        let schedule = parse_cron("0 */5 * * * *").unwrap();
        let next = schedule.upcoming(Utc).next();
        assert!(next.is_some(), "should compute a next fire time");
    }

    #[test]
    fn test_parse_cron_five_field_auto_prefix() {
        // 5-field: every hour at :00
        let schedule = parse_cron("0 * * * *").unwrap();
        let next = schedule.upcoming(Utc).next();
        assert!(next.is_some(), "should compute a next fire time");
    }

    #[test]
    fn test_parse_cron_invalid() {
        let result = parse_cron("not a cron");
        assert!(result.is_err(), "should fail on invalid cron expression");
    }

    #[test]
    fn test_parse_cron_next_fire_is_future() {
        let schedule = parse_cron("0 */5 * * * *").unwrap();
        let next = schedule.upcoming(Utc).next().unwrap();
        assert!(next > Utc::now(), "next fire time should be in the future");
    }

    #[test]
    fn test_parse_cron_daily_midnight() {
        // "At midnight every day" — 5-field
        let schedule = parse_cron("0 0 * * *").unwrap();
        let next = schedule.upcoming(Utc).next().unwrap();
        assert_eq!(next.format("%H:%M:%S").to_string(), "00:00:00");
    }
}
