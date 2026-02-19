//! Core types for OpenSearch enrichment.
//!
//! Defines the client trait, error enum, and result types used
//! across the enrichment subsystem.

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

impl EnrichmentResult {
    /// A skipped enrichment always passes (fail-open behavior).
    pub(crate) fn skipped() -> Self {
        Self {
            passed: true,
            hit_count: 0,
            sample_hits: Vec::new(),
            query_time_ms: 0,
        }
    }
}
