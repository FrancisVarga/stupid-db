use std::path::Path;

use futures::stream::{self, StreamExt};
use object_store::ObjectStore;
use tracing::info;

use crate::backend::StorageBackend;
use crate::error::StorageError;

/// Export local segments and graph data to S3.
pub struct S3Exporter;

impl S3Exporter {
    /// Upload local segments to S3.
    /// Skips segments that already exist in S3 (incremental).
    pub async fn export_segments(
        backend: &StorageBackend,
        data_dir: &Path,
        segment_ids: &[String],
    ) -> Result<(usize, usize), StorageError> {
        let store = backend.store();
        let prefix = backend.prefix();
        let start = std::time::Instant::now();

        let mut uploaded = 0usize;
        let mut skipped = 0usize;

        for segment_id in segment_ids {
            let s3_doc_key = Self::s3_key(prefix, segment_id, "documents.dat");
            let s3_path = object_store::path::Path::from(s3_doc_key.as_str());

            // Check if already exported
            if store.head(&s3_path).await.is_ok() {
                skipped += 1;
                continue;
            }

            let seg_dir = data_dir.join("segments").join(segment_id);

            for filename in &["documents.dat", "meta.json"] {
                let local_path = seg_dir.join(filename);
                if !local_path.exists() {
                    continue;
                }

                let data = tokio::fs::read(&local_path)
                    .await
                    .map_err(StorageError::Io)?;
                let key = Self::s3_key(prefix, segment_id, filename);
                let path = object_store::path::Path::from(key.as_str());
                store
                    .put(&path, bytes::Bytes::from(data).into())
                    .await?;
            }

            uploaded += 1;

            if uploaded % 5 == 0 {
                info!(
                    "  Export progress: {}/{} segments ({} skipped, {:.1}s)",
                    uploaded,
                    segment_ids.len(),
                    skipped,
                    start.elapsed().as_secs_f64()
                );
            }
        }

        info!(
            "Export complete: {} uploaded, {} skipped in {:.1}s",
            uploaded,
            skipped,
            start.elapsed().as_secs_f64()
        );

        Ok((uploaded, skipped))
    }

    /// Export graph stats/summary to S3 as JSON.
    /// Accepts any Serialize value (typically GraphStats from stupid-graph).
    pub async fn export_graph(
        backend: &StorageBackend,
        graph_stats: &impl serde::Serialize,
    ) -> Result<(), StorageError> {
        let store = backend.store();
        let prefix = backend.prefix();

        let json = serde_json::to_string_pretty(graph_stats)
            .map_err(|e| StorageError::Other(e.to_string()))?;

        let key = if prefix.is_empty() {
            "graph/stats.json".to_string()
        } else {
            format!("{}/graph/stats.json", prefix)
        };

        let path = object_store::path::Path::from(key.as_str());
        store
            .put(&path, bytes::Bytes::from(json).into())
            .await?;

        info!("Exported graph stats to S3");
        Ok(())
    }

    /// Upload parquet files in parallel (for large exports).
    pub async fn upload_files_parallel(
        backend: &StorageBackend,
        files: Vec<(String, Vec<u8>)>,
        concurrency: usize,
    ) -> Result<usize, StorageError> {
        let store = backend.store_arc();

        let results: Vec<Result<_, StorageError>> = stream::iter(files)
            .map(|(key, data)| {
                let store = store.clone();
                async move {
                    let path = object_store::path::Path::from(key.as_str());
                    store
                        .put(&path, bytes::Bytes::from(data).into())
                        .await
                        .map_err(StorageError::ObjectStore)
                }
            })
            .buffer_unordered(concurrency)
            .collect()
            .await;

        let mut success = 0;
        for r in &results {
            if r.is_ok() {
                success += 1;
            }
        }

        Ok(success)
    }

    fn s3_key(prefix: &str, segment_id: &str, filename: &str) -> String {
        if prefix.is_empty() {
            format!("segments/{}/{}", segment_id, filename)
        } else {
            format!("{}/segments/{}/{}", prefix, segment_id, filename)
        }
    }
}
