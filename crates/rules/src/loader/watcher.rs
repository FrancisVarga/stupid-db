//! Filesystem event handler for the notify watcher (hot-reload).

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::{Arc, RwLock};

use notify::event::{CreateKind, ModifyKind, RemoveKind};
use notify::{Event, EventKind};
use tracing::{info, warn};

use crate::schema::{AnomalyRule, RuleDocument, RuleEnvelope};

/// Handle a single filesystem event from the notify watcher.
pub(super) fn handle_fs_event(
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
