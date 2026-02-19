//! Ingestion API route handlers — sources (CRUD) and jobs (list/get/trigger).

pub mod jobs;
pub mod sources;

pub use jobs::{ingestion_jobs_list, ingestion_jobs_get};
pub use sources::{
    ingestion_sources_list, ingestion_sources_create, ingestion_sources_get,
    ingestion_sources_update, ingestion_sources_delete, ingestion_sources_trigger,
};
