use std::collections::HashMap;

use serde::Serialize;
use stupid_core::NodeId;
use stupid_graph::GraphStore;

/// Degree information for a single node.
#[derive(Debug, Clone, Serialize)]
pub struct DegreeInfo {
    pub in_deg: usize,
    pub out_deg: usize,
    pub total: usize,
}

/// Compute in-degree, out-degree, and total degree for every node.
pub fn degree_centrality(graph: &GraphStore) -> HashMap<NodeId, DegreeInfo> {
    graph
        .nodes
        .keys()
        .map(|&node_id| {
            let in_deg = graph.incoming.get(&node_id).map_or(0, |v| v.len());
            let out_deg = graph.outgoing.get(&node_id).map_or(0, |v| v.len());
            (
                node_id,
                DegreeInfo {
                    in_deg,
                    out_deg,
                    total: in_deg + out_deg,
                },
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use stupid_core::{EdgeType, EntityType};

    #[test]
    fn degree_basic() {
        let mut g = GraphStore::new();
        let seg = "test".to_string();

        let a = g.upsert_node(EntityType::Member, "alice", &seg);
        let b = g.upsert_node(EntityType::Member, "bob", &seg);
        let c = g.upsert_node(EntityType::Member, "carol", &seg);

        // A -> B, A -> C
        g.add_edge(a, b, EdgeType::LoggedInFrom, &seg);
        g.add_edge(a, c, EdgeType::OpenedGame, &seg);

        let deg = degree_centrality(&g);

        assert_eq!(deg[&a].out_deg, 2);
        assert_eq!(deg[&a].in_deg, 0);
        assert_eq!(deg[&b].in_deg, 1);
        assert_eq!(deg[&c].in_deg, 1);
    }
}
