//! Ingestion Manager — source configuration, scheduling, and job tracking.
//!
//! This module defines the type system for the ingestion pipeline:
//! - [`SourceConfig`]: tagged union of source-specific configurations
//! - [`IngestionSource`]: database row from `ingestion_sources`
//! - [`IngestionJob`] / [`IngestionJobStore`]: in-memory job tracking
//! - [`job_runner`]: async job execution with ZMQ event emission

pub mod job_runner;
pub mod queue_listener;
pub mod scheduler;
pub mod source_store;
pub mod types;

pub use source_store::*;
pub use types::*;
