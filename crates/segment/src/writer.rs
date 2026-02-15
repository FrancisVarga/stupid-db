use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use stupid_core::{Document, SegmentId, StupidError};
use tracing::info;

/// Segment metadata stored as meta.json.
#[derive(serde::Serialize)]
struct SegmentMeta {
    segment_id: String,
    document_count: usize,
    size_bytes: u64,
    raw_bytes: u64,
    compression: String,
}

pub struct SegmentWriter {
    segment_id: SegmentId,
    segment_dir: PathBuf,
    encoder: zstd::Encoder<'static, std::io::BufWriter<fs::File>>,
    raw_bytes: u64,
    doc_count: usize,
}

impl SegmentWriter {
    pub fn new(data_dir: &Path, segment_id: &str) -> Result<Self, StupidError> {
        let segment_dir = data_dir.join("segments").join(segment_id);
        fs::create_dir_all(&segment_dir)?;

        let doc_path = segment_dir.join("documents.dat");
        let file = fs::File::create(&doc_path)?;
        let buf = std::io::BufWriter::new(file);
        let encoder = zstd::Encoder::new(buf, 3).map_err(StupidError::Io)?;

        Ok(Self {
            segment_id: segment_id.to_string(),
            segment_dir,
            encoder,
            raw_bytes: 0,
            doc_count: 0,
        })
    }

    /// Append a document to the zstd-compressed stream.
    pub fn append(&mut self, doc: &Document) -> Result<u64, StupidError> {
        let doc_offset = self.raw_bytes;

        let encoded =
            rmp_serde::to_vec(doc).map_err(|e| StupidError::Serialize(e.to_string()))?;

        let len = encoded.len() as u32;
        self.encoder.write_all(&len.to_le_bytes())?;
        self.encoder.write_all(&encoded)?;

        self.raw_bytes += 4 + encoded.len() as u64;
        self.doc_count += 1;
        Ok(doc_offset)
    }

    /// Finish zstd stream and write meta.json.
    pub fn finalize(self) -> Result<(), StupidError> {
        let buf_writer = self.encoder.finish().map_err(StupidError::Io)?;
        let mut inner = buf_writer.into_inner().map_err(|e| StupidError::Io(e.into_error()))?;
        inner.flush()?;

        // Get compressed file size
        let doc_path = self.segment_dir.join("documents.dat");
        let compressed_size = fs::metadata(&doc_path).map(|m| m.len()).unwrap_or(0);

        let ratio = if self.raw_bytes > 0 {
            (compressed_size as f64 / self.raw_bytes as f64 * 100.0) as u32
        } else {
            100
        };

        let meta = SegmentMeta {
            segment_id: self.segment_id.clone(),
            document_count: self.doc_count,
            size_bytes: compressed_size,
            raw_bytes: self.raw_bytes,
            compression: "zstd".to_string(),
        };

        let meta_path = self.segment_dir.join("meta.json");
        let meta_json = serde_json::to_string_pretty(&meta)
            .map_err(|e| StupidError::Serialize(e.to_string()))?;
        fs::write(&meta_path, meta_json)?;

        info!(
            "Segment {} finalized: {} docs, {} bytes ({}% of raw {})",
            self.segment_id, self.doc_count, compressed_size, ratio, self.raw_bytes
        );
        Ok(())
    }
}
