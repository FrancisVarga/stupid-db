"""Team executor for coordinated multi-agent tasks."""

import asyncio
import time
from typing import Any, Literal

from stupid_claude_agent.sdk.agent_executor import AgentExecutor


class TeamExecutor:
    """Executes tasks with coordinated teams of agents."""

    def __init__(self):
        self.agent_executor = AgentExecutor()

    async def execute_team(
        self,
        task: str,
        strategy: Literal["architect_only", "leads_only", "full_hierarchy"],
        context: dict[str, Any] | None = None,
    ) -> dict[str, Any]:
        """
        Execute a task with a team of agents based on the strategy.

        Strategies:
        - architect_only: Single architect agent
        - leads_only: Architect + 3 domain leads (parallel)
        - full_hierarchy: All 7 agents with delegation
        """
        start_time = time.time()
        context = context or {}

        if strategy == "architect_only":
            agents = ["architect"]
        elif strategy == "leads_only":
            agents = ["architect", "backend-lead", "frontend-lead", "data-lead"]
        else:  # full_hierarchy
            agents = [
                "architect",
                "backend-lead",
                "frontend-lead",
                "data-lead",
                "compute-specialist",
                "ingest-specialist",
                "query-specialist",
                "athena-specialist",
            ]

        # Execute agents
        # For full_hierarchy, architect delegates to leads, leads delegate to specialists
        # For now, simple parallel execution (TODO: implement delegation)
        tasks = [self.agent_executor.execute_agent(agent, task, context) for agent in agents]
        results = await asyncio.gather(*tasks, return_exceptions=True)

        # Collect outputs
        outputs = {}
        status = "success"
        for agent, result in zip(agents, results):
            if isinstance(result, Exception):
                outputs[agent] = f"Error: {str(result)}"
                status = "partial"
            else:
                outputs[agent] = result["output"]

        execution_time_ms = int((time.time() - start_time) * 1000)

        return {
            "task": task,
            "strategy": strategy,
            "agents_used": agents,
            "status": status,
            "outputs": outputs,
            "execution_time_ms": execution_time_ms,
        }
