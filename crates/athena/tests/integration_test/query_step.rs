//! Tests for AthenaQueryStep: deserialization, optional params, and full construction.

use stupid_athena::*;

#[test]
fn test_query_step_construction() {
    let json = r#"{
        "id": "event_query",
        "params": {
            "sql": "SELECT * FROM events WHERE date = '2025-01-15'",
            "max_scan_gb": 5.0
        }
    }"#;

    let step: AthenaQueryStep = serde_json::from_str(json).expect("deserialize");

    assert_eq!(step.id, "event_query");
    assert_eq!(
        step.params.sql,
        "SELECT * FROM events WHERE date = '2025-01-15'"
    );
    assert_eq!(step.params.max_scan_gb, Some(5.0));
}

#[test]
fn test_query_step_optional_params() {
    let json = r#"{
        "id": "simple_query",
        "params": {
            "sql": "SELECT COUNT(*) FROM users"
        }
    }"#;

    let step: AthenaQueryStep = serde_json::from_str(json).expect("deserialize");

    assert_eq!(step.id, "simple_query");
    assert_eq!(step.params.sql, "SELECT COUNT(*) FROM users");
    assert_eq!(step.params.max_scan_gb, None);
    assert_eq!(step.params.event_type, None);
    assert_eq!(step.params.timestamp_column, None);
}

#[test]
fn test_query_step_with_all_params() {
    let json = r#"{
        "id": "error_analysis",
        "params": {
            "sql": "SELECT timestamp, message FROM errors",
            "max_scan_gb": 2.5,
            "event_type": "ErrorLog",
            "timestamp_column": "timestamp"
        }
    }"#;

    let step: AthenaQueryStep = serde_json::from_str(json).expect("deserialize");

    assert_eq!(step.id, "error_analysis");
    assert_eq!(step.params.sql, "SELECT timestamp, message FROM errors");
    assert_eq!(step.params.max_scan_gb, Some(2.5));
    assert_eq!(step.params.event_type.as_deref(), Some("ErrorLog"));
    assert_eq!(
        step.params.timestamp_column.as_deref(),
        Some("timestamp")
    );
}
