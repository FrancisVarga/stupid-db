---
name: create-anomaly-rule
description: Interactively create a new anomaly detection rule YAML file. Guides through detection type selection, threshold configuration, notification setup, and schedule definition. Writes validated YAML to the rules directory.
argument-hint: "describe what you want to detect (e.g., 'alert when VIP members stop logging in')"
allowed-tools: ["Read", "Write", "Edit", "Glob", "Grep", "Bash", "AskUserQuestion"]
---

# Create Anomaly Rule

You are creating a new anomaly detection rule for stupid-db. Guide the user through an interactive conversation to build a complete, validated YAML rule file.

## Context

Read the DSL reference skill first to understand the full schema:
- Skill: `anomaly-rule-dsl` — complete YAML schema, templates, signals, notifications

Rule files go in `{data_dir}/rules/{id}.yml` where `data_dir` is typically the project's configured data directory. For development, use `data/rules/` relative to the project root.

## Workflow

### Step 1: Understand Intent

If `$ARGUMENTS` is provided, parse it to understand what the user wants to detect. Otherwise ask:

"What anomaly do you want to detect? Describe it in plain English."

Examples:
- "Alert when any member's login count spikes 3x"
- "Notify me when VIP members haven't logged in for 7 days"
- "Detect fraud: unusual login patterns combined with device switching"

### Step 2: Choose Detection Approach

Based on the user's description, recommend the best detection approach:

| Pattern | Recommended |
|---------|-------------|
| "spikes", "sudden increase", "burst" | Template: `spike` |
| "changes gradually", "drifts", "shifts" | Template: `drift` |
| "stops", "missing", "inactive", "absence" | Template: `absence` |
| "exceeds", "above", "below", "threshold" | Template: `threshold` |
| Multiple conditions, "and", "or", "both" | Signal composition |

Ask the user to confirm or adjust.

### Step 3: Configure Detection Parameters

Based on the chosen approach, ask for specific parameters. Provide sensible defaults:

**For templates**: Ask which feature(s) to monitor, threshold values, baseline method.
**For composition**: Ask which signals to combine, thresholds per signal, AND/OR logic.

Show the available features:
```
login_count_7d, game_count_7d, unique_games_7d, error_count_7d,
popup_interaction_7d, platform_mobile_ratio, session_count_7d,
avg_session_gap_hours, vip_group_numeric, currency_encoded
```

### Step 4: Ask About OpenSearch Enrichment

"Do you want to enrich detection with raw OpenSearch event data? This adds a query against the event index for additional context (e.g., check specific event types, count raw logins in last hour)."

If yes, help construct the OpenSearch query DSL and set rate limits.

### Step 5: Configure Filters

Ask about entity scope:
- Which entity types? (Default: Member)
- Minimum anomaly score? (Default: 0.5)
- Any entities to exclude? (e.g., test accounts)
- Feature-level filters? (e.g., VIP level >= 4)

### Step 6: Set Schedule

Ask about monitoring frequency. Recommend based on use case:
- Security: every 5 minutes
- Operations: every 15 minutes
- Business: hourly
- Reports: daily

Ask for timezone (default: UTC).
Ask about cooldown to prevent alert fatigue.

### Step 7: Configure Notifications

Ask which channels to notify:

1. **Webhook**: Ask for URL, any custom headers, body template preference
2. **Email**: Ask for SMTP config, recipients, subject template
3. **Telegram**: Ask for bot token env var, chat ID

For each channel, ask:
- Trigger on: `[trigger]`, `[resolve]`, or `[trigger, resolve]`?
- Custom message template? (Or use defaults)

Remind the user: use `${ENV_VAR}` syntax for secrets (tokens, passwords).

### Step 8: Generate Metadata

Generate:
- `id`: kebab-case from the rule name (e.g., "Login Spike Detection" → "login-spike-detection")
- `name`: User's description or ask for a display name
- `tags`: Infer from content (e.g., security, vip, fraud, operations)

### Step 9: Validate and Write

Before writing, validate the complete rule against the schema:
1. Check all required fields are present
2. Verify feature names match the 10-dimensional vector
3. Verify signal names are valid
4. Check cron syntax
5. Ensure at least one notification channel

Present the complete YAML to the user for review. Ask: "Does this look correct? Should I write it?"

If confirmed, write to `data/rules/{id}.yml`.

### Step 10: Summary

After writing, display:
```
✅ Rule created: data/rules/{id}.yml
   Name: {name}
   Detection: {template/compose summary}
   Schedule: {cron} ({timezone})
   Notifications: {channel list}

   Next: Start the rule via dashboard or API:
   POST /anomaly-rules/{id}/start
```

## Example Interaction

User: `/create-anomaly-rule alert when VIP members stop logging in`

→ Recommend: Template `absence` on `login_count_7d`
→ Ask: lookback period? (suggest 7 days)
→ Ask: VIP level filter? (suggest >= 4)
→ Ask: Schedule? (suggest daily at 9 AM)
→ Ask: Notifications? (suggest email + telegram)
→ Generate YAML → confirm → write
