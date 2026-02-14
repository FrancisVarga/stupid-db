---
name: Rust Conventions
description: Rust-specific coding patterns and conventions for the stupid-db Cargo workspace. Use when writing or reviewing Rust code.
version: 1.0.0
---

# Rust Conventions

## Cargo Workspace Structure

```toml
# Root Cargo.toml
[workspace]
members = ["crates/*"]

[workspace.package]
version = "0.1.0"
edition = "2021"

[workspace.dependencies]
# All shared dependencies defined here with versions
serde = { version = "1", features = ["derive"] }
tokio = { version = "1", features = ["full"] }
```

Each crate references workspace dependencies:
```toml
[dependencies]
serde = { workspace = true }
```

## Error Handling

- Library crates: `thiserror` with domain-specific error enums
- Binary crate (server): `anyhow` for top-level error propagation
- Never use `unwrap()` in library code (use `?` or `expect()` with message)

```rust
// Library crate pattern
#[derive(Debug, thiserror::Error)]
pub enum SegmentError {
    #[error("segment {0} not found")]
    NotFound(String),
    #[error("segment full, rotation needed")]
    Full,
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}
```

## Logging

Always use `tracing`, never `println!` or `eprintln!`:

```rust
use tracing::{info, warn, error, debug, instrument};

#[instrument(skip(data))]
pub fn ingest_batch(&self, data: &[Document]) -> Result<()> {
    info!(count = data.len(), "ingesting batch");
    // ...
}
```

## Async Patterns

- Use `tokio` for I/O-bound work (network, file system)
- Use `rayon` for CPU-bound work (algorithms, batch processing)
- Bridge with `tokio::task::spawn_blocking` for rayon in async context

```rust
let result = tokio::task::spawn_blocking(move || {
    // CPU-intensive work with rayon
    data.par_iter().map(|d| process(d)).collect()
}).await?;
```

## Trait Design

Pluggable components use traits with async methods:

```rust
#[async_trait::async_trait]
pub trait Embedder: Send + Sync {
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>>;
    fn dimensions(&self) -> usize;
}
```

## Testing

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_segment_rotation_on_time_boundary() {
        // Descriptive test names
    }

    #[tokio::test]
    async fn test_api_endpoint_returns_200() {
        // Async test with tokio
    }
}
```

## Module Organization

Each crate follows:
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
