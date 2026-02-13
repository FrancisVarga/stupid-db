# Remote Data Access

## Overview

Like DuckDB's `httpfs` extension, stupid-db can read parquet files from remote sources (S3, HTTP, Azure Blob) without downloading them entirely. This enables querying datasets that live in cloud storage or on other machines.

## Supported Protocols

| Protocol | URI Format | Auth |
|----------|-----------|------|
| **Local file** | `file:///D:/w88_data/Login/2025-06-12.parquet` | None |
| **HTTP/HTTPS** | `https://host/data/Login/2025-06-12.parquet` | Bearer token, Basic |
| **S3** | `s3://bucket/prefix/Login/2025-06-12.parquet` | AWS credentials |
| **Azure Blob** | `az://container/path/file.parquet` | SAS token, managed identity |
| **GCS** | `gs://bucket/path/file.parquet` | Service account |

## How It Works

### Parquet Remote Reading

Parquet files have a footer at the end containing schema and row group metadata. Remote reading exploits this:

```
1. HTTP Range request: last 8 bytes → magic number + footer length
2. HTTP Range request: footer → column metadata, row group offsets
3. For each needed column/row group:
   HTTP Range request: specific byte range → column chunk data
```

This means we only download the data we actually need. A 1GB parquet file might require only 10MB of network transfer if we're reading 2 columns.

### Implementation

```rust
trait RemoteReader: Send + Sync {
    /// Read a byte range from the remote source
    async fn read_range(&self, offset: u64, length: u64) -> Result<Bytes>;

    /// Get total file size
    async fn file_size(&self) -> Result<u64>;

    /// Check if the source exists
    async fn exists(&self) -> Result<bool>;
}

/// Wraps a RemoteReader to provide parquet reading
struct RemoteParquetReader<R: RemoteReader> {
    reader: R,
    metadata: Option<ParquetMetadata>,  // Cached after first read
}
```

### Caching

Remote data is cached at two levels:

1. **Metadata cache** — parquet footer/schema cached indefinitely (immutable files)
2. **Page cache** — recently read column chunks cached with LRU eviction

```rust
struct RemoteCache {
    metadata: HashMap<String, ParquetMetadata>,
    pages: LruCache<(String, RowGroupId, ColumnId), Bytes>,
    max_cache_size: usize,  // Default: 1GB
}
```

## External Segments

Remote data can be registered as **external segments** — they appear in the catalog alongside local segments but are read-only:

```rust
struct ExternalSegment {
    source_uri: String,
    event_type: String,
    time_range: (DateTime<Utc>, DateTime<Utc>),
    schema: Schema,
    // Not materialized — read on demand
    reader: Box<dyn RemoteReader>,
}
```

External segments:
- Are queryable via document scan (with predicate pushdown to parquet)
- Can be "materialized" (imported) into local segments for full processing
- Do NOT participate in vector/graph stores unless materialized
- Useful for ad-hoc analysis: "also look at this remote dataset"

## Registration

```rust
// Register a remote parquet directory as an external source
fn register_remote(uri: &str, event_type: &str, pattern: &str) -> Result<()>
```

Example:
```
register_remote("s3://analytics/events/", "Login", "Login/*.parquet")
```

This scans the remote path, discovers matching files, and registers each as an external segment.

## Predicate Pushdown

When scanning remote parquet, filters are pushed down to minimize data transfer:

```
scan(event_type: "Login", filter: { time > "2025-06-10", currency = "VND" })

→ Parquet statistics check: skip row groups where min(timestamp) > filter
→ Only read columns: memberCode, @timestamp, currency (not all 72)
→ Apply currency = "VND" filter at row group level if stats available
```

## Limitations

- Remote data is **read-only** — cannot append to remote parquet
- No vector/graph indexing unless materialized locally
- Network latency adds to scan time — best for batch analysis, not real-time
- Large scans over remote data are slower than local — consider materializing hot datasets
