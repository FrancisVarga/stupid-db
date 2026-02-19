use chrono::{DateTime, NaiveDate, NaiveDateTime, Utc};

use stupid_core::FieldValue;

/// Parse a string value into a `FieldValue` based on Athena data type.
///
/// Numeric and boolean types attempt parsing and fall back to `Text` if parsing fails.
pub(crate) fn parse_field_value(value: &str, data_type: &str) -> FieldValue {
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
pub(crate) fn parse_timestamp(value: &str) -> Option<DateTime<Utc>> {
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
