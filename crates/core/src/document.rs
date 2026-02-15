use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Unique document identifier within a segment.
pub type DocId = Uuid;

/// Segment identifier (date string like "2025-06-14").
pub type SegmentId = String;

/// A document is a flat key-value map with a mandatory timestamp and event type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    pub id: DocId,
    pub timestamp: DateTime<Utc>,
    pub event_type: String,
    pub fields: HashMap<String, FieldValue>,
}

/// Typed field values — all source data arrives as strings but we preserve type info.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum FieldValue {
    Text(String),
    Integer(i64),
    Float(f64),
    Boolean(bool),
    Null,
}

impl FieldValue {
    /// Extract as string, returning None for Null.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            FieldValue::Text(s) => Some(s.as_str()),
            _ => None,
        }
    }
}

/// Address of a document in storage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocAddress {
    pub segment_id: SegmentId,
    pub offset: u64,
}
