//! Enrichment engine with per-rule rate limiting.
//!
//! Executes OpenSearch enrichment queries after pipeline signals fire,
//! confirming or refining detections. Each rule has an independent
//! token-bucket rate limiter.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::schema::OpenSearchEnrichment;
use crate::templates::RuleMatch;

use super::query_helpers::{evaluate_hit_bounds, resolve_query_templates};
use super::types::{EnrichmentError, EnrichmentResult, OpenSearchQuery};

// ── Simple token-bucket rate limiter ────────────────────────────────

/// Per-rule token bucket rate limiter.
///
/// Allows `max_per_hour` queries per rolling hour window.
pub(crate) struct RateLimiter {
    max_per_hour: u32,
    /// Timestamps of recent queries within the current window.
    timestamps: Vec<Instant>,
}

impl RateLimiter {
    pub(crate) fn new(max_per_hour: u32) -> Self {
        Self {
            max_per_hour,
            timestamps: Vec::new(),
        }
    }

    /// Try to acquire a token. Returns true if allowed.
    pub(crate) fn try_acquire(&mut self) -> bool {
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
