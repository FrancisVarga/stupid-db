use chrono::{Duration, TimeZone, Utc};

use stupid_core::FieldValue;
use stupid_segment::filter::ScanFilter;
use stupid_segment::store::DocumentStore;

use crate::helpers::{make_config, make_doc_with_fields, test_data_dir};

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
