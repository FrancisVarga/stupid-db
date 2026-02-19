use std::collections::{HashMap, HashSet};

use stupid_graph::GraphStore;
use tracing::info;

use super::types::{Catalog, CatalogEntry, EdgeSummary, ExternalSource, PartialCatalog};

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
        let mut edge_info: HashMap<String, (usize, HashSet<String>, HashSet<String>)> =
            HashMap::new();

        for edge in graph.edges.values() {
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
            external_sources: Vec::new(),
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

    /// Build a catalog by merging multiple per-segment partial catalogs.
    ///
    /// Entity type counts are summed across partials, sample keys are
    /// merged (capped at 5 per entity type), and edge source/target type
    /// sets are unioned. The result is sorted by count descending, matching
    /// `from_graph()` ordering.
    pub fn from_partials(partials: &[PartialCatalog]) -> Self {
        let mut type_counts: HashMap<String, usize> = HashMap::new();
        let mut type_samples: HashMap<String, Vec<String>> = HashMap::new();
        let mut edge_info: HashMap<String, (usize, HashSet<String>, HashSet<String>)> =
            HashMap::new();
        let mut total_nodes: usize = 0;
        let mut total_edges: usize = 0;

        for partial in partials {
            total_nodes += partial.node_count;
            total_edges += partial.edge_count;

            for entry in &partial.entity_types {
                *type_counts.entry(entry.entity_type.clone()).or_default() += entry.node_count;
                let samples = type_samples.entry(entry.entity_type.clone()).or_default();
                for key in &entry.sample_keys {
                    if samples.len() < 5 && !samples.contains(key) {
                        samples.push(key.clone());
                    }
                }
            }

            for edge in &partial.edge_types {
                let e = edge_info
                    .entry(edge.edge_type.clone())
                    .or_insert_with(|| (0, HashSet::new(), HashSet::new()));
                e.0 += edge.count;
                e.1.extend(edge.source_types.iter().cloned());
                e.2.extend(edge.target_types.iter().cloned());
            }
        }

        let mut entity_types: Vec<CatalogEntry> = type_counts
            .into_iter()
            .map(|(entity_type, node_count)| {
                let mut sample_keys = type_samples.remove(&entity_type).unwrap_or_default();
                sample_keys.sort();
                CatalogEntry {
                    entity_type,
                    node_count,
                    sample_keys,
                }
            })
            .collect();
        entity_types.sort_by(|a, b| b.node_count.cmp(&a.node_count));

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

        Catalog {
            entity_types,
            edge_types,
            total_nodes,
            total_edges,
            external_sources: Vec::new(),
        }
    }

    /// Attach external SQL sources (e.g. Athena, Trino) to the catalog.
    pub fn with_external_sources(mut self, sources: Vec<ExternalSource>) -> Self {
        self.external_sources = sources;
        self
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
                "  - {} ({} edges): {} \u{2192} {}",
                edge.edge_type,
                edge.count,
                edge.source_types.join("|"),
                edge.target_types.join("|"),
            ));
        }

        // External SQL sources (Athena, Trino, etc.)
        if !self.external_sources.is_empty() {
            lines.push(String::new());
            lines.push("External SQL sources:".to_string());
            for src in &self.external_sources {
                lines.push(format!(
                    "  {} (kind: {}, id: {}):",
                    src.name, src.kind, src.connection_id
                ));
                for db in &src.databases {
                    lines.push(format!("    database {}:", db.name));
                    for table in &db.tables {
                        let cols: Vec<String> = table
                            .columns
                            .iter()
                            .map(|c| format!("{} {}", c.name, c.data_type))
                            .collect();
                        lines.push(format!("      table {} ({})", table.name, cols.join(", ")));
                    }
                }
            }
        }

        lines.join("\n")
    }
}
