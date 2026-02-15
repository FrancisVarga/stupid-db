"""
stupid-claude-agent — Claude Code SDK agent with FastAPI + MCP server

Architecture:
- FastAPI server with Scalar docs at /docs
- FastMCP server exposing agents as MCP tools
- Claude Code SDK team-based agent execution
- 7 hierarchical agents (architect, leads, specialists)
"""

import asyncio
from pathlib import Path

import uvicorn

from stupid_claude_agent.api import create_app
from stupid_claude_agent.config import settings


def main():
    """Run the FastAPI server with Scalar docs and MCP server."""
    print(f"🚀 Starting stupid-claude-agent server")
    print(f"   API: http://{settings.host}:{settings.port}")
    print(f"   Docs (Scalar): http://{settings.host}:{settings.port}/docs")
    print(f"   MCP server: stdio (for Claude Desktop integration)")

    # Create FastAPI app
    app = create_app()

    # Run with uvicorn
    uvicorn.run(
        app,
        host=settings.host,
        port=settings.port,
        log_level="info",
    )


if __name__ == "__main__":
    main()
