mod anomaly_rules;
mod api;
#[cfg(feature = "aws")]
mod athena_connections;
#[cfg(feature = "aws")]
mod athena_query;
#[cfg(feature = "aws")]
mod athena_query_log;
mod connections;
mod live;
#[cfg(feature = "aws")]
mod queue;
#[cfg(feature = "aws")]
mod queue_connections;
mod rule_runner;
mod state;

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::Arc;

use axum::routing::{get, post};
use axum::Router;
use tokio::sync::RwLock;
use tower_http::cors::CorsLayer;
use tracing::{error, info};

#[cfg(feature = "aws")]
use stupid_storage::{StorageEngine, S3Exporter, S3Importer};

use crate::state::{LoadingPhase, LoadingState};

fn load_config() -> stupid_core::Config {
    stupid_core::config::load_dotenv();
    stupid_core::Config::from_env()
}

/// Build the agent executor from config, loading agents from .claude/agents/.
fn build_agent_executor(config: &stupid_core::Config) -> Option<stupid_agent::AgentExecutor> {
    // Look for agents directory relative to the data dir or current directory
    let agents_dir = std::env::var("AGENTS_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            std::path::PathBuf::from("packages/stupid-claude-agent/.claude/agents")
        });

    if !agents_dir.exists() {
        info!("Agents directory not found at {} — agent system disabled", agents_dir.display());
        return None;
    }

    let agents = match stupid_agent::config::load_agents(&agents_dir) {
        Ok(agents) if agents.is_empty() => {
            info!("No agent configs found in {} — agent system disabled", agents_dir.display());
            return None;
        }
        Ok(agents) => {
            info!("Loaded {} agent configs from {}", agents.len(), agents_dir.display());
            agents
        }
        Err(e) => {
            tracing::warn!("Failed to load agents: {} — agent system disabled", e);
            return None;
        }
    };

    // Create LLM provider for agents (reuse existing LLM config)
    let provider = match stupid_llm::providers::create_provider(&config.llm, &config.ollama) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!("Failed to create LLM provider for agents: {} — agent system disabled", e);
            return None;
        }
    };

    Some(stupid_agent::AgentExecutor::new(
        agents,
        provider,
        config.llm.temperature,
        config.llm.max_tokens,
    ))
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

fn build_graph_multi(data_dir: &Path, segment_ids: &[String]) -> anyhow::Result<(stupid_graph::GraphStore, u64)> {
    info!("Reading {} segments (streaming)...", segment_ids.len());
    let start = std::time::Instant::now();

    let mut graph = stupid_graph::GraphStore::new();
    let mut total_docs: u64 = 0;
    let mut skipped: u64 = 0;

    for (i, seg_id) in segment_ids.iter().enumerate() {
        let reader = match stupid_segment::reader::SegmentReader::open(data_dir, seg_id) {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("Skipping segment '{}': {}", seg_id, e);
                skipped += 1;
                continue;
            }
        };

        let mut seg_docs: u64 = 0;
        for doc_result in reader.iter() {
            match doc_result {
                Ok(doc) => {
                    stupid_connector::entity_extract::EntityExtractor::extract(&doc, &mut graph, seg_id);
                    seg_docs += 1;
                }
                Err(e) => {
                    tracing::warn!("Bad document in '{}': {}", seg_id, e);
                }
            }
        }

        total_docs += seg_docs;
        if (i + 1) % 5 == 0 || i + 1 == segment_ids.len() {
            info!(
                "  Progress: {}/{} segments, {} docs total ({:.1}s)",
                i + 1, segment_ids.len(), total_docs, start.elapsed().as_secs_f64()
            );
        }
    }

    info!(
        "Graph built: {} docs from {} segments in {:.1}s ({} skipped)",
        total_docs, segment_ids.len() - skipped as usize, start.elapsed().as_secs_f64(), skipped
    );

    Ok((graph, total_docs))
}

#[cfg(feature = "aws")]
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

#[cfg(feature = "aws")]
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

    // LLM init is config-based and fast — keep it synchronous.
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

    // Create shared state with empty data — will be populated by background loader.
    let shared_graph: state::SharedGraph = Arc::new(RwLock::new(stupid_graph::GraphStore::new()));
    let knowledge = stupid_compute::scheduler::state::new_shared_state();
    let pipeline: state::SharedPipeline = Arc::new(std::sync::Mutex::new(stupid_compute::Pipeline::new()));
    let catalog: Arc<RwLock<Option<stupid_catalog::Catalog>>> = Arc::new(RwLock::new(None));
    let segment_ids_shared: Arc<RwLock<Vec<String>>> = Arc::new(RwLock::new(Vec::new()));
    let doc_count_shared = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let loading = Arc::new(LoadingState::new());

    let (broadcast_tx, _) = tokio::sync::broadcast::channel::<String>(64);
    let watcher_broadcast_tx = broadcast_tx.clone();

    let queue_metrics = Arc::new(std::sync::RwLock::new(std::collections::HashMap::new()));

    // Initialize connection credential store.
    let conn_store = connections::ConnectionStore::new(&config.storage.data_dir)
        .expect("Failed to initialize connection store");
    info!("Connection store initialized (data_dir: {})", config.storage.data_dir.display());

    // Initialize queue connection store (AWS feature).
    #[cfg(feature = "aws")]
    let queue_conn_store = queue_connections::QueueConnectionStore::new(&config.storage.data_dir)
        .expect("Failed to initialize queue connection store");
    #[cfg(feature = "aws")]
    info!("Queue connection store initialized (data_dir: {})", config.storage.data_dir.display());

    // Initialize Athena connection store (AWS feature).
    #[cfg(feature = "aws")]
    let athena_conn_store = athena_connections::AthenaConnectionStore::new(&config.storage.data_dir)
        .expect("Failed to initialize Athena connection store");
    #[cfg(feature = "aws")]
    info!("Athena connection store initialized");

    // Initialize session store for agent chat history.
    let session_store = stupid_agent::session::SessionStore::new(&config.storage.data_dir)
        .expect("Failed to initialize session store");
    info!("Session store initialized");

    // Initialize catalog store for persistent catalog.
    let catalog_store = stupid_catalog::CatalogStore::new(config.storage.data_dir.join("catalog"))
        .expect("Failed to initialize catalog store");
    let catalog_store = Arc::new(catalog_store);
    info!("Catalog store initialized at {}/catalog", config.storage.data_dir.display());

    // Initialize anomaly rule loader.
    let rules_dir = config.storage.data_dir.join("rules");
    let rule_loader = stupid_rules::loader::RuleLoader::new(rules_dir.clone());
    match rule_loader.load_all() {
        Ok(results) => {
            let loaded = results.iter().filter(|r| matches!(r.status, stupid_rules::loader::LoadStatus::Loaded { .. })).count();
            info!("Loaded {} anomaly rules from {}", loaded, rules_dir.display());
        }
        Err(e) => {
            tracing::warn!("Failed to load anomaly rules: {} — rules API will start empty", e);
        }
    }

    let state = Arc::new(state::AppState {
        graph: shared_graph.clone(),
        knowledge: knowledge.clone(),
        pipeline: pipeline.clone(),
        scheduler: RwLock::new(None),
        catalog: catalog.clone(),
        catalog_store: catalog_store.clone(),
        query_generator,
        segment_ids: segment_ids_shared.clone(),
        doc_count: doc_count_shared.clone(),
        loading: loading.clone(),
        broadcast: broadcast_tx,
        queue_metrics,
        queue_writer: Arc::new(std::sync::Mutex::new(None)),
        data_dir: config.storage.data_dir.clone(),
        agent_executor: build_agent_executor(config),
        connections: Arc::new(RwLock::new(conn_store)),
        #[cfg(feature = "aws")]
        queue_connections: Arc::new(RwLock::new(queue_conn_store)),
        #[cfg(feature = "aws")]
        athena_connections: Arc::new(RwLock::new(athena_conn_store)),
        session_store: Arc::new(RwLock::new(session_store)),
        rule_loader,
        trigger_history: Arc::new(std::sync::RwLock::new(std::collections::HashMap::new())),
        audit_log: stupid_rules::audit_log::AuditLog::new(),
        #[cfg(feature = "aws")]
        athena_query_log: crate::athena_query_log::AthenaQueryLog::new(&config.storage.data_dir),
    });

    let app = Router::new()
        .route("/health", get(api::health))
        .route("/loading", get(api::loading))
        .route("/stats", get(api::stats))
        .route("/graph/nodes", get(api::graph_nodes))
        .route("/graph/nodes/{id}", get(api::graph_node_by_id))
        .route("/graph/force", get(api::graph_force))
        .route("/catalog", get(api::catalog))
        .route("/compute/pagerank", get(api::compute_pagerank))
        .route("/compute/communities", get(api::compute_communities))
        .route("/compute/degrees", get(api::compute_degrees))
        .route("/compute/patterns", get(api::compute_patterns))
        .route("/compute/cooccurrence", get(api::compute_cooccurrence))
        .route("/compute/trends", get(api::compute_trends))
        .route("/compute/anomalies", get(api::compute_anomalies))
        .route("/scheduler/metrics", get(api::scheduler_metrics))
        .route("/queue/status", get(api::queue_status))
        .route("/query", post(api::query))
        .route("/agents/list", get(api::agents_list))
        .route("/agents/execute", post(api::agents_execute))
        .route("/agents/chat", post(api::agents_chat))
        .route("/teams/execute", post(api::teams_execute))
        .route("/teams/strategies", get(api::teams_strategies))
        .route("/sessions", get(api::sessions_list).post(api::sessions_create))
        .route("/sessions/{id}", get(api::sessions_get).put(api::sessions_update).delete(api::sessions_delete))
        .route("/sessions/{id}/execute-agent", post(api::sessions_execute_agent))
        .route("/sessions/{id}/execute-team", post(api::sessions_execute_team))
        .route("/sessions/{id}/execute", post(api::sessions_execute))
        .route("/connections", get(api::connections_list).post(api::connections_add))
        .route("/connections/{id}", get(api::connections_get).put(api::connections_update).delete(api::connections_delete))
        .route("/connections/{id}/credentials", get(api::connections_credentials))
        .route("/ws", get(live::ws_upgrade));

    // AWS-gated routes: queue connections, Athena connections + query
    #[cfg(feature = "aws")]
    let app = app
        .route("/queue-connections", get(api::queue_connections_list).post(api::queue_connections_add))
        .route("/queue-connections/{id}", get(api::queue_connections_get).put(api::queue_connections_update).delete(api::queue_connections_delete))
        .route("/queue-connections/{id}/credentials", get(api::queue_connections_credentials))
        .route("/athena-connections", get(api::athena_connections_list).post(api::athena_connections_add))
        .route("/athena-connections/{id}", get(api::athena_connections_get).put(api::athena_connections_update).delete(api::athena_connections_delete))
        .route("/athena-connections/{id}/credentials", get(api::athena_connections_credentials))
        .route("/athena-connections/{id}/query", post(api::athena_query_sse))
        .route("/athena-connections/{id}/query/parquet", post(api::athena_query_parquet))
        .route("/athena-connections/{id}/schema", get(api::athena_connections_schema))
        .route("/athena-connections/{id}/schema/refresh", post(api::athena_connections_schema_refresh))
        .route("/athena-connections/{id}/query-log", get(api::athena_connections_query_log));

    let app = app
        .merge(anomaly_rules::anomaly_rules_router())
        .layer(CorsLayer::permissive())
        .with_state(state.clone());

    // Bind and start serving IMMEDIATELY.
    let addr = format!("{}:{}", config.server.host, config.server.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    info!("Server listening on http://localhost:{} (data loading in background)", config.server.port);

    // Spawn background data loading task.
    let data_dir = config.storage.data_dir.clone();
    let watcher_data_dir = data_dir.clone();
    #[cfg(feature = "aws")]
    let storage = StorageEngine::from_config(config)?;
    let single_segment = segment_id.map(|s| s.to_string());

    // Clones for the segment watcher (needs its own references).
    let watcher_graph = shared_graph.clone();
    let watcher_knowledge = knowledge.clone();
    let watcher_pipeline = state.pipeline.clone();
    let watcher_segments = segment_ids_shared.clone();
    let watcher_doc_count = doc_count_shared.clone();

    let state_for_loader = state.clone();
    tokio::spawn(async move {
        #[cfg(feature = "aws")]
        let result = background_load(
            storage,
            data_dir,
            single_segment,
            shared_graph,
            knowledge,
            pipeline,
            catalog,
            segment_ids_shared,
            doc_count_shared,
            loading,
            state_for_loader,
        )
        .await;
        #[cfg(not(feature = "aws"))]
        let result = background_load_local(
            data_dir,
            single_segment,
            shared_graph,
            knowledge,
            pipeline,
            catalog,
            segment_ids_shared,
            doc_count_shared,
            loading,
            state_for_loader,
        )
        .await;
        if let Err(e) = result {
            error!("Background data loading failed: {}", e);
        }
    });

    // Spawn background rule evaluation loop (waits for loading internally).
    tokio::spawn(rule_runner::run_rule_loop(state.clone()));

    // Spawn segment file watcher for live updates.
    // Note: catalog was moved into the background_load spawn above, so we use
    // the clone stored in state.
    let watcher_catalog = state.catalog.clone();
    let watcher_catalog_store = catalog_store.clone();
    tokio::spawn(async move {
        live::start_segment_watcher(
            watcher_data_dir,
            watcher_graph,
            watcher_knowledge,
            watcher_pipeline,
            watcher_segments,
            watcher_doc_count,
            watcher_broadcast_tx,
            watcher_catalog,
            watcher_catalog_store,
        )
        .await;
    });

    // Spawn queue consumers from the encrypted connection store (AWS feature).
    #[cfg(feature = "aws")]
    {
        let queue_state = state.clone();
        tokio::spawn(async move {
            queue::spawn_queue_consumers(queue_state).await;
        });
    }

    axum::serve(listener, app).await?;
    Ok(())
}

// ── Lightweight graph operations for parallel extraction ──────────────
//
// Instead of sending full Documents through a channel, rayon workers
// extract these tiny ops (~50-100 bytes each). The consumer replays
// them into the single-threaded GraphStore.

pub(crate) enum GraphOp {
    /// Upsert a node and add edges from it.
    Node {
        entity_type: stupid_core::EntityType,
        key: String,
        edges: Vec<(stupid_core::EntityType, String, stupid_core::EdgeType)>,
    },
}

/// Extract graph ops from a document (runs in rayon worker, no GraphStore needed).
pub(crate) fn extract_graph_ops(doc: &stupid_core::Document, _seg_id: &str, ops: &mut Vec<GraphOp>) {
    use stupid_core::{EdgeType, EntityType};

    let member_code = match doc.fields.get("memberCode").and_then(|v| v.as_str()) {
        Some(s) if !s.trim().is_empty() && s.trim() != "None" && s.trim() != "null" => s.trim().to_string(),
        _ => return,
    };

    let member_key = format!("member:{}", member_code);
    let mut edges = Vec::new();

    let get = |name: &str| -> Option<String> {
        doc.fields.get(name).and_then(|v| v.as_str()).and_then(|s| {
            let t = s.trim();
            if t.is_empty() || t == "None" || t == "null" || t == "undefined" { None }
            else { Some(t.to_string()) }
        })
    };

    match doc.event_type.as_str() {
        "Login" => {
            if let Some(fp) = get("fingerprint") {
                edges.push((EntityType::Device, format!("device:{}", fp), EdgeType::LoggedInFrom));
            }
            if let Some(p) = get("platform") {
                edges.push((EntityType::Platform, format!("platform:{}", p), EdgeType::PlaysOnPlatform));
            }
            if let Some(c) = get("currency") {
                edges.push((EntityType::Currency, format!("currency:{}", c), EdgeType::UsesCurrency));
            }
            if let Some(g) = get("rGroup") {
                edges.push((EntityType::VipGroup, format!("vipgroup:{}", g), EdgeType::BelongsToGroup));
            }
            let aff = get("affiliateId").or_else(|| get("affiliateid")).or_else(|| get("affiliateID"));
            if let Some(a) = aff {
                edges.push((EntityType::Affiliate, format!("affiliate:{}", a), EdgeType::ReferredBy));
            }
        }
        "GameOpened" | "GridClick" => {
            if let Some(g) = get("game") {
                edges.push((EntityType::Game, format!("game:{}", g), EdgeType::OpenedGame));
                if let Some(p) = get("gameTrackingProvider") {
                    // Provider linked to game, not member — handled separately below.
                    ops.push(GraphOp::Node {
                        entity_type: EntityType::Game,
                        key: format!("game:{}", g),
                        edges: vec![(EntityType::Provider, format!("provider:{}", p), EdgeType::ProvidedBy)],
                    });
                }
            }
            if let Some(p) = get("platform") {
                edges.push((EntityType::Platform, format!("platform:{}", p), EdgeType::PlaysOnPlatform));
            }
            if let Some(c) = get("currency") {
                edges.push((EntityType::Currency, format!("currency:{}", c), EdgeType::UsesCurrency));
            }
        }
        "PopupModule" | "PopUpModule" => {
            let popup_key = get("trackingId").or_else(|| get("popupType"));
            if let Some(pk) = popup_key {
                edges.push((EntityType::Popup, format!("popup:{}", pk), EdgeType::SawPopup));
            }
            if let Some(p) = get("platform") {
                edges.push((EntityType::Platform, format!("platform:{}", p), EdgeType::PlaysOnPlatform));
            }
        }
        "API Error" => {
            let error_key = match (get("url"), get("statusCode")) {
                (Some(url), Some(code)) => Some(format!("error:{}:{}", code, url)),
                (Some(url), None) => Some(format!("error:{}", url)),
                _ => get("error").map(|e| format!("error:{}", e)),
            };
            if let Some(ek) = error_key {
                edges.push((EntityType::Error, ek, EdgeType::HitError));
            }
            if let Some(p) = get("platform") {
                edges.push((EntityType::Platform, format!("platform:{}", p), EdgeType::PlaysOnPlatform));
            }
        }
        _ => return,
    }

    if !edges.is_empty() {
        ops.push(GraphOp::Node {
            entity_type: EntityType::Member,
            key: member_key,
            edges,
        });
    }
}

/// Replay a graph op into the GraphStore (runs on consumer thread).
pub(crate) fn apply_graph_op(op: &GraphOp, graph: &mut stupid_graph::GraphStore, seg_id: &str) {
    match op {
        GraphOp::Node { entity_type, key, edges } => {
            let source_id = graph.upsert_node(*entity_type, key, &seg_id.to_string());
            for (target_type, target_key, edge_type) in edges {
                let target_id = graph.upsert_node(*target_type, target_key, &seg_id.to_string());
                graph.add_edge(source_id, target_id, *edge_type, &seg_id.to_string());
            }
        }
    }
}

/// Background task (no AWS): discover local segments, build graph, catalog, and compute.
#[cfg(not(feature = "aws"))]
async fn background_load_local(
    data_dir: PathBuf,
    single_segment: Option<String>,
    shared_graph: state::SharedGraph,
    knowledge: stupid_compute::SharedKnowledgeState,
    pipeline: state::SharedPipeline,
    catalog: Arc<RwLock<Option<stupid_catalog::Catalog>>>,
    segment_ids_shared: Arc<RwLock<Vec<String>>>,
    doc_count_shared: Arc<std::sync::atomic::AtomicU64>,
    loading: Arc<LoadingState>,
    app_state: Arc<state::AppState>,
) -> anyhow::Result<()> {
    loading.set_phase(LoadingPhase::Discovering).await;
    info!("Discovering local segments (AWS disabled)...");

    let segments = if let Some(seg_id) = single_segment {
        info!("Single segment mode: '{}'", seg_id);
        vec![seg_id]
    } else {
        let local_segments = discover_segments(&data_dir);
        if local_segments.is_empty() {
            let msg = "No local segments found. Run 'import' or 'import-dir' first (S3 disabled without --features aws).";
            loading.set_phase(LoadingPhase::Failed(msg.to_string())).await;
            anyhow::bail!("{}", msg);
        }
        info!("Found {} local segments", local_segments.len());
        local_segments
    };

    // Delegate to the shared graph+compute loading logic.
    load_segments_and_compute(
        &data_dir, &data_dir, &segments,
        shared_graph, knowledge, pipeline, catalog,
        segment_ids_shared, doc_count_shared, loading, app_state,
    ).await
}

/// Background task (with AWS): discover segments (local or S3), build graph, catalog, and compute.
#[cfg(feature = "aws")]
async fn background_load(
    storage: StorageEngine,
    data_dir: PathBuf,
    single_segment: Option<String>,
    shared_graph: state::SharedGraph,
    knowledge: stupid_compute::SharedKnowledgeState,
    pipeline: state::SharedPipeline,
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
    shared_graph: &state::SharedGraph,
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

/// Shared graph building + catalog + compute logic used by both AWS and local loaders.
async fn load_segments_and_compute(
    _data_dir: &std::path::Path,
    effective_data_dir: &std::path::Path,
    segments: &[String],
    shared_graph: state::SharedGraph,
    knowledge: stupid_compute::SharedKnowledgeState,
    pipeline: state::SharedPipeline,
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

        struct SegmentResult {
            seg_id: String,
            ops: Vec<GraphOp>,
            doc_count: u64,
            elapsed: std::time::Duration,
        }

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

        // Build and persist external sources from Athena connections (AWS feature).
        #[cfg(feature = "aws")]
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
        #[cfg(feature = "aws")]
        Some("import-s3") => {
            let prefix = args.get(2).expect("Usage: server import-s3 <s3-prefix>");
            import_s3(&config, prefix).await?;
        }
        #[cfg(feature = "aws")]
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
