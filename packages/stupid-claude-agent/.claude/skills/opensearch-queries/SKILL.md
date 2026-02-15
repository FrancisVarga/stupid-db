---
name: OpenSearch Queries
description: OpenSearch query patterns for stupid-db data exploration. Query DSL, aggregations, filters, and the Windmill MCP tools available for querying. Use when analyzing production data, writing queries, or exploring event patterns.
version: 1.0.0
---

# OpenSearch Queries

## Available Tools (via Windmill MCP)

| Tool | Purpose |
|------|---------|
| `mcp__windmill__s-f_mcp__tools_query__opensearch` | Basic document queries |
| `mcp__windmill__s-f_mcp__tools_opeansearch__query__aggregration` | Aggregation queries |
| `mcp__windmill__s-f_mcp__tools_opensearch__schema__docs` | Schema documentation |

All tools accept a `query` parameter with OpenSearch Query DSL.

## Common Query Patterns

### Count Events by Type
```json
{
  "query": { "match_all": {} },
  "size": 0,
  "aggs": {
    "event_types": {
      "terms": { "field": "eventType.keyword", "size": 20 }
    }
  }
}
```

### Filter by Time Range
```json
{
  "query": {
    "range": {
      "@timestamp": {
        "gte": "now-24h",
        "lte": "now"
      }
    }
  }
}
```

### Member Activity
```json
{
  "query": {
    "bool": {
      "must": [
        { "term": { "memberCode.keyword": "MEMBER_ID" } },
        { "range": { "@timestamp": { "gte": "now-7d" } } }
      ]
    }
  },
  "sort": [{ "@timestamp": "asc" }],
  "size": 100
}
```

### Top Games by Play Count
```json
{
  "query": {
    "bool": {
      "must": [
        { "term": { "eventType.keyword": "GameOpened" } },
        { "range": { "@timestamp": { "gte": "now-7d" } } }
      ]
    }
  },
  "size": 0,
  "aggs": {
    "top_games": {
      "terms": { "field": "gameUid.keyword", "size": 50 },
      "aggs": {
        "unique_players": {
          "cardinality": { "field": "memberCode.keyword" }
        }
      }
    }
  }
}
```

### Error Distribution
```json
{
  "query": { "term": { "eventType.keyword": "apiError" } },
  "size": 0,
  "aggs": {
    "by_error_code": {
      "terms": { "field": "errorCode.keyword", "size": 50 },
      "aggs": {
        "by_platform": {
          "terms": { "field": "platform.keyword" }
        },
        "over_time": {
          "date_histogram": {
            "field": "@timestamp",
            "calendar_interval": "hour"
          }
        }
      }
    }
  }
}
```

### Device Sharing Detection (Potential Fraud)
```json
{
  "query": { "term": { "eventType.keyword": "Login" } },
  "size": 0,
  "aggs": {
    "by_fingerprint": {
      "terms": { "field": "fingerprint.keyword", "size": 100, "min_doc_count": 2 },
      "aggs": {
        "unique_members": {
          "cardinality": { "field": "memberCode.keyword" }
        },
        "member_list": {
          "terms": { "field": "memberCode.keyword", "size": 10 }
        }
      }
    }
  }
}
```

### Hourly Activity Pattern
```json
{
  "query": {
    "range": { "@timestamp": { "gte": "now-7d" } }
  },
  "size": 0,
  "aggs": {
    "hourly": {
      "date_histogram": {
        "field": "@timestamp",
        "calendar_interval": "hour"
      },
      "aggs": {
        "by_event_type": {
          "terms": { "field": "eventType.keyword" }
        }
      }
    }
  }
}
```

### Member Journey (Cross-Event)
```json
{
  "query": {
    "bool": {
      "must": [
        { "term": { "memberCode.keyword": "MEMBER_ID" } },
        { "range": { "@timestamp": { "gte": "now-1d" } } }
      ]
    }
  },
  "sort": [{ "@timestamp": "asc" }],
  "size": 500,
  "_source": ["eventType", "memberCode", "gameUid", "errorCode", "rGroup", "@timestamp"]
}
```

## Field Types

Most text fields have both `.keyword` (exact match) and analyzed (full-text) versions:
- Use `.keyword` for `term`, `terms`, aggregations
- Use base field name for `match` (full-text search)
- `@timestamp` is a date type — use `range` queries

## Aggregation Types

| Type | Use Case |
|------|----------|
| `terms` | Top-N values (cardinality grouping) |
| `date_histogram` | Time series buckets |
| `cardinality` | Unique count (HyperLogLog) |
| `avg`, `sum`, `min`, `max` | Numeric statistics |
| `percentiles` | Distribution analysis |
| `filters` | Named bucket filters |
| `nested` | Sub-aggregations within parent buckets |

## Tips

- Always set `size: 0` for aggregation-only queries (faster, no document results)
- Use `min_doc_count` to filter low-frequency buckets
- For large cardinality fields, use `cardinality` agg instead of `terms` with high size
- Date histograms: use `calendar_interval` for hour/day/month, `fixed_interval` for 5m/30m
- Combine `bool` → `must` for AND conditions, `should` for OR
