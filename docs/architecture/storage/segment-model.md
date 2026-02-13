# Segment Model

## Overview

stupid-db uses **time-partitioned segments** as the fundamental storage unit. Each segment represents a fixed time window (default: 1 day) and contains all documents ingested during that period. This model is inspired by time-series databases (InfluxDB, Prometheus) and provides O(1) eviction.

## Segment Structure

```
segments/
├── 2025-06-12/
│   ├── meta.json           # Segment metadata
│   ├── documents.dat       # Mmap'd document store
│   ├── documents.idx       # Document offset index
│   ├── vectors.hnsw        # HNSW vector index for this segment
│   ├── vectors.dat         # Raw embedding vectors
│   └── entities.idx        # Entity → doc_id mapping for this segment
├── 2025-06-13/
│   └── ...
└── current -> 2025-06-14/  # Symlink to active write segment
```

## Segment Metadata

```json
{
  "segment_id": "2025-06-12",
  "time_range": {
    "start": "2025-06-12T00:00:00Z",
    "end": "2025-06-12T23:59:59Z"
  },
  "status": "sealed",
  "document_count": 958207,
  "entity_count": 142893,
  "vector_count": 958207,
  "size_bytes": 3489726464,
  "created_at": "2025-06-12T00:00:01Z",
  "sealed_at": "2025-06-13T00:00:01Z"
}
```

## Segment Lifecycle

```
┌──────────┐     ┌──────────┐     ┌──────────┐     ┌──────────┐
│  Active   │────→│  Sealed  │────→│ Expiring │────→│ Dropped  │
│ (writing) │     │ (read)   │     │ (cleanup)│     │ (gone)   │
└──────────┘     └──────────┘     └──────────┘     └──────────┘
  1 day            28 days          < 1 hour          -
```

### Active
- Currently receiving writes
- Only one active segment at a time
- Documents appended sequentially
- HNSW index built incrementally

### Sealed
- No more writes accepted
- HNSW index finalized and optimized
- Fully available for reads, compute, and queries
- Mmap'd for zero-copy reads

### Expiring
- Past retention window
- Graph edges referencing this segment are removed
- Vector index removed from search pool
- Segment file scheduled for deletion

### Dropped
- Segment file deleted from disk
- All references cleaned up
- Space reclaimed

## Addressing

Every document has a globally unique address:

```
(segment_id, doc_offset) → Document
```

- `segment_id`: Date string like "2025-06-12"
- `doc_offset`: Byte offset into the segment's document file

Graph edges reference documents as `segment_id:doc_offset`. When a segment is evicted, all edges with that segment_id prefix are batch-removed.

## Segment Rotation

- At midnight UTC (configurable), the current active segment is sealed
- A new active segment is created for the next day
- Sealing triggers HNSW index optimization (background task)
- If the active segment exceeds a size limit before midnight, it is split

## Configuration

| Parameter | Default | Description |
|-----------|---------|-------------|
| `segment.duration` | `1d` | Time window per segment |
| `segment.retention` | `30d` | How long segments are kept |
| `segment.max_size` | `10GB` | Max size before forced rotation |
| `segment.directory` | `./data/segments` | Storage directory |
| `segment.mmap_populate` | `true` | Prefault mmap pages on open |

## Why Segments?

1. **O(1) eviction** — drop the file, done. No compaction, no tombstones.
2. **Isolation** — each segment has its own vector index, reducing interference
3. **Parallelism** — compute can process segments independently
4. **Simplicity** — no LSM trees, no WAL, no complex merge strategies
5. **Predictable memory** — each segment's memory footprint is bounded
