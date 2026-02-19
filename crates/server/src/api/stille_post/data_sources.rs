//! Data source CRUD endpoints including file upload.

use std::sync::Arc;

use axum::extract::{Multipart, Path, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use sqlx::types::Uuid;
use tokio::fs;
use tracing::info;

use crate::state::AppState;

use super::common::{bad_request, internal_error, not_found, require_pg, ApiResult};

// ── Types ────────────────────────────────────────────────────────

const VALID_SOURCE_TYPES: &[&str] = &["athena", "s3", "api", "upload"];

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct SpDataSource {
    pub id: Uuid,
    pub name: String,
    pub source_type: String,
    pub config_json: serde_json::Value,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateDataSourceRequest {
    pub name: String,
    pub source_type: String,
    pub config_json: serde_json::Value,
}

#[derive(Debug, Deserialize)]
pub struct UpdateDataSourceRequest {
    pub name: Option<String>,
    pub source_type: Option<String>,
    pub config_json: Option<serde_json::Value>,
}

// ── Handlers ─────────────────────────────────────────────────────

/// GET /sp/data-sources -- list all data sources.
pub async fn sp_data_sources_list(
    State(state): State<Arc<AppState>>,
) -> ApiResult<Json<Vec<SpDataSource>>> {
    let pool = require_pg(&state)?;
    let rows = sqlx::query_as::<_, SpDataSource>(
        "SELECT id, name, source_type, config_json, created_at, updated_at
         FROM sp_data_sources ORDER BY created_at DESC",
    )
    .fetch_all(pool)
    .await
    .map_err(internal_error)?;
    Ok(Json(rows))
}

/// POST /sp/data-sources -- create a new data source.
pub async fn sp_data_sources_create(
    State(state): State<Arc<AppState>>,
    Json(input): Json<CreateDataSourceRequest>,
) -> ApiResult<(axum::http::StatusCode, Json<SpDataSource>)> {
    if !VALID_SOURCE_TYPES.contains(&input.source_type.as_str()) {
        return Err(bad_request(format!(
            "Invalid source_type '{}'. Must be one of: {}",
            input.source_type,
            VALID_SOURCE_TYPES.join(", ")
        )));
    }
    let pool = require_pg(&state)?;
    let row = sqlx::query_as::<_, SpDataSource>(
        "INSERT INTO sp_data_sources (name, source_type, config_json)
         VALUES ($1, $2, $3)
         RETURNING id, name, source_type, config_json, created_at, updated_at",
    )
    .bind(&input.name)
    .bind(&input.source_type)
    .bind(&input.config_json)
    .fetch_one(pool)
    .await
    .map_err(internal_error)?;
    Ok((axum::http::StatusCode::CREATED, Json(row)))
}

/// GET /sp/data-sources/:id -- get a single data source.
pub async fn sp_data_sources_get(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<SpDataSource>> {
    let pool = require_pg(&state)?;
    let row = sqlx::query_as::<_, SpDataSource>(
        "SELECT id, name, source_type, config_json, created_at, updated_at
         FROM sp_data_sources WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(internal_error)?
    .ok_or_else(|| not_found("Data source", id))?;
    Ok(Json(row))
}

/// PUT /sp/data-sources/:id -- update a data source.
pub async fn sp_data_sources_update(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Json(input): Json<UpdateDataSourceRequest>,
) -> ApiResult<Json<SpDataSource>> {
    if let Some(ref st) = input.source_type {
        if !VALID_SOURCE_TYPES.contains(&st.as_str()) {
            return Err(bad_request(format!(
                "Invalid source_type '{}'. Must be one of: {}",
                st,
                VALID_SOURCE_TYPES.join(", ")
            )));
        }
    }
    let pool = require_pg(&state)?;
    let row = sqlx::query_as::<_, SpDataSource>(
        "UPDATE sp_data_sources SET
            name = COALESCE($2, name),
            source_type = COALESCE($3, source_type),
            config_json = COALESCE($4, config_json),
            updated_at = now()
         WHERE id = $1
         RETURNING id, name, source_type, config_json, created_at, updated_at",
    )
    .bind(id)
    .bind(&input.name)
    .bind(&input.source_type)
    .bind(&input.config_json)
    .fetch_optional(pool)
    .await
    .map_err(internal_error)?
    .ok_or_else(|| not_found("Data source", id))?;
    Ok(Json(row))
}

/// DELETE /sp/data-sources/:id -- delete a data source.
pub async fn sp_data_sources_delete(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> ApiResult<axum::http::StatusCode> {
    let pool = require_pg(&state)?;
    let result = sqlx::query("DELETE FROM sp_data_sources WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await
        .map_err(internal_error)?;
    if result.rows_affected() == 0 {
        return Err(not_found("Data source", id));
    }
    Ok(axum::http::StatusCode::NO_CONTENT)
}

/// POST /sp/data-sources/:id/test -- placeholder connection test.
pub async fn sp_data_sources_test(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let pool = require_pg(&state)?;
    let exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM sp_data_sources WHERE id = $1)",
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .map_err(internal_error)?;
    if !exists {
        return Err(not_found("Data source", id));
    }
    Ok(Json(serde_json::json!({
        "status": "ok",
        "message": "Connection test placeholder — not yet implemented"
    })))
}

/// POST /sp/data-sources/upload -- multipart file upload.
///
/// Accepts multipart/form-data with a `file` field (max 100 MB).
/// Saves the file to `{data_dir}/sp-uploads/{uuid}-{filename}` and creates
/// an `sp_data_sources` record with `source_type='upload'`.
pub async fn sp_data_sources_upload(
    State(state): State<Arc<AppState>>,
    mut multipart: Multipart,
) -> ApiResult<(axum::http::StatusCode, Json<SpDataSource>)> {
    let pool = require_pg(&state)?;

    // Extract file from multipart
    let field = multipart
        .next_field()
        .await
        .map_err(|e| bad_request(format!("Multipart error: {e}")))?
        .ok_or_else(|| bad_request("No file provided"))?;

    let filename = field
        .file_name()
        .unwrap_or("unnamed")
        .to_string();
    let bytes = field
        .bytes()
        .await
        .map_err(|e| bad_request(format!("Failed to read file: {e}")))?;

    // 100 MB limit
    const MAX_UPLOAD: usize = 100 * 1024 * 1024;
    if bytes.len() > MAX_UPLOAD {
        return Err(bad_request(format!(
            "File exceeds 100 MB limit ({} bytes)",
            bytes.len()
        )));
    }

    // Save to {data_dir}/sp-uploads/{uuid}-{filename}
    let file_id = Uuid::new_v4();
    let stored_name = format!("{}-{}", file_id, filename);
    let upload_dir = state.data_dir.join("sp-uploads");
    fs::create_dir_all(&upload_dir)
        .await
        .map_err(|e| internal_error(format!("Failed to create upload dir: {e}")))?;
    let file_path = upload_dir.join(&stored_name);
    fs::write(&file_path, &bytes)
        .await
        .map_err(|e| internal_error(format!("Failed to save file: {e}")))?;

    info!(
        "SP upload: saved '{}' ({} bytes) to {}",
        filename,
        bytes.len(),
        file_path.display()
    );

    // Insert sp_data_sources record
    let config = serde_json::json!({
        "file_path": file_path.to_string_lossy(),
        "original_filename": filename,
        "file_size": bytes.len(),
    });

    let row = sqlx::query_as::<_, SpDataSource>(
        "INSERT INTO sp_data_sources (name, source_type, config_json)
         VALUES ($1, 'upload', $2)
         RETURNING id, name, source_type, config_json, created_at, updated_at",
    )
    .bind(&filename)
    .bind(&config)
    .fetch_one(pool)
    .await
    .map_err(internal_error)?;

    Ok((axum::http::StatusCode::CREATED, Json(row)))
}
