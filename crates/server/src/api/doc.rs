//! OpenAPI documentation aggregator.
//!
//! Collects all `#[utoipa::path]`-annotated handlers and `ToSchema`-derived
//! types into a single OpenAPI 3.1 spec, served via Scalar UI at `/docs`.

use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(
    info(
        title = "stupid-db API",
        version = "0.1.0",
        description = "Knowledge-graph database with semantic search, anomaly detection, and agentic workflows.",
    ),
    tags(
        (name = "Health", description = "Server readiness, stats, and operational metrics"),
        (name = "Catalog", description = "Schema catalog CRUD, segment partials, external sources, snapshots, and query execution"),
        (name = "Graph", description = "Knowledge-graph node queries and force-layout data"),
        (name = "Compute", description = "PageRank, communities, co-occurrence, trends, and anomalies"),
        (name = "Query", description = "Natural-language query via LLM-generated plans"),
        (name = "Embeddings", description = "Document upload, semantic search via pgvector"),
        (name = "Connections", description = "Database connection CRUD with encrypted credential storage"),
        (name = "Queue Connections", description = "Message-queue connection CRUD"),
        (name = "Athena Connections", description = "AWS Athena connection CRUD and schema introspection"),
        (name = "Athena Queries", description = "Athena SQL queries with SSE streaming and Parquet export"),
        (name = "Agents", description = "LLM agent execution (single-agent and team)"),
        (name = "Sessions", description = "Persistent agent chat sessions"),
        (name = "Anomaly Rules", description = "Anomaly detection rule CRUD, execution, and history"),
        (name = "Rules", description = "Generic rule CRUD (anomaly, schema, feature, scoring, trend, pattern)"),
    ),
    paths(
        // Health
        crate::api::health::health,
        crate::api::health::loading,
        crate::api::health::stats,
        crate::api::health::queue_status,
        crate::api::health::scheduler_metrics,
        // Catalog
        crate::catalog_api::get_catalog,
        crate::catalog_api::get_manifest,
        crate::catalog_api::rebuild_catalog,
        crate::catalog_api::list_segments,
        crate::catalog_api::get_segment,
        crate::catalog_api::delete_segment,
        crate::catalog_api::list_externals,
        crate::catalog_api::get_external,
        crate::catalog_api::add_external,
        crate::catalog_api::delete_external,
        crate::catalog_api::create_snapshot,
        crate::catalog_api::execute_query,
        // Graph
        crate::api::graph::graph_nodes,
        crate::api::graph::graph_node_by_id,
        crate::api::graph::graph_force,
        // Compute
        crate::api::compute::compute_pagerank,
        crate::api::compute::compute_communities,
        crate::api::compute::compute_degrees,
        crate::api::compute::compute_patterns,
        crate::api::compute::compute_cooccurrence,
        crate::api::compute::compute_trends,
        crate::api::compute::compute_anomalies,
        // Query
        crate::api::query::query,
        // Embeddings
        crate::api::embedding::upload,
        crate::api::embedding::search,
        crate::api::embedding::list_documents,
        crate::api::embedding::delete_document,
        // Connections
        crate::api::connections::connections_list,
        crate::api::connections::connections_add,
        crate::api::connections::connections_get,
        crate::api::connections::connections_update,
        crate::api::connections::connections_delete,
        crate::api::connections::connections_credentials,
        // Queue Connections
        crate::api::connections::queue_connections_list,
        crate::api::connections::queue_connections_add,
        crate::api::connections::queue_connections_get,
        crate::api::connections::queue_connections_update,
        crate::api::connections::queue_connections_delete,
        crate::api::connections::queue_connections_credentials,
        // Athena Connections
        crate::api::connections::athena_connections_list,
        crate::api::connections::athena_connections_add,
        crate::api::connections::athena_connections_get,
        crate::api::connections::athena_connections_update,
        crate::api::connections::athena_connections_delete,
        crate::api::connections::athena_connections_credentials,
        // Athena Queries
        crate::api::athena_query::athena_query_sse,
        crate::api::athena_query::athena_query_parquet,
        crate::api::athena_query::athena_connections_schema,
        crate::api::athena_query::athena_connections_schema_refresh,
        crate::api::athena_query::athena_connections_query_log,
        // Agents
        crate::api::agents::agents_list,
        crate::api::agents::agents_execute,
        crate::api::agents::agents_chat,
        crate::api::agents::teams_execute,
        crate::api::agents::teams_strategies,
        // Sessions
        crate::api::agents::sessions_list,
        crate::api::agents::sessions_create,
        crate::api::agents::sessions_get,
        crate::api::agents::sessions_update,
        crate::api::agents::sessions_delete,
        crate::api::agents::sessions_execute_agent,
        crate::api::agents::sessions_execute_team,
        crate::api::agents::sessions_execute,
        crate::api::agents::sessions_stream,
        // Anomaly Rules
        crate::anomaly_rules::list_anomaly_rules,
        crate::anomaly_rules::create_anomaly_rule,
        crate::anomaly_rules::get_anomaly_rule,
        crate::anomaly_rules::update_anomaly_rule,
        crate::anomaly_rules::delete_anomaly_rule,
        crate::anomaly_rules::start_anomaly_rule,
        crate::anomaly_rules::pause_anomaly_rule,
        crate::anomaly_rules::run_anomaly_rule,
        crate::anomaly_rules::test_notify_rule,
        crate::anomaly_rules::rule_history,
        crate::anomaly_rules::rule_logs,
        // Rules
        crate::rules::list_rules,
        crate::rules::get_rule,
        crate::rules::get_rule_yaml,
        crate::rules::create_rule,
        crate::rules::update_rule,
        crate::rules::delete_rule,
        crate::rules::toggle_rule,
        crate::rules::recent_triggers,
    ),
    components(schemas(
        // Shared
        crate::api::NotReadyResponse,
        crate::api::QueryErrorResponse,
        // Health
        crate::api::health::HealthResponse,
        crate::api::health::StatsResponse,
        // Graph
        crate::api::graph::NodeResponse,
        crate::api::graph::NodeDetailResponse,
        crate::api::graph::NeighborResponse,
        crate::api::graph::ForceGraphResponse,
        crate::api::graph::ForceNode,
        crate::api::graph::ForceLink,
        // Compute
        crate::api::compute::PageRankEntry,
        crate::api::compute::CommunitySummary,
        crate::api::compute::CommunityNode,
        crate::api::compute::DegreeEntry,
        crate::api::compute::PatternResponse,
        crate::api::compute::CooccurrenceEntry,
        crate::api::compute::CooccurrenceResponse,
        crate::api::compute::TrendResponse,
        crate::api::compute::FeatureDimension,
        crate::api::compute::AnomalyEntry,
        // Query
        crate::api::query::QueryRequest,
        crate::api::query::QueryResponse,
        // Embeddings
        crate::api::embedding::SearchRequest,
        crate::api::embedding::UploadResponse,
        crate::api::embedding::SearchResponse,
        crate::api::embedding::DocumentListResponse,
        // Athena Query
        crate::api::athena_query::AthenaQueryRequest,
        // Connections
        crate::connections::ConnectionSafe,
        crate::connections::ConnectionCredentials,
        crate::connections::ConnectionInput,
        crate::queue_connections::QueueConnectionSafe,
        crate::queue_connections::QueueConnectionCredentials,
        crate::queue_connections::QueueConnectionInput,
        crate::athena_connections::AthenaConnectionSafe,
        crate::athena_connections::AthenaConnectionCredentials,
        crate::athena_connections::AthenaConnectionInput,
        crate::athena_connections::AthenaSchema,
        crate::athena_connections::AthenaDatabase,
        crate::athena_connections::AthenaTable,
        crate::athena_connections::AthenaColumn,
        // Agents
        crate::api::agents::AgentExecuteRequest,
        crate::api::agents::TeamExecuteRequest,
        crate::api::agents::SessionCreateRequest,
        crate::api::agents::SessionUpdateRequest,
        crate::api::agents::SessionExecuteAgentRequest,
        crate::api::agents::SessionExecuteTeamRequest,
        crate::api::agents::SessionExecuteRequest,
        crate::api::agents::SessionStreamRequest,
        // Catalog
        crate::catalog_api::SegmentListResponse,
        crate::catalog_api::RebuildResponse,
        crate::catalog_api::SnapshotResponse,
        crate::catalog_api::QueryExecuteRequest,
        // Anomaly Rules
        crate::anomaly_rules::RuleSummary,
        crate::anomaly_rules::RunResult,
        crate::anomaly_rules::TestNotifyResult,
        crate::anomaly_rules::MatchSummary,
        crate::anomaly_rules::TriggerEntry,
        // Rules
        crate::rules::RecentTrigger,
        crate::rules::GenericRuleSummary,
    ))
)]
pub struct ApiDoc;
