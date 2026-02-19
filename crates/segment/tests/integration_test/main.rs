/// Integration tests for the document store covering full pipeline, parquet import,
/// segment rotation, eviction, persistence, scan filters, and statistics.

mod helpers;
mod pipeline;
mod rotation;
mod scan_filters;
mod store_ops;
