---
name: Rust Patterns
description: Rust coding patterns and conventions for the stupid-db Cargo workspace. Error handling, async patterns, trait design, module organization, logging, and testing. Use when writing or reviewing Rust code.
version: 1.0.0
---

# Rust Patterns

## Error Handling

### Library Crates: `thiserror`
```rust
#[derive(Debug, thiserror::Error)]
pub enum SegmentError {
    #[error("segment {0} not found")]
    NotFound(String),
    #[error("segment full, rotation needed")]
    Full,
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialization error: {0}")]
    Serialize(#[from] rmp_serde::encode::Error),
}
```

### Binary Crate (server): `anyhow`
```rust
use anyhow::{Context, Result};

async fn main() -> Result<()> {
    let config = Config::load()
        .context("failed to load configuration")?;
    // ...
}
```

### Rules
- Never `unwrap()` in library code — use `?` or `expect("descriptive reason")`
- Each crate has its own error enum
- Use `#[from]` for automatic conversion from dependency errors
- Use `context()` / `with_context()` for adding context in anyhow

## Async Patterns

### I/O-Bound: Tokio
```rust
use tokio::fs;

async fn read_segment(path: &Path) -> Result<Vec<u8>> {
    let data = fs::read(path).await?;
    Ok(data)
}
```

### CPU-Bound: Rayon
```rust
use rayon::prelude::*;

fn compute_scores(items: &[Item]) -> Vec<f64> {
    items.par_iter()
        .map(|item| expensive_computation(item))
        .collect()
}
```

### Bridge: spawn_blocking
```rust
async fn run_algorithm(data: Vec<f64>) -> Result<Vec<Cluster>> {
    let result = tokio::task::spawn_blocking(move || {
        // CPU-intensive work with rayon
        kmeans::cluster(&data, k)
    }).await?;
    Ok(result)
}
```

### Shared State
```rust
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct AppState {
    pub graph: Arc<RwLock<GraphStore>>,
    pub segments: Arc<RwLock<SegmentManager>>,
    pub knowledge: Arc<RwLock<KnowledgeState>>,
}
```

## Trait Design

### Pluggable Backends
```rust
#[async_trait::async_trait]
pub trait Embedder: Send + Sync {
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>>;
    fn dimensions(&self) -> usize;
    fn provider_name(&self) -> &str;
}
```

### Compute Tasks
```rust
pub trait ComputeTask: Send + Sync {
    fn name(&self) -> &str;
    fn priority(&self) -> u32;
    fn should_run(&self, state: &KnowledgeState) -> bool;
    fn execute(&self, ctx: &ComputeContext) -> Result<()>;
}
```

## Logging with Tracing

```rust
use tracing::{info, warn, error, debug, instrument};

#[instrument(skip(data), fields(count = data.len()))]
pub fn ingest_batch(&self, data: &[Document]) -> Result<()> {
    info!("ingesting batch");

    for doc in data {
        debug!(doc_id = %doc.id, "processing document");
    }

    if data.is_empty() {
        warn!("empty batch received");
    }
}
```

Rules:
- Always `tracing`, never `println!` or `eprintln!`
- Use `#[instrument]` on public functions
- Skip large parameters with `skip(data)`
- Use structured fields: `info!(count = 42, "message")`

## Module Organization

```
crates/{name}/
├── Cargo.toml
├── src/
│   ├── lib.rs          # Public API, re-exports
│   ├── {feature}.rs    # One file per major feature
│   └── {subdir}/       # Subdirectory for related features
│       └── mod.rs
└── tests/              # Integration tests
    └── {feature}.rs
```

### lib.rs Pattern
```rust
pub mod writer;
pub mod reader;
pub mod index;

pub use writer::SegmentWriter;
pub use reader::SegmentReader;
pub use index::SegmentIndex;
```

## Workspace Dependencies

All shared deps in root `Cargo.toml`:
```toml
[workspace.dependencies]
serde = { version = "1", features = ["derive"] }
tokio = { version = "1", features = ["full"] }
```

Per-crate reference:
```toml
[dependencies]
serde = { workspace = true }
core = { path = "../core" }
```

## Testing Patterns

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_segment_rotation_on_time_boundary() {
        let dir = TempDir::new().unwrap();
        let writer = SegmentWriter::new(dir.path());
        // ...
    }

    #[tokio::test]
    async fn test_embedding_batch_processing() {
        let embedder = MockEmbedder::new(384);
        let result = embedder.embed(&["hello".into()]).await.unwrap();
        assert_eq!(result[0].len(), 384);
    }
}
```

Rules:
- Descriptive names: `test_segment_evicts_after_ttl`
- `tempdir` for filesystem tests
- Never modify D:\w88_data
- Mock external services (Ollama, OpenAI)
- `#[tokio::test]` for async tests
