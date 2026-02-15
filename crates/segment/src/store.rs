use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tracing::{debug, info};

use stupid_core::config::StorageConfig;
use stupid_core::{DocAddress, DocId, Document, SegmentId, StupidError};
use stupid_ingest::parquet_import::ParquetImporter;

use crate::filter::ScanFilter;
use crate::index::{DocIndex, DocIndexEntry};
use crate::manager::SegmentManager;
use crate::schema::SchemaRegistry;

/// Overall statistics for the document store.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoreStats {
    /// Number of segments (active writers + sealed readers).
    pub segment_count: usize,
    /// Total number of documents across all segments.
    pub document_count: u64,
    /// Total size in bytes across all segment data files.
    pub total_bytes: u64,
}

/// High-level API for document storage combining segment management,
/// indexing, and schema tracking.
pub struct DocumentStore {
    manager: SegmentManager,
    /// Per-segment document indexes, keyed by segment ID.
    indexes: HashMap<SegmentId, DocIndex>,
    schema_registry: SchemaRegistry,
    data_dir: PathBuf,
}

impl DocumentStore {
    /// Create a new DocumentStore, loading existing segments, indexes, and schema.
    pub fn new(config: &StorageConfig) -> Result<Self, StupidError> {
        let manager = SegmentManager::new(config)?;
        let data_dir = config.data_dir.clone();

        // Load per-segment indexes
        let mut indexes = HashMap::new();
        for segment_id in manager.list_segments() {
            let index_path = data_dir
                .join("segments")
                .join(&segment_id)
                .join("documents.idx");
            let index = DocIndex::load(&index_path)?;
            if !index.is_empty() {
                debug!(
                    segment_id = %segment_id,
                    doc_count = index.len(),
                    "Loaded document index"
                );
                indexes.insert(segment_id, index);
            }
        }

        // Load schema registry
        let schema_path = data_dir.join("schema_registry.json");
        let schema_registry = SchemaRegistry::load(&schema_path)?;

        info!(
            segments = indexes.len(),
            event_types = schema_registry.event_types().len(),
            "DocumentStore initialized"
        );

        Ok(Self {
            manager,
            indexes,
            schema_registry,
            data_dir,
        })
    }

    /// Insert a document into the store, returning its address.
    ///
    /// The document is appended to the appropriate segment (based on timestamp),
    /// the index is updated, and schema statistics are tracked.
    pub fn insert(&mut self, doc: Document) -> Result<DocAddress, StupidError> {
        // Determine target segment
        let segment_id = SegmentManager::segment_id_for_timestamp(&doc.timestamp);

        // Get or create writer for this segment
        let writer = self.manager.get_or_create_writer(&segment_id)?;

        // Serialize the document to get its length
        let encoded =
            rmp_serde::to_vec(&doc).map_err(|e| StupidError::Serialize(e.to_string()))?;
        let length = encoded.len() as u32;

        // Append to segment (returns offset before write)
        let offset = writer.append(&doc)?;

        // Update the index for this segment
        let index = self
            .indexes
            .entry(segment_id.clone())
            .or_insert_with(DocIndex::new);

        let entry = DocIndexEntry {
            offset,
            length,
            timestamp: doc.timestamp,
            event_type: doc.event_type.clone(),
        };
        index.add(doc.id, entry);

        // Update schema registry
        self.schema_registry.observe(&doc);

        debug!(
            doc_id = %doc.id,
            segment_id = %segment_id,
            offset = offset,
            "Document inserted"
        );

        Ok(DocAddress { segment_id, offset })
    }

    /// Retrieve a document by its address.
    pub fn get(&self, addr: &DocAddress) -> Result<Document, StupidError> {
        if let Some(reader) = self.manager.get_reader(&addr.segment_id) {
            reader.read_at(addr.offset)
        } else {
            Err(StupidError::SegmentNotFound(addr.segment_id.clone()))
        }
    }

    /// Retrieve a document by its ID, searching all segment indexes.
    pub fn get_by_id(&self, id: &DocId) -> Result<Document, StupidError> {
        // Search all segment indexes for this document ID
        for (segment_id, index) in &self.indexes {
            if let Some(entry) = index.get(id) {
                let addr = DocAddress {
                    segment_id: segment_id.clone(),
                    offset: entry.offset,
                };
                return self.get(&addr);
            }
        }

        Err(StupidError::DocumentNotFound(0)) // 0 is placeholder offset
    }

    /// Scan documents matching the given filter.
    ///
    /// Determines the relevant segments based on the time range, iterates each
    /// segment's documents, and applies the filter predicate.
    pub fn scan(&self, filter: &ScanFilter) -> Result<Vec<Document>, StupidError> {
        // Determine segments in the time range
        let segment_ids = self
            .manager
            .segments_in_range(filter.time_start, filter.time_end);

        let mut results = Vec::new();

        for segment_id in segment_ids {
            if let Some(reader) = self.manager.get_reader(&segment_id) {
                for doc_result in reader.iter() {
                    let doc = doc_result?;
                    if filter.matches(&doc) {
                        results.push(doc);
                    }
                }
            }
        }

        debug!(
            filter_event_type = ?filter.event_type,
            filter_fields = filter.field_filters.len(),
            results = results.len(),
            "Scan completed"
        );

        Ok(results)
    }

    /// Import documents from a Parquet file, inserting each document into the store.
    ///
    /// Returns the number of documents imported.
    pub fn import_parquet(&mut self, path: &Path, event_type: &str) -> Result<usize, StupidError> {
        let documents = ParquetImporter::import(path, event_type)?;
        let count = documents.len();

        for doc in documents {
            self.insert(doc)?;
        }

        info!(
            path = %path.display(),
            event_type = event_type,
            count = count,
            "Parquet import completed"
        );

        Ok(count)
    }

    /// Flush all active writers, persist all indexes and schema to disk.
    pub fn flush(&mut self) -> Result<(), StupidError> {
        // Seal all active writers
        self.manager.flush_all()?;

        // Save all indexes
        for (segment_id, index) in &self.indexes {
            let index_path = self
                .data_dir
                .join("segments")
                .join(segment_id)
                .join("documents.idx");
            index.save(&index_path)?;
            debug!(segment_id = %segment_id, "Index saved");
        }

        // Save schema registry
        let schema_path = self.data_dir.join("schema_registry.json");
        self.schema_registry.save(&schema_path)?;

        info!("DocumentStore flushed");
        Ok(())
    }

    /// Return overall statistics for the store.
    pub fn stats(&self) -> StoreStats {
        let segment_count = self.manager.list_segments().len();
        let document_count: u64 = self.indexes.values().map(|idx| idx.len() as u64).sum();

        // Estimate total bytes from segment meta.json files
        let mut total_bytes = 0u64;
        for segment_id in self.manager.list_segments() {
            let meta_path = self
                .data_dir
                .join("segments")
                .join(&segment_id)
                .join("meta.json");

            if let Ok(content) = std::fs::read_to_string(&meta_path) {
                if let Ok(meta) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(size) = meta.get("size_bytes").and_then(|v| v.as_u64()) {
                        total_bytes += size;
                    }
                }
            }
        }

        StoreStats {
            segment_count,
            document_count,
            total_bytes,
        }
    }

    /// Access the schema registry.
    pub fn schema_registry(&self) -> &SchemaRegistry {
        &self.schema_registry
    }

    /// Access the segment manager.
    pub fn manager(&self) -> &SegmentManager {
        &self.manager
    }

    /// Access the segment manager mutably (for advanced operations like eviction).
    pub fn manager_mut(&mut self) -> &mut SegmentManager {
        &mut self.manager
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use std::fs;
    use uuid::Uuid;

    use stupid_core::FieldValue;

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("stupid-store-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn make_config(data_dir: PathBuf) -> StorageConfig {
        StorageConfig {
            data_dir: data_dir.clone(),
            segment_retention_days: 30,
            cache_dir: data_dir.join("cache"),
            cache_max_gb: 1,
        }
    }

    fn make_doc(event_type: &str, timestamp: chrono::DateTime<Utc>) -> Document {
        Document {
            id: Uuid::new_v4(),
            timestamp,
            event_type: event_type.to_string(),
            fields: {
                let mut m = std::collections::HashMap::new();
                m.insert("test".to_string(), FieldValue::Text("value".to_string()));
                m
            },
        }
    }

    #[test]
    fn test_new_creates_store() {
        let dir = temp_dir();
        let config = make_config(dir.clone());
        let _store = DocumentStore::new(&config).unwrap();
        assert!(dir.join("segments").is_dir());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_insert_and_get() {
        let dir = temp_dir();
        let config = make_config(dir.clone());
        let mut store = DocumentStore::new(&config).unwrap();

        let doc = make_doc("Login", Utc::now());
        let doc_id = doc.id;
        let addr = store.insert(doc.clone()).unwrap();

        // Flush to seal the segment so we can read from it
        store.flush().unwrap();

        let retrieved = store.get(&addr).unwrap();
        assert_eq!(retrieved.id, doc_id);
        assert_eq!(retrieved.event_type, "Login");

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_insert_and_get_by_id() {
        let dir = temp_dir();
        let config = make_config(dir.clone());
        let mut store = DocumentStore::new(&config).unwrap();

        let doc = make_doc("GameOpened", Utc::now());
        let doc_id = doc.id;
        store.insert(doc.clone()).unwrap();

        // Flush to seal the segment so we can read from it
        store.flush().unwrap();

        let retrieved = store.get_by_id(&doc_id).unwrap();
        assert_eq!(retrieved.id, doc_id);
        assert_eq!(retrieved.event_type, "GameOpened");

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_get_by_id_not_found() {
        let dir = temp_dir();
        let config = make_config(dir.clone());
        let store = DocumentStore::new(&config).unwrap();

        let missing_id = Uuid::new_v4();
        let result = store.get_by_id(&missing_id);
        assert!(result.is_err());

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_scan_empty_filter() {
        let dir = temp_dir();
        let config = make_config(dir.clone());
        let mut store = DocumentStore::new(&config).unwrap();

        store.insert(make_doc("Login", Utc::now())).unwrap();
        store.insert(make_doc("GameOpened", Utc::now())).unwrap();
        store.flush().unwrap();

        let filter = ScanFilter::new();
        let results = store.scan(&filter).unwrap();
        assert_eq!(results.len(), 2);

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_scan_by_event_type() {
        let dir = temp_dir();
        let config = make_config(dir.clone());
        let mut store = DocumentStore::new(&config).unwrap();

        store.insert(make_doc("Login", Utc::now())).unwrap();
        store.insert(make_doc("Login", Utc::now())).unwrap();
        store.insert(make_doc("GameOpened", Utc::now())).unwrap();
        store.flush().unwrap();

        let filter = ScanFilter::new().event_type("Login");
        let results = store.scan(&filter).unwrap();
        assert_eq!(results.len(), 2);

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_scan_by_time_range() {
        let dir = temp_dir();
        let config = make_config(dir.clone());
        let mut store = DocumentStore::new(&config).unwrap();

        let t1 = Utc.with_ymd_and_hms(2025, 6, 14, 10, 0, 0).unwrap();
        let t2 = Utc.with_ymd_and_hms(2025, 6, 14, 12, 0, 0).unwrap();
        let t3 = Utc.with_ymd_and_hms(2025, 6, 14, 14, 0, 0).unwrap();

        store.insert(make_doc("Login", t1)).unwrap();
        store.insert(make_doc("Login", t2)).unwrap();
        store.insert(make_doc("Login", t3)).unwrap();
        store.flush().unwrap();

        let start = Utc.with_ymd_and_hms(2025, 6, 14, 11, 0, 0).unwrap();
        let end = Utc.with_ymd_and_hms(2025, 6, 14, 13, 0, 0).unwrap();
        let filter = ScanFilter::time_range(start, end);
        let results = store.scan(&filter).unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].timestamp, t2);

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_scan_by_field() {
        let dir = temp_dir();
        let config = make_config(dir.clone());
        let mut store = DocumentStore::new(&config).unwrap();

        let mut doc1 = make_doc("Login", Utc::now());
        doc1.fields
            .insert("member".to_string(), FieldValue::Text("alice".to_string()));

        let mut doc2 = make_doc("Login", Utc::now());
        doc2.fields
            .insert("member".to_string(), FieldValue::Text("bob".to_string()));

        store.insert(doc1).unwrap();
        store.insert(doc2).unwrap();
        store.flush().unwrap();

        let filter = ScanFilter::new().field_eq("member", "alice");
        let results = store.scan(&filter).unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(
            results[0].fields.get("member").unwrap(),
            &FieldValue::Text("alice".to_string())
        );

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_flush_and_reload() {
        let dir = temp_dir();
        let config = make_config(dir.clone());

        let doc_id = {
            let mut store = DocumentStore::new(&config).unwrap();
            let doc = make_doc("Login", Utc::now());
            let doc_id = doc.id;
            store.insert(doc).unwrap();
            store.flush().unwrap();
            doc_id
        };

        // Reload the store
        let store = DocumentStore::new(&config).unwrap();
        let retrieved = store.get_by_id(&doc_id).unwrap();
        assert_eq!(retrieved.id, doc_id);

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_stats() {
        let dir = temp_dir();
        let config = make_config(dir.clone());
        let mut store = DocumentStore::new(&config).unwrap();

        store.insert(make_doc("Login", Utc::now())).unwrap();
        store.insert(make_doc("GameOpened", Utc::now())).unwrap();
        store.flush().unwrap();

        let stats = store.stats();
        assert_eq!(stats.document_count, 2);
        assert!(stats.segment_count > 0);
        assert!(stats.total_bytes > 0);

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_schema_registry_tracks_docs() {
        let dir = temp_dir();
        let config = make_config(dir.clone());
        let mut store = DocumentStore::new(&config).unwrap();

        let mut doc = make_doc("Login", Utc::now());
        doc.fields
            .insert("member".to_string(), FieldValue::Text("alice".to_string()));
        store.insert(doc).unwrap();

        let schema = store.schema_registry().get_schema("Login").unwrap();
        assert_eq!(schema.total_documents, 1);
        assert!(schema.fields.contains_key("member"));

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_multiple_segments() {
        let dir = temp_dir();
        let config = make_config(dir.clone());
        let mut store = DocumentStore::new(&config).unwrap();

        // Insert docs on different days
        let day1 = Utc.with_ymd_and_hms(2025, 6, 14, 12, 0, 0).unwrap();
        let day2 = Utc.with_ymd_and_hms(2025, 6, 15, 12, 0, 0).unwrap();

        store.insert(make_doc("Login", day1)).unwrap();
        store.insert(make_doc("Login", day2)).unwrap();
        store.flush().unwrap();

        let stats = store.stats();
        assert_eq!(stats.segment_count, 2);
        assert_eq!(stats.document_count, 2);

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_insert_updates_index() {
        let dir = temp_dir();
        let config = make_config(dir.clone());
        let mut store = DocumentStore::new(&config).unwrap();

        let doc = make_doc("Login", Utc::now());
        let doc_id = doc.id;
        let segment_id = SegmentManager::segment_id_for_timestamp(&doc.timestamp);

        store.insert(doc).unwrap();

        // Check that the index was updated
        let index = store.indexes.get(&segment_id).unwrap();
        assert!(index.get(&doc_id).is_some());

        fs::remove_dir_all(&dir).ok();
    }
}
