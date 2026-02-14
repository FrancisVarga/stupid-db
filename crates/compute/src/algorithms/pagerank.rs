use std::collections::HashMap;

use stupid_core::NodeId;
use stupid_graph::GraphStore;
use tracing::info;

/// Compute PageRank scores using the iterative power method.
///
/// Returns a map of node ID to PageRank score. Scores sum to 1.0.
pub fn pagerank(
    graph: &GraphStore,
    damping: f64,
    max_iterations: usize,
    convergence: f64,
) -> HashMap<NodeId, f64> {
    let n = graph.nodes.len();
    if n == 0 {
        return HashMap::new();
    }

    let node_ids: Vec<NodeId> = graph.nodes.keys().copied().collect();
    let initial = 1.0 / n as f64;

    let mut scores: HashMap<NodeId, f64> = node_ids.iter().map(|&id| (id, initial)).collect();

    // Pre-compute out-degrees for each node
    let out_degree: HashMap<NodeId, usize> = node_ids
        .iter()
        .map(|&id| {
            let deg = graph.outgoing.get(&id).map_or(0, |v| v.len());
            (id, deg)
        })
        .collect();

    let base = (1.0 - damping) / n as f64;

    for iteration in 0..max_iterations {
        let mut new_scores: HashMap<NodeId, f64> = HashMap::with_capacity(n);

        for &node_id in &node_ids {
            let mut sum = 0.0;

            // Sum contributions from nodes that link TO this node
            if let Some(incoming_edges) = graph.incoming.get(&node_id) {
                for edge_id in incoming_edges {
                    if let Some(edge) = graph.edges.get(edge_id) {
                        let source = edge.source;
                        let source_out = *out_degree.get(&source).unwrap_or(&1);
                        sum += scores.get(&source).unwrap_or(&0.0) / source_out as f64;
                    }
                }
            }

            new_scores.insert(node_id, base + damping * sum);
        }

        // Check convergence (L1 norm)
        let diff: f64 = node_ids
            .iter()
            .map(|id| (new_scores[id] - scores[id]).abs())
            .sum();

        scores = new_scores;

        if diff < convergence {
            info!(
                "PageRank converged after {} iterations (diff={:.2e})",
                iteration + 1,
                diff
            );
            return scores;
        }
    }

    info!(
        "PageRank completed {} iterations without convergence",
        max_iterations
    );
    scores
}

/// Run PageRank with default parameters (damping=0.85, 20 iterations, threshold=1e-6).
pub fn pagerank_default(graph: &GraphStore) -> HashMap<NodeId, f64> {
    pagerank(graph, 0.85, 20, 1e-6)
}

#[cfg(test)]
mod tests {
    use super::*;
    use stupid_core::{EdgeType, EntityType};

    fn build_test_graph() -> GraphStore {
        let mut g = GraphStore::new();
        let seg = "test".to_string();

        // A -> B -> C -> A (cycle)
        let a = g.upsert_node(EntityType::Member, "alice", &seg);
        let b = g.upsert_node(EntityType::Member, "bob", &seg);
        let c = g.upsert_node(EntityType::Member, "carol", &seg);

        g.add_edge(a, b, EdgeType::LoggedInFrom, &seg);
        g.add_edge(b, c, EdgeType::LoggedInFrom, &seg);
        g.add_edge(c, a, EdgeType::LoggedInFrom, &seg);

        g
    }

    #[test]
    fn pagerank_cycle_equal() {
        let g = build_test_graph();
        let pr = pagerank_default(&g);

        assert_eq!(pr.len(), 3);

        // In a 3-node cycle, all nodes should have roughly equal rank
        let values: Vec<f64> = pr.values().copied().collect();
        let mean = values.iter().sum::<f64>() / values.len() as f64;
        for v in &values {
            assert!((v - mean).abs() < 1e-4, "expected ~{}, got {}", mean, v);
        }
    }

    #[test]
    fn pagerank_empty() {
        let g = GraphStore::new();
        let pr = pagerank_default(&g);
        assert!(pr.is_empty());
    }
}
