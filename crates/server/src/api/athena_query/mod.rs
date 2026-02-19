//! Athena SQL execution endpoints: SSE streaming query, Parquet download,
//! schema introspection, and query audit log.
//!
//! SRP: Athena SQL execution and schema management.

mod helpers;
mod query_log;
mod query_parquet;
mod query_sse;
mod schema;
mod types;

pub use query_log::athena_connections_query_log;
pub use query_parquet::athena_query_parquet;
pub use query_sse::athena_query_sse;
pub use schema::{athena_connections_schema, athena_connections_schema_refresh};
pub use types::AthenaQueryRequest;

// Re-export utoipa-generated path types so `doc.rs` can reference them
// via `crate::api::athena_query::__path_*`.
pub use query_log::__path_athena_connections_query_log;
pub use query_parquet::__path_athena_query_parquet;
pub use query_sse::__path_athena_query_sse;
pub use schema::{__path_athena_connections_schema, __path_athena_connections_schema_refresh};
