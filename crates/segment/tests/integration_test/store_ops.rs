use std::path::PathBuf;

use chrono::{TimeZone, Utc};

use stupid_core::FieldValue;
use stupid_segment::filter::ScanFilter;
use stupid_segment::store::DocumentStore;

use crate::helpers::{make_config, make_doc_with_fields, make_test_doc, test_data_dir};

#[test]
fn test_stats() {
    let data_dir = test_data_dir();
    let config = make_config(data_dir.clone(), 30);
    let mut store = DocumentStore::new(&config).unwrap();

    // Initial stats: empty
    let stats = store.stats();
    assert_eq!(stats.segment_count, 0);
    assert_eq!(stats.document_count, 0);
    assert_eq!(stats.total_bytes, 0);

    // Insert documents across 3 segments
    let day1 = Utc.with_ymd_and_hms(2025, 6, 14, 10, 0, 0).unwrap();
    let day2 = Utc.with_ymd_and_hms(2025, 6, 15, 10, 0, 0).unwrap();
    let day3 = Utc.with_ymd_and_hms(2025, 6, 16, 10, 0, 0).unwrap();

    store.insert(make_test_doc("Login", day1)).unwrap();
    store.insert(make_test_doc("Login", day1)).unwrap();
    store.insert(make_test_doc("GameOpened", day2)).unwrap();
    store.insert(make_test_doc("Login", day3)).unwrap();

    store.flush().unwrap();

    // After flush
    let stats = store.stats();
    assert_eq!(stats.segment_count, 3, "Should have 3 segments");
    assert_eq!(stats.document_count, 4, "Should have 4 documents");
    assert!(stats.total_bytes > 0, "Should have non-zero total bytes");

    // Verify segment distribution
    let segment_ids = store.manager().list_segments();
    assert_eq!(segment_ids.len(), 3);
    assert!(segment_ids.contains(&"2025-06-14".to_string()));
    assert!(segment_ids.contains(&"2025-06-15".to_string()));
    assert!(segment_ids.contains(&"2025-06-16".to_string()));

    std::fs::remove_dir_all(&data_dir).ok();
}

#[test]
fn test_concurrent_segment_writes() {
    let data_dir = test_data_dir();
    let config = make_config(data_dir.clone(), 30);
    let mut store = DocumentStore::new(&config).unwrap();

    // Insert multiple documents to same segment rapidly
    let day = Utc.with_ymd_and_hms(2025, 6, 14, 10, 0, 0).unwrap();

    let mut doc_ids = Vec::new();
    for _ in 0..100 {
        let doc = make_test_doc("Login", day);
        doc_ids.push(doc.id);
        store.insert(doc).unwrap();
    }

    store.flush().unwrap();

    // Verify all documents are retrievable
    for id in &doc_ids {
        let retrieved = store.get_by_id(id).unwrap();
        assert_eq!(retrieved.id, *id);
    }

    let stats = store.stats();
    assert_eq!(stats.segment_count, 1);
    assert_eq!(stats.document_count, 100);

    std::fs::remove_dir_all(&data_dir).ok();
}

#[test]
fn test_empty_scan() {
    let data_dir = test_data_dir();
    let config = make_config(data_dir.clone(), 30);
    let store = DocumentStore::new(&config).unwrap();

    // Scan empty store
    let filter = ScanFilter::new();
    let results = store.scan(&filter).unwrap();
    assert_eq!(results.len(), 0);

    // Scan with specific filter
    let filter = ScanFilter::new().event_type("Login");
    let results = store.scan(&filter).unwrap();
    assert_eq!(results.len(), 0);

    std::fs::remove_dir_all(&data_dir).ok();
}

#[test]
fn test_schema_tracking() {
    let data_dir = test_data_dir();
    let config = make_config(data_dir.clone(), 30);
    let mut store = DocumentStore::new(&config).unwrap();

    let now = Utc::now();

    // Insert documents with various fields
    store
        .insert(make_doc_with_fields(
            "Login",
            now,
            vec![
                ("memberCode", FieldValue::Text("alice".to_string())),
                ("fingerprint", FieldValue::Text("fp123".to_string())),
            ],
        ))
        .unwrap();

    store
        .insert(make_doc_with_fields(
            "Login",
            now,
            vec![
                ("memberCode", FieldValue::Text("bob".to_string())),
                ("fingerprint", FieldValue::Text("fp456".to_string())),
                ("sessionId", FieldValue::Text("sess789".to_string())),
            ],
        ))
        .unwrap();

    store.flush().unwrap();

    // Check schema registry
    let schema = store.schema_registry().get_schema("Login").unwrap();
    assert_eq!(schema.total_documents, 2);
    assert!(schema.fields.contains_key("memberCode"));
    assert!(schema.fields.contains_key("fingerprint"));
    assert!(schema.fields.contains_key("sessionId"));

    // Check field stats
    let member_stats = schema.fields.get("memberCode").unwrap();
    assert_eq!(member_stats.seen_count, 2);

    let session_stats = schema.fields.get("sessionId").unwrap();
    assert_eq!(session_stats.seen_count, 1);

    std::fs::remove_dir_all(&data_dir).ok();
}

#[test]
#[ignore] // Run with: cargo test --test integration_test test_parquet_import -- --ignored
fn test_parquet_import() {
    // This test requires D:\w88_data to exist. Skip if not available.
    let parquet_path = PathBuf::from(r"D:\w88_data\Login\2025-06-14.parquet");
    if !parquet_path.exists() {
        println!("Skipping parquet test: {} not found", parquet_path.display());
        return;
    }

    let data_dir = test_data_dir();
    let config = make_config(data_dir.clone(), 30);
    let mut store = DocumentStore::new(&config).unwrap();

    // Import the parquet file
    let count = store.import_parquet(&parquet_path, "Login").unwrap();
    assert!(count > 0, "Should import at least one document");

    // Check schema registry
    let schema = store.schema_registry().get_schema("Login").unwrap();
    assert_eq!(schema.total_documents, count as u64);
    assert!(schema.fields.len() > 0, "Should have tracked some fields");

    // Flush to persist
    store.flush().unwrap();

    // Verify we can scan the imported data
    let filter = ScanFilter::new().event_type("Login");
    let results = store.scan(&filter).unwrap();
    assert_eq!(results.len(), count);

    // Verify documents can be retrieved by ID
    if let Some(first_doc) = results.first() {
        let retrieved = store.get_by_id(&first_doc.id).unwrap();
        assert_eq!(retrieved.id, first_doc.id);
        assert_eq!(retrieved.event_type, "Login");
    }

    // Verify field filters work on imported data
    // Most Login events should have a memberCode field
    if let Some(sample_doc) = results.iter().find(|d| d.fields.contains_key("memberCode")) {
        let member_code = sample_doc.fields.get("memberCode").unwrap();
        if let FieldValue::Text(mc) = member_code {
            let filter = ScanFilter::new()
                .event_type("Login")
                .field_eq("memberCode", mc.clone());
            let filtered = store.scan(&filter).unwrap();
            assert!(filtered.len() > 0);
            assert!(filtered.iter().all(|d| d.event_type == "Login"));
        }
    }

    std::fs::remove_dir_all(&data_dir).ok();
}
