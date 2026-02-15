//! OpenSearch enrichment queries with rate limiting.
//!
//! After pipeline signals fire, an optional enrichment query runs
//! against OpenSearch to confirm or refine the detection.
//! Each rule has an independent rate limiter (token bucket).
//!
//! The enrichment engine uses an [`OpenSearchQuery`] trait to abstract
//! the actual HTTP client, so the rules crate has no SDK dependency.
//! The server injects the real implementation at startup.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::schema::OpenSearchEnrichment;
use crate::templates::RuleMatch;

// ── OpenSearch client trait ─────────────────────────────────────────

/// Abstraction over the OpenSearch HTTP client.
///
/// The server crate implements this trait using reqwest against the
/// configured OpenSearch cluster. The rules crate only depends on
/// this trait, keeping it SDK-free.
#[async_trait::async_trait]
pub trait OpenSearchQuery: Send + Sync {
    /// Execute a query DSL body against the configured index.
    ///
    /// Returns the total hit count and a sample of raw hit documents.
    async fn search(
        &self,
        query_body: &serde_json::Value,
        timeout_ms: u64,
    ) -> Result<SearchResult, EnrichmentError>;
}

/// Raw search result from OpenSearch.
#[derive(Debug, Clone)]
pub struct SearchResult {
    /// Total matching documents.
    pub total_hits: u64,
    /// First N hit documents (for notification context).
    pub sample_hits: Vec<serde_json::Value>,
    /// Server-side query time in milliseconds.
    pub took_ms: u64,
}

/// Errors specific to enrichment.
#[derive(Debug, thiserror::Error)]
pub enum EnrichmentError {
    #[error("OpenSearch query failed: {0}")]
    QueryFailed(String),

    #[error("OpenSearch query timed out after {0}ms")]
    Timeout(u64),

    #[error("Rate limit exceeded for rule '{0}'")]
    RateLimited(String),

    #[error("No OpenSearch client configured")]
    NotConfigured,

    #[error("Template resolution failed: {0}")]
    TemplateError(String),
}

// ── Enrichment result ───────────────────────────────────────────────

/// Result of running an enrichment query for a rule match.
#[derive(Debug, Clone)]
pub struct EnrichmentResult {
    /// Whether the enrichment condition passed (hit count within bounds).
    pub passed: bool,
    /// Number of matching documents.
    pub hit_count: u64,
    /// First few hits for notification context.
    pub sample_hits: Vec<serde_json::Value>,
    /// Total query time in milliseconds.
    pub query_time_ms: u64,
}

// ── Simple token-bucket rate limiter ────────────────────────────────

/// Per-rule token bucket rate limiter.
///
/// Allows `max_per_hour` queries per rolling hour window.
struct RateLimiter {
    max_per_hour: u32,
    /// Timestamps of recent queries within the current window.
    timestamps: Vec<Instant>,
}

impl RateLimiter {
    fn new(max_per_hour: u32) -> Self {
        Self {
            max_per_hour,
            timestamps: Vec::new(),
        }
    }

    /// Try to acquire a token. Returns true if allowed.
    fn try_acquire(&mut self) -> bool {
        let now = Instant::now();
        let window = Duration::from_secs(3600);

        // Evict old timestamps outside the window
        self.timestamps.retain(|t| now.duration_since(*t) < window);

        if self.timestamps.len() < self.max_per_hour as usize {
            self.timestamps.push(now);
            true
        } else {
            false
        }
    }
}

// ── Enrichment engine ───────────────────────────────────────────────

/// Executes OpenSearch enrichment queries with per-rule rate limiting.
///
/// The engine is stateful: it holds the OpenSearch client and rate
/// limiters for each rule. Create one per server instance.
pub struct EnrichmentEngine {
    /// OpenSearch query executor. None = enrichment disabled.
    client: Option<Box<dyn OpenSearchQuery>>,
    /// Per-rule rate limiters (behind Mutex for interior mutability).
    rate_limiters: Mutex<HashMap<String, RateLimiter>>,
    /// Max sample hits to return in results.
    max_sample_hits: usize,
}

impl EnrichmentEngine {
    /// Create an enrichment engine with an OpenSearch client.
    pub fn new(client: Box<dyn OpenSearchQuery>) -> Self {
        Self {
            client: Some(client),
            rate_limiters: Mutex::new(HashMap::new()),
            max_sample_hits: 3,
        }
    }

    /// Create an enrichment engine with no client (enrichment always skipped).
    pub fn disabled() -> Self {
        Self {
            client: None,
            rate_limiters: Mutex::new(HashMap::new()),
            max_sample_hits: 3,
        }
    }

    /// Enrich a single rule match with OpenSearch data.
    ///
    /// If the client is not configured, or rate limit is exceeded, or
    /// the query fails/times out, enrichment is skipped gracefully
    /// and `EnrichmentResult.passed` is set to `true` (fail-open).
    pub async fn enrich(
        &self,
        rule_id: &str,
        config: &OpenSearchEnrichment,
        rule_match: &RuleMatch,
    ) -> EnrichmentResult {
        // Check if client is configured
        let client = match &self.client {
            Some(c) => c,
            None => {
                tracing::debug!(rule_id, "No OpenSearch client, skipping enrichment");
                return EnrichmentResult::skipped();
            }
        };

        // Check rate limit
        {
            let mut limiters = self.rate_limiters.lock().unwrap();
            let limiter = limiters
                .entry(rule_id.to_string())
                .or_insert_with(|| RateLimiter::new(config.rate_limit));

            if !limiter.try_acquire() {
                tracing::info!(
                    rule_id,
                    rate_limit = config.rate_limit,
                    "Enrichment rate limit exceeded, skipping"
                );
                return EnrichmentResult::skipped();
            }
        }

        // Resolve template variables in query
        let query = resolve_query_templates(&config.query, rule_match);

        let timeout = config.timeout_ms.unwrap_or(5000);

        // Execute query
        match client.search(&query, timeout).await {
            Ok(result) => {
                let hit_count = result.total_hits;
                let passed = evaluate_hit_bounds(hit_count, config.min_hits, config.max_hits);

                tracing::info!(
                    rule_id,
                    entity = %rule_match.entity_key,
                    hit_count,
                    passed,
                    took_ms = result.took_ms,
                    "Enrichment query completed"
                );

                EnrichmentResult {
                    passed,
                    hit_count,
                    sample_hits: result
                        .sample_hits
                        .into_iter()
                        .take(self.max_sample_hits)
                        .collect(),
                    query_time_ms: result.took_ms,
                }
            }
            Err(EnrichmentError::Timeout(ms)) => {
                tracing::warn!(rule_id, timeout_ms = ms, "Enrichment query timed out, skipping");
                EnrichmentResult::skipped()
            }
            Err(e) => {
                tracing::error!(rule_id, error = %e, "Enrichment query failed, skipping");
                EnrichmentResult::skipped()
            }
        }
    }
}

impl EnrichmentResult {
    /// A skipped enrichment always passes (fail-open behavior).
    fn skipped() -> Self {
        Self {
            passed: true,
            hit_count: 0,
            sample_hits: Vec::new(),
            query_time_ms: 0,
        }
    }
}

// ── Template variable resolution ────────────────────────────────────

/// Simple template variable resolution in OpenSearch query JSON.
///
/// Replaces `{{ anomaly.key }}` → actual entity key, and similar patterns.
/// This is a lightweight string replacement, not full minijinja rendering.
fn resolve_query_templates(
    query: &serde_json::Value,
    rule_match: &RuleMatch,
) -> serde_json::Value {
    let json_str = serde_json::to_string(query).unwrap_or_default();

    let resolved = json_str
        .replace("{{ anomaly.key }}", &rule_match.entity_key)
        .replace("{{anomaly.key}}", &rule_match.entity_key)
        .replace("{{ anomaly.entity_type }}", &rule_match.entity_type)
        .replace("{{anomaly.entity_type}}", &rule_match.entity_type);

    serde_json::from_str(&resolved).unwrap_or_else(|_| query.clone())
}

/// Evaluate whether the hit count falls within the configured bounds.
fn evaluate_hit_bounds(hit_count: u64, min_hits: Option<u64>, max_hits: Option<u64>) -> bool {
    match (min_hits, max_hits) {
        (Some(min), Some(max)) => hit_count >= min && hit_count <= max,
        (Some(min), None) => hit_count >= min,
        (None, Some(max)) => hit_count <= max,
        (None, None) => hit_count > 0,
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hit_bounds_min_only() {
        assert!(evaluate_hit_bounds(20, Some(10), None));
        assert!(evaluate_hit_bounds(10, Some(10), None));
        assert!(!evaluate_hit_bounds(5, Some(10), None));
    }

    #[test]
    fn hit_bounds_max_only() {
        assert!(evaluate_hit_bounds(5, None, Some(10)));
        assert!(evaluate_hit_bounds(10, None, Some(10)));
        assert!(!evaluate_hit_bounds(15, None, Some(10)));
    }

    #[test]
    fn hit_bounds_both() {
        assert!(evaluate_hit_bounds(15, Some(10), Some(20)));
        assert!(!evaluate_hit_bounds(5, Some(10), Some(20)));
        assert!(!evaluate_hit_bounds(25, Some(10), Some(20)));
    }

    #[test]
    fn hit_bounds_neither() {
        assert!(evaluate_hit_bounds(1, None, None));
        assert!(!evaluate_hit_bounds(0, None, None));
    }

    #[test]
    fn rate_limiter_allows_within_limit() {
        let mut limiter = RateLimiter::new(3);
        assert!(limiter.try_acquire());
        assert!(limiter.try_acquire());
        assert!(limiter.try_acquire());
        assert!(!limiter.try_acquire()); // 4th exceeds limit
    }

    #[test]
    fn resolve_templates_in_query() {
        let query = serde_json::json!({
            "bool": {
                "must": [
                    { "term": { "memberCode.keyword": "{{ anomaly.key }}" } },
                    { "term": { "entityType": "{{anomaly.entity_type}}" } }
                ]
            }
        });

        let rule_match = crate::templates::RuleMatch {
            entity_id: "uuid-123".to_string(),
            entity_key: "M0042".to_string(),
            entity_type: "Member".to_string(),
            score: 0.9,
            signals: vec![],
            matched_reason: "test".to_string(),
        };

        let resolved = resolve_query_templates(&query, &rule_match);
        let json_str = serde_json::to_string(&resolved).unwrap();

        assert!(json_str.contains("M0042"));
        assert!(json_str.contains("Member"));
        assert!(!json_str.contains("{{ anomaly.key }}"));
    }

    #[test]
    fn enrichment_result_skipped_is_pass() {
        let result = EnrichmentResult::skipped();
        assert!(result.passed);
        assert_eq!(result.hit_count, 0);
    }

    #[tokio::test]
    async fn disabled_engine_skips_enrichment() {
        let engine = EnrichmentEngine::disabled();

        let config = OpenSearchEnrichment {
            query: serde_json::json!({"match_all": {}}),
            min_hits: Some(1),
            max_hits: None,
            rate_limit: 60,
            timeout_ms: Some(5000),
        };

        let rule_match = crate::templates::RuleMatch {
            entity_id: "e1".to_string(),
            entity_key: "M001".to_string(),
            entity_type: "Member".to_string(),
            score: 0.9,
            signals: vec![],
            matched_reason: "test".to_string(),
        };

        let result = engine.enrich("rule-1", &config, &rule_match).await;
        assert!(result.passed, "Disabled engine should pass (fail-open)");
    }

    #[tokio::test]
    async fn mock_enrichment_with_hits() {
        struct MockClient;

        #[async_trait::async_trait]
        impl OpenSearchQuery for MockClient {
            async fn search(
                &self,
                _query_body: &serde_json::Value,
                _timeout_ms: u64,
            ) -> Result<SearchResult, EnrichmentError> {
                Ok(SearchResult {
                    total_hits: 25,
                    sample_hits: vec![
                        serde_json::json!({"_id": "1", "memberCode": "M0042"}),
                        serde_json::json!({"_id": "2", "memberCode": "M0042"}),
                    ],
                    took_ms: 42,
                })
            }
        }

        let engine = EnrichmentEngine::new(Box::new(MockClient));

        let config = OpenSearchEnrichment {
            query: serde_json::json!({"match_all": {}}),
            min_hits: Some(20),
            max_hits: None,
            rate_limit: 60,
            timeout_ms: Some(5000),
        };

        let rule_match = crate::templates::RuleMatch {
            entity_id: "e1".to_string(),
            entity_key: "M0042".to_string(),
            entity_type: "Member".to_string(),
            score: 0.9,
            signals: vec![],
            matched_reason: "test".to_string(),
        };

        let result = engine.enrich("rule-1", &config, &rule_match).await;
        assert!(result.passed);
        assert_eq!(result.hit_count, 25);
        assert_eq!(result.sample_hits.len(), 2);
        assert_eq!(result.query_time_ms, 42);
    }

    #[tokio::test]
    async fn mock_enrichment_below_min_hits() {
        struct MockLowHits;

        #[async_trait::async_trait]
        impl OpenSearchQuery for MockLowHits {
            async fn search(
                &self,
                _query_body: &serde_json::Value,
                _timeout_ms: u64,
            ) -> Result<SearchResult, EnrichmentError> {
                Ok(SearchResult {
                    total_hits: 3,
                    sample_hits: vec![],
                    took_ms: 10,
                })
            }
        }

        let engine = EnrichmentEngine::new(Box::new(MockLowHits));

        let config = OpenSearchEnrichment {
            query: serde_json::json!({"match_all": {}}),
            min_hits: Some(20),
            max_hits: None,
            rate_limit: 60,
            timeout_ms: None,
        };

        let rule_match = crate::templates::RuleMatch {
            entity_id: "e1".to_string(),
            entity_key: "M001".to_string(),
            entity_type: "Member".to_string(),
            score: 0.9,
            signals: vec![],
            matched_reason: "test".to_string(),
        };

        let result = engine.enrich("rule-1", &config, &rule_match).await;
        assert!(!result.passed, "3 hits should fail min_hits=20");
    }

    #[tokio::test]
    async fn mock_enrichment_timeout_skips() {
        struct MockTimeout;

        #[async_trait::async_trait]
        impl OpenSearchQuery for MockTimeout {
            async fn search(
                &self,
                _query_body: &serde_json::Value,
                timeout_ms: u64,
            ) -> Result<SearchResult, EnrichmentError> {
                Err(EnrichmentError::Timeout(timeout_ms))
            }
        }

        let engine = EnrichmentEngine::new(Box::new(MockTimeout));

        let config = OpenSearchEnrichment {
            query: serde_json::json!({"match_all": {}}),
            min_hits: Some(1),
            max_hits: None,
            rate_limit: 60,
            timeout_ms: Some(1000),
        };

        let rule_match = crate::templates::RuleMatch {
            entity_id: "e1".to_string(),
            entity_key: "M001".to_string(),
            entity_type: "Member".to_string(),
            score: 0.9,
            signals: vec![],
            matched_reason: "test".to_string(),
        };

        let result = engine.enrich("rule-1", &config, &rule_match).await;
        assert!(result.passed, "Timeout should fail-open");
    }
}
