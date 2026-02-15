---
name: Anomaly Rule DSL
description: Complete YAML DSL schema reference for stupid-db anomaly detection rules. Covers rule structure, detection templates (spike/drift/absence/threshold), signal composition (AND/OR/NOT), OpenSearch enrichment, notification channels (webhook/email/telegram), scheduling, and filtering. Use when creating, editing, or validating anomaly rules.
version: 1.0.0
---

# Anomaly Rule DSL Reference

## File Location & Hot Reload

Rules live as individual YAML files in `{data_dir}/rules/`. The Rust `rules` crate watches this directory and hot-reloads on change. The dashboard writes YAML files directly through the REST API.

## Complete Schema

```yaml
apiVersion: v1                    # Required. Always "v1"
kind: AnomalyRule                 # Required. Always "AnomalyRule"

metadata:
  id: kebab-case-unique-id        # Required. Filesystem-safe slug
  name: "Human-Readable Name"     # Required. Display name
  description: "What this rule detects and why"  # Optional
  tags: [security, login, spike]  # Optional. For filtering/grouping
  enabled: true                   # Required. false = loaded but won't run

schedule:
  cron: "0 */15 * * *"           # Required. Standard cron (5-field)
  timezone: "Asia/Manila"         # Required. IANA timezone
  cooldown: "30m"                 # Optional. Suppress re-trigger duration

detection:                        # Required. ONE of: template, compose
  # --- Option A: Template-based (simple) ---
  template: spike | drift | absence | threshold
  params:
    feature: <feature_name>       # Required for most templates
    # ... template-specific params (see Templates section)

  # --- Option B: Signal composition (advanced) ---
  compose:
    operator: AND | OR | NOT
    conditions:
      - signal: z_score | dbscan_noise | behavioral_deviation | graph_anomaly
        feature: <feature_name>   # Optional (signal-dependent)
        threshold: 0.5            # Required. Trigger threshold
      - operator: OR              # Nested composition
        conditions: [...]

  # --- Optional: OpenSearch enrichment ---
  enrich:
    opensearch:
      query: { ... }             # OpenSearch Query DSL
      min_hits: 20               # Trigger only if >= N results
      max_hits: null             # Trigger only if <= N results (optional)
      rate_limit: 60             # Max queries/hour for this rule
      timeout_ms: 5000           # Query timeout

filters:
  entity_types: [Member, Device, Game, ...]  # Optional. Restrict to types
  classifications: [Anomalous, HighlyAnomalous]  # Optional. Min classification
  min_score: 0.5                 # Optional. Minimum anomaly score
  exclude_keys: ["test-*", "admin-*"]  # Optional. Glob patterns to skip
  where:                          # Optional. Feature-level filters
    vip_group_numeric: { gte: 4 }
    login_count_7d: { gt: 0 }

notifications:                    # Required. At least one channel
  - channel: webhook | email | telegram
    on: [trigger, resolve]        # When to notify
    template: "{{ jinja }}"       # Optional. Minijinja body template
    # ... channel-specific config (see Notifications section)
```

## Detection Templates

### `spike` — Sudden Increase Detection

Detects when a feature value exceeds a multiplier of the baseline.

```yaml
detection:
  template: spike
  params:
    feature: login_count_7d       # Which feature to monitor
    multiplier: 3.0               # Alert when value > baseline × multiplier
    baseline: cluster_centroid    # cluster_centroid | rolling_mean | global_mean
    min_samples: 10               # Ignore entities with fewer data points
```

### `drift` — Gradual Behavioral Change

Detects when an entity's behavior gradually shifts away from its established pattern.

```yaml
detection:
  template: drift
  params:
    features: [login_count_7d, game_count_7d, unique_games_7d]  # Multi-feature
    method: cosine_distance       # cosine_distance | euclidean
    threshold: 0.4                # Distance threshold (0.0 = identical, 1.0 = opposite)
    window: 7d                    # Recent window to compare
    baseline_window: 30d          # Historical baseline window
```

### `absence` — Missing Activity Detection

Detects when expected activity drops to zero or below a threshold.

```yaml
detection:
  template: absence
  params:
    feature: login_count_7d       # Feature to monitor
    threshold: 0                  # Alert when value <= threshold
    lookback_days: 7              # How far back to check
    compare_to: previous_period   # previous_period | rolling_mean
```

### `threshold` — Simple Value Threshold

Fires when any feature crosses a static boundary.

```yaml
detection:
  template: threshold
  params:
    feature: error_count_7d
    operator: gt | gte | lt | lte | eq | neq
    value: 100
```

## Signal Composition

Power users can combine raw detector signals with boolean logic:

```yaml
detection:
  compose:
    operator: AND
    conditions:
      - signal: z_score
        feature: login_count_7d
        threshold: 3.0
      - signal: dbscan_noise
        threshold: 0.6
      - operator: OR
        conditions:
          - signal: behavioral_deviation
            threshold: 0.7
          - signal: graph_anomaly
            threshold: 0.5
```

### Available Signals

| Signal | Description | Needs `feature`? | Weight in default |
|--------|-------------|-------------------|-------------------|
| `z_score` | Statistical outlier (z-score on feature) | Yes | 0.2 |
| `dbscan_noise` | DBSCAN noise ratio | No | 0.3 |
| `behavioral_deviation` | Cosine distance from cluster centroid | No | 0.3 |
| `graph_anomaly` | Device proliferation + cross-community | No | 0.2 |

## OpenSearch Enrichment

Optional. Runs a query against OpenSearch to augment detection with raw event data.

```yaml
detection:
  template: spike
  params:
    feature: login_count_7d
    multiplier: 3.0
  enrich:
    opensearch:
      query:
        bool:
          must:
            - term: { "memberCode.keyword": "{{ anomaly.key }}" }
            - range: { "@timestamp": { gte: "now-1h" } }
          filter:
            - term: { "action.keyword": "login" }
      min_hits: 20
      rate_limit: 60     # Max 60 OS queries per hour
      timeout_ms: 5000
```

**Cost Guard**: The `rate_limit` field caps OpenSearch queries per rule per hour. If exceeded, enrichment is skipped (rule still fires based on pipeline signals alone).

## Available Features (10-Dimensional Vector)

| Feature | Type | Description |
|---------|------|-------------|
| `login_count_7d` | count | Logins in last 7 days |
| `game_count_7d` | count | Games opened in last 7 days |
| `unique_games_7d` | count | Distinct games played |
| `error_count_7d` | count | API errors experienced |
| `popup_interaction_7d` | count | Popup interactions |
| `platform_mobile_ratio` | ratio | Mobile events / total events |
| `session_count_7d` | count | Distinct sessions |
| `avg_session_gap_hours` | hours | Average time between sessions |
| `vip_group_numeric` | encoded | VIP level (0-6) |
| `currency_encoded` | encoded | Currency code (1-8) |

## Entity Types

`Member`, `Device`, `Game`, `Popup`, `Error`, `VipGroup`, `Affiliate`, `Currency`, `Platform`, `Provider`

## Notification Channels

### Webhook (Generic HTTP)

```yaml
notifications:
  - channel: webhook
    url: "https://example.com/hooks/anomaly"
    method: POST                   # Optional. Default: POST
    headers:                       # Optional. Custom headers
      Authorization: "Bearer ${API_TOKEN}"
      X-Signature: "{{ hmac_sha256(payload, env.WEBHOOK_SECRET) }}"
    on: [trigger, resolve]
    body_template: |               # Optional. Default: JSON anomaly payload
      {
        "text": "Anomaly: {{ rule.name }} — {{ anomaly.key }} ({{ anomaly.score }})"
      }
```

### Email (SMTP)

```yaml
notifications:
  - channel: email
    smtp_host: "smtp.gmail.com"
    smtp_port: 587
    tls: true                      # Optional. Default: true
    from: "alerts@example.com"
    to: ["ops@example.com", "security@example.com"]
    credentials: "${SMTP_PASSWORD}"  # Environment variable reference
    on: [trigger]
    subject: "[ANOMALY] {{ rule.name }} — {{ anomaly.key }}"
    template: |
      Anomaly detected by rule **{{ rule.name }}**

      - Member: {{ anomaly.key }}
      - Score: {{ anomaly.score | round(3) }}
      - Classification: {{ anomaly.classification }}
      - Signals: {% for s in anomaly.signals %}{{ s.0 }}: {{ s.1 | round(2) }} {% endfor %}
```

### Telegram (Bot API)

```yaml
notifications:
  - channel: telegram
    bot_token: "${TELEGRAM_BOT_TOKEN}"
    chat_id: "-1001234567890"      # Group/channel chat ID
    parse_mode: "MarkdownV2"       # Optional. MarkdownV2 | HTML
    on: [trigger]
    template: |
      🚨 *Anomaly Detected*
      Rule: {{ rule.name }}
      Member: `{{ anomaly.key }}`
      Score: {{ anomaly.score | round(3) }}
      Classification: {{ anomaly.classification }}
```

## Template Variables

Available in all `template` and `body_template` fields:

| Variable | Type | Description |
|----------|------|-------------|
| `rule.id` | string | Rule ID |
| `rule.name` | string | Rule display name |
| `rule.description` | string | Rule description |
| `rule.tags` | list | Rule tags |
| `anomaly.key` | string | Entity identifier (e.g., member code) |
| `anomaly.score` | float | Combined anomaly score [0.0, 1.0] |
| `anomaly.classification` | string | Normal/Mild/Anomalous/HighlyAnomalous |
| `anomaly.signals` | list | Per-detector scores [(name, score)] |
| `anomaly.features` | object | Feature values by name |
| `anomaly.cluster_id` | int? | Cluster assignment |
| `anomaly.entity_type` | string | Entity type |
| `env.<VAR>` | string | Environment variable |
| `now` | datetime | Current timestamp |

## Validation Rules

When creating or editing rules:

1. `metadata.id` must be unique, kebab-case, alphanumeric + hyphens only
2. `schedule.cron` must be valid 5-field cron expression
3. `schedule.timezone` must be valid IANA timezone
4. `detection` must have exactly one of `template` or `compose`
5. `notifications` must have at least one channel
6. `enrich.opensearch.rate_limit` must be > 0 and <= 600
7. Feature names in conditions must match the 10-dimensional vector
8. Template variables must reference valid fields
9. Environment variables (`${VAR}`) are resolved at runtime, not parse time

## REST API

```
GET    /anomaly-rules                    # List all rules
POST   /anomaly-rules                    # Create rule (YAML body)
GET    /anomaly-rules/{id}               # Get rule + recent triggers
PUT    /anomaly-rules/{id}               # Update rule (YAML body)
DELETE /anomaly-rules/{id}               # Delete rule + YAML file
POST   /anomaly-rules/{id}/start         # Resume paused rule
POST   /anomaly-rules/{id}/pause         # Pause active rule
POST   /anomaly-rules/{id}/run           # Trigger immediate evaluation
POST   /anomaly-rules/{id}/test-notify   # Send test notification
GET    /anomaly-rules/{id}/history       # Trigger history
PUT    /anomaly-rules/{id}/schedule      # Update cron schedule
```
