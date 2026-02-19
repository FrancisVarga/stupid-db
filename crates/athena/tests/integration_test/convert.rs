//! Tests for result_to_documents conversion: field mapping, timestamps, and NULL handling.

use chrono::Utc;
use stupid_athena::*;
use stupid_core::FieldValue;

#[test]
fn test_convert_to_documents() {
    let result = AthenaQueryResult {
        columns: vec![
            AthenaColumn {
                name: "member_id".to_string(),
                data_type: "bigint".to_string(),
            },
            AthenaColumn {
                name: "username".to_string(),
                data_type: "varchar".to_string(),
            },
            AthenaColumn {
                name: "balance".to_string(),
                data_type: "double".to_string(),
            },
            AthenaColumn {
                name: "is_vip".to_string(),
                data_type: "boolean".to_string(),
            },
            AthenaColumn {
                name: "created_at".to_string(),
                data_type: "timestamp".to_string(),
            },
        ],
        rows: vec![
            vec![
                Some("1001".to_string()),
                Some("alice".to_string()),
                Some("250.75".to_string()),
                Some("true".to_string()),
                Some("2025-01-15T10:30:00Z".to_string()),
            ],
            vec![
                Some("1002".to_string()),
                Some("bob".to_string()),
                None, // NULL balance
                Some("false".to_string()),
                Some("2025-01-16T14:20:00Z".to_string()),
            ],
        ],
        metadata: QueryMetadata {
            query_id: "conv-123".to_string(),
            bytes_scanned: 1024,
            execution_time_ms: 100,
            state: "SUCCEEDED".to_string(),
            output_location: None,
        },
    };

    let docs = result_to_documents(&result, "member_query", Some("created_at"));

    // Verify correct number of documents
    assert_eq!(docs.len(), 2);

    // Verify event type
    assert_eq!(docs[0].event_type, "member_query");
    assert_eq!(docs[1].event_type, "member_query");

    // Verify field types mapped correctly
    let doc1_fields = &docs[0].fields;
    assert_eq!(
        doc1_fields.get("member_id"),
        Some(&FieldValue::Integer(1001))
    );
    assert_eq!(
        doc1_fields.get("username"),
        Some(&FieldValue::Text("alice".to_string()))
    );
    assert_eq!(
        doc1_fields.get("balance"),
        Some(&FieldValue::Float(250.75))
    );
    assert_eq!(doc1_fields.get("is_vip"), Some(&FieldValue::Boolean(true)));

    // Verify NULL values are skipped (not included in fields map)
    let doc2_fields = &docs[1].fields;
    assert_eq!(doc2_fields.get("balance"), None);

    // Verify timestamps parsed from created_at column
    assert_eq!(
        docs[0].timestamp.to_rfc3339(),
        "2025-01-15T10:30:00+00:00"
    );
    assert_eq!(
        docs[1].timestamp.to_rfc3339(),
        "2025-01-16T14:20:00+00:00"
    );

    // created_at should still be in fields (not removed like we initially thought)
    assert!(doc1_fields.contains_key("created_at"));
}

#[test]
fn test_convert_missing_timestamp() {
    let result = AthenaQueryResult {
        columns: vec![AthenaColumn {
            name: "col1".to_string(),
            data_type: "varchar".to_string(),
        }],
        rows: vec![vec![Some("value1".to_string())]],
        metadata: QueryMetadata {
            query_id: "ts-test".to_string(),
            bytes_scanned: 100,
            execution_time_ms: 50,
            state: "SUCCEEDED".to_string(),
            output_location: None,
        },
    };

    let docs = result_to_documents(&result, "no_timestamp", None);

    assert_eq!(docs.len(), 1);

    // Verify timestamp is recent (within 5 seconds of now)
    let now = Utc::now();
    let diff = (now - docs[0].timestamp).num_seconds().abs();
    assert!(
        diff < 5,
        "Timestamp should be recent (within 5s), got diff: {}s",
        diff
    );
}
