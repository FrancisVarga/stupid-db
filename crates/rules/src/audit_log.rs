//! In-memory structured audit log for anomaly rule evaluation.
//!
//! Stores per-rule log entries capped at a configurable maximum (default 500)
//! with FIFO eviction. Uses `std::sync::RwLock` so it can be accessed from
//! both async (tokio) and sync (rayon/std::thread) contexts.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, RwLock};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Severity level for audit log entries.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Debug,
    Info,
    Warning,
    Error,
}

impl LogLevel {
    /// Numeric severity for comparison (higher = more severe).
    pub fn as_severity(&self) -> u8 {
        match self {
            LogLevel::Debug => 0,
            LogLevel::Info => 1,
            LogLevel::Warning => 2,
            LogLevel::Error => 3,
        }
    }
}

/// Phase of rule execution that produced the log entry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionPhase {
    ScheduleCheck,
    Evaluation,
    TemplateMatch,
    SignalCheck,
    FilterApply,
    Enrichment,
    RateLimit,
    Notification,
    NotifyError,
    Complete,
}

/// A single audit log entry for a rule evaluation.
#[derive(Debug, Clone, Serialize)]
pub struct LogEntry {
    pub timestamp: DateTime<Utc>,
    pub rule_id: String,
    pub level: LogLevel,
    pub phase: ExecutionPhase,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
}

/// Query parameters for filtering audit log entries.
#[derive(Debug, Deserialize)]
pub struct LogQueryParams {
    /// Minimum log level (inclusive). Entries below this severity are excluded.
    pub level: Option<LogLevel>,
    /// Filter to a specific execution phase.
    pub phase: Option<ExecutionPhase>,
    /// Maximum number of entries to return.
    pub limit: Option<u32>,
    /// Only return entries at or after this ISO 8601 timestamp.
    pub since: Option<String>,
}

/// In-memory per-rule audit log with FIFO eviction.
///
/// Thread-safe via `std::sync::RwLock` — safe to use from both async handlers
/// and synchronous compute threads.
pub struct AuditLog {
    entries: Arc<RwLock<HashMap<String, VecDeque<LogEntry>>>>,
    max_entries_per_rule: usize,
}

impl AuditLog {
    /// Create a new audit log with the default cap of 500 entries per rule.
    pub fn new() -> Self {
        Self {
            entries: Arc::new(RwLock::new(HashMap::new())),
            max_entries_per_rule: 500,
        }
    }

    /// Create a new audit log with a custom per-rule entry cap.
    pub fn with_max_entries(max: usize) -> Self {
        Self {
            entries: Arc::new(RwLock::new(HashMap::new())),
            max_entries_per_rule: max,
        }
    }

    /// Append a basic log entry for a rule.
    pub fn log(
        &self,
        rule_id: &str,
        level: LogLevel,
        phase: ExecutionPhase,
        message: impl Into<String>,
    ) {
        self.log_with_details(rule_id, level, phase, message, None, None);
    }

    /// Append a log entry with optional structured details and duration.
    pub fn log_with_details(
        &self,
        rule_id: &str,
        level: LogLevel,
        phase: ExecutionPhase,
        message: impl Into<String>,
        details: Option<serde_json::Value>,
        duration_ms: Option<u64>,
    ) {
        let entry = LogEntry {
            timestamp: Utc::now(),
            rule_id: rule_id.to_string(),
            level,
            phase,
            message: message.into(),
            details,
            duration_ms,
        };

        let mut guard = self.entries.write().expect("audit_log lock poisoned");
        let deque = guard
            .entry(rule_id.to_string())
            .or_insert_with(VecDeque::new);
        deque.push_back(entry);
        while deque.len() > self.max_entries_per_rule {
            deque.pop_front();
        }
    }

    /// Query log entries for a rule, filtered by the given parameters.
    ///
    /// Returns entries newest-first. The lock is held only for the duration of
    /// the clone+filter, not across caller allocations.
    pub fn query(&self, rule_id: &str, params: &LogQueryParams) -> Vec<LogEntry> {
        let guard = self.entries.read().expect("audit_log lock poisoned");
        let Some(deque) = guard.get(rule_id) else {
            return Vec::new();
        };

        let min_severity = params
            .level
            .as_ref()
            .map(|l| l.as_severity())
            .unwrap_or(0);

        let since: Option<DateTime<Utc>> = params
            .since
            .as_ref()
            .and_then(|s| s.parse::<DateTime<Utc>>().ok());

        let limit = params.limit.unwrap_or(100) as usize;

        // Iterate newest-first (reverse), filter, take limit.
        let results: Vec<LogEntry> = deque
            .iter()
            .rev()
            .filter(|e| e.level.as_severity() >= min_severity)
            .filter(|e| {
                params
                    .phase
                    .as_ref()
                    .map_or(true, |p| &e.phase == p)
            })
            .filter(|e| since.map_or(true, |s| e.timestamp >= s))
            .take(limit)
            .cloned()
            .collect();

        results
    }

    /// Clear all log entries for a specific rule.
    pub fn clear(&self, rule_id: &str) {
        let mut guard = self.entries.write().expect("audit_log lock poisoned");
        guard.remove(rule_id);
    }
}

impl Default for AuditLog {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_log_and_query() {
        let log = AuditLog::new();
        log.log("rule-1", LogLevel::Info, ExecutionPhase::Evaluation, "started eval");
        log.log("rule-1", LogLevel::Debug, ExecutionPhase::TemplateMatch, "checking spike");
        log.log("rule-1", LogLevel::Warning, ExecutionPhase::SignalCheck, "high z-score");

        let params = LogQueryParams {
            level: None,
            phase: None,
            limit: None,
            since: None,
        };
        let entries = log.query("rule-1", &params);
        assert_eq!(entries.len(), 3);
        // Newest first
        assert_eq!(entries[0].phase, ExecutionPhase::SignalCheck);
        assert_eq!(entries[2].phase, ExecutionPhase::Evaluation);
    }

    #[test]
    fn test_level_filter() {
        let log = AuditLog::new();
        log.log("r1", LogLevel::Debug, ExecutionPhase::Evaluation, "debug msg");
        log.log("r1", LogLevel::Info, ExecutionPhase::Evaluation, "info msg");
        log.log("r1", LogLevel::Warning, ExecutionPhase::Evaluation, "warn msg");
        log.log("r1", LogLevel::Error, ExecutionPhase::Evaluation, "error msg");

        let params = LogQueryParams {
            level: Some(LogLevel::Warning),
            phase: None,
            limit: None,
            since: None,
        };
        let entries = log.query("r1", &params);
        assert_eq!(entries.len(), 2);
        assert!(entries.iter().all(|e| e.level.as_severity() >= 2));
    }

    #[test]
    fn test_phase_filter() {
        let log = AuditLog::new();
        log.log("r1", LogLevel::Info, ExecutionPhase::Evaluation, "eval");
        log.log("r1", LogLevel::Info, ExecutionPhase::Notification, "notify");
        log.log("r1", LogLevel::Info, ExecutionPhase::Evaluation, "eval2");

        let params = LogQueryParams {
            level: None,
            phase: Some(ExecutionPhase::Evaluation),
            limit: None,
            since: None,
        };
        let entries = log.query("r1", &params);
        assert_eq!(entries.len(), 2);
        assert!(entries.iter().all(|e| e.phase == ExecutionPhase::Evaluation));
    }

    #[test]
    fn test_limit() {
        let log = AuditLog::new();
        for i in 0..10 {
            log.log("r1", LogLevel::Info, ExecutionPhase::Evaluation, format!("msg {}", i));
        }

        let params = LogQueryParams {
            level: None,
            phase: None,
            limit: Some(3),
            since: None,
        };
        let entries = log.query("r1", &params);
        assert_eq!(entries.len(), 3);
    }

    #[test]
    fn test_fifo_eviction() {
        let log = AuditLog::with_max_entries(3);
        log.log("r1", LogLevel::Info, ExecutionPhase::Evaluation, "msg 1");
        log.log("r1", LogLevel::Info, ExecutionPhase::Evaluation, "msg 2");
        log.log("r1", LogLevel::Info, ExecutionPhase::Evaluation, "msg 3");
        log.log("r1", LogLevel::Info, ExecutionPhase::Evaluation, "msg 4");

        let params = LogQueryParams {
            level: None,
            phase: None,
            limit: None,
            since: None,
        };
        let entries = log.query("r1", &params);
        assert_eq!(entries.len(), 3);
        // Oldest ("msg 1") should have been evicted
        assert_eq!(entries[2].message, "msg 2");
        assert_eq!(entries[0].message, "msg 4");
    }

    #[test]
    fn test_clear() {
        let log = AuditLog::new();
        log.log("r1", LogLevel::Info, ExecutionPhase::Evaluation, "msg");
        log.clear("r1");

        let params = LogQueryParams {
            level: None,
            phase: None,
            limit: None,
            since: None,
        };
        let entries = log.query("r1", &params);
        assert!(entries.is_empty());
    }

    #[test]
    fn test_query_nonexistent_rule() {
        let log = AuditLog::new();
        let params = LogQueryParams {
            level: None,
            phase: None,
            limit: None,
            since: None,
        };
        let entries = log.query("nonexistent", &params);
        assert!(entries.is_empty());
    }

    #[test]
    fn test_log_with_details() {
        let log = AuditLog::new();
        let details = serde_json::json!({"entity_count": 42, "threshold": 3.5});
        log.log_with_details(
            "r1",
            LogLevel::Info,
            ExecutionPhase::TemplateMatch,
            "spike detected",
            Some(details.clone()),
            Some(150),
        );

        let params = LogQueryParams {
            level: None,
            phase: None,
            limit: None,
            since: None,
        };
        let entries = log.query("r1", &params);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].details, Some(details));
        assert_eq!(entries[0].duration_ms, Some(150));
    }

    #[test]
    fn test_severity_ordering() {
        assert!(LogLevel::Debug.as_severity() < LogLevel::Info.as_severity());
        assert!(LogLevel::Info.as_severity() < LogLevel::Warning.as_severity());
        assert!(LogLevel::Warning.as_severity() < LogLevel::Error.as_severity());
    }

    #[test]
    fn test_per_rule_isolation() {
        let log = AuditLog::new();
        log.log("r1", LogLevel::Info, ExecutionPhase::Evaluation, "r1 msg");
        log.log("r2", LogLevel::Error, ExecutionPhase::Notification, "r2 msg");

        let params = LogQueryParams {
            level: None,
            phase: None,
            limit: None,
            since: None,
        };
        let r1_entries = log.query("r1", &params);
        let r2_entries = log.query("r2", &params);
        assert_eq!(r1_entries.len(), 1);
        assert_eq!(r2_entries.len(), 1);
        assert_eq!(r1_entries[0].rule_id, "r1");
        assert_eq!(r2_entries[0].rule_id, "r2");
    }
}
