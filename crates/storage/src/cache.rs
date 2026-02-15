use std::path::{Path, PathBuf};
use std::sync::Mutex;

use lru::LruCache;
use std::num::NonZeroUsize;
use tracing::{debug, info};

use crate::backend::StorageBackend;
use crate::error::StorageError;

/// Cached segment entry.
struct CachedSegment {
    local_path: PathBuf,
    size_bytes: u64,
}

/// LRU cache for remote segments, stored on local disk.
pub struct SegmentCache {
    cache: Mutex<LruCache<String, CachedSegment>>,
    cache_dir: PathBuf,
    max_bytes: u64,
    current_bytes: Mutex<u64>,
}

impl SegmentCache {
    pub fn new(cache_dir: &Path, max_gb: u32) -> Result<Self, StorageError> {
        std::fs::create_dir_all(cache_dir)?;
        Ok(Self {
            cache: Mutex::new(LruCache::new(NonZeroUsize::new(10_000).unwrap())),
            cache_dir: cache_dir.to_path_buf(),
            max_bytes: max_gb as u64 * 1_073_741_824, // GB → bytes
            current_bytes: Mutex::new(0),
        })
    }

    /// Check if a segment is cached locally.
    pub fn get_cached_dir(&self, segment_id: &str) -> Option<PathBuf> {
        let mut cache = self.cache.lock().unwrap();
        cache.get(segment_id).map(|entry| entry.local_path.clone())
    }

    /// Download a segment from remote storage into the cache.
    pub async fn fetch_segment(
        &self,
        backend: &StorageBackend,
        segment_id: &str,
    ) -> Result<PathBuf, StorageError> {
        // Check cache first
        if let Some(path) = self.get_cached_dir(segment_id) {
            debug!("Cache hit: {}", segment_id);
            return Ok(path);
        }

        info!("Cache miss: {} — downloading from S3...", segment_id);

        let prefix = backend.prefix();
        let store = backend.store();

        // Create local cache directory for this segment
        let seg_cache_dir = self.cache_dir.join("segments").join(segment_id);
        std::fs::create_dir_all(&seg_cache_dir)?;

        let mut total_size = 0u64;

        // Download documents.dat
        for filename in &["documents.dat", "meta.json"] {
            let s3_key = if prefix.is_empty() {
                format!("segments/{}/{}", segment_id, filename)
            } else {
                format!("{}/segments/{}/{}", prefix, segment_id, filename)
            };

            let path = object_store::path::Path::from(s3_key.as_str());
            match store.get(&path).await {
                Ok(result) => {
                    let data = result.bytes().await?;
                    let local_file = seg_cache_dir.join(filename);
                    tokio::fs::write(&local_file, &data).await.map_err(StorageError::Io)?;
                    total_size += data.len() as u64;
                }
                Err(object_store::Error::NotFound { .. }) if *filename == "meta.json" => {
                    // meta.json is optional for uncompressed segments
                    continue;
                }
                Err(e) => return Err(StorageError::ObjectStore(e)),
            }
        }

        // Evict if over capacity
        self.evict_if_needed(total_size)?;

        // Add to cache
        let mut cache = self.cache.lock().unwrap();
        cache.put(
            segment_id.to_string(),
            CachedSegment {
                local_path: seg_cache_dir.clone(),
                size_bytes: total_size,
            },
        );

        let mut current = self.current_bytes.lock().unwrap();
        *current += total_size;

        info!(
            "Cached segment '{}' ({:.1} MB, cache: {:.1}/{:.1} GB)",
            segment_id,
            total_size as f64 / 1_048_576.0,
            *current as f64 / 1_073_741_824.0,
            self.max_bytes as f64 / 1_073_741_824.0
        );

        Ok(seg_cache_dir)
    }

    fn evict_if_needed(&self, incoming_bytes: u64) -> Result<(), StorageError> {
        let mut cache = self.cache.lock().unwrap();
        let mut current = self.current_bytes.lock().unwrap();

        while *current + incoming_bytes > self.max_bytes {
            if let Some((evicted_id, evicted)) = cache.pop_lru() {
                debug!("Evicting cached segment: {}", evicted_id);
                *current = current.saturating_sub(evicted.size_bytes);
                // Best-effort delete
                std::fs::remove_dir_all(&evicted.local_path).ok();
            } else {
                break;
            }
        }

        Ok(())
    }
}
