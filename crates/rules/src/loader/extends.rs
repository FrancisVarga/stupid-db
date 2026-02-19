//! Deep-merge and `extends` inheritance resolution for YAML rules.

use std::collections::{HashMap, HashSet};

/// Maximum inheritance chain depth to prevent infinite loops.
const MAX_EXTENDS_DEPTH: usize = 5;

/// Deep-merge two YAML `Value` maps: child fields win, arrays replace entirely.
///
/// For map values: recursively merge. For all other types (scalars, arrays):
/// child value replaces parent.
pub fn deep_merge(parent: &serde_yaml::Value, child: &serde_yaml::Value) -> serde_yaml::Value {
    match (parent, child) {
        (serde_yaml::Value::Mapping(pm), serde_yaml::Value::Mapping(cm)) => {
            let mut merged = pm.clone();
            for (key, child_val) in cm {
                if let Some(parent_val) = pm.get(key) {
                    merged.insert(key.clone(), deep_merge(parent_val, child_val));
                } else {
                    merged.insert(key.clone(), child_val.clone());
                }
            }
            serde_yaml::Value::Mapping(merged)
        }
        // For scalars, arrays, etc.: child wins.
        (_, child) => child.clone(),
    }
}

/// Resolve `extends` chains: for each rule with an `extends` field,
/// find the parent and deep-merge the YAML values.
///
/// Returns a new map with all extends chains resolved.
pub fn resolve_extends(
    raw_values: &HashMap<String, serde_yaml::Value>,
) -> std::result::Result<HashMap<String, serde_yaml::Value>, String> {
    let mut resolved: HashMap<String, serde_yaml::Value> = HashMap::new();
    let mut in_progress: HashSet<String> = HashSet::new();

    for id in raw_values.keys() {
        resolve_single(id, raw_values, &mut resolved, &mut in_progress, 0)?;
    }

    Ok(resolved)
}

fn resolve_single(
    id: &str,
    raw_values: &HashMap<String, serde_yaml::Value>,
    resolved: &mut HashMap<String, serde_yaml::Value>,
    in_progress: &mut HashSet<String>,
    depth: usize,
) -> std::result::Result<serde_yaml::Value, String> {
    // Already resolved.
    if let Some(val) = resolved.get(id) {
        return Ok(val.clone());
    }

    // Cycle detection.
    if in_progress.contains(id) {
        return Err(format!("circular extends chain detected for rule '{}'", id));
    }

    // Depth limit.
    if depth > MAX_EXTENDS_DEPTH {
        return Err(format!(
            "extends chain exceeds maximum depth ({}) for rule '{}'",
            MAX_EXTENDS_DEPTH, id
        ));
    }

    let raw = raw_values
        .get(id)
        .ok_or_else(|| format!("rule '{}' not found for extends resolution", id))?
        .clone();

    // Check for extends field.
    let parent_id = raw
        .as_mapping()
        .and_then(|m| m.get(&serde_yaml::Value::String("metadata".to_string())))
        .and_then(|meta| meta.as_mapping())
        .and_then(|m| m.get(&serde_yaml::Value::String("extends".to_string())))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let result = if let Some(ref parent_id) = parent_id {
        in_progress.insert(id.to_string());
        let parent_val = resolve_single(parent_id, raw_values, resolved, in_progress, depth + 1)?;
        in_progress.remove(id);
        deep_merge(&parent_val, &raw)
    } else {
        raw
    };

    resolved.insert(id.to_string(), result.clone());
    Ok(result)
}
