# ── Stage 1: Chef base ────────────────────────────────────────
# Install cargo-chef on the Rust build image.
FROM rust:1-slim-bookworm AS chef

RUN cargo install cargo-chef --locked
WORKDIR /app

# ── Stage 2: Planner ─────────────────────────────────────────
# Analyze workspace and generate a dependency recipe.
FROM chef AS planner

COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# ── Stage 3: Builder ─────────────────────────────────────────
# Build dependencies from recipe (cached), then compile source.
FROM chef AS builder

# Build dependencies first (this layer is cached until Cargo.toml changes)
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json

# Now copy actual source and build only our code
COPY . .
RUN cargo build --release -p stupid-server

# ── Stage 4: Runtime ─────────────────────────────────────────
# Minimal runtime image with just the binary.
FROM debian:bookworm-slim AS runtime

# curl for healthcheck, ca-certificates for HTTPS (AWS, LLM APIs)
RUN apt-get update && apt-get install -y --no-install-recommends \
    curl \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Non-root user for security
RUN useradd --system --uid 1000 --create-home appuser

WORKDIR /app

# Copy the compiled binary
COPY --from=builder /app/target/release/stupid-server ./stupid-server

# Create data directory (will be mounted as volume)
RUN mkdir -p /data/rules && chown -R appuser:appuser /data

# Copy default anomaly rules (can be overridden via bind mount)
COPY --chown=appuser:appuser data/rules/ /data/rules/

USER appuser

ENV PORT=39100
ENV HOST=0.0.0.0
ENV DATA_DIR=/data
ENV RUST_LOG=info

EXPOSE 39100

HEALTHCHECK --interval=30s --timeout=5s --start-period=30s --retries=3 \
    CMD curl -f http://localhost:${PORT}/health || exit 1

CMD ["./stupid-server", "serve"]
