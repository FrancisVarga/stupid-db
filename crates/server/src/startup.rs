//! Server startup: shared state initialization and background task spawning.

use std::sync::Arc;

use tokio::sync::RwLock;
use tracing::{error, info};

use stupid_storage::StorageEngine;

use crate::app_config;
use crate::state::{AppState, LoadingState};
use crate::{athena_connections, background, connections, db, eisenbahn_client, live, queue, queue_connections, rule_runner};

/// Holds ephemeral references needed by background spawns after `AppState` is built.
pub struct StartupContext {
    pub shared_graph: crate::state::SharedGraph,
    pub knowledge: stupid_compute::SharedKnowledgeState,
    pub pipeline: crate::state::SharedPipeline,
    pub catalog: Arc<RwLock<Option<stupid_catalog::Catalog>>>,
    pub segment_ids_shared: Arc<RwLock<Vec<String>>>,
    pub doc_count_shared: Arc<std::sync::atomic::AtomicU64>,
    pub loading: Arc<LoadingState>,
    pub watcher_broadcast_tx: tokio::sync::broadcast::Sender<String>,
    pub catalog_store: Arc<stupid_catalog::CatalogStore>,
    pub eb_client: Option<Arc<eisenbahn_client::EisenbahnClient>>,
}

/// Build `AppState` and return it along with the context needed for background spawns.
pub async fn build_app_state(config: &stupid_core::Config, eisenbahn: bool) -> anyhow::Result<(Arc<AppState>, StartupContext)> {
    // LLM init is config-based and fast -- keep it synchronous.
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

    // Create shared state with empty data -- will be populated by background loader.
    let shared_graph: crate::state::SharedGraph = Arc::new(RwLock::new(stupid_graph::GraphStore::new()));
    let knowledge = stupid_compute::scheduler::state::new_shared_state();
    let pipeline: crate::state::SharedPipeline = Arc::new(std::sync::Mutex::new(stupid_compute::Pipeline::new()));
    let catalog: Arc<RwLock<Option<stupid_catalog::Catalog>>> = Arc::new(RwLock::new(None));
    let segment_ids_shared: Arc<RwLock<Vec<String>>> = Arc::new(RwLock::new(Vec::new()));
    let doc_count_shared = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let loading = Arc::new(LoadingState::new());

    let (broadcast_tx, _) = tokio::sync::broadcast::channel::<String>(64);
    let watcher_broadcast_tx = broadcast_tx.clone();

    let queue_metrics = Arc::new(std::sync::RwLock::new(std::collections::HashMap::new()));

    // Initialize persistent stores.
    let conn_store = connections::ConnectionStore::new(&config.storage.data_dir)
        .expect("Failed to initialize connection store");
    info!("Connection store initialized (data_dir: {})", config.storage.data_dir.display());

    let queue_conn_store = queue_connections::QueueConnectionStore::new(&config.storage.data_dir)
        .expect("Failed to initialize queue connection store");
    info!("Queue connection store initialized (data_dir: {})", config.storage.data_dir.display());

    let athena_conn_store = athena_connections::AthenaConnectionStore::new(&config.storage.data_dir)
        .expect("Failed to initialize Athena connection store");
    info!("Athena connection store initialized");

    let session_store = stupid_agent::session::SessionStore::new(&config.storage.data_dir)
        .expect("Failed to initialize session store");
    info!("Session store initialized");

    let group_store = stupid_agent::group_store::AgentGroupStore::new(&config.storage.data_dir)
        .expect("Failed to initialize agent group store");

    let telemetry_store = stupid_agent::telemetry_store::TelemetryStore::new(&config.storage.data_dir)
        .expect("Failed to initialize telemetry store");

    // Initialize mutable agent store (YAML-backed CRUD with hot-reload).
    let agent_store_dir = config.storage.data_dir.join("agents");
    let agent_store = match stupid_agent::AgentStore::new(&agent_store_dir) {
        Ok(store) => {
            // Seed internal agents that are required by dashboard features.
            seed_internal_agents(&store, &config.storage.data_dir).await;
            info!("Agent store initialized at {}", agent_store_dir.display());
            Some(Arc::new(store))
        }
        Err(e) => {
            tracing::warn!("Failed to create agent store: {} — CRUD endpoints disabled", e);
            None
        }
    };

    // Initialize mutable skill store (YAML-backed CRUD with hot-reload).
    let skill_store_dir = config.storage.data_dir.join("bundeswehr").join("skills");
    let skill_store = match stupid_agent::SkillStore::new(&skill_store_dir) {
        Ok(store) => {
            info!("Skill store initialized at {}", skill_store_dir.display());
            Some(Arc::new(store))
        }
        Err(e) => {
            tracing::warn!("Failed to create skill store: {} — skill CRUD endpoints disabled", e);
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

    // Seed SpAgent prompts from version-controlled YAML into PostgreSQL.
    if let Some(ref pool) = pg_pool {
        seed_sp_agents(pool, &config.storage.data_dir).await;
        seed_prompts(pool, &config.storage.data_dir).await;
    }

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

    let state = Arc::new(AppState {
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
        agent_executor: app_config::build_agent_executor(config),
        agentic_loop: app_config::build_agentic_loop(config),
        connections: Arc::new(RwLock::new(conn_store)),
        queue_connections: Arc::new(RwLock::new(queue_conn_store)),
        athena_connections: Arc::new(RwLock::new(athena_conn_store)),
        embedder: app_config::build_embedder(config),
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
        skill_store,
    });

    let ctx = StartupContext {
        shared_graph,
        knowledge,
        pipeline,
        catalog,
        segment_ids_shared,
        doc_count_shared,
        loading,
        watcher_broadcast_tx,
        catalog_store,
        eb_client,
    };

    Ok((state, ctx))
}

/// Spawn all background tasks (data loading, rule evaluation, file watcher, queue consumers).
pub fn spawn_background_tasks(
    config: &stupid_core::Config,
    state: Arc<AppState>,
    ctx: StartupContext,
    segment_id: Option<&str>,
) -> anyhow::Result<()> {
    let data_dir = config.storage.data_dir.clone();
    let watcher_data_dir = data_dir.clone();
    let storage = StorageEngine::from_config(config)?;
    let single_segment = segment_id.map(|s| s.to_string());

    // Clones for the segment watcher (needs its own references).
    let watcher_graph = ctx.shared_graph.clone();
    let watcher_knowledge = ctx.knowledge.clone();
    let watcher_pipeline = state.pipeline.clone();
    let watcher_segments = ctx.segment_ids_shared.clone();
    let watcher_doc_count = ctx.doc_count_shared.clone();

    let state_for_loader = state.clone();
    tokio::spawn(async move {
        let result = background::background_load(
            storage,
            data_dir,
            single_segment,
            ctx.shared_graph,
            ctx.knowledge,
            ctx.pipeline,
            ctx.catalog,
            ctx.segment_ids_shared,
            ctx.doc_count_shared,
            ctx.loading,
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
    let watcher_catalog = state.catalog.clone();
    let watcher_catalog_store = ctx.catalog_store;
    tokio::spawn(async move {
        live::start_segment_watcher(
            watcher_data_dir,
            watcher_graph,
            watcher_knowledge,
            watcher_pipeline,
            watcher_segments,
            watcher_doc_count,
            ctx.watcher_broadcast_tx,
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

    Ok(())
}

/// Seed SpAgent prompts from version-controlled YAML files into PostgreSQL.
///
/// Reads `data/stille-post/*.yml` files with `kind: SpAgent`, parses them
/// using the existing `SpYamlEnvelope` schema, and upserts into the
/// `sp_agents` table. YAML is source of truth — existing rows are overwritten.
async fn seed_sp_agents(pool: &sqlx::PgPool, data_dir: &std::path::Path) {
    use crate::api::stille_post::yaml_types::{SpAgentSpec, SpYamlEnvelope, SpYamlKind};

    let seed_dir = data_dir.join("stille-post");
    let entries = match std::fs::read_dir(&seed_dir) {
        Ok(e) => e,
        Err(e) => {
            tracing::warn!("stille-post seed dir not readable: {} — skipping SpAgent seed", e);
            return;
        }
    };

    let mut seeded = 0u32;
    for entry in entries.flatten() {
        let path = entry.path();
        if !matches!(path.extension().and_then(|e| e.to_str()), Some("yml" | "yaml")) {
            continue;
        }
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                error!("failed to read stille-post YAML {}: {}", path.display(), e);
                continue;
            }
        };

        // Support multi-document YAML files (split on `---` separators).
        for doc_str in content.split("\n---") {
            let trimmed = doc_str.trim();
            if trimmed.is_empty() {
                continue;
            }
            let envelope: SpYamlEnvelope = match serde_yaml::from_str(trimmed) {
                Ok(env) => env,
                Err(_) => continue, // skip non-envelope fragments (comments, non-agent docs)
            };
            if envelope.kind != SpYamlKind::SpAgent {
                continue;
            }
            let spec: SpAgentSpec = match serde_yaml::from_value(envelope.spec) {
                Ok(s) => s,
                Err(e) => {
                    error!("failed to parse SpAgent spec in {}: {}", path.display(), e);
                    continue;
                }
            };

            let name = &envelope.metadata.name;
            let model = spec.model.as_deref().unwrap_or("claude-sonnet-4-6");
            let skills = serde_json::to_value(&spec.skills_config).unwrap_or_default();
            let mcp = serde_json::to_value(&spec.mcp_servers_config).unwrap_or_default();
            let tools = serde_json::to_value(&spec.tools_config).unwrap_or_default();

            let result = sqlx::query(
                "INSERT INTO sp_agents (name, description, system_prompt, model,
                     skills_config, mcp_servers_config, tools_config, template_id)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                 ON CONFLICT (name) DO UPDATE SET
                     description = EXCLUDED.description,
                     system_prompt = EXCLUDED.system_prompt,
                     model = EXCLUDED.model,
                     skills_config = EXCLUDED.skills_config,
                     mcp_servers_config = EXCLUDED.mcp_servers_config,
                     tools_config = EXCLUDED.tools_config,
                     template_id = EXCLUDED.template_id,
                     updated_at = now()",
            )
            .bind(name)
            .bind(&envelope.metadata.description)
            .bind(&spec.system_prompt)
            .bind(model)
            .bind(&skills)
            .bind(&mcp)
            .bind(&tools)
            .bind(&spec.template_id)
            .execute(pool)
            .await;

            match result {
                Ok(_) => seeded += 1,
                Err(e) => error!("failed to upsert SpAgent '{}': {}", name, e),
            }
        }
    }

    if seeded > 0 {
        info!("Seeded {} SpAgents from {}", seeded, seed_dir.display());
    }
}

/// Seed internal agents required by dashboard features.
///
/// Recursively scans `data/bundeswehr/agents/` (including subdirectories like
/// `internal/`, `security/`, `analytics/`) and creates any missing agents in
/// the main AgentStore. This ensures agents like `playground-assistant` are
/// always available on fresh deployments.
async fn seed_internal_agents(store: &stupid_agent::AgentStore, data_dir: &std::path::Path) {
    let seed_dir = data_dir.join("bundeswehr").join("agents");
    if !seed_dir.exists() {
        return;
    }
    for entry in walkdir::WalkDir::new(&seed_dir).follow_links(true).into_iter() {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!("walkdir error scanning seed agents: {}", e);
                continue;
            }
        };
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.into_path();
        if !matches!(path.extension().and_then(|e| e.to_str()), Some("yml" | "yaml")) {
            continue;
        }
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("failed to read seed agent {}: {}", path.display(), e);
                continue;
            }
        };
        let config: stupid_agent::yaml_schema::AgentYamlConfig = match serde_yaml::from_str(&content) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("failed to parse seed agent {}: {}", path.display(), e);
                continue;
            }
        };
        if store.get(&config.name).await.is_none() {
            let name = config.name.clone();
            match store.create(config).await {
                Ok(_) => info!("seeded internal agent: {}", name),
                Err(e) => tracing::warn!("failed to seed agent {}: {}", name, e),
            }
        }
    }
}

/// Seed prompt templates from version-controlled markdown files into PostgreSQL.
///
/// Reads `data/bundeswehr/prompts/*.md` files, extracts `<<<placeholder>>>`
/// patterns, and upserts into the `prompts` table. YAML/markdown files are
/// source of truth — existing rows are overwritten on each startup.
async fn seed_prompts(pool: &sqlx::PgPool, data_dir: &std::path::Path) {
    let seed_dir = data_dir.join("bundeswehr").join("prompts");
    let entries = match std::fs::read_dir(&seed_dir) {
        Ok(e) => e,
        Err(e) => {
            tracing::warn!("prompts seed dir not readable: {} — skipping prompt seed", e);
            return;
        }
    };

    let mut seeded = 0u32;
    for entry in entries.flatten() {
        let path = entry.path();
        if !matches!(path.extension().and_then(|e| e.to_str()), Some("md")) {
            continue;
        }
        let name = match path.file_stem().and_then(|s| s.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                error!("failed to read prompt template {}: {}", path.display(), e);
                continue;
            }
        };

        let placeholders = crate::api::prompts::extract_placeholders(&content);

        let result = sqlx::query(
            "INSERT INTO prompts (name, content, placeholders, updated_at)
             VALUES ($1, $2, $3, NOW())
             ON CONFLICT (name) DO UPDATE SET
                 content = EXCLUDED.content,
                 placeholders = EXCLUDED.placeholders,
                 updated_at = NOW()",
        )
        .bind(&name)
        .bind(&content)
        .bind(&placeholders)
        .execute(pool)
        .await;

        match result {
            Ok(_) => seeded += 1,
            Err(e) => error!("failed to upsert prompt '{}': {}", name, e),
        }
    }

    if seeded > 0 {
        info!("Seeded {} prompt templates from {}", seeded, seed_dir.display());
    }
}

#[cfg(test)]
mod tests {
    use crate::api::stille_post::yaml_types::{SpAgentSpec, SpYamlEnvelope, SpYamlKind};

    /// Verify all YAML files in data/stille-post/ with kind=SpAgent parse correctly.
    /// Handles both single-document and multi-document YAML files.
    #[test]
    fn test_stille_post_agent_yamls_parse() {
        let seed_dir = std::path::Path::new("../../data/stille-post");
        if !seed_dir.exists() {
            // CI may not have the data dir — skip gracefully.
            return;
        }
        let mut agent_count = 0;
        for entry in std::fs::read_dir(seed_dir).unwrap().flatten() {
            let path = entry.path();
            if !matches!(path.extension().and_then(|e| e.to_str()), Some("yml" | "yaml")) {
                continue;
            }
            let content = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("failed to read {}: {}", path.display(), e));

            for doc_str in content.split("\n---") {
                let trimmed = doc_str.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let envelope: SpYamlEnvelope = match serde_yaml::from_str(trimmed) {
                    Ok(e) => e,
                    Err(_) => continue, // skip non-envelope fragments
                };
                if envelope.kind != SpYamlKind::SpAgent {
                    continue;
                }

                let spec: SpAgentSpec = serde_yaml::from_value(envelope.spec)
                    .unwrap_or_else(|e| panic!("failed to parse SpAgent spec {}: {}", path.display(), e));

                assert!(!envelope.metadata.name.is_empty(), "agent name empty in {}", path.display());
                assert!(!spec.system_prompt.is_empty(), "system_prompt empty in {}", path.display());
                agent_count += 1;
            }
        }
        assert!(agent_count >= 2, "expected at least 2 SpAgent YAMLs, found {}", agent_count);
    }
}
