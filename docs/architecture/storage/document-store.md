# Document Store

## Overview

The document store is the **primary write interface** for stupid-db. All data enters through document insertion. It stores raw event data in a schema-flexible format within time-partitioned segments.

## Storage Format

Documents are stored as length-prefixed MessagePack (or BSON) blobs in a flat file per segment.

```
┌─────────┬──────────────┬─────────┬──────────────┬─────┐
│ len (4B)│ doc_0 bytes  │ len (4B)│ doc_1 bytes  │ ... │
└─────────┴──────────────┴─────────┴──────────────┴─────┘
```

### Why Not Parquet Internally?

- Parquet is columnar — great for analytics reads, terrible for append writes
- We ingest row-at-a-time (or small batches), need fast appends
- Parquet is used as the **input format** (external data), not internal storage
- Internal format optimizes for: fast append, mmap read, schema flexibility

### Why MessagePack?

- Compact binary format (30-50% smaller than JSON)
- Schema-flexible (each document can have different fields)
- Fast serialization/deserialization
- Well-supported in Rust (`rmp-serde`)
- Zero-copy reads possible with careful layout

## Document Model

```rust
/// A document is a flat key-value map with a mandatory timestamp
/// and event type. All values are stored as typed enum variants.
struct Document {
    /// Globally unique within a segment
    id: DocId,
    /// ISO 8601 timestamp, used for segment assignment
    timestamp: DateTime<Utc>,
    /// Event type: "Login", "GameOpened", "API Error", etc.
    event_type: String,
    /// All fields from the source event
    fields: HashMap<String, FieldValue>,
}

enum FieldValue {
    Text(String),
    Integer(i64),
    Float(f64),
    Boolean(bool),
    Null,
}

/// Address of a document in storage
struct DocAddress {
    segment_id: SegmentId,
    offset: u64,
}
```

## Index

Each segment maintains a lightweight document index for fast lookups:

```
documents.idx:
  doc_id → (offset, length, timestamp, event_type)
```

This index is small enough to fit in memory for all active segments. For a segment with 1M documents, the index is approximately 40MB.

## Operations

### Insert

```
insert(doc: Document) → DocAddress
```

1. Serialize document to MessagePack
2. Append to active segment's document file
3. Record offset in segment index
4. Return `(segment_id, offset)`

**Latency target**: < 10 microseconds per document (excluding fsync).

### Scan

```
scan(filter: ScanFilter) → Iterator<Document>
```

1. Determine which segments overlap the time range
2. For each segment, iterate documents matching the filter
3. Filters applied: time range, event type, field predicates

Scans are sequential reads through mmap'd files — cache-friendly and fast.

### Get

```
get(address: DocAddress) → Document
```

1. Mmap the target segment file (cached)
2. Seek to offset
3. Deserialize document

**Latency target**: < 1 microsecond for cached mmap reads.

## Schema Registry

Although documents are schema-flexible, the system maintains a **schema registry** that tracks observed fields per event type:

```json
{
  "Login": {
    "fields": {
      "memberCode": { "type": "text", "seen_count": 57359, "null_rate": 0.0 },
      "success": { "type": "text", "seen_count": 57359, "null_rate": 0.02 },
      "platform": { "type": "text", "seen_count": 57359, "null_rate": 0.0 },
      "currency": { "type": "text", "seen_count": 57359, "null_rate": 0.01 }
    },
    "total_documents": 57359
  }
}
```

This registry is used by:
- The LLM query layer to understand available fields
- The entity extractor to know which fields map to entities
- The dashboard to show schema information

## Parquet Import

Since source data arrives as parquet files:

```
import_parquet(path: &Path, event_type: &str) → ImportResult
```

1. Read parquet file using Arrow reader
2. Iterate RecordBatches
3. Convert each row to a Document
4. Insert into appropriate segment (based on `@timestamp`)
5. Return count of documents imported

Supports:
- Local files: `D:/w88_data/Login/2025-06-12.parquet`
- Remote files: `s3://bucket/Login/2025-06-12.parquet`
- HTTP files: `https://host/data/Login/2025-06-12.parquet`
