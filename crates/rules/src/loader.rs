//! Filesystem rule loader with hot-reload via `notify` watcher.
//!
//! Watches the rules directory for YAML file changes (create, modify, delete)
//! and reloads affected rules into the in-memory rule set.
//! Supports all rule kinds via two-pass deserialization (RuleEnvelope → RuleDocument).

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use notify::{
    event::{CreateKind, ModifyKind, RemoveKind},
    Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher,
};
use tracing::{info, warn};

use crate::schema::{AnomalyRule, RuleDocument, RuleEnvelope};

// ── Deep-merge for `extends` inheritance ────────────────────────────

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
    let mut in_progress: std::collections::HashSet<String> = std::collections::HashSet::new();

    for id in raw_values.keys() {
        resolve_single(id, raw_values, &mut resolved, &mut in_progress, 0)?;
    }

    Ok(resolved)
}

fn resolve_single(
    id: &str,
    raw_values: &HashMap<String, serde_yaml::Value>,
    resolved: &mut HashMap<String, serde_yaml::Value>,
    in_progress: &mut std::collections::HashSet<String>,
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

// ── Error type ──────────────────────────────────────────────────────

/// Errors that can occur during rule loading and management.
#[derive(Debug, thiserror::Error)]
pub enum RuleError {
    /// Filesystem I/O error.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// YAML parse/deserialization error.
    #[error("YAML parse error: {0}")]
    Parse(#[from] serde_yaml::Error),

    /// Rule validation error (e.g. missing required fields, duplicate IDs).
    #[error("Validation error: {0}")]
    Validation(String),

    /// Filesystem watcher error.
    #[error("Notify watcher error: {0}")]
    Notify(#[from] notify::Error),
}

/// Result alias for rule operations.
pub type Result<T> = std::result::Result<T, RuleError>;

// ── Load result types ───────────────────────────────────────────────

/// Outcome of loading a single rule file.
#[derive(Debug)]
pub struct LoadResult {
    /// Path to the file that was loaded.
    pub path: PathBuf,
    /// Status of the load attempt.
    pub status: LoadStatus,
}

/// Status of a single file load attempt.
#[derive(Debug)]
pub enum LoadStatus {
    /// Rule was successfully loaded.
    Loaded { rule_id: String },
    /// File was skipped (dotfile, non-YAML, etc.).
    Skipped { reason: String },
    /// Parse or validation error occurred.
    Failed { error: String },
}

// ── Rule loader ─────────────────────────────────────────────────────

/// Filesystem-backed rule loader with optional hot-reload.
///
/// Scans a directory (recursively) for `*.yml` / `*.yaml` files, deserializes
/// them into [`RuleDocument`] instances via two-pass deserialization, and
/// maintains an in-memory map keyed by rule ID.
///
/// For backward compatibility, anomaly rules are also accessible via [`rules()`].
pub struct RuleLoader {
    /// Root directory containing rule YAML files.
    rules_dir: PathBuf,
    /// In-memory store of all rule documents keyed by `metadata.id`.
    documents: Arc<RwLock<HashMap<String, RuleDocument>>>,
    /// Backward-compatible anomaly-only store.
    anomaly_rules: Arc<RwLock<HashMap<String, AnomalyRule>>>,
    /// Active filesystem watcher (held to keep it alive).
    _watcher: Option<RecommendedWatcher>,
}

impl RuleLoader {
    /// Create a new loader for the given directory.
    ///
    /// Creates the directory (and parents) if it does not exist.
    pub fn new(rules_dir: PathBuf) -> Self {
        if !rules_dir.exists() {
            if let Err(e) = fs::create_dir_all(&rules_dir) {
                warn!(path = %rules_dir.display(), error = %e, "failed to create rules directory");
            }
        }
        Self {
            rules_dir,
            documents: Arc::new(RwLock::new(HashMap::new())),
            anomaly_rules: Arc::new(RwLock::new(HashMap::new())),
            _watcher: None,
        }
    }

    /// Recursively scan the rules directory and load all YAML files.
    ///
    /// Dotfiles (filenames starting with `.`) and non-YAML files are skipped.
    /// Subdirectories are scanned recursively.
    /// Parse errors are reported per-file but do not abort the scan.
    pub fn load_all(&self) -> Result<Vec<LoadResult>> {
        let mut results = Vec::new();
        self.scan_dir_recursive(&self.rules_dir, &mut results)?;
        Ok(results)
    }

    /// Recursively scan a directory for YAML rule files.
    fn scan_dir_recursive(&self, dir: &Path, results: &mut Vec<LoadResult>) -> Result<()> {
        let entries = match fs::read_dir(dir) {
            Ok(e) => e,
            Err(e) => {
                warn!(path = %dir.display(), error = %e, "failed to read directory");
                return Ok(());
            }
        };

        for entry in entries {
            let entry = entry?;
            let path = entry.path();

            // Skip dotfiles/dotdirs
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.starts_with('.') {
                    if path.is_file() {
                        results.push(LoadResult {
                            path,
                            status: LoadStatus::Skipped {
                                reason: "dotfile".to_string(),
                            },
                        });
                    }
                    continue;
                }
            }

            // Recurse into subdirectories
            if path.is_dir() {
                self.scan_dir_recursive(&path, results)?;
                continue;
            }

            // Skip non-YAML extensions
            let is_yaml = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e == "yml" || e == "yaml")
                .unwrap_or(false);

            if !is_yaml {
                results.push(LoadResult {
                    path,
                    status: LoadStatus::Skipped {
                        reason: "not a YAML file".to_string(),
                    },
                });
                continue;
            }

            match self.load_file(&path) {
                Ok(doc) => {
                    let rule_id = doc.metadata().id.clone();
                    info!(rule_id = %rule_id, kind = %doc.kind(), path = %path.display(), "loaded rule");
                    self.insert_document(rule_id.clone(), doc);
                    results.push(LoadResult {
                        path,
                        status: LoadStatus::Loaded { rule_id },
                    });
                }
                Err(e) => {
                    warn!(path = %path.display(), error = %e, "failed to load rule file");
                    results.push(LoadResult {
                        path,
                        status: LoadStatus::Failed {
                            error: e.to_string(),
                        },
                    });
                }
            }
        }

        Ok(())
    }

    /// Insert a RuleDocument into both the documents map and (if anomaly) the anomaly_rules map.
    fn insert_document(&self, id: String, doc: RuleDocument) {
        if let RuleDocument::Anomaly(ref rule) = doc {
            self.anomaly_rules
                .write()
                .expect("anomaly_rules lock poisoned")
                .insert(id.clone(), rule.clone());
        }
        self.documents
            .write()
            .expect("documents lock poisoned")
            .insert(id, doc);
    }

    /// Remove a document from both maps.
    fn remove_document(&self, id: &str) {
        self.documents
            .write()
            .expect("documents lock poisoned")
            .remove(id);
        self.anomaly_rules
            .write()
            .expect("anomaly_rules lock poisoned")
            .remove(id);
    }

    /// Parse a single YAML file into a [`RuleDocument`] via two-pass deserialization.
    ///
    /// First pass: deserialize as [`RuleEnvelope`] to read the `kind` field.
    /// Second pass: reconstruct and deserialize into the kind-specific type.
    pub fn load_file(&self, path: &Path) -> Result<RuleDocument> {
        let contents = fs::read_to_string(path)?;

        // First pass: extract envelope (kind + metadata).
        let envelope: RuleEnvelope = serde_yaml::from_str(&contents)?;

        // Basic validation.
        if envelope.metadata.id.is_empty() {
            return Err(RuleError::Validation(
                "rule metadata.id must not be empty".to_string(),
            ));
        }

        // Second pass: deserialize into kind-specific type.
        envelope
            .parse_full()
            .map_err(|e| RuleError::Validation(format!("failed to parse rule '{}': {}", envelope.metadata.id, e)))
    }

    /// Start a filesystem watcher with 500ms debounce.
    ///
    /// On file create/modify the rule is re-parsed and upserted.
    /// On file delete the rule is removed from the in-memory map.
    /// Parse errors are logged as warnings; the previous version is kept.
    pub fn watch(&mut self) -> Result<()> {
        let documents = Arc::clone(&self.documents);
        let anomaly_rules = Arc::clone(&self.anomaly_rules);
        let rules_dir = self.rules_dir.clone();

        let mut watcher = notify::recommended_watcher(move |res: std::result::Result<Event, notify::Error>| {
            match res {
                Ok(event) => handle_fs_event(&event, &documents, &anomaly_rules, &rules_dir),
                Err(e) => warn!(error = %e, "filesystem watcher error"),
            }
        })?;

        // Watch recursively to pick up changes in subdirectories.
        watcher.watch(&self.rules_dir, RecursiveMode::Recursive)?;

        let _ = watcher.configure(notify::Config::default().with_poll_interval(Duration::from_millis(500)));

        info!(path = %self.rules_dir.display(), "watching rules directory for changes (recursive)");
        self._watcher = Some(watcher);
        Ok(())
    }

    /// Get the shared anomaly rules map (backward compatibility).
    /// Get the rules directory path.
    pub fn rules_dir(&self) -> &Path {
        &self.rules_dir
    }

    pub fn rules(&self) -> Arc<RwLock<HashMap<String, AnomalyRule>>> {
        Arc::clone(&self.anomaly_rules)
    }

    /// Get the shared documents map containing all rule kinds.
    pub fn documents(&self) -> Arc<RwLock<HashMap<String, RuleDocument>>> {
        Arc::clone(&self.documents)
    }

    /// Atomically write a rule document to a YAML file.
    ///
    /// Writes to a `.tmp` file first, then renames to the final path to
    /// avoid partial writes on crash.
    pub fn write_document(&self, doc: &RuleDocument) -> Result<PathBuf> {
        let meta = doc.metadata();
        let filename = format!("{}.yml", meta.id);
        let final_path = self.rules_dir.join(&filename);
        let tmp_path = self.rules_dir.join(format!(".{}.tmp", meta.id));

        let yaml = doc.to_yaml().map_err(RuleError::Parse)?;
        fs::write(&tmp_path, yaml)?;
        fs::rename(&tmp_path, &final_path)?;

        info!(rule_id = %meta.id, kind = %doc.kind(), path = %final_path.display(), "wrote rule file");

        self.insert_document(meta.id.clone(), doc.clone());
        Ok(final_path)
    }

    /// Atomically write an anomaly rule to a YAML file (backward compatibility).
    pub fn write_rule(&self, rule: &AnomalyRule) -> Result<PathBuf> {
        self.write_document(&RuleDocument::Anomaly(rule.clone()))
    }

    /// Delete a rule file by rule ID.
    ///
    /// Removes both the file and the in-memory entry.
    pub fn delete_rule(&self, id: &str) -> Result<()> {
        // Try both extensions
        let yml_path = self.rules_dir.join(format!("{}.yml", id));
        let yaml_path = self.rules_dir.join(format!("{}.yaml", id));

        let removed = if yml_path.exists() {
            fs::remove_file(&yml_path)?;
            true
        } else if yaml_path.exists() {
            fs::remove_file(&yaml_path)?;
            true
        } else {
            false
        };

        if !removed {
            return Err(RuleError::Validation(format!(
                "no rule file found for id '{}'",
                id
            )));
        }

        self.remove_document(id);

        info!(rule_id = %id, "deleted rule");
        Ok(())
    }
}

// ── Filesystem event handler ────────────────────────────────────────

/// Handle a single filesystem event from the notify watcher.
fn handle_fs_event(
    event: &Event,
    documents: &Arc<RwLock<HashMap<String, RuleDocument>>>,
    anomaly_rules: &Arc<RwLock<HashMap<String, AnomalyRule>>>,
    _rules_dir: &Path,
) {
    for path in &event.paths {
        // Only process YAML files
        let is_yaml = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e == "yml" || e == "yaml")
            .unwrap_or(false);

        if !is_yaml {
            continue;
        }

        // Skip dotfiles (including our .tmp files)
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if name.starts_with('.') {
                continue;
            }
        }

        match &event.kind {
            EventKind::Create(CreateKind::File)
            | EventKind::Modify(ModifyKind::Data(_))
            | EventKind::Modify(ModifyKind::Name(_)) => {
                // File created or modified: two-pass parse and upsert.
                match fs::read_to_string(path) {
                    Ok(contents) => {
                        match serde_yaml::from_str::<RuleEnvelope>(&contents)
                            .map_err(|e| e.to_string())
                            .and_then(|env| env.parse_full())
                        {
                            Ok(doc) => {
                                let rule_id = doc.metadata().id.clone();
                                let kind = doc.kind();
                                info!(rule_id = %rule_id, kind = %kind, path = %path.display(), "hot-reloaded rule");

                                if let RuleDocument::Anomaly(ref rule) = doc {
                                    anomaly_rules
                                        .write()
                                        .expect("anomaly_rules lock poisoned")
                                        .insert(rule_id.clone(), rule.clone());
                                }
                                documents
                                    .write()
                                    .expect("documents lock poisoned")
                                    .insert(rule_id, doc);
                            }
                            Err(e) => {
                                warn!(
                                    path = %path.display(),
                                    error = %e,
                                    "failed to parse rule during hot-reload, keeping previous version"
                                );
                            }
                        }
                    }
                    Err(e) => {
                        warn!(path = %path.display(), error = %e, "failed to read file during hot-reload");
                    }
                }
            }
            EventKind::Remove(RemoveKind::File) => {
                // File deleted: find and remove the rule whose file this was.
                let _ = remove_rule_by_path(documents, anomaly_rules, path);
            }
            _ => {}
        }
    }
}

/// Remove a rule from both maps given its file path.
fn remove_rule_by_path(
    documents: &Arc<RwLock<HashMap<String, RuleDocument>>>,
    anomaly_rules: &Arc<RwLock<HashMap<String, AnomalyRule>>>,
    path: &Path,
) -> Option<RuleDocument> {
    let stem = path.file_stem()?.to_str()?;
    let removed = documents.write().expect("documents lock poisoned").remove(stem);
    anomaly_rules.write().expect("anomaly_rules lock poisoned").remove(stem);
    if removed.is_some() {
        info!(rule_id = %stem, path = %path.display(), "removed rule after file deletion");
    }
    removed
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    const VALID_RULE_YAML: &str = r#"
apiVersion: v1
kind: AnomalyRule
metadata:
  id: test-rule
  name: Test Rule
  enabled: true
schedule:
  cron: "*/15 * * * *"
detection:
  template: spike
  params:
    feature: login_count
    multiplier: 3.0
"#;

    fn temp_loader() -> (TempDir, RuleLoader) {
        let dir = TempDir::new().expect("create tempdir");
        let loader = RuleLoader::new(dir.path().to_path_buf());
        (dir, loader)
    }

    #[test]
    fn load_rule_from_file() {
        let (dir, loader) = temp_loader();
        let rule_path = dir.path().join("test-rule.yml");
        fs::write(&rule_path, VALID_RULE_YAML).unwrap();

        let doc = loader.load_file(&rule_path).unwrap();
        assert_eq!(doc.metadata().id, "test-rule");
        assert_eq!(doc.metadata().name, "Test Rule");
        assert!(doc.as_anomaly().is_some());
    }

    #[test]
    fn load_all_skips_dotfiles_and_non_yaml() {
        let (dir, loader) = temp_loader();

        // Valid YAML
        fs::write(dir.path().join("rule1.yml"), VALID_RULE_YAML).unwrap();

        // Dotfile (should be skipped)
        fs::write(dir.path().join(".hidden.yml"), VALID_RULE_YAML).unwrap();

        // Non-YAML (should be skipped)
        fs::write(dir.path().join("readme.txt"), "not a rule").unwrap();

        let results = loader.load_all().unwrap();

        let loaded: Vec<_> = results
            .iter()
            .filter(|r| matches!(r.status, LoadStatus::Loaded { .. }))
            .collect();
        let skipped: Vec<_> = results
            .iter()
            .filter(|r| matches!(r.status, LoadStatus::Skipped { .. }))
            .collect();

        assert_eq!(loaded.len(), 1);
        assert_eq!(skipped.len(), 2);

        // Verify rule is in both maps
        let rules = loader.rules();
        let guard = rules.read().unwrap();
        assert!(guard.contains_key("test-rule"));

        let docs = loader.documents();
        let guard = docs.read().unwrap();
        assert!(guard.contains_key("test-rule"));
    }

    #[test]
    fn load_all_recursive_subdirectories() {
        let (dir, loader) = temp_loader();

        // Rule in root
        fs::write(dir.path().join("rule1.yml"), VALID_RULE_YAML).unwrap();

        // Rule in subdirectory
        let sub = dir.path().join("subdir");
        fs::create_dir(&sub).unwrap();
        let sub_yaml = VALID_RULE_YAML.replace("test-rule", "sub-rule");
        fs::write(sub.join("sub-rule.yml"), sub_yaml).unwrap();

        let results = loader.load_all().unwrap();

        let loaded: Vec<_> = results
            .iter()
            .filter_map(|r| match &r.status {
                LoadStatus::Loaded { rule_id } => Some(rule_id.as_str()),
                _ => None,
            })
            .collect();

        assert_eq!(loaded.len(), 2, "Should load rules from root and subdirectory");

        let docs = loader.documents();
        let guard = docs.read().unwrap();
        assert!(guard.contains_key("test-rule"));
        assert!(guard.contains_key("sub-rule"));
    }

    #[test]
    fn load_file_two_pass_non_anomaly() {
        let (dir, loader) = temp_loader();
        let yaml = r#"
apiVersion: v1
kind: TrendConfig
metadata:
  id: trend-test
  name: Trend Test
  enabled: true
spec:
  default_window_size: 168
  min_data_points: 3
  z_score_trigger: 2.0
  direction_thresholds:
    up: 0.5
    down: 0.5
  severity_thresholds:
    notable: 2.0
    significant: 3.0
    critical: 4.0
"#;
        let path = dir.path().join("trend-test.yml");
        fs::write(&path, yaml).unwrap();

        let doc = loader.load_file(&path).unwrap();
        assert_eq!(doc.kind(), crate::schema::RuleKind::TrendConfig);
        assert_eq!(doc.metadata().id, "trend-test");
        assert!(doc.as_trend_config().is_some());
        // Should NOT be in anomaly_rules
        assert!(doc.as_anomaly().is_none());
    }

    #[test]
    fn load_all_multi_kind() {
        let (dir, loader) = temp_loader();

        // AnomalyRule
        fs::write(dir.path().join("anomaly.yml"), VALID_RULE_YAML).unwrap();

        // TrendConfig in subdirectory
        let sub = dir.path().join("scoring");
        fs::create_dir(&sub).unwrap();
        fs::write(sub.join("trend.yml"), r#"
apiVersion: v1
kind: TrendConfig
metadata:
  id: trend-cfg
  name: Trend Config
  enabled: true
spec:
  default_window_size: 168
  min_data_points: 3
  z_score_trigger: 2.0
  direction_thresholds:
    up: 0.5
    down: 0.5
  severity_thresholds:
    notable: 2.0
    significant: 3.0
    critical: 4.0
"#).unwrap();

        let results = loader.load_all().unwrap();
        let loaded: Vec<_> = results
            .iter()
            .filter(|r| matches!(r.status, LoadStatus::Loaded { .. }))
            .collect();
        assert_eq!(loaded.len(), 2);

        // Both in documents map
        let docs = loader.documents();
        let guard = docs.read().unwrap();
        assert_eq!(guard.len(), 2);

        // Only anomaly rule in the backward-compat map
        let rules = loader.rules();
        let guard = rules.read().unwrap();
        assert_eq!(guard.len(), 1);
        assert!(guard.contains_key("test-rule"));
    }

    #[test]
    fn write_and_read_back() {
        let (_dir, loader) = temp_loader();

        let rule: AnomalyRule = serde_yaml::from_str(VALID_RULE_YAML).unwrap();
        let path = loader.write_rule(&rule).unwrap();

        assert!(path.exists());
        assert!(path.file_name().unwrap().to_str().unwrap() == "test-rule.yml");

        // Read it back
        let loaded = loader.load_file(&path).unwrap();
        assert_eq!(loaded.metadata().id, rule.metadata.id);
        assert_eq!(loaded.metadata().name, rule.metadata.name);
    }

    #[test]
    fn write_document_non_anomaly() {
        let (dir, loader) = temp_loader();

        let yaml = include_str!("../../../data/rules/scoring/trend-config.yml");
        let rule: crate::trend_config::TrendConfigRule = serde_yaml::from_str(yaml).unwrap();
        let doc = RuleDocument::TrendConfig(rule);

        let path = loader.write_document(&doc).unwrap();
        assert!(path.exists());

        // Should be in documents but not in anomaly_rules
        let docs = loader.documents();
        let guard = docs.read().unwrap();
        assert!(guard.contains_key("trend-config-default"));

        let rules = loader.rules();
        let guard = rules.read().unwrap();
        assert!(!guard.contains_key("trend-config-default"));

        // Read back
        let loaded = loader.load_file(&path).unwrap();
        assert_eq!(loaded.kind(), crate::schema::RuleKind::TrendConfig);

        // Clean up
        let _ = fs::remove_file(dir.path().join("trend-config-default.yml"));
    }

    #[test]
    fn delete_rule_removes_file_and_entry() {
        let (_dir, loader) = temp_loader();

        let rule: AnomalyRule = serde_yaml::from_str(VALID_RULE_YAML).unwrap();
        let path = loader.write_rule(&rule).unwrap();
        assert!(path.exists());

        loader.delete_rule("test-rule").unwrap();
        assert!(!path.exists());

        // Removed from both maps
        let rules = loader.rules();
        let guard = rules.read().unwrap();
        assert!(!guard.contains_key("test-rule"));

        let docs = loader.documents();
        let guard = docs.read().unwrap();
        assert!(!guard.contains_key("test-rule"));
    }

    #[test]
    fn delete_nonexistent_rule_errors() {
        let (_dir, loader) = temp_loader();
        let err = loader.delete_rule("no-such-rule").unwrap_err();
        assert!(matches!(err, RuleError::Validation(_)));
    }

    #[test]
    fn invalid_yaml_produces_error_not_panic() {
        let (dir, loader) = temp_loader();
        let bad_path = dir.path().join("bad.yml");
        fs::write(&bad_path, "this: is: not: valid: yaml: [[[").unwrap();

        let result = loader.load_file(&bad_path);
        assert!(result.is_err());
    }

    #[test]
    fn empty_id_fails_validation() {
        let (dir, loader) = temp_loader();
        let yaml = r#"
apiVersion: v1
kind: AnomalyRule
metadata:
  id: ""
  name: Empty ID Rule
  enabled: true
schedule:
  cron: "* * * * *"
detection:
  template: spike
"#;
        let path = dir.path().join("empty-id.yml");
        fs::write(&path, yaml).unwrap();

        let result = loader.load_file(&path);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), RuleError::Validation(_)));
    }

    #[test]
    fn load_all_reports_failed_files() {
        let (dir, loader) = temp_loader();

        // One valid, one invalid
        fs::write(dir.path().join("good.yml"), VALID_RULE_YAML).unwrap();
        fs::write(dir.path().join("bad.yml"), "not valid yaml: [[[").unwrap();

        let results = loader.load_all().unwrap();

        let loaded = results
            .iter()
            .filter(|r| matches!(r.status, LoadStatus::Loaded { .. }))
            .count();
        let failed = results
            .iter()
            .filter(|r| matches!(r.status, LoadStatus::Failed { .. }))
            .count();

        assert_eq!(loaded, 1);
        assert_eq!(failed, 1);
    }

    #[test]
    fn new_creates_missing_directory() {
        let dir = TempDir::new().unwrap();
        let nested = dir.path().join("sub").join("rules");
        assert!(!nested.exists());

        let _loader = RuleLoader::new(nested.clone());
        assert!(nested.exists());
    }

    // ── Deep-merge tests ────────────────────────────────────────────

    #[test]
    fn deep_merge_child_scalar_wins() {
        let parent: serde_yaml::Value = serde_yaml::from_str("a: 1\nb: 2").unwrap();
        let child: serde_yaml::Value = serde_yaml::from_str("b: 99").unwrap();
        let merged = super::deep_merge(&parent, &child);
        let m = merged.as_mapping().unwrap();
        assert_eq!(
            m.get(&serde_yaml::Value::String("a".into())).and_then(|v| v.as_i64()),
            Some(1)
        );
        assert_eq!(
            m.get(&serde_yaml::Value::String("b".into())).and_then(|v| v.as_i64()),
            Some(99)
        );
    }

    #[test]
    fn deep_merge_nested_maps() {
        let parent: serde_yaml::Value =
            serde_yaml::from_str("spec:\n  a: 1\n  b: 2").unwrap();
        let child: serde_yaml::Value =
            serde_yaml::from_str("spec:\n  b: 99\n  c: 3").unwrap();
        let merged = super::deep_merge(&parent, &child);
        let spec = merged
            .as_mapping()
            .unwrap()
            .get(&serde_yaml::Value::String("spec".into()))
            .unwrap()
            .as_mapping()
            .unwrap();
        assert_eq!(spec.get(&serde_yaml::Value::String("a".into())).and_then(|v| v.as_i64()), Some(1));
        assert_eq!(spec.get(&serde_yaml::Value::String("b".into())).and_then(|v| v.as_i64()), Some(99));
        assert_eq!(spec.get(&serde_yaml::Value::String("c".into())).and_then(|v| v.as_i64()), Some(3));
    }

    #[test]
    fn deep_merge_arrays_replace_entirely() {
        let parent: serde_yaml::Value =
            serde_yaml::from_str("tags:\n  - a\n  - b").unwrap();
        let child: serde_yaml::Value =
            serde_yaml::from_str("tags:\n  - x").unwrap();
        let merged = super::deep_merge(&parent, &child);
        let tags = merged
            .as_mapping()
            .unwrap()
            .get(&serde_yaml::Value::String("tags".into()))
            .unwrap()
            .as_sequence()
            .unwrap();
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].as_str(), Some("x"));
    }

    #[test]
    fn resolve_extends_simple_chain() {
        let parent: serde_yaml::Value = serde_yaml::from_str(
            "metadata:\n  id: parent\n  name: Parent\n  enabled: true\nspec:\n  a: 1\n  b: 2"
        ).unwrap();
        let child: serde_yaml::Value = serde_yaml::from_str(
            "metadata:\n  id: child\n  name: Child\n  extends: parent\n  enabled: true\nspec:\n  b: 99"
        ).unwrap();

        let mut raw = HashMap::new();
        raw.insert("parent".to_string(), parent);
        raw.insert("child".to_string(), child);

        let resolved = super::resolve_extends(&raw).unwrap();
        let child_resolved = resolved.get("child").unwrap().as_mapping().unwrap();
        let spec = child_resolved
            .get(&serde_yaml::Value::String("spec".into()))
            .unwrap()
            .as_mapping()
            .unwrap();

        // Inherited from parent.
        assert_eq!(spec.get(&serde_yaml::Value::String("a".into())).and_then(|v| v.as_i64()), Some(1));
        // Overridden by child.
        assert_eq!(spec.get(&serde_yaml::Value::String("b".into())).and_then(|v| v.as_i64()), Some(99));
    }

    #[test]
    fn resolve_extends_circular_detected() {
        let a: serde_yaml::Value = serde_yaml::from_str(
            "metadata:\n  id: a\n  name: A\n  extends: b\n  enabled: true"
        ).unwrap();
        let b: serde_yaml::Value = serde_yaml::from_str(
            "metadata:\n  id: b\n  name: B\n  extends: a\n  enabled: true"
        ).unwrap();

        let mut raw = HashMap::new();
        raw.insert("a".to_string(), a);
        raw.insert("b".to_string(), b);

        let result = super::resolve_extends(&raw);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("circular"));
    }

    #[test]
    fn resolve_extends_missing_parent_errors() {
        let child: serde_yaml::Value = serde_yaml::from_str(
            "metadata:\n  id: child\n  name: Child\n  extends: nonexistent\n  enabled: true"
        ).unwrap();

        let mut raw = HashMap::new();
        raw.insert("child".to_string(), child);

        let result = super::resolve_extends(&raw);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    #[test]
    fn resolve_extends_no_extends_passthrough() {
        let rule: serde_yaml::Value = serde_yaml::from_str(
            "metadata:\n  id: standalone\n  name: Standalone\n  enabled: true\nspec:\n  x: 42"
        ).unwrap();

        let mut raw = HashMap::new();
        raw.insert("standalone".to_string(), rule.clone());

        let resolved = super::resolve_extends(&raw).unwrap();
        assert_eq!(resolved.get("standalone").unwrap(), &rule);
    }
}
