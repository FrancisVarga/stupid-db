# Vector Index

## Overview

Every document ingested into stupid-db receives a vector embedding. These embeddings power semantic similarity search, clustering algorithms, and anomaly detection. The vector index is organized per-segment, with cross-segment search handled by a merge layer.

## Index Structure

### Per-Segment HNSW

Each sealed segment has its own HNSW (Hierarchical Navigable Small World) index:

```
segment/2025-06-12/
├── vectors.dat        # Raw float32 vectors, contiguous
├── vectors.hnsw       # HNSW graph structure
└── vectors.meta       # Vector ID → doc_offset mapping
```

### Why Per-Segment?

1. **Eviction is free** — delete the segment, the index disappears
2. **Build parallelism** — each segment's index is built independently
3. **Bounded memory** — each index has a predictable size
4. **Incremental** — active segment builds HNSW incrementally; sealed segments are optimized

### HNSW Parameters

| Parameter | Value | Rationale |
|-----------|-------|-----------|
| `M` (edges per node) | 16 | Good balance of recall vs memory |
| `ef_construction` | 200 | High quality build, acceptable speed |
| `ef_search` | 100 | Good recall for top-k queries |
| Dimensions | 384 or 768 | Depends on embedding model |
| Distance metric | Cosine | Standard for text embeddings |

## Embedding Models

Pluggable via trait:

```rust
trait Embedder: Send + Sync {
    /// Generate embedding for a text input
    fn embed(&self, text: &str) -> Result<Vec<f32>>;

    /// Batch embed for throughput
    fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>>;

    /// Dimensionality of output vectors
    fn dimensions(&self) -> usize;

    /// Model identifier for index compatibility
    fn model_id(&self) -> &str;
}
```

### Supported Backends

| Backend | Model | Dimensions | Speed | Use Case |
|---------|-------|------------|-------|----------|
| **ONNX Local** | all-MiniLM-L6-v2 | 384 | ~1ms/doc | Default, no network |
| **ONNX Local** | bge-small-en-v1.5 | 384 | ~1ms/doc | Better quality |
| **Ollama** | nomic-embed-text | 768 | ~5ms/doc | Local, higher quality |
| **OpenAI** | text-embedding-3-small | 1536 | ~20ms/doc | Highest quality, costs $ |
| **OpenAI** | text-embedding-3-large | 3072 | ~30ms/doc | Maximum quality |

Default: **ONNX local** for hot-path speed. Can be configured to use remote models for higher quality at the cost of throughput.

## Text Representation

Before embedding, a document is converted to a text representation by concatenating key fields:

```
Login event: "Login memberCode:thongtran2904 platform:web-android
              currency:VND rGroup:VIPB success:true method:username"

GameOpened event: "GameOpened memberCode:vominhsang543 game:retrow88
                   category:Slots provider:GPI platform:web-android currency:VND"
```

The concatenation template is configured per event type. Only semantically meaningful fields are included (skip fingerprints, raw IDs, timestamps).

## Cross-Segment Search

When searching across all active segments:

```rust
fn search(query: &[f32], top_k: usize) -> Vec<SearchResult> {
    let mut results = BinaryHeap::new();

    // Search each segment's index in parallel
    for segment in active_segments.par_iter() {
        let segment_results = segment.hnsw.search(query, top_k);
        for result in segment_results {
            results.push(result);
        }
    }

    // Merge and return global top-k
    results.into_sorted_vec().truncate(top_k)
}
```

Each segment search returns its local top-k, then results are merged globally. This is a standard technique used by distributed vector databases.

## Memory Budget

For 30 days of data at ~960K events/day:

```
Total vectors: ~28.8M
Dimensions: 384 (MiniLM)
Bytes per vector: 384 * 4 = 1,536 bytes
Raw vector storage: 28.8M * 1,536 = ~44 GB
HNSW overhead (~1.5x): ~66 GB total

With quantization (int8): ~22 GB total
```

### Quantization Options

| Method | Memory Reduction | Recall Loss |
|--------|-----------------|-------------|
| **None (float32)** | Baseline | None |
| **Scalar Quantization (int8)** | 4x | < 1% |
| **Product Quantization (PQ)** | 10-20x | 2-5% |

Recommendation: Start with float32, add scalar quantization when memory becomes a constraint.

## Operations

### Insert (Hot Path)

```
insert(doc_id: DocId, embedding: Vec<f32>) → ()
```

Added to active segment's HNSW index. Incremental insertion — ~100 microseconds per vector.

### Search

```
search(query: Vec<f32>, top_k: usize, filter: Option<Filter>) → Vec<(DocId, f32)>
```

Returns document IDs with similarity scores. Optional filter for time range, event type, etc. Filter is applied post-search (pre-filter is possible but complex).

### Batch Nearest Neighbors

```
batch_knn(vectors: &[Vec<f32>], k: usize) → Vec<Vec<(DocId, f32)>>
```

Used by clustering algorithms. Parallelized across segments.
