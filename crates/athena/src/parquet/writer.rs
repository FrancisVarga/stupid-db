//! Public API for writing Athena query results to Parquet format.

use std::path::Path;
use std::sync::Arc;

use arrow::record_batch::RecordBatch;
use parquet::arrow::ArrowWriter;
use parquet::basic::Compression;
use parquet::file::properties::WriterProperties;
use tracing::debug;

use crate::result::AthenaQueryResult;
use super::builders::build_arrays;
use super::error::ParquetError;
use super::schema::build_schema;

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
