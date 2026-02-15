use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use chrono::{DateTime, NaiveDate, Utc};
use stupid_core::config::StorageConfig;
use stupid_core::{SegmentId, StupidError};
use tracing::{info, warn};

use crate::reader::SegmentReader;
use crate::writer::SegmentWriter;

/// Manages the lifecycle of time-partitioned segments: creation, sealing,
/// reading, and TTL-based eviction.
pub struct SegmentManager {
    data_dir: PathBuf,
    retention_days: u32,
    /// Active writers keyed by segment ID (date string).
    writers: HashMap<SegmentId, SegmentWriter>,
    /// Sealed segment readers keyed by segment ID.
    readers: HashMap<SegmentId, SegmentReader>,
}

impl SegmentManager {
    /// Create a new SegmentManager, scanning `data_dir/segments/` for existing
    /// sealed segments (directories containing `documents.dat`) and opening a
    /// reader for each.
    pub fn new(config: &StorageConfig) -> Result<Self, StupidError> {
        let segments_dir = config.data_dir.join("segments");
        fs::create_dir_all(&segments_dir)?;

        let mut readers = HashMap::new();

        let entries = fs::read_dir(&segments_dir)?;
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            let doc_path = path.join("documents.dat");
            if !doc_path.exists() {
                continue;
            }

            let segment_id = match path.file_name().and_then(|n| n.to_str()) {
                Some(name) => name.to_string(),
                None => continue,
            };

            match SegmentReader::open(&config.data_dir, &segment_id) {
                Ok(reader) => {
                    info!(segment_id = %segment_id, "Opened existing segment reader");
                    readers.insert(segment_id, reader);
                }
                Err(e) => {
                    warn!(segment_id = %segment_id, error = %e, "Failed to open segment, skipping");
                }
            }
        }

        info!(
            segment_count = readers.len(),
            "SegmentManager initialized with existing segments"
        );

        Ok(Self {
            data_dir: config.data_dir.clone(),
            retention_days: config.segment_retention_days,
            writers: HashMap::new(),
            readers,
        })
    }

    /// Derive the segment ID for a given timestamp (formatted as "YYYY-MM-DD").
    pub fn segment_id_for_timestamp(ts: &DateTime<Utc>) -> SegmentId {
        ts.format("%Y-%m-%d").to_string()
    }

    /// Return a mutable reference to the writer for the given segment ID,
    /// creating a new writer if one does not already exist.
    pub fn get_or_create_writer(
        &mut self,
        segment_id: &str,
    ) -> Result<&mut SegmentWriter, StupidError> {
        if !self.writers.contains_key(segment_id) {
            let writer = SegmentWriter::new(&self.data_dir, segment_id)?;
            info!(segment_id = %segment_id, "Created new segment writer");
            self.writers.insert(segment_id.to_string(), writer);
        }
        Ok(self.writers.get_mut(segment_id).expect("just inserted"))
    }

    /// Seal an active writer: finalize the compressed stream, write metadata,
    /// then open the segment as a reader.
    pub fn seal_segment(&mut self, segment_id: &str) -> Result<(), StupidError> {
        let writer = self
            .writers
            .remove(segment_id)
            .ok_or_else(|| StupidError::SegmentNotFound(segment_id.to_string()))?;

        writer.finalize()?;

        let reader = SegmentReader::open(&self.data_dir, segment_id)?;
        self.readers.insert(segment_id.to_string(), reader);

        info!(segment_id = %segment_id, "Segment sealed and opened as reader");
        Ok(())
    }

    /// Evict segments whose date is older than `retention_days` from today.
    /// Removes from the readers map and deletes the segment directory on disk.
    /// Returns the list of evicted segment IDs.
    pub fn evict_expired(&mut self) -> Result<Vec<SegmentId>, StupidError> {
        let today = Utc::now().date_naive();
        let mut evicted = Vec::new();

        let expired_ids: Vec<SegmentId> = self
            .readers
            .keys()
            .filter(|sid| {
                if let Ok(date) = NaiveDate::parse_from_str(sid, "%Y-%m-%d") {
                    let age = today.signed_duration_since(date).num_days();
                    age > self.retention_days as i64
                } else {
                    false
                }
            })
            .cloned()
            .collect();

        for sid in expired_ids {
            self.readers.remove(&sid);
            let seg_dir = self.data_dir.join("segments").join(&sid);
            if seg_dir.exists() {
                fs::remove_dir_all(&seg_dir)?;
                info!(segment_id = %sid, "Evicted expired segment");
            }
            evicted.push(sid);
        }

        Ok(evicted)
    }

    /// List all known segment IDs (both active writers and sealed readers), sorted.
    pub fn list_segments(&self) -> Vec<SegmentId> {
        let mut ids: Vec<SegmentId> = self
            .writers
            .keys()
            .chain(self.readers.keys())
            .cloned()
            .collect();
        ids.sort();
        ids.dedup();
        ids
    }

    /// Look up a sealed segment reader by ID.
    pub fn get_reader(&self, segment_id: &str) -> Option<&SegmentReader> {
        self.readers.get(segment_id)
    }

    /// Return segment IDs that overlap with the given time range.
    /// Both `start` and `end` are optional (unbounded if None).
    pub fn segments_in_range(
        &self,
        start: Option<DateTime<Utc>>,
        end: Option<DateTime<Utc>>,
    ) -> Vec<SegmentId> {
        let start_date = start.map(|dt| dt.date_naive());
        let end_date = end.map(|dt| dt.date_naive());

        let mut result: Vec<SegmentId> = self
            .list_segments()
            .into_iter()
            .filter(|sid| {
                if let Ok(date) = NaiveDate::parse_from_str(sid, "%Y-%m-%d") {
                    let after_start = start_date.is_none_or(|s| date >= s);
                    let before_end = end_date.is_none_or(|e| date <= e);
                    after_start && before_end
                } else {
                    false
                }
            })
            .collect();
        result.sort();
        result
    }

    /// Seal all active writers: drain the writers map, finalize each, and open
    /// as readers.
    pub fn flush_all(&mut self) -> Result<(), StupidError> {
        let writers: HashMap<SegmentId, SegmentWriter> = self.writers.drain().collect();
        for (sid, writer) in writers {
            writer.finalize()?;
            let reader = SegmentReader::open(&self.data_dir, &sid)?;
            self.readers.insert(sid, reader);
        }
        info!("All active writers flushed and sealed");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use stupid_core::{Document, FieldValue};
    use uuid::Uuid;

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("stupid-segment-test-{}", Uuid::new_v4()));
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

    fn make_doc(event_type: &str) -> Document {
        Document {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            event_type: event_type.to_string(),
            fields: {
                let mut m = std::collections::HashMap::new();
                m.insert("test".to_string(), FieldValue::Text("value".to_string()));
                m
            },
        }
    }

    #[test]
    fn test_new_creates_segments_dir() {
        let dir = temp_dir();
        let config = make_config(dir.clone());
        let _mgr = SegmentManager::new(&config).unwrap();
        assert!(dir.join("segments").is_dir());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_segment_id_for_timestamp() {
        let ts = Utc.with_ymd_and_hms(2025, 6, 14, 12, 30, 0).unwrap();
        assert_eq!(SegmentManager::segment_id_for_timestamp(&ts), "2025-06-14");
    }

    #[test]
    fn test_write_seal_read_cycle() {
        let dir = temp_dir();
        let config = make_config(dir.clone());
        let mut mgr = SegmentManager::new(&config).unwrap();

        let sid = "2025-06-14";
        let doc = make_doc("Login");

        // Write a document
        let writer = mgr.get_or_create_writer(sid).unwrap();
        let offset = writer.append(&doc).unwrap();
        assert_eq!(offset, 0);

        // Seal the segment
        mgr.seal_segment(sid).unwrap();

        // Writer should be gone, reader should exist
        assert!(mgr.writers.is_empty());
        assert!(mgr.get_reader(sid).is_some());

        // Read back the document
        let reader = mgr.get_reader(sid).unwrap();
        let read_doc = reader.read_at(0).unwrap();
        assert_eq!(read_doc.event_type, "Login");
        assert_eq!(read_doc.id, doc.id);

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_list_segments_sorted() {
        let dir = temp_dir();
        let config = make_config(dir.clone());
        let mut mgr = SegmentManager::new(&config).unwrap();

        // Create writers in non-sorted order
        mgr.get_or_create_writer("2025-06-16").unwrap();
        mgr.get_or_create_writer("2025-06-14").unwrap();
        mgr.get_or_create_writer("2025-06-15").unwrap();

        let segments = mgr.list_segments();
        assert_eq!(segments, vec!["2025-06-14", "2025-06-15", "2025-06-16"]);

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_segments_in_range() {
        let dir = temp_dir();
        let config = make_config(dir.clone());
        let mut mgr = SegmentManager::new(&config).unwrap();

        mgr.get_or_create_writer("2025-06-13").unwrap();
        mgr.get_or_create_writer("2025-06-14").unwrap();
        mgr.get_or_create_writer("2025-06-15").unwrap();
        mgr.get_or_create_writer("2025-06-16").unwrap();

        let start = Utc.with_ymd_and_hms(2025, 6, 14, 0, 0, 0).unwrap();
        let end = Utc.with_ymd_and_hms(2025, 6, 15, 23, 59, 59).unwrap();
        let result = mgr.segments_in_range(Some(start), Some(end));
        assert_eq!(result, vec!["2025-06-14", "2025-06-15"]);

        // Unbounded start
        let result = mgr.segments_in_range(None, Some(end));
        assert_eq!(result, vec!["2025-06-13", "2025-06-14", "2025-06-15"]);

        // Unbounded end
        let result = mgr.segments_in_range(Some(start), None);
        assert_eq!(result, vec!["2025-06-14", "2025-06-15", "2025-06-16"]);

        // Fully unbounded
        let result = mgr.segments_in_range(None, None);
        assert_eq!(
            result,
            vec!["2025-06-13", "2025-06-14", "2025-06-15", "2025-06-16"]
        );

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_flush_all() {
        let dir = temp_dir();
        let config = make_config(dir.clone());
        let mut mgr = SegmentManager::new(&config).unwrap();

        let doc = make_doc("GameOpened");

        mgr.get_or_create_writer("2025-06-14")
            .unwrap()
            .append(&doc)
            .unwrap();
        mgr.get_or_create_writer("2025-06-15")
            .unwrap()
            .append(&doc)
            .unwrap();

        assert_eq!(mgr.writers.len(), 2);
        assert_eq!(mgr.readers.len(), 0);

        mgr.flush_all().unwrap();

        assert_eq!(mgr.writers.len(), 0);
        assert_eq!(mgr.readers.len(), 2);

        // Verify both are readable
        assert!(mgr.get_reader("2025-06-14").is_some());
        assert!(mgr.get_reader("2025-06-15").is_some());

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_evict_expired() {
        let dir = temp_dir();
        let config = StorageConfig {
            data_dir: dir.clone(),
            segment_retention_days: 5,
            cache_dir: dir.join("cache"),
            cache_max_gb: 1,
        };
        let mut mgr = SegmentManager::new(&config).unwrap();

        // Create an old segment (far in the past) and a recent one (today)
        let today = Utc::now().date_naive();
        let old_date = today - chrono::Duration::days(10);
        let old_sid = old_date.format("%Y-%m-%d").to_string();
        let today_sid = today.format("%Y-%m-%d").to_string();

        let doc = make_doc("Login");

        mgr.get_or_create_writer(&old_sid)
            .unwrap()
            .append(&doc)
            .unwrap();
        mgr.get_or_create_writer(&today_sid)
            .unwrap()
            .append(&doc)
            .unwrap();

        mgr.flush_all().unwrap();
        assert_eq!(mgr.readers.len(), 2);

        let evicted = mgr.evict_expired().unwrap();
        assert_eq!(evicted, vec![old_sid.clone()]);
        assert!(mgr.get_reader(&old_sid).is_none());
        assert!(mgr.get_reader(&today_sid).is_some());

        // Directory should be gone
        assert!(!dir.join("segments").join(&old_sid).exists());

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_new_discovers_existing_segments() {
        let dir = temp_dir();
        let config = make_config(dir.clone());
        let mut mgr = SegmentManager::new(&config).unwrap();

        let doc = make_doc("Login");
        mgr.get_or_create_writer("2025-06-14")
            .unwrap()
            .append(&doc)
            .unwrap();
        mgr.flush_all().unwrap();
        drop(mgr);

        // Reconstruct -- should discover the sealed segment
        let mgr2 = SegmentManager::new(&config).unwrap();
        assert!(mgr2.get_reader("2025-06-14").is_some());
        assert_eq!(mgr2.list_segments(), vec!["2025-06-14"]);

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_seal_nonexistent_segment_errors() {
        let dir = temp_dir();
        let config = make_config(dir.clone());
        let mut mgr = SegmentManager::new(&config).unwrap();

        let result = mgr.seal_segment("2025-01-01");
        assert!(result.is_err());

        fs::remove_dir_all(&dir).ok();
    }
}
