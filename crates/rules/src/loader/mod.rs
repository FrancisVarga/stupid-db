//! Filesystem rule loader with hot-reload via `notify` watcher.
//!
//! Watches the rules directory for YAML file changes (create, modify, delete)
//! and reloads affected rules into the in-memory rule set.
//! Supports all rule kinds via two-pass deserialization (RuleEnvelope -> RuleDocument).

mod core;
mod error;
mod extends;
mod watcher;

#[cfg(test)]
mod tests;

pub use self::core::RuleLoader;
pub use self::error::{LoadResult, LoadStatus, Result, RuleError};
pub use self::extends::{deep_merge, resolve_extends};
