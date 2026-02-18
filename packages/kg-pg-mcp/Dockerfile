# ── Stage 1: Build ───────────────────────────────────────────
FROM ghcr.io/astral-sh/uv:python3.13-bookworm-slim AS builder

WORKDIR /app

# Layer 1: Install only dependencies (cached unless pyproject.toml changes)
# Extract deps and write to requirements.txt to avoid shell metachar issues (<, >)
COPY pyproject.toml README.md ./
RUN --mount=type=cache,target=/root/.cache/uv \
    python3 -c "import tomllib,pathlib;d=tomllib.loads(pathlib.Path('pyproject.toml').read_text());print('\n'.join(d['project']['dependencies']))" > /tmp/requirements.txt && \
    uv pip install --system --compile-bytecode -r /tmp/requirements.txt

# Layer 2: Copy source and install the project itself (no-deps — deps already cached)
COPY kg_pg_mcp/ ./kg_pg_mcp/
RUN --mount=type=cache,target=/root/.cache/uv \
    uv pip install --system --compile-bytecode --no-deps .

# Layer 3: Copy non-Python files needed at runtime
COPY alembic/ ./alembic/
COPY alembic.ini ./

# ── Stage 2: Runtime ────────────────────────────────────────
FROM python:3.13-slim-bookworm AS runtime

# Security: non-root user
RUN groupadd -r mcp && useradd -r -g mcp -d /app -s /sbin/nologin mcp

WORKDIR /app

# Copy installed packages from builder
COPY --from=builder /usr/local/lib/python3.13/site-packages /usr/local/lib/python3.13/site-packages
COPY --from=builder /usr/local/bin /usr/local/bin

# Copy application code
COPY --from=builder /app/kg_pg_mcp ./kg_pg_mcp
COPY --from=builder /app/alembic ./alembic
COPY --from=builder /app/alembic.ini ./

# Alembic needs write access to its directory for stamp operations
RUN chown -R mcp:mcp /app

USER mcp

# Default: SSE transport for Docker (stdio doesn't make sense in containers)
ENV KG_TRANSPORT=sse
ENV KG_HOST=0.0.0.0
ENV KG_PORT=12312

EXPOSE 12312

# Health check via HTTP — use HEAD to avoid SSE stream hang
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD python -c "import httpx; httpx.head('http://localhost:12312/sse', timeout=3)" || exit 1

# Run migrations then start the MCP server
CMD ["sh", "-c", "python -m alembic upgrade head && python -m kg_pg_mcp.server"]
