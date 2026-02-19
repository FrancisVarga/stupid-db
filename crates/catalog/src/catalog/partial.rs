use std::collections::{HashMap, HashSet};

use stupid_graph::GraphStore;
use tracing::info;

use super::types::{CatalogEntry, EdgeSummary, PartialCatalog};

impl PartialCatalog {
    /// Extract one segment's contribution from the graph.
    ///
    /// Nodes are included if their `segment_refs` contains `segment_id`.
    /// Edges are included if their `segment_id` matches exactly.
    pub fn from_graph_segment(graph: &GraphStore, segment_id: &str) -> Self {
        // Collect nodes that belong to this segment.
        let mut type_nodes: HashMap<String, Vec<String>> = HashMap::new();
        let mut segment_node_count = 0usize;

        for node in graph.nodes.values() {
            if node.segment_refs.contains(segment_id) {
                segment_node_count += 1;
                type_nodes
                    .entry(node.entity_type.to_string())
                    .or_default()
                    .push(node.key.clone());
            }
        }

        let mut entity_types: Vec<CatalogEntry> = type_nodes
            .into_iter()
            .map(|(entity_type, keys)| {
                let node_count = keys.len();
                let mut sample_keys: Vec<String> = keys.into_iter().take(5).collect();
                sample_keys.sort();
                CatalogEntry {
                    entity_type,
                    node_count,
                    sample_keys,
                }
            })
            .collect();
        entity_types.sort_by(|a, b| b.node_count.cmp(&a.node_count));

        // Collect edges that belong to this segment.
        let mut edge_info: HashMap<String, (usize, HashSet<String>, HashSet<String>)> =
            HashMap::new();
        let mut segment_edge_count = 0usize;

        for edge in graph.edges.values() {
            if edge.segment_id == segment_id {
                segment_edge_count += 1;
                let entry = edge_info
                    .entry(edge.edge_type.to_string())
                    .or_insert_with(|| (0, HashSet::new(), HashSet::new()));
                entry.0 += 1;

                if let Some(source_node) = graph.nodes.get(&edge.source) {
                    entry.1.insert(source_node.entity_type.to_string());
                }
                if let Some(target_node) = graph.nodes.get(&edge.target) {
                    entry.2.insert(target_node.entity_type.to_string());
                }
            }
        }

        let mut edge_types: Vec<EdgeSummary> = edge_info
            .into_iter()
            .map(|(edge_type, (count, sources, targets))| {
                let mut source_types: Vec<String> = sources.into_iter().collect();
                source_types.sort();
                let mut target_types: Vec<String> = targets.into_iter().collect();
                target_types.sort();
                EdgeSummary {
                    edge_type,
                    count,
                    source_types,
                    target_types,
                }
            })
            .collect();
        edge_types.sort_by(|a, b| b.count.cmp(&a.count));

        info!(
            "Partial catalog for '{}': {} entity types, {} edge types ({} nodes, {} edges)",
            segment_id,
            entity_types.len(),
            edge_types.len(),
            segment_node_count,
            segment_edge_count
        );

        PartialCatalog {
            segment_id: segment_id.to_string(),
            entity_types,
            edge_types,
            node_count: segment_node_count,
            edge_count: segment_edge_count,
        }
    }
}
