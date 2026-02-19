//! Catalog API sub-router.
//!
//! Exposes the full catalog surface: merged catalog, segment partials,
//! external SQL sources, snapshots, and query plan execution.
//!
//! Mount via `.merge(catalog_router())` in main.rs.

mod externals;
mod metadata;
mod query;
mod segments;
mod snapshots;
mod types;

pub use externals::*;
pub use metadata::*;
pub use query::*;
pub use segments::*;
pub use snapshots::*;
pub use types::*;

use std::sync::Arc;

use axum::routing::{get, post};
use axum::Router;

use crate::state::AppState;

// ── Router ──────────────────────────────────────────────────────

/// Build the catalog sub-router.
///
/// Mount on the main router with `.merge(catalog_router())`.
pub fn catalog_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/catalog", get(get_catalog))
        .route("/catalog/manifest", get(get_manifest))
        .route("/catalog/rebuild", post(rebuild_catalog))
        .route("/catalog/segments", get(list_segments))
        .route(
            "/catalog/segments/{id}",
            get(get_segment).delete(delete_segment),
        )
        // External sources: list (lightweight) + create
        .route(
            "/catalog/externals",
            get(list_externals).post(add_external),
        )
        // External sources: get (shallow/full) + delete
        .route(
            "/catalog/externals/{kind}/{connection_id}",
            get(get_external).delete(delete_external),
        )
        // Drill-down: databases
        .route(
            "/catalog/externals/{kind}/{connection_id}/databases",
            get(list_databases),
        )
        // Drill-down: tables
        .route(
            "/catalog/externals/{kind}/{connection_id}/databases/{db_name}/tables",
            get(list_tables),
        )
        // Drill-down: columns
        .route(
            "/catalog/externals/{kind}/{connection_id}/databases/{db_name}/tables/{table_name}/columns",
            get(list_columns),
        )
        .route("/catalog/snapshots", post(create_snapshot))
        .route("/catalog/query", post(execute_query))
}

// ── Tests ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use stupid_catalog::{ExternalColumn, ExternalDatabase, ExternalSource, ExternalTable};

    fn sample_source() -> ExternalSource {
        ExternalSource {
            name: "Production Data Lake".into(),
            kind: "athena".into(),
            connection_id: "prod-lake".into(),
            databases: vec![
                ExternalDatabase {
                    name: "analytics".into(),
                    tables: vec![
                        ExternalTable {
                            name: "events".into(),
                            columns: vec![
                                ExternalColumn { name: "id".into(), data_type: "bigint".into() },
                                ExternalColumn { name: "user_id".into(), data_type: "bigint".into() },
                                ExternalColumn { name: "ts".into(), data_type: "timestamp".into() },
                                ExternalColumn { name: "event_type".into(), data_type: "varchar".into() },
                            ],
                        },
                        ExternalTable {
                            name: "users".into(),
                            columns: vec![
                                ExternalColumn { name: "id".into(), data_type: "bigint".into() },
                                ExternalColumn { name: "username".into(), data_type: "varchar".into() },
                            ],
                        },
                    ],
                },
                ExternalDatabase {
                    name: "raw".into(),
                    tables: vec![ExternalTable {
                        name: "logs".into(),
                        columns: vec![
                            ExternalColumn { name: "line".into(), data_type: "text".into() },
                        ],
                    }],
                },
            ],
        }
    }

    fn sample_sources() -> Vec<ExternalSource> {
        vec![
            sample_source(),
            ExternalSource {
                name: "Staging Lake".into(),
                kind: "athena".into(),
                connection_id: "staging".into(),
                databases: vec![],
            },
            ExternalSource {
                name: "Production Postgres".into(),
                kind: "postgres".into(),
                connection_id: "prod-pg".into(),
                databases: vec![ExternalDatabase {
                    name: "app".into(),
                    tables: vec![],
                }],
            },
        ]
    }

    // ── ExternalSourceSummary mapping ─────────────────────────

    #[test]
    fn source_summary_has_correct_database_count() {
        let src = sample_source();
        let summary = ExternalSourceSummary {
            database_count: src.databases.len(),
            name: src.name.clone(),
            kind: src.kind.clone(),
            connection_id: src.connection_id.clone(),
        };
        assert_eq!(summary.database_count, 2);
        assert_eq!(summary.name, "Production Data Lake");
    }

    #[test]
    fn source_summary_excludes_nested_data() {
        let src = sample_source();
        let summary = ExternalSourceSummary {
            database_count: src.databases.len(),
            name: src.name,
            kind: src.kind,
            connection_id: src.connection_id,
        };
        let json = serde_json::to_value(&summary).unwrap();
        assert!(json.get("databases").is_none());
        assert!(json.get("tables").is_none());
        assert!(json.get("columns").is_none());
        assert_eq!(json["database_count"], 2);
    }

    // ── Search filtering ──────────────────────────────────────

    #[test]
    fn search_filters_sources_case_insensitive() {
        let sources = sample_sources();
        let search = Some("production".to_lowercase());

        let filtered: Vec<_> = sources
            .iter()
            .filter(|s| {
                search
                    .as_ref()
                    .map_or(true, |q| s.name.to_lowercase().contains(q))
            })
            .collect();

        assert_eq!(filtered.len(), 2); // "Production Data Lake" + "Production Postgres"
    }

    #[test]
    fn search_returns_all_when_none() {
        let sources = sample_sources();
        let search: Option<String> = None;

        let filtered: Vec<_> = sources
            .iter()
            .filter(|s| {
                search
                    .as_ref()
                    .map_or(true, |q| s.name.to_lowercase().contains(q))
            })
            .collect();

        assert_eq!(filtered.len(), 3);
    }

    // ── DatabaseSummary mapping ───────────────────────────────

    #[test]
    fn database_summary_has_correct_table_count() {
        let src = sample_source();
        let summaries: Vec<DatabaseSummary> = src
            .databases
            .iter()
            .map(|db| DatabaseSummary {
                name: db.name.clone(),
                table_count: db.tables.len(),
            })
            .collect();

        assert_eq!(summaries.len(), 2);
        assert_eq!(summaries[0].name, "analytics");
        assert_eq!(summaries[0].table_count, 2);
        assert_eq!(summaries[1].name, "raw");
        assert_eq!(summaries[1].table_count, 1);
    }

    #[test]
    fn database_search_filters_by_name() {
        let src = sample_source();
        let search = Some("ana".to_string());

        let filtered: Vec<_> = src
            .databases
            .iter()
            .filter(|db| {
                search
                    .as_ref()
                    .map_or(true, |q| db.name.to_lowercase().contains(&q.to_lowercase()))
            })
            .collect();

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].name, "analytics");
    }

    // ── TableSummary mapping ──────────────────────────────────

    #[test]
    fn table_summary_has_correct_column_count() {
        let src = sample_source();
        let db = &src.databases[0]; // analytics

        let summaries: Vec<TableSummary> = db
            .tables
            .iter()
            .map(|t| TableSummary {
                name: t.name.clone(),
                column_count: t.columns.len(),
            })
            .collect();

        assert_eq!(summaries.len(), 2);
        assert_eq!(summaries[0].name, "events");
        assert_eq!(summaries[0].column_count, 4);
        assert_eq!(summaries[1].name, "users");
        assert_eq!(summaries[1].column_count, 2);
    }

    // ── Column filtering ──────────────────────────────────────

    #[test]
    fn column_filter_by_name() {
        let src = sample_source();
        let table = &src.databases[0].tables[0]; // events
        let search = Some("user".to_lowercase());

        let filtered: Vec<_> = table
            .columns
            .iter()
            .filter(|c| {
                search
                    .as_ref()
                    .map_or(true, |q| c.name.to_lowercase().contains(q))
            })
            .collect();

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].name, "user_id");
    }

    #[test]
    fn column_filter_by_data_type() {
        let src = sample_source();
        let table = &src.databases[0].tables[0]; // events
        let data_type = Some("bigint".to_string());

        let filtered: Vec<_> = table
            .columns
            .iter()
            .filter(|c| {
                data_type
                    .as_ref()
                    .map_or(true, |dt| c.data_type == *dt)
            })
            .collect();

        assert_eq!(filtered.len(), 2); // id + user_id
    }

    #[test]
    fn column_filter_combined_search_and_type() {
        let src = sample_source();
        let table = &src.databases[0].tables[0]; // events
        let search = Some("id".to_lowercase());
        let data_type = Some("bigint".to_string());

        let filtered: Vec<_> = table
            .columns
            .iter()
            .filter(|c| {
                search
                    .as_ref()
                    .map_or(true, |q| c.name.to_lowercase().contains(q))
            })
            .filter(|c| {
                data_type
                    .as_ref()
                    .map_or(true, |dt| c.data_type == *dt)
            })
            .collect();

        assert_eq!(filtered.len(), 2); // id + user_id (both contain "id" and are bigint)
    }

    #[test]
    fn column_filter_no_match() {
        let src = sample_source();
        let table = &src.databases[0].tables[0]; // events

        let filtered: Vec<_> = table
            .columns
            .iter()
            .filter(|c| c.data_type == "boolean")
            .collect();

        assert!(filtered.is_empty());
    }

    // ── find_database / find_table helpers ─────────────────────

    #[test]
    fn find_database_returns_ok_for_existing() {
        let src = sample_source();
        let result = externals::find_database(&src, "analytics");
        assert!(result.is_ok());
        assert_eq!(result.unwrap().name, "analytics");
    }

    #[test]
    fn find_database_returns_404_for_missing() {
        let src = sample_source();
        let result = externals::find_database(&src, "nonexistent");
        assert!(result.is_err());
        let (status, body) = result.unwrap_err();
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert!(body.error.contains("nonexistent"));
    }

    #[test]
    fn find_table_returns_ok_for_existing() {
        let src = sample_source();
        let db = &src.databases[0];
        let result = externals::find_table(db, "events", &src);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().name, "events");
    }

    #[test]
    fn find_table_returns_404_for_missing() {
        let src = sample_source();
        let db = &src.databases[0];
        let result = externals::find_table(db, "nonexistent", &src);
        assert!(result.is_err());
        let (status, body) = result.unwrap_err();
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert!(body.error.contains("nonexistent"));
    }

    // ── Depth param ───────────────────────────────────────────

    #[test]
    fn depth_full_returns_complete_tree() {
        let src = sample_source();
        let is_full = Some("full".to_string())
            .as_deref()
            .map_or(false, |d| d.eq_ignore_ascii_case("full"));
        assert!(is_full);

        let json = serde_json::to_value(&src).unwrap();
        assert!(json.get("databases").is_some());
    }

    #[test]
    fn depth_shallow_returns_summary() {
        let is_full = Some("shallow".to_string())
            .as_deref()
            .map_or(false, |d| d.eq_ignore_ascii_case("full"));
        assert!(!is_full);

        let is_full_none: bool = None::<String>
            .as_deref()
            .map_or(false, |d: &str| d.eq_ignore_ascii_case("full"));
        assert!(!is_full_none);
    }

    // ── JSON round-trip ───────────────────────────────────────

    #[test]
    fn summary_types_serialize_correctly() {
        let summary = ExternalSourceSummary {
            name: "Lake".into(),
            kind: "athena".into(),
            connection_id: "prod".into(),
            database_count: 3,
        };
        let json = serde_json::to_string(&summary).unwrap();
        let restored: ExternalSourceSummary = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.name, "Lake");
        assert_eq!(restored.database_count, 3);

        let db_summary = DatabaseSummary { name: "db1".into(), table_count: 5 };
        let json = serde_json::to_string(&db_summary).unwrap();
        let restored: DatabaseSummary = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.table_count, 5);

        let table_summary = TableSummary { name: "t1".into(), column_count: 10 };
        let json = serde_json::to_string(&table_summary).unwrap();
        let restored: TableSummary = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.column_count, 10);
    }
}
