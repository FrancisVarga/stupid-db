---
name: rule-validation
description: Rule validation patterns, common errors, debugging, and testing strategies for the YAML rules system
triggers:
  - validate rule
  - rule error
  - rule validation
  - rule testing
  - fix rule
  - debug rule
---

# Rule Validation & Testing

## Validation Pipeline

Rules pass through validation before loading. The validator returns structured results:

```rust
ValidationResult {
    valid: bool,
    errors: Vec<ValidationError>,    // Blocking — rule won't load
    warnings: Vec<ValidationWarning>, // Advisory — rule loads but may misbehave
}
```

Errors include JSON-path locations and optional suggestions (fuzzy matching for typos).

## Validation Checks by Kind

### AnomalyRule
- metadata.id must be non-empty and unique
- Must have EITHER template OR compose, never both
- Template must be one of: spike, threshold, absence, drift
- Template params must match the template type (e.g., spike needs feature + multiplier)
- Compose operator must be "and" or "or"
- Compose conditions must be non-empty array
- Signal types must be: z_score, dbscan_noise, behavioral_deviation, graph_anomaly
- Threshold must be positive number
- Cron expression must be valid 5-field format
- Cooldown must be parseable (e.g., "30m", "1h", "1d2h30m15s")
- Notification channel must be: webhook, email, telegram
- Notification on events must be: trigger, resolve

### EntitySchema
- Entity type names must match Rust EntityType enum variants
- Edge from/to must reference valid entity types
- Key prefixes must be unique per entity type
- Field mappings must reference valid entity types
- Event extraction entities must reference valid fields

### FeatureConfig
- Feature indices must be contiguous 0..N
- Feature names must be unique
- VIP/currency encoding values must be positive numbers
- Fallback must be "hash_based" or a default number

### ScoringConfig
- multi_signal_weights should sum to approximately 1.0
- Classification thresholds must be ordered: mild < anomalous < highly_anomalous
- Z-score divisor must be positive

### TrendConfig
- Window size must be positive integer
- min_data_points must be >= 2
- Severity thresholds must be ordered: notable < significant < critical

### PatternConfig
- min_support must be 0.0 to 1.0
- max_length must be positive
- Classification rules should have a fallback (last rule matches everything)
- Check types must be: count_gte, sequence_match, has_then_absent

## Common Errors & Fixes

### "unknown field" Error
**Cause**: YAML field name doesn't match Rust struct (serde strict mode with `deny_unknown_fields`)
**Fix**: Check exact field names in rule-authoring skill. Common typos:
- `api_version` → `apiVersion` (camelCase)
- `entity_type` → `entity_types` (plural)
- `notification` → `notifications` (plural)

### "missing field" Error
**Cause**: Required field not provided
**Fix**: All rules require: apiVersion, kind, metadata.id, metadata.name, metadata.enabled

### extends Resolution Failure
**Cause**: Parent rule ID not found, circular reference, or depth > 5
**Fix**:
- Verify parent ID exists in loaded rules
- Check for circular chains: A extends B extends A
- Flatten deep hierarchies (max 5 levels)

### Cron Parse Error
**Cause**: Invalid cron expression
**Fix**: Use 5-field format: `minute hour day month weekday`
- `*/15 * * * *` — every 15 minutes
- `0 * * * *` — every hour
- `0 9 * * *` — daily at 9am

### Template/Compose Conflict
**Cause**: Both template and compose specified in detection
**Fix**: Use ONE detection mode. Template for simple rules, compose for boolean logic.

### Signal Type Not Recognized
**Valid values**: z_score, dbscan_noise, behavioral_deviation, graph_anomaly
**Common mistake**: Using "zscore" or "z-score" instead of "z_score"

## Testing Patterns

### Integration Tests (crates/rules/tests/examples.rs)

Tests load real YAML files from `data/rules/` and verify:
- Deserialization succeeds without errors
- Fields have expected values
- Template parameters are correctly parsed

```rust
#[test]
fn test_login_spike_loads() {
    let yaml = include_str!("../../../data/rules/anomaly/login-spike.yml");
    let doc = RuleLoader::parse_yaml(yaml).unwrap();
    assert!(matches!(doc, RuleDocument::Anomaly(_)));
}
```

### Schema Sync Tests

Verify YAML entity/edge types match Rust enum variants:
```rust
// If YAML defines entity type "Member", Rust EntityType::Member must exist
// If YAML defines edge type "LoggedInFrom", Rust EdgeType::LoggedInFrom must exist
```

### Writing Tests for New Rules

1. Create YAML file in `data/rules/{kind}/`
2. Add test in `crates/rules/tests/examples.rs`:
   ```rust
   #[test]
   fn test_my_new_rule() {
       let yaml = include_str!("../../../data/rules/anomaly/my-rule.yml");
       let doc = RuleLoader::parse_yaml(yaml).unwrap();
       let rule = doc.as_anomaly().expect("should be AnomalyRule");
       assert_eq!(rule.metadata.id, "my-rule");
       assert!(rule.metadata.enabled);
   }
   ```
3. Run: `cargo nextest run -p stupid-rules`

### Testing extends Chains

```rust
#[test]
fn test_extends_resolution() {
    let loader = RuleLoader::new(PathBuf::from("data/rules"));
    let results = loader.load_all().unwrap();
    // Verify child rule has merged parent fields
}
```

### Testing Validation

```rust
use stupid_rules::validation::validate_rule;

let result = validate_rule(&rule);
assert!(result.valid);
assert!(result.errors.is_empty());
```

## Debugging Rule Loading

### Step 1: Check file is discovered
```bash
# RuleLoader scans recursively, skips dotfiles
ls data/rules/anomaly/*.yml
```

### Step 2: Validate YAML syntax
```bash
# Quick check for YAML parse errors
python3 -c "import yaml; yaml.safe_load(open('data/rules/anomaly/my-rule.yml'))"
```

### Step 3: Check LoadResult
RuleLoader.load_all() returns Vec<LoadResult>:
- `Loaded { rule_id }` — success
- `Skipped { reason }` — file ignored (e.g., dotfile)
- `Failed { error }` — parse or validation error

### Step 4: Run specific test
```bash
cargo nextest run -p stupid-rules test_my_rule
```

## Validation API

```rust
use stupid_rules::validation::{validate_rule, validate_document};

// Validate a parsed AnomalyRule
let result = validate_rule(&anomaly_rule);

// Validate any RuleDocument
let result = validate_document(&document);

// Check results
if !result.valid {
    for error in &result.errors {
        println!("ERROR at {}: {} (suggestion: {:?})",
            error.path, error.message, error.suggestion);
    }
}
for warning in &result.warnings {
    println!("WARNING at {}: {}", warning.path, warning.message);
}
```

## Hot-Reload Behavior

- RuleLoader watches filesystem with 500ms debounce
- Modified files trigger re-parse + re-validation
- Invalid files are rejected (previous version retained)
- New files in scanned directories are auto-discovered
- Deleted files remove the rule from active set
