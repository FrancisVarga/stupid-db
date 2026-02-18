---
name: storage-engine
description: Storage abstractions including segment model, S3/local backends, and caching
---

# Storage Engine

## Overview

The storage layer provides a unified interface for local filesystem and S3 backends with segment-based data organization and optional caching.

## StorageEngine

```rust
pub struct StorageEngine {
    pub backend: StorageBackend,    // Local or S3
    pub cache: Option<SegmentCache>, // For S3 backend
    pub data_dir: PathBuf,
}
```

Created from config — automatically selects backend based on AWS configuration.

## Backends

| Backend | When | Cache |
|---------|------|-------|
| LocalBackend | No AWS config | None needed |
| S3Backend | AWS configured | SegmentCache required |

## Segment Model

Documents are stored in time-partitioned segments:

```
data/segments/
├── 2026-02-15/
│   ├── documents.dat    # MessagePack-encoded documents (mmap'd)
│   ├── vector.idx       # HNSW vector index
│   └── metadata.json    # Segment metadata
├── 2026-02-16/
│   └── ...
```

### Segment Lifecycle
1. **Active** — Currently receiving writes via SegmentWriter
2. **Sealed** — Read-only, fully indexed, serving queries
3. **Archived** — Cold storage (S3), fetched on demand
4. **Evicted** — Dropped (O(1) directory removal)

### Rolling Window
- 15-30 day TTL based on configuration
- Eviction is O(1) — drop segment directory
- Never design for append-only — always support continuous eviction

## SegmentWriter / SegmentReader

```rust
// Writing
let writer = SegmentWriter::new(data_dir, segment_id)?;
writer.append(&document)?;
writer.seal()?;  // Finalize, build indexes

// Reading
let reader = SegmentReader::open(data_dir, segment_id)?;
let docs = reader.scan(predicate)?;  // Predicate push-down
```

## S3 Integration

- Discover segments by listing `segments/` prefix
- Cache segments locally for read access
- SegmentCache manages LRU eviction based on `cache_max_gb`

## Key Files

- `crates/storage/src/lib.rs` — StorageEngine, discover_local_segments
- `crates/storage/src/backend.rs` — LocalBackend, S3Backend, StorageBackend enum
- `crates/storage/src/cache.rs` — SegmentCache with LRU
- `crates/segment/src/` — SegmentWriter, SegmentReader, mmap operations
