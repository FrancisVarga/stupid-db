//! Pipeline message payloads for PUSH/PULL work distribution.
//!
//! These types represent data flowing through processing pipelines:
//! ingest → compute → graph. They are self-contained DTOs that don't
//! depend on core domain types, keeping the messaging layer decoupled.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// A single record flowing through the ingest pipeline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Record {
    /// Source-assigned record identifier.
    pub id: String,
    /// Key-value fields extracted from the source data.
    pub fields: HashMap<String, serde_json::Value>,
}

/// Batch of records pushed into the ingest pipeline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IngestBatch {
    /// Records to be ingested.
    pub records: Vec<Record>,
}

/// A single computed feature value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Feature {
    /// Feature name (e.g. "login_frequency", "tx_amount_mean").
    pub name: String,
    /// Entity this feature belongs to.
    pub entity_id: String,
    /// Computed feature value.
    pub value: f64,
}

/// Results flowing out of the compute pipeline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComputeResult {
    /// Computed features for this batch.
    pub features: Vec<Feature>,
}

/// An entity update for the graph store.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entity {
    /// Entity identifier.
    pub id: String,
    /// Entity type (e.g. "user", "device", "ip").
    pub entity_type: String,
    /// Key-value properties for the entity.
    pub properties: HashMap<String, String>,
}

/// An edge update for the graph store.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Edge {
    /// Source entity identifier.
    pub source_id: String,
    /// Target entity identifier.
    pub target_id: String,
    /// Edge type (e.g. "logged_in_from", "transferred_to").
    pub edge_type: String,
    /// Edge weight (default 1.0).
    pub weight: f64,
}

/// Entity and edge updates flowing into the graph store.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphUpdate {
    /// Entity upserts.
    pub entities: Vec<Entity>,
    /// Edge upserts.
    pub edges: Vec<Edge>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip<T>(val: &T) -> T
    where
        T: Serialize + for<'de> Deserialize<'de> + std::fmt::Debug + PartialEq,
    {
        let bytes = rmp_serde::to_vec(val).expect("serialize");
        rmp_serde::from_slice(&bytes).expect("deserialize")
    }

    #[test]
    fn roundtrip_ingest_batch() {
        let batch = IngestBatch {
            records: vec![
                Record {
                    id: "r-1".into(),
                    fields: HashMap::from([
                        ("user".into(), serde_json::Value::String("alice".into())),
                        ("amount".into(), serde_json::json!(42.0)),
                    ]),
                },
                Record {
                    id: "r-2".into(),
                    fields: HashMap::new(),
                },
            ],
        };
        assert_eq!(roundtrip(&batch), batch);
    }

    #[test]
    fn roundtrip_compute_result() {
        let result = ComputeResult {
            features: vec![
                Feature {
                    name: "login_freq".into(),
                    entity_id: "user-1".into(),
                    value: 3.14,
                },
            ],
        };
        assert_eq!(roundtrip(&result), result);
    }

    #[test]
    fn roundtrip_graph_update() {
        let update = GraphUpdate {
            entities: vec![Entity {
                id: "user-1".into(),
                entity_type: "user".into(),
                properties: HashMap::from([("name".into(), "Alice".into())]),
            }],
            edges: vec![Edge {
                source_id: "user-1".into(),
                target_id: "device-1".into(),
                edge_type: "logged_in_from".into(),
                weight: 1.0,
            }],
        };
        assert_eq!(roundtrip(&update), update);
    }

    #[test]
    fn empty_graph_update_roundtrips() {
        let update = GraphUpdate {
            entities: vec![],
            edges: vec![],
        };
        assert_eq!(roundtrip(&update), update);
    }
}
