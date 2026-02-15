---
name: Segment Storage
description: Time-partitioned segment storage patterns for stupid-db. Covers mmap-backed segments, rotation, eviction, TTL, MessagePack format, and the segment lifecycle. Use when working on storage, segment management, or data retention.
version: 1.0.0
---

# Segment Storage

## Segment Lifecycle

```
Active (writing) → Sealed (read-only, mmap) → Archived (optional) → Evicted (deleted)
```

### Active Segment
- Receives writes via `SegmentWriter`
- One active segment at a time per event type (or global)
- Backed by append-only file with MessagePack + zstd compression
- Maintains in-memory offset index (doc_id → file offset)

### Sealed Segment
- Triggered by time boundary (default: 1 hour) or size threshold
- Writer flushes, closes file, writes final index
- Reader opens with `memmap2` for zero-copy access
- Index is loaded into memory for O(1) doc lookup

### Eviction
- TTL-based: segments older than retention window (15-30 days) deleted
- O(1) operation: delete segment file + remove per-segment vector index + prune graph edges
- Critical design point: **nothing is append-only, everything expires**

## Storage Format

### MessagePack + zstd
```
[segment file]
┌──────────────────────────┐
│ Header (magic + version) │
├──────────────────────────┤
│ Document 1 (msgpack)     │
│ Document 2 (msgpack)     │
│ ...                      │
│ Document N (msgpack)     │
├──────────────────────────┤
│ Index (doc_id → offset)  │
├──────────────────────────┤
│ Footer (index offset)    │
└──────────────────────────┘
```

- Each document serialized with `rmp-serde`
- Optional zstd compression per block
- Index stored at end of file for single-pass writing

### Memory Mapping (memmap2)
```rust
use memmap2::MmapOptions;

let file = File::open(segment_path)?;
let mmap = unsafe { MmapOptions::new().map(&file)? };
// mmap[offset..offset+len] gives zero-copy access to document bytes
```

**Why mmap**: Sealed segments are read-only and potentially larger than RAM. mmap lets the OS manage page caching efficiently.

## Key Components

### SegmentWriter
```rust
pub struct SegmentWriter {
    file: BufWriter<File>,
    index: HashMap<DocId, u64>, // doc_id → file offset
    doc_count: usize,
    created_at: DateTime<Utc>,
}

impl SegmentWriter {
    pub fn write_document(&mut self, doc: &Document) -> Result<()>;
    pub fn seal(self) -> Result<SealedSegment>;
    pub fn should_rotate(&self, config: &SegmentConfig) -> bool;
}
```

### SegmentReader
```rust
pub struct SegmentReader {
    mmap: Mmap,
    index: HashMap<DocId, u64>,
    time_range: (DateTime<Utc>, DateTime<Utc>),
}

impl SegmentReader {
    pub fn get(&self, doc_id: &DocId) -> Result<Option<Document>>;
    pub fn scan(&self, filter: &Filter) -> Result<Vec<Document>>;
    pub fn scan_range(&self, start: DateTime<Utc>, end: DateTime<Utc>) -> Result<Vec<Document>>;
}
```

### SegmentRotator
```rust
pub struct SegmentRotator {
    config: SegmentConfig, // max_age, max_size
}

impl SegmentRotator {
    pub fn should_rotate(&self, writer: &SegmentWriter) -> bool;
    pub fn rotate(&self, writer: SegmentWriter) -> Result<(SealedSegment, SegmentWriter)>;
}
```

### SegmentEvictor
```rust
pub struct SegmentEvictor {
    retention: Duration, // 15-30 days
}

impl SegmentEvictor {
    pub fn evict_expired(&self, segments: &mut Vec<SealedSegment>) -> Vec<SegmentId>;
    // Returns IDs of evicted segments for vector index + graph cleanup
}
```

## Cross-Store Eviction

When a segment is evicted, cleanup cascades:
1. **Document store**: Delete segment file
2. **Vector index**: Remove per-segment HNSW instance
3. **Graph store**: Remove edges that reference evicted segment's documents

This is why edges in the graph carry segment metadata — to enable efficient pruning.

## Configuration

```rust
pub struct SegmentConfig {
    pub max_segment_age: Duration,     // default: 1 hour
    pub max_segment_size: usize,       // default: 100MB
    pub retention_days: u32,           // default: 30
    pub compression: CompressionType,  // zstd level
    pub data_dir: PathBuf,
}
```

## Performance Characteristics

| Operation | Complexity | Notes |
|-----------|-----------|-------|
| Write document | O(1) amortized | Append to file |
| Read by doc_id | O(1) | Index lookup + mmap read |
| Scan with filter | O(n) per segment | Full scan, filter in memory |
| Rotation | O(1) | Close file, open new |
| Eviction | O(1) per segment | Delete file |
| Time range scan | O(segments) | Check time range, scan matching |

## Important: Read Sequential, Not Parallel

Recent architectural decision: segment reading uses **sequential** iteration instead of parallel to **limit memory usage**. When scanning across many segments, parallel reads can cause memory spikes from concurrent mmap faults.
