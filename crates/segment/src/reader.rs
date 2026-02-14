use std::path::Path;

use memmap2::Mmap;
use stupid_core::{Document, SegmentId, StupidError};

#[derive(Debug, Clone, Copy, PartialEq)]
enum Compression {
    None,
    Zstd,
}

pub struct SegmentReader {
    segment_id: SegmentId,
    /// Decompressed data (either raw mmap or zstd-decompressed buffer).
    data: SegmentData,
}

enum SegmentData {
    Raw(Mmap),
    Decompressed(Vec<u8>),
}

impl SegmentData {
    fn as_slice(&self) -> &[u8] {
        match self {
            SegmentData::Raw(mmap) => mmap,
            SegmentData::Decompressed(buf) => buf,
        }
    }
}

impl SegmentReader {
    pub fn open(data_dir: &Path, segment_id: &str) -> Result<Self, StupidError> {
        let seg_dir = data_dir.join("segments").join(segment_id);
        let doc_path = seg_dir.join("documents.dat");

        if !doc_path.exists() {
            return Err(StupidError::SegmentNotFound(segment_id.to_string()));
        }

        let compression = Self::detect_compression(&seg_dir);

        let file = std::fs::File::open(&doc_path)?;
        let mmap = unsafe { Mmap::map(&file)? };

        let data = match compression {
            Compression::None => SegmentData::Raw(mmap),
            Compression::Zstd => {
                let decompressed = zstd::decode_all(mmap.as_ref())
                    .map_err(StupidError::Io)?;
                SegmentData::Decompressed(decompressed)
            }
        };

        Ok(Self {
            segment_id: segment_id.to_string(),
            data,
        })
    }

    fn detect_compression(seg_dir: &Path) -> Compression {
        let meta_path = seg_dir.join("meta.json");
        if let Ok(content) = std::fs::read_to_string(&meta_path) {
            if let Ok(meta) = serde_json::from_str::<serde_json::Value>(&content) {
                if meta.get("compression").and_then(|v| v.as_str()) == Some("zstd") {
                    return Compression::Zstd;
                }
            }
        }
        Compression::None
    }

    pub fn segment_id(&self) -> &str {
        &self.segment_id
    }

    /// Read a single document at the given byte offset.
    pub fn read_at(&self, offset: u64) -> Result<Document, StupidError> {
        let data = self.data.as_slice();
        let off = offset as usize;
        if off + 4 > data.len() {
            return Err(StupidError::DocumentNotFound(offset));
        }

        let len =
            u32::from_le_bytes(data[off..off + 4].try_into().unwrap()) as usize;

        if off + 4 + len > data.len() {
            return Err(StupidError::DocumentNotFound(offset));
        }

        rmp_serde::from_slice(&data[off + 4..off + 4 + len])
            .map_err(|e| StupidError::Serialize(e.to_string()))
    }

    /// Iterate all documents in the segment.
    pub fn iter(&self) -> SegmentIter<'_> {
        SegmentIter {
            data: self.data.as_slice(),
            pos: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.data.as_slice().len()
    }
}

pub struct SegmentIter<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Iterator for SegmentIter<'a> {
    type Item = Result<Document, StupidError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.pos + 4 > self.data.len() {
            return None;
        }

        let len = u32::from_le_bytes(
            self.data[self.pos..self.pos + 4].try_into().unwrap(),
        ) as usize;

        if self.pos + 4 + len > self.data.len() {
            return None;
        }

        let result: Result<Document, _> =
            rmp_serde::from_slice(&self.data[self.pos + 4..self.pos + 4 + len])
                .map_err(|e| StupidError::Serialize(e.to_string()));

        self.pos += 4 + len;
        Some(result)
    }
}
