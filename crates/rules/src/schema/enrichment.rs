//! Post-detection enrichment configuration.

use serde::{Deserialize, Serialize};

/// Optional enrichment step that runs after detection signals fire.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Enrichment {
    #[serde(default)]
    pub opensearch: Option<OpenSearchEnrichment>,
}

/// OpenSearch query enrichment configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct OpenSearchEnrichment {
    pub query: serde_json::Value,
    #[serde(default)]
    pub min_hits: Option<u64>,
    #[serde(default)]
    pub max_hits: Option<u64>,
    #[serde(default = "default_rate_limit")]
    pub rate_limit: u32,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

fn default_rate_limit() -> u32 {
    60
}
