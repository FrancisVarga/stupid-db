use std::collections::HashMap;

use chrono::Utc;
use uuid::Uuid;

use crate::result::AthenaQueryResult;
use stupid_core::Document;

use super::parsing::{parse_field_value, parse_timestamp};

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
/// - `varchar`, `char`, `string` -> `Text`
/// - `bigint`, `int`, `integer`, `smallint`, `tinyint` -> `Integer` (with fallback to `Text`)
/// - `double`, `float`, `decimal`, `real` -> `Float` (with fallback to `Text`)
/// - `boolean` -> `Boolean` (with fallback to `Text`)
/// - `timestamp`, `date` -> `Text` (raw storage; timestamp column handled separately)
/// - Unknown types -> `Text`
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
