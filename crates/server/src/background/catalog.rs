use std::sync::Arc;

use tokio::sync::RwLock;
use tracing::info;

use crate::athena_connections::AthenaConnectionStore;
use crate::credential_store::CredentialStore;
use crate::state::SharedGraph;

/// Build the catalog from the graph, persist per-segment partials and merged catalog.
pub(super) async fn build_and_persist_catalog(
    shared_graph: &SharedGraph,
    segments: &[String],
    catalog_store: &stupid_catalog::CatalogStore,
) -> stupid_catalog::Catalog {
    let graph_read = shared_graph.read().await;

    // Build per-segment partial catalogs and persist them.
    let mut partials = Vec::with_capacity(segments.len());
    for seg_id in segments {
        let partial = stupid_catalog::PartialCatalog::from_graph_segment(&graph_read, seg_id);
        if let Err(e) = catalog_store.save_partial(seg_id, &partial) {
            tracing::warn!("Failed to persist partial catalog for '{}': {}", seg_id, e);
        }
        partials.push(partial);
    }
    drop(graph_read);

    // Merge all partials into the full catalog.
    let catalog = stupid_catalog::Catalog::from_partials(&partials);

    // Persist merged catalog, manifest, and snapshot.
    if let Err(e) = catalog_store.save_current(&catalog) {
        tracing::warn!("Failed to persist current catalog: {}", e);
    }
    let segment_ids: Vec<String> = segments.to_vec();
    let manifest = stupid_catalog::CatalogManifest::new(&segment_ids);
    if let Err(e) = catalog_store.save_manifest(&manifest) {
        tracing::warn!("Failed to persist catalog manifest: {}", e);
    }
    if let Err(e) = catalog_store.save_snapshot(&catalog) {
        tracing::warn!("Failed to save catalog snapshot: {}", e);
    }

    info!(
        "Catalog built and persisted: {} entity types, {} edge types ({} nodes, {} edges)",
        catalog.entity_types.len(),
        catalog.edge_types.len(),
        catalog.total_nodes,
        catalog.total_edges
    );

    catalog
}

/// Sync Athena external sources into the catalog and persist them.
///
/// Returns the final catalog with external sources merged in.
pub(super) async fn sync_catalog_with_external_sources(
    mut cat: stupid_catalog::Catalog,
    catalog_store: &stupid_catalog::CatalogStore,
    athena_connections: &Arc<RwLock<AthenaConnectionStore>>,
) -> stupid_catalog::Catalog {
    // Build and persist external sources from Athena connections.
    {
        let athena_store = athena_connections.read().await;
        if let Ok(conns) = athena_store.list() {
            let sources: Vec<stupid_catalog::ExternalSource> = conns
                .iter()
                .filter(|c| c.enabled && c.schema.is_some())
                .map(|c| {
                    let schema = c.schema.as_ref().unwrap();
                    stupid_catalog::ExternalSource {
                        name: c.name.clone(),
                        kind: "athena".to_string(),
                        connection_id: c.id.clone(),
                        databases: schema
                            .databases
                            .iter()
                            .map(|db| stupid_catalog::ExternalDatabase {
                                name: db.name.clone(),
                                tables: db
                                    .tables
                                    .iter()
                                    .map(|t| stupid_catalog::ExternalTable {
                                        name: t.name.clone(),
                                        columns: t
                                            .columns
                                            .iter()
                                            .map(|col| stupid_catalog::ExternalColumn {
                                                name: col.name.clone(),
                                                data_type: col.data_type.clone(),
                                            })
                                            .collect(),
                                    })
                                    .collect(),
                            })
                            .collect(),
                    }
                })
                .collect();

            if !sources.is_empty() {
                info!("Persisting {} Athena source(s) to catalog/external/", sources.len());
                for source in &sources {
                    if let Err(e) = catalog_store.save_external_source(source) {
                        tracing::warn!("Failed to persist external source '{}': {}", source.connection_id, e);
                    }
                }
            }
        }
        drop(athena_store);
    }

    // Load all persisted external sources from catalog/external/*.json and merge into catalog.
    if let Ok(persisted_sources) = catalog_store.list_external_sources() {
        if !persisted_sources.is_empty() {
            info!("Loaded {} external source(s) from catalog/external/", persisted_sources.len());
            cat = cat.with_external_sources(persisted_sources);
        }
    }

    cat
}
