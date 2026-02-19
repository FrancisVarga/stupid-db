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

#[cfg(test)]
mod tests;

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
