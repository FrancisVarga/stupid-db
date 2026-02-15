#!/usr/bin/env bash
set -euo pipefail

# stupid-db dev pipeline — build, import, and run everything
# Usage:
#   scripts/dev.sh                          # build (release) + start server + dashboard
#   scripts/dev.sh --watch                  # hot reload: auto-rebuild on save + dashboard
#   scripts/dev.sh --import "D:\w88_data"   # import folder first, then start
#   scripts/dev.sh --import-file "path.parquet" "seg-id"  # import single file
#   scripts/dev.sh --build-only             # just build, don't run

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SERVER="$ROOT/target/release/stupid-server.exe"
DEV_SERVER="$ROOT/target/debug/stupid-server.exe"
DASHBOARD="$ROOT/dashboard"

# Parse args
IMPORT_DIR=""
IMPORT_FILE=""
IMPORT_SEG=""
BUILD_ONLY=false
WATCH_MODE=false

while [[ $# -gt 0 ]]; do
    case "$1" in
        --import)
            IMPORT_DIR="$2"
            shift 2
            ;;
        --import-file)
            IMPORT_FILE="$2"
            IMPORT_SEG="$3"
            shift 3
            ;;
        --build-only)
            BUILD_ONLY=true
            shift
            ;;
        --watch)
            WATCH_MODE=true
            shift
            ;;
        *)
            echo "Unknown option: $1"
            echo "Usage: scripts/dev.sh [--watch] [--import <dir>] [--import-file <path> <segment_id>] [--build-only]"
            exit 1
            ;;
    esac
done

# ── Step 1: Install dashboard deps ────────────────────────────
if [ ! -d "$DASHBOARD/node_modules" ]; then
    echo "==> Installing dashboard dependencies..."
    cd "$DASHBOARD"
    npm install
    cd "$ROOT"
fi

if $BUILD_ONLY; then
    echo "==> Building Rust (release)..."
    cd "$ROOT"
    cargo build --release
    echo "    Done: $SERVER"
    echo "==> Build complete (--build-only)"
    exit 0
fi

# ── Step 2: Import data if requested ──────────────────────────
if [ -n "$IMPORT_DIR" ] || [ -n "$IMPORT_FILE" ]; then
    echo "==> Building Rust (release) for import..."
    cd "$ROOT"
    cargo build --release
    echo "    Done: $SERVER"

    if [ -n "$IMPORT_DIR" ]; then
        echo "==> Importing parquet files from $IMPORT_DIR..."
        "$SERVER" import-dir "$IMPORT_DIR"
    fi

    if [ -n "$IMPORT_FILE" ]; then
        echo "==> Importing $IMPORT_FILE as segment '$IMPORT_SEG'..."
        "$SERVER" import "$IMPORT_FILE" "$IMPORT_SEG"
    fi
fi

# ── Step 3: Start server + dashboard ─────────────────────────
cleanup() {
    echo ""
    echo "==> Shutting down..."
    [ -n "${SERVER_PID:-}" ] && kill "$SERVER_PID" 2>/dev/null || true
    [ -n "${DASHBOARD_PID:-}" ] && kill "$DASHBOARD_PID" 2>/dev/null || true
    wait 2>/dev/null
    echo "    Done."
}
trap cleanup EXIT INT TERM

if $WATCH_MODE; then
    # ── Watch mode: cargo-watch auto-rebuilds on .rs file changes ──
    echo "==> Starting in watch mode (auto-rebuild on save)..."
    echo "    Incremental builds take ~3s with rust-lld + optimized profile"
    echo ""

    cd "$ROOT"
    cargo watch -c -w crates/ --delay 1 -x 'run -- serve' &
    SERVER_PID=$!

    echo "==> Starting dashboard..."
    cd "$DASHBOARD"
    npm run dev &
    DASHBOARD_PID=$!

    echo ""
    echo "================================================"
    echo "  stupid-db is running (WATCH MODE)"
    echo "  API:       http://localhost:3001 (auto-rebuild)"
    echo "  Dashboard: http://localhost:3000 (hot reload)"
    echo "  Edit any .rs file to trigger rebuild (~3s)"
    echo "  Press Ctrl+C to stop"
    echo "================================================"
    echo ""
else
    # ── One-shot mode: build release, then run ────────────────────
    if [ -z "$IMPORT_DIR" ] && [ -z "$IMPORT_FILE" ]; then
        echo "==> Building Rust (release)..."
        cd "$ROOT"
        cargo build --release
        echo "    Done: $SERVER"
    fi

    echo "==> Starting server..."
    "$SERVER" serve &
    SERVER_PID=$!

    echo "==> Starting dashboard..."
    cd "$DASHBOARD"
    npm run dev &
    DASHBOARD_PID=$!

    echo ""
    echo "================================================"
    echo "  stupid-db is running!"
    echo "  API:       http://localhost:3001"
    echo "  Dashboard: http://localhost:3000"
    echo "  Press Ctrl+C to stop"
    echo "================================================"
    echo ""
fi

wait
