//! OpenSearch enrichment queries with rate limiting.
//!
//! After pipeline signals fire, an optional enrichment query runs
//! against OpenSearch to confirm or refine the detection.
//! Each rule has an independent rate limiter (token bucket).
//!
//! The enrichment engine uses an [`OpenSearchQuery`] trait to abstract
//! the actual HTTP client, so the rules crate has no SDK dependency.
//! The server injects the real implementation at startup.

mod engine;
mod query_helpers;
mod types;

pub use engine::*;
pub use types::*;

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use query_helpers::evaluate_hit_bounds;

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
        let mut limiter = engine::RateLimiter::new(3);
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

        let resolved = query_helpers::resolve_query_templates(&query, &rule_match);
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

        let config = crate::schema::OpenSearchEnrichment {
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

        let config = crate::schema::OpenSearchEnrichment {
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

        let config = crate::schema::OpenSearchEnrichment {
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

        let config = crate::schema::OpenSearchEnrichment {
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
