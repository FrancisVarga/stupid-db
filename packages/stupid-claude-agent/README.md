# stupid-claude-agent

Claude Code SDK agent application with FastAPI + Scalar docs, FastMCP server, and team-based architecture.

## Architecture

```
FastAPI Server (HTTP/REST)
    ├── /docs — Scalar API documentation
    ├── /api/health — Health check
    ├── /api/agents/execute — Execute single agent
    ├── /api/agents/list — List agents
    ├── /api/teams/execute — Execute team
    └── /api/teams/strategies — List strategies

FastMCP Server (stdio)
    ├── execute_agent(agent_name, task) → str
    ├── execute_team(task, strategy) → dict
    ├── list_agents() → list[dict]
    └── config://settings resource

Claude Code SDK Integration
    ├── 7 hierarchical agents (.claude/agents/)
    ├── 13 self-contained skills (.claude/skills/)
    └── Team coordination with delegation
```

## Agents (8 Total)

| Tier | Agent | Role |
|------|-------|------|
| T1 | `architect` | System design, cross-cutting review, delegation |
| T2 | `backend-lead` | All Rust crates |
| T2 | `frontend-lead` | Next.js + D3.js dashboard |
| T2 | `data-lead` | w88 data domain, OpenSearch |
| T3 | `compute-specialist` | Algorithms (KMeans, DBSCAN, etc.) |
| T3 | `ingest-specialist` | Data pipeline |
| T3 | `query-specialist` | Query + LLM integration |
| T3 | `athena-specialist` | AWS Athena historical queries |

## Skills (15 Total)

Architecture: `system-architecture`, `crate-map`
Rust: `rust-patterns`
Storage: `segment-storage`
Algorithms: `graph-algorithms`, `compute-algorithms`
Data: `w88-data-model`, `opensearch-queries`
Frontend: `dashboard-patterns`
Query: `query-interface`, `llm-prompt-patterns`
AWS: `athena-query-patterns`, `aws-integration`
Ops: `debugging-playbook`, `performance-guide`

## Installation

```bash
cd packages/stupid-claude-agent
uv pip install -e .
```

## Usage

### FastAPI Server

```bash
python main.py
```

Open http://localhost:8000/docs for Scalar API documentation.

### MCP Server (for Claude Desktop)

Add to `claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "stupid-claude-agent": {
      "command": "python",
      "args": ["-m", "stupid_claude_agent.mcp_server"]
    }
  }
}
```

### API Examples

**Execute single agent:**
```bash
curl -X POST http://localhost:8000/api/agents/execute \
  -H "Content-Type: application/json" \
  -d '{
    "agent_name": "architect",
    "task": "Review the segment storage architecture"
  }'
```

**Execute team:**
```bash
curl -X POST http://localhost:8000/api/teams/execute \
  -H "Content-Type: application/json" \
  -d '{
    "task": "Implement new clustering algorithm",
    "team_strategy": "full_hierarchy"
  }'
```

## Configuration

Set environment variables in `.env` (root level):

```bash
# Server
HOST=0.0.0.0
PORT=8000

# LLM
LLM_PROVIDER=anthropic
ANTHROPIC_API_KEY=sk-ant-...
ANTHROPIC_MODEL=claude-sonnet-4-5-20250929

# Agent execution
MAX_CONCURRENT_AGENTS=3
AGENT_TIMEOUT_SECONDS=300
```

## Development

```bash
# Install dev dependencies
uv pip install -e ".[dev]"

# Run tests
pytest

# Format code
ruff format .

# Lint
ruff check .
```

## TODO

- [ ] Integrate Claude Code SDK for agent execution (currently placeholder)
- [ ] Implement agent delegation (architect → leads → specialists)
- [ ] Add streaming SSE endpoints for real-time agent output
- [ ] Implement agent result persistence
- [ ] Add authentication/authorization
- [ ] Metrics and observability
