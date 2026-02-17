---
name: rule-authoring
description: Complete YAML schema reference for writing rules in stupid-db — all 6 kinds with field-level detail
triggers:
  - write rule
  - create rule
  - new rule
  - YAML schema
  - rule template
  - rule example
  - authoring
---

# Rule Authoring Guide

## Overview

Rules are YAML files in `data/rules/` loaded by RuleLoader with recursive directory scanning. All rules share a common header (apiVersion, kind, metadata) and have kind-specific bodies.

## Common Header (All Rule Kinds)

```yaml
apiVersion: v1
kind: AnomalyRule | EntitySchema | FeatureConfig | ScoringConfig | TrendConfig | PatternConfig
metadata:
  id: unique-rule-id          # Required, globally unique
  name: Human Readable Name   # Required
  description: |               # Optional, multiline
    What this rule does
  tags: [tag1, tag2]          # Optional
  enabled: true               # Default true, set false to disable
  extends: parent-rule-id     # Optional, inherit from parent
```

## 1. AnomalyRule — Detection Rules

**Location**: `data/rules/anomaly/`

Two detection modes: **template** (simple) or **compose** (complex). Never both.

### Template Mode (Simple Detection)

```yaml
apiVersion: v1
kind: AnomalyRule
metadata:
  id: login-spike
  name: Login Activity Spike
  tags: [login, spike]
  enabled: true
schedule:
  cron: "*/15 * * * *"
  timezone: UTC
  cooldown: "30m"             # Optional, suppress re-alerts
detection:
  template: spike             # spike | threshold | absence | drift
  params:
    feature: login_count
    multiplier: 3.0
    baseline: cluster_centroid  # or population_mean
    min_samples: 5
filters:
  entity_types: [Member]
  min_score: 0.7
notifications:
  - channel: webhook
    on: [trigger]
    url: ${WEBHOOK_URL}
    method: POST
    headers: { Content-Type: application/json }
    body_template: |
      {"rule": "{{ rule_id }}", "entity": "{{ entity_key }}", "score": {{ score }}}
```

### Template Parameters

| Template | Params | Description |
|----------|--------|-------------|
| **spike** | feature, multiplier, baseline, min_samples | Feature exceeds N× baseline |
| **threshold** | feature, operator (gt/gte/lt/lte/eq/neq), value | Feature crosses absolute value |
| **absence** | feature, threshold, lookback_days | Feature is zero/null for N days |
| **drift** | features (list), method (cosine), threshold | Behavioral vector deviation |

### Compose Mode (Complex Logic)

```yaml
detection:
  compose:
    operator: and
    conditions:
      - signal: z_score
        threshold: 3.0
      - operator: or               # Nested boolean logic
        conditions:
          - signal: dbscan_noise
            threshold: 0.6
          - signal: graph_anomaly
            threshold: 0.5
```

### Signal Types

| Signal | Key | Meaning | Typical Range |
|--------|-----|---------|---------------|
| ZScore | z_score | Statistical deviation | 2.5 – 3.0 |
| DbscanNoise | dbscan_noise | Cluster noise ratio | 0.5 – 0.7 |
| BehavioralDeviation | behavioral_deviation | Cosine distance | 0.3 – 0.5 |
| GraphAnomaly | graph_anomaly | Topology anomaly | 0.4 – 0.6 |

### Enrichment (Post-Detection)

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
      min_hits: 20
      rate_limit: 30            # Max queries/minute
      timeout_ms: 5000
```

Template variables: `{{ anomaly.key }}`, `{{ anomaly.entity_type }}`

### Filters

```yaml
filters:
  entity_types: [Member, Device]     # Only these entity types
  classifications: [Anomalous]       # Only these severity classes
  min_score: 0.7                     # Minimum anomaly score
  exclude_keys: [M000, M001]         # Skip specific entities
  where:                              # Numeric feature conditions
    vip_group_numeric:
      gte: 4.0
    error_count:
      gt: 50
```

### Notification Channels

```yaml
notifications:
  # Webhook
  - channel: webhook
    on: [trigger, resolve]
    url: ${WEBHOOK_URL}
    method: POST
    headers: { Content-Type: application/json }
    body_template: |
      {"rule": "{{ rule_id }}", "entity": "{{ entity_key }}", "score": {{ score }}}

  # Email
  - channel: email
    on: [trigger]
    smtp_host: ${SMTP_HOST}
    smtp_port: 587
    tls: true
    from: ${SMTP_FROM}
    to: [${ALERT_EMAIL}]
    subject: "Alert: {{ rule_id }}"
    template: "Entity {{ entity_key }} scored {{ score }}"

  # Telegram
  - channel: telegram
    on: [trigger]
    bot_token: ${TELEGRAM_BOT_TOKEN}
    chat_id: ${TELEGRAM_CHAT_ID}
    parse_mode: MarkdownV2
```

Template variables: `{{ rule_id }}`, `{{ entity_key }}`, `{{ score }}`, `{{ summary }}`, `{{ z_score }}`, `{{ dbscan_noise }}`, `{{ graph_anomaly }}`, `{{ value }}`, `{{ event }}`, `{{ last_seen }}`, `{{ vip_group }}`

### Schedule

```yaml
schedule:
  cron: "*/15 * * * *"    # Every 15 minutes (5-field cron)
  timezone: "UTC"
  cooldown: "30m"          # Formats: "30m", "1h", "1d2h30m15s", "120" (secs)
```

## 2. EntitySchema — Entity & Edge Definitions

**Location**: `data/rules/schema/`

```yaml
apiVersion: v1
kind: EntitySchema
metadata:
  id: entity-schema-default
  name: W88 Entity Schema
  tags: [schema]
  enabled: true
spec:
  null_values: ["None", "null", "undefined", ""]

  entity_types:
    - name: Member
      key_prefix: "member:"
    - name: Device
      key_prefix: "device:"
    # 10 types total: Member, Device, Game, Affiliate, Currency,
    #   VipGroup, Error, Platform, Popup, Provider

  edge_types:
    - name: LoggedInFrom
      from: Member
      to: Device
    # 9 edges total: LoggedInFrom, OpenedGame, SawPopup, HitError,
    #   BelongsToGroup, ReferredBy, UsesCurrency, PlaysOnPlatform, ProvidedBy

  field_mappings:
    - field: memberCode
      aliases: [playerId, memberId]
      entity_type: Member
      key_prefix: "member:"

  event_extraction:
    Login:
      aliases: []
      entities:
        - field: memberCode
          entity_type: Member
          fallback_fields: []
        - field: deviceId
          entity_type: Device
          fallback_fields: [fingerprint]
      edges:
        - from_field: memberCode
          to_field: deviceId
          edge: LoggedInFrom

  embedding_templates:
    Login: "{memberCode} logged in from {deviceId} on {platform|truncate:50}"
```

**Embedding template syntax**: `{field}` for substitution, `{field|truncate:N}` for truncated output.

## 3. FeatureConfig — Feature Vector Definition

**Location**: `data/rules/features/`

```yaml
apiVersion: v1
kind: FeatureConfig
metadata:
  id: feature-config-default
  name: W88 Feature Configuration
  tags: [features]
  enabled: true
spec:
  features:                         # 10 dimensions, index-ordered
    - name: login_count
      index: 0
    - name: game_count
      index: 1
    - name: unique_games
      index: 2
    - name: error_count
      index: 3
    - name: popup_count
      index: 4
    - name: platform_mobile_ratio
      index: 5
    - name: session_count
      index: 6
    - name: avg_session_gap_hours
      index: 7
    - name: vip_group
      index: 8
    - name: currency
      index: 9

  vip_encoding:                     # Categorical → numeric
    bronze: 1.0
    silver: 2.0
    gold: 3.0
    platinum: 4.0
    diamond: 5.0
    vip: 6.0
  vip_fallback: hash_based          # or: default: 0.0

  currency_encoding:
    USD: 1.0
    EUR: 2.0
    GBP: 3.0
    CNY: 4.0
    JPY: 5.0
    VND: 6.0
    THB: 7.0
    IDR: 8.0
    MYR: 9.0
    PHP: 10.0
  currency_fallback: hash_based

  event_classification:
    login: [login, Login, signin]
    game: [GameOpened, game, play]
    error: [error, Error, "API Error"]
    popup: [PopupModule, popup, PopUpModule]

  mobile_keywords: [mobile, android, ios, iphone, ipad, tablet]

  event_compression:                # For PrefixSpan pattern mining
    Login:
      code: L
      subtype_field: null
    GameOpened:
      code: G
      subtype_field: game
    PopupModule:
      code: P
      subtype_field: popupType
    "API Error":
      code: E
      subtype_field: null
```

## 4. ScoringConfig — Anomaly Scoring Weights

**Location**: `data/rules/scoring/`

```yaml
apiVersion: v1
kind: ScoringConfig
metadata:
  id: scoring-config-default
  name: Default Scoring Configuration
  tags: [scoring]
  enabled: true
spec:
  multi_signal_weights:             # Must sum to ~1.0
    statistical: 0.2                # Z-score weight
    dbscan_noise: 0.3              # Cluster noise weight
    behavioral: 0.3                # Behavioral drift weight
    graph: 0.2                     # Graph topology weight

  classification_thresholds:
    mild: 0.3                      # Score < 0.3 = Normal
    anomalous: 0.5                 # 0.3-0.5 = Mild, 0.5-0.7 = Anomalous
    highly_anomalous: 0.7          # Score > 0.7 = Highly Anomalous

  z_score_normalization:
    divisor: 5.0                   # max(|z|) / divisor → [0, 1]

  graph_anomaly:
    neighbor_multiplier: 3.0       # Device proliferation: >3× avg neighbors
    high_connectivity_score: 0.5   # Score addition for proliferation
    community_threshold: 3         # Cross-community trigger count
    multi_community_score: 0.3     # Score addition for cross-community

  default_anomaly_threshold: 2.0
```

## 5. TrendConfig — Trend Detection Parameters

**Location**: `data/rules/scoring/`

```yaml
apiVersion: v1
kind: TrendConfig
metadata:
  id: trend-config-default
  name: Default Trend Configuration
  tags: [trends]
  enabled: true
spec:
  default_window_size: 168          # 7 days × 24 hours
  min_data_points: 3                # Minimum for valid z-score
  z_score_trigger: 2.0             # |z| must exceed this

  direction_thresholds:
    up: 0.5                        # z > 0.5 → Up trend
    down: 0.5                      # z < -0.5 → Down trend

  severity_thresholds:
    notable: 2.0                   # |z| > 2.0
    significant: 3.0               # |z| > 3.0
    critical: 4.0                  # |z| > 4.0
```

## 6. PatternConfig — Temporal Pattern Detection

**Location**: `data/rules/patterns/`

```yaml
apiVersion: v1
kind: PatternConfig
metadata:
  id: pattern-config-default
  name: Default Pattern Configuration
  tags: [patterns]
  enabled: true
spec:
  prefixspan_defaults:
    min_support: 0.01              # 1% minimum support ratio
    max_length: 10                 # Max sequence length
    min_members: 50                # Min sequences to analyze

  classification_rules:            # Evaluated in order, first match wins
    - category: ErrorChain
      condition:
        check: count_gte
        event_code: E
        min_count: 2

    - category: Churn
      condition:
        check: has_then_absent
        present_code: E
        absent_code: G

    - category: Funnel
      condition:
        check: sequence_match
        sequence: [L, G]

    - category: Engagement
      condition:
        check: count_gte
        event_code: G
        min_count: 2

    - category: Unknown            # Fallback, must be last
      condition:
        check: count_gte
        event_code: ""
        min_count: 0
```

## extends / Inheritance

```yaml
# Parent rule (base-spike.yml)
apiVersion: v1
kind: AnomalyRule
metadata:
  id: base-spike
  name: Base Spike Template
  enabled: true
schedule:
  cron: "*/15 * * * *"
  timezone: UTC
detection:
  template: spike
  params:
    baseline: cluster_centroid
    min_samples: 5
notifications:
  - channel: webhook
    on: [trigger]
    url: ${WEBHOOK_URL}

# Child rule (login-spike-strict.yml)
apiVersion: v1
kind: AnomalyRule
metadata:
  id: login-spike-strict
  name: Strict Login Spike
  extends: base-spike             # Inherits everything from base-spike
detection:
  params:
    feature: login_count
    multiplier: 5.0               # Override: stricter threshold
```

**Merge rules**: Scalars → child wins. Maps → recursive merge. Arrays → child replaces entirely. Max depth: 5 levels.

## Environment Variables

All string fields support `${VAR_NAME}` substitution at runtime:
- `${WEBHOOK_URL}`, `${SMTP_HOST}`, `${SMTP_FROM}`, `${ALERT_EMAIL}`
- `${TELEGRAM_BOT_TOKEN}`, `${TELEGRAM_CHAT_ID}`
- `${API_TOKEN}` (in headers)

## File Naming Convention

- Use kebab-case: `login-spike.yml`, `multi-signal-fraud.yml`
- Extensions: `.yml` or `.yaml` (both supported)
- Place in correct subdirectory for the rule kind
- RuleLoader scans recursively — subdirectories are auto-discovered

## Quick Reference: Feature Names

The 10-dimensional feature vector (from FeatureConfig):
`login_count`, `game_count`, `unique_games`, `error_count`, `popup_count`, `platform_mobile_ratio`, `session_count`, `avg_session_gap_hours`, `vip_group`, `currency`
