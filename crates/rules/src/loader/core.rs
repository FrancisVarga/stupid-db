//! Core [`RuleLoader`] struct: filesystem-backed rule loading with optional hot-reload.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use tracing::{info, warn};

use crate::schema::{AnomalyRule, RuleDocument, RuleEnvelope};

use super::error::{LoadResult, LoadStatus, Result, RuleError};
use super::watcher::handle_fs_event;

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
    pub(super) fn insert_document(&self, id: String, doc: RuleDocument) {
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

        let mut watcher = notify::recommended_watcher(move |res: std::result::Result<notify::Event, notify::Error>| {
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

    /// Get the rules directory path.
    pub fn rules_dir(&self) -> &Path {
        &self.rules_dir
    }

    /// Get the shared anomaly rules map (backward compatibility).
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
