"""Health check endpoints."""

from fastapi import APIRouter

from stupid_claude_agent.config import settings

router = APIRouter()


@router.get("/health")
async def health_check():
    """Health check endpoint."""
    return {
        "status": "healthy",
        "version": "0.1.0",
        "agents_available": 7,
        "skills_available": 13,
    }


@router.get("/config")
async def get_config():
    """Get current configuration (non-sensitive)."""
    return {
        "llm_provider": settings.llm_provider,
        "max_concurrent_agents": settings.max_concurrent_agents,
        "agent_timeout_seconds": settings.agent_timeout_seconds,
        "agents_dir": str(settings.agents_dir),
        "skills_dir": str(settings.skills_dir),
    }
