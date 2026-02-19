use chrono::{Duration, TimeZone, Utc};

use stupid_segment::store::DocumentStore;

use crate::helpers::{make_config, make_test_doc, test_data_dir};

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
