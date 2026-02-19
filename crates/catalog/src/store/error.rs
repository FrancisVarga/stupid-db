use thiserror::Error;

/// Errors produced by [`CatalogStore`](super::CatalogStore) operations.
#[derive(Debug, Error)]
pub enum CatalogStoreError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

/// Convert a segment ID (which may contain `/`) to a safe JSON filename.
pub(super) fn segment_filename(segment_id: &str) -> String {
    format!("{}.json", segment_id.replace('/', "__"))
}
