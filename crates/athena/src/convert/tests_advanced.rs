#[cfg(test)]
mod tests {
    use chrono::{DateTime, NaiveDate, NaiveDateTime, Utc};

    use crate::convert::documents::result_to_documents;
    use crate::convert::parsing::parse_timestamp;
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
    fn test_timestamp_column_present() {
        let result = AthenaQueryResult {
            columns: vec![
                AthenaColumn {
                    name: "ts".to_string(),
                    data_type: "timestamp".to_string(),
                },
                AthenaColumn {
                    name: "value".to_string(),
                    data_type: "varchar".to_string(),
                },
            ],
            rows: vec![vec![
                Some("2025-06-14T10:30:00Z".to_string()),
                Some("test".to_string()),
            ]],
            metadata: test_metadata(),
        };

        let docs = result_to_documents(&result, "test_event", Some("ts"));
        assert_eq!(docs.len(), 1);

        // Verify timestamp was parsed correctly.
        let expected = DateTime::parse_from_rfc3339("2025-06-14T10:30:00Z")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(docs[0].timestamp, expected);
    }

    #[test]
    fn test_multiple_rows() {
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
            ],
            rows: vec![
                vec![Some("1".to_string()), Some("alice".to_string())],
                vec![Some("2".to_string()), Some("bob".to_string())],
                vec![Some("3".to_string()), Some("charlie".to_string())],
            ],
            metadata: test_metadata(),
        };

        let docs = result_to_documents(&result, "user_event", None);
        assert_eq!(docs.len(), 3);

        // Verify each document.
        assert_eq!(docs[0].event_type, "user_event");
        assert_eq!(docs[0].fields.get("id"), Some(&FieldValue::Integer(1)));
        assert_eq!(
            docs[0].fields.get("name"),
            Some(&FieldValue::Text("alice".to_string()))
        );

        assert_eq!(docs[1].event_type, "user_event");
        assert_eq!(docs[1].fields.get("id"), Some(&FieldValue::Integer(2)));
        assert_eq!(
            docs[1].fields.get("name"),
            Some(&FieldValue::Text("bob".to_string()))
        );

        assert_eq!(docs[2].event_type, "user_event");
        assert_eq!(docs[2].fields.get("id"), Some(&FieldValue::Integer(3)));
        assert_eq!(
            docs[2].fields.get("name"),
            Some(&FieldValue::Text("charlie".to_string()))
        );

        // Verify unique IDs.
        assert_ne!(docs[0].id, docs[1].id);
        assert_ne!(docs[1].id, docs[2].id);
        assert_ne!(docs[0].id, docs[2].id);
    }

    #[test]
    fn test_parse_timestamp_formats() {
        // RFC3339.
        let ts1 = parse_timestamp("2025-06-14T10:30:00Z");
        assert!(ts1.is_some());
        assert_eq!(
            ts1.unwrap(),
            DateTime::parse_from_rfc3339("2025-06-14T10:30:00Z")
                .unwrap()
                .with_timezone(&Utc)
        );

        // Space-separated datetime.
        let ts2 = parse_timestamp("2025-06-14 10:30:00");
        assert!(ts2.is_some());
        let expected2 =
            NaiveDateTime::parse_from_str("2025-06-14 10:30:00", "%Y-%m-%d %H:%M:%S")
                .unwrap()
                .and_utc();
        assert_eq!(ts2.unwrap(), expected2);

        // Just date.
        let ts3 = parse_timestamp("2025-06-14");
        assert!(ts3.is_some());
        let expected3 = NaiveDate::from_ymd_opt(2025, 6, 14)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc();
        assert_eq!(ts3.unwrap(), expected3);

        // Invalid format.
        let ts4 = parse_timestamp("not-a-date");
        assert!(ts4.is_none());
    }

    #[test]
    fn test_all_data_types_in_one_row() {
        let result = AthenaQueryResult {
            columns: vec![
                AthenaColumn {
                    name: "text_col".to_string(),
                    data_type: "varchar".to_string(),
                },
                AthenaColumn {
                    name: "int_col".to_string(),
                    data_type: "bigint".to_string(),
                },
                AthenaColumn {
                    name: "float_col".to_string(),
                    data_type: "double".to_string(),
                },
                AthenaColumn {
                    name: "bool_col".to_string(),
                    data_type: "boolean".to_string(),
                },
                AthenaColumn {
                    name: "null_col".to_string(),
                    data_type: "varchar".to_string(),
                },
                AthenaColumn {
                    name: "timestamp_col".to_string(),
                    data_type: "timestamp".to_string(),
                },
            ],
            rows: vec![vec![
                Some("hello".to_string()),
                Some("123".to_string()),
                Some("45.67".to_string()),
                Some("true".to_string()),
                None,
                Some("2025-06-14T10:30:00Z".to_string()),
            ]],
            metadata: test_metadata(),
        };

        let docs = result_to_documents(&result, "mixed_event", Some("timestamp_col"));
        assert_eq!(docs.len(), 1);

        let doc = &docs[0];
        assert_eq!(doc.event_type, "mixed_event");
        assert_eq!(
            doc.fields.get("text_col"),
            Some(&FieldValue::Text("hello".to_string()))
        );
        assert_eq!(doc.fields.get("int_col"), Some(&FieldValue::Integer(123)));
        assert_eq!(
            doc.fields.get("float_col"),
            Some(&FieldValue::Float(45.67))
        );
        assert_eq!(
            doc.fields.get("bool_col"),
            Some(&FieldValue::Boolean(true))
        );
        assert!(!doc.fields.contains_key("null_col")); // NULL skipped.
        assert_eq!(
            doc.fields.get("timestamp_col"),
            Some(&FieldValue::Text("2025-06-14T10:30:00Z".to_string()))
        );

        // Verify timestamp was extracted correctly.
        let expected_ts = DateTime::parse_from_rfc3339("2025-06-14T10:30:00Z")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(doc.timestamp, expected_ts);
    }
}
