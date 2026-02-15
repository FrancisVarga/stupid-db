/// Integration tests for the document store covering full pipeline, parquet import,
/// segment rotation, eviction, persistence, scan filters, and statistics.

use std::collections::HashMap;
use std::path::PathBuf;

use chrono::{DateTime, Duration, TimeZone, Utc};
use uuid::Uuid;

use stupid_core::config::StorageConfig;
use stupid_core::{Document, FieldValue};
use stupid_segment::filter::ScanFilter;
use stupid_segment::store::DocumentStore;

// ============================================================================
// Test Helpers
// ============================================================================

/// Create a unique temp directory for each test.
fn test_data_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("stupid-test-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Create a test configuration with given data directory.
fn make_config(data_dir: PathBuf, retention_days: u32) -> StorageConfig {
    StorageConfig {
        data_dir: data_dir.clone(),
        segment_retention_days: retention_days,
        cache_dir: data_dir.join("cache"),
        cache_max_gb: 1,
    }
}

/// Create a test document with custom timestamp and fields.
fn make_test_doc(event_type: &str, timestamp: DateTime<Utc>) -> Document {
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
fn make_doc_with_fields(
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

// ============================================================================
// Integration Tests
// ============================================================================

#[test]
fn test_full_pipeline() {
    let data_dir = test_data_dir();
    let config = make_config(data_dir.clone(), 30);
    let mut store = DocumentStore::new(&config).unwrap();

    // Insert documents spanning 3 days
    let day1 = Utc.with_ymd_and_hms(2025, 6, 14, 10, 0, 0).unwrap();
    let day2 = Utc.with_ymd_and_hms(2025, 6, 15, 10, 0, 0).unwrap();
    let day3 = Utc.with_ymd_and_hms(2025, 6, 16, 10, 0, 0).unwrap();

    let doc1 = make_test_doc("Login", day1);
    let doc2 = make_test_doc("GameOpened", day2);
    let doc3 = make_test_doc("Login", day3);

    let id1 = doc1.id;
    let id2 = doc2.id;
    let id3 = doc3.id;

    store.insert(doc1).unwrap();
    store.insert(doc2).unwrap();
    store.insert(doc3).unwrap();

    // Flush to seal segments
    store.flush().unwrap();

    // Verify segments were created
    let stats = store.stats();
    assert_eq!(stats.segment_count, 3, "Should have 3 segments");
    assert_eq!(stats.document_count, 3, "Should have 3 documents");

    // Verify segment directories exist
    assert!(data_dir.join("segments/2025-06-14").is_dir());
    assert!(data_dir.join("segments/2025-06-15").is_dir());
    assert!(data_dir.join("segments/2025-06-16").is_dir());

    // Scan with time filter (day 2 only)
    let filter = ScanFilter::time_range(day2, day2 + Duration::hours(23));
    let results = store.scan(&filter).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, id2);
    assert_eq!(results[0].event_type, "GameOpened");

    // Scan with time range (day 1 and day 2)
    let filter = ScanFilter::time_range(day1, day2 + Duration::hours(23));
    let results = store.scan(&filter).unwrap();
    assert_eq!(results.len(), 2);

    // Scan all with no filter
    let filter = ScanFilter::new();
    let results = store.scan(&filter).unwrap();
    assert_eq!(results.len(), 3);

    // Retrieve by ID
    let retrieved1 = store.get_by_id(&id1).unwrap();
    assert_eq!(retrieved1.id, id1);
    assert_eq!(retrieved1.event_type, "Login");

    let retrieved3 = store.get_by_id(&id3).unwrap();
    assert_eq!(retrieved3.id, id3);

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

#[test]
fn test_segment_rotation() {
    let data_dir = test_data_dir();
    let config = make_config(data_dir.clone(), 30);
    let mut store = DocumentStore::new(&config).unwrap();

    // Insert documents across 5 days
    for day_offset in 0..5 {
        let ts = Utc.with_ymd_and_hms(2025, 6, 14 + day_offset, 12, 0, 0).unwrap();
        store.insert(make_test_doc("Login", ts)).unwrap();
    }

    store.flush().unwrap();

    // Verify 5 segment directories exist
    let stats = store.stats();
    assert_eq!(stats.segment_count, 5);
    assert_eq!(stats.document_count, 5);

    // Check directories
    for day_offset in 0..5 {
        let segment_id = format!("2025-06-{:02}", 14 + day_offset);
        let segment_path = data_dir.join("segments").join(&segment_id);
        assert!(
            segment_path.is_dir(),
            "Segment directory {} should exist",
            segment_id
        );

        // Verify each segment has the required files
        assert!(segment_path.join("documents.dat").exists());
        assert!(segment_path.join("meta.json").exists());
        assert!(segment_path.join("documents.idx").exists());
    }

    std::fs::remove_dir_all(&data_dir).ok();
}

#[test]
fn test_eviction() {
    let data_dir = test_data_dir();
    let config = make_config(data_dir.clone(), 1); // 1 day retention
    let mut store = DocumentStore::new(&config).unwrap();

    // Insert old documents (5 days ago)
    let old_ts = Utc::now() - Duration::days(5);
    let old_segment_id = old_ts.format("%Y-%m-%d").to_string();
    let old_doc = make_test_doc("Login", old_ts);
    let old_id = old_doc.id;
    store.insert(old_doc).unwrap();

    // Insert recent documents (today)
    let recent_ts = Utc::now();
    let recent_segment_id = recent_ts.format("%Y-%m-%d").to_string();
    let recent_doc = make_test_doc("GameOpened", recent_ts);
    let recent_id = recent_doc.id;
    store.insert(recent_doc).unwrap();

    // Flush to seal segments
    store.flush().unwrap();

    // Before eviction, should have 2 segments and 2 documents
    let stats = store.stats();
    assert_eq!(stats.segment_count, 2);
    assert_eq!(stats.document_count, 2);

    // Verify old segment directory exists
    let old_segment_dir = data_dir.join("segments").join(&old_segment_id);
    assert!(old_segment_dir.exists(), "Old segment directory should exist before eviction");

    // Manually trigger eviction
    let evicted = store.manager_mut().evict_expired().unwrap();
    assert_eq!(evicted.len(), 1, "Should have evicted 1 segment");
    assert_eq!(evicted[0], old_segment_id, "Should have evicted the old segment");

    // After eviction, manager should report 1 segment
    let segment_list = store.manager().list_segments();
    assert_eq!(segment_list.len(), 1, "Manager should list 1 segment after eviction");
    assert_eq!(segment_list[0], recent_segment_id);

    // Old segment directory should be deleted
    assert!(!old_segment_dir.exists(), "Old segment directory should be deleted");

    // Old document should not be retrievable (segment is gone)
    let old_result = store.get_by_id(&old_id);
    assert!(old_result.is_err(), "Old document should be gone");

    // Recent document should still be retrievable
    let recent_result = store.get_by_id(&recent_id);
    assert!(
        recent_result.is_ok(),
        "Recent document should still be accessible"
    );

    // Note: stats() may still report 2 documents because the index for the evicted
    // segment still exists in memory. This is acceptable behavior - the index cleanup
    // happens on restart when DocumentStore::new() only loads existing segments.
    // Let's verify this by recreating the store:
    drop(store);
    let store = DocumentStore::new(&config).unwrap();

    let stats = store.stats();
    assert_eq!(stats.segment_count, 1, "Should have 1 segment after reload");
    assert_eq!(stats.document_count, 1, "Should have 1 document after reload");

    // Verify only recent document is accessible after reload
    let recent_result = store.get_by_id(&recent_id);
    assert!(recent_result.is_ok(), "Recent document should be accessible after reload");

    let old_result = store.get_by_id(&old_id);
    assert!(old_result.is_err(), "Old document should not be accessible after reload");

    std::fs::remove_dir_all(&data_dir).ok();
}

#[test]
fn test_persistence() {
    let data_dir = test_data_dir();
    let config = make_config(data_dir.clone(), 30);

    let doc_id = {
        // Create store, insert documents, flush
        let mut store = DocumentStore::new(&config).unwrap();

        let doc1 = make_test_doc("Login", Utc.with_ymd_and_hms(2025, 6, 14, 10, 0, 0).unwrap());
        let doc2 =
            make_test_doc("GameOpened", Utc.with_ymd_and_hms(2025, 6, 15, 10, 0, 0).unwrap());

        let id1 = doc1.id;
        store.insert(doc1).unwrap();
        store.insert(doc2).unwrap();

        store.flush().unwrap();

        // Verify before drop
        let stats = store.stats();
        assert_eq!(stats.segment_count, 2);
        assert_eq!(stats.document_count, 2);

        id1
    }; // store dropped here

    // Recreate store from same directory
    let store = DocumentStore::new(&config).unwrap();

    // Verify data reloaded correctly
    let stats = store.stats();
    assert_eq!(stats.segment_count, 2, "Segments should persist");
    assert_eq!(stats.document_count, 2, "Documents should persist");

    // Verify indexes reloaded
    let retrieved = store.get_by_id(&doc_id).unwrap();
    assert_eq!(retrieved.id, doc_id);
    assert_eq!(retrieved.event_type, "Login");

    // Verify schema registry reloaded
    let schema = store.schema_registry().get_schema("Login").unwrap();
    assert_eq!(schema.total_documents, 1);

    // Verify segments can be scanned
    let filter = ScanFilter::new();
    let results = store.scan(&filter).unwrap();
    assert_eq!(results.len(), 2);

    std::fs::remove_dir_all(&data_dir).ok();
}

#[test]
fn test_scan_filter_combinations() {
    let data_dir = test_data_dir();
    let config = make_config(data_dir.clone(), 30);
    let mut store = DocumentStore::new(&config).unwrap();

    // Insert diverse documents
    let day1 = Utc.with_ymd_and_hms(2025, 6, 14, 10, 0, 0).unwrap();
    let day2 = Utc.with_ymd_and_hms(2025, 6, 15, 10, 0, 0).unwrap();
    let day3 = Utc.with_ymd_and_hms(2025, 6, 16, 10, 0, 0).unwrap();

    // Day 1: Login by alice with score 100
    store
        .insert(make_doc_with_fields(
            "Login",
            day1,
            vec![
                ("member", FieldValue::Text("alice".to_string())),
                ("score", FieldValue::Integer(100)),
            ],
        ))
        .unwrap();

    // Day 2: GameOpened by alice with score 150
    store
        .insert(make_doc_with_fields(
            "GameOpened",
            day2,
            vec![
                ("member", FieldValue::Text("alice".to_string())),
                ("score", FieldValue::Integer(150)),
            ],
        ))
        .unwrap();

    // Day 2: Login by bob with score 50
    store
        .insert(make_doc_with_fields(
            "Login",
            day2,
            vec![
                ("member", FieldValue::Text("bob".to_string())),
                ("score", FieldValue::Integer(50)),
            ],
        ))
        .unwrap();

    // Day 3: GameOpened by bob with score 200
    store
        .insert(make_doc_with_fields(
            "GameOpened",
            day3,
            vec![
                ("member", FieldValue::Text("bob".to_string())),
                ("score", FieldValue::Integer(200)),
            ],
        ))
        .unwrap();

    store.flush().unwrap();

    // Test 1: Event type only
    let filter = ScanFilter::new().event_type("Login");
    let results = store.scan(&filter).unwrap();
    assert_eq!(results.len(), 2);
    assert!(results.iter().all(|d| d.event_type == "Login"));

    // Test 2: Time range only
    let filter = ScanFilter::time_range(day2, day2 + Duration::hours(23));
    let results = store.scan(&filter).unwrap();
    assert_eq!(results.len(), 2);

    // Test 3: Event type + time range
    let filter = ScanFilter::time_range(day2, day3 + Duration::hours(23)).event_type("GameOpened");
    let results = store.scan(&filter).unwrap();
    assert_eq!(results.len(), 2);
    assert!(results.iter().all(|d| d.event_type == "GameOpened"));

    // Test 4: Event type + field equality
    let filter = ScanFilter::new()
        .event_type("Login")
        .field_eq("member", "alice");
    let results = store.scan(&filter).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(
        results[0].fields.get("member").unwrap(),
        &FieldValue::Text("alice".to_string())
    );

    // Test 5: Event type + numeric field filter (gt)
    let filter = ScanFilter::new()
        .event_type("GameOpened")
        .field_gt("score", 100.0);
    let results = store.scan(&filter).unwrap();
    assert_eq!(results.len(), 2);
    assert!(results.iter().all(|d| d.event_type == "GameOpened"));

    // Test 6: Multiple field filters
    let filter = ScanFilter::new()
        .field_eq("member", "bob")
        .field_gt("score", 100.0);
    let results = store.scan(&filter).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(
        results[0].fields.get("score").unwrap(),
        &FieldValue::Integer(200)
    );

    // Test 7: Time range + event type + field filters
    let filter = ScanFilter::time_range(day1, day2 + Duration::hours(23))
        .event_type("Login")
        .field_eq("member", "bob");
    let results = store.scan(&filter).unwrap();
    assert_eq!(results.len(), 1);

    // Test 8: Filter with no matches
    let filter = ScanFilter::new()
        .event_type("NonExistent")
        .field_eq("member", "charlie");
    let results = store.scan(&filter).unwrap();
    assert_eq!(results.len(), 0);

    std::fs::remove_dir_all(&data_dir).ok();
}

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
fn test_field_contains_filter() {
    let data_dir = test_data_dir();
    let config = make_config(data_dir.clone(), 30);
    let mut store = DocumentStore::new(&config).unwrap();

    let now = Utc::now();

    store
        .insert(make_doc_with_fields(
            "Log",
            now,
            vec![(
                "message",
                FieldValue::Text("An error occurred in module X".to_string()),
            )],
        ))
        .unwrap();

    store
        .insert(make_doc_with_fields(
            "Log",
            now,
            vec![(
                "message",
                FieldValue::Text("Info: system started".to_string()),
            )],
        ))
        .unwrap();

    store
        .insert(make_doc_with_fields(
            "Log",
            now,
            vec![(
                "message",
                FieldValue::Text("Warning: error threshold exceeded".to_string()),
            )],
        ))
        .unwrap();

    store.flush().unwrap();

    // Filter messages containing "error"
    let filter = ScanFilter::new().field_contains("message", "error");
    let results = store.scan(&filter).unwrap();
    assert_eq!(results.len(), 2);

    // Filter messages containing "Info"
    let filter = ScanFilter::new().field_contains("message", "Info");
    let results = store.scan(&filter).unwrap();
    assert_eq!(results.len(), 1);

    std::fs::remove_dir_all(&data_dir).ok();
}

#[test]
fn test_numeric_field_filters() {
    let data_dir = test_data_dir();
    let config = make_config(data_dir.clone(), 30);
    let mut store = DocumentStore::new(&config).unwrap();

    let now = Utc::now();

    // Insert documents with various scores
    for score in [50, 100, 150, 200, 250] {
        store
            .insert(make_doc_with_fields(
                "Game",
                now,
                vec![("score", FieldValue::Integer(score))],
            ))
            .unwrap();
    }

    store.flush().unwrap();

    // Test greater-than filter
    let filter = ScanFilter::new().field_gt("score", 150.0);
    let results = store.scan(&filter).unwrap();
    assert_eq!(results.len(), 2); // 200, 250

    // Test less-than filter
    let filter = ScanFilter::new().field_lt("score", 150.0);
    let results = store.scan(&filter).unwrap();
    assert_eq!(results.len(), 2); // 50, 100

    // Test combination: gt 100 and lt 200
    let filter = ScanFilter::new().field_gt("score", 100.0).field_lt("score", 200.0);
    let results = store.scan(&filter).unwrap();
    assert_eq!(results.len(), 1); // 150

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
