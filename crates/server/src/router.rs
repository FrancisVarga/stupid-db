//! HTTP router construction.
//!
//! Assembles all Axum routes, middleware, and OpenAPI docs into a single `Router`.

use std::sync::Arc;

use axum::extract::DefaultBodyLimit;
use axum::routing::{get, post};
use axum::Router;
use tower_http::cors::CorsLayer;
use utoipa::OpenApi;
use utoipa_scalar::{Scalar, Servable};

use crate::state::AppState;
use crate::{anomaly_rules, api, catalog_api, live, rules};

/// Build the complete application router with all routes and middleware.
pub fn build_router(state: Arc<AppState>) -> Router {
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
        .route(
            "/api/agents/{name}",
            get(api::agents_get)
                .put(api::agents_update)
                .delete(api::agents_delete),
        )
        .route("/api/agents", post(api::agents_create))
        .route("/teams/execute", post(api::teams_execute))
        .route("/teams/strategies", get(api::teams_strategies))
        .route(
            "/sessions",
            get(api::sessions_list).post(api::sessions_create),
        )
        .route(
            "/sessions/{id}",
            get(api::sessions_get)
                .put(api::sessions_update)
                .delete(api::sessions_delete),
        )
        .route(
            "/sessions/{id}/execute-agent",
            post(api::sessions_execute_agent),
        )
        .route(
            "/sessions/{id}/execute-team",
            post(api::sessions_execute_team),
        )
        .route("/sessions/{id}/execute", post(api::sessions_execute))
        .route("/sessions/{id}/stream", post(api::sessions_stream))
        .route(
            "/connections",
            get(api::connections_list).post(api::connections_add),
        )
        .route(
            "/connections/{id}",
            get(api::connections_get)
                .put(api::connections_update)
                .delete(api::connections_delete),
        )
        .route(
            "/connections/{id}/credentials",
            get(api::connections_credentials),
        )
        // Bundeswehr fleet overview
        .route("/api/bundeswehr/overview", get(api::bundeswehr_overview))
        // Bundeswehr skill CRUD
        .route(
            "/api/bundeswehr/skills",
            get(api::skills_list).post(api::skills_create),
        )
        .route(
            "/api/bundeswehr/skills/{name}",
            get(api::skills_get)
                .put(api::skills_update)
                .delete(api::skills_delete),
        )
        // Telemetry: overview MUST precede {agent_name} to avoid capture
        .route("/api/telemetry/overview", get(api::telemetry_overview))
        .route("/api/telemetry/{agent_name}", get(api::telemetry_events))
        .route(
            "/api/telemetry/{agent_name}/stats",
            get(api::telemetry_stats),
        )
        // Agent Groups
        .route(
            "/api/agent-groups",
            get(api::agent_groups_list).post(api::agent_groups_create),
        )
        .route(
            "/api/agent-groups/{name}",
            axum::routing::put(api::agent_groups_update).delete(api::agent_groups_delete),
        )
        .route(
            "/api/agent-groups/{name}/agents",
            post(api::agent_groups_add_agent),
        )
        .route(
            "/api/agent-groups/{group_name}/{agent_name}",
            axum::routing::delete(api::agent_groups_remove_agent),
        )
        // Prompt templates
        .route("/api/prompts", get(api::prompts_list))
        .route(
            "/api/prompts/{name}",
            get(api::prompts_get).put(api::prompts_update),
        )
        // Ingestion Manager
        .route(
            "/api/ingestion/sources",
            get(api::ingestion_sources_list).post(api::ingestion_sources_create),
        )
        .route(
            "/api/ingestion/sources/{id}",
            get(api::ingestion_sources_get)
                .put(api::ingestion_sources_update)
                .delete(api::ingestion_sources_delete),
        )
        .route(
            "/api/ingestion/sources/{id}/trigger",
            post(api::ingestion_sources_trigger),
        )
        .route("/api/ingestion/jobs", get(api::ingestion_jobs_list))
        .route("/api/ingestion/jobs/{id}", get(api::ingestion_jobs_get))
        // Villa layout engine
        .route("/api/villa/suggest", post(api::villa_suggest))
        .route("/ws", get(live::ws_upgrade));

    let app = app
        .route(
            "/queue-connections",
            get(api::queue_connections_list).post(api::queue_connections_add),
        )
        .route(
            "/queue-connections/{id}",
            get(api::queue_connections_get)
                .put(api::queue_connections_update)
                .delete(api::queue_connections_delete),
        )
        .route(
            "/queue-connections/{id}/credentials",
            get(api::queue_connections_credentials),
        )
        .route(
            "/athena-connections",
            get(api::athena_connections_list).post(api::athena_connections_add),
        )
        .route(
            "/athena-connections/{id}",
            get(api::athena_connections_get)
                .put(api::athena_connections_update)
                .delete(api::athena_connections_delete),
        )
        .route(
            "/athena-connections/{id}/credentials",
            get(api::athena_connections_credentials),
        )
        .route(
            "/athena-connections/{id}/query",
            post(api::athena_query_sse),
        )
        .route(
            "/athena-connections/{id}/query/parquet",
            post(api::athena_query_parquet),
        )
        .route(
            "/athena-connections/{id}/schema",
            get(api::athena_connections_schema),
        )
        .route(
            "/athena-connections/{id}/schema/refresh",
            post(api::athena_connections_schema_refresh),
        )
        .route(
            "/athena-connections/{id}/query-log",
            get(api::athena_connections_query_log),
        )
        // Stille Post: pipeline CRUD
        .route(
            "/sp/pipelines",
            get(api::sp_pipelines_list).post(api::sp_pipelines_create),
        )
        .route(
            "/sp/pipelines/{id}",
            get(api::sp_pipelines_get)
                .put(api::sp_pipelines_update)
                .delete(api::sp_pipelines_delete),
        )
        // Stille Post: delivery CRUD
        .route(
            "/sp/deliveries",
            get(api::sp_deliveries_list).post(api::sp_deliveries_create),
        )
        .route(
            "/sp/deliveries/{id}",
            axum::routing::put(api::sp_deliveries_update).delete(api::sp_deliveries_delete),
        )
        .route("/sp/deliveries/{id}/test", post(api::sp_deliveries_test))
        // Stille Post: data source CRUD
        .route(
            "/sp/data-sources",
            get(api::sp_data_sources_list).post(api::sp_data_sources_create),
        )
        .route(
            "/sp/data-sources/{id}",
            get(api::sp_data_sources_get)
                .put(api::sp_data_sources_update)
                .delete(api::sp_data_sources_delete),
        )
        .route(
            "/sp/data-sources/{id}/test",
            post(api::sp_data_sources_test),
        )
        .route(
            "/sp/data-sources/upload",
            post(api::sp_data_sources_upload).layer(DefaultBodyLimit::max(100 * 1024 * 1024)),
        )
        // Stille Post: schedule CRUD
        .route(
            "/sp/schedules",
            get(api::sp_schedules_list).post(api::sp_schedules_create),
        )
        .route(
            "/sp/schedules/{id}",
            axum::routing::put(api::sp_schedules_update).delete(api::sp_schedules_delete),
        )
        // Stille Post: agent CRUD
        .route(
            "/sp/agents",
            get(api::sp_agents_list).post(api::sp_agents_create),
        )
        .route(
            "/sp/agents/{id}",
            get(api::sp_agents_get)
                .put(api::sp_agents_update)
                .delete(api::sp_agents_delete),
        )
        // Stille Post: runs and reports
        .route(
            "/sp/runs",
            get(api::sp_runs_list).post(api::sp_runs_create),
        )
        .route(
            "/sp/runs/{id}",
            get(api::sp_runs_get).delete(api::sp_runs_delete),
        )
        .route("/sp/reports", get(api::sp_reports_list))
        .route("/sp/reports/{id}", get(api::sp_reports_get))
        // Stille Post: YAML import/export
        .route("/sp/export", get(api::sp_export))
        .route("/sp/import", post(api::sp_import));

    let app = app
        .route(
            "/embeddings/upload",
            post(api::embedding::upload).layer(DefaultBodyLimit::max(1024 * 1024 * 1024)), // 1GB
        )
        .route("/embeddings/search", post(api::embedding::search))
        .route(
            "/embeddings/documents",
            get(api::embedding::list_documents),
        )
        .route(
            "/embeddings/documents/{id}",
            axum::routing::delete(api::embedding::delete_document),
        );

    app.merge(anomaly_rules::anomaly_rules_router())
        .merge(rules::rules_router())
        .merge(catalog_api::catalog_router())
        .layer(CorsLayer::permissive())
        .with_state(state)
        .merge(Scalar::with_url("/docs", api::doc::ApiDoc::openapi()))
}
