---
name: Athena Query Patterns
description: AWS Athena SQL query patterns for the stupid-db project. Partition pruning, cost optimization, temporal analysis, and integration with local data. Use when writing Athena queries, optimizing costs, or analyzing historical data.
version: 1.0.0
---

# Athena Query Patterns

## Table Schema

```sql
CREATE EXTERNAL TABLE events (
    memberCode STRING,
    fingerprint STRING,
    gameUid STRING,
    gameName STRING,
    provider STRING,
    platform STRING,
    currency STRING,
    rGroup STRING,
    errorCode STRING,
    errorMessage STRING,
    apiPath STRING,
    vipGroup STRING,
    affiliateId STRING,
    `@timestamp` STRING  -- ISO 8601 format
)
PARTITIONED BY (
    event_type STRING,   -- Login | GameOpened | apiError | PopupModule
    dt STRING            -- YYYY-MM-DD
)
STORED AS PARQUET
LOCATION 's3://w88-analytics/events/';
```

## Partition Pruning (Critical for Cost)

**Always include partition columns in WHERE clause:**

```sql
-- ✅ GOOD: Scans only 1 day × 1 event type
SELECT * FROM events
WHERE event_type = 'Login' AND dt = '2025-02-15'
LIMIT 100;

-- ✅ GOOD: Date range with event type
SELECT * FROM events
WHERE event_type = 'GameOpened'
  AND dt BETWEEN '2025-02-01' AND '2025-02-15';

-- ❌ BAD: No partition filters — scans entire table
SELECT * FROM events
WHERE `@timestamp` >= '2025-02-01';

-- ❌ BAD: Timestamp filter without dt partition
SELECT * FROM events
WHERE from_iso8601_timestamp(`@timestamp`) >= date '2025-02-01';
```

**Cost impact**: Without partition pruning, a query can scan **100x more data**.

## Common Query Patterns

### 1. Event Counts by Day

```sql
SELECT
    dt,
    event_type,
    count(*) as event_count
FROM events
WHERE dt BETWEEN '2025-01-01' AND '2025-02-15'
GROUP BY 1, 2
ORDER BY 1, 2;
```

### 2. Top Members by Activity

```sql
SELECT
    memberCode,
    count(DISTINCT gameUid) as unique_games,
    count(*) as total_events,
    count(DISTINCT dt) as active_days
FROM events
WHERE event_type = 'GameOpened'
  AND dt BETWEEN '2025-02-01' AND '2025-02-15'
  AND memberCode IS NOT NULL
GROUP BY 1
HAVING count(*) > 100
ORDER BY total_events DESC
LIMIT 100;
```

### 3. Error Analysis

```sql
SELECT
    errorCode,
    platform,
    count(*) as error_count,
    count(DISTINCT memberCode) as affected_members,
    approx_percentile(
        cast(regexp_extract(`@timestamp`, '\\.(\d{3})Z', 1) as double),
        0.5
    ) as median_ms
FROM events
WHERE event_type = 'apiError'
  AND dt = '2025-02-15'
  AND errorCode IS NOT NULL
GROUP BY 1, 2
ORDER BY error_count DESC;
```

### 4. Game Popularity Over Time

```sql
SELECT
    dt,
    gameUid,
    gameName,
    count(DISTINCT memberCode) as unique_players,
    count(*) as total_plays
FROM events
WHERE event_type = 'GameOpened'
  AND dt BETWEEN '2025-02-01' AND '2025-02-15'
  AND gameUid IS NOT NULL
GROUP BY 1, 2, 3
ORDER BY 1, total_plays DESC;
```

### 5. Member Journey (Cross-Event)

```sql
WITH member_events AS (
    SELECT
        memberCode,
        event_type,
        from_iso8601_timestamp(`@timestamp`) as ts,
        gameUid,
        errorCode
    FROM events
    WHERE memberCode = 'M12345'
      AND dt BETWEEN '2025-02-14' AND '2025-02-15'
    ORDER BY ts
)
SELECT * FROM member_events
ORDER BY ts;
```

### 6. Device Sharing Detection

```sql
SELECT
    fingerprint,
    count(DISTINCT memberCode) as member_count,
    array_agg(DISTINCT memberCode) as members
FROM events
WHERE event_type = 'Login'
  AND dt BETWEEN '2025-02-01' AND '2025-02-15'
  AND fingerprint IS NOT NULL
GROUP BY 1
HAVING count(DISTINCT memberCode) > 1
ORDER BY member_count DESC
LIMIT 100;
```

## Temporal Comparisons

### Week-over-Week Change

```sql
WITH this_week AS (
    SELECT event_type, count(*) as cnt
    FROM events
    WHERE dt BETWEEN '2025-02-08' AND '2025-02-14'
    GROUP BY 1
),
last_week AS (
    SELECT event_type, count(*) as cnt
    FROM events
    WHERE dt BETWEEN '2025-02-01' AND '2025-02-07'
    GROUP BY 1
)
SELECT
    this_week.event_type,
    this_week.cnt as this_week_count,
    last_week.cnt as last_week_count,
    ((this_week.cnt - last_week.cnt) * 100.0 / last_week.cnt) as pct_change
FROM this_week
LEFT JOIN last_week ON this_week.event_type = last_week.event_type
ORDER BY pct_change DESC;
```

### Year-over-Year Comparison

```sql
SELECT
    date_format(from_iso8601_timestamp(`@timestamp`), '%m-%d') as day_of_year,
    count(*) as event_count,
    year(from_iso8601_timestamp(`@timestamp`)) as year
FROM events
WHERE event_type = 'GameOpened'
  AND dt BETWEEN '2024-02-01' AND '2025-02-15'
GROUP BY 1, 3
ORDER BY 3, 1;
```

## Cost Optimization Techniques

### Column Pruning

```sql
-- ✅ GOOD: Select only needed columns
SELECT memberCode, gameUid, `@timestamp`
FROM events
WHERE event_type = 'GameOpened' AND dt = '2025-02-15';

-- ❌ BAD: Select all columns
SELECT *
FROM events
WHERE event_type = 'GameOpened' AND dt = '2025-02-15';
```

**Impact**: Selecting 3 columns vs 13 columns = ~4x cost reduction.

### Pre-Aggregation Before Join

```sql
-- ✅ GOOD: Aggregate before join
WITH member_stats AS (
    SELECT
        memberCode,
        count(*) as play_count
    FROM events
    WHERE event_type = 'GameOpened'
      AND dt BETWEEN '2025-02-01' AND '2025-02-15'
    GROUP BY 1
)
SELECT * FROM member_stats WHERE play_count > 100;

-- ❌ BAD: Join then filter (scans more data)
SELECT memberCode, count(*)
FROM events e
WHERE event_type = 'GameOpened'
  AND dt BETWEEN '2025-02-01' AND '2025-02-15'
  AND memberCode IN (SELECT member_code FROM other_table)
GROUP BY 1;
```

### Use LIMIT for Exploration

```sql
-- Always use LIMIT when exploring
SELECT * FROM events
WHERE event_type = 'Login' AND dt = '2025-02-15'
LIMIT 100;
```

## Advanced Patterns

### Cohort Analysis

```sql
WITH first_login AS (
    SELECT
        memberCode,
        min(from_iso8601_timestamp(`@timestamp`)) as first_seen
    FROM events
    WHERE event_type = 'Login'
      AND dt BETWEEN '2025-02-01' AND '2025-02-07'
    GROUP BY 1
),
activity AS (
    SELECT
        e.memberCode,
        date_diff('day', f.first_seen, from_iso8601_timestamp(e.`@timestamp`)) as days_since_first
    FROM events e
    JOIN first_login f ON e.memberCode = f.memberCode
    WHERE e.event_type = 'GameOpened'
      AND e.dt BETWEEN '2025-02-01' AND '2025-02-15'
)
SELECT
    days_since_first,
    count(DISTINCT memberCode) as active_members
FROM activity
GROUP BY 1
ORDER BY 1;
```

### Provider Performance

```sql
SELECT
    provider,
    count(DISTINCT gameUid) as game_count,
    count(DISTINCT memberCode) as player_count,
    count(*) as total_plays,
    count(*) * 1.0 / count(DISTINCT memberCode) as plays_per_player
FROM events
WHERE event_type = 'GameOpened'
  AND dt BETWEEN '2025-02-01' AND '2025-02-15'
  AND provider IS NOT NULL
GROUP BY 1
ORDER BY total_plays DESC;
```

## Integration with Local Data

Athena queries are merged with local 30-day window data:

```rust
// Pseudo-code
let athena_baseline = athena_client.execute_query(
    "SELECT memberCode, count(*) as cnt FROM events
     WHERE event_type = 'GameOpened' AND dt BETWEEN '2024-11-01' AND '2024-11-30'
     GROUP BY 1"
).await?;

let local_current = local_query_executor.execute(
    DocumentScan {
        filter: Filter::And(vec![
            Filter::Eq("eventType", "GameOpened"),
            Filter::Range("@timestamp", "2025-02-01", "2025-02-15"),
        ]),
        projection: vec!["memberCode"],
    }
).await?;

// Compare baseline vs current
let comparison = compare_distributions(athena_baseline, local_current);
```

## Performance Targets

| Query Type | Target Time | Max Scan |
|-----------|-------------|----------|
| Exploration (LIMIT 100) | < 5s | < 100MB |
| Daily aggregation | < 15s | < 500MB |
| Weekly aggregation | < 30s | < 2GB |
| Multi-month analysis | < 60s | < 10GB |

## Error Handling

```sql
-- Handle NULL values
SELECT
    coalesce(memberCode, 'UNKNOWN') as member,
    count(*) as cnt
FROM events
WHERE event_type = 'Login' AND dt = '2025-02-15'
GROUP BY 1;

-- Safe timestamp parsing
SELECT
    CASE
        WHEN `@timestamp` IS NOT NULL
        THEN from_iso8601_timestamp(`@timestamp`)
        ELSE null
    END as ts
FROM events
WHERE dt = '2025-02-15';
```

## Best Practices Checklist

- [ ] Include `event_type` and `dt` in WHERE clause
- [ ] Use column pruning (SELECT specific columns, not *)
- [ ] Add LIMIT for exploration queries
- [ ] Use `count(DISTINCT x)` instead of `SELECT DISTINCT x` when possible
- [ ] Pre-aggregate before joins
- [ ] Test queries on small date ranges first
- [ ] Monitor `bytes_scanned` in query metadata
- [ ] Use Athena result caching (identical queries are free)
