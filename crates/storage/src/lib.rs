pub mod backend;
pub mod cache;
pub mod error;
pub mod s3_export;
pub mod s3_import;

use std::path::{Path, PathBuf};

use futures::TryStreamExt;
use tracing::info;

pub use backend::{LocalBackend, S3Backend, StorageBackend};
pub use cache::SegmentCache;
pub use error::StorageError;
pub use s3_export::S3Exporter;
pub use s3_import::S3Importer;

/// High-level storage engine: config-driven backend with optional cache.
pub struct StorageEngine {
    pub backend: StorageBackend,
    pub cache: Option<SegmentCache>,
    pub data_dir: PathBuf,
}

impl StorageEngine {
    /// Create a StorageEngine from config. Selects local or S3 based on AwsConfig.
    pub fn from_config(config: &stupid_core::Config) -> Result<Self, StorageError> {
        let data_dir = config.storage.data_dir.clone();

        if config.aws.is_configured() {
            let s3 = S3Backend::new(&config.aws)?;
            let cache = SegmentCache::new(
                &config.storage.cache_dir,
                config.storage.cache_max_gb,
            )?;
            Ok(Self {
                backend: StorageBackend::S3(s3),
                cache: Some(cache),
                data_dir,
            })
        } else {
            // Ensure data dir exists for local backend
            std::fs::create_dir_all(&data_dir).ok();
            let local = LocalBackend::new(&data_dir)?;
            Ok(Self {
                backend: StorageBackend::Local(local),
                cache: None,
                data_dir,
            })
        }
    }

    /// Discover all segment IDs (local or S3).
    pub async fn discover_segments(&self) -> Result<Vec<String>, StorageError> {
        match &self.backend {
            StorageBackend::Local(_) => Ok(discover_local_segments(&self.data_dir)),
            StorageBackend::S3(_) => self.discover_s3_segments().await,
        }
    }

    /// Discover segments from S3 by listing objects under segments/ prefix.
    async fn discover_s3_segments(&self) -> Result<Vec<String>, StorageError> {
        let store = self.backend.store();
        let prefix = self.backend.prefix();

        let list_prefix = if prefix.is_empty() {
            "segments/".to_string()
        } else {
            format!("{}/segments/", prefix)
        };

        let path = object_store::path::Path::from(list_prefix.as_str());
        let mut stream = store.list(Some(&path));
        let mut segment_ids = std::collections::HashSet::new();

        while let Some(meta) = stream.try_next().await? {
            let key = meta.location.to_string();
            // Look for documents.dat files
            if key.ends_with("/documents.dat") {
                // Extract segment_id: strip prefix and trailing /documents.dat
                let stripped = key
                    .strip_prefix(&list_prefix)
                    .unwrap_or(&key);
                let seg_id = stripped
                    .strip_suffix("/documents.dat")
                    .unwrap_or(stripped);
                if !seg_id.is_empty() {
                    segment_ids.insert(seg_id.to_string());
                }
            }
        }

        let mut segments: Vec<String> = segment_ids.into_iter().collect();
        segments.sort();
        info!("Discovered {} segments in S3", segments.len());
        Ok(segments)
    }

    /// Get segment data directory — either local data_dir or cache dir for remote.
    /// For S3, downloads the segment to cache if not already cached.
    pub async fn segment_data_dir(&self, segment_id: &str) -> Result<PathBuf, StorageError> {
        match (&self.backend, &self.cache) {
            (StorageBackend::Local(_), _) => Ok(self.data_dir.clone()),
            (StorageBackend::S3(_), Some(cache)) => {
                let cache_dir = cache.fetch_segment(&self.backend, segment_id).await?;
                // Return the parent of the segment dir (cache_dir IS the segment dir)
                // SegmentReader::open expects data_dir where segments/{id}/ lives
                // cache_dir = cache/segments/{id}/, so parent.parent = cache/
                Ok(cache_dir
                    .parent()
                    .and_then(|p| p.parent())
                    .unwrap_or(&cache_dir)
                    .to_path_buf())
            }
            (StorageBackend::S3(_), None) => Err(StorageError::NotConfigured(
                "S3 backend requires cache to be configured".into(),
            )),
        }
    }
}

/// Discover segments on local filesystem (same logic as server's discover_segments).
fn discover_local_segments(data_dir: &Path) -> Vec<String> {
    let segments_dir = data_dir.join("segments");
    if !segments_dir.exists() {
        return Vec::new();
    }

    let mut segments = Vec::new();
    for entry in walkdir::WalkDir::new(&segments_dir)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if path.is_file() && path.file_name().map(|n| n == "documents.dat").unwrap_or(false) {
            if let Ok(rel) = path.parent().unwrap_or(path).strip_prefix(&segments_dir) {
                if let Some(seg_id) = rel.to_str() {
                    segments.push(seg_id.replace('\\', "/"));
                }
            }
        }
    }

    segments.sort();
    segments
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discover_empty_dir() {
        let tmp = std::env::temp_dir().join("stupid-storage-discover-test");
        std::fs::create_dir_all(&tmp).unwrap();
        let segments = discover_local_segments(&tmp);
        assert!(segments.is_empty());
        std::fs::remove_dir_all(&tmp).ok();
    }
}
