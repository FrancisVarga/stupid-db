# Rust Rules

- Query interface supports OpenAI, Claude, Gemini, and Ollama — never hardcode single provider
- Sample parquet files define entity extraction schema — analyze before changing entity model
- Segment storage supports 15-30 day rolling window — design for continuous eviction, not append-only
- Use Cargo workspace for 12 crates — never create monolithic single-crate architecture
- Use `pub(crate)` for cross-store shared helpers — never duplicate encryption/utility code
- Use three-tier type system for credentials: Config (internal), Safe (API/masked), Credentials (consumer)
- **Use rust-analyzer for validation, not `cargo check`** — faster IDE feedback and incremental compilation
- Place `rustc-wrapper` in `[build]` section of `.cargo/config.toml` -- never in `[env]`
- After changing `.cargo/config.toml` or build profiles, verify with `cargo build -vv` to confirm tools are active
- Expect sccache 0 hits with `incremental = true` -- sccache caches clean builds, not incremental ones
- Place test utilities (tempfile, mockall, criterion) in `[dev-dependencies]` — never as regular deps
- Use interior `Arc<RwLock<HashMap>>` in domain stores — AppState holds `Arc<Store>` to avoid double-locking
- Use JSONL for append-heavy data (telemetry), JSON for small bounded configs (groups)
- Use `#[serde(tag = "type")]` tagged union for polymorphic YAML configs — matches RuleDocument pattern
- Follow SessionStore file-based JSON pattern for new stores — proven template for simple persistence
