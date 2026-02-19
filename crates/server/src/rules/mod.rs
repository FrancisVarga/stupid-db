//! Generic rule CRUD API endpoints for all rule kinds.
//!
//! Provides REST endpoints for managing any [`RuleDocument`] variant stored
//! as YAML files on disk via [`stupid_rules::loader::RuleLoader`].
//! Complements the anomaly-specific lifecycle endpoints in [`crate::anomaly_rules`].

mod evaluate;
mod handlers;
mod types;

use std::sync::Arc;

use axum::routing::{get, post};
use axum::Router;

use crate::state::AppState;

pub use evaluate::*;
pub use handlers::*;
pub use types::*;

/// Build the generic rules sub-router.
pub fn rules_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/rules", get(list_rules).post(create_rule))
        .route("/rules/validate", post(validate_rule))
        .route("/rules/dry-run", post(dry_run_rule))
        .route("/rules/recent-triggers", get(recent_triggers))
        .route("/rules/{id}", get(get_rule).put(update_rule).delete(delete_rule))
        .route("/rules/{id}/yaml", get(get_rule_yaml))
        .route("/rules/{id}/toggle", post(toggle_rule))
}
