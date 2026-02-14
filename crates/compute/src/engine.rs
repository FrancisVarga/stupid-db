use std::collections::HashMap;

use stupid_core::NodeId;
use stupid_graph::GraphStore;
use tracing::info;

use crate::algorithms::{communities, degree, pagerank};

pub use degree::DegreeInfo;

/// Pre-computed graph analytics, built once after graph construction.
pub struct ComputeEngine {
    pub pagerank: HashMap<NodeId, f64>,
    pub degrees: HashMap<NodeId, DegreeInfo>,
    pub communities: HashMap<NodeId, u64>,
}

impl ComputeEngine {
    /// Run all graph algorithms against the given store.
    pub fn run_all(graph: &GraphStore) -> Self {
        let start = std::time::Instant::now();

        info!("Running PageRank...");
        let pr_start = std::time::Instant::now();
        let pagerank = pagerank::pagerank_default(graph);
        info!("  PageRank done in {:.1}s", pr_start.elapsed().as_secs_f64());

        info!("Running degree centrality...");
        let degrees = degree::degree_centrality(graph);

        info!("Running label propagation communities...");
        let lp_start = std::time::Instant::now();
        let communities = communities::label_propagation_default(graph);
        info!(
            "  Label propagation done in {:.1}s",
            lp_start.elapsed().as_secs_f64()
        );

        let unique_communities: std::collections::HashSet<u64> =
            communities.values().copied().collect();
        info!(
            "Compute complete in {:.1}s — {} communities detected",
            start.elapsed().as_secs_f64(),
            unique_communities.len()
        );

        Self {
            pagerank,
            degrees,
            communities,
        }
    }
}
