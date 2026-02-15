---
name: list-anomaly-rules
description: List all anomaly detection rule YAML files with status summary. Shows each rule's detection type, schedule, notification channels, and enabled state.
allowed-tools: ["Read", "Glob", "Grep"]
---

# List Anomaly Rules

Scan for all anomaly rule YAML files and display a summary table.

## Search Locations

Look for rule files in these locations (in order):
1. `data/rules/*.yml`
2. `data/rules/*.yaml`
3. If `DATA_DIR` env is set: `${DATA_DIR}/rules/*.yml`

## Output Format

For each rule file found, read it and extract key fields. Display as a table:

```
📋 Anomaly Detection Rules ({count} total)

| ID | Name | Detection | Schedule | Channels | Status |
|----|------|-----------|----------|----------|--------|
| login-spike | Login Spike Detection | spike (login_count_7d × 3.0) | */15 * * * * | webhook, email | ✅ Enabled |
| vip-absence | VIP Member Absence | absence (login_count_7d = 0) | 0 9 * * * | telegram | ✅ Enabled |
| multi-signal-fraud | Multi-Signal Fraud | compose (AND: z_score + dbscan) | */30 * * * * | webhook | ⏸ Disabled |
| error-burst | Error Burst Alert | threshold (error_count_7d > 100) | */5 * * * * | webhook, telegram | ✅ Enabled |

Summary:
  Active: 3 | Paused: 1 | Total: 4
  Channels: webhook (3), email (1), telegram (2)
  Templates: spike (1), absence (1), threshold (1), compose (1)
```

If no rules found:

```
📋 No anomaly rules found.

Create your first rule:
  /create-anomaly-rule "describe what you want to detect"

Or manually create a YAML file in data/rules/
```

## Detection Type Summary

Format the detection column based on type:
- Template: `{template} ({feature} {operator} {value})`
- Compose: `compose ({operator}: {signal list})`
