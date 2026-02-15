"""Agent execution endpoints."""

from typing import Literal

from fastapi import APIRouter, HTTPException
from pydantic import BaseModel, Field

from stupid_claude_agent.sdk.agent_executor import AgentExecutor

router = APIRouter()


class AgentRequest(BaseModel):
    """Request to execute an agent."""

    agent_name: Literal[
        "architect",
        "backend-lead",
        "frontend-lead",
        "data-lead",
        "compute-specialist",
        "ingest-specialist",
        "query-specialist",
        "athena-specialist",
    ] = Field(..., description="Name of the agent to execute")
    task: str = Field(..., description="Task description for the agent", min_length=1)
    context: dict = Field(default_factory=dict, description="Additional context for the task")


class AgentResponse(BaseModel):
    """Response from agent execution."""

    agent_name: str
    status: Literal["success", "error", "timeout"]
    output: str
    execution_time_ms: int
    tokens_used: int | None = None


@router.post("/execute", response_model=AgentResponse)
async def execute_agent(request: AgentRequest):
    """
    Execute a single agent with a task.

    The agent will run autonomously using the Claude Code SDK,
    with access to the full project context and skills.
    """
    executor = AgentExecutor()

    try:
        result = await executor.execute_agent(
            agent_name=request.agent_name,
            task=request.task,
            context=request.context,
        )
        return result
    except Exception as e:
        raise HTTPException(status_code=500, detail=f"Agent execution failed: {str(e)}")


@router.get("/list")
async def list_agents():
    """List all available agents with their descriptions."""
    return {
        "agents": [
            {
                "name": "architect",
                "tier": 1,
                "description": "System architect for cross-cutting design decisions and delegation",
            },
            {
                "name": "backend-lead",
                "tier": 2,
                "description": "Rust backend lead for all 12 crates",
            },
            {
                "name": "frontend-lead",
                "tier": 2,
                "description": "Next.js + D3.js dashboard specialist",
            },
            {
                "name": "data-lead",
                "tier": 2,
                "description": "w88 data domain and OpenSearch expert",
            },
            {
                "name": "compute-specialist",
                "tier": 3,
                "description": "Algorithm specialist (KMeans, DBSCAN, PageRank, etc.)",
            },
            {
                "name": "ingest-specialist",
                "tier": 3,
                "description": "Data pipeline specialist (ingest, connector, embedder)",
            },
            {
                "name": "query-specialist",
                "tier": 3,
                "description": "Query system and LLM integration specialist",
            },
            {
                "name": "athena-specialist",
                "tier": 3,
                "description": "AWS Athena specialist for historical data queries and cost optimization",
            },
        ]
    }
