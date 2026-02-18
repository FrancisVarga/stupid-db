# ── Stille Post Worker Dockerfile ──────────────────────────────────
# Multi-stage build for the AI agent runtime (Node.js + TypeScript).
#
# Usage:
#   docker build -t stupid-db/stille-post-worker ./services/stille-post-worker

# ── Stage 1: Dependencies ─────────────────────────────────────────
FROM node:20-slim AS deps

WORKDIR /app
COPY package.json package-lock.json* ./
RUN npm ci --omit=dev

# ── Stage 2: Builder ──────────────────────────────────────────────
FROM node:20-slim AS builder

WORKDIR /app
COPY package.json package-lock.json* ./
RUN npm ci
COPY tsconfig.json ./
COPY src/ ./src/
RUN npx tsc

# ── Stage 3: Runtime ──────────────────────────────────────────────
FROM node:20-slim AS runtime

RUN useradd --system --uid 1000 --create-home appuser

WORKDIR /app

# Production deps only
COPY --from=deps /app/node_modules ./node_modules
COPY --from=builder /app/dist ./dist
COPY --from=builder /app/package.json ./

USER appuser

ENV NODE_ENV=production
ENV PORT=4100
EXPOSE 4100

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD node -e "fetch('http://localhost:4100/health').then(r => r.ok ? process.exit(0) : process.exit(1)).catch(() => process.exit(1))"

CMD ["node", "dist/index.js"]
