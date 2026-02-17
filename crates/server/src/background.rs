use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::Arc;

use tokio::sync::RwLock;
use tracing::info;

use stupid_storage::StorageEngine;

use crate::credential_store::CredentialStore;
use crate::graph_ops::{extract_graph_ops, apply_graph_op, GraphOp};
use crate::state::{self, LoadingPhase, LoadingState, SharedGraph, SharedPipeline};

pub(crate) fn discover_segments(data_dir: &Path) -> Vec<String> {
    let segments_dir = data_dir.join("segments");
    if !segments_dir.exists() {
        return Vec::new();
    }

    let mut segments = Vec::new();
    for entry in walkdir::WalkDir::new(&segments_dir)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if path.is_file() && path.file_name().map(|n| n == "documents.dat").unwrap_or(false) {
            // segment_id = path relative to segments_dir, minus the filename
            if let Ok(rel) = path.parent().unwrap_or(path).strip_prefix(&segments_dir) {
                if let Some(seg_id) = rel.to_str() {
                    // Normalize backslashes to forward slashes
                    segments.push(seg_id.replace('\\', "/"));
                }
            }
        }
    }

    segments.sort();
    segments
}

/// Background task: discover segments (local or S3), build graph, catalog, and compute.
pub(crate) async fn background_load(
    storage: StorageEngine,
    data_dir: PathBuf,
    single_segment: Option<String>,
    shared_graph: SharedGraph,
    knowledge: stupid_compute::SharedKnowledgeState,
    pipeline: SharedPipeline,
    catalog: Arc<RwLock<Option<stupid_catalog::Catalog>>>,
    segment_ids_shared: Arc<RwLock<Vec<String>>>,
    doc_count_shared: Arc<std::sync::atomic::AtomicU64>,
    loading: Arc<LoadingState>,
    app_state: Arc<state::AppState>,
) -> anyhow::Result<()> {
    // Phase 1: Discover segments.
    loading.set_phase(LoadingPhase::Discovering).await;
    info!("Discovering segments...");

    let (segments, effective_data_dir) = if let Some(seg_id) = single_segment {
        info!("Single segment mode: '{}'", seg_id);
        (vec![seg_id], data_dir.clone())
    } else {
        let local_segments = discover_segments(&data_dir);
        if !local_segments.is_empty() {
            info!("Found {} local segments", local_segments.len());
            (local_segments, data_dir.clone())
        } else {
            let remote = storage.discover_segments().await?;
            if remote.is_empty() {
                let msg = "No segments found (local or remote). Run 'import', 'import-dir', or 'import-s3' first.";
                loading.set_phase(LoadingPhase::Failed(msg.to_string())).await;
                anyhow::bail!("{}", msg);
            }

            // Pre-fetch S3 segments to local cache.
            // SegmentReader requires local files, so we download before graph building.
            info!("Downloading {} remote segments to local cache...", remote.len());
            let mut cache_dir = None;
            for (i, seg_id) in remote.iter().enumerate() {
                let dir = storage.segment_data_dir(seg_id).await?;
                cache_dir = Some(dir);
                if (i + 1) % 10 == 0 || i + 1 == remote.len() {
                    info!("  Downloaded {}/{} segments", i + 1, remote.len());
                }
            }
            let effective = cache_dir.unwrap_or_else(|| data_dir.clone());
            (remote, effective)
        }
    };

    load_segments_and_compute(
        &data_dir, &effective_data_dir, &segments,
        shared_graph, knowledge, pipeline, catalog,
        segment_ids_shared, doc_count_shared, loading, app_state,
    ).await
}

/// Build the catalog from the graph, persist per-segment partials and merged catalog.
async fn build_and_persist_catalog(
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

struct SegmentResult {
    seg_id: String,
    ops: Vec<GraphOp>,
    doc_count: u64,
    elapsed: std::time::Duration,
}

/// Shared graph building + catalog + compute logic used by both AWS and local loaders.
async fn load_segments_and_compute(
    _data_dir: &std::path::Path,
    effective_data_dir: &std::path::Path,
    segments: &[String],
    shared_graph: SharedGraph,
    knowledge: stupid_compute::SharedKnowledgeState,
    pipeline: SharedPipeline,
    catalog: Arc<RwLock<Option<stupid_catalog::Catalog>>>,
    segment_ids_shared: Arc<RwLock<Vec<String>>>,
    doc_count_shared: Arc<std::sync::atomic::AtomicU64>,
    loading: Arc<LoadingState>,
    app_state: Arc<state::AppState>,
) -> anyhow::Result<()> {
    let total = segments.len() as u64;
    loading.set_progress(0, total);

    // Store segment IDs immediately so /stats can show them.
    {
        let mut ids = segment_ids_shared.write().await;
        *ids = segments.to_vec();
    }

    // Phase 2: Build graph from segments.
    loading.set_phase(LoadingPhase::LoadingSegments).await;
    let reader_threads = 4usize;
    info!(
        "Loading {} segments and building graph ({} reader threads, streaming ops)...",
        segments.len(), reader_threads
    );

    let (graph, doc_count) = {
        use rayon::prelude::*;
        use std::sync::atomic::AtomicU64;

        let start = std::time::Instant::now();
        let mut graph = stupid_graph::GraphStore::new();
        let mut total_docs: u64 = 0;

        let (tx, rx) = std::sync::mpsc::sync_channel::<SegmentResult>(1);

        let segments_for_pool: Vec<String> = segments.to_vec();
        let data_dir_for_pool = effective_data_dir.to_path_buf();
        let skipped = Arc::new(AtomicU64::new(0));
        let skipped_clone = skipped.clone();

        let producer = std::thread::spawn(move || {
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(reader_threads)
                .build()
                .expect("Failed to create segment reader thread pool");

            pool.install(|| {
                segments_for_pool.par_iter().for_each(|seg_id| {
                    let seg_start = std::time::Instant::now();
                    let reader = match stupid_segment::reader::SegmentReader::open(&data_dir_for_pool, seg_id) {
                        Ok(r) => r,
                        Err(e) => {
                            tracing::warn!("Skipping segment '{}': {}", seg_id, e);
                            skipped_clone.fetch_add(1, Ordering::Relaxed);
                            let _ = tx.send(SegmentResult {
                                seg_id: seg_id.clone(), ops: Vec::new(),
                                doc_count: 0, elapsed: seg_start.elapsed(),
                            });
                            return;
                        }
                    };

                    let mut ops = Vec::new();
                    let mut doc_count = 0u64;
                    for doc_result in reader.iter() {
                        match doc_result {
                            Ok(doc) => {
                                extract_graph_ops(&doc, seg_id, &mut ops);
                                doc_count += 1;
                            }
                            Err(e) => {
                                tracing::warn!("Bad document in '{}': {}", seg_id, e);
                            }
                        }
                    }

                    let _ = tx.send(SegmentResult {
                        seg_id: seg_id.clone(), ops, doc_count,
                        elapsed: seg_start.elapsed(),
                    });
                });
            });
        });

        let mut seg_num = 0u64;
        while let Ok(result) = rx.recv() {
            for op in &result.ops {
                apply_graph_op(op, &mut graph, &result.seg_id);
            }

            total_docs += result.doc_count;
            seg_num += 1;
            loading.set_progress(seg_num, total);

            info!(
                "  [{}/{}] Loaded segment '{}': {} docs, {} ops in {:.1}s (total: {} docs, {:.1}s)",
                seg_num, segments.len(), result.seg_id,
                result.doc_count, result.ops.len(),
                result.elapsed.as_secs_f64(),
                total_docs, start.elapsed().as_secs_f64()
            );
        }

        let _ = producer.join();
        let final_skipped = skipped.load(Ordering::Relaxed);
        info!(
            "Graph built: {} docs from {} segments in {:.1}s ({} skipped)",
            total_docs, segments.len() - final_skipped as usize,
            start.elapsed().as_secs_f64(), final_skipped
        );

        (graph, total_docs)
    };

    doc_count_shared.store(doc_count, Ordering::Relaxed);

    let stats = graph.stats();
    info!(
        "Graph ready: {} nodes, {} edges from {} documents across {} segments",
        stats.node_count, stats.edge_count, doc_count, segments.len()
    );

    {
        let mut graph_lock = shared_graph.write().await;
        *graph_lock = graph;
    }

    loading.set_phase(LoadingPhase::Ready).await;
    info!(
        "Graph loaded and server ready in {:.1}s — catalog and compute will build in background",
        loading.started_at.elapsed().as_secs_f64()
    );

    // ── Catalog persistence: check freshness, load or rebuild ──
    info!("Building catalog...");
    let catalog_store = &app_state.catalog_store;
    {
        // Check if persisted catalog is fresh.
        let manifest_fresh = match catalog_store.load_manifest() {
            Ok(Some(manifest)) => {
                let fresh = manifest.is_fresh(segments);
                if fresh {
                    info!("Persisted catalog manifest is fresh ({} segments)", segments.len());
                } else {
                    info!("Persisted catalog manifest is stale — rebuilding");
                }
                fresh
            }
            Ok(None) => {
                info!("No persisted catalog manifest — building from scratch");
                false
            }
            Err(e) => {
                tracing::warn!("Failed to load catalog manifest: {} — rebuilding", e);
                false
            }
        };

        let mut cat = if manifest_fresh {
            // Fast path: load from disk.
            match catalog_store.load_current() {
                Ok(Some(c)) => {
                    info!("Loaded catalog from disk: {} entity types, {} edge types", c.entity_types.len(), c.edge_types.len());
                    c
                }
                _ => {
                    info!("Failed to load current.json despite fresh manifest — rebuilding from graph");
                    build_and_persist_catalog(&shared_graph, segments, catalog_store).await
                }
            }
        } else {
            // Slow path: build from graph and persist.
            build_and_persist_catalog(&shared_graph, segments, catalog_store).await
        };

        // Build and persist external sources from Athena connections.
        {
            let athena_store = app_state.athena_connections.read().await;
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
                    // Persist each external source to catalog/external/{kind}-{id}.json
                    for source in &sources {
                        if let Err(e) = catalog_store.save_external_source(source) {
                            tracing::warn!("Failed to persist external source '{}': {}", source.connection_id, e);
                        }
                    }
                }
            }
            drop(athena_store);
        }

        // Load all persisted external sources from catalog/external/*.json and merge into catalog
        if let Ok(persisted_sources) = catalog_store.list_external_sources() {
            if !persisted_sources.is_empty() {
                info!("Loaded {} external source(s) from catalog/external/", persisted_sources.len());
                cat = cat.with_external_sources(persisted_sources);
            }
        }

        let mut catalog_lock = catalog.write().await;
        *catalog_lock = Some(cat);
    }
    info!("Catalog ready");

    info!("Starting compute scheduler in background...");
    {
        let sched_config = stupid_compute::SchedulerConfig::default();
        let mut scheduler = stupid_compute::Scheduler::new(sched_config, knowledge.clone());

        let p2_interval = std::time::Duration::from_secs(3600);
        scheduler.register_task(Arc::new(
            stupid_compute::scheduler::tasks::PageRankTask::new(shared_graph.clone(), p2_interval),
        ));
        scheduler.register_task(Arc::new(
            stupid_compute::scheduler::tasks::DegreeCentralityTask::new(shared_graph.clone(), p2_interval),
        ));
        scheduler.register_task(Arc::new(
            stupid_compute::scheduler::tasks::CommunityDetectionTask::new(shared_graph.clone(), p2_interval),
        ));
        scheduler.register_task(Arc::new(
            stupid_compute::AnomalyDetectionTask::new(p2_interval),
        ));

        scheduler.add_dependency("entity_extraction", "pagerank");
        scheduler.add_dependency("entity_extraction", "community_detection");

        info!("Running initial PageRank, degree, community computations...");
        {
            let graph_read = shared_graph.read().await;
            let t = std::time::Instant::now();
            let pagerank = stupid_compute::algorithms::pagerank::pagerank_default(&graph_read);
            info!("  pagerank done in {:.1}s ({} nodes)", t.elapsed().as_secs_f64(), pagerank.len());

            let t = std::time::Instant::now();
            let degrees = stupid_compute::algorithms::degree::degree_centrality(&graph_read);
            info!("  degree_centrality done in {:.1}s ({} nodes)", t.elapsed().as_secs_f64(), degrees.len());

            let t = std::time::Instant::now();
            let communities = stupid_compute::algorithms::communities::label_propagation_default(&graph_read);
            info!("  community_detection done in {:.1}s ({} communities)", t.elapsed().as_secs_f64(), communities.len());

            drop(graph_read);

            let mut state = knowledge.write().unwrap();
            state.pagerank = pagerank;
            state.degrees = degrees;
            state.communities = communities;
        }
        {
            let k = knowledge.read().unwrap();
            info!(
                "Initial compute complete — knowledge state: {} pagerank scores, {} degree entries, {} communities",
                k.pagerank.len(), k.degrees.len(), k.communities.len()
            );
        }

        info!("Running compute pipeline (hot_connect + warm_compute)...");
        {
            let pipeline_start = std::time::Instant::now();
            let mut all_docs: Vec<stupid_core::Document> = Vec::new();

            for seg_id in segments {
                let reader = match stupid_segment::reader::SegmentReader::open(effective_data_dir, seg_id) {
                    Ok(r) => r,
                    Err(_) => continue,
                };

                let mut seg_docs: Vec<stupid_core::Document> = Vec::new();
                for doc_result in reader.iter() {
                    if let Ok(doc) = doc_result {
                        seg_docs.push(doc);
                    }
                }

                {
                    let mut pipe = pipeline.lock().unwrap();
                    let mut state = knowledge.write().unwrap();
                    pipe.hot_connect(&seg_docs, &mut state);
                }

                all_docs.extend(seg_docs);

                if all_docs.len() > 100_000 {
                    let mut pipe = pipeline.lock().unwrap();
                    let mut state = knowledge.write().unwrap();
                    pipe.warm_compute(&mut state, &all_docs);
                    all_docs.clear();
                }
            }

            if !all_docs.is_empty() {
                let mut pipe = pipeline.lock().unwrap();
                let mut state = knowledge.write().unwrap();
                pipe.warm_compute(&mut state, &all_docs);
            }

            let k = knowledge.read().unwrap();
            info!(
                "Pipeline complete in {:.1}s — {} anomalies, {} trends, {} co-occurrence matrices, {} clusters",
                pipeline_start.elapsed().as_secs_f64(),
                k.anomalies.len(),
                k.trends.len(),
                k.cooccurrence.len(),
                k.clusters.len()
            );
        }

        let shutdown = scheduler.shutdown_signal();
        let metrics = scheduler.metrics_handle();
        {
            let mut sched_lock = app_state.scheduler.write().await;
            *sched_lock = Some(state::SchedulerHandle { shutdown, metrics });
        }

        std::thread::spawn(move || {
            scheduler.run();
        });
    }

    Ok(())
}
