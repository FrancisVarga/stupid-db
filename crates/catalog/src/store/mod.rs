mod error;
mod external;
mod operations;

pub use error::CatalogStoreError;

use std::path::{Path, PathBuf};

use crate::catalog::{Catalog, PartialCatalog};
use crate::manifest::CatalogManifest;

use error::segment_filename;

/// Filesystem-backed catalog persistence.
///
/// Manages the `data/catalog/` directory structure:
/// ```text
/// catalog/
///   current.json              <- latest merged catalog (graph data only)
///   manifest.json             <- segment IDs + hash, timestamp
///   segments/
///     Login_2024-W03.json     <- per-segment partial catalog
///   external/
///     athena-{connection-id}/
///       metadata.json         <- connection metadata (name, kind, connection_id)
///       {database}/
///         {table}.json        <- table schema (name, columns)
///   snapshots/
///     2026-02-16T04-16-00.json  <- historical snapshots
/// ```
pub struct CatalogStore {
    base_dir: PathBuf,
}

impl CatalogStore {
    /// Create a new CatalogStore, ensuring the directory structure exists.
    pub fn new(base_dir: impl Into<PathBuf>) -> Result<Self, CatalogStoreError> {
        let base_dir = base_dir.into();
        std::fs::create_dir_all(base_dir.join("segments"))?;
        std::fs::create_dir_all(base_dir.join("external"))?;
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
    pub fn load_partial(
        &self,
        segment_id: &str,
    ) -> Result<Option<PartialCatalog>, CatalogStoreError> {
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
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
