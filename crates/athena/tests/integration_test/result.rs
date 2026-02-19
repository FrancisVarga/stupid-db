//! Tests for AthenaQueryResult: parsing, display, empty results, and mixed types.

use stupid_athena::*;
use stupid_core::FieldValue;

#[test]
fn test_result_parsing() {
    let result = AthenaQueryResult {
        columns: vec![
            AthenaColumn {
                name: "id".to_string(),
                data_type: "bigint".to_string(),
            },
            AthenaColumn {
                name: "name".to_string(),
                data_type: "varchar".to_string(),
            },
            AthenaColumn {
                name: "score".to_string(),
                data_type: "double".to_string(),
            },
        ],
        rows: vec![
            vec![Some("123".to_string()), Some("Alice".to_string()), Some("9.5".to_string())],
            vec![Some("456".to_string()), Some("Bob".to_string()), None],
            vec![Some("789".to_string()), None, Some("7.2".to_string())],
        ],
        metadata: QueryMetadata {
            query_id: "test-123".to_string(),
            bytes_scanned: 1_073_741_824, // 1 GB
            execution_time_ms: 1500,
            state: "SUCCEEDED".to_string(),
            output_location: Some("s3://bucket/results/".to_string()),
        },
    };

    // Test accessors
    assert_eq!(result.row_count(), 3);
    assert_eq!(result.column_count(), 3);
    assert!(!result.is_empty());

    // Test column_index
    assert_eq!(result.column_index("id"), Some(0));
    assert_eq!(result.column_index("name"), Some(1));
    assert_eq!(result.column_index("score"), Some(2));
    assert_eq!(result.column_index("missing"), None);

    // Test get_value
    assert_eq!(result.get_value(0, "id"), Some("123"));
    assert_eq!(result.get_value(0, "name"), Some("Alice"));
    assert_eq!(result.get_value(0, "score"), Some("9.5"));

    // Test NULL handling
    assert_eq!(result.get_value(1, "score"), None);
    assert_eq!(result.get_value(2, "name"), None);

    // Test out-of-bounds
    assert_eq!(result.get_value(99, "id"), None);
    assert_eq!(result.get_value(0, "nonexistent"), None);

    // Test cost estimate (1 GB -> $5/1024)
    let expected_cost = 5.0 / 1024.0;
    assert!((result.cost_estimate_usd() - expected_cost).abs() < 1e-9);
}

#[test]
fn test_result_display() {
    let result = AthenaQueryResult {
        columns: vec![
            AthenaColumn {
                name: "id".to_string(),
                data_type: "bigint".to_string(),
            },
            AthenaColumn {
                name: "value".to_string(),
                data_type: "varchar".to_string(),
            },
        ],
        rows: vec![
            vec![Some("1".to_string()), Some("alpha".to_string())],
            vec![Some("2".to_string()), None],
        ],
        metadata: QueryMetadata {
            query_id: "disp-123".to_string(),
            bytes_scanned: 500_000_000,
            execution_time_ms: 750,
            state: "SUCCEEDED".to_string(),
            output_location: None,
        },
    };

    let output = result.to_string();

    // Verify headers present
    assert!(output.contains("id"));
    assert!(output.contains("value"));

    // Verify data present
    assert!(output.contains("alpha"));
    assert!(output.contains("NULL")); // NULL displayed for None

    // Verify metadata
    assert!(output.contains("disp-123"));
    assert!(output.contains("2 rows"));
    assert!(output.contains("750ms"));
    assert!(output.contains("$")); // Cost displayed
}

#[test]
fn test_empty_result_set() {
    let result = AthenaQueryResult {
        columns: vec![],
        rows: vec![],
        metadata: QueryMetadata {
            query_id: "empty".to_string(),
            bytes_scanned: 0,
            execution_time_ms: 10,
            state: "SUCCEEDED".to_string(),
            output_location: None,
        },
    };

    assert_eq!(result.row_count(), 0);
    assert_eq!(result.column_count(), 0);
    assert!(result.is_empty());
    assert!((result.cost_estimate_usd()).abs() < f64::EPSILON);

    let docs = result_to_documents(&result, "empty_query", None);
    assert!(docs.is_empty());
}

#[test]
fn test_mixed_type_parsing() {
    let result = AthenaQueryResult {
        columns: vec![
            AthenaColumn {
                name: "int_col".to_string(),
                data_type: "integer".to_string(),
            },
            AthenaColumn {
                name: "bigint_col".to_string(),
                data_type: "bigint".to_string(),
            },
            AthenaColumn {
                name: "float_col".to_string(),
                data_type: "float".to_string(),
            },
            AthenaColumn {
                name: "decimal_col".to_string(),
                data_type: "decimal".to_string(),
            },
            AthenaColumn {
                name: "bool_col".to_string(),
                data_type: "boolean".to_string(),
            },
            AthenaColumn {
                name: "varchar_col".to_string(),
                data_type: "varchar".to_string(),
            },
        ],
        rows: vec![vec![
            Some("42".to_string()),
            Some("9223372036854775807".to_string()),
            Some("3.14159".to_string()),
            Some("99.99".to_string()),
            Some("true".to_string()),
            Some("hello world".to_string()),
        ]],
        metadata: QueryMetadata {
            query_id: "type-test".to_string(),
            bytes_scanned: 512,
            execution_time_ms: 20,
            state: "SUCCEEDED".to_string(),
            output_location: None,
        },
    };

    let docs = result_to_documents(&result, "type_test", None);
    assert_eq!(docs.len(), 1);

    let fields = &docs[0].fields;
    assert_eq!(fields.get("int_col"), Some(&FieldValue::Integer(42)));
    assert_eq!(
        fields.get("bigint_col"),
        Some(&FieldValue::Integer(9223372036854775807))
    );
    assert_eq!(
        fields.get("float_col"),
        Some(&FieldValue::Float(3.14159))
    );
    assert_eq!(fields.get("decimal_col"), Some(&FieldValue::Float(99.99)));
    assert_eq!(fields.get("bool_col"), Some(&FieldValue::Boolean(true)));
    assert_eq!(
        fields.get("varchar_col"),
        Some(&FieldValue::Text("hello world".to_string()))
    );
}
