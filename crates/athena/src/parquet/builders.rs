//! Build typed Arrow arrays from string-based Athena result rows.

use std::sync::Arc;

use arrow::array::{
    ArrayRef, BooleanBuilder, Float64Builder, Int64Builder, StringBuilder,
    TimestampMillisecondBuilder,
};
use arrow::datatypes::{DataType, Schema, TimeUnit};

use crate::result::AthenaColumn;
use super::error::ParquetError;

/// Build typed Arrow arrays from the string-based Athena rows.
///
/// For each column we inspect the target Arrow type and parse the string
/// values into the correct native type. Unparseable values become NULL in
/// the output (safe fallback -- no data loss, just type downgrade).
pub(crate) fn build_arrays(
    columns: &[AthenaColumn],
    rows: &[Vec<Option<String>>],
    schema: &Schema,
) -> Result<Vec<ArrayRef>, ParquetError> {
    let num_rows = rows.len();
    let mut arrays: Vec<ArrayRef> = Vec::with_capacity(columns.len());

    for (col_idx, field) in schema.fields().iter().enumerate() {
        let array: ArrayRef = match field.data_type() {
            DataType::Int64 => {
                let mut builder = Int64Builder::with_capacity(num_rows);
                for row in rows {
                    match row.get(col_idx).and_then(|v| v.as_deref()) {
                        Some(s) => match s.parse::<i64>() {
                            Ok(v) => builder.append_value(v),
                            Err(_) => builder.append_null(),
                        },
                        None => builder.append_null(),
                    }
                }
                Arc::new(builder.finish())
            }
            DataType::Float64 => {
                let mut builder = Float64Builder::with_capacity(num_rows);
                for row in rows {
                    match row.get(col_idx).and_then(|v| v.as_deref()) {
                        Some(s) => match s.parse::<f64>() {
                            Ok(v) => builder.append_value(v),
                            Err(_) => builder.append_null(),
                        },
                        None => builder.append_null(),
                    }
                }
                Arc::new(builder.finish())
            }
            DataType::Boolean => {
                let mut builder = BooleanBuilder::with_capacity(num_rows);
                for row in rows {
                    match row.get(col_idx).and_then(|v| v.as_deref()) {
                        Some(s) => {
                            let lower = s.to_lowercase();
                            match lower.as_str() {
                                "true" | "1" => builder.append_value(true),
                                "false" | "0" => builder.append_value(false),
                                _ => builder.append_null(),
                            }
                        }
                        None => builder.append_null(),
                    }
                }
                Arc::new(builder.finish())
            }
            DataType::Timestamp(TimeUnit::Millisecond, _) => {
                let mut builder = TimestampMillisecondBuilder::with_capacity(num_rows);
                for row in rows {
                    match row.get(col_idx).and_then(|v| v.as_deref()) {
                        Some(s) => match parse_timestamp_ms(s) {
                            Some(ms) => builder.append_value(ms),
                            None => builder.append_null(),
                        },
                        None => builder.append_null(),
                    }
                }
                Arc::new(
                    builder
                        .finish()
                        .with_timezone("UTC"),
                )
            }
            // Default: UTF-8 string
            _ => {
                let mut builder = StringBuilder::with_capacity(num_rows, num_rows * 32);
                for row in rows {
                    match row.get(col_idx).and_then(|v| v.as_deref()) {
                        Some(s) => builder.append_value(s),
                        None => builder.append_null(),
                    }
                }
                Arc::new(builder.finish())
            }
        };

        arrays.push(array);
    }

    Ok(arrays)
}

/// Parse a timestamp string into epoch milliseconds.
///
/// Supports the same formats as `convert.rs`:
/// 1. RFC 3339: `2025-06-14T10:30:00Z`
/// 2. Space-separated: `2025-06-14 10:30:00`
/// 3. Date only: `2025-06-14` (midnight UTC)
pub(crate) fn parse_timestamp_ms(value: &str) -> Option<i64> {
    use chrono::{DateTime, NaiveDate, NaiveDateTime, Utc};

    if let Ok(dt) = DateTime::parse_from_rfc3339(value) {
        return Some(dt.with_timezone(&Utc).timestamp_millis());
    }
    if let Ok(ndt) = NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S") {
        return Some(ndt.and_utc().timestamp_millis());
    }
    // Also try with fractional seconds
    if let Ok(ndt) = NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S%.f") {
        return Some(ndt.and_utc().timestamp_millis());
    }
    if let Ok(nd) = NaiveDate::parse_from_str(value, "%Y-%m-%d") {
        return Some(nd.and_hms_opt(0, 0, 0)?.and_utc().timestamp_millis());
    }
    None
}
