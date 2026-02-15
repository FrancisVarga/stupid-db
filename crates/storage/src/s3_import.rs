use std::collections::BTreeMap;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use arrow::array::{Array, StringArray};
use bytes::Bytes;
use chrono::{DateTime, Datelike, NaiveDate, Utc};
use futures::TryStreamExt;
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use tracing::info;

use stupid_core::{DocId, Document, FieldValue};

use crate::backend::StorageBackend;
use crate::error::StorageError;

/// S3 parquet import: list, download, and convert parquet files from S3 into local segments.
pub struct S3Importer;

/// A parquet file discovered in S3.
#[derive(Debug, Clone)]
pub struct S3ParquetFile {
    pub key: String,
    pub size: usize,
    pub event_type: String,
    pub date_stem: String,
}

/// A group of S3 parquet files destined for one weekly segment.
pub struct S3ImportGroup {
    pub segment_id: String,
    pub event_type: String,
    pub files: Vec<S3ParquetFile>,
}

impl S3Importer {
    /// List all .parquet files under an S3 prefix.
    /// The prefix is combined with the backend's configured prefix.
    pub async fn list_parquet(
        backend: &StorageBackend,
        prefix: &str,
    ) -> Result<Vec<S3ParquetFile>, StorageError> {
        let store = backend.store();
        let backend_prefix = backend.prefix();
        let user_prefix = prefix.trim_start_matches('/');
        let full_prefix = if backend_prefix.is_empty() {
            user_prefix.to_string()
        } else {
            format!("{}/{}", backend_prefix, user_prefix)
        };
        info!("Listing S3 objects under: {}", full_prefix);
        let path = object_store::path::Path::from(full_prefix.as_str());

        let mut files = Vec::new();
        let mut list = store.list(Some(&path));

        while let Some(meta) = list.try_next().await? {
            let key = meta.location.to_string();
            if key.ends_with(".parquet") {
                let event_type = Self::extract_event_type(&key);
                let date_stem = Self::extract_date_stem(&key);
                files.push(S3ParquetFile {
                    key,
                    size: meta.size,
                    event_type,
                    date_stem,
                });
            }
        }

        files.sort_by(|a, b| a.key.cmp(&b.key));
        info!("Found {} parquet files in S3", files.len());
        Ok(files)
    }

    /// Group parquet files by event_type/iso_week (matching local import-dir pattern).
    pub fn group_by_week(files: Vec<S3ParquetFile>) -> Vec<S3ImportGroup> {
        let mut groups: BTreeMap<String, S3ImportGroup> = BTreeMap::new();

        for file in files {
            let week = date_to_iso_week(&file.date_stem);
            let segment_id = format!("{}/{}", file.event_type, week);

            groups
                .entry(segment_id.clone())
                .or_insert_with(|| S3ImportGroup {
                    segment_id,
                    event_type: file.event_type.clone(),
                    files: Vec::new(),
                })
                .files
                .push(file);
        }

        groups.into_values().collect()
    }

    /// Import all parquet files from S3 prefix into local segments.
    pub async fn import_all(
        backend: &StorageBackend,
        prefix: &str,
        data_dir: &Path,
    ) -> Result<(u64, usize), StorageError> {
        let files = Self::list_parquet(backend, prefix).await?;
        if files.is_empty() {
            return Err(StorageError::Other(format!(
                "No .parquet files found under '{}'",
                prefix
            )));
        }

        let groups = Self::group_by_week(files);
        let total_groups = groups.len();
        info!(
            "Grouped into {} weekly segments for import",
            total_groups
        );

        let total_docs = AtomicU64::new(0);
        let completed = AtomicU64::new(0);
        let start = std::time::Instant::now();
        let store = backend.store();

        for group in &groups {
            let mut writer =
                stupid_segment::writer::SegmentWriter::new(data_dir, &group.segment_id)
                    .map_err(StorageError::Core)?;

            let mut group_docs = 0u64;

            for file in &group.files {
                let path = object_store::path::Path::from(file.key.as_str());
                let result = store.get(&path).await?;
                let data = result.bytes().await?;

                let documents =
                    parquet_bytes_to_documents(&data, &group.event_type)?;

                for doc in &documents {
                    writer.append(doc).map_err(StorageError::Core)?;
                }
                group_docs += documents.len() as u64;
            }

            writer.finalize().map_err(StorageError::Core)?;
            total_docs.fetch_add(group_docs, Ordering::Relaxed);
            let done = completed.fetch_add(1, Ordering::Relaxed) + 1;

            if done % 5 == 0 || done as usize == total_groups {
                info!(
                    "  Progress: {}/{} segments ({} docs, {:.1}s)",
                    done,
                    total_groups,
                    total_docs.load(Ordering::Relaxed),
                    start.elapsed().as_secs_f64()
                );
            }
        }

        let final_docs = total_docs.load(Ordering::Relaxed);
        let elapsed = start.elapsed();
        info!(
            "S3 import complete: {} segments, {} docs in {:.1}s",
            total_groups, final_docs, elapsed.as_secs_f64()
        );

        Ok((final_docs, total_groups))
    }

    /// Extract event type from S3 key path.
    /// Assumes: prefix/EventType/2025-06-14.parquet
    fn extract_event_type(key: &str) -> String {
        let parts: Vec<&str> = key.rsplitn(3, '/').collect();
        if parts.len() >= 2 {
            parts[1].to_string()
        } else {
            "Unknown".to_string()
        }
    }

    /// Extract date stem from filename.
    fn extract_date_stem(key: &str) -> String {
        key.rsplit('/')
            .next()
            .unwrap_or("unknown")
            .trim_end_matches(".parquet")
            .to_string()
    }
}

/// Convert parquet bytes to Documents (shared with local import).
pub fn parquet_bytes_to_documents(
    data: &[u8],
    event_type: &str,
) -> Result<Vec<Document>, StorageError> {
    let bytes = Bytes::copy_from_slice(data);
    let builder = ParquetRecordBatchReaderBuilder::try_new(bytes)
        .map_err(|e| StorageError::Parquet(e.to_string()))?;
    let reader = builder
        .build()
        .map_err(|e| StorageError::Parquet(e.to_string()))?;

    let mut documents = Vec::new();

    for batch_result in reader {
        let batch = batch_result.map_err(|e| StorageError::Parquet(e.to_string()))?;
        let schema = batch.schema();
        let num_rows = batch.num_rows();

        let columns: Vec<(&str, &StringArray)> = schema
            .fields()
            .iter()
            .enumerate()
            .filter_map(|(i, field)| {
                batch
                    .column(i)
                    .as_any()
                    .downcast_ref::<StringArray>()
                    .map(|arr| (field.name().as_str(), arr))
            })
            .collect();

        for row_idx in 0..num_rows {
            let mut fields = std::collections::HashMap::new();
            let mut timestamp: Option<DateTime<Utc>> = None;

            for &(col_name, arr) in &columns {
                if arr.is_null(row_idx) {
                    continue;
                }
                let val = arr.value(row_idx).trim();
                if val.is_empty() || val == "None" || val == "null" || val == "undefined" {
                    continue;
                }
                if col_name == "@timestamp" {
                    if let Ok(ts) = val.parse::<DateTime<Utc>>() {
                        timestamp = Some(ts);
                    }
                }
                fields.insert(col_name.to_string(), FieldValue::Text(val.to_string()));
            }

            documents.push(Document {
                id: DocId::new_v4(),
                timestamp: timestamp.unwrap_or_else(Utc::now),
                event_type: event_type.to_string(),
                fields,
            });
        }
    }

    Ok(documents)
}

/// Parse ISO week from a date-like string (e.g., "2025-06-14" -> "2025-W24").
fn date_to_iso_week(date_stem: &str) -> String {
    if let Ok(date) = NaiveDate::parse_from_str(date_stem, "%Y-%m-%d") {
        let iso = date.iso_week();
        format!("{}-W{:02}", iso.year(), iso.week())
    } else {
        "misc".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_event_type_from_key() {
        assert_eq!(
            S3Importer::extract_event_type("events/Login/2025-06-14.parquet"),
            "Login"
        );
        assert_eq!(
            S3Importer::extract_event_type("prefix/GameOpened/2025-06-14.parquet"),
            "GameOpened"
        );
    }

    #[test]
    fn extract_date_stem_from_key() {
        assert_eq!(
            S3Importer::extract_date_stem("events/Login/2025-06-14.parquet"),
            "2025-06-14"
        );
    }

    #[test]
    fn iso_week_parsing() {
        assert_eq!(date_to_iso_week("2025-06-14"), "2025-W24");
        assert_eq!(date_to_iso_week("invalid"), "misc");
    }
}
