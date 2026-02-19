//! Anomaly rule CRUD and lifecycle API endpoints.
//!
//! Provides REST endpoints for managing anomaly detection rules stored as
//! YAML files on disk via [`stupid_rules::loader::RuleLoader`], plus
//! lifecycle operations (start, pause, run, test-notify, history).

mod crud;
mod lifecycle;
mod types;

use std::sync::Arc;

use axum::routing::{get, post};
use axum::Router;

use crate::state::AppState;

pub use self::crud::*;
pub use self::lifecycle::*;
pub use self::types::*;

/// Build the anomaly rules sub-router.
///
/// Mount this on the main router with `.merge(anomaly_rules_router())` or
/// `.nest("/", anomaly_rules_router())`.
pub fn anomaly_rules_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/anomaly-rules", get(list_anomaly_rules).post(create_anomaly_rule))
        .route(
            "/anomaly-rules/{id}",
            get(get_anomaly_rule)
                .put(update_anomaly_rule)
                .delete(delete_anomaly_rule),
        )
        .route("/anomaly-rules/{id}/start", post(start_anomaly_rule))
        .route("/anomaly-rules/{id}/pause", post(pause_anomaly_rule))
        .route("/anomaly-rules/{id}/run", post(run_anomaly_rule))
        .route("/anomaly-rules/{id}/test-notify", post(test_notify_rule))
        .route("/anomaly-rules/{id}/history", get(rule_history))
        .route("/anomaly-rules/{id}/logs", get(rule_logs))
}
