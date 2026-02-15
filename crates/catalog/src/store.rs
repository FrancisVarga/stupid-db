use std::path::{Path, PathBuf};

use chrono::Utc;
use thiserror::Error;
use tracing::info;

use crate::catalog::{Catalog, PartialCatalog};
use crate::manifest::CatalogManifest;

#[derive(Debug, Error)]
pub enum CatalogStoreError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

/// Filesystem-backed catalog persistence.
///
/// Manages the `data/catalog/` directory structure:
/// ```text
/// catalog/
///   current.json              ← latest merged catalog
///   manifest.json             ← segment IDs + hash, timestamp
///   segments/
///     Login_2024-W03.json     ← per-segment partial catalog
///   snapshots/
///     2026-02-16T04-16-00.json  ← historical snapshots
/// ```
pub struct CatalogStore {
    base_dir: PathBuf,
}

impl CatalogStore {
    /// Create a new CatalogStore, ensuring the directory structure exists.
    pub fn new(base_dir: impl Into<PathBuf>) -> Result<Self, CatalogStoreError> {
        let base_dir = base_dir.into();
        std::fs::create_dir_all(base_dir.join("segments"))?;
        std::fs::create_dir_all(base_dir.join("snapshots"))?;
        Ok(Self { base_dir })
    }

    /// Base path for this store.
    pub fn base_dir(&self) -> &Path {
        &self.base_dir
    }

    // ── Current catalog ─────────────────────────────────────────

    /// Save the merged catalog as `current.json`.
    pub fn save_current(&self, catalog: &Catalog) -> Result<(), CatalogStoreError> {
        let json = serde_json::to_string_pretty(catalog)?;
        std::fs::write(self.base_dir.join("current.json"), json)?;
        Ok(())
    }

    /// Load the merged catalog from `current.json`.
    pub fn load_current(&self) -> Result<Option<Catalog>, CatalogStoreError> {
        let path = self.base_dir.join("current.json");
        if !path.exists() {
            return Ok(None);
        }
        let json = std::fs::read_to_string(path)?;
        let catalog = serde_json::from_str(&json)?;
        Ok(Some(catalog))
    }

    // ── Partial catalogs (per-segment) ──────────────────────────

    /// Save a partial catalog for a specific segment.
    ///
    /// Segment IDs may contain `/` (e.g. `Login/2025-W24`), so we
    /// flatten to a safe filename by replacing `/` with `__`.
    pub fn save_partial(
        &self,
        segment_id: &str,
        partial: &PartialCatalog,
    ) -> Result<(), CatalogStoreError> {
        let filename = segment_filename(segment_id);
        let path = self.base_dir.join("segments").join(filename);
        let json = serde_json::to_string_pretty(partial)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    /// Load a partial catalog for a specific segment.
    pub fn load_partial(&self, segment_id: &str) -> Result<Option<PartialCatalog>, CatalogStoreError> {
        let filename = segment_filename(segment_id);
        let path = self.base_dir.join("segments").join(filename);
        if !path.exists() {
            return Ok(None);
        }
        let json = std::fs::read_to_string(path)?;
        let partial = serde_json::from_str(&json)?;
        Ok(Some(partial))
    }

    /// List all partial catalog segment IDs on disk.
    pub fn list_partials(&self) -> Result<Vec<String>, CatalogStoreError> {
        let dir = self.base_dir.join("segments");
        let mut ids = Vec::new();
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if let Some(stem) = name.strip_suffix(".json") {
                // Reverse the flattening: `__` back to `/`
                ids.push(stem.replace("__", "/"));
            }
        }
        ids.sort();
        Ok(ids)
    }

    /// Remove a partial catalog for a specific segment.
    pub fn remove_partial(&self, segment_id: &str) -> Result<(), CatalogStoreError> {
        let filename = segment_filename(segment_id);
        let path = self.base_dir.join("segments").join(filename);
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        Ok(())
    }

    // ── Manifest ────────────────────────────────────────────────

    /// Save the catalog manifest.
    pub fn save_manifest(&self, manifest: &CatalogManifest) -> Result<(), CatalogStoreError> {
        let json = serde_json::to_string_pretty(manifest)?;
        std::fs::write(self.base_dir.join("manifest.json"), json)?;
        Ok(())
    }

    /// Load the catalog manifest.
    pub fn load_manifest(&self) -> Result<Option<CatalogManifest>, CatalogStoreError> {
        let path = self.base_dir.join("manifest.json");
        if !path.exists() {
            return Ok(None);
        }
        let json = std::fs::read_to_string(path)?;
        let manifest = serde_json::from_str(&json)?;
        Ok(Some(manifest))
    }

    // ── Snapshots ───────────────────────────────────────────────

    /// Save a snapshot of the catalog with a timestamp-based filename.
    pub fn save_snapshot(&self, catalog: &Catalog) -> Result<String, CatalogStoreError> {
        let ts = Utc::now().format("%Y-%m-%dT%H-%M-%S").to_string();
        let filename = format!("{}.json", ts);
        let path = self.base_dir.join("snapshots").join(&filename);
        let json = serde_json::to_string_pretty(catalog)?;
        std::fs::write(path, json)?;
        Ok(filename)
    }

    // ── High-level operations ───────────────────────────────────

    /// Rebuild the full catalog from all persisted partial catalogs.
    ///
    /// Loads every `segments/*.json` file, merges them via
    /// `Catalog::from_partials()`, and saves `current.json`, manifest,
    /// and a snapshot.
    pub fn rebuild_from_partials(&self) -> Result<Catalog, CatalogStoreError> {
        let segment_ids = self.list_partials()?;
        let mut partials = Vec::with_capacity(segment_ids.len());

        for seg_id in &segment_ids {
            if let Some(partial) = self.load_partial(seg_id)? {
                partials.push(partial);
            }
        }

        let catalog = Catalog::from_partials(&partials);

        self.save_current(&catalog)?;
        let manifest = CatalogManifest::new(&segment_ids);
        self.save_manifest(&manifest)?;
        self.save_snapshot(&catalog)?;

        info!(
            "Catalog rebuilt from {} partials: {} nodes, {} edges",
            partials.len(),
            catalog.total_nodes,
            catalog.total_edges
        );

        Ok(catalog)
    }

    /// Incrementally add a new segment's partial catalog.
    ///
    /// Merges the partial into the existing `current.json` (O(1) merge
    /// of one partial), then updates manifest and writes a snapshot.
    pub fn add_segment(
        &self,
        segment_id: &str,
        partial: &PartialCatalog,
    ) -> Result<Catalog, CatalogStoreError> {
        // Persist the partial.
        self.save_partial(segment_id, partial)?;

        // Load existing catalog (or start empty).
        let existing = self.load_current()?.unwrap_or_else(|| Catalog::from_partials(&[]));

        // Merge: treat existing as a "partial" + new partial.
        let existing_as_partial = PartialCatalog {
            segment_id: "__existing__".to_string(),
            entity_types: existing.entity_types,
            edge_types: existing.edge_types,
            node_count: existing.total_nodes,
            edge_count: existing.total_edges,
        };

        let merged = Catalog::from_partials(&[existing_as_partial, partial.clone()]);

        // Save updated state.
        self.save_current(&merged)?;
        let segment_ids = self.list_partials()?;
        let manifest = CatalogManifest::new(&segment_ids);
        self.save_manifest(&manifest)?;
        self.save_snapshot(&merged)?;

        info!(
            "Catalog updated after adding '{}': {} nodes, {} edges",
            segment_id, merged.total_nodes, merged.total_edges
        );

        Ok(merged)
    }

    /// Remove a segment and rebuild the catalog from remaining partials.
    ///
    /// Removal requires a full re-merge because we cannot subtract a
    /// partial's counts from the merged totals without re-summing.
    pub fn remove_segment(&self, segment_id: &str) -> Result<Catalog, CatalogStoreError> {
        self.remove_partial(segment_id)?;
        info!("Removed partial for '{}', rebuilding catalog...", segment_id);
        self.rebuild_from_partials()
    }
}

/// Convert a segment ID (which may contain `/`) to a safe JSON filename.
fn segment_filename(segment_id: &str) -> String {
    format!("{}.json", segment_id.replace('/', "__"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{CatalogEntry, EdgeSummary};

    fn make_partial(segment_id: &str, node_count: usize, edge_count: usize) -> PartialCatalog {
        PartialCatalog {
            segment_id: segment_id.to_string(),
            entity_types: vec![CatalogEntry {
                entity_type: "Member".to_string(),
                node_count,
                sample_keys: vec!["alice".to_string()],
            }],
            edge_types: vec![EdgeSummary {
                edge_type: "LoggedInFrom".to_string(),
                count: edge_count,
                source_types: vec!["Member".to_string()],
                target_types: vec!["Device".to_string()],
            }],
            node_count,
            edge_count,
        }
    }

    #[test]
    fn store_create_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        let store = CatalogStore::new(tmp.path().join("catalog")).unwrap();
        assert!(store.base_dir().join("segments").exists());
        assert!(store.base_dir().join("snapshots").exists());
    }

    #[test]
    fn store_save_load_current() {
        let tmp = tempfile::tempdir().unwrap();
        let store = CatalogStore::new(tmp.path().join("catalog")).unwrap();

        let cat = Catalog::from_partials(&[make_partial("seg-1", 10, 5)]);
        store.save_current(&cat).unwrap();

        let loaded = store.load_current().unwrap().unwrap();
        assert_eq!(loaded.total_nodes, 10);
        assert_eq!(loaded.total_edges, 5);
    }

    #[test]
    fn store_load_current_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let store = CatalogStore::new(tmp.path().join("catalog")).unwrap();
        assert!(store.load_current().unwrap().is_none());
    }

    #[test]
    fn store_save_load_partial() {
        let tmp = tempfile::tempdir().unwrap();
        let store = CatalogStore::new(tmp.path().join("catalog")).unwrap();

        let partial = make_partial("Login/2025-W24", 100, 50);
        store.save_partial("Login/2025-W24", &partial).unwrap();

        let loaded = store.load_partial("Login/2025-W24").unwrap().unwrap();
        assert_eq!(loaded.segment_id, "Login/2025-W24");
        assert_eq!(loaded.node_count, 100);
    }

    #[test]
    fn store_list_partials() {
        let tmp = tempfile::tempdir().unwrap();
        let store = CatalogStore::new(tmp.path().join("catalog")).unwrap();

        store.save_partial("seg-a", &make_partial("seg-a", 10, 5)).unwrap();
        store.save_partial("seg-b", &make_partial("seg-b", 20, 10)).unwrap();

        let ids = store.list_partials().unwrap();
        assert_eq!(ids, vec!["seg-a", "seg-b"]);
    }

    #[test]
    fn store_remove_partial() {
        let tmp = tempfile::tempdir().unwrap();
        let store = CatalogStore::new(tmp.path().join("catalog")).unwrap();

        store.save_partial("seg-a", &make_partial("seg-a", 10, 5)).unwrap();
        store.remove_partial("seg-a").unwrap();

        assert!(store.load_partial("seg-a").unwrap().is_none());
        assert!(store.list_partials().unwrap().is_empty());
    }

    #[test]
    fn store_save_load_manifest() {
        let tmp = tempfile::tempdir().unwrap();
        let store = CatalogStore::new(tmp.path().join("catalog")).unwrap();

        let manifest = CatalogManifest::new(&["seg-1".to_string(), "seg-2".to_string()]);
        store.save_manifest(&manifest).unwrap();

        let loaded = store.load_manifest().unwrap().unwrap();
        assert_eq!(loaded.segment_ids, vec!["seg-1", "seg-2"]);
        assert_eq!(loaded.segments_hash, manifest.segments_hash);
    }

    #[test]
    fn store_save_snapshot() {
        let tmp = tempfile::tempdir().unwrap();
        let store = CatalogStore::new(tmp.path().join("catalog")).unwrap();

        let cat = Catalog::from_partials(&[make_partial("seg-1", 10, 5)]);
        let filename = store.save_snapshot(&cat).unwrap();
        assert!(filename.ends_with(".json"));

        let path = store.base_dir().join("snapshots").join(&filename);
        assert!(path.exists());
    }

    #[test]
    fn store_rebuild_from_partials() {
        let tmp = tempfile::tempdir().unwrap();
        let store = CatalogStore::new(tmp.path().join("catalog")).unwrap();

        store.save_partial("seg-a", &make_partial("seg-a", 10, 5)).unwrap();
        store.save_partial("seg-b", &make_partial("seg-b", 20, 10)).unwrap();

        let catalog = store.rebuild_from_partials().unwrap();
        assert_eq!(catalog.total_nodes, 30);
        assert_eq!(catalog.total_edges, 15);

        // current.json and manifest.json should exist
        assert!(store.load_current().unwrap().is_some());
        assert!(store.load_manifest().unwrap().is_some());

        let manifest = store.load_manifest().unwrap().unwrap();
        assert!(manifest.is_fresh(&["seg-a".to_string(), "seg-b".to_string()]));
    }

    #[test]
    fn store_rebuild_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let store = CatalogStore::new(tmp.path().join("catalog")).unwrap();

        let catalog = store.rebuild_from_partials().unwrap();
        assert_eq!(catalog.total_nodes, 0);
        assert_eq!(catalog.total_edges, 0);
    }

    #[test]
    fn store_add_segment() {
        let tmp = tempfile::tempdir().unwrap();
        let store = CatalogStore::new(tmp.path().join("catalog")).unwrap();

        let cat1 = store.add_segment("seg-a", &make_partial("seg-a", 10, 5)).unwrap();
        assert_eq!(cat1.total_nodes, 10);

        let cat2 = store.add_segment("seg-b", &make_partial("seg-b", 20, 10)).unwrap();
        assert_eq!(cat2.total_nodes, 30);
        assert_eq!(cat2.total_edges, 15);
    }

    #[test]
    fn store_remove_segment() {
        let tmp = tempfile::tempdir().unwrap();
        let store = CatalogStore::new(tmp.path().join("catalog")).unwrap();

        store.add_segment("seg-a", &make_partial("seg-a", 10, 5)).unwrap();
        store.add_segment("seg-b", &make_partial("seg-b", 20, 10)).unwrap();

        let catalog = store.remove_segment("seg-a").unwrap();
        // After removing seg-a, only seg-b remains
        assert_eq!(catalog.total_nodes, 20);
        assert_eq!(catalog.total_edges, 10);
    }

    #[test]
    fn store_segment_filename_with_slashes() {
        let tmp = tempfile::tempdir().unwrap();
        let store = CatalogStore::new(tmp.path().join("catalog")).unwrap();

        let partial = make_partial("Login/2025-W24", 10, 5);
        store.save_partial("Login/2025-W24", &partial).unwrap();

        let ids = store.list_partials().unwrap();
        assert_eq!(ids, vec!["Login/2025-W24"]);

        let loaded = store.load_partial("Login/2025-W24").unwrap().unwrap();
        assert_eq!(loaded.node_count, 10);
    }
}
