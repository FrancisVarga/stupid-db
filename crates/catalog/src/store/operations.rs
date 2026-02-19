use tracing::info;

use crate::catalog::{Catalog, PartialCatalog};
use crate::manifest::CatalogManifest;

use super::error::CatalogStoreError;
use super::CatalogStore;

impl CatalogStore {
    // ── Snapshots ───────────────────────────────────────────────

    /// Save a snapshot of the catalog with a timestamp-based filename.
    pub fn save_snapshot(&self, catalog: &Catalog) -> Result<String, CatalogStoreError> {
        let ts = chrono::Utc::now().format("%Y-%m-%dT%H-%M-%S").to_string();
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
