#!/usr/bin/env bash
set -euo pipefail

# stupid-db dev pipeline — build, import, and run everything
# Usage:
#   scripts/dev.sh                          # build (debug) + start server + dashboard
#   scripts/dev.sh --release                # build release + start server + dashboard
#   scripts/dev.sh --eisenbahn              # start with eisenbahn ZMQ broker + workers
#   scripts/dev.sh --watch                  # hot reload: auto-rebuild on save + dashboard
#   scripts/dev.sh --import <dir>           # import folder first, then start
#   scripts/dev.sh --import-file <path> <seg-id>  # import single file
#   scripts/dev.sh --build-only             # just build, don't run

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DASHBOARD="$ROOT/dashboard"

# ── Load .env if present ────────────────────────────────────────
if [ -f "$ROOT/.env" ]; then
    set -a
    # shellcheck disable=SC1091
    source "$ROOT/.env"
    set +a
fi

# Resolve ports from env (with defaults matching core/config.rs)
SERVER_PORT="${PORT:-3001}"
DASHBOARD_PORT="19283"

# ── Parse args ──────────────────────────────────────────────────
IMPORT_DIR=""
IMPORT_FILE=""
IMPORT_SEG=""
BUILD_ONLY=false
WATCH_MODE=false
EISENBAHN=false
RELEASE=false

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
        --eisenbahn)
            EISENBAHN=true
            shift
            ;;
        --release)
            RELEASE=true
            shift
            ;;
        *)
            echo "Unknown option: $1"
            echo "Usage: scripts/dev.sh [--release] [--eisenbahn] [--watch] [--import <dir>] [--import-file <path> <segment_id>] [--build-only]"
            exit 1
            ;;
    esac
done

# ── Resolve binary path ────────────────────────────────────────
if $RELEASE; then
    BUILD_PROFILE="release"
    SERVER_BIN="$ROOT/target/release/stupid-server"
else
    BUILD_PROFILE="dev"
    SERVER_BIN="$ROOT/target/debug/stupid-server"
fi

# Append .exe on Windows/MSYS
if [[ "$(uname -s)" =~ MINGW|MSYS|CYGWIN ]]; then
    SERVER_BIN="${SERVER_BIN}.exe"
fi

# ── Step 1: Install dashboard deps ─────────────────────────────
if [ ! -d "$DASHBOARD/node_modules" ]; then
    echo "==> Installing dashboard dependencies..."
    (cd "$DASHBOARD" && npm install)
fi

# ── Build ───────────────────────────────────────────────────────
build_rust() {
    local args=("build")
    if $RELEASE; then
        args+=("--release")
    fi
    echo "==> Building Rust ($BUILD_PROFILE)..."
    (cd "$ROOT" && cargo "${args[@]}")
    echo "    Done: $SERVER_BIN"
}

if $BUILD_ONLY; then
    build_rust
    echo "==> Build complete (--build-only)"
    exit 0
fi

# ── Step 2: Import data if requested ───────────────────────────
if [ -n "$IMPORT_DIR" ] || [ -n "$IMPORT_FILE" ]; then
    build_rust

    if [ -n "$IMPORT_DIR" ]; then
        echo "==> Importing parquet files from $IMPORT_DIR..."
        "$SERVER_BIN" import-dir "$IMPORT_DIR"
    fi

    if [ -n "$IMPORT_FILE" ]; then
        echo "==> Importing $IMPORT_FILE as segment '$IMPORT_SEG'..."
        "$SERVER_BIN" import "$IMPORT_FILE" "$IMPORT_SEG"
    fi
fi

# ── Step 3: Cleanup handler ────────────────────────────────────
PIDS=()
CLEANED_UP=false

cleanup() {
    if $CLEANED_UP; then return; fi
    CLEANED_UP=true

    echo ""
    echo "==> Shutting down..."

    # Send SIGTERM to entire process groups (kills children of children)
    for pid in "${PIDS[@]}"; do
        kill -- -"$pid" 2>/dev/null || kill "$pid" 2>/dev/null || true
    done

    # Give processes a moment to exit gracefully
    local timeout=5
    for ((i = 0; i < timeout; i++)); do
        local still_alive=false
        for pid in "${PIDS[@]}"; do
            if kill -0 "$pid" 2>/dev/null; then
                still_alive=true
                break
            fi
        done
        if ! $still_alive; then break; fi
        sleep 1
    done

    # Force-kill anything still lingering
    for pid in "${PIDS[@]}"; do
        if kill -0 "$pid" 2>/dev/null; then
            echo "    Force-killing PID $pid..."
            kill -9 -- -"$pid" 2>/dev/null || kill -9 "$pid" 2>/dev/null || true
        fi
    done

    wait 2>/dev/null
    echo "    Done."
}
trap cleanup EXIT INT TERM

# ── Step 4: Start services ─────────────────────────────────────

if $WATCH_MODE; then
    # ── Watch mode: cargo-watch auto-rebuilds on .rs file changes
    echo "==> Starting in watch mode (auto-rebuild on save)..."

    SERVE_ARGS="serve"
    if $EISENBAHN; then
        SERVE_ARGS="serve --eisenbahn"
    fi

    setsid bash -c "cd '$ROOT' && cargo watch -c -w crates/ --delay 1 -x 'run -- $SERVE_ARGS'" &
    PIDS+=($!)

else
    # ── One-shot mode: build + run ──────────────────────────────
    if [ -z "$IMPORT_DIR" ] && [ -z "$IMPORT_FILE" ]; then
        build_rust
    fi

    # Start eisenbahn broker + workers if requested
    if $EISENBAHN; then
        echo "==> Starting eisenbahn broker + workers..."
        setsid "$ROOT/target/${BUILD_PROFILE/dev/debug}/eisenbahn-launcher" --config "$ROOT/config/eisenbahn.toml" &
        PIDS+=($!)
        # Give broker time to bind sockets
        sleep 2
    fi

    # Start the Rust server
    SERVE_ARGS=("serve")
    if $EISENBAHN; then
        SERVE_ARGS+=("--eisenbahn")
    fi

    echo "==> Starting server..."
    setsid "$SERVER_BIN" "${SERVE_ARGS[@]}" &
    PIDS+=($!)
fi

# Start the Next.js dashboard
echo "==> Starting dashboard..."
setsid bash -c "cd '$DASHBOARD' && npm run dev" &
PIDS+=($!)

# ── Banner ──────────────────────────────────────────────────────
echo ""
echo "================================================"
if $WATCH_MODE; then
    echo "  stupid-db is running (WATCH MODE)"
else
    echo "  stupid-db is running!"
fi
echo ""
echo "  API:       http://localhost:${SERVER_PORT}"
echo "  Dashboard: http://localhost:${DASHBOARD_PORT}"
if $EISENBAHN; then
    echo "  Eisenbahn: http://localhost:9090/metrics"
    echo "  ZMQ Bus:   http://localhost:${DASHBOARD_PORT}/eisenbahn"
fi
if $WATCH_MODE; then
    echo ""
    echo "  Edit any .rs file to trigger rebuild"
fi
echo "  Press Ctrl+C to stop"
echo "================================================"
echo ""

wait
