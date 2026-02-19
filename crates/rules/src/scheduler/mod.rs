//! Per-rule cron scheduling with cooldown support.
//!
//! Manages scheduling state for all loaded anomaly rules. Each rule has its own
//! cron expression and optional cooldown period. The [`RuleScheduler`] tracks
//! when each rule last triggered and determines which rules are due to run.
//!
//! This module does NOT depend on the compute crate. It provides the scheduling
//! building blocks that the server crate wires into the compute scheduler via
//! a `ComputeTask` adapter.

mod core;
pub(crate) mod cron;
mod entry;

#[cfg(test)]
mod tests;

pub use self::core::RuleScheduler;
pub use self::cron::parse_cooldown;
pub use self::entry::RuleScheduleEntry;
