use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use tokio::sync::RwLock;
use tracing::info;

use stupid_storage::StorageEngine;

use crate::graph_ops::{apply_graph_op, extract_graph_ops, GraphOp};
use crate::state::{self, LoadingPhase, LoadingState, SharedGraph, SharedPipeline};

use super::catalog::{build_and_persist_catalog, sync_catalog_with_external_sources};
use super::compute::run_compute;
use super::discovery::discover_segments;

struct SegmentResult {
    seg_id: String,
    ops: Vec<GraphOp>,
    doc_count: u64,
    elapsed: std::time::Duration,
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
        &effective_data_dir, &segments,
        shared_graph, knowledge, pipeline, catalog,
        segment_ids_shared, doc_count_shared, loading, app_state,
    ).await
}

/// Build graph from segments, then build catalog and run compute pipeline.
async fn load_segments_and_compute(
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

    let (graph, doc_count) = build_graph(
        effective_data_dir, segments, reader_threads, &loading, total,
    ).await;

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

    // ── Catalog: check freshness, load or rebuild ──
    info!("Building catalog...");
    let catalog_store = &app_state.catalog_store;
    {
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

        let cat = if manifest_fresh {
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
            build_and_persist_catalog(&shared_graph, segments, catalog_store).await
        };

        let cat = sync_catalog_with_external_sources(
            cat, catalog_store, &app_state.athena_connections,
        ).await;

        let mut catalog_lock = catalog.write().await;
        *catalog_lock = Some(cat);
    }
    info!("Catalog ready");

    info!("Starting compute scheduler in background...");
    run_compute(
        segments, effective_data_dir,
        shared_graph, knowledge, pipeline, &app_state,
    ).await;

    Ok(())
}

/// Read all segments in parallel with Rayon, extract graph ops, and apply them
/// sequentially to build the in-memory graph.
async fn build_graph(
    effective_data_dir: &std::path::Path,
    segments: &[String],
    reader_threads: usize,
    loading: &Arc<LoadingState>,
    total: u64,
) -> (stupid_graph::GraphStore, u64) {
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
}
