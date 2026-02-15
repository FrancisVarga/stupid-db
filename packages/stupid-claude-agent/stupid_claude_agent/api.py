"""FastAPI application with Scalar docs."""

from contextlib import asynccontextmanager
from typing import Any

from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from fastapi.responses import HTMLResponse

from stupid_claude_agent.config import settings
from stupid_claude_agent.routers import agents, health, teams


@asynccontextmanager
async def lifespan(app: FastAPI):
    """Lifespan context manager for startup/shutdown."""
    # Startup
    print("🔧 Initializing agent system...")
    # TODO: Initialize agent pool, load team configs, etc.
    yield
    # Shutdown
    print("🛑 Shutting down agent system...")
    # TODO: Cleanup resources


def create_app() -> FastAPI:
    """Create and configure the FastAPI application."""
    app = FastAPI(
        title="stupid-claude-agent API",
        description="Claude Code SDK agent execution API with team-based architecture",
        version="0.1.0",
        docs_url=None,  # Disable default docs
        redoc_url=None,  # Disable redoc
        lifespan=lifespan,
    )

    # CORS
    app.add_middleware(
        CORSMiddleware,
        allow_origins=settings.cors_origin.split(","),
        allow_credentials=True,
        allow_methods=["*"],
        allow_headers=["*"],
    )

    # Routers
    app.include_router(health.router, prefix="/api", tags=["Health"])
    app.include_router(agents.router, prefix="/api/agents", tags=["Agents"])
    app.include_router(teams.router, prefix="/api/teams", tags=["Teams"])

    # Scalar docs endpoint
    @app.get("/docs", include_in_schema=False)
    async def scalar_docs() -> HTMLResponse:
        """Scalar API documentation UI."""
        return HTMLResponse(
            f"""
            <!doctype html>
            <html>
            <head>
                <title>stupid-claude-agent API Documentation</title>
                <meta charset="utf-8" />
                <meta name="viewport" content="width=device-width, initial-scale=1" />
            </head>
            <body>
                <script
                    id="api-reference"
                    data-url="/openapi.json"
                    data-configuration='{{"theme":"purple"}}'
                ></script>
                <script src="https://cdn.jsdelivr.net/npm/@scalar/api-reference"></script>
            </body>
            </html>
            """
        )

    return app
