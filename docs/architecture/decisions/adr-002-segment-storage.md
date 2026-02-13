# ADR-002: Time-Partitioned Segment Storage

## Status
**Accepted**

## Context

Data has a 15-30 day retention window. The system needs efficient eviction, fast sequential reads, and bounded memory per time window. Traditional approaches include LSM trees, B-trees, and append-only logs.

## Decision

Use **time-partitioned segments** as the fundamental storage unit. Each segment represents one day of data (configurable). Eviction is done by dropping entire segments.

## Rationale

### O(1) Eviction
- Delete the segment file — done
- No tombstones, no compaction, no GC
- Contrast with LSM: eviction requires rewriting SSTables
- Contrast with B-tree: eviction requires scanning and deleting individual records

### Natural Fit for Rolling Window
- Data arrives chronologically
- Queries often filter by time range
- Segments map directly to the retention window
- 30 segments for 30 days — simple mental model

### Isolation Benefits
- Each segment has its own vector index — no cross-contamination
- Compute can process segments independently in parallel
- A corrupted segment doesn't affect others
- Memory budget per segment is predictable

### Alternatives Considered

| Approach | Rejected Because |
|----------|-----------------|
| **LSM Tree (RocksDB)** | Compaction overhead, complex eviction, overkill for append-mostly workload |
| **Single append log** | Eviction requires rewriting, unbounded file growth |
| **SQLite per day** | SQL overhead unnecessary, not optimized for mmap scan patterns |
| **Just keep parquet** | Write performance is terrible for streaming inserts |

## Consequences

- Segment boundary = potential data skew (some days have more data)
- Cross-segment queries require merging results from multiple segments
- Segment rotation adds a small operational concern (midnight boundary)
- Vector index per segment means cross-segment ANN search requires top-k merge
