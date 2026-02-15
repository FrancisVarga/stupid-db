# Stage 1: Base image with system dependencies
FROM python:3.13-slim AS base

WORKDIR /app

# Install system dependencies and uv in one layer
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
        curl \
        git \
    && rm -rf /var/lib/apt/lists/* \
    && pip install --no-cache-dir uv

# Stage 2: Dependencies installation
FROM base AS deps

# Copy only dependency specification first (cache layer)
COPY pyproject.toml README.md ./

# Copy source code needed for package installation
COPY stupid_claude_agent/ ./stupid_claude_agent/

# Install dependencies using uv with caching
# Mount cache for uv to speed up subsequent builds
RUN --mount=type=cache,target=/root/.cache/uv \
    uv pip install --system .

# Stage 3: Builder - copy source code
FROM base AS builder

# Copy installed dependencies from deps stage
COPY --from=deps /usr/local/lib/python3.13/site-packages /usr/local/lib/python3.13/site-packages
COPY --from=deps /usr/local/bin /usr/local/bin

# Copy application source code
COPY main.py ./
COPY stupid_claude_agent/ ./stupid_claude_agent/
COPY .claude/ ./.claude/

# Stage 4: Runtime - minimal final image
FROM python:3.13-slim AS runtime

WORKDIR /app

# Install only runtime dependencies (curl for healthcheck)
RUN apt-get update && \
    apt-get install -y --no-install-recommends curl && \
    rm -rf /var/lib/apt/lists/*

# Copy Python packages from builder
COPY --from=builder /usr/local/lib/python3.13/site-packages /usr/local/lib/python3.13/site-packages
COPY --from=builder /usr/local/bin /usr/local/bin

# Copy application code from builder
COPY --from=builder /app /app

# Create non-root user for security
RUN useradd -m -u 1000 appuser && \
    chown -R appuser:appuser /app

USER appuser

# Expose FastAPI and MCP SSE ports
EXPOSE 39048 39049

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -f http://localhost:39048/api/health || exit 1

CMD ["python", "main.py"]
