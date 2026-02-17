//! Convert [`AthenaQueryResult`] to Apache Parquet files.
//!
//! Maps Athena SQL types to Arrow data types and writes typed, columnar
//! Parquet files with Zstd compression. This avoids the naive approach of
//! storing everything as strings and enables downstream tools (DuckDB,
//! Polars, Spark) to read the data with proper types and predicate pushdown.

use std::path::Path;
use std::sync::Arc;

use arrow::array::{
    ArrayRef, BooleanBuilder, Float64Builder, Int64Builder, StringBuilder,
    TimestampMillisecondBuilder,
};
use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use arrow::record_batch::RecordBatch;
use parquet::arrow::ArrowWriter;
use parquet::basic::Compression;
use parquet::file::properties::WriterProperties;
use tracing::debug;

use crate::result::{AthenaColumn, AthenaQueryResult};

// ---------------------------------------------------------------------------
// Error
// ---------------------------------------------------------------------------

/// Errors that can occur during Parquet conversion or writing.
#[derive(Debug, thiserror::Error)]
pub enum ParquetError {
    /// Failed to build Arrow arrays from result data.
    #[error("Arrow conversion error: {0}")]
    Arrow(#[from] arrow::error::ArrowError),

    /// Failed to write Parquet file.
    #[error("Parquet write error: {0}")]
    Write(#[from] parquet::errors::ParquetError),

    /// I/O error when creating/writing the output file.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

// ---------------------------------------------------------------------------
// Type mapping
// ---------------------------------------------------------------------------

/// Map an Athena SQL type string to an Arrow DataType.
///
/// Athena types are documented at:
/// <https://docs.aws.amazon.com/athena/latest/ug/data-types.html>
fn athena_type_to_arrow(athena_type: &str) -> DataType {
    match athena_type.to_lowercase().as_str() {
        // Integer family
        "tinyint" | "smallint" | "int" | "integer" | "bigint" => DataType::Int64,

        // Floating-point family
        "float" | "real" | "double" | "decimal" => DataType::Float64,

        // Boolean
        "boolean" => DataType::Boolean,

        // Timestamps
        "timestamp" | "timestamp with time zone" => {
            DataType::Timestamp(TimeUnit::Millisecond, Some("UTC".into()))
        }

        // Date stored as UTF-8 (Athena returns dates as strings like "2025-01-15")
        "date" => DataType::Utf8,

        // String family (varchar, char, string, arrays, maps, structs, etc.)
        _ => DataType::Utf8,
    }
}

/// Build an Arrow [`Schema`] from Athena column definitions.
fn build_schema(columns: &[AthenaColumn]) -> Schema {
    let fields: Vec<Field> = columns
        .iter()
        .map(|col| Field::new(&col.name, athena_type_to_arrow(&col.data_type), true))
        .collect();
    Schema::new(fields)
}

// ---------------------------------------------------------------------------
// Column builders
// ---------------------------------------------------------------------------

/// Build typed Arrow arrays from the string-based Athena rows.
///
/// For each column we inspect the target Arrow type and parse the string
/// values into the correct native type. Unparseable values become NULL in
/// the output (safe fallback — no data loss, just type downgrade).
fn build_arrays(
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
fn parse_timestamp_ms(value: &str) -> Option<i64> {
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

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Convert an [`AthenaQueryResult`] into an Arrow [`RecordBatch`].
///
/// This is useful when you want to do further in-memory processing with
/// Arrow before deciding on the output format.
pub fn result_to_record_batch(result: &AthenaQueryResult) -> Result<RecordBatch, ParquetError> {
    let schema = Arc::new(build_schema(&result.columns));
    let arrays = build_arrays(&result.columns, &result.rows, &schema)?;
    let batch = RecordBatch::try_new(schema, arrays)?;
    Ok(batch)
}

/// Write an [`AthenaQueryResult`] to a Parquet file at the given path.
///
/// Uses Zstd compression (level 3) and stores query metadata (query_id,
/// bytes_scanned, execution_time_ms) as key-value metadata in the Parquet
/// file footer.
pub fn write_parquet(result: &AthenaQueryResult, path: &Path) -> Result<u64, ParquetError> {
    let batch = result_to_record_batch(result)?;
    let row_count = batch.num_rows() as u64;

    // Ensure parent directories exist.
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let file = std::fs::File::create(path)?;

    let props = WriterProperties::builder()
        .set_compression(Compression::ZSTD(Default::default()))
        .set_key_value_metadata(Some(vec![
            parquet::format::KeyValue::new("athena.query_id".to_string(), Some(result.metadata.query_id.clone())),
            parquet::format::KeyValue::new(
                "athena.bytes_scanned".to_string(),
                Some(result.metadata.bytes_scanned.to_string()),
            ),
            parquet::format::KeyValue::new(
                "athena.execution_time_ms".to_string(),
                Some(result.metadata.execution_time_ms.to_string()),
            ),
            parquet::format::KeyValue::new(
                "athena.state".to_string(),
                Some(result.metadata.state.clone()),
            ),
        ]))
        .build();

    let mut writer = ArrowWriter::try_new(file, batch.schema(), Some(props))?;
    writer.write(&batch)?;
    writer.close()?;

    debug!(
        path = %path.display(),
        rows = row_count,
        query_id = %result.metadata.query_id,
        "Wrote Parquet file"
    );

    Ok(row_count)
}

/// Write an [`AthenaQueryResult`] to an in-memory Parquet buffer.
///
/// Returns the raw bytes of a valid Parquet file. Useful for HTTP responses
/// where you want to stream the file without touching disk.
pub fn write_parquet_bytes(result: &AthenaQueryResult) -> Result<Vec<u8>, ParquetError> {
    let batch = result_to_record_batch(result)?;

    let props = WriterProperties::builder()
        .set_compression(Compression::ZSTD(Default::default()))
        .set_key_value_metadata(Some(vec![
            parquet::format::KeyValue::new("athena.query_id".to_string(), Some(result.metadata.query_id.clone())),
            parquet::format::KeyValue::new(
                "athena.bytes_scanned".to_string(),
                Some(result.metadata.bytes_scanned.to_string()),
            ),
        ]))
        .build();

    let mut buf = Vec::new();
    let mut writer = ArrowWriter::try_new(&mut buf, batch.schema(), Some(props))?;
    writer.write(&batch)?;
    writer.close()?;

    Ok(buf)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::result::{AthenaColumn, AthenaQueryResult, QueryMetadata};
    use arrow::datatypes::DataType;

    fn test_metadata() -> QueryMetadata {
        QueryMetadata {
            query_id: "test-parquet-001".to_string(),
            bytes_scanned: 2048,
            execution_time_ms: 150,
            state: "SUCCEEDED".to_string(),
            output_location: Some("s3://bucket/results/test.csv".into()),
        }
    }

    fn sample_result() -> AthenaQueryResult {
        AthenaQueryResult {
            columns: vec![
                AthenaColumn { name: "id".into(), data_type: "bigint".into() },
                AthenaColumn { name: "name".into(), data_type: "varchar".into() },
                AthenaColumn { name: "score".into(), data_type: "double".into() },
                AthenaColumn { name: "active".into(), data_type: "boolean".into() },
                AthenaColumn { name: "created_at".into(), data_type: "timestamp".into() },
            ],
            rows: vec![
                vec![
                    Some("1".into()),
                    Some("alice".into()),
                    Some("9.5".into()),
                    Some("true".into()),
                    Some("2025-06-14T10:30:00Z".into()),
                ],
                vec![
                    Some("2".into()),
                    Some("bob".into()),
                    None,
                    Some("false".into()),
                    Some("2025-06-14 11:00:00".into()),
                ],
                vec![
                    Some("3".into()),
                    None,
                    Some("7.0".into()),
                    None,
                    None,
                ],
            ],
            metadata: test_metadata(),
        }
    }

    #[test]
    fn test_athena_type_mapping() {
        assert_eq!(athena_type_to_arrow("bigint"), DataType::Int64);
        assert_eq!(athena_type_to_arrow("int"), DataType::Int64);
        assert_eq!(athena_type_to_arrow("INTEGER"), DataType::Int64);
        assert_eq!(athena_type_to_arrow("tinyint"), DataType::Int64);
        assert_eq!(athena_type_to_arrow("double"), DataType::Float64);
        assert_eq!(athena_type_to_arrow("FLOAT"), DataType::Float64);
        assert_eq!(athena_type_to_arrow("decimal"), DataType::Float64);
        assert_eq!(athena_type_to_arrow("boolean"), DataType::Boolean);
        assert_eq!(
            athena_type_to_arrow("timestamp"),
            DataType::Timestamp(TimeUnit::Millisecond, Some("UTC".into()))
        );
        assert_eq!(athena_type_to_arrow("varchar"), DataType::Utf8);
        assert_eq!(athena_type_to_arrow("string"), DataType::Utf8);
        assert_eq!(athena_type_to_arrow("date"), DataType::Utf8);
        assert_eq!(athena_type_to_arrow("array<string>"), DataType::Utf8);
    }

    #[test]
    fn test_build_schema() {
        let columns = vec![
            AthenaColumn { name: "id".into(), data_type: "bigint".into() },
            AthenaColumn { name: "name".into(), data_type: "varchar".into() },
        ];
        let schema = build_schema(&columns);
        assert_eq!(schema.fields().len(), 2);
        assert_eq!(schema.field(0).name(), "id");
        assert_eq!(*schema.field(0).data_type(), DataType::Int64);
        assert_eq!(schema.field(1).name(), "name");
        assert_eq!(*schema.field(1).data_type(), DataType::Utf8);
    }

    #[test]
    fn test_result_to_record_batch() {
        let result = sample_result();
        let batch = result_to_record_batch(&result).unwrap();
        assert_eq!(batch.num_rows(), 3);
        assert_eq!(batch.num_columns(), 5);
        assert_eq!(batch.schema().field(0).name(), "id");
        assert_eq!(*batch.schema().field(0).data_type(), DataType::Int64);
    }

    #[test]
    fn test_null_handling_in_batch() {
        let result = sample_result();
        let batch = result_to_record_batch(&result).unwrap();

        // Column "score" (index 2): row 1 is NULL
        let score_col = batch.column(2);
        assert!(score_col.is_valid(0)); // 9.5
        assert!(!score_col.is_valid(1)); // NULL
        assert!(score_col.is_valid(2)); // 7.0

        // Column "name" (index 1): row 2 is NULL
        let name_col = batch.column(1);
        assert!(name_col.is_valid(0)); // alice
        assert!(name_col.is_valid(1)); // bob
        assert!(!name_col.is_valid(2)); // NULL
    }

    #[test]
    fn test_write_parquet_to_file() {
        let result = sample_result();
        let dir = std::env::temp_dir().join("stupid-db-test-parquet");
        let path = dir.join("test_output.parquet");

        let row_count = write_parquet(&result, &path).unwrap();
        assert_eq!(row_count, 3);
        assert!(path.exists());

        // Read back and verify.
        let file = std::fs::File::open(&path).unwrap();
        let reader = parquet::arrow::arrow_reader::ParquetRecordBatchReader::try_new(file, 1024).unwrap();
        let batches: Vec<RecordBatch> = reader.into_iter().map(|r| r.unwrap()).collect();
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].num_rows(), 3);
        assert_eq!(batches[0].num_columns(), 5);

        // Cleanup.
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_write_parquet_bytes() {
        let result = sample_result();
        let bytes = write_parquet_bytes(&result).unwrap();

        // Parquet files start with magic bytes "PAR1".
        assert!(bytes.len() > 4);
        assert_eq!(&bytes[..4], b"PAR1");
    }

    #[test]
    fn test_parquet_metadata_in_file() {
        use parquet::file::reader::FileReader;

        let result = sample_result();
        let dir = std::env::temp_dir().join("stupid-db-test-parquet-meta");
        let path = dir.join("meta_test.parquet");

        write_parquet(&result, &path).unwrap();

        let file = std::fs::File::open(&path).unwrap();
        let reader = parquet::file::reader::SerializedFileReader::new(file).unwrap();
        let file_metadata = reader.metadata().file_metadata();
        let kv = file_metadata.key_value_metadata().expect("metadata present");

        let query_id_kv = kv.iter().find(|kv| kv.key == "athena.query_id").unwrap();
        assert_eq!(query_id_kv.value.as_deref(), Some("test-parquet-001"));

        let scanned_kv = kv.iter().find(|kv| kv.key == "athena.bytes_scanned").unwrap();
        assert_eq!(scanned_kv.value.as_deref(), Some("2048"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_empty_result_writes_valid_parquet() {
        let result = AthenaQueryResult {
            columns: vec![
                AthenaColumn { name: "col1".into(), data_type: "varchar".into() },
            ],
            rows: vec![],
            metadata: test_metadata(),
        };

        let bytes = write_parquet_bytes(&result).unwrap();
        assert!(bytes.len() > 4);
        assert_eq!(&bytes[..4], b"PAR1");
    }

    #[test]
    fn test_parse_timestamp_formats() {
        // RFC3339
        let ms = parse_timestamp_ms("2025-06-14T10:30:00Z").unwrap();
        assert!(ms > 0);

        // Space-separated
        let ms2 = parse_timestamp_ms("2025-06-14 10:30:00").unwrap();
        assert!(ms2 > 0);

        // With fractional seconds
        let ms3 = parse_timestamp_ms("2025-06-14 10:30:00.123").unwrap();
        assert!(ms3 > 0);

        // Date only
        let ms4 = parse_timestamp_ms("2025-06-14").unwrap();
        assert!(ms4 > 0);

        // Invalid
        assert!(parse_timestamp_ms("not-a-date").is_none());
    }

    #[test]
    fn test_invalid_numbers_become_null() {
        let result = AthenaQueryResult {
            columns: vec![
                AthenaColumn { name: "val".into(), data_type: "bigint".into() },
            ],
            rows: vec![
                vec![Some("not-a-number".into())],
                vec![Some("42".into())],
            ],
            metadata: test_metadata(),
        };

        let batch = result_to_record_batch(&result).unwrap();
        let col = batch.column(0);
        assert!(!col.is_valid(0)); // "not-a-number" → NULL
        assert!(col.is_valid(1)); // 42 → valid
    }
}
