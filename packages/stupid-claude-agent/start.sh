#!/bin/bash
# Quick start script for stupid-claude-agent

set -e

echo "🚀 Starting stupid-claude-agent"
echo ""

# Check if virtual environment exists
if [ ! -d ".venv" ]; then
    echo "Creating virtual environment..."
    uv venv
fi

# Activate virtual environment
source .venv/bin/activate || source .venv/Scripts/activate

# Install dependencies
echo "Installing dependencies..."
uv pip install -e .

# Start server
echo ""
echo "Starting FastAPI server..."
echo "  API: http://localhost:8000"
echo "  Docs (Scalar): http://localhost:8000/docs"
echo ""
python main.py
