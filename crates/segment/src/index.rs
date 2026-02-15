use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use stupid_core::{DocId, StupidError};

/// Metadata for a single document within a segment's data file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocIndexEntry {
    /// Byte offset of the document in the segment data file.
    pub offset: u64,
    /// Length of the serialized document in bytes (excluding the 4-byte length prefix).
    pub length: u32,
    /// Timestamp of the document.
    pub timestamp: DateTime<Utc>,
    /// Event type string (e.g. "Login", "GameOpened").
    pub event_type: String,
}

/// In-memory index mapping document IDs to their location and metadata within a segment.
///
/// Persisted as `documents.idx` alongside the segment data file using
/// length-prefixed msgpack encoding: each entry is a `(DocId, DocIndexEntry)`
/// tuple preceded by a u32 little-endian byte length.
pub struct DocIndex {
    entries: HashMap<DocId, DocIndexEntry>,
}

impl DocIndex {
    /// Create a new empty document index.
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// Insert or overwrite an entry for the given document ID.
    pub fn add(&mut self, doc_id: DocId, entry: DocIndexEntry) {
        self.entries.insert(doc_id, entry);
    }

    /// Look up a document's index entry by ID.
    pub fn get(&self, doc_id: &DocId) -> Option<&DocIndexEntry> {
        self.entries.get(doc_id)
    }

    /// Return the number of indexed documents.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Return true if the index contains no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Iterate over all `(DocId, DocIndexEntry)` pairs.
    pub fn iter(&self) -> impl Iterator<Item = (&DocId, &DocIndexEntry)> {
        self.entries.iter()
    }

    /// Serialize the index to a file as length-prefixed msgpack tuples.
    ///
    /// Each entry is written as:
    ///   - 4 bytes: u32 little-endian length of the msgpack payload
    ///   - N bytes: msgpack-encoded `(DocId, DocIndexEntry)`
    pub fn save(&self, path: &Path) -> Result<(), StupidError> {
        let mut file = std::io::BufWriter::new(fs::File::create(path)?);

        for (doc_id, entry) in &self.entries {
            let tuple: (&DocId, &DocIndexEntry) = (doc_id, entry);
            let encoded =
                rmp_serde::to_vec(&tuple).map_err(|e| StupidError::Serialize(e.to_string()))?;

            let len = encoded.len() as u32;
            file.write_all(&len.to_le_bytes())?;
            file.write_all(&encoded)?;
        }

        file.flush()?;
        Ok(())
    }

    /// Load an index from a length-prefixed msgpack file.
    ///
    /// If the file does not exist, returns an empty index.
    pub fn load(path: &Path) -> Result<Self, StupidError> {
        if !path.exists() {
            return Ok(Self::new());
        }

        let data = fs::read(path)?;
        let mut index = Self::new();
        let mut pos = 0;

        while pos + 4 <= data.len() {
            let len =
                u32::from_le_bytes(data[pos..pos + 4].try_into().unwrap()) as usize;
            pos += 4;

            if pos + len > data.len() {
                return Err(StupidError::Serialize(
                    "truncated index entry".to_string(),
                ));
            }

            let (doc_id, entry): (DocId, DocIndexEntry) =
                rmp_serde::from_slice(&data[pos..pos + len])
                    .map_err(|e| StupidError::Serialize(e.to_string()))?;

            index.entries.insert(doc_id, entry);
            pos += len;
        }

        Ok(index)
    }
}

impl Default for DocIndex {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use uuid::Uuid;

    fn sample_entry(offset: u64, event_type: &str) -> DocIndexEntry {
        DocIndexEntry {
            offset,
            length: 256,
            timestamp: Utc.with_ymd_and_hms(2025, 6, 14, 12, 0, 0).unwrap(),
            event_type: event_type.to_string(),
        }
    }

    #[test]
    fn new_index_is_empty() {
        let index = DocIndex::new();
        assert!(index.is_empty());
        assert_eq!(index.len(), 0);
    }

    #[test]
    fn add_and_get() {
        let mut index = DocIndex::new();
        let id = Uuid::new_v4();
        let entry = sample_entry(0, "Login");

        index.add(id, entry);
        assert_eq!(index.len(), 1);
        assert!(!index.is_empty());

        let found = index.get(&id).expect("entry should exist");
        assert_eq!(found.offset, 0);
        assert_eq!(found.event_type, "Login");
    }

    #[test]
    fn get_missing_returns_none() {
        let index = DocIndex::new();
        let id = Uuid::new_v4();
        assert!(index.get(&id).is_none());
    }

    #[test]
    fn iter_returns_all_entries() {
        let mut index = DocIndex::new();
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();

        index.add(id1, sample_entry(0, "Login"));
        index.add(id2, sample_entry(100, "GameOpened"));

        let collected: HashMap<DocId, &DocIndexEntry> = index.iter().map(|(k, v)| (*k, v)).collect();
        assert_eq!(collected.len(), 2);
        assert_eq!(collected[&id1].offset, 0);
        assert_eq!(collected[&id2].offset, 100);
    }

    #[test]
    fn save_and_load_roundtrip() {
        let dir = std::env::temp_dir().join("stupid_db_test_index_roundtrip");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("documents.idx");

        let mut index = DocIndex::new();
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        let id3 = Uuid::new_v4();

        index.add(id1, sample_entry(0, "Login"));
        index.add(id2, sample_entry(260, "GameOpened"));
        index.add(id3, sample_entry(520, "APIError"));

        index.save(&path).expect("save should succeed");

        let loaded = DocIndex::load(&path).expect("load should succeed");
        assert_eq!(loaded.len(), 3);

        let e1 = loaded.get(&id1).expect("id1 should exist");
        assert_eq!(e1.offset, 0);
        assert_eq!(e1.length, 256);
        assert_eq!(e1.event_type, "Login");

        let e2 = loaded.get(&id2).expect("id2 should exist");
        assert_eq!(e2.offset, 260);
        assert_eq!(e2.event_type, "GameOpened");

        let e3 = loaded.get(&id3).expect("id3 should exist");
        assert_eq!(e3.offset, 520);
        assert_eq!(e3.event_type, "APIError");

        // Cleanup
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn load_missing_file_returns_empty() {
        let path = std::env::temp_dir().join("stupid_db_nonexistent.idx");
        let index = DocIndex::load(&path).expect("should return empty index");
        assert!(index.is_empty());
    }

    #[test]
    fn add_overwrites_existing() {
        let mut index = DocIndex::new();
        let id = Uuid::new_v4();

        index.add(id, sample_entry(0, "Login"));
        index.add(id, sample_entry(100, "GameOpened"));

        assert_eq!(index.len(), 1);
        let entry = index.get(&id).unwrap();
        assert_eq!(entry.offset, 100);
        assert_eq!(entry.event_type, "GameOpened");
    }

    #[test]
    fn default_is_empty() {
        let index = DocIndex::default();
        assert!(index.is_empty());
    }
}
