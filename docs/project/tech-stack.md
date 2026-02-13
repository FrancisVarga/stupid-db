# Tech Stack

## Backend (Rust)

| Component | Crate/Tool | Version | Purpose |
|-----------|-----------|---------|---------|
| **Runtime** | `tokio` | 1.x | Async runtime for I/O |
| **Parallelism** | `rayon` | 1.x | Data-parallel compute |
| **HTTP Server** | `axum` | 0.7+ | REST API + SSE + WebSocket |
| **Serialization** | `serde`, `rmp-serde` | 1.x | JSON + MessagePack |
| **Parquet** | `arrow-rs`, `parquet` | Latest | Parquet reading + Arrow interop |
| **Mmap** | `memmap2` | 0.9+ | Memory-mapped segment files |
| **HNSW** | `usearch` or `hnsw_rs` | Latest | Vector similarity index |
| **ONNX** | `ort` | 2.x | Local embedding model inference |
| **HTTP Client** | `reqwest` | 0.12+ | Ollama/OpenAI API calls, remote parquet |
| **Config** | `config` | 0.14+ | TOML configuration |
| **Logging** | `tracing` | 0.1+ | Structured logging |
| **Error** | `anyhow` / `thiserror` | Latest | Error handling |
| **CLI** | `clap` | 4.x | Command-line argument parsing |
| **File Watch** | `notify` | 6.x | File system watching for auto-ingest |
| **UUID** | `uuid` | 1.x | Document and session IDs |
| **Time** | `chrono` | 0.4+ | Timestamp handling |
| **S3** | `aws-sdk-s3` | Latest | S3 remote data access |

## Frontend (Next.js)

| Component | Package | Version | Purpose |
|-----------|---------|---------|---------|
| **Framework** | `next` | 14+ | React SSR + App Router |
| **Language** | TypeScript | 5.x | Type safety |
| **Visualization** | `d3` | 7.x | Data visualization |
| **Styling** | `tailwindcss` | 3.x | Utility-first CSS |
| **Icons** | `lucide-react` | Latest | Icon set |

## Development Tools

| Tool | Purpose |
|------|---------|
| `cargo-watch` | Auto-rebuild on file change |
| `cargo-nextest` | Fast parallel test runner |
| `just` | Command runner (like make) |

## Infrastructure (Development)

| Component | Tool | Notes |
|-----------|------|-------|
| **Embedding models** | ONNX files (downloaded) | `all-MiniLM-L6-v2` default |
| **LLM** | Ollama (local) or OpenAI (cloud) | Configurable |
| **Storage** | Local filesystem | Mmap'd segment files |

## Minimum System Requirements

| Resource | Development | Production |
|----------|------------|------------|
| **CPU** | 4 cores | 16+ cores |
| **RAM** | 8 GB | 64+ GB |
| **Disk** | 200 GB SSD | 5+ TB NVMe |
| **GPU** | Optional (ONNX CPU fine) | Optional (ONNX GPU for embedding) |
| **OS** | Windows/Linux/macOS | Linux preferred |

## Build & Run

```bash
# Backend
cargo build --release
./target/release/stupid-db --config config/default.toml

# Dashboard
cd dashboard
npm install
npm run dev

# Development (both)
just dev  # Starts both backend and dashboard
```
