use chrono::{Duration, TimeZone, Utc};

use stupid_segment::filter::ScanFilter;
use stupid_segment::store::DocumentStore;

use crate::helpers::{make_config, make_test_doc, test_data_dir};

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
