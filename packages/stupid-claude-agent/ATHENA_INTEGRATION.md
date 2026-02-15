# AWS Athena Integration for stupid-claude-agent

## What Was Added

✅ **1 new agent**: `athena-specialist` (Tier 3)
✅ **2 new skills**: `athena-query-patterns`, `aws-integration`
✅ **Updated team execution** to include Athena specialist in `full_hierarchy` strategy
✅ **Updated API endpoints** to expose Athena specialist

## New Agent: athena-specialist

**Location**: `packages/stupid-claude-agent/.claude/agents/athena-specialist.md`

### Expertise
- AWS Athena SQL query design and optimization
- Historical data analysis (beyond 30-day local window)
- Partition pruning and cost control
- Integration with local materialization
- Temporal comparisons and baseline analysis

### Key Capabilities
- Query archival data in S3 via Athena
- Optimize queries for cost (partition/column pruning)
- Design federated queries (Athena + local data)
- Analyze scan bytes and execution time
- Prevent runaway costs with scan limits

## New Skills

### 1. athena-query-patterns

**Location**: `packages/stupid-claude-agent/.claude/skills/athena-query-patterns/SKILL.md`

**Content**:
- Complete Athena table schema for `events` table
- Partition pruning patterns (critical for cost)
- Common query templates (temporal, aggregation, cohort)
- Cost optimization techniques (column pruning, pre-aggregation)
- Advanced patterns (cohort analysis, provider performance)
- Integration with local data
- Performance targets and best practices checklist

**Example Query**:
```sql
SELECT
    dt,
    count(*) as error_count
FROM events
WHERE event_type = 'apiError'
  AND dt BETWEEN '2024-10-01' AND '2024-12-31'
GROUP BY 1
ORDER BY 1;
```

### 2. aws-integration

**Location**: `packages/stupid-claude-agent/.claude/skills/aws-integration/SKILL.md`

**Content**:
- Complete AWS integration overview (Athena, Aurora/RDS, S3)
- Configuration patterns from `.env` and config files
- Use cases for each AWS service
- Query plan integration (AthenaQueryStep, AuroraQuery, S3ParquetRead)
- Cost monitoring and security best practices
- Rust crate dependencies and common patterns

## Updated Team Execution

### full_hierarchy Strategy
Now includes **8 agents** (was 7):

```python
agents = [
    "architect",
    "backend-lead",
    "frontend-lead",
    "data-lead",
    "compute-specialist",
    "ingest-specialist",
    "query-specialist",
    "athena-specialist",  # NEW
]
```

### Example Team Execution
```bash
curl -X POST http://localhost:8000/api/teams/execute \
  -H "Content-Type: application/json" \
  -d '{
    "task": "Analyze error rate trends: compare current week to same week last quarter using Athena",
    "team_strategy": "full_hierarchy"
  }'
```

**Agent delegation**:
1. **architect** — Overall strategy, coordinate Athena + local data merge
2. **data-lead** — Identify error patterns and relevant fields
3. **athena-specialist** — Design Athena query for Q4 2024 baseline
4. **backend-lead** — Integrate Athena results with local compute
5. **query-specialist** — Expose comparison via query interface
6. **frontend-lead** — Visualize trend comparison in dashboard

## API Changes

### Execute Agent
Now accepts `"athena-specialist"` as a valid agent:

```python
class AgentRequest(BaseModel):
    agent_name: Literal[
        "architect",
        "backend-lead",
        "frontend-lead",
        "data-lead",
        "compute-specialist",
        "ingest-specialist",
        "query-specialist",
        "athena-specialist",  # NEW
    ]
```

### List Agents
Returns 8 agents:

```bash
GET /api/agents/list
```

```json
{
  "agents": [
    ...,
    {
      "name": "athena-specialist",
      "tier": 3,
      "description": "AWS Athena specialist for historical data queries and cost optimization"
    }
  ]
}
```

## MCP Server

The FastMCP server now exposes the Athena specialist:

```python
# Execute Athena specialist via MCP
await execute_agent(
    agent_name="athena-specialist",
    task="Design a cost-optimized Athena query to find top games by provider in Q4 2024"
)
```

## Use Cases

### 1. Historical Baseline Comparison
**Task**: "How does this month's member activity compare to last year?"

**Agents**:
- `athena-specialist` — Query same month last year from Athena
- `data-lead` — Identify activity metrics (game diversity, session frequency)
- `backend-lead` — Merge Athena baseline with local current data
- `frontend-lead` — Visualize year-over-year comparison

### 2. Seasonal Pattern Analysis
**Task**: "Find seasonal trends in game preferences over the past 2 years"

**Agents**:
- `athena-specialist` — Multi-year query with temporal grouping
- `compute-specialist` — Detect recurring patterns
- `query-specialist` — Design query plan for seasonal analysis
- `frontend-lead` — Heatmap visualization by month/game

### 3. Cost-Optimized Exploration
**Task**: "Explore error patterns in production data from 6 months ago without exceeding $0.50"

**Agents**:
- `athena-specialist` — Design query with scan limit (max 25GB @ $0.02/GB)
- `data-lead` — Recommend partition/column pruning strategy
- `architect` — Review cost vs insight tradeoff

## Configuration

The Athena specialist understands the full AWS config from `.env`:

```bash
# Athena credentials
ATHENA_ACCESS_KEY=AKIA6EBOOQIN2HEWUGKP
ATHENA_ACCESS_KEY_SECRET=fgwq...
ATHENA_REGION=ap-northeast-1

# Query settings
ATHENA_S3_BUCKET_OUTPUT=s3://services-athena-query-output-s3-9707
ATHENA_LOCAL_CACHE_DIR=/mnt/camelot/duckdb/athena
ATHENA_LOCAL_CACHE_EXPIRE=360000
```

## Integration with Existing Crate

The agent understands the `crates/athena/` implementation:

**AthenaClient API**:
```rust
pub async fn execute_query(&self, sql: &str) -> Result<AthenaQueryResult, AthenaError>;
pub async fn execute_query_with_limit(&self, sql: &str, max_scan_bytes: u64) -> Result<AthenaQueryResult, AthenaError>;
pub async fn cancel_query(&self, query_id: &str) -> Result<(), AthenaError>;
```

**Cost control**:
- Pre-query: Set `max_scan_bytes` limit
- Post-query: Check `metadata.bytes_scanned`
- Automatic timeout and cancellation

## Example Queries (from Skills)

### Temporal Comparison
```sql
-- Current vs last quarter error rate
SELECT
    date_trunc('day', from_iso8601_timestamp(`@timestamp`)) as day,
    count(*) as error_count
FROM events
WHERE event_type = 'apiError'
  AND dt BETWEEN '2024-10-01' AND '2024-12-31'
GROUP BY 1
ORDER BY 1;
```

### Top Games by Provider
```sql
SELECT
    provider,
    count(DISTINCT gameUid) as game_count,
    count(DISTINCT memberCode) as player_count,
    count(*) as total_plays
FROM events
WHERE event_type = 'GameOpened'
  AND dt >= '2024-02-01'
  AND provider IS NOT NULL
GROUP BY 1
ORDER BY total_plays DESC;
```

### Device Sharing Detection
```sql
SELECT
    fingerprint,
    count(DISTINCT memberCode) as member_count,
    array_agg(DISTINCT memberCode) as members
FROM events
WHERE event_type = 'Login'
  AND dt BETWEEN '2025-02-01' AND '2025-02-15'
GROUP BY 1
HAVING count(DISTINCT memberCode) > 1;
```

## Cost Optimization Checklist

The `athena-query-patterns` skill includes a comprehensive checklist:

- [ ] Include `event_type` and `dt` in WHERE clause
- [ ] Use column pruning (SELECT specific columns, not *)
- [ ] Add LIMIT for exploration queries
- [ ] Use `count(DISTINCT x)` instead of `SELECT DISTINCT x`
- [ ] Pre-aggregate before joins
- [ ] Test on small date ranges first
- [ ] Monitor `bytes_scanned` in metadata
- [ ] Leverage Athena result caching (24h)

## Performance Targets

| Query Type | Target Time | Max Scan |
|-----------|-------------|----------|
| Exploration (LIMIT 100) | < 5s | < 100MB |
| Daily aggregation | < 15s | < 500MB |
| Weekly aggregation | < 30s | < 2GB |
| Multi-month analysis | < 60s | < 10GB |

## Summary

The stupid-claude-agent now has **complete AWS Athena expertise**:

- **1 specialist agent** for historical data queries
- **2 comprehensive skills** covering query patterns and AWS integration
- **Full team integration** for coordinated multi-agent tasks
- **API + MCP exposure** for external access
- **Cost-aware design** with scan limits and optimization patterns

The Athena specialist enables the team to answer questions like:
- "How does today compare to last year?"
- "What were member activity patterns 6 months ago?"
- "Find seasonal trends over 2+ years"
- "Compare current errors to historical baselines"

All while maintaining cost control and query optimization best practices.
