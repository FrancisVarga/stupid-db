"""Team execution endpoints for coordinated multi-agent tasks."""

from typing import Literal

from fastapi import APIRouter, HTTPException
from pydantic import BaseModel, Field

from stupid_claude_agent.sdk.team_executor import TeamExecutor

router = APIRouter()


class TeamRequest(BaseModel):
    """Request to execute a task with a team."""

    task: str = Field(..., description="High-level task description", min_length=1)
    team_strategy: Literal["architect_only", "leads_only", "full_hierarchy"] = Field(
        default="full_hierarchy",
        description="Which agents to use: architect only, tier 2 leads, or full hierarchy",
    )
    context: dict = Field(default_factory=dict, description="Additional context")


class TeamResponse(BaseModel):
    """Response from team execution."""

    task: str
    strategy: str
    agents_used: list[str]
    status: Literal["success", "partial", "error"]
    outputs: dict[str, str]
    execution_time_ms: int


@router.post("/execute", response_model=TeamResponse)
async def execute_team(request: TeamRequest):
    """
    Execute a task with a team of agents.

    The team will coordinate based on the strategy:
    - architect_only: Just the architect agent
    - leads_only: Architect + domain leads (backend, frontend, data)
    - full_hierarchy: All 7 agents with proper delegation
    """
    executor = TeamExecutor()

    try:
        result = await executor.execute_team(
            task=request.task,
            strategy=request.team_strategy,
            context=request.context,
        )
        return result
    except Exception as e:
        raise HTTPException(status_code=500, detail=f"Team execution failed: {str(e)}")


@router.get("/strategies")
async def list_strategies():
    """List available team execution strategies."""
    return {
        "strategies": [
            {
                "name": "architect_only",
                "agents": ["architect"],
                "description": "Single agent for simple architectural questions",
            },
            {
                "name": "leads_only",
                "agents": ["architect", "backend-lead", "frontend-lead", "data-lead"],
                "description": "Domain leads for coordinated cross-domain work",
            },
            {
                "name": "full_hierarchy",
                "agents": [
                    "architect",
                    "backend-lead",
                    "frontend-lead",
                    "data-lead",
                    "compute-specialist",
                    "ingest-specialist",
                    "query-specialist",
                    "athena-specialist",
                ],
                "description": "Full team with all specialists for complex tasks",
            },
        ]
    }
