use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use stupid_core::document::{Document, FieldValue};
use stupid_core::error::StupidError;
use tracing::debug;

/// Statistics for a single field across all documents of a given event type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldStats {
    /// The type name of the field ("text", "integer", "float", "boolean", "null").
    pub field_type: String,
    /// Number of documents where this field was present and non-null.
    pub seen_count: u64,
    /// Number of documents where this field was absent or null.
    pub null_count: u64,
}

impl FieldStats {
    /// Returns the fraction of observations that were null.
    ///
    /// Returns 0.0 when both `seen_count` and `null_count` are zero.
    pub fn null_rate(&self) -> f64 {
        let total = self.seen_count + self.null_count;
        if total == 0 {
            0.0
        } else {
            self.null_count as f64 / total as f64
        }
    }
}

/// Accumulated schema for a single event type, built from observed documents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventSchema {
    /// Per-field statistics keyed by field name.
    pub fields: HashMap<String, FieldStats>,
    /// Total number of documents observed for this event type.
    pub total_documents: u64,
}

/// Registry that tracks per-event-type schemas by observing documents.
///
/// This is a statistics tracker, not a validation engine. It records which
/// fields appear, their types, and how often they are null or absent.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SchemaRegistry {
    /// Event type name to its accumulated schema.
    pub schemas: HashMap<String, EventSchema>,
}

/// Map a `FieldValue` variant to its type name string.
fn field_type_name(value: &FieldValue) -> &'static str {
    match value {
        FieldValue::Text(_) => "text",
        FieldValue::Integer(_) => "integer",
        FieldValue::Float(_) => "float",
        FieldValue::Boolean(_) => "boolean",
        FieldValue::Null => "null",
    }
}

impl SchemaRegistry {
    /// Create an empty schema registry.
    pub fn new() -> SchemaRegistry {
        SchemaRegistry {
            schemas: HashMap::new(),
        }
    }

    /// Observe a document and update the schema for its event type.
    ///
    /// For each field present in the document, the corresponding `FieldStats`
    /// entry is created or updated. Fields previously known for this event type
    /// but absent from this document get their `null_count` incremented.
    pub fn observe(&mut self, doc: &Document) {
        let schema = self
            .schemas
            .entry(doc.event_type.clone())
            .or_insert_with(|| EventSchema {
                fields: HashMap::new(),
                total_documents: 0,
            });

        schema.total_documents += 1;

        // Track which previously-known fields are present in this document.
        let known_fields: Vec<String> = schema.fields.keys().cloned().collect();

        // Update stats for each field in the document.
        for (name, value) in &doc.fields {
            let type_name = field_type_name(value);
            let stats = schema.fields.entry(name.clone()).or_insert_with(|| {
                // This field was not previously known. Documents before this one
                // did not contain it, so backfill null_count.
                FieldStats {
                    field_type: type_name.to_string(),
                    seen_count: 0,
                    null_count: schema.total_documents - 1,
                }
            });

            if matches!(value, FieldValue::Null) {
                stats.null_count += 1;
            } else {
                stats.field_type = type_name.to_string();
                stats.seen_count += 1;
            }
        }

        // Increment null_count for known fields absent from this document.
        for field_name in &known_fields {
            if !doc.fields.contains_key(field_name) {
                if let Some(stats) = schema.fields.get_mut(field_name) {
                    stats.null_count += 1;
                }
            }
        }
    }

    /// Look up the schema for a given event type.
    pub fn get_schema(&self, event_type: &str) -> Option<&EventSchema> {
        self.schemas.get(event_type)
    }

    /// Return all known event types, sorted alphabetically.
    pub fn event_types(&self) -> Vec<&str> {
        let mut types: Vec<&str> = self.schemas.keys().map(|s| s.as_str()).collect();
        types.sort_unstable();
        types
    }

    /// Persist the registry to a JSON file.
    pub fn save(&self, path: &Path) -> Result<(), StupidError> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| StupidError::Serialize(e.to_string()))?;
        std::fs::write(path, json)?;
        debug!("schema registry saved to {}", path.display());
        Ok(())
    }

    /// Load a registry from a JSON file. Returns an empty registry if the file
    /// does not exist.
    pub fn load(path: &Path) -> Result<SchemaRegistry, StupidError> {
        if !path.exists() {
            debug!(
                "schema file {} does not exist, returning empty registry",
                path.display()
            );
            return Ok(SchemaRegistry::new());
        }
        let data = std::fs::read_to_string(path)?;
        let registry: SchemaRegistry =
            serde_json::from_str(&data).map_err(|e| StupidError::Serialize(e.to_string()))?;
        debug!("schema registry loaded from {}", path.display());
        Ok(registry)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;

    fn make_doc(event_type: &str, fields: Vec<(&str, FieldValue)>) -> Document {
        Document {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            event_type: event_type.to_string(),
            fields: fields
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect(),
        }
    }

    #[test]
    fn test_field_stats_null_rate_both_zero() {
        let stats = FieldStats {
            field_type: "text".into(),
            seen_count: 0,
            null_count: 0,
        };
        assert_eq!(stats.null_rate(), 0.0);
    }

    #[test]
    fn test_field_stats_null_rate() {
        let stats = FieldStats {
            field_type: "text".into(),
            seen_count: 3,
            null_count: 1,
        };
        assert!((stats.null_rate() - 0.25).abs() < f64::EPSILON);
    }

    #[test]
    fn test_observe_single_doc() {
        let mut registry = SchemaRegistry::new();
        let doc = make_doc(
            "login",
            vec![
                ("member", FieldValue::Text("alice".into())),
                ("score", FieldValue::Integer(42)),
            ],
        );
        registry.observe(&doc);

        let schema = registry.get_schema("login").unwrap();
        assert_eq!(schema.total_documents, 1);
        assert_eq!(schema.fields["member"].seen_count, 1);
        assert_eq!(schema.fields["member"].null_count, 0);
        assert_eq!(schema.fields["member"].field_type, "text");
        assert_eq!(schema.fields["score"].field_type, "integer");
    }

    #[test]
    fn test_observe_tracks_absent_fields_as_null() {
        let mut registry = SchemaRegistry::new();

        // First doc has fields A and B.
        let doc1 = make_doc(
            "evt",
            vec![
                ("a", FieldValue::Text("x".into())),
                ("b", FieldValue::Integer(1)),
            ],
        );
        registry.observe(&doc1);

        // Second doc has only field A — field B should get null_count incremented.
        let doc2 = make_doc("evt", vec![("a", FieldValue::Text("y".into()))]);
        registry.observe(&doc2);

        let schema = registry.get_schema("evt").unwrap();
        assert_eq!(schema.total_documents, 2);
        assert_eq!(schema.fields["a"].seen_count, 2);
        assert_eq!(schema.fields["a"].null_count, 0);
        assert_eq!(schema.fields["b"].seen_count, 1);
        assert_eq!(schema.fields["b"].null_count, 1);
    }

    #[test]
    fn test_observe_new_field_backfills_null() {
        let mut registry = SchemaRegistry::new();

        // First doc has only field A.
        let doc1 = make_doc("evt", vec![("a", FieldValue::Boolean(true))]);
        registry.observe(&doc1);

        // Second doc introduces field B — it was absent in doc1 so null_count
        // should be backfilled to 1.
        let doc2 = make_doc("evt", vec![("b", FieldValue::Float(3.14))]);
        registry.observe(&doc2);

        let schema = registry.get_schema("evt").unwrap();
        assert_eq!(schema.fields["b"].seen_count, 1);
        assert_eq!(schema.fields["b"].null_count, 1);
        assert_eq!(schema.fields["b"].field_type, "float");
    }

    #[test]
    fn test_observe_null_value() {
        let mut registry = SchemaRegistry::new();
        let doc = make_doc("evt", vec![("x", FieldValue::Null)]);
        registry.observe(&doc);

        let stats = &registry.get_schema("evt").unwrap().fields["x"];
        assert_eq!(stats.seen_count, 0);
        assert_eq!(stats.null_count, 1);
        assert_eq!(stats.field_type, "null");
    }

    #[test]
    fn test_event_types_sorted() {
        let mut registry = SchemaRegistry::new();
        registry.observe(&make_doc("zebra", vec![]));
        registry.observe(&make_doc("alpha", vec![]));
        registry.observe(&make_doc("middle", vec![]));

        assert_eq!(registry.event_types(), vec!["alpha", "middle", "zebra"]);
    }

    #[test]
    fn test_save_and_load_roundtrip() {
        let mut registry = SchemaRegistry::new();
        registry.observe(&make_doc(
            "login",
            vec![("user", FieldValue::Text("bob".into()))],
        ));

        let dir = std::env::temp_dir().join("stupid_db_schema_test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("schema.json");

        registry.save(&path).unwrap();
        let loaded = SchemaRegistry::load(&path).unwrap();

        assert_eq!(loaded.event_types(), vec!["login"]);
        let schema = loaded.get_schema("login").unwrap();
        assert_eq!(schema.total_documents, 1);
        assert_eq!(schema.fields["user"].seen_count, 1);

        // Cleanup
        std::fs::remove_file(&path).ok();
        std::fs::remove_dir(&dir).ok();
    }

    #[test]
    fn test_load_missing_file_returns_empty() {
        let path = std::env::temp_dir().join("nonexistent_schema_xyz.json");
        let registry = SchemaRegistry::load(&path).unwrap();
        assert!(registry.schemas.is_empty());
    }

    #[test]
    fn test_multiple_event_types_independent() {
        let mut registry = SchemaRegistry::new();
        registry.observe(&make_doc(
            "login",
            vec![("user", FieldValue::Text("a".into()))],
        ));
        registry.observe(&make_doc(
            "game",
            vec![("score", FieldValue::Integer(100))],
        ));

        assert_eq!(registry.get_schema("login").unwrap().total_documents, 1);
        assert_eq!(registry.get_schema("game").unwrap().total_documents, 1);
        assert!(registry.get_schema("login").unwrap().fields.contains_key("user"));
        assert!(!registry.get_schema("login").unwrap().fields.contains_key("score"));
    }
}
