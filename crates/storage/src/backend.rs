use std::path::PathBuf;
use std::sync::Arc;

use object_store::aws::AmazonS3Builder;
use object_store::local::LocalFileSystem;
use object_store::ObjectStore;
use tracing::info;

use stupid_core::config::AwsConfig;

use crate::error::StorageError;

/// Unified storage backend wrapping object_store.
pub enum StorageBackend {
    Local(LocalBackend),
    S3(S3Backend),
}

impl StorageBackend {
    /// Get the underlying ObjectStore.
    pub fn store(&self) -> &dyn ObjectStore {
        match self {
            StorageBackend::Local(b) => b.store.as_ref(),
            StorageBackend::S3(b) => b.store.as_ref(),
        }
    }

    /// Get an Arc-wrapped ObjectStore (needed for parquet async reader).
    pub fn store_arc(&self) -> Arc<dyn ObjectStore> {
        match self {
            StorageBackend::Local(b) => b.store.clone(),
            StorageBackend::S3(b) => b.store.clone(),
        }
    }

    pub fn is_remote(&self) -> bool {
        matches!(self, StorageBackend::S3(_))
    }

    /// S3 key prefix for segments (e.g. "production/segments/").
    pub fn prefix(&self) -> &str {
        match self {
            StorageBackend::Local(_) => "",
            StorageBackend::S3(b) => &b.prefix,
        }
    }
}

/// Local filesystem backend.
pub struct LocalBackend {
    pub store: Arc<dyn ObjectStore>,
    pub data_dir: PathBuf,
}

impl LocalBackend {
    pub fn new(data_dir: &std::path::Path) -> Result<Self, StorageError> {
        let canonical = std::fs::canonicalize(data_dir).unwrap_or_else(|_| data_dir.to_path_buf());
        let store = LocalFileSystem::new_with_prefix(&canonical)
            .map_err(|e| StorageError::Other(format!("local filesystem error: {e}")))?;
        info!("Storage: local backend at {}", canonical.display());
        Ok(Self {
            store: Arc::new(store),
            data_dir: canonical,
        })
    }
}

/// S3 backend.
pub struct S3Backend {
    pub store: Arc<dyn ObjectStore>,
    pub bucket: String,
    pub prefix: String,
}

impl S3Backend {
    pub fn new(aws: &AwsConfig) -> Result<Self, StorageError> {
        let bucket = aws
            .s3_bucket
            .as_deref()
            .ok_or_else(|| StorageError::NotConfigured("S3_BUCKET not set".into()))?;

        let mut builder = AmazonS3Builder::new()
            .with_region(&aws.region);

        if let Some(ref key) = aws.access_key_id {
            builder = builder.with_access_key_id(key);
        }
        if let Some(ref secret) = aws.secret_access_key {
            builder = builder.with_secret_access_key(secret);
        }
        if let Some(ref token) = aws.session_token {
            builder = builder.with_token(token);
        }

        if let Some(ref endpoint) = aws.endpoint_url {
            if !endpoint.is_empty() {
                // Ensure endpoint has a scheme — object_store requires absolute URLs
                let endpoint_url = if endpoint.starts_with("http://") || endpoint.starts_with("https://") {
                    endpoint.clone()
                } else {
                    format!("https://{}", endpoint)
                };
                builder = builder
                    .with_bucket_name(bucket)
                    .with_endpoint(&endpoint_url)
                    .with_allow_http(endpoint_url.starts_with("http://"));
            }
        } else {
            // Standard AWS S3 — use with_url for proper endpoint resolution
            let url = format!("s3://{}", bucket);
            builder = builder.with_url(&url);
        }

        let store = builder.build()?;

        let prefix = aws
            .s3_prefix
            .as_deref()
            .unwrap_or("")
            .trim_end_matches('/')
            .to_string();

        info!(
            "Storage: S3 backend s3://{}/{} (region: {})",
            bucket, prefix, aws.region
        );

        Ok(Self {
            store: Arc::new(store),
            bucket: bucket.to_string(),
            prefix,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_backend_creates() {
        let tmp = std::env::temp_dir().join("stupid-storage-test");
        std::fs::create_dir_all(&tmp).unwrap();
        let backend = LocalBackend::new(&tmp).unwrap();
        assert!(!StorageBackend::Local(backend).is_remote());
        std::fs::remove_dir_all(&tmp).ok();
    }
}
