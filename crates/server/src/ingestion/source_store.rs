//! CRUD operations for the `ingestion_sources` PostgreSQL table.
//!
//! [`IngestionSourceStore`] is a stateless unit struct with async methods
//! that take a `&PgPool`. Validation (source type, schedule constraints)
//! is performed before hitting the database.

use chrono::{DateTime, Utc};
use serde::Deserialize;
use sqlx::PgPool;
use tracing::error;
use uuid::Uuid;

use super::types::IngestionSource;

// ── Valid source types ───────────────────────────────────────────────

const VALID_SOURCE_TYPES: &[&str] = &["parquet", "directory", "s3", "csv_json", "push", "queue"];

/// Source types that do NOT support scheduling (one-shot or event-driven).
const NON_SCHEDULABLE_TYPES: &[&str] = &["parquet", "csv_json"];

// ── Request types ────────────────────────────────────────────────────

/// Request body for creating an ingestion source.
#[derive(Debug, Deserialize)]
pub struct CreateIngestionSource {
    pub name: String,
    pub source_type: String,
    pub config_json: serde_json::Value,
    /// Defaults to `"summary"` if not provided.
    pub zmq_granularity: Option<String>,
    pub schedule_json: Option<serde_json::Value>,
    /// Defaults to `true` if not provided.
    pub enabled: Option<bool>,
}

/// Request body for updating an ingestion source (all fields optional).
#[derive(Debug, Deserialize)]
pub struct UpdateIngestionSource {
    pub name: Option<String>,
    pub config_json: Option<serde_json::Value>,
    pub zmq_granularity: Option<String>,
    pub schedule_json: Option<serde_json::Value>,
    pub enabled: Option<bool>,
}

// ── Error type ───────────────────────────────────────────────────────

/// Errors from ingestion source store operations.
#[derive(Debug)]
pub enum IngestionStoreError {
    InvalidSourceType(String),
    ScheduleNotAllowed(String),
    NotFound(Uuid),
    DuplicateName(String),
    Database(sqlx::Error),
}

impl std::fmt::Display for IngestionStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidSourceType(t) => write!(
                f, "invalid source_type '{}': must be one of: {}", t, VALID_SOURCE_TYPES.join(", ")
            ),
            Self::ScheduleNotAllowed(t) => write!(
                f, "source_type '{}' does not support scheduling — schedule_json must be null", t
            ),
            Self::NotFound(id) => write!(f, "ingestion source not found: {}", id),
            Self::DuplicateName(name) => write!(
                f, "duplicate name '{}': an ingestion source with this name already exists", name
            ),
            Self::Database(e) => write!(f, "database error: {}", e),
        }
    }
}

impl std::error::Error for IngestionStoreError {}

impl From<sqlx::Error> for IngestionStoreError {
    fn from(e: sqlx::Error) -> Self {
        Self::Database(e)
    }
}

impl IngestionStoreError {
    /// Map to an HTTP status code for API responses.
    pub fn status_code(&self) -> u16 {
        match self {
            Self::InvalidSourceType(_) | Self::ScheduleNotAllowed(_) => 400,
            Self::NotFound(_) => 404,
            Self::DuplicateName(_) => 409,
            Self::Database(_) => 500,
        }
    }
}

// ── Store ────────────────────────────────────────────────────────────

/// Stateless CRUD store for `ingestion_sources`.
pub struct IngestionSourceStore;

impl IngestionSourceStore {
    /// Create a new ingestion source.
    pub async fn create(
        pool: &PgPool,
        req: CreateIngestionSource,
    ) -> Result<IngestionSource, IngestionStoreError> {
        // Validate source type.
        if !VALID_SOURCE_TYPES.contains(&req.source_type.as_str()) {
            return Err(IngestionStoreError::InvalidSourceType(req.source_type));
        }

        // Reject schedule for non-schedulable types.
        if req.schedule_json.is_some() && NON_SCHEDULABLE_TYPES.contains(&req.source_type.as_str()) {
            return Err(IngestionStoreError::ScheduleNotAllowed(req.source_type));
        }

        let zmq = req.zmq_granularity.unwrap_or_else(|| "summary".to_string());
        let enabled = req.enabled.unwrap_or(true);

        let result = sqlx::query_as::<_, IngestionSource>(
            "INSERT INTO ingestion_sources (name, source_type, config_json, zmq_granularity, schedule_json, enabled)
             VALUES ($1, $2, $3, $4, $5, $6)
             RETURNING id, name, source_type, config_json, zmq_granularity,
                       schedule_json, enabled, created_at, updated_at,
                       last_run_at, next_run_at",
        )
        .bind(&req.name)
        .bind(&req.source_type)
        .bind(&req.config_json)
        .bind(&zmq)
        .bind(&req.schedule_json)
        .bind(enabled)
        .fetch_one(pool)
        .await;

        match result {
            Ok(row) => Ok(row),
            Err(e) => Err(map_unique_violation(e, &req.name)),
        }
    }

    /// List all ingestion sources, ordered by creation time (newest first).
    pub async fn list(pool: &PgPool) -> Result<Vec<IngestionSource>, IngestionStoreError> {
        let rows = sqlx::query_as::<_, IngestionSource>(
            "SELECT id, name, source_type, config_json, zmq_granularity,
                    schedule_json, enabled, created_at, updated_at,
                    last_run_at, next_run_at
             FROM ingestion_sources
             ORDER BY created_at DESC",
        )
        .fetch_all(pool)
        .await?;

        Ok(rows)
    }

    /// Get a single ingestion source by ID.
    pub async fn get(
        pool: &PgPool,
        id: Uuid,
    ) -> Result<Option<IngestionSource>, IngestionStoreError> {
        let row = sqlx::query_as::<_, IngestionSource>(
            "SELECT id, name, source_type, config_json, zmq_granularity,
                    schedule_json, enabled, created_at, updated_at,
                    last_run_at, next_run_at
             FROM ingestion_sources
             WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(pool)
        .await?;

        Ok(row)
    }

    /// Partial update of an ingestion source.
    ///
    /// Uses `COALESCE` so only provided fields are changed.
    pub async fn update(
        pool: &PgPool,
        id: Uuid,
        req: UpdateIngestionSource,
    ) -> Result<IngestionSource, IngestionStoreError> {
        // If schedule_json is being set, check the source type doesn't prohibit it.
        if req.schedule_json.is_some() {
            let source_type = sqlx::query_scalar::<_, String>(
                "SELECT source_type FROM ingestion_sources WHERE id = $1",
            )
            .bind(id)
            .fetch_optional(pool)
            .await?
            .ok_or(IngestionStoreError::NotFound(id))?;

            if NON_SCHEDULABLE_TYPES.contains(&source_type.as_str()) {
                return Err(IngestionStoreError::ScheduleNotAllowed(source_type));
            }
        }

        let result = sqlx::query_as::<_, IngestionSource>(
            "UPDATE ingestion_sources SET
                name = COALESCE($2, name),
                config_json = COALESCE($3, config_json),
                zmq_granularity = COALESCE($4, zmq_granularity),
                schedule_json = COALESCE($5, schedule_json),
                enabled = COALESCE($6, enabled),
                updated_at = now()
             WHERE id = $1
             RETURNING id, name, source_type, config_json, zmq_granularity,
                       schedule_json, enabled, created_at, updated_at,
                       last_run_at, next_run_at",
        )
        .bind(id)
        .bind(&req.name)
        .bind(&req.config_json)
        .bind(&req.zmq_granularity)
        .bind(&req.schedule_json)
        .bind(req.enabled)
        .fetch_optional(pool)
        .await;

        match result {
            Ok(Some(row)) => Ok(row),
            Ok(None) => Err(IngestionStoreError::NotFound(id)),
            Err(e) => Err(map_unique_violation(e, req.name.as_deref().unwrap_or(""))),
        }
    }

    /// Delete an ingestion source by ID.
    pub async fn delete(pool: &PgPool, id: Uuid) -> Result<(), IngestionStoreError> {
        let result = sqlx::query("DELETE FROM ingestion_sources WHERE id = $1")
            .bind(id)
            .execute(pool)
            .await?;

        if result.rows_affected() == 0 {
            return Err(IngestionStoreError::NotFound(id));
        }

        Ok(())
    }

    /// Find enabled, scheduled sources that are due for execution.
    ///
    /// Returns sources where `next_run_at` is NULL (never run) or <= `now`.
    pub async fn find_due_scheduled(
        pool: &PgPool,
        now: DateTime<Utc>,
    ) -> Result<Vec<IngestionSource>, IngestionStoreError> {
        let rows = sqlx::query_as::<_, IngestionSource>(
            "SELECT id, name, source_type, config_json, zmq_granularity,
                    schedule_json, enabled, created_at, updated_at,
                    last_run_at, next_run_at
             FROM ingestion_sources
             WHERE enabled = true
               AND schedule_json IS NOT NULL
               AND (next_run_at IS NULL OR next_run_at <= $1)",
        )
        .bind(now)
        .fetch_all(pool)
        .await?;

        Ok(rows)
    }

    /// Update the `next_run_at` timestamp for a source.
    pub async fn update_next_run_at(
        pool: &PgPool,
        id: Uuid,
        next_run_at: DateTime<Utc>,
    ) -> Result<(), IngestionStoreError> {
        let result = sqlx::query(
            "UPDATE ingestion_sources SET next_run_at = $2 WHERE id = $1",
        )
        .bind(id)
        .bind(next_run_at)
        .execute(pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(IngestionStoreError::NotFound(id));
        }

        Ok(())
    }

    /// Update the `last_run_at` timestamp for a source (set after a job completes).
    pub async fn update_last_run_at(
        pool: &PgPool,
        id: Uuid,
        last_run_at: DateTime<Utc>,
    ) -> Result<(), IngestionStoreError> {
        let result = sqlx::query(
            "UPDATE ingestion_sources SET last_run_at = $2, updated_at = now() WHERE id = $1",
        )
        .bind(id)
        .bind(last_run_at)
        .execute(pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(IngestionStoreError::NotFound(id));
        }

        Ok(())
    }
}

// ── Helpers ──────────────────────────────────────────────────────────

/// Map a PostgreSQL unique violation (23505) to a friendly `DuplicateName` error.
fn map_unique_violation(e: sqlx::Error, name: &str) -> IngestionStoreError {
    if let sqlx::Error::Database(ref db_err) = e {
        if db_err.code().as_deref() == Some("23505") {
            return IngestionStoreError::DuplicateName(name.to_string());
        }
    }
    error!("ingestion source store database error: {}", e);
    IngestionStoreError::Database(e)
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_invalid_source_type_error_message() {
        let err = IngestionStoreError::InvalidSourceType("ftp".to_string());
        let msg = err.to_string();
        assert!(msg.contains("ftp"));
        assert!(msg.contains("parquet"));
        assert!(msg.contains("queue"));
        assert_eq!(err.status_code(), 400);
    }

    #[test]
    fn test_schedule_not_allowed_error() {
        let err = IngestionStoreError::ScheduleNotAllowed("parquet".to_string());
        assert!(err.to_string().contains("parquet"));
        assert!(err.to_string().contains("schedule_json must be null"));
        assert_eq!(err.status_code(), 400);
    }

    #[test]
    fn test_not_found_error() {
        let id = Uuid::new_v4();
        let err = IngestionStoreError::NotFound(id);
        assert!(err.to_string().contains(&id.to_string()));
        assert_eq!(err.status_code(), 404);
    }

    #[test]
    fn test_duplicate_name_error() {
        let err = IngestionStoreError::DuplicateName("my-source".to_string());
        assert!(err.to_string().contains("my-source"));
        assert_eq!(err.status_code(), 409);
    }

    #[test]
    fn test_create_request_defaults_deserialize() {
        let json = r#"{"name":"test","source_type":"parquet","config_json":{"type":"parquet","event_type":"ev"}}"#;
        let req: CreateIngestionSource = serde_json::from_str(json).unwrap();
        assert_eq!(req.name, "test");
        assert_eq!(req.source_type, "parquet");
        assert!(req.zmq_granularity.is_none());
        assert!(req.schedule_json.is_none());
        assert!(req.enabled.is_none());
    }

    #[test]
    fn test_update_request_all_none() {
        let json = r#"{}"#;
        let req: UpdateIngestionSource = serde_json::from_str(json).unwrap();
        assert!(req.name.is_none());
        assert!(req.config_json.is_none());
        assert!(req.zmq_granularity.is_none());
        assert!(req.schedule_json.is_none());
        assert!(req.enabled.is_none());
    }

    #[test]
    fn test_valid_source_types_matches_migration() {
        // Verify our constant matches the CHECK constraint in migration 010.
        assert_eq!(VALID_SOURCE_TYPES, &["parquet", "directory", "s3", "csv_json", "push", "queue"]);
    }

    #[test]
    fn test_non_schedulable_subset() {
        // Non-schedulable types must be a subset of valid types.
        for st in NON_SCHEDULABLE_TYPES {
            assert!(VALID_SOURCE_TYPES.contains(st), "{} not in VALID_SOURCE_TYPES", st);
        }
    }
}
