---
name: validate-anomaly-rule
description: Validate an anomaly detection rule YAML file against the DSL schema. Checks structure, feature references, cron syntax, template variables, notification config, and reports issues.
argument-hint: "path to YAML rule file (e.g., data/rules/login-spike.yml)"
allowed-tools: ["Read", "Glob", "Grep", "Bash"]
---

# Validate Anomaly Rule

You are validating a stupid-db anomaly detection rule YAML file. Perform comprehensive schema validation and report all issues found.

## Input

If `$ARGUMENTS` is provided, validate that specific file. Otherwise, find all rule files:
```
data/rules/*.yml
```

## Validation Checks

### 1. Structure (Required Fields)

- [ ] `apiVersion` is present and equals `v1`
- [ ] `kind` is present and equals `AnomalyRule`
- [ ] `metadata.id` — present, kebab-case, no special chars beyond hyphens
- [ ] `metadata.name` — present, non-empty string
- [ ] `metadata.enabled` — present, boolean
- [ ] `schedule.cron` — present, valid 5-field cron expression
- [ ] `schedule.timezone` — present, valid IANA timezone
- [ ] `detection` — present, has exactly ONE of `template` or `compose`
- [ ] `notifications` — present, at least one channel configured

### 2. Detection Validation

**If template-based:**
- [ ] `template` is one of: `spike`, `drift`, `absence`, `threshold`
- [ ] Required `params` are present for the chosen template:
  - `spike`: `feature`, `multiplier`
  - `drift`: `features`, `method`, `threshold`
  - `absence`: `feature`, `threshold`, `lookback_days`
  - `threshold`: `feature`, `operator`, `value`
- [ ] All feature names match the 10-dimensional vector

**If composition-based:**
- [ ] `compose.operator` is one of: `AND`, `OR`, `NOT`
- [ ] `compose.conditions` is a non-empty array
- [ ] Each condition has a valid `signal`: `z_score`, `dbscan_noise`, `behavioral_deviation`, `graph_anomaly`
- [ ] Each condition has a `threshold` (numeric, 0.0-1.0 for scores, any float for z_score)
- [ ] Nested compositions are properly structured (recursive check)

### 3. OpenSearch Enrichment (if present)

- [ ] `enrich.opensearch.query` is valid OpenSearch Query DSL object
- [ ] `enrich.opensearch.rate_limit` is 1-600
- [ ] `enrich.opensearch.timeout_ms` is positive integer, <= 30000
- [ ] `min_hits` or `max_hits` is set (at least one)

### 4. Filter Validation

- [ ] `entity_types` contains valid types: Member, Device, Game, Popup, Error, VipGroup, Affiliate, Currency, Platform, Provider
- [ ] `classifications` contains valid values: Normal, Mild, Anomalous, HighlyAnomalous
- [ ] `min_score` is 0.0-1.0
- [ ] `where` keys reference valid feature names
- [ ] `where` operators are valid: `gt`, `gte`, `lt`, `lte`, `eq`, `neq`

### 5. Notification Validation

**For each notification entry:**
- [ ] `channel` is one of: `webhook`, `email`, `telegram`
- [ ] `on` is a non-empty array of: `trigger`, `resolve`

**Webhook:**
- [ ] `url` is present and looks like a valid URL
- [ ] `method` (if present) is GET, POST, PUT, or PATCH

**Email:**
- [ ] `smtp_host` is present
- [ ] `smtp_port` is present (integer)
- [ ] `from` is present (valid email format)
- [ ] `to` is present (non-empty array of email addresses)
- [ ] `credentials` references an env var (`${...}`)

**Telegram:**
- [ ] `bot_token` references an env var (`${...}`)
- [ ] `chat_id` is present (string, starts with `-` for groups)

### 6. Template Variable Check

Scan all `template`, `body_template`, and `subject` fields for `{{ }}` variables:
- [ ] All variable references use valid paths: `rule.*`, `anomaly.*`, `env.*`, `now`
- [ ] No undefined variable paths

### 7. Security Check

- [ ] No hardcoded secrets (API keys, passwords, tokens)
- [ ] Secrets use `${ENV_VAR}` syntax
- [ ] Webhook URLs use HTTPS (warn if HTTP)

## Output Format

```
🔍 Validating: {filepath}

✅ Structure: OK
✅ Detection: spike template — feature: login_count_7d, multiplier: 3.0
✅ Schedule: */15 * * * * (Asia/Manila) — valid
✅ Filters: Member entities, min_score: 0.5
✅ Notifications: webhook (1), email (1)
✅ Templates: All variables valid
✅ Security: No hardcoded secrets

Result: VALID ✅
```

Or with errors:

```
🔍 Validating: {filepath}

✅ Structure: OK
❌ Detection: Unknown feature "login_frequency" — did you mean "login_count_7d"?
✅ Schedule: */15 * * * * (UTC) — valid
⚠️ Filters: No entity_types specified — rule will match ALL entities
❌ Notifications: Telegram bot_token is hardcoded — use ${TELEGRAM_BOT_TOKEN}
⚠️ Security: Webhook URL uses HTTP — consider HTTPS

Result: INVALID ❌ (2 errors, 2 warnings)
```

If validating multiple files, show a summary table at the end:

```
| File | Status | Errors | Warnings |
|------|--------|--------|----------|
| login-spike.yml | ✅ VALID | 0 | 0 |
| vip-absence.yml | ❌ INVALID | 1 | 1 |
| fraud-detect.yml | ⚠️ VALID | 0 | 2 |
```
