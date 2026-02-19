use serde::{Deserialize, Serialize};

/// Describes a single entity type discovered in the graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogEntry {
    pub entity_type: String,
    pub node_count: usize,
    pub sample_keys: Vec<String>,
}

/// Describes an edge type discovered in the graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeSummary {
    pub edge_type: String,
    pub count: usize,
    pub source_types: Vec<String>,
    pub target_types: Vec<String>,
}

/// An external SQL-queryable data source (e.g. Athena, Trino).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalSource {
    /// Human-readable name (e.g. "Production Data Lake").
    pub name: String,
    /// Source kind (e.g. "athena", "trino", "postgres").
    pub kind: String,
    /// Connection identifier for routing queries.
    pub connection_id: String,
    pub databases: Vec<ExternalDatabase>,
}

/// A database within an external source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalDatabase {
    pub name: String,
    pub tables: Vec<ExternalTable>,
}

/// A table within an external database.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalTable {
    pub name: String,
    pub columns: Vec<ExternalColumn>,
}

/// A column within an external table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalColumn {
    pub name: String,
    pub data_type: String,
}

/// Schema catalog auto-discovered from the loaded graph and external sources.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Catalog {
    pub entity_types: Vec<CatalogEntry>,
    pub edge_types: Vec<EdgeSummary>,
    pub total_nodes: usize,
    pub total_edges: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub external_sources: Vec<ExternalSource>,
}

/// A single segment's contribution to the overall catalog.
///
/// Built by filtering the graph to only nodes/edges associated with a
/// specific segment. Used as the building block for incremental catalog
/// updates -- new segments produce a `PartialCatalog` that gets merged
/// into the persisted `Catalog`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartialCatalog {
    pub segment_id: String,
    pub entity_types: Vec<CatalogEntry>,
    pub edge_types: Vec<EdgeSummary>,
    pub node_count: usize,
    pub edge_count: usize,
}
