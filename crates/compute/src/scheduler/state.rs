use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, RwLock};

use stupid_core::{EntityType, NodeId};

use crate::algorithms::degree::DegreeInfo;

use super::types::{
    AnomalyScore, ClusterId, ClusterInfo, CommunityId, Insight, SparseMatrix, TemporalPattern,
    Trend,
};

/// Shared materialized knowledge produced by compute tasks.
///
/// All compute results are written here. The dashboard and query engine
/// read from this state via the `Arc<RwLock<_>>` wrapper.
#[derive(Debug, Default)]
pub struct KnowledgeState {
    /// Cluster assignments: member -> cluster_id
    pub clusters: HashMap<NodeId, ClusterId>,
    /// Cluster centroids and metadata.
    pub cluster_info: HashMap<ClusterId, ClusterInfo>,
    /// Community assignments: node -> community_id
    pub communities: HashMap<NodeId, CommunityId>,
    /// PageRank scores: node -> score
    pub pagerank: HashMap<NodeId, f64>,
    /// Degree centrality: node -> degree info
    pub degrees: HashMap<NodeId, DegreeInfo>,
    /// Anomaly flags: entity -> anomaly_score
    pub anomalies: HashMap<NodeId, AnomalyScore>,
    /// Detected temporal patterns.
    pub patterns: Vec<TemporalPattern>,
    /// Co-occurrence matrices keyed by entity type pair.
    pub cooccurrence: HashMap<(EntityType, EntityType), SparseMatrix>,
    /// Trends: metric name -> trend data.
    pub trends: HashMap<String, Trend>,
    /// Proactive insights queue (newest at back).
    pub insights: VecDeque<Insight>,
}

/// Thread-safe handle to shared knowledge state.
pub type SharedKnowledgeState = Arc<RwLock<KnowledgeState>>;

/// Create a new shared knowledge state.
pub fn new_shared_state() -> SharedKnowledgeState {
    Arc::new(RwLock::new(KnowledgeState::default()))
}
