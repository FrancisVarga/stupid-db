use super::*;
use chrono::{TimeZone, Utc};
use std::fs;
use uuid::Uuid;

use stupid_core::FieldValue;

fn temp_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("stupid-store-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn make_config(data_dir: PathBuf) -> StorageConfig {
    StorageConfig {
        data_dir: data_dir.clone(),
        segment_retention_days: 30,
        cache_dir: data_dir.join("cache"),
        cache_max_gb: 1,
    }
}

fn make_doc(event_type: &str, timestamp: chrono::DateTime<Utc>) -> Document {
    Document {
        id: Uuid::new_v4(),
        timestamp,
        event_type: event_type.to_string(),
        fields: {
            let mut m = std::collections::HashMap::new();
            m.insert("test".to_string(), FieldValue::Text("value".to_string()));
            m
        },
    }
}

#[test]
fn test_new_creates_store() {
    let dir = temp_dir();
    let config = make_config(dir.clone());
    let _store = DocumentStore::new(&config).unwrap();
    assert!(dir.join("segments").is_dir());
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_insert_and_get() {
    let dir = temp_dir();
    let config = make_config(dir.clone());
    let mut store = DocumentStore::new(&config).unwrap();

    let doc = make_doc("Login", Utc::now());
    let doc_id = doc.id;
    let addr = store.insert(doc.clone()).unwrap();

    // Flush to seal the segment so we can read from it
    store.flush().unwrap();

    let retrieved = store.get(&addr).unwrap();
    assert_eq!(retrieved.id, doc_id);
    assert_eq!(retrieved.event_type, "Login");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_insert_and_get_by_id() {
    let dir = temp_dir();
    let config = make_config(dir.clone());
    let mut store = DocumentStore::new(&config).unwrap();

    let doc = make_doc("GameOpened", Utc::now());
    let doc_id = doc.id;
    store.insert(doc.clone()).unwrap();

    // Flush to seal the segment so we can read from it
    store.flush().unwrap();

    let retrieved = store.get_by_id(&doc_id).unwrap();
    assert_eq!(retrieved.id, doc_id);
    assert_eq!(retrieved.event_type, "GameOpened");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_get_by_id_not_found() {
    let dir = temp_dir();
    let config = make_config(dir.clone());
    let store = DocumentStore::new(&config).unwrap();

    let missing_id = Uuid::new_v4();
    let result = store.get_by_id(&missing_id);
    assert!(result.is_err());

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_scan_empty_filter() {
    let dir = temp_dir();
    let config = make_config(dir.clone());
    let mut store = DocumentStore::new(&config).unwrap();

    store.insert(make_doc("Login", Utc::now())).unwrap();
    store.insert(make_doc("GameOpened", Utc::now())).unwrap();
    store.flush().unwrap();

    let filter = ScanFilter::new();
    let results = store.scan(&filter).unwrap();
    assert_eq!(results.len(), 2);

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_scan_by_event_type() {
    let dir = temp_dir();
    let config = make_config(dir.clone());
    let mut store = DocumentStore::new(&config).unwrap();

    store.insert(make_doc("Login", Utc::now())).unwrap();
    store.insert(make_doc("Login", Utc::now())).unwrap();
    store.insert(make_doc("GameOpened", Utc::now())).unwrap();
    store.flush().unwrap();

    let filter = ScanFilter::new().event_type("Login");
    let results = store.scan(&filter).unwrap();
    assert_eq!(results.len(), 2);

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_scan_by_time_range() {
    let dir = temp_dir();
    let config = make_config(dir.clone());
    let mut store = DocumentStore::new(&config).unwrap();

    let t1 = Utc.with_ymd_and_hms(2025, 6, 14, 10, 0, 0).unwrap();
    let t2 = Utc.with_ymd_and_hms(2025, 6, 14, 12, 0, 0).unwrap();
    let t3 = Utc.with_ymd_and_hms(2025, 6, 14, 14, 0, 0).unwrap();

    store.insert(make_doc("Login", t1)).unwrap();
    store.insert(make_doc("Login", t2)).unwrap();
    store.insert(make_doc("Login", t3)).unwrap();
    store.flush().unwrap();

    let start = Utc.with_ymd_and_hms(2025, 6, 14, 11, 0, 0).unwrap();
    let end = Utc.with_ymd_and_hms(2025, 6, 14, 13, 0, 0).unwrap();
    let filter = ScanFilter::time_range(start, end);
    let results = store.scan(&filter).unwrap();

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].timestamp, t2);

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_scan_by_field() {
    let dir = temp_dir();
    let config = make_config(dir.clone());
    let mut store = DocumentStore::new(&config).unwrap();

    let mut doc1 = make_doc("Login", Utc::now());
    doc1.fields
        .insert("member".to_string(), FieldValue::Text("alice".to_string()));

    let mut doc2 = make_doc("Login", Utc::now());
    doc2.fields
        .insert("member".to_string(), FieldValue::Text("bob".to_string()));

    store.insert(doc1).unwrap();
    store.insert(doc2).unwrap();
    store.flush().unwrap();

    let filter = ScanFilter::new().field_eq("member", "alice");
    let results = store.scan(&filter).unwrap();

    assert_eq!(results.len(), 1);
    assert_eq!(
        results[0].fields.get("member").unwrap(),
        &FieldValue::Text("alice".to_string())
    );

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_flush_and_reload() {
    let dir = temp_dir();
    let config = make_config(dir.clone());

    let doc_id = {
        let mut store = DocumentStore::new(&config).unwrap();
        let doc = make_doc("Login", Utc::now());
        let doc_id = doc.id;
        store.insert(doc).unwrap();
        store.flush().unwrap();
        doc_id
    };

    // Reload the store
    let store = DocumentStore::new(&config).unwrap();
    let retrieved = store.get_by_id(&doc_id).unwrap();
    assert_eq!(retrieved.id, doc_id);

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_stats() {
    let dir = temp_dir();
    let config = make_config(dir.clone());
    let mut store = DocumentStore::new(&config).unwrap();

    store.insert(make_doc("Login", Utc::now())).unwrap();
    store.insert(make_doc("GameOpened", Utc::now())).unwrap();
    store.flush().unwrap();

    let stats = store.stats();
    assert_eq!(stats.document_count, 2);
    assert!(stats.segment_count > 0);
    assert!(stats.total_bytes > 0);

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_schema_registry_tracks_docs() {
    let dir = temp_dir();
    let config = make_config(dir.clone());
    let mut store = DocumentStore::new(&config).unwrap();

    let mut doc = make_doc("Login", Utc::now());
    doc.fields
        .insert("member".to_string(), FieldValue::Text("alice".to_string()));
    store.insert(doc).unwrap();

    let schema = store.schema_registry().get_schema("Login").unwrap();
    assert_eq!(schema.total_documents, 1);
    assert!(schema.fields.contains_key("member"));

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_multiple_segments() {
    let dir = temp_dir();
    let config = make_config(dir.clone());
    let mut store = DocumentStore::new(&config).unwrap();

    // Insert docs on different days
    let day1 = Utc.with_ymd_and_hms(2025, 6, 14, 12, 0, 0).unwrap();
    let day2 = Utc.with_ymd_and_hms(2025, 6, 15, 12, 0, 0).unwrap();

    store.insert(make_doc("Login", day1)).unwrap();
    store.insert(make_doc("Login", day2)).unwrap();
    store.flush().unwrap();

    let stats = store.stats();
    assert_eq!(stats.segment_count, 2);
    assert_eq!(stats.document_count, 2);

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_insert_updates_index() {
    let dir = temp_dir();
    let config = make_config(dir.clone());
    let mut store = DocumentStore::new(&config).unwrap();

    let doc = make_doc("Login", Utc::now());
    let doc_id = doc.id;
    let segment_id = SegmentManager::segment_id_for_timestamp(&doc.timestamp);

    store.insert(doc).unwrap();

    // Check that the index was updated
    let index = store.indexes.get(&segment_id).unwrap();
    assert!(index.get(&doc_id).is_some());

    fs::remove_dir_all(&dir).ok();
}
