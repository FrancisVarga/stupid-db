#[cfg(test)]
mod tests {
    use chrono::Utc;

    use crate::convert::documents::result_to_documents;
    use crate::result::{AthenaColumn, AthenaQueryResult, QueryMetadata};
    use stupid_core::FieldValue;

    /// Helper to create a minimal metadata for testing.
    fn test_metadata() -> QueryMetadata {
        QueryMetadata {
            query_id: "test-query-id".to_string(),
            bytes_scanned: 1024,
            execution_time_ms: 100,
            state: "SUCCEEDED".to_string(),
            output_location: None,
        }
    }

    #[test]
    fn test_type_conversion_varchar() {
        let result = AthenaQueryResult {
            columns: vec![AthenaColumn {
                name: "name".to_string(),
                data_type: "varchar".to_string(),
            }],
            rows: vec![vec![Some("alice".to_string())]],
            metadata: test_metadata(),
        };

        let docs = result_to_documents(&result, "test_event", None);
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].event_type, "test_event");
        assert_eq!(
            docs[0].fields.get("name"),
            Some(&FieldValue::Text("alice".to_string()))
        );
    }

    #[test]
    fn test_type_conversion_integer() {
        let result = AthenaQueryResult {
            columns: vec![AthenaColumn {
                name: "count".to_string(),
                data_type: "bigint".to_string(),
            }],
            rows: vec![vec![Some("42".to_string())]],
            metadata: test_metadata(),
        };

        let docs = result_to_documents(&result, "test_event", None);
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].fields.get("count"), Some(&FieldValue::Integer(42)));
    }

    #[test]
    fn test_type_conversion_float() {
        let result = AthenaQueryResult {
            columns: vec![AthenaColumn {
                name: "score".to_string(),
                data_type: "double".to_string(),
            }],
            rows: vec![vec![Some("3.14159".to_string())]],
            metadata: test_metadata(),
        };

        let docs = result_to_documents(&result, "test_event", None);
        assert_eq!(docs.len(), 1);
        assert_eq!(
            docs[0].fields.get("score"),
            Some(&FieldValue::Float(3.14159))
        );
    }

    #[test]
    fn test_type_conversion_boolean() {
        let result = AthenaQueryResult {
            columns: vec![
                AthenaColumn {
                    name: "flag1".to_string(),
                    data_type: "boolean".to_string(),
                },
                AthenaColumn {
                    name: "flag2".to_string(),
                    data_type: "boolean".to_string(),
                },
                AthenaColumn {
                    name: "flag3".to_string(),
                    data_type: "boolean".to_string(),
                },
                AthenaColumn {
                    name: "flag4".to_string(),
                    data_type: "boolean".to_string(),
                },
            ],
            rows: vec![vec![
                Some("true".to_string()),
                Some("false".to_string()),
                Some("1".to_string()),
                Some("0".to_string()),
            ]],
            metadata: test_metadata(),
        };

        let docs = result_to_documents(&result, "test_event", None);
        assert_eq!(docs.len(), 1);
        assert_eq!(
            docs[0].fields.get("flag1"),
            Some(&FieldValue::Boolean(true))
        );
        assert_eq!(
            docs[0].fields.get("flag2"),
            Some(&FieldValue::Boolean(false))
        );
        assert_eq!(
            docs[0].fields.get("flag3"),
            Some(&FieldValue::Boolean(true))
        );
        assert_eq!(
            docs[0].fields.get("flag4"),
            Some(&FieldValue::Boolean(false))
        );
    }

    #[test]
    fn test_null_values_skipped() {
        let result = AthenaQueryResult {
            columns: vec![
                AthenaColumn {
                    name: "col1".to_string(),
                    data_type: "varchar".to_string(),
                },
                AthenaColumn {
                    name: "col2".to_string(),
                    data_type: "varchar".to_string(),
                },
                AthenaColumn {
                    name: "col3".to_string(),
                    data_type: "varchar".to_string(),
                },
            ],
            rows: vec![vec![Some("a".to_string()), None, Some("c".to_string())]],
            metadata: test_metadata(),
        };

        let docs = result_to_documents(&result, "test_event", None);
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].fields.len(), 2); // Only col1 and col3.
        assert!(docs[0].fields.contains_key("col1"));
        assert!(!docs[0].fields.contains_key("col2")); // NULL skipped.
        assert!(docs[0].fields.contains_key("col3"));
    }

    #[test]
    fn test_invalid_numeric_fallback_to_text() {
        let result = AthenaQueryResult {
            columns: vec![
                AthenaColumn {
                    name: "bad_int".to_string(),
                    data_type: "bigint".to_string(),
                },
                AthenaColumn {
                    name: "bad_float".to_string(),
                    data_type: "double".to_string(),
                },
            ],
            rows: vec![vec![
                Some("not-a-number".to_string()),
                Some("xyz".to_string()),
            ]],
            metadata: test_metadata(),
        };

        let docs = result_to_documents(&result, "test_event", None);
        assert_eq!(docs.len(), 1);
        assert_eq!(
            docs[0].fields.get("bad_int"),
            Some(&FieldValue::Text("not-a-number".to_string()))
        );
        assert_eq!(
            docs[0].fields.get("bad_float"),
            Some(&FieldValue::Text("xyz".to_string()))
        );
    }

    #[test]
    fn test_invalid_boolean_fallback_to_text() {
        let result = AthenaQueryResult {
            columns: vec![AthenaColumn {
                name: "flag".to_string(),
                data_type: "boolean".to_string(),
            }],
            rows: vec![vec![Some("maybe".to_string())]],
            metadata: test_metadata(),
        };

        let docs = result_to_documents(&result, "test_event", None);
        assert_eq!(docs.len(), 1);
        assert_eq!(
            docs[0].fields.get("flag"),
            Some(&FieldValue::Text("maybe".to_string()))
        );
    }

    #[test]
    fn test_empty_result_returns_empty_vec() {
        let result = AthenaQueryResult {
            columns: vec![],
            rows: vec![],
            metadata: test_metadata(),
        };

        let docs = result_to_documents(&result, "test_event", None);
        assert_eq!(docs.len(), 0);
    }

    #[test]
    fn test_timestamp_column_missing() {
        let result = AthenaQueryResult {
            columns: vec![AthenaColumn {
                name: "value".to_string(),
                data_type: "varchar".to_string(),
            }],
            rows: vec![vec![Some("test".to_string())]],
            metadata: test_metadata(),
        };

        let before = Utc::now();
        let docs = result_to_documents(&result, "test_event", None);
        let after = Utc::now();

        assert_eq!(docs.len(), 1);
        // Timestamp should be close to current time.
        assert!(docs[0].timestamp >= before && docs[0].timestamp <= after);
    }

    #[test]
    fn test_case_insensitive_type_matching() {
        let result = AthenaQueryResult {
            columns: vec![
                AthenaColumn {
                    name: "col1".to_string(),
                    data_type: "VARCHAR".to_string(), // uppercase
                },
                AthenaColumn {
                    name: "col2".to_string(),
                    data_type: "BIGINT".to_string(), // uppercase
                },
                AthenaColumn {
                    name: "col3".to_string(),
                    data_type: "Boolean".to_string(), // mixed case
                },
            ],
            rows: vec![vec![
                Some("text".to_string()),
                Some("999".to_string()),
                Some("true".to_string()),
            ]],
            metadata: test_metadata(),
        };

        let docs = result_to_documents(&result, "test_event", None);
        assert_eq!(docs.len(), 1);
        assert_eq!(
            docs[0].fields.get("col1"),
            Some(&FieldValue::Text("text".to_string()))
        );
        assert_eq!(
            docs[0].fields.get("col2"),
            Some(&FieldValue::Integer(999))
        );
        assert_eq!(
            docs[0].fields.get("col3"),
            Some(&FieldValue::Boolean(true))
        );
    }
}
