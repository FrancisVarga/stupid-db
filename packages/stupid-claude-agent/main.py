"""
stupid-claude-agent — Claude Code SDK agent with FastAPI + MCP server

Architecture:
- FastAPI server with Scalar docs at /docs
- FastMCP SSE server exposing agents as MCP tools
- Claude Code SDK team-based agent execution
- 7 hierarchical agents (architect, leads, specialists)
"""

import multiprocessing

import uvicorn

from stupid_claude_agent.api import create_app
from stupid_claude_agent.config import settings


def run_fastapi():
    """Run FastAPI server."""
    app = create_app()
    uvicorn.run(
        app,
        host=settings.host,
        port=settings.port,
        log_level="info",
    )


def run_mcp_sse():
    """Run MCP SSE server."""
    from stupid_claude_agent.mcp_server import run_mcp_server_sse

    run_mcp_server_sse()


def main():
    """Run both FastAPI and MCP SSE servers concurrently."""
    print(f"🚀 Starting stupid-claude-agent servers")
    print(f"   FastAPI: http://{settings.host}:{settings.port}")
    print(f"   Docs (Scalar): http://{settings.host}:{settings.port}/docs")
    print(f"   MCP SSE: http://{settings.host}:{settings.mcp_port}")
    print()

    # Run both servers in separate processes
    fastapi_process = multiprocessing.Process(target=run_fastapi, name="FastAPI")
    mcp_process = multiprocessing.Process(target=run_mcp_sse, name="MCP-SSE")

    fastapi_process.start()
    mcp_process.start()

    try:
        fastapi_process.join()
        mcp_process.join()
    except KeyboardInterrupt:
        print("\n⏹️  Shutting down servers...")
        fastapi_process.terminate()
        mcp_process.terminate()
        fastapi_process.join()
        mcp_process.join()
        print("✅ Shutdown complete")


if __name__ == "__main__":
    main()
