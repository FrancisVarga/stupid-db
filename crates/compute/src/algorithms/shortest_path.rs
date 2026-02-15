use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};

use stupid_core::NodeId;
use stupid_graph::GraphStore;

/// A priority queue entry for Dijkstra's algorithm.
///
/// Uses reversed ordering so `BinaryHeap` (a max-heap) behaves as a min-heap.
#[derive(Debug, Clone)]
struct State {
    distance: f64,
    node: NodeId,
}

impl PartialEq for State {
    fn eq(&self, other: &Self) -> bool {
        self.distance.total_cmp(&other.distance) == Ordering::Equal
    }
}

impl Eq for State {}

impl Ord for State {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reverse ordering for min-heap behavior
        other.distance.total_cmp(&self.distance)
    }
}

impl PartialOrd for State {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Compute the shortest path between two nodes using weighted Dijkstra.
///
/// Edge weights are **inverted**: distance = 1.0 / edge.weight, so edges with
/// high weight (frequent connections) are treated as "closer". This makes sense
/// for knowledge graphs where weight represents co-occurrence frequency.
///
/// Returns `Some((path, total_distance))` where `path` is the sequence of node IDs
/// from `from` to `to` (inclusive), or `None` if no path exists.
pub fn shortest_path(
    graph: &GraphStore,
    from: NodeId,
    to: NodeId,
) -> Option<(Vec<NodeId>, f64)> {
    // Trivial case: source == target
    if from == to {
        return Some((vec![from], 0.0));
    }

    // Verify both nodes exist
    if !graph.nodes.contains_key(&from) || !graph.nodes.contains_key(&to) {
        return None;
    }

    let mut dist: HashMap<NodeId, f64> = HashMap::new();
    let mut prev: HashMap<NodeId, NodeId> = HashMap::new();
    let mut heap = BinaryHeap::new();

    dist.insert(from, 0.0);
    heap.push(State {
        distance: 0.0,
        node: from,
    });

    while let Some(State { distance, node }) = heap.pop() {
        // Reached the target — reconstruct and return path
        if node == to {
            let path = reconstruct_path(&prev, from, to);
            return Some((path, distance));
        }

        // Skip if we already found a shorter path to this node
        if distance > *dist.get(&node).unwrap_or(&f64::INFINITY) {
            continue;
        }

        // Explore outgoing edges
        if let Some(edge_ids) = graph.outgoing.get(&node) {
            for edge_id in edge_ids {
                if let Some(edge) = graph.edges.get(edge_id) {
                    // Invert weight: high weight = short distance
                    let edge_distance = if edge.weight > 0.0 {
                        1.0 / edge.weight
                    } else {
                        f64::INFINITY
                    };

                    let new_dist = distance + edge_distance;
                    let current = *dist.get(&edge.target).unwrap_or(&f64::INFINITY);

                    if new_dist < current {
                        dist.insert(edge.target, new_dist);
                        prev.insert(edge.target, node);
                        heap.push(State {
                            distance: new_dist,
                            node: edge.target,
                        });
                    }
                }
            }
        }

        // Also explore incoming edges (treat graph as undirected for reachability)
        if let Some(edge_ids) = graph.incoming.get(&node) {
            for edge_id in edge_ids {
                if let Some(edge) = graph.edges.get(edge_id) {
                    let edge_distance = if edge.weight > 0.0 {
                        1.0 / edge.weight
                    } else {
                        f64::INFINITY
                    };

                    let new_dist = distance + edge_distance;
                    let current = *dist.get(&edge.source).unwrap_or(&f64::INFINITY);

                    if new_dist < current {
                        dist.insert(edge.source, new_dist);
                        prev.insert(edge.source, node);
                        heap.push(State {
                            distance: new_dist,
                            node: edge.source,
                        });
                    }
                }
            }
        }
    }

    // No path found
    None
}

/// Reconstruct the path from `from` to `to` by following predecessor links.
fn reconstruct_path(prev: &HashMap<NodeId, NodeId>, from: NodeId, to: NodeId) -> Vec<NodeId> {
    let mut path = vec![to];
    let mut current = to;

    while current != from {
        current = *prev.get(&current).expect("broken predecessor chain");
        path.push(current);
    }

    path.reverse();
    path
}

#[cfg(test)]
mod tests {
    use super::*;
    use stupid_core::{EdgeType, EntityType};

    /// Build a simple linear graph: A -> B -> C
    fn build_linear_graph() -> (GraphStore, NodeId, NodeId, NodeId) {
        let mut g = GraphStore::new();
        let seg = "test".to_string();

        let a = g.upsert_node(EntityType::Member, "alice", &seg);
        let b = g.upsert_node(EntityType::Member, "bob", &seg);
        let c = g.upsert_node(EntityType::Member, "carol", &seg);

        g.add_edge(a, b, EdgeType::LoggedInFrom, &seg);
        g.add_edge(b, c, EdgeType::LoggedInFrom, &seg);

        (g, a, b, c)
    }

    /// Build a diamond graph with different weights:
    ///   A --(1)--> B --(1)--> D
    ///   A --(3)--> C --(3)--> D
    /// Through C should be shorter (weight 3 → distance 1/3 each = 2/3 total)
    fn build_diamond_graph() -> (GraphStore, NodeId, NodeId) {
        let mut g = GraphStore::new();
        let seg = "test".to_string();

        let a = g.upsert_node(EntityType::Member, "alice", &seg);
        let b = g.upsert_node(EntityType::Member, "bob", &seg);
        let c = g.upsert_node(EntityType::Member, "carol", &seg);
        let d = g.upsert_node(EntityType::Member, "dave", &seg);

        // Path through B: weight=1 each → distance = 1/1 + 1/1 = 2.0
        g.add_edge(a, b, EdgeType::LoggedInFrom, &seg);
        g.add_edge(b, d, EdgeType::LoggedInFrom, &seg);

        // Path through C: weight=3 each → distance = 1/3 + 1/3 ≈ 0.667
        // Bump C edges by adding them 3 times (weight increments on dedup)
        g.add_edge(a, c, EdgeType::LoggedInFrom, &seg);
        g.add_edge(a, c, EdgeType::LoggedInFrom, &seg); // weight -> 2
        g.add_edge(a, c, EdgeType::LoggedInFrom, &seg); // weight -> 3

        g.add_edge(c, d, EdgeType::LoggedInFrom, &seg);
        g.add_edge(c, d, EdgeType::LoggedInFrom, &seg); // weight -> 2
        g.add_edge(c, d, EdgeType::LoggedInFrom, &seg); // weight -> 3

        (g, a, d)
    }

    #[test]
    fn shortest_path_linear() {
        let (g, a, _b, c) = build_linear_graph();
        let result = shortest_path(&g, a, c);

        assert!(result.is_some(), "should find a path A -> B -> C");
        let (path, dist) = result.unwrap();

        assert_eq!(path.len(), 3);
        assert_eq!(path[0], a);
        assert_eq!(path[2], c);
        // Each edge has weight=1, so distance = 1/1 + 1/1 = 2.0
        assert!((dist - 2.0).abs() < 1e-9);
    }

    #[test]
    fn shortest_path_diamond_prefers_heavier_edges() {
        let (g, a, d) = build_diamond_graph();
        let result = shortest_path(&g, a, d);

        assert!(result.is_some());
        let (path, dist) = result.unwrap();

        // Should pick path through C (weight=3, distance=1/3+1/3≈0.667)
        // rather than through B (weight=1, distance=1+1=2)
        assert_eq!(path.len(), 3);
        assert_eq!(path[0], a);
        assert_eq!(path[2], d);
        assert!(dist < 1.0, "expected ~0.667, got {}", dist);
        assert!((dist - 2.0 / 3.0).abs() < 1e-9);
    }

    #[test]
    fn shortest_path_same_node() {
        let (g, a, _, _) = build_linear_graph();
        let result = shortest_path(&g, a, a);

        assert!(result.is_some());
        let (path, dist) = result.unwrap();
        assert_eq!(path, vec![a]);
        assert!((dist - 0.0).abs() < 1e-9);
    }

    #[test]
    fn shortest_path_no_path() {
        let mut g = GraphStore::new();
        let seg = "test".to_string();

        // Two disconnected nodes
        let a = g.upsert_node(EntityType::Member, "alice", &seg);
        let b = g.upsert_node(EntityType::Member, "bob", &seg);

        let result = shortest_path(&g, a, b);
        assert!(result.is_none(), "disconnected nodes should return None");
    }

    #[test]
    fn shortest_path_nonexistent_node() {
        let (g, a, _, _) = build_linear_graph();
        let fake = uuid::Uuid::new_v4();

        assert!(shortest_path(&g, a, fake).is_none());
        assert!(shortest_path(&g, fake, a).is_none());
    }

    #[test]
    fn shortest_path_empty_graph() {
        let g = GraphStore::new();
        let a = uuid::Uuid::new_v4();
        let b = uuid::Uuid::new_v4();

        assert!(shortest_path(&g, a, b).is_none());
    }

    #[test]
    fn shortest_path_bidirectional_reachability() {
        let mut g = GraphStore::new();
        let seg = "test".to_string();

        // Only edge: A -> B (directed)
        let a = g.upsert_node(EntityType::Member, "alice", &seg);
        let b = g.upsert_node(EntityType::Member, "bob", &seg);
        g.add_edge(a, b, EdgeType::LoggedInFrom, &seg);

        // B -> A should still work (we traverse incoming edges too)
        let result = shortest_path(&g, b, a);
        assert!(result.is_some(), "should find path B -> A via incoming edge");
        let (path, _) = result.unwrap();
        assert_eq!(path, vec![b, a]);
    }
}
