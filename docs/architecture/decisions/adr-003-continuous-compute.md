# ADR-003: Continuous Compute over On-Demand Query

## Status
**Accepted**

## Context

The system needs to surface insights from event data: clusters, anomalies, patterns, relationships. These could be computed on-demand (at query time) or continuously (in the background).

## Decision

Algorithms run **continuously in the background**, materializing results that are ready for instant querying. The system is a **knowledge materialization engine**, not a query engine.

## Rationale

### Query Latency
- K-means on 28M vectors takes minutes — unacceptable for interactive queries
- PageRank on 30M edges takes seconds to minutes
- Pre-computing means queries read materialized results in milliseconds
- Users ask natural language questions — they expect fast answers

### Data Freshness
- New data arrives continuously
- Clusters, anomalies, and patterns should update as data arrives
- Continuous compute keeps materialized knowledge fresh
- Tradeoff: results may lag by minutes, not real-time to the millisecond

### Resource Utilization
- Continuous compute uses idle CPU between query bursts
- Better utilization than spike-on-query pattern
- Predictable resource consumption — easier capacity planning
- Backpressure is manageable: slow down ingest, not user queries

### Proactive Insights
- The system can surface anomalies it discovers, without being asked
- "Alert: unusual login pattern detected" — computed, not queried
- This is impossible with on-demand-only compute

## Consequences

- Higher baseline CPU usage (always computing)
- Results may be slightly stale (lag between ingest and compute)
- Need a scheduler to prioritize compute tasks
- Need to store materialized results alongside raw data
- Significant advantage for interactive exploration via dashboard

## Hybrid Approach

Some operations are still on-demand:
- Document scans with arbitrary filters
- Graph traversals with custom starting points
- Vector similarity search (HNSW is fast enough)

The continuous compute handles the expensive stuff: clustering, community detection, pattern mining, anomaly detection.
