# Rust Development Rules

## Build & Tooling
- Use `cargo nextest run` for tests — never `cargo test` (90% faster with parallel process isolation)
- Use rust-analyzer for validation — never `cargo check` (faster incremental feedback)
- Place `rustc-wrapper` in `[build]` section of `.cargo/config.toml` — never in `[env]`
- After changing `.cargo/config.toml` or build profiles, verify with `cargo build -vv`
- Expect sccache 0 hits with `incremental = true` — sccache caches clean builds only

## Architecture
- 18-crate Cargo workspace — never create monolithic single-crate architecture
- Dependency flow: core → rules → {compute, connector, ingest} → server (no cycles)
- Use `pub(crate)` for cross-crate shared helpers — never duplicate utility code

## Error Handling
- Libraries (`thiserror`): Define typed errors with `#[derive(Error)]`
- Binaries/servers (`anyhow`): Use `anyhow::Result` for top-level error propagation
- Never use `unwrap()` in library code — always propagate errors
- Use `?` operator for error propagation, not manual match

## Logging
- Always use `tracing` crate — never `println!` or `eprintln!`
- Use structured fields: `tracing::info!(segment_id, doc_count, "loaded segment")`
- Span-based context for request tracing

## Async Patterns
- `tokio` runtime for I/O-bound work (network, file, timers)
- `rayon` thread pool for CPU-bound work (compute algorithms, clustering)
- Use `async_trait` for async trait methods
- Prefer `tokio::sync::mpsc` for streaming channels

## Type System
- Three-tier credentials: Config (internal) → Safe (API/masked) → Credentials (consumer)
- Never expose raw credentials in API responses
- Use `serde` with `#[serde(rename_all = "snake_case")]` for consistent JSON
- Prefer newtype wrappers for domain IDs (SegmentId, EntityId)

## Testing
- Unit tests in `#[cfg(test)]` modules within source files
- Integration tests in `crates/{name}/tests/` directory
- Use `#[tokio::test]` for async test functions
- Property-based testing with `proptest` where applicable
- Test YAML rules with actual rule files from `data/rules/`

## Module Organization
- One public type per file for major types
- Group related small types in a single file
- Re-export key types from `lib.rs`
- Use `mod.rs` only for directory modules

## LLM Provider Pattern
- Never hardcode a single provider — always use trait abstraction
- `LlmProvider` trait for basic completion
- `ToolAwareLlmProvider` for tool-calling support
- `SimpleLlmProvider` → `LlmProviderBridge` for non-streaming → streaming adapter
