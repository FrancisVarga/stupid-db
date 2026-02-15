use std::collections::HashMap;

use stupid_core::NodeId;
use stupid_graph::GraphStore;
use tracing::info;

/// Detect communities via label propagation.
///
/// Each node starts with a unique label (0..N). On each iteration, every node
/// adopts the most frequent label among its neighbors. Ties are broken by
/// choosing the smallest label for determinism.
///
/// Returns a map of node ID to community label.
pub fn label_propagation(graph: &GraphStore, max_iterations: usize) -> HashMap<NodeId, u64> {
    let node_ids: Vec<NodeId> = graph.nodes.keys().copied().collect();
    if node_ids.is_empty() {
        return HashMap::new();
    }

    // Assign initial labels 0..N
    let mut labels: HashMap<NodeId, u64> = node_ids
        .iter()
        .enumerate()
        .map(|(i, &id)| (id, i as u64))
        .collect();

    for iteration in 0..max_iterations {
        let mut changed = false;

        for &node_id in &node_ids {
            // Count neighbor labels (both incoming and outgoing)
            let mut label_counts: HashMap<u64, usize> = HashMap::new();

            if let Some(edge_ids) = graph.outgoing.get(&node_id) {
                for eid in edge_ids {
                    if let Some(edge) = graph.edges.get(eid) {
                        if let Some(&label) = labels.get(&edge.target) {
                            *label_counts.entry(label).or_default() += 1;
                        }
                    }
                }
            }

            if let Some(edge_ids) = graph.incoming.get(&node_id) {
                for eid in edge_ids {
                    if let Some(edge) = graph.edges.get(eid) {
                        if let Some(&label) = labels.get(&edge.source) {
                            *label_counts.entry(label).or_default() += 1;
                        }
                    }
                }
            }

            if label_counts.is_empty() {
                continue; // isolated node keeps its label
            }

            // Find max-frequency label, break ties with smallest label
            let max_count = *label_counts.values().max().unwrap();
            let best_label = label_counts
                .iter()
                .filter(|(_, &count)| count == max_count)
                .map(|(&label, _)| label)
                .min()
                .unwrap();

            if labels[&node_id] != best_label {
                labels.insert(node_id, best_label);
                changed = true;
            }
        }

        if !changed {
            info!(
                "Label propagation converged after {} iterations",
                iteration + 1
            );
            return labels;
        }
    }

    info!(
        "Label propagation completed {} iterations without convergence",
        max_iterations
    );
    labels
}

/// Run label propagation with default max iterations (10).
pub fn label_propagation_default(graph: &GraphStore) -> HashMap<NodeId, u64> {
    label_propagation(graph, 10)
}

#[cfg(test)]
mod tests {
    use super::*;
    use stupid_core::{EdgeType, EntityType};

    #[test]
    fn communities_connected_components() {
        let mut g = GraphStore::new();
        let seg = "test".to_string();

        // Two disconnected pairs: (A-B) and (C-D)
        let a = g.upsert_node(EntityType::Member, "alice", &seg);
        let b = g.upsert_node(EntityType::Member, "bob", &seg);
        let c = g.upsert_node(EntityType::Member, "carol", &seg);
        let d = g.upsert_node(EntityType::Member, "dave", &seg);

        g.add_edge(a, b, EdgeType::LoggedInFrom, &seg);
        g.add_edge(b, a, EdgeType::LoggedInFrom, &seg);
        g.add_edge(c, d, EdgeType::LoggedInFrom, &seg);
        g.add_edge(d, c, EdgeType::LoggedInFrom, &seg);

        let labels = label_propagation_default(&g);

        // A and B should share a label
        assert_eq!(labels[&a], labels[&b]);
        // C and D should share a label
        assert_eq!(labels[&c], labels[&d]);
        // The two groups should differ
        assert_ne!(labels[&a], labels[&c]);
    }

    #[test]
    fn communities_empty() {
        let g = GraphStore::new();
        let labels = label_propagation_default(&g);
        assert!(labels.is_empty());
    }
}
