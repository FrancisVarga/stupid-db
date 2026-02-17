//! Unified rules engine — YAML-based rule definitions with hot-reload.
//!
//! This crate provides:
//! - Multi-kind YAML rule definitions (anomaly, entity schema, features, scoring, trends, patterns)
//! - Rule inheritance via `extends` with deep-merge
//! - Filesystem loader with hot-reload via `notify` watcher
//! - Detection template evaluators (spike, drift, absence, threshold)
//! - Signal composition with AND/OR/NOT trees
//! - OpenSearch enrichment queries

pub mod audit_log;
pub mod enrichment;
pub mod entity_schema;
pub mod evaluator;
pub mod feature_config;
pub mod loader;
pub mod pattern_config;
pub mod scheduler;
pub mod schema;
pub mod scoring_config;
pub mod templates;
pub mod trend_config;
pub mod validation;
