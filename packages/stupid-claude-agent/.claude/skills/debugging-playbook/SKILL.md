---
name: Debugging Playbook
description: Debugging and troubleshooting guide for stupid-db. Common issues, diagnostic approaches, tracing analysis, and cross-crate debugging patterns. Use when debugging issues, investigating errors, or troubleshooting performance problems.
version: 1.0.0
---

# Debugging Playbook

## Diagnostic Approach

1. **Identify the layer**: Which crate/component is failing?
2. **Check logs**: Use tracing output to trace the error path
3. **Reproduce**: Find the minimal reproduction case
4. **Fix and test**: Write a regression test before fixing

## Common Issues by Layer

### Ingestion Layer

| Symptom | Likely Cause | Diagnosis |
|---------|-------------|-----------|
| "Parquet read failed" | Schema mismatch, corrupted file | Check arrow schema vs expected fields |
| "Entity extraction empty" | Missing expected fields | Log document fields, check extraction rules |
| "Embedding timeout" | Ollama/OpenAI API down | Check `EMBEDDING_PROVIDER`, test endpoint |
| Slow ingestion | Large batch + sync embedding | Profile hot-path, check embedding batch size |

**Key diagnostic**: Enable `RUST_LOG=ingest=debug` to see per-document processing.

### Storage Layer

| Symptom | Likely Cause | Diagnosis |
|---------|-------------|-----------|
| "Segment not found" | Evicted or not yet sealed | Check segment age vs retention window |
| "mmap failed" | File deleted or permissions | Check file existence, OS limits |
| High memory usage | Too many mmap'd segments | Count open segments, check eviction |
| Corrupt segment read | Incomplete seal, zstd error | Check segment footer, try re-reading |

**Key diagnostic**: `RUST_LOG=segment=debug` for segment lifecycle events.

### Graph Layer

| Symptom | Likely Cause | Diagnosis |
|---------|-------------|-----------|
| Graph empty after restart | In-memory only, no persistence | Graph rebuilds from segments on startup |
| Missing edges after eviction | Expected — edges pruned with segments | Check segment_id on edges |
| PageRank not converging | Disconnected components, bad damping | Check graph connectivity, lower damping |
| Louvain unstable | Resolution too high, small graph | Lower resolution, check min community size |

**Key diagnostic**: `RUST_LOG=graph=debug` for node/edge operations.

### Compute Layer

| Symptom | Likely Cause | Diagnosis |
|---------|-------------|-----------|
| Scheduler not running tasks | No tasks registered, all completed | Check task list, should_run conditions |
| KMeans poor clusters | Bad k, not enough data | Try different k, check silhouette score |
| Anomaly false positives | Baseline too short, threshold too low | Extend baseline window, raise threshold |
| High CPU from compute | Algorithm on large dataset | Profile with `perf`, check rayon thread count |

**Key diagnostic**: `RUST_LOG=compute=debug` for scheduler decisions and task execution.

### Query Layer

| Symptom | Likely Cause | Diagnosis |
|---------|-------------|-----------|
| "Invalid query plan" | LLM generated bad plan | Log raw LLM output, check catalog summary |
| Slow query execution | Large DocumentScan, no filter | Check plan steps, add filters |
| Empty results | Wrong field names, expired data | Validate against catalog, check time range |
| LLM timeout | Provider API slow | Check LLM_PROVIDER, test endpoint |

**Key diagnostic**: `RUST_LOG=query=debug,llm=debug` for plan generation and execution.

### Dashboard Layer

| Symptom | Likely Cause | Diagnosis |
|---------|-------------|-----------|
| Blank visualization | Empty data, D3 error | Check browser console, verify data prop |
| SSE not streaming | CORS, wrong URL, backend down | Check network tab, CORS headers |
| WebSocket disconnect | Backend restart, network | Check reconnect logic, server logs |
| Type error | Render spec mismatch | Check RenderBlockView type mapping |

**Key diagnostic**: Browser DevTools console + Network tab.

## Tracing Configuration

### Log Levels
```bash
# Minimal — errors only
RUST_LOG=error

# Normal operation
RUST_LOG=info

# Debug specific crate
RUST_LOG=info,compute=debug

# Full debug (verbose)
RUST_LOG=debug

# Trace everything (very verbose)
RUST_LOG=trace
```

### Structured Log Fields
```rust
#[instrument(skip(data), fields(batch_size = data.len(), segment_id = %segment.id))]
pub fn process_batch(&self, data: &[Document], segment: &Segment) -> Result<()> {
    info!("processing batch");
    // Logs: INFO process_batch{batch_size=1000 segment_id=seg-2024-01-15-12}: processing batch
}
```

### Finding Log Patterns
```bash
# Find all errors
grep "ERROR" server.log

# Trace a specific request
grep "request_id=abc-123" server.log

# Find slow operations (>1s)
grep "elapsed.*[0-9]\{4,\}ms" server.log
```

## Cross-Crate Debugging

When an issue spans multiple crates:

1. **Start at the API layer** (server) — what request triggered it?
2. **Follow the data flow** — ingest → connect → store → compute → query
3. **Check interfaces** — are the types correct at crate boundaries?
4. **Enable multi-crate debug**: `RUST_LOG=server=debug,query=debug,compute=debug`

### Common Cross-Crate Issues
- **Ingest → Graph**: Entity extraction produces wrong NodeIds → graph has orphan edges
- **Compute → Catalog**: Algorithm results not published to catalog → LLM can't reference them
- **Query → Vector**: Vector search returns segment-evicted document IDs → stale results
- **Server → Query**: SSE stream closes before all results sent → incomplete response

## Build Issues

| Error | Fix |
|-------|-----|
| `cargo build` fails with dependency conflict | Check `[workspace.dependencies]` versions |
| `cannot find module` in server | Ensure crate is in workspace members |
| Linker errors on Windows | Check MSVC toolchain, vcpkg dependencies |
| ONNX runtime not found | Set `ORT_DYLIB_PATH` or install onnxruntime |

## Performance Debugging

```bash
# Profile with cargo flamegraph
cargo flamegraph --bin stupid-db

# Check memory usage
# Windows: Task Manager or Process Explorer
# Track with tracing: log memory at key points

# Benchmark specific function
cargo bench --bench my_benchmark
```

See `performance-guide` skill for detailed optimization strategies.
