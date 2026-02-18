---
name: rules-system
description: YAML-based rules system with 6 kinds, two-pass deserialization, and extends/deep-merge
---

# Rules System

## Overview

The rules system provides declarative YAML configuration for anomaly detection, entity schemas, feature engineering, scoring, trends, and patterns. Rules are loaded by `RuleLoader` which does recursive directory scanning of `data/rules/`.

## Two-Pass Deserialization

```rust
// Pass 1: Parse header to determine kind
let envelope: RuleEnvelope = serde_yaml::from_str(yaml)?;
// envelope.kind → "AnomalyRule" | "EntitySchema" | "FeatureConfig" | ...

// Pass 2: Parse kind-specific body
let document: RuleDocument = match envelope.kind {
    "AnomalyRule" => RuleDocument::AnomalyRule(serde_yaml::from_str(yaml)?),
    "EntitySchema" => RuleDocument::EntitySchema(serde_yaml::from_str(yaml)?),
    // ...
};
```

## 6 Rule Kinds

| Kind | File Location | Purpose |
|------|---------------|---------|
| AnomalyRule | `data/rules/anomaly/` | Multi-signal detection (statistical, behavioral, graph) |
| EntitySchema | `data/rules/schema/` | Entity/edge type definitions, event mappings |
| FeatureConfig | `data/rules/features/` | 10-dimensional feature vector definitions |
| ScoringConfig | `data/rules/scoring/` | Anomaly scoring weights and thresholds |
| TrendConfig | `data/rules/scoring/` | Trend detection parameters |
| PatternConfig | `data/rules/patterns/` | Temporal pattern (PrefixSpan) settings |

## Compiled Types

Each kind has a Compiled* type for O(1) hot-path lookups:

```rust
// Example: CompiledEntitySchema
pub struct CompiledEntitySchema {
    pub entity_types: HashSet<String>,
    pub edge_types: HashSet<String>,
    pub event_mappings: HashMap<String, Vec<EntityMapping>>,
}
```

## extends Keyword

Rules support inheritance via `extends`:

```yaml
kind: AnomalyRule
extends: base-statistical
name: login-spike
# child fields override parent, arrays replace entirely
```

Deep-merge behavior: child wins for scalar fields, arrays replace entirely (not concatenated).

## Key Source Files

- `crates/rules/src/schema.rs` — RuleDocument, RuleEnvelope, CommonMetadata
- `crates/rules/src/loader.rs` — RuleLoader with extends resolution
- `crates/rules/src/validation/` — Per-kind validation logic
- `crates/rules/tests/examples.rs` — Integration tests with real YAML

## Adding a New Rule Kind

1. Add variant to `RuleDocument` enum in `schema.rs`
2. Create struct for the new kind with serde derives
3. Add Compiled* type with HashMap/HashSet for hot-path
4. Add `*_with_config()` method alongside hardcoded original
5. Update RuleLoader to handle new kind
6. Add validation in `validation/` module
7. Add YAML examples in `data/rules/{new_kind}/`
8. Add integration tests
