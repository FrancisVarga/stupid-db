//! Notification engine for anomaly detection alerts.
//!
//! This crate provides:
//! - `Notifier` trait for pluggable notification channels
//! - Webhook, email, and Telegram notifier implementations
//! - Minijinja template rendering for notification messages
//! - Dispatcher that routes notifications to configured channels

pub mod dispatcher;
pub mod email;
pub mod telegram;
pub mod templating;
pub mod traits;
pub mod webhook;

pub use dispatcher::Dispatcher;
pub use traits::{Notifier, NotifyError};
