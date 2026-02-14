use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use serde::Serialize;
use stupid_core::{EdgeId, EdgeType, EntityType, NodeId, SegmentId};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
pub struct Node {
    pub id: NodeId,
    pub entity_type: EntityType,
    pub key: String,
    pub segment_refs: HashSet<SegmentId>,
    pub created_at: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Edge {
    pub id: EdgeId,
    pub source: NodeId,
    pub target: NodeId,
    pub edge_type: EdgeType,
    pub weight: f64,
    pub first_seen: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
    pub segment_id: SegmentId,
}

#[derive(Debug, Serialize)]
pub struct GraphStats {
    pub node_count: usize,
    pub edge_count: usize,
    pub nodes_by_type: HashMap<String, usize>,
    pub edges_by_type: HashMap<String, usize>,
}

pub struct GraphStore {
    pub nodes: HashMap<NodeId, Node>,
    key_index: HashMap<(EntityType, String), NodeId>,
    edge_dedup: HashMap<(NodeId, NodeId, EdgeType), EdgeId>,
    pub edges: HashMap<EdgeId, Edge>,
    pub outgoing: HashMap<NodeId, Vec<EdgeId>>,
    pub incoming: HashMap<NodeId, Vec<EdgeId>>,
    segment_edges: HashMap<SegmentId, Vec<EdgeId>>,
}

impl GraphStore {
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            key_index: HashMap::new(),
            edge_dedup: HashMap::new(),
            edges: HashMap::new(),
            outgoing: HashMap::new(),
            incoming: HashMap::new(),
            segment_edges: HashMap::new(),
        }
    }

    pub fn upsert_node(
        &mut self,
        entity_type: EntityType,
        key: &str,
        segment_id: &SegmentId,
    ) -> NodeId {
        let lookup = (entity_type, key.to_string());
        if let Some(&existing_id) = self.key_index.get(&lookup) {
            let node = self.nodes.get_mut(&existing_id).unwrap();
            node.last_seen = Utc::now();
            node.segment_refs.insert(segment_id.clone());
            return existing_id;
        }

        let id = Uuid::new_v4();
        let now = Utc::now();
        let mut segment_refs = HashSet::new();
        segment_refs.insert(segment_id.clone());

        let node = Node {
            id,
            entity_type,
            key: key.to_string(),
            segment_refs,
            created_at: now,
            last_seen: now,
        };

        self.nodes.insert(id, node);
        self.key_index.insert(lookup, id);
        id
    }

    pub fn add_edge(
        &mut self,
        source: NodeId,
        target: NodeId,
        edge_type: EdgeType,
        segment_id: &SegmentId,
    ) -> EdgeId {
        let dedup_key = (source, target, edge_type);
        if let Some(&existing_id) = self.edge_dedup.get(&dedup_key) {
            let edge = self.edges.get_mut(&existing_id).unwrap();
            edge.weight += 1.0;
            edge.last_seen = Utc::now();
            return existing_id;
        }

        let id = Uuid::new_v4();
        let now = Utc::now();

        let edge = Edge {
            id,
            source,
            target,
            edge_type,
            weight: 1.0,
            first_seen: now,
            last_seen: now,
            segment_id: segment_id.clone(),
        };

        self.edges.insert(id, edge);
        self.edge_dedup.insert(dedup_key, id);
        self.outgoing.entry(source).or_default().push(id);
        self.incoming.entry(target).or_default().push(id);
        self.segment_edges.entry(segment_id.clone()).or_default().push(id);
        id
    }

    pub fn stats(&self) -> GraphStats {
        let mut nodes_by_type: HashMap<String, usize> = HashMap::new();
        for node in self.nodes.values() {
            *nodes_by_type.entry(node.entity_type.to_string()).or_default() += 1;
        }

        let mut edges_by_type: HashMap<String, usize> = HashMap::new();
        for edge in self.edges.values() {
            *edges_by_type.entry(edge.edge_type.to_string()).or_default() += 1;
        }

        GraphStats {
            node_count: self.nodes.len(),
            edge_count: self.edges.len(),
            nodes_by_type,
            edges_by_type,
        }
    }

    /// Get neighbors of a node (outgoing edges).
    pub fn neighbors(&self, node_id: &NodeId) -> Vec<(&Edge, &Node)> {
        let mut result = Vec::new();
        if let Some(edge_ids) = self.outgoing.get(node_id) {
            for eid in edge_ids {
                if let Some(edge) = self.edges.get(eid) {
                    if let Some(target) = self.nodes.get(&edge.target) {
                        result.push((edge, target));
                    }
                }
            }
        }
        if let Some(edge_ids) = self.incoming.get(node_id) {
            for eid in edge_ids {
                if let Some(edge) = self.edges.get(eid) {
                    if let Some(source) = self.nodes.get(&edge.source) {
                        result.push((edge, source));
                    }
                }
            }
        }
        result
    }
}
