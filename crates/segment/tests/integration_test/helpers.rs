use std::collections::HashMap;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use uuid::Uuid;

use stupid_core::config::StorageConfig;
use stupid_core::{Document, FieldValue};

/// Create a unique temp directory for each test.
pub fn test_data_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("stupid-test-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Create a test configuration with given data directory.
pub fn make_config(data_dir: PathBuf, retention_days: u32) -> StorageConfig {
    StorageConfig {
        data_dir: data_dir.clone(),
        segment_retention_days: retention_days,
        cache_dir: data_dir.join("cache"),
        cache_max_gb: 1,
    }
}

/// Create a test document with custom timestamp and fields.
pub fn make_test_doc(event_type: &str, timestamp: DateTime<Utc>) -> Document {
    let mut fields = HashMap::new();
    fields.insert(
        "test_field".to_string(),
        FieldValue::Text("test_value".to_string()),
    );
    fields.insert(
        "member".to_string(),
        FieldValue::Text("test_user".to_string()),
    );

    Document {
        id: Uuid::new_v4(),
        timestamp,
        event_type: event_type.to_string(),
        fields,
    }
}

/// Create a document with custom fields.
pub fn make_doc_with_fields(
    event_type: &str,
    timestamp: DateTime<Utc>,
    fields: Vec<(&str, FieldValue)>,
) -> Document {
    Document {
        id: Uuid::new_v4(),
        timestamp,
        event_type: event_type.to_string(),
        fields: fields
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect(),
    }
}
