---
name: AWS Integration
description: Complete AWS integration guide for stupid-db. Athena historical queries, Aurora/RDS enrichment, S3 remote parquet reading, credential management, and cost control. Use when working with AWS services or external data sources.
version: 1.0.0
---

# AWS Integration

## Overview

stupid-db integrates with AWS to:
1. **Query historical data** via Athena (S3 parquet, no import)
2. **Enrich entities** from Aurora/RDS (member profiles, game catalogs)
3. **Read remote parquet** directly from S3 (without Athena)
4. **Archive results** to S3 (computed knowledge, reports)

## Architecture

```
Local stupid-db (30-day window)
    ↓
    ├─→ AWS Athena (historical queries, S3 parquet)
    ├─→ Aurora/RDS (relational reference data)
    └─→ S3 Direct (remote parquet files)
```

## AWS Athena Integration

### Purpose
Query **archival data** in S3 without importing. Serverless, pay-per-query.

### Configuration (.env)
```bash
ATHENA_ACCESS_KEY=AKIA6EBOOQIN2HEWUGKP
ATHENA_ACCESS_KEY_SECRET=fgwq...
ATHENA_REGION=ap-northeast-1
ATHENA_S3_BUCKET_OUTPUT=s3://services-athena-query-output-s3-9707
ATHENA_LOCAL_CACHE_DIR=/mnt/camelot/duckdb/athena
ATHENA_LOCAL_CACHE_EXPIRE=360000  # 100 hours
```

### Use Cases
- Compare current trends to 6 months ago
- Analyze multi-year historical patterns
- Federated queries (local + Athena results)
- Ad-hoc exploration of old data

### Query Example
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

### Cost Control
- **Max scan limit**: 10GB default (configurable)
- **Partition pruning**: Always include `event_type` and `dt` in WHERE
- **Column pruning**: SELECT only needed columns
- **Result caching**: Athena caches for 24h (free re-runs)

### Rust API
```rust
use athena::{AthenaClient, AthenaConfig};

let config = AthenaConfig::from_env();
let client = AthenaClient::new(config).await?;

let result = client.execute_query_with_limit(
    "SELECT * FROM events WHERE dt = '2025-02-15' LIMIT 100",
    10 * 1024 * 1024 * 1024  // 10GB limit
).await?;

println!("Rows: {}, Bytes scanned: {}",
    result.rows.len(),
    result.metadata.bytes_scanned
);
```

## Aurora / RDS Integration

### Purpose
Read **relational reference data** to enrich graph entities.

### Configuration
```toml
[aws.aurora]
enabled = true
engine = "postgresql"
host = "cluster.ap-northeast-1.rds.amazonaws.com"
port = 5432
database = "crm"
username_env = "AURORA_USERNAME"
password_env = "AURORA_PASSWORD"
pool_min = 2
pool_max = 10
ssl_mode = "require"
```

### Sync Modes

| Mode | Frequency | Use Case |
|------|-----------|----------|
| **Full sync** | Daily | Complete reference table refresh |
| **Incremental** | 15 min | Only changed rows since last sync |
| **On-demand** | Per query | Fetch specific records when needed |

### Schema Mapping
Map relational columns to graph entity properties:

```toml
[[aws.aurora.mappings]]
table = "members"
entity_type = "Member"
key_column = "member_code"
properties = [
    { column = "registration_date", property = "registered_at" },
    { column = "total_deposit", property = "total_deposit" },
    { column = "account_status", property = "status" },
]
sync_mode = "incremental"
sync_key = "updated_at"
```

### Enrichment Flow
```
Aurora/RDS → Reference Cache → Entity Properties → Graph Store
```

Member nodes gain properties like:
- `registered_at`: When account was created
- `total_deposit`: Lifetime deposit amount
- `status`: active | suspended | closed
- `email_verified`, `phone_verified`: Verification status

### Connection Pooling
Uses `bb8` async connection pool:
- Min connections kept warm
- Max connection limit
- Auto-reconnect on failure
- Health checks on idle connections

## S3 Direct Integration

### Purpose
Read parquet files directly from S3 without Athena (when full scans are acceptable).

### Configuration
```toml
[aws.s3]
region = "ap-northeast-1"

[[aws.s3.sources]]
bucket = "w88-analytics"
prefix = "events/"
event_type_from_path = true  # events/Login/2025-02-15.parquet
pattern = "**/*.parquet"
```

### S3 Parquet Reader
```rust
struct S3ParquetReader {
    client: aws_sdk_s3::Client,
    bucket: String,
    key: String,
}

impl RemoteReader for S3ParquetReader {
    async fn read_range(&self, offset: u64, length: u64) -> Result<Bytes> {
        let range = format!("bytes={}-{}", offset, offset + length - 1);
        let resp = self.client
            .get_object()
            .bucket(&self.bucket)
            .key(&self.key)
            .range(range)
            .send()
            .await?;
        Ok(resp.body.collect().await?.into_bytes())
    }
}
```

### Use Cases
- Import historical parquet from S3
- Register S3 paths as external segments
- Archive computed results to S3
- Read recent data uploaded to S3 (before Athena table registration)

## AWS Authentication

Uses standard AWS credential chain:
1. Environment variables (`AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`)
2. AWS SSO cache (`~/.aws/sso/`)
3. AWS config file (`~/.aws/credentials`)
4. EC2 instance profile
5. ECS task role

**No credentials stored in stupid-db config.** All auth via standard AWS SDK.

## Catalog Integration

External sources appear in the knowledge catalog:

```json
{
  "external_sources": {
    "athena": {
      "enabled": true,
      "database": "analytics",
      "tables": ["events"],
      "estimated_data_gb": 450,
      "retention_days": 2000
    },
    "aurora": {
      "enabled": true,
      "database": "crm",
      "tables": ["members", "games", "affiliates"],
      "sync_mode": "incremental",
      "last_sync": "2025-02-15T14:00:00Z"
    },
    "s3_sources": [{
      "bucket": "w88-analytics",
      "prefix": "events/",
      "file_count": 8400,
      "total_size_gb": 1200
    }]
  }
}
```

The LLM sees these sources in the catalog summary and can generate query plans that use them.

## Query Plan Integration

### Athena Query Step
```json
{
  "type": "AthenaQuery",
  "sql": "SELECT memberCode, count(*) FROM events WHERE event_type = 'GameOpened' AND dt BETWEEN '2024-11-01' AND '2024-11-30' GROUP BY 1",
  "max_scan_gb": 5.0
}
```

### Aurora Query Step
```json
{
  "type": "AuroraQuery",
  "sql": "SELECT member_code, total_deposit FROM members WHERE member_code = ANY($1)",
  "bind": { "$1": { "ref": "s1", "field": "memberCode" } },
  "database": "crm"
}
```

### S3 Parquet Read Step
```json
{
  "type": "S3ParquetRead",
  "bucket": "w88-analytics",
  "key": "events/Login/2025-01-15.parquet"
}
```

## Cost Monitoring

Track AWS usage:
- **Athena**: Bytes scanned per query, cumulative monthly scan
- **Aurora/RDS**: Connection count, query duration
- **S3**: GET requests, data transfer out

Dashboard shows:
- Athena cost per query (based on scan bytes)
- Monthly Athena spend trend
- RDS connection pool utilization
- S3 transfer costs

## Performance Targets

| Operation | Target | Notes |
|-----------|--------|-------|
| Athena query | < 30s | For queries under 1GB scan |
| Aurora sync (incremental) | < 5s | For <10K changed rows |
| S3 parquet read | < 10s | For 100MB file |

## Security Best Practices

- **Use IAM roles** when running on EC2/ECS (no static credentials)
- **Enable SSL** for Aurora/RDS connections
- **Restrict S3 bucket access** to minimum required paths
- **Rotate credentials** regularly
- **Enable CloudTrail** for audit logging
- **Use VPC endpoints** for S3/Athena (avoid internet egress costs)

## Rust Crate Dependencies

```toml
# crates/athena/Cargo.toml
[dependencies]
aws-config = "1"
aws-sdk-athena = "1"
aws-sdk-s3 = "1"
tokio = { version = "1", features = ["full"] }
tracing = "0.1"
thiserror = "2"

# For Aurora/RDS (future crate or in server)
sqlx = { version = "0.8", features = ["postgres", "mysql", "runtime-tokio"] }
bb8 = "0.8"
```

## Common Patterns

### Temporal Baseline Comparison
```rust
// Fetch historical baseline from Athena
let athena_result = athena_client.execute_query(
    "SELECT memberCode, count(*) as play_count
     FROM events
     WHERE event_type = 'GameOpened' AND dt BETWEEN '2024-11-01' AND '2024-11-30'
     GROUP BY 1"
).await?;

// Fetch current data from local store
let local_result = segment_reader.scan(
    Filter::and(vec![
        Filter::eq("eventType", "GameOpened"),
        Filter::range("@timestamp", "2025-02-01", "2025-02-15"),
    ])
).await?;

// Compare distributions
let comparison = compare_member_activity(athena_result, local_result);
```

### Member Profile Enrichment
```rust
// Sync member profiles from Aurora
let members = aurora_client.query(
    "SELECT member_code, registration_date, total_deposit, account_status
     FROM members
     WHERE updated_at > $1",
    vec![last_sync_timestamp]
).await?;

// Enrich graph nodes
for member in members {
    graph_store.update_node_properties(
        NodeId::member(&member.member_code),
        hashmap! {
            "registered_at" => member.registration_date,
            "total_deposit" => member.total_deposit,
            "status" => member.account_status,
        }
    ).await?;
}
```
