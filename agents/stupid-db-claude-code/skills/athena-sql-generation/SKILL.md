---
name: athena-sql-generation
description: Generate cost-aware, partition-pruned Athena SQL queries from natural language user input
triggers:
  - athena query
  - athena sql
  - sql generation
  - historical query
  - data exploration
  - generate sql
  - query data
---

# Athena SQL Generation

## Overview

Generate optimized AWS Athena SQL queries from natural language input. Queries target the `events` table partitioned by `event_type` and `dt` (date). Always prioritize partition pruning and cost awareness ($5/TB scanned).

## Table Schema: events

```sql
-- Data columns
memberCode      VARCHAR    -- Player/member identifier
deviceId        VARCHAR    -- Device fingerprint
game            VARCHAR    -- Game name (for GameOpened events)
platform        VARCHAR    -- Platform (Desktop, Mobile, etc.)
vipGroup        VARCHAR    -- VIP tier (bronze, silver, gold, platinum, diamond, vip)
currency        VARCHAR    -- Currency code (USD, EUR, VND, THB, etc.)
popupType       VARCHAR    -- Popup type (for PopupModule events)
errorCode       VARCHAR    -- Error code (for API Error events)
errorMessage    VARCHAR    -- Error message text
affiliateCode   VARCHAR    -- Affiliate referral code
providerCode    VARCHAR    -- Game provider code
timestamp       TIMESTAMP  -- Event timestamp (ISO 8601)
sessionId       VARCHAR    -- Session identifier

-- Partition columns (ALWAYS filter on these)
event_type      VARCHAR    -- 'Login', 'GameOpened', 'PopupModule', 'API Error'
dt              VARCHAR    -- Date partition: 'YYYY-MM-DD'
```

## Cost Model

- **$5 per TB scanned** (Athena pricing)
- Default scan budget: 10 GB (`ATHENA_MAX_SCAN_BYTES`)
- Always estimate scan cost in query comments
- Partition pruning is the #1 cost optimization

## Query Generation Rules

### MUST DO (Every Query)
1. **Always filter on `event_type`** — Never scan all event types
2. **Always filter on `dt`** — Never scan all dates
3. **Never use `SELECT *`** — Only select needed columns
4. **Add `LIMIT`** for exploration queries (default 1000)
5. **Include cost estimate** as SQL comment

### SHOULD DO
- Pre-aggregate before JOINs (use CTEs)
- Use `APPROX_DISTINCT()` instead of `COUNT(DISTINCT)` for large datasets
- Use `date_parse()` for timestamp operations
- Cast partition columns explicitly when comparing

### MUST NOT
- Never scan more than 30 days without explicit user request
- Never use `SELECT *` with large tables
- Never JOIN without partition filters on both sides

## Query Patterns

### Pattern 1: Event Count by Day

**User**: "How many logins happened last week?"

```sql
-- Estimated scan: ~500MB (Login events, 7 days)
SELECT
    dt,
    COUNT(*) AS login_count,
    COUNT(DISTINCT memberCode) AS unique_members
FROM events
WHERE event_type = 'Login'
  AND dt BETWEEN '2026-02-10' AND '2026-02-16'
GROUP BY dt
ORDER BY dt
```

### Pattern 2: Top Entities by Activity

**User**: "Show me the most active players this month"

```sql
-- Estimated scan: ~2GB (Login events, 17 days)
SELECT
    memberCode,
    COUNT(*) AS total_logins,
    COUNT(DISTINCT deviceId) AS unique_devices,
    MIN(dt) AS first_seen,
    MAX(dt) AS last_seen
FROM events
WHERE event_type = 'Login'
  AND dt >= '2026-02-01'
GROUP BY memberCode
HAVING COUNT(*) > 10
ORDER BY total_logins DESC
LIMIT 100
```

### Pattern 3: Error Analysis

**User**: "What are the most common errors today?"

```sql
-- Estimated scan: ~200MB (API Error events, 1 day)
SELECT
    errorCode,
    errorMessage,
    COUNT(*) AS error_count,
    COUNT(DISTINCT memberCode) AS affected_members
FROM events
WHERE event_type = 'API Error'
  AND dt = '2026-02-17'
GROUP BY errorCode, errorMessage
ORDER BY error_count DESC
LIMIT 50
```

### Pattern 4: Game Popularity

**User**: "Which games are trending this week?"

```sql
-- Estimated scan: ~1GB (GameOpened events, 7 days)
SELECT
    game,
    providerCode,
    COUNT(*) AS play_count,
    APPROX_DISTINCT(memberCode) AS unique_players
FROM events
WHERE event_type = 'GameOpened'
  AND dt BETWEEN '2026-02-10' AND '2026-02-16'
GROUP BY game, providerCode
ORDER BY play_count DESC
LIMIT 50
```

### Pattern 5: Member Journey

**User**: "Show me what player M12345 did yesterday"

```sql
-- Estimated scan: ~3GB (all event types, 1 day, filtered by member)
SELECT
    timestamp,
    event_type,
    COALESCE(game, popupType, errorCode, deviceId) AS detail,
    platform,
    sessionId
FROM events
WHERE memberCode = 'M12345'
  AND dt = '2026-02-16'
ORDER BY timestamp
```

### Pattern 6: Device Sharing Detection

**User**: "Find devices shared by multiple players"

```sql
-- Estimated scan: ~500MB (Login events, 7 days)
SELECT
    deviceId,
    COUNT(DISTINCT memberCode) AS member_count,
    ARRAY_AGG(DISTINCT memberCode) AS members
FROM events
WHERE event_type = 'Login'
  AND dt BETWEEN '2026-02-10' AND '2026-02-16'
GROUP BY deviceId
HAVING COUNT(DISTINCT memberCode) > 1
ORDER BY member_count DESC
LIMIT 100
```

### Pattern 7: Week-over-Week Comparison

**User**: "Compare this week's logins to last week"

```sql
-- Estimated scan: ~1GB (Login events, 14 days)
WITH this_week AS (
    SELECT dt, COUNT(*) AS cnt
    FROM events
    WHERE event_type = 'Login'
      AND dt BETWEEN '2026-02-10' AND '2026-02-16'
    GROUP BY dt
),
last_week AS (
    SELECT dt, COUNT(*) AS cnt
    FROM events
    WHERE event_type = 'Login'
      AND dt BETWEEN '2026-02-03' AND '2026-02-09'
    GROUP BY dt
)
SELECT
    tw.dt AS this_week_date,
    tw.cnt AS this_week_count,
    lw.cnt AS last_week_count,
    ROUND(100.0 * (tw.cnt - lw.cnt) / lw.cnt, 1) AS pct_change
FROM this_week tw
JOIN last_week lw
  ON DATE_ADD('day', -7, DATE_PARSE(tw.dt, '%Y-%m-%d'))
     = DATE_PARSE(lw.dt, '%Y-%m-%d')
ORDER BY tw.dt
```

### Pattern 8: VIP Cohort Analysis

**User**: "How do VIP tiers compare in activity?"

```sql
-- Estimated scan: ~1GB (Login events, 7 days)
SELECT
    vipGroup,
    COUNT(DISTINCT memberCode) AS members,
    COUNT(*) AS total_logins,
    ROUND(CAST(COUNT(*) AS DOUBLE) / COUNT(DISTINCT memberCode), 1) AS avg_logins_per_member
FROM events
WHERE event_type = 'Login'
  AND dt BETWEEN '2026-02-10' AND '2026-02-16'
  AND vipGroup IS NOT NULL
GROUP BY vipGroup
ORDER BY avg_logins_per_member DESC
```

### Pattern 9: Temporal Patterns (Hourly)

**User**: "What hours are most active for logins?"

```sql
-- Estimated scan: ~500MB (Login events, 7 days)
SELECT
    HOUR(date_parse(timestamp, '%Y-%m-%dT%H:%i:%s')) AS hour_of_day,
    COUNT(*) AS login_count
FROM events
WHERE event_type = 'Login'
  AND dt BETWEEN '2026-02-10' AND '2026-02-16'
GROUP BY HOUR(date_parse(timestamp, '%Y-%m-%dT%H:%i:%s'))
ORDER BY hour_of_day
```

### Pattern 10: Cross-Event Correlation

**User**: "Do players who see errors play fewer games?"

```sql
-- Estimated scan: ~2GB (2 event types, 7 days)
WITH error_members AS (
    SELECT memberCode, COUNT(*) AS error_count
    FROM events
    WHERE event_type = 'API Error'
      AND dt BETWEEN '2026-02-10' AND '2026-02-16'
    GROUP BY memberCode
),
game_members AS (
    SELECT memberCode, COUNT(*) AS game_count
    FROM events
    WHERE event_type = 'GameOpened'
      AND dt BETWEEN '2026-02-10' AND '2026-02-16'
    GROUP BY memberCode
)
SELECT
    CASE
        WHEN e.error_count IS NULL THEN 'No Errors'
        WHEN e.error_count BETWEEN 1 AND 5 THEN '1-5 Errors'
        ELSE '6+ Errors'
    END AS error_bucket,
    COUNT(*) AS member_count,
    ROUND(AVG(COALESCE(g.game_count, 0)), 1) AS avg_games
FROM error_members e
LEFT JOIN game_members g ON e.memberCode = g.memberCode
GROUP BY CASE
    WHEN e.error_count IS NULL THEN 'No Errors'
    WHEN e.error_count BETWEEN 1 AND 5 THEN '1-5 Errors'
    ELSE '6+ Errors'
END
ORDER BY avg_games DESC
```

## Query Generation Workflow

1. **Parse user intent**: What data? What time range? What aggregation?
2. **Select event types**: Map user terms → event_type values
3. **Determine date range**: Default to 7 days if not specified
4. **Choose columns**: Only what's needed, never SELECT *
5. **Add partition filters**: event_type + dt in WHERE clause
6. **Add aggregation**: GROUP BY for summaries, raw for exploration
7. **Estimate cost**: Comment with estimated scan size
8. **Add LIMIT**: For exploration, always limit results

## User Term Mapping

| User Says | Maps To |
|-----------|---------|
| "logins", "sign-ins", "authentication" | `event_type = 'Login'` |
| "games", "plays", "gaming" | `event_type = 'GameOpened'` |
| "errors", "failures", "bugs" | `event_type = 'API Error'` |
| "popups", "notifications", "modals" | `event_type = 'PopupModule'` |
| "player", "member", "user" | `memberCode` column |
| "device", "fingerprint" | `deviceId` column |
| "today" | `dt = 'YYYY-MM-DD'` (current date) |
| "yesterday" | `dt = 'YYYY-MM-DD'` (current date - 1) |
| "this week" | `dt BETWEEN 'Mon' AND 'Sun'` |
| "last week" | Previous 7 days |
| "this month" | `dt >= 'YYYY-MM-01'` |

## Athena-Specific SQL Notes

- **String comparison**: Case-sensitive by default
- **NULL handling**: Use `IS NOT NULL` or `COALESCE()`
- **Array functions**: `ARRAY_AGG()`, `CARDINALITY()` for arrays
- **Approximate functions**: `APPROX_DISTINCT()` cheaper than `COUNT(DISTINCT)`
- **Date functions**: `date_parse()`, `DATE_ADD()`, `DATE_DIFF()`, `CURRENT_DATE`
- **Partitions are strings**: `dt` is VARCHAR, compare as strings ('2026-02-17')
- **No UPDATE/DELETE**: Athena is read-only, append via new partitions

## Integration Points

### Rust Execution
```rust
// AthenaQueryStepParams for execution
AthenaQueryStepParams {
    sql: "SELECT ...",
    max_scan_gb: Some(10.0),
    event_type: Some("Login"),        // For document conversion
    timestamp_column: Some("timestamp"),
}
```

### API Endpoint
```
POST /athena/{connection_id}/query
Content-Type: application/json
{ "sql": "SELECT ..." }
→ Server-Sent Events (streaming rows)
```

### Result Types
```rust
AthenaQueryResult {
    columns: Vec<AthenaColumn>,      // name + data_type
    rows: Vec<Vec<Option<String>>>,  // String values (parse client-side)
    metadata: QueryMetadata,         // query_id, bytes_scanned, cost_usd
}
```
