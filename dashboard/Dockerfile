# Stage 1: Base image with bun
FROM oven/bun:1 AS base

WORKDIR /app

# Stage 2: Dependencies installation
FROM base AS deps

# Copy only dependency specification first (cache layer)
COPY package.json bun.lock ./

# Install dependencies using frozen lockfile for deterministic builds
RUN bun install --frozen-lockfile

# Stage 3: Builder - build the Next.js application
FROM base AS builder

WORKDIR /app

# Copy installed dependencies from deps stage
COPY --from=deps /app/node_modules ./node_modules

# Copy application source code
COPY . .

# Set Next.js to produce standalone output for minimal image
ENV NEXT_TELEMETRY_DISABLED=1

# Build the application
RUN bun run build

# Stage 4: Runtime - minimal final image
FROM oven/bun:1-slim AS runtime

WORKDIR /app

ENV NODE_ENV=production
ENV NEXT_TELEMETRY_DISABLED=1

# Create non-root user for security
RUN adduser --system --uid 1000 appuser

# Copy standalone server and static assets from builder
COPY --from=builder /app/.next/standalone ./
COPY --from=builder /app/.next/static ./.next/static
COPY --from=builder /app/public ./public

USER appuser

EXPOSE 39300

ENV PORT=39300
ENV HOSTNAME="0.0.0.0"

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD bun -e "fetch('http://localhost:39300').then(r => r.ok ? process.exit(0) : process.exit(1)).catch(() => process.exit(1))"

CMD ["bun", "server.js"]
