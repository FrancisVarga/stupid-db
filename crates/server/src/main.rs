mod api;
mod state;

use std::path::Path;
use std::sync::Arc;

use axum::routing::{get, post};
use axum::Router;
use tokio::sync::RwLock;
use tower_http::cors::CorsLayer;
use tracing::info;

use stupid_storage::{StorageEngine, S3Exporter, S3Importer};

fn load_config() -> stupid_core::Config {
    stupid_core::config::load_dotenv();
    stupid_core::Config::from_env()
}

fn import(config: &stupid_core::Config, parquet_path: &Path, segment_id: &str) -> anyhow::Result<()> {
    info!("Importing {} as segment '{}'", parquet_path.display(), segment_id);

    let event_type = parquet_path
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("Unknown");

    let documents = stupid_ingest::parquet_import::ParquetImporter::import(parquet_path, event_type)?;
    info!("Read {} documents from parquet", documents.len());

    let data_dir = &config.storage.data_dir;
    let mut writer = stupid_segment::writer::SegmentWriter::new(data_dir, segment_id)?;

    for doc in &documents {
        writer.append(doc)?;
    }
    writer.finalize()?;

    // Build graph from segment
    let reader = stupid_segment::reader::SegmentReader::open(data_dir, segment_id)?;
    let mut graph = stupid_graph::GraphStore::new();
    let seg_id = segment_id.to_string();

    let mut doc_count = 0u64;
    for doc_result in reader.iter() {
        let doc = doc_result?;
        stupid_connector::entity_extract::EntityExtractor::extract(&doc, &mut graph, &seg_id);
        doc_count += 1;
    }

    info!("Processed {} documents for entity extraction", doc_count);

    let stats = graph.stats();
    info!("Graph stats:");
    info!("  Nodes: {}", stats.node_count);
    for (typ, count) in &stats.nodes_by_type {
        info!("    {}: {}", typ, count);
    }
    info!("  Edges: {}", stats.edge_count);
    for (typ, count) in &stats.edges_by_type {
        info!("    {}: {}", typ, count);
    }

    Ok(())
}

/// Parse ISO week from a date-like filename stem (e.g., "2025-06-14" -> "2025-W24").
fn date_to_iso_week(date_stem: &str) -> String {
    use chrono::{Datelike, NaiveDate};

    if let Ok(date) = NaiveDate::parse_from_str(date_stem, "%Y-%m-%d") {
        let iso = date.iso_week();
        format!("{}-W{:02}", iso.year(), iso.week())
    } else {
        // Can't parse date — put in "misc" bucket
        "misc".to_string()
    }
}

/// A group of parquet files that will be merged into one weekly segment.
struct ImportGroup {
    segment_id: String,
    event_type: String,
    files: Vec<std::path::PathBuf>,
}

fn import_dir(config: &stupid_core::Config, dir_path: &Path) -> anyhow::Result<()> {
    use rayon::prelude::*;
    use std::collections::BTreeMap;
    use std::sync::atomic::{AtomicU64, Ordering};

    info!("Scanning {} for parquet files...", dir_path.display());

    let mut parquet_files: Vec<std::path::PathBuf> = Vec::new();
    for entry in walkdir::WalkDir::new(dir_path)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if path.is_file() && path.extension().map(|e| e == "parquet").unwrap_or(false) {
            parquet_files.push(path.to_path_buf());
        }
    }

    parquet_files.sort();
    info!("Found {} parquet files", parquet_files.len());

    if parquet_files.is_empty() {
        anyhow::bail!("No .parquet files found in {}", dir_path.display());
    }

    // Group files by (event_type, iso_week)
    let mut groups: BTreeMap<String, ImportGroup> = BTreeMap::new();

    for parquet_path in &parquet_files {
        let event_type = parquet_path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("Unknown")
            .to_string();

        let date_stem = parquet_path
            .file_stem()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");

        let week = date_to_iso_week(date_stem);
        let segment_id = format!("{}/{}", event_type, week);

        groups
            .entry(segment_id.clone())
            .or_insert_with(|| ImportGroup {
                segment_id,
                event_type: event_type.clone(),
                files: Vec::new(),
            })
            .files
            .push(parquet_path.clone());
    }

    let group_list: Vec<ImportGroup> = groups.into_values().collect();
    let total_groups = group_list.len();
    info!(
        "Grouped into {} weekly segments — importing with {} threads",
        total_groups,
        rayon::current_num_threads()
    );

    let total_docs = AtomicU64::new(0);
    let completed = AtomicU64::new(0);
    let failed = AtomicU64::new(0);
    let data_dir = &config.storage.data_dir;
    let start = std::time::Instant::now();

    // Parallel import: one group = one segment, each group processes independently
    group_list.par_iter().for_each(|group| {
        let mut writer = match stupid_segment::writer::SegmentWriter::new(data_dir, &group.segment_id) {
            Ok(w) => w,
            Err(e) => {
                tracing::warn!("Failed to create segment '{}': {}", group.segment_id, e);
                failed.fetch_add(1, Ordering::Relaxed);
                return;
            }
        };

        let mut group_docs = 0u64;

        for parquet_path in &group.files {
            let documents = match stupid_ingest::parquet_import::ParquetImporter::import(
                parquet_path,
                &group.event_type,
            ) {
                Ok(docs) => docs,
                Err(e) => {
                    tracing::warn!("Failed to read {}: {}", parquet_path.display(), e);
                    continue;
                }
            };

            for doc in &documents {
                if let Err(e) = writer.append(doc) {
                    tracing::warn!("Write error in '{}': {}", group.segment_id, e);
                    break;
                }
            }

            group_docs += documents.len() as u64;
        }

        if let Err(e) = writer.finalize() {
            tracing::warn!("Failed to finalize '{}': {}", group.segment_id, e);
            failed.fetch_add(1, Ordering::Relaxed);
            return;
        }

        total_docs.fetch_add(group_docs, Ordering::Relaxed);
        let done = completed.fetch_add(1, Ordering::Relaxed) + 1;

        if done % 5 == 0 || done as usize == total_groups {
            info!(
                "  Progress: {}/{} segments ({} docs, {:.1}s)",
                done,
                total_groups,
                total_docs.load(Ordering::Relaxed),
                start.elapsed().as_secs_f64()
            );
        }
    });

    let elapsed = start.elapsed();
    let final_docs = total_docs.load(Ordering::Relaxed);
    let final_done = completed.load(Ordering::Relaxed);
    let final_failed = failed.load(Ordering::Relaxed);
    info!(
        "Import complete: {} segments ({} parquet files), {} docs total in {:.1}s ({} failed)",
        final_done,
        parquet_files.len(),
        final_docs,
        elapsed.as_secs_f64(),
        final_failed
    );
    Ok(())
}

fn discover_segments(data_dir: &Path) -> Vec<String> {
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

fn build_graph(data_dir: &Path, segment_id: &str) -> anyhow::Result<(stupid_graph::GraphStore, u64)> {
    let reader = stupid_segment::reader::SegmentReader::open(data_dir, segment_id)?;
    let mut graph = stupid_graph::GraphStore::new();
    let seg_id = segment_id.to_string();

    let mut doc_count = 0u64;
    for doc_result in reader.iter() {
        let doc = doc_result?;
        stupid_connector::entity_extract::EntityExtractor::extract(&doc, &mut graph, &seg_id);
        doc_count += 1;
    }

    Ok((graph, doc_count))
}

/// Per-segment extraction result for parallel loading.
struct SegmentData {
    segment_id: String,
    docs: Vec<stupid_core::Document>,
}

fn build_graph_multi(data_dir: &Path, segment_ids: &[String]) -> anyhow::Result<(stupid_graph::GraphStore, u64)> {
    use rayon::prelude::*;

    info!("Reading {} segments in parallel...", segment_ids.len());
    let start = std::time::Instant::now();

    // Phase 1: parallel I/O — read + decompress all segments
    let segment_data: Vec<SegmentData> = segment_ids
        .par_iter()
        .filter_map(|seg_id| {
            let reader = match stupid_segment::reader::SegmentReader::open(data_dir, seg_id) {
                Ok(r) => r,
                Err(e) => {
                    tracing::warn!("Skipping segment '{}': {}", seg_id, e);
                    return None;
                }
            };

            let docs: Vec<stupid_core::Document> = reader
                .iter()
                .filter_map(|r| r.ok())
                .collect();

            Some(SegmentData {
                segment_id: seg_id.clone(),
                docs,
            })
        })
        .collect();

    let read_elapsed = start.elapsed();
    let total_docs: u64 = segment_data.iter().map(|s| s.docs.len() as u64).sum();
    info!(
        "  Read {} docs from {} segments in {:.1}s",
        total_docs,
        segment_data.len(),
        read_elapsed.as_secs_f64()
    );

    // Phase 2: sequential graph build (graph needs &mut self)
    info!("Building graph from {} documents...", total_docs);
    let graph_start = std::time::Instant::now();
    let mut graph = stupid_graph::GraphStore::new();

    for (i, seg) in segment_data.iter().enumerate() {
        for doc in &seg.docs {
            stupid_connector::entity_extract::EntityExtractor::extract(doc, &mut graph, &seg.segment_id);
        }
        if (i + 1) % 20 == 0 || i + 1 == segment_data.len() {
            info!(
                "  Graph: {}/{} segments processed",
                i + 1,
                segment_data.len()
            );
        }
    }

    let graph_elapsed = graph_start.elapsed();
    info!(
        "  Graph built in {:.1}s (total: {:.1}s)",
        graph_elapsed.as_secs_f64(),
        start.elapsed().as_secs_f64()
    );

    Ok((graph, total_docs))
}

async fn import_s3(config: &stupid_core::Config, s3_prefix: &str) -> anyhow::Result<()> {
    let storage = StorageEngine::from_config(config)?;
    if !storage.backend.is_remote() {
        anyhow::bail!("import-s3 requires S3 configuration (S3_BUCKET, AWS_REGION, etc.)");
    }
    info!("Importing parquet files from S3 prefix: {}", s3_prefix);
    let (docs, segments) = S3Importer::import_all(
        &storage.backend,
        s3_prefix,
        &storage.data_dir,
    )
    .await?;
    info!(
        "S3 import complete: {} documents in {} segments",
        docs, segments
    );
    Ok(())
}

async fn export(config: &stupid_core::Config, export_segments: bool, export_graph: bool) -> anyhow::Result<()> {
    let storage = StorageEngine::from_config(config)?;
    if !storage.backend.is_remote() {
        anyhow::bail!("export requires S3 configuration (S3_BUCKET, AWS_REGION, etc.)");
    }
    let data_dir = &config.storage.data_dir;

    if export_segments {
        let segment_ids = discover_segments(data_dir);
        if segment_ids.is_empty() {
            info!("No local segments to export");
        } else {
            info!("Exporting {} segments to S3...", segment_ids.len());
            let (uploaded, skipped) =
                S3Exporter::export_segments(&storage.backend, data_dir, &segment_ids).await?;
            info!("Segment export: {} uploaded, {} skipped", uploaded, skipped);
        }
    }

    if export_graph {
        let segment_ids = discover_segments(data_dir);
        if segment_ids.is_empty() {
            info!("No segments — skipping graph export");
        } else {
            info!("Building graph for export...");
            let (graph, _) = build_graph_multi(data_dir, &segment_ids)?;
            let stats = graph.stats();
            S3Exporter::export_graph(&storage.backend, &stats).await?;
            info!("Graph stats exported to S3");
        }
    }

    Ok(())
}

async fn serve(config: &stupid_core::Config, segment_id: Option<&str>) -> anyhow::Result<()> {
    config.log_summary();
    let storage = StorageEngine::from_config(config)?;
    let data_dir = &config.storage.data_dir;

    let (graph, doc_count, segment_ids) = if let Some(seg_id) = segment_id {
        // Single segment mode (backwards compatible)
        info!("Loading segment '{}' and building graph...", seg_id);
        let (graph, doc_count) = build_graph(data_dir, seg_id)?;
        (graph, doc_count, vec![seg_id.to_string()])
    } else {
        // Try local segments first, fall back to S3 discovery
        let local_segments = discover_segments(data_dir);
        let (segments, from_local) = if !local_segments.is_empty() {
            info!("Found {} local segments", local_segments.len());
            (local_segments, true)
        } else {
            // No local segments — try S3 if configured
            let remote = storage.discover_segments().await?;
            if remote.is_empty() {
                anyhow::bail!(
                    "No segments found (local or remote). Run 'import', 'import-dir', or 'import-s3' first."
                );
            }
            (remote, false)
        };
        info!("Discovered {} segments, building graph...", segments.len());

        // Only resolve through S3 cache when segments actually came from S3
        let effective_data_dir = if !from_local && storage.backend.is_remote() {
            // Pre-fetch all segments to cache, use cache root as data_dir
            let mut cache_dir = None;
            for seg_id in &segments {
                let dir = storage.segment_data_dir(seg_id).await?;
                cache_dir = Some(dir);
            }
            cache_dir.unwrap_or_else(|| data_dir.clone())
        } else {
            data_dir.clone()
        };

        let (graph, doc_count) = build_graph_multi(&effective_data_dir, &segments)?;
        (graph, doc_count, segments)
    };

    let stats = graph.stats();
    info!(
        "Graph ready: {} nodes, {} edges from {} documents across {} segments",
        stats.node_count, stats.edge_count, doc_count, segment_ids.len()
    );

    info!("Building catalog...");
    let catalog = stupid_catalog::Catalog::from_graph(&graph);

    let query_generator = match stupid_llm::QueryGenerator::from_config(&config.llm, &config.ollama) {
        Ok(qg) => {
            info!("LLM query generator ready (provider: {})", config.llm.provider);
            Some(qg)
        }
        Err(e) => {
            tracing::warn!("LLM query generator not available: {} — POST /query will be disabled", e);
            None
        }
    };

    let compute: Arc<RwLock<Option<stupid_compute::ComputeEngine>>> =
        Arc::new(RwLock::new(None));

    let shared_graph = Arc::new(RwLock::new(graph));

    let state = Arc::new(state::AppState {
        graph: shared_graph.clone(),
        compute: compute.clone(),
        catalog,
        query_generator,
        segment_ids,
        doc_count,
    });

    let app = Router::new()
        .route("/health", get(api::health))
        .route("/stats", get(api::stats))
        .route("/graph/nodes", get(api::graph_nodes))
        .route("/graph/nodes/{id}", get(api::graph_node_by_id))
        .route("/graph/force", get(api::graph_force))
        .route("/catalog", get(api::catalog))
        .route("/compute/pagerank", get(api::compute_pagerank))
        .route("/compute/communities", get(api::compute_communities))
        .route("/compute/degrees", get(api::compute_degrees))
        .route("/query", post(api::query))
        .layer(CorsLayer::permissive())
        .with_state(state);

    // Spawn background compute task — server starts immediately
    tokio::spawn(async move {
        info!("Running compute algorithms in background...");
        let graph_read = shared_graph.read().await;
        let engine = stupid_compute::ComputeEngine::run_all(&graph_read);
        drop(graph_read);
        let mut compute_lock = compute.write().await;
        *compute_lock = Some(engine);
        info!("Compute algorithms complete — PageRank, communities, degrees now available");
    });

    let addr = format!("{}:{}", config.server.host, config.server.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    info!("Server listening on http://localhost:{}", config.server.port);
    axum::serve(listener, app).await?;

    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_target(false)
        .with_level(true)
        .init();

    let config = load_config();
    let args: Vec<String> = std::env::args().collect();

    match args.get(1).map(|s| s.as_str()) {
        Some("import") => {
            let path = args.get(2).expect("Usage: server import <parquet_path> <segment_id>");
            let segment_id = args.get(3).expect("Usage: server import <parquet_path> <segment_id>");
            import(&config, Path::new(path), segment_id)?;
        }
        Some("import-dir") => {
            let path = args.get(2).expect("Usage: server import-dir <directory>");
            import_dir(&config, Path::new(path))?;
        }
        Some("import-s3") => {
            let prefix = args.get(2).expect("Usage: server import-s3 <s3-prefix>");
            import_s3(&config, prefix).await?;
        }
        Some("export") => {
            let flag = args.get(2).map(|s| s.as_str()).unwrap_or("--all");
            let (do_segments, do_graph) = match flag {
                "--segments" => (true, false),
                "--graph" => (false, true),
                "--all" | _ => (true, true),
            };
            export(&config, do_segments, do_graph).await?;
        }
        Some("serve") => {
            let segment_id = args.get(2).map(|s| s.as_str());
            serve(&config, segment_id).await?;
        }
        _ => {
            println!("stupid-db v0.1.0");
            println!("Usage: server.exe <command>");
            println!("  import <parquet_path> <segment_id>  Import single parquet file");
            println!("  import-dir <directory>               Import all parquet files recursively");
            println!("  import-s3 <s3-prefix>                Import parquet files from S3");
            println!("  export [--segments|--graph|--all]     Export to S3 (default: --all)");
            println!("  serve [segment_id]                   Start HTTP server (all segments if omitted)");
        }
    }

    Ok(())
}
