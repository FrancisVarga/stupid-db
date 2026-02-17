---
name: anomaly-detection-patterns
description: Anomaly detection patterns — templates, composition trees, multi-signal scoring, enrichment, and evaluation flow
triggers:
  - anomaly detection
  - detection template
  - signal scoring
  - compose rule
  - enrichment
  - rule evaluation
---

# Anomaly Detection Patterns

## Evaluation Flow

```
RuleScheduler.due_rules(now)
    → for each due rule:
        RuleEvaluator.evaluate(rule, entities, cluster_stats, signal_scores)
            → Check rule.enabled
            → Dispatch to template OR composition evaluator
            → Apply post-detection filters
            → Return Vec<RuleMatch>
        EnrichmentEngine.enrich(rule_id, config, match)
            → Rate limit check (token bucket)
            → OpenSearch query with template variables
            → Hit bounds evaluation (min_hits, max_hits)
            → Return EnrichmentResult { passed, hit_count, sample_hits }
        Notify via configured channels
```

## Detection Templates

### Spike Detection
**Use when**: A feature value exceeds a multiple of the baseline (cluster centroid or population mean).

```yaml
detection:
  template: spike
  params:
    feature: login_count
    multiplier: 3.0                 # Alert when 3× above baseline
    baseline: cluster_centroid      # or population_mean
    min_samples: 5                  # Need 5+ observations
```

**Algorithm**: `entity.features[feature_index] > multiplier × baseline_value`

Baselines:
- `cluster_centroid` — Uses the entity's assigned K-Means cluster centroid
- `population_mean` — Uses the global mean across all entities

### Threshold Detection
**Use when**: A feature crosses an absolute numeric boundary.

```yaml
detection:
  template: threshold
  params:
    feature: error_count
    operator: gte                   # gt, gte, lt, lte, eq, neq
    value: 100
```

**Algorithm**: `entity.features[feature_index] <operator> value`

### Absence Detection
**Use when**: An entity's activity drops to zero over a lookback period.

```yaml
detection:
  template: absence
  params:
    feature: login_count
    threshold: 0                    # What counts as "absent"
    lookback_days: 7
```

**Algorithm**: `entity.features[feature_index] <= threshold` for `lookback_days` continuous days

### Drift Detection
**Use when**: An entity's behavioral vector diverges from its cluster centroid.

```yaml
detection:
  template: drift
  params:
    features: [login_count, game_count, unique_games, error_count]
    method: cosine                  # Cosine distance
    threshold: 0.4                  # Distance threshold
```

**Algorithm**: `cosine_distance(entity.features[selected], centroid[selected]) > threshold`

## Composition Trees (Boolean Logic)

### Simple AND
All signals must exceed their thresholds:

```yaml
detection:
  compose:
    operator: and
    conditions:
      - signal: z_score
        threshold: 3.0
      - signal: dbscan_noise
        threshold: 0.6
```

### Simple OR
At least one signal exceeds threshold:

```yaml
detection:
  compose:
    operator: or
    conditions:
      - signal: graph_anomaly
        threshold: 0.5
      - signal: dbscan_noise
        threshold: 0.6
```

### Nested (AND with inner OR)
Complex boolean trees — no depth limit:

```yaml
detection:
  compose:
    operator: and
    conditions:
      - signal: z_score
        threshold: 3.0
      - operator: or
        conditions:
          - signal: dbscan_noise
            threshold: 0.6
          - signal: graph_anomaly
            threshold: 0.5
```

**Reads as**: z_score > 3.0 AND (dbscan_noise > 0.6 OR graph_anomaly > 0.5)

## Multi-Signal Scoring

Four signals combine into a final anomaly score using configurable weights:

| Signal | Source | What It Measures |
|--------|--------|-----------------|
| **statistical** (z_score) | Z-score from feature distribution | How far from the mean |
| **dbscan_noise** | DBSCAN clustering | Whether entity is cluster noise |
| **behavioral** | Cosine distance from centroid | Behavioral drift over time |
| **graph** | Graph topology analysis | Device proliferation, cross-community |

**Scoring formula**:
```
final_score = statistical × w1 + dbscan_noise × w2 + behavioral × w3 + graph × w4
```

Where weights come from ScoringConfig (default: 0.2, 0.3, 0.3, 0.2).

**Classification** (from ScoringConfig thresholds):
- Score < 0.3 → Normal
- 0.3 – 0.5 → Mild
- 0.5 – 0.7 → Anomalous
- Score > 0.7 → Highly Anomalous

## Graph Anomaly Scoring

Two sub-signals from graph topology:

1. **Device Proliferation**: Entity has > neighbor_multiplier × avg_neighbor_count devices
   - Adds `high_connectivity_score` (default 0.5) to graph signal

2. **Cross-Community**: Entity appears in > community_threshold distinct Louvain communities
   - Adds `multi_community_score` (default 0.3) to graph signal

Parameters from ScoringConfig `graph_anomaly` section.

## Enrichment Patterns

Post-detection enrichment queries OpenSearch for additional evidence:

### Basic Enrichment
```yaml
detection:
  compose: { ... }
  enrich:
    opensearch:
      query:
        bool:
          must:
            - match: { memberCode: "{{ anomaly.key }}" }
          filter:
            - range: { "@timestamp": { gte: "now-24h" } }
      min_hits: 20                  # Must have 20+ matching docs
      rate_limit: 30                # Max 30 queries/hour per rule
      timeout_ms: 5000
```

### Enrichment Behavior
- **Fail-open**: If OpenSearch is unavailable, enrichment passes (doesn't block alerts)
- **Rate limiting**: Token bucket per rule, based on max_per_hour
- **Hit bounds**: `min_hits` and `max_hits` define valid range. Outside range → enrichment fails → alert suppressed
- **Template variables**: `{{ anomaly.key }}` and `{{ anomaly.entity_type }}` resolved from RuleMatch

## Filter Patterns

### Entity Type Filter
Only evaluate specific entity types:
```yaml
filters:
  entity_types: [Member]           # Only Members, skip Devices etc.
```

### VIP-Only Rules
Target high-value entities:
```yaml
filters:
  entity_types: [Member]
  where:
    vip_group_numeric:
      gte: 4.0                     # Diamond (5.0) and VIP (6.0)
```

### Score Floor
Suppress low-confidence alerts:
```yaml
filters:
  min_score: 0.7                   # Only Highly Anomalous
```

### Exclusion Lists
Skip known false positives:
```yaml
filters:
  exclude_keys: [M000, M001, M999] # System/test accounts
```

## Common Rule Patterns

### Login Spike (Template)
Detect sudden login surges:
```yaml
detection:
  template: spike
  params: { feature: login_count, multiplier: 3.0, baseline: cluster_centroid, min_samples: 5 }
filters:
  entity_types: [Member]
```

### Multi-Signal Fraud (Composition + Enrichment)
Combine statistical + behavioral signals with evidence:
```yaml
detection:
  compose:
    operator: and
    conditions:
      - signal: z_score
        threshold: 2.5
      - operator: or
        conditions:
          - signal: dbscan_noise
            threshold: 0.6
          - signal: graph_anomaly
            threshold: 0.4
  enrich:
    opensearch:
      query: { bool: { must: [{ match: { memberCode: "{{ anomaly.key }}" } }] } }
      min_hits: 20
      rate_limit: 30
```

### VIP Absence (Template + Filter)
Alert when VIP members go silent:
```yaml
detection:
  template: absence
  params: { feature: login_count, threshold: 0, lookback_days: 7 }
filters:
  entity_types: [Member]
  where:
    vip_group_numeric:
      gte: 4.0
```

### Error Burst (Threshold)
Alert on error count spike:
```yaml
detection:
  template: threshold
  params: { feature: error_count, operator: gte, value: 100 }
filters:
  entity_types: [Member]
```

## Scheduler Integration

Rules run on cron schedules with cooldown:

```
RuleScheduler.sync_rules(rules)      # Load/update schedules from rules
RuleScheduler.due_rules(now)         # Get rules ready to execute
RuleScheduler.record_trigger(id)     # Mark rule as triggered (resets cooldown)
```

Cron normalized from 5-field → 6-field (seconds prepended as 0).
Cooldown parsed from human-readable: "30m", "1h", "1d2h30m15s".
