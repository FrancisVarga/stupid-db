---
name: cargo-check
description: Run full Rust workspace validation — clippy lints and nextest — and summarize results
user-invocable: true
---

# Cargo Check — Full Workspace Validation

Run clippy and nextest across the entire stupid-db Rust workspace. Summarize pass/fail status for each step.

## Steps

1. **Clippy** — Run `cargo clippy --workspace --all-targets -- -D warnings` to catch lint issues
2. **Nextest** — Run `cargo nextest run --workspace` for all tests with parallel per-process isolation
3. **Summary** — Report pass/fail counts, list any failing tests or clippy warnings

## Rules

- Use `cargo nextest run` — never plain `cargo test`
- If clippy fails, list the specific warnings with file:line locations
- If tests fail, show the test name and failure reason (first 10 lines of output)
- Do NOT auto-fix anything — this skill is diagnostic only
- Run clippy and nextest sequentially (clippy first, to catch compile errors early)
