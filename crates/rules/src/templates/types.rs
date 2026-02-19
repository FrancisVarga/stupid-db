//! Data types used as input and output for template evaluators.

/// Per-entity data used as input to template evaluators.
#[derive(Debug, Clone)]
pub struct EntityData {
    /// Human-readable key (e.g., member code).
    pub key: String,
    /// Entity type label (e.g., "Member").
    pub entity_type: String,
    /// 10-element feature vector matching `FEATURE_NAMES` order.
    pub features: Vec<f64>,
    /// Anomaly score from the compute pipeline.
    pub score: f64,
    /// Cluster assignment, if available.
    pub cluster_id: Option<usize>,
}

/// Aggregate statistics for a single cluster, used as baseline in spike detection.
#[derive(Debug, Clone)]
pub struct ClusterStats {
    /// Mean feature vector (centroid) of the cluster.
    pub centroid: Vec<f64>,
    /// Number of members in the cluster.
    pub member_count: usize,
}

/// A single detection match produced by a template evaluator.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RuleMatch {
    /// Node ID as string.
    pub entity_id: String,
    /// Human-readable key (e.g., member code).
    pub entity_key: String,
    /// Entity type label.
    pub entity_type: String,
    /// Detection score (typically the feature value or distance).
    pub score: f64,
    /// Signals that contributed to the match: (name, value) pairs.
    pub signals: Vec<(String, f64)>,
    /// Human-readable explanation of why this entity matched.
    pub matched_reason: String,
}
