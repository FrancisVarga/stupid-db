//! Type definitions for anomaly rule API responses and shared state.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, RwLock};

use serde::{Deserialize, Serialize};

use stupid_rules::schema::AnomalyRule;

/// Lightweight summary returned by the list endpoint.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct RuleSummary {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub template: Option<String>,
    pub cron: String,
    pub channel_count: usize,
    /// ISO-8601 timestamp of the most recent trigger, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_triggered: Option<String>,
    /// How many times this rule has been triggered.
    pub trigger_count: usize,
}

impl RuleSummary {
    /// Build a summary from a rule, enriched with trigger history.
    pub(crate) fn from_rule_with_history(rule: &AnomalyRule, history: &HashMap<String, VecDeque<TriggerEntry>>) -> Self {
        let template = rule.detection.template.as_ref().map(|t| {
            serde_json::to_value(t)
                .ok()
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_else(|| format!("{:?}", t).to_lowercase())
        });

        let (last_triggered, trigger_count) = match history.get(&rule.metadata.id) {
            Some(deque) => (
                deque.back().map(|e| e.timestamp.clone()),
                deque.len(),
            ),
            None => (None, 0),
        };

        Self {
            id: rule.metadata.id.clone(),
            name: rule.metadata.name.clone(),
            enabled: rule.metadata.enabled,
            template,
            cron: rule.schedule.cron.clone(),
            channel_count: rule.notifications.len(),
            last_triggered,
            trigger_count,
        }
    }
}

/// Result of an immediate rule evaluation via `POST /anomaly-rules/{id}/run`.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct RunResult {
    pub rule_id: String,
    pub matches_found: usize,
    pub evaluation_ms: u64,
    pub message: String,
}

/// Result of a test notification dispatch via `POST /anomaly-rules/{id}/test-notify`.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct TestNotifyResult {
    pub channel: String,
    pub success: bool,
    pub error: Option<String>,
    pub response_ms: u64,
}

/// Compact match summary stored in trigger history.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct MatchSummary {
    pub entity_key: String,
    pub entity_type: String,
    pub score: f64,
    pub reason: String,
}

/// A single trigger history entry stored per rule.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct TriggerEntry {
    pub timestamp: String,
    pub matches_found: usize,
    pub evaluation_ms: u64,
    /// Top matches (capped at 50) sorted by score descending.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub matches: Vec<MatchSummary>,
}

/// Query parameters for the history endpoint.
#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct HistoryParams {
    pub limit: Option<u32>,
}

/// Shared trigger history map: rule_id -> deque of recent trigger entries.
pub type SharedTriggerHistory = Arc<RwLock<HashMap<String, VecDeque<TriggerEntry>>>>;
