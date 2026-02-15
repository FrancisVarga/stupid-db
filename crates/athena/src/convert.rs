use std::collections::HashMap;

use chrono::{DateTime, NaiveDate, NaiveDateTime, Utc};
use uuid::Uuid;

use crate::result::AthenaQueryResult;
use stupid_core::{Document, FieldValue};

/// Convert an Athena query result into a vector of Documents.
///
/// # Arguments
///
/// * `result` - The Athena query result to convert
/// * `event_type` - The event type to assign to all generated documents
/// * `timestamp_column` - Optional column name to use for document timestamps.
///   If None or if parsing fails, defaults to `Utc::now()`
///
/// # Type Mapping
///
/// Athena types are mapped to `FieldValue` as follows:
/// - `varchar`, `char`, `string` → `Text`
/// - `bigint`, `int`, `integer`, `smallint`, `tinyint` → `Integer` (with fallback to `Text`)
/// - `double`, `float`, `decimal`, `real` → `Float` (with fallback to `Text`)
/// - `boolean` → `Boolean` (with fallback to `Text`)
/// - `timestamp`, `date` → `Text` (raw storage; timestamp column handled separately)
/// - Unknown types → `Text`
///
/// NULL values (represented as `None` in Athena rows) are skipped and not added to fields.
pub fn result_to_documents(
    result: &AthenaQueryResult,
    event_type: &str,
    timestamp_column: Option<&str>,
) -> Vec<Document> {
    let mut documents = Vec::with_capacity(result.rows.len());

    // Find the timestamp column index if specified.
    let ts_col_idx = timestamp_column.and_then(|name| result.column_index(name));

    for row in &result.rows {
        // Extract timestamp from the specified column or use current time.
        let timestamp = if let Some(idx) = ts_col_idx {
            row.get(idx)
                .and_then(|opt_val| opt_val.as_deref())
                .and_then(parse_timestamp)
                .unwrap_or_else(Utc::now)
        } else {
            Utc::now()
        };

        // Build field map from all columns.
        let mut fields = HashMap::new();
        for (i, col) in result.columns.iter().enumerate() {
            if let Some(Some(value)) = row.get(i) {
                // Skip NULL values (None).
                let field_value = parse_field_value(value, &col.data_type);
                fields.insert(col.name.clone(), field_value);
            }
        }

        documents.push(Document {
            id: Uuid::new_v4(),
            timestamp,
            event_type: event_type.to_string(),
            fields,
        });
    }

    documents
}

/// Parse a string value into a `FieldValue` based on Athena data type.
///
/// Numeric and boolean types attempt parsing and fall back to `Text` if parsing fails.
fn parse_field_value(value: &str, data_type: &str) -> FieldValue {
    let normalized_type = data_type.to_lowercase();

    match normalized_type.as_str() {
        // Integer types.
        "bigint" | "int" | "integer" | "smallint" | "tinyint" => value
            .parse::<i64>()
            .map(FieldValue::Integer)
            .unwrap_or_else(|_| FieldValue::Text(value.to_string())),
        // Floating-point types.
        "double" | "float" | "decimal" | "real" => value
            .parse::<f64>()
            .map(FieldValue::Float)
            .unwrap_or_else(|_| FieldValue::Text(value.to_string())),
        // Boolean type.
        "boolean" => {
            let lower = value.to_lowercase();
            match lower.as_str() {
                "true" | "1" => FieldValue::Boolean(true),
                "false" | "0" => FieldValue::Boolean(false),
                _ => FieldValue::Text(value.to_string()),
            }
        }
        // String types.
        "varchar" | "char" | "string" | "timestamp" | "date" => {
            FieldValue::Text(value.to_string())
        }
        // Unknown types default to text.
        _ => FieldValue::Text(value.to_string()),
    }
}

/// Parse a timestamp string into a `DateTime<Utc>`.
///
/// Tries multiple common formats in order:
/// 1. RFC3339: `"2025-06-14T10:30:00Z"`
/// 2. Space-separated: `"2025-06-14 10:30:00"`
/// 3. Just date: `"2025-06-14"` (assumes midnight UTC)
///
/// Returns `None` if all formats fail.
fn parse_timestamp(value: &str) -> Option<DateTime<Utc>> {
    // Try RFC3339 format first.
    if let Ok(dt) = DateTime::parse_from_rfc3339(value) {
        return Some(dt.with_timezone(&Utc));
    }

    // Try space-separated datetime format.
    if let Ok(ndt) = NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S") {
        return Some(ndt.and_utc());
    }

    // Try just date format (assume midnight UTC).
    if let Ok(nd) = NaiveDate::parse_from_str(value, "%Y-%m-%d") {
        return Some(nd.and_hms_opt(0, 0, 0)?.and_utc());
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::result::{AthenaColumn, QueryMetadata};

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
