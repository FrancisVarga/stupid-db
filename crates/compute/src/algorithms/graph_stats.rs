use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use stupid_core::{EdgeType, EntityType, NodeId};
use stupid_graph::GraphStore;

/// Extended graph statistics beyond basic node/edge counts.
///
/// Includes structural metrics like average degree, density,
/// and connected component count using union-find.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtendedGraphStats {
    pub total_nodes: usize,
    pub total_edges: usize,
    pub nodes_by_type: HashMap<EntityType, usize>,
    pub edges_by_type: HashMap<EdgeType, usize>,
    pub avg_degree: f64,
    pub max_degree: (NodeId, usize),
    pub connected_components: usize,
    pub density: f64,
}

/// Compute extended graph statistics from a GraphStore.
pub fn extended_graph_stats(graph: &GraphStore) -> ExtendedGraphStats {
    let total_nodes = graph.nodes.len();
    let total_edges = graph.edges.len();

    // Count nodes by entity type
    let mut nodes_by_type: HashMap<EntityType, usize> = HashMap::new();
    for node in graph.nodes.values() {
        *nodes_by_type.entry(node.entity_type).or_insert(0) += 1;
    }

    // Count edges by edge type
    let mut edges_by_type: HashMap<EdgeType, usize> = HashMap::new();
    for edge in graph.edges.values() {
        *edges_by_type.entry(edge.edge_type).or_insert(0) += 1;
    }

    // Compute degree for each node (outgoing + incoming)
    let mut max_degree = (NodeId::nil(), 0usize);
    let mut total_degree = 0usize;

    for &node_id in graph.nodes.keys() {
        let out = graph.outgoing.get(&node_id).map_or(0, |v| v.len());
        let inc = graph.incoming.get(&node_id).map_or(0, |v| v.len());
        let deg = out + inc;
        total_degree += deg;

        if deg > max_degree.1 {
            max_degree = (node_id, deg);
        }
    }

    let avg_degree = if total_nodes > 0 {
        total_degree as f64 / total_nodes as f64
    } else {
        0.0
    };

    // Density: 2*E / (N*(N-1)) for undirected interpretation
    let density = if total_nodes > 1 {
        (2.0 * total_edges as f64) / (total_nodes as f64 * (total_nodes as f64 - 1.0))
    } else {
        0.0
    };

    // Connected components via union-find
    let connected_components = count_connected_components(graph);

    ExtendedGraphStats {
        total_nodes,
        total_edges,
        nodes_by_type,
        edges_by_type,
        avg_degree,
        max_degree,
        connected_components,
        density,
    }
}

/// Union-Find (disjoint set) with path compression and union by rank.
struct UnionFind {
    parent: Vec<usize>,
    rank: Vec<usize>,
}

impl UnionFind {
    fn new(n: usize) -> Self {
        Self {
            parent: (0..n).collect(),
            rank: vec![0; n],
        }
    }

    fn find(&mut self, x: usize) -> usize {
        if self.parent[x] != x {
            self.parent[x] = self.find(self.parent[x]);
        }
        self.parent[x]
    }

    fn union(&mut self, a: usize, b: usize) {
        let ra = self.find(a);
        let rb = self.find(b);
        if ra == rb {
            return;
        }
        match self.rank[ra].cmp(&self.rank[rb]) {
            std::cmp::Ordering::Less => self.parent[ra] = rb,
            std::cmp::Ordering::Greater => self.parent[rb] = ra,
            std::cmp::Ordering::Equal => {
                self.parent[rb] = ra;
                self.rank[ra] += 1;
            }
        }
    }
}

/// Count connected components treating edges as undirected.
fn count_connected_components(graph: &GraphStore) -> usize {
    let n = graph.nodes.len();
    if n == 0 {
        return 0;
    }

    // Map NodeIds to dense indices
    let node_ids: Vec<NodeId> = graph.nodes.keys().copied().collect();
    let id_to_idx: HashMap<NodeId, usize> = node_ids
        .iter()
        .enumerate()
        .map(|(i, &id)| (id, i))
        .collect();

    let mut uf = UnionFind::new(n);

    for edge in graph.edges.values() {
        if let (Some(&a), Some(&b)) = (id_to_idx.get(&edge.source), id_to_idx.get(&edge.target)) {
            uf.union(a, b);
        }
    }

    // Count distinct roots
    let mut roots = std::collections::HashSet::new();
    for i in 0..n {
        roots.insert(uf.find(i));
    }
    roots.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use stupid_core::{EdgeType, EntityType};

    fn build_test_graph() -> GraphStore {
        let mut g = GraphStore::new();
        let seg = "test".to_string();

        // A -> B -> C -> A (cycle, 1 component)
        let a = g.upsert_node(EntityType::Member, "alice", &seg);
        let b = g.upsert_node(EntityType::Member, "bob", &seg);
        let c = g.upsert_node(EntityType::Device, "device1", &seg);

        g.add_edge(a, b, EdgeType::LoggedInFrom, &seg);
        g.add_edge(b, c, EdgeType::LoggedInFrom, &seg);
        g.add_edge(c, a, EdgeType::LoggedInFrom, &seg);

        g
    }

    #[test]
    fn stats_basic_counts() {
        let g = build_test_graph();
        let stats = extended_graph_stats(&g);

        assert_eq!(stats.total_nodes, 3);
        assert_eq!(stats.total_edges, 3);
        assert_eq!(stats.nodes_by_type[&EntityType::Member], 2);
        assert_eq!(stats.nodes_by_type[&EntityType::Device], 1);
        assert_eq!(stats.edges_by_type[&EdgeType::LoggedInFrom], 3);
    }

    #[test]
    fn stats_degree() {
        let g = build_test_graph();
        let stats = extended_graph_stats(&g);

        // Each node in a 3-node cycle has degree 2 (1 out + 1 in)
        assert!((stats.avg_degree - 2.0).abs() < 1e-10);
        assert_eq!(stats.max_degree.1, 2);
    }

    #[test]
    fn stats_single_component() {
        let g = build_test_graph();
        let stats = extended_graph_stats(&g);
        assert_eq!(stats.connected_components, 1);
    }

    #[test]
    fn stats_multiple_components() {
        let mut g = GraphStore::new();
        let seg = "test".to_string();

        // Component 1: A -> B
        let a = g.upsert_node(EntityType::Member, "alice", &seg);
        let b = g.upsert_node(EntityType::Member, "bob", &seg);
        g.add_edge(a, b, EdgeType::LoggedInFrom, &seg);

        // Component 2: C -> D
        let c = g.upsert_node(EntityType::Device, "d1", &seg);
        let d = g.upsert_node(EntityType::Device, "d2", &seg);
        g.add_edge(c, d, EdgeType::LoggedInFrom, &seg);

        // Component 3: E (isolated)
        let _e = g.upsert_node(EntityType::Game, "game1", &seg);

        let stats = extended_graph_stats(&g);
        assert_eq!(stats.connected_components, 3);
        assert_eq!(stats.total_nodes, 5);
        assert_eq!(stats.total_edges, 2);
    }

    #[test]
    fn stats_density() {
        let g = build_test_graph();
        let stats = extended_graph_stats(&g);

        // 3 nodes, 3 edges: density = 2*3 / (3*2) = 1.0
        assert!((stats.density - 1.0).abs() < 1e-10);
    }

    #[test]
    fn stats_empty_graph() {
        let g = GraphStore::new();
        let stats = extended_graph_stats(&g);

        assert_eq!(stats.total_nodes, 0);
        assert_eq!(stats.total_edges, 0);
        assert_eq!(stats.connected_components, 0);
        assert!((stats.avg_degree - 0.0).abs() < 1e-10);
        assert!((stats.density - 0.0).abs() < 1e-10);
    }

    #[test]
    fn stats_mixed_edge_types() {
        let mut g = GraphStore::new();
        let seg = "test".to_string();

        let m = g.upsert_node(EntityType::Member, "alice", &seg);
        let d = g.upsert_node(EntityType::Device, "dev1", &seg);
        let game = g.upsert_node(EntityType::Game, "slots", &seg);

        g.add_edge(m, d, EdgeType::LoggedInFrom, &seg);
        g.add_edge(m, game, EdgeType::OpenedGame, &seg);

        let stats = extended_graph_stats(&g);
        assert_eq!(stats.edges_by_type[&EdgeType::LoggedInFrom], 1);
        assert_eq!(stats.edges_by_type[&EdgeType::OpenedGame], 1);
        assert_eq!(stats.connected_components, 1);
    }
}
