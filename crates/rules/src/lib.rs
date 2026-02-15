//! Anomaly detection YAML DSL rule engine.
//!
//! This crate provides:
//! - YAML-based rule definition with serde deserialization
//! - Filesystem loader with hot-reload via `notify` watcher
//! - Detection template evaluators (spike, drift, absence, threshold)
//! - Signal composition with AND/OR/NOT trees
//! - OpenSearch enrichment queries

pub mod audit_log;
pub mod enrichment;
pub mod evaluator;
pub mod loader;
pub mod scheduler;
pub mod schema;
pub mod templates;
pub mod validation;
