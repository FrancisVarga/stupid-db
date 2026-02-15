use std::collections::HashMap;

use serde::Serialize;
use stupid_graph::GraphStore;
use tracing::info;

/// Describes a single entity type discovered in the graph.
#[derive(Debug, Clone, Serialize)]
pub struct CatalogEntry {
    pub entity_type: String,
    pub node_count: usize,
    pub sample_keys: Vec<String>,
}

/// Describes an edge type discovered in the graph.
#[derive(Debug, Clone, Serialize)]
pub struct EdgeSummary {
    pub edge_type: String,
    pub count: usize,
    pub source_types: Vec<String>,
    pub target_types: Vec<String>,
}

/// Schema catalog auto-discovered from the loaded graph.
#[derive(Debug, Clone, Serialize)]
pub struct Catalog {
    pub entity_types: Vec<CatalogEntry>,
    pub edge_types: Vec<EdgeSummary>,
    pub total_nodes: usize,
    pub total_edges: usize,
}

impl Catalog {
    /// Inspect a GraphStore and build a catalog of all entity/edge types.
    pub fn from_graph(graph: &GraphStore) -> Self {
        // Count nodes per entity type and collect sample keys
        let mut type_nodes: HashMap<String, Vec<String>> = HashMap::new();
        for node in graph.nodes.values() {
            type_nodes
                .entry(node.entity_type.to_string())
                .or_default()
                .push(node.key.clone());
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

        // Analyze edges: group by edge_type, track source/target entity types
        let mut edge_info: HashMap<String, (usize, std::collections::HashSet<String>, std::collections::HashSet<String>)> =
            HashMap::new();

        for edge in graph.edges.values() {
            let entry = edge_info
                .entry(edge.edge_type.to_string())
                .or_insert_with(|| (0, std::collections::HashSet::new(), std::collections::HashSet::new()));
            entry.0 += 1;

            if let Some(source_node) = graph.nodes.get(&edge.source) {
                entry.1.insert(source_node.entity_type.to_string());
            }
            if let Some(target_node) = graph.nodes.get(&edge.target) {
                entry.2.insert(target_node.entity_type.to_string());
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

        let catalog = Catalog {
            total_nodes: graph.nodes.len(),
            total_edges: graph.edges.len(),
            entity_types,
            edge_types,
        };

        info!(
            "Catalog built: {} entity types, {} edge types ({} nodes, {} edges)",
            catalog.entity_types.len(),
            catalog.edge_types.len(),
            catalog.total_nodes,
            catalog.total_edges
        );

        catalog
    }

    /// Generate a natural-language schema description for an LLM system prompt.
    pub fn to_system_prompt(&self) -> String {
        let mut lines = Vec::new();
        lines.push(format!(
            "The graph contains {} nodes and {} edges.\n",
            self.total_nodes, self.total_edges
        ));

        lines.push("Entity types:".to_string());
        for entry in &self.entity_types {
            let samples = if entry.sample_keys.is_empty() {
                String::new()
            } else {
                format!(" (examples: {})", entry.sample_keys.join(", "))
            };
            lines.push(format!(
                "  - {} ({} nodes){}",
                entry.entity_type, entry.node_count, samples
            ));
        }

        lines.push(String::new());
        lines.push("Edge types:".to_string());
        for edge in &self.edge_types {
            lines.push(format!(
                "  - {} ({} edges): {} → {}",
                edge.edge_type,
                edge.count,
                edge.source_types.join("|"),
                edge.target_types.join("|"),
            ));
        }

        lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use stupid_core::{EdgeType, EntityType};

    fn build_test_graph() -> GraphStore {
        let mut g = GraphStore::new();
        let seg = "test".to_string();

        let a = g.upsert_node(EntityType::Member, "alice", &seg);
        let b = g.upsert_node(EntityType::Member, "bob", &seg);
        let d = g.upsert_node(EntityType::Device, "iphone-1", &seg);

        g.add_edge(a, d, EdgeType::LoggedInFrom, &seg);
        g.add_edge(b, d, EdgeType::LoggedInFrom, &seg);
        g
    }

    #[test]
    fn catalog_from_graph() {
        let g = build_test_graph();
        let cat = Catalog::from_graph(&g);

        assert_eq!(cat.total_nodes, 3);
        assert_eq!(cat.total_edges, 2);
        assert_eq!(cat.entity_types.len(), 2);
        assert_eq!(cat.edge_types.len(), 1);

        // Members should come first (2 > 1)
        assert_eq!(cat.entity_types[0].entity_type, "Member");
        assert_eq!(cat.entity_types[0].node_count, 2);

        // Edge should be LoggedInFrom: Member → Device
        assert_eq!(cat.edge_types[0].edge_type, "LoggedInFrom");
        assert_eq!(cat.edge_types[0].source_types, vec!["Member"]);
        assert_eq!(cat.edge_types[0].target_types, vec!["Device"]);
    }

    #[test]
    fn catalog_system_prompt() {
        let g = build_test_graph();
        let cat = Catalog::from_graph(&g);
        let prompt = cat.to_system_prompt();

        assert!(prompt.contains("3 nodes"));
        assert!(prompt.contains("2 edges"));
        assert!(prompt.contains("Member"));
        assert!(prompt.contains("LoggedInFrom"));
        assert!(prompt.contains("Member → Device"));
    }

    #[test]
    fn catalog_empty_graph() {
        let g = GraphStore::new();
        let cat = Catalog::from_graph(&g);
        assert_eq!(cat.total_nodes, 0);
        assert_eq!(cat.total_edges, 0);
        assert!(cat.entity_types.is_empty());
        assert!(cat.edge_types.is_empty());
    }
}
