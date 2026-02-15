"""FastMCP server exposing agents as MCP tools."""

import asyncio

from fastmcp import FastMCP

from stupid_claude_agent.config import settings
from stupid_claude_agent.sdk.agent_executor import AgentExecutor
from stupid_claude_agent.sdk.team_executor import TeamExecutor

# Create MCP server
mcp = FastMCP("stupid-claude-agent")


@mcp.tool()
async def execute_agent(agent_name: str, task: str) -> str:
    """
    Execute a single agent with a task.

    Args:
        agent_name: Name of agent (architect, backend-lead, frontend-lead, etc.)
        task: Task description

    Returns:
        Agent output
    """
    executor = AgentExecutor()
    result = await executor.execute_agent(agent_name, task)
    return result["output"]


@mcp.tool()
async def execute_team(task: str, strategy: str = "full_hierarchy") -> dict:
    """
    Execute a task with a team of agents.

    Args:
        task: High-level task description
        strategy: Team strategy (architect_only, leads_only, full_hierarchy)

    Returns:
        Team execution results with outputs from each agent
    """
    executor = TeamExecutor()
    result = await executor.execute_team(task, strategy)  # type: ignore
    return result


@mcp.tool()
async def list_agents() -> list[dict]:
    """
    List all available agents.

    Returns:
        List of agent metadata (name, tier, description)
    """
    return [
        {"name": "architect", "tier": 1, "description": "System architect"},
        {"name": "backend-lead", "tier": 2, "description": "Rust backend lead"},
        {"name": "frontend-lead", "tier": 2, "description": "Next.js dashboard lead"},
        {"name": "data-lead", "tier": 2, "description": "Data domain expert"},
        {"name": "compute-specialist", "tier": 3, "description": "Algorithm specialist"},
        {"name": "ingest-specialist", "tier": 3, "description": "Pipeline specialist"},
        {"name": "query-specialist", "tier": 3, "description": "Query/LLM specialist"},
        {"name": "athena-specialist", "tier": 3, "description": "AWS Athena specialist"},
    ]


@mcp.resource("config://settings")
async def get_settings() -> str:
    """
    Get current agent system configuration.

    Returns:
        Configuration as JSON string
    """
    import json

    return json.dumps(
        {
            "llm_provider": settings.llm_provider,
            "max_concurrent_agents": settings.max_concurrent_agents,
            "agents_dir": str(settings.agents_dir),
            "skills_dir": str(settings.skills_dir),
        },
        indent=2,
    )


def run_mcp_server_stdio():
    """Run the MCP server in stdio mode for Claude Desktop integration."""
    mcp.run(transport="stdio")


def run_mcp_server_sse():
    """Run the MCP server in SSE mode for HTTP integration."""
    mcp.run(transport="sse", host=settings.host, port=settings.mcp_port)


if __name__ == "__main__":
    # Default to stdio for Claude Desktop
    run_mcp_server_stdio()
