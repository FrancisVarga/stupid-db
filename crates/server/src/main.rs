mod anomaly_rules;
mod api;
mod catalog_api;
mod db;
mod eisenbahn_client;
mod vector_store;
mod rules;
mod athena_connections;
mod athena_query;
mod athena_query_log;
mod background;
mod connections;
mod credential_store;
mod export;
mod graph_ops;
mod import;
mod live;
mod queue;
mod queue_connections;
mod rule_runner;
mod state;

use std::path::Path;
use std::sync::Arc;

use axum::extract::DefaultBodyLimit;
use axum::routing::{get, post};
use axum::Router;
use tokio::sync::RwLock;
use tower_http::cors::CorsLayer;
use tracing::{error, info};
use utoipa::OpenApi;
use utoipa_scalar::{Scalar, Servable};

use stupid_storage::StorageEngine;
use stupid_tool_runtime::permission::{PermissionLevel, PermissionPolicy, PolicyChecker};
use stupid_tool_runtime::{
    AgenticLoop, LlmProviderBridge, PermissionChecker, ToolRegistry,
    BashExecuteTool, FileReadTool, FileWriteTool,
    GraphQueryTool, RuleListTool, RuleEvaluateTool,
};

use crate::state::LoadingState;

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
            std::path::PathBuf::from("agents/stupid-db-claude-code/agents")
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

/// Build the agentic loop from config, using `LlmProviderBridge` to wrap the
/// existing LLM provider into a `ToolAwareLlmProvider` with all 6 tools registered.
fn build_agentic_loop(config: &stupid_core::Config) -> Option<AgenticLoop> {
    // Create LLM provider and wrap it through the bridge
    let llm_provider = match stupid_llm::providers::create_provider(&config.llm, &config.ollama) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!("Failed to create LLM provider for agentic loop: {} — agentic loop disabled", e);
            return None;
        }
    };

    let adapter = stupid_llm::LlmProviderAdapter(llm_provider);
    let provider = Arc::new(LlmProviderBridge::new(
        Box::new(adapter),
        config.llm.provider.clone(),
    ));

    // Register all 6 built-in tools
    let mut registry = ToolRegistry::new();
    registry.register(BashExecuteTool).expect("register BashExecuteTool");
    registry.register(FileReadTool).expect("register FileReadTool");
    registry.register(FileWriteTool).expect("register FileWriteTool");
    registry.register(GraphQueryTool).expect("register GraphQueryTool");
    registry.register(RuleListTool).expect("register RuleListTool");
    registry.register(RuleEvaluateTool).expect("register RuleEvaluateTool");

    // Server-side: auto-approve all tool executions (no interactive confirmation)
    let mut policy = PermissionPolicy::new();
    policy.default = PermissionLevel::AutoApprove;
    let permission_checker: Arc<dyn PermissionChecker> = Arc::new(PolicyChecker::new(policy));

    let agentic_loop = AgenticLoop::new(provider, Arc::new(registry), permission_checker)
        .with_temperature(config.llm.temperature)
        .with_max_tokens(config.llm.max_tokens);

    info!("Agentic loop ready (provider: {}, 6 tools registered)", config.llm.provider);
    Some(agentic_loop)
}

/// Build an Embedder from config. Returns None if no embedding provider configured.
fn build_embedder(config: &stupid_core::Config) -> Option<Arc<dyn stupid_ingest::embedding::Embedder>> {
    use stupid_ingest::embedding::{OllamaEmbedder, OpenAiEmbedder};

    match config.embedding.provider.as_str() {
        "ollama" => {
            let embedder = OllamaEmbedder::new(
                config.ollama.url.clone(),
                config.ollama.embedding_model.clone(),
                config.embedding.dimensions as usize,
            );
            info!("Embedding provider ready: ollama (model: {}, dims: {})",
                  config.ollama.embedding_model, config.embedding.dimensions);
            Some(Arc::new(embedder))
        }
        "openai" => {
            let Some(api_key) = config.llm.openai_api_key.clone() else {
                tracing::warn!("EMBEDDING_PROVIDER=openai but OPENAI_API_KEY is empty — embedding features disabled");
                return None;
            };
            let embedder = OpenAiEmbedder::new(
                api_key,
                "text-embedding-3-small".to_string(),
                config.llm.openai_base_url.clone(),
                config.embedding.dimensions as usize,
            );
            info!("Embedding provider ready: openai (dims: {})", config.embedding.dimensions);
            Some(Arc::new(embedder))
        }
        other => {
            tracing::warn!("Unknown embedding provider '{}' — embedding features disabled", other);
            None
        }
    }
}

async fn serve(config: &stupid_core::Config, segment_id: Option<&str>, eisenbahn: bool) -> anyhow::Result<()> {
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

    // Initialize queue connection store.
    let queue_conn_store = queue_connections::QueueConnectionStore::new(&config.storage.data_dir)
        .expect("Failed to initialize queue connection store");
    info!("Queue connection store initialized (data_dir: {})", config.storage.data_dir.display());

    // Initialize Athena connection store.
    let athena_conn_store = athena_connections::AthenaConnectionStore::new(&config.storage.data_dir)
        .expect("Failed to initialize Athena connection store");
    info!("Athena connection store initialized");

    // Initialize session store for agent chat history.
    let session_store = stupid_agent::session::SessionStore::new(&config.storage.data_dir)
        .expect("Failed to initialize session store");
    info!("Session store initialized");

    // Initialize agent group store for agent-to-group mappings.
    let group_store = stupid_agent::group_store::AgentGroupStore::new(&config.storage.data_dir)
        .expect("Failed to initialize agent group store");

    // Initialize telemetry store for per-agent execution metrics.
    let telemetry_store = stupid_agent::telemetry_store::TelemetryStore::new(&config.storage.data_dir)
        .expect("Failed to initialize telemetry store");

    // Initialize mutable agent store (YAML-backed CRUD with hot-reload).
    let agent_store_dir = config.storage.data_dir.join("agents");
    let agent_store = match stupid_agent::AgentStore::new(&agent_store_dir) {
        Ok(store) => {
            info!("Agent store initialized at {}", agent_store_dir.display());
            Some(Arc::new(store))
        }
        Err(e) => {
            tracing::warn!("Failed to create agent store: {} — CRUD endpoints disabled", e);
            None
        }
    };

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

    // Initialize PostgreSQL connection pool and run migrations.
    let pg_pool = db::init_pg_pool(&config.postgres).await;

    // Optionally connect the eisenbahn messaging client (ZMQ broker integration).
    let eb_client = if eisenbahn {
        info!("--eisenbahn flag active — connecting to broker");
        let eb_config = eisenbahn_client::EisenbahnClientConfig::default();
        match eisenbahn_client::EisenbahnClient::connect(&eb_config, broadcast_tx.clone()).await {
            Ok(client) => Some(client),
            Err(e) => {
                error!("failed to connect to eisenbahn broker: {} — continuing without eisenbahn", e);
                None
            }
        }
    } else {
        None
    };

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
        agentic_loop: build_agentic_loop(config),
        connections: Arc::new(RwLock::new(conn_store)),
        queue_connections: Arc::new(RwLock::new(queue_conn_store)),
        athena_connections: Arc::new(RwLock::new(athena_conn_store)),
        embedder: build_embedder(config),
        session_store: Arc::new(RwLock::new(session_store)),
        group_store: Arc::new(RwLock::new(group_store)),
        eisenbahn: eb_client.clone(),
        rule_loader,
        trigger_history: Arc::new(std::sync::RwLock::new(std::collections::HashMap::new())),
        audit_log: stupid_rules::audit_log::AuditLog::new(),
        athena_query_log: crate::athena_query_log::AthenaQueryLog::new(&config.storage.data_dir),
        pg_pool,
        telemetry_store: Arc::new(RwLock::new(telemetry_store)),
        agent_store,
    });

    let app = Router::new()
        .route("/health", get(api::health))
        .route("/loading", get(api::loading))
        .route("/stats", get(api::stats))
        .route("/graph/nodes", get(api::graph_nodes))
        .route("/graph/nodes/{id}", get(api::graph_node_by_id))
        .route("/graph/force", get(api::graph_force))
        // /catalog routes handled by catalog_api::catalog_router()
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
        // Agent CRUD: /reload MUST precede /{name} to avoid "reload" being captured
        .route("/api/agents/reload", post(api::agents_reload))
        .route("/api/agents/{name}", get(api::agents_get).put(api::agents_update).delete(api::agents_delete))
        .route("/api/agents", post(api::agents_create))
        .route("/teams/execute", post(api::teams_execute))
        .route("/teams/strategies", get(api::teams_strategies))
        .route("/sessions", get(api::sessions_list).post(api::sessions_create))
        .route("/sessions/{id}", get(api::sessions_get).put(api::sessions_update).delete(api::sessions_delete))
        .route("/sessions/{id}/execute-agent", post(api::sessions_execute_agent))
        .route("/sessions/{id}/execute-team", post(api::sessions_execute_team))
        .route("/sessions/{id}/execute", post(api::sessions_execute))
        .route("/sessions/{id}/stream", post(api::sessions_stream))
        .route("/connections", get(api::connections_list).post(api::connections_add))
        .route("/connections/{id}", get(api::connections_get).put(api::connections_update).delete(api::connections_delete))
        .route("/connections/{id}/credentials", get(api::connections_credentials))
        // Telemetry: overview MUST precede {agent_name} to avoid capture
        .route("/api/telemetry/overview", get(api::telemetry_overview))
        .route("/api/telemetry/{agent_name}", get(api::telemetry_events))
        .route("/api/telemetry/{agent_name}/stats", get(api::telemetry_stats))
        // Agent Groups
        .route("/api/agent-groups", get(api::agent_groups_list).post(api::agent_groups_create))
        .route("/api/agent-groups/{name}", axum::routing::put(api::agent_groups_update).delete(api::agent_groups_delete))
        .route("/api/agent-groups/{name}/agents", post(api::agent_groups_add_agent))
        .route("/api/agent-groups/{group_name}/{agent_name}", axum::routing::delete(api::agent_groups_remove_agent))
        .route("/ws", get(live::ws_upgrade));

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
        .route("/athena-connections/{id}/query-log", get(api::athena_connections_query_log))
        // Stille Post: pipeline CRUD
        .route("/sp/pipelines", get(api::sp_pipelines_list).post(api::sp_pipelines_create))
        .route("/sp/pipelines/{id}", get(api::sp_pipelines_get).put(api::sp_pipelines_update).delete(api::sp_pipelines_delete))
        // Stille Post: delivery CRUD
        .route("/sp/deliveries", get(api::sp_deliveries_list).post(api::sp_deliveries_create))
        .route("/sp/deliveries/{id}", axum::routing::put(api::sp_deliveries_update).delete(api::sp_deliveries_delete))
        .route("/sp/deliveries/{id}/test", post(api::sp_deliveries_test))
        // Stille Post: data source CRUD
        .route("/sp/data-sources", get(api::sp_data_sources_list).post(api::sp_data_sources_create))
        .route("/sp/data-sources/{id}", get(api::sp_data_sources_get).put(api::sp_data_sources_update).delete(api::sp_data_sources_delete))
        .route("/sp/data-sources/{id}/test", post(api::sp_data_sources_test))
        .route("/sp/data-sources/upload", post(api::sp_data_sources_upload)
            .layer(DefaultBodyLimit::max(100 * 1024 * 1024)))
        // Stille Post: schedule CRUD
        .route("/sp/schedules", get(api::sp_schedules_list).post(api::sp_schedules_create))
        .route("/sp/schedules/{id}", axum::routing::put(api::sp_schedules_update).delete(api::sp_schedules_delete))
        // Stille Post: agent CRUD
        .route("/sp/agents", get(api::sp_agents_list).post(api::sp_agents_create))
        .route("/sp/agents/{id}", get(api::sp_agents_get).put(api::sp_agents_update).delete(api::sp_agents_delete))
        // Stille Post: runs and reports
        .route("/sp/runs", get(api::sp_runs_list).post(api::sp_runs_create))
        .route("/sp/runs/{id}", get(api::sp_runs_get).delete(api::sp_runs_delete))
        .route("/sp/reports", get(api::sp_reports_list))
        .route("/sp/reports/{id}", get(api::sp_reports_get))
        // Stille Post: YAML import/export
        .route("/sp/export", get(api::sp_export))
        .route("/sp/import", post(api::sp_import));

    let app = app
        .route("/embeddings/upload", post(api::embedding::upload)
            .layer(DefaultBodyLimit::max(1024 * 1024 * 1024))) // 1GB
        .route("/embeddings/search", post(api::embedding::search))
        .route("/embeddings/documents", get(api::embedding::list_documents))
        .route("/embeddings/documents/{id}", axum::routing::delete(api::embedding::delete_document));

    let app = app
        .merge(anomaly_rules::anomaly_rules_router())
        .merge(rules::rules_router())
        .merge(catalog_api::catalog_router())
        .layer(CorsLayer::permissive())
        .with_state(state.clone())
        .merge(Scalar::with_url("/docs", api::doc::ApiDoc::openapi()));

    // Bind and start serving IMMEDIATELY.
    let addr = format!("{}:{}", config.server.host, config.server.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    info!("Server listening on http://localhost:{} (data loading in background)", config.server.port);

    // Spawn background data loading task.
    let data_dir = config.storage.data_dir.clone();
    let watcher_data_dir = data_dir.clone();
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
        let result = background::background_load(
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

    // Spawn queue consumers from the encrypted connection store.
    {
        let queue_state = state.clone();
        tokio::spawn(async move {
            queue::spawn_queue_consumers(queue_state).await;
        });
    }

    // Start the eisenbahn event loop and worker runner if connected.
    if let Some(ref eb) = eb_client {
        eb.start().await;
        info!("eisenbahn client active — server registered as api-gateway worker");
    }

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
            import::import(&config, Path::new(path), segment_id)?;
        }
        Some("import-dir") => {
            let path = args.get(2).expect("Usage: server import-dir <directory>");
            import::import_dir(&config, Path::new(path))?;
        }
        Some("import-s3") => {
            let prefix = args.get(2).expect("Usage: server import-s3 <s3-prefix>");
            export::import_s3(&config, prefix).await?;
        }
        Some("export") => {
            let flag = args.get(2).map(|s| s.as_str()).unwrap_or("--all");
            let (do_segments, do_graph) = match flag {
                "--segments" => (true, false),
                "--graph" => (false, true),
                "--all" | _ => (true, true),
            };
            export::export(&config, do_segments, do_graph).await?;
        }
        Some("serve") => {
            let eisenbahn = args.iter().any(|a| a == "--eisenbahn");
            let segment_id = args.iter().skip(2).find(|a| !a.starts_with("--")).map(|s| s.as_str());
            serve(&config, segment_id, eisenbahn).await?;
        }
        _ => {
            println!("stupid-db v0.1.0");
            println!("Usage: server.exe <command>");
            println!("  import <parquet_path> <segment_id>  Import single parquet file");
            println!("  import-dir <directory>               Import all parquet files recursively");
            println!("  import-s3 <s3-prefix>                Import parquet files from S3");
            println!("  export [--segments|--graph|--all]     Export to S3 (default: --all)");
            println!("  serve [segment_id] [--eisenbahn]     Start HTTP server (--eisenbahn enables ZMQ broker)");
        }
    }

    Ok(())
}
