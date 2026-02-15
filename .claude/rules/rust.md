# Rust Rules

- Query interface supports OpenAI, Claude, and Ollama — never hardcode single provider
- Sample parquet files define entity extraction schema — analyze before changing entity model
- Segment storage supports 15-30 day rolling window — design for continuous eviction, not append-only
- Use Cargo workspace for 12 crates — never create monolithic single-crate architecture
- Use `pub(crate)` for cross-store shared helpers — never duplicate encryption/utility code
- Use three-tier type system for credentials: Config (internal), Safe (API/masked), Credentials (consumer)
