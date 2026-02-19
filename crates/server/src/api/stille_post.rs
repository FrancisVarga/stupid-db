//! Stille Post CRUD endpoints for agents, pipelines, data sources,
//! schedules, runs, reports, and deliveries.
//!
//! All endpoints require a PostgreSQL pool (`pg_pool`) on AppState.

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Multipart, Path, State};
use axum::http::header;
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};
use sqlx::types::Uuid;
use tokio::fs;
use tracing::info;

use crate::state::AppState;

use super::QueryErrorResponse;

// ── Types ────────────────────────────────────────────────────────

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct SpPipeline {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize)]
pub struct SpPipelineWithSteps {
    #[serde(flatten)]
    pub pipeline: SpPipeline,
    pub steps: Vec<SpPipelineStep>,
}

#[derive(Debug, Serialize)]
pub struct SpPipelineListItem {
    #[serde(flatten)]
    pub pipeline: SpPipeline,
    pub step_count: i64,
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct SpPipelineStep {
    pub id: Uuid,
    pub pipeline_id: Uuid,
    pub agent_id: Option<Uuid>,
    pub step_order: i32,
    pub input_mapping: serde_json::Value,
    pub output_mapping: serde_json::Value,
    pub parallel_group: Option<i32>,
    pub data_source_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct CreatePipelineRequest {
    pub name: String,
    pub description: Option<String>,
    #[serde(default)]
    pub steps: Vec<CreatePipelineStepRequest>,
}

#[derive(Debug, Deserialize)]
pub struct CreatePipelineStepRequest {
    pub agent_id: Option<Uuid>,
    pub step_order: i32,
    #[serde(default = "default_json_object")]
    pub input_mapping: serde_json::Value,
    #[serde(default = "default_json_object")]
    pub output_mapping: serde_json::Value,
    pub parallel_group: Option<i32>,
    pub data_source_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct UpdatePipelineRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub steps: Option<Vec<CreatePipelineStepRequest>>,
}

fn default_json_object() -> serde_json::Value {
    serde_json::json!({})
}

// ── Helpers ──────────────────────────────────────────────────────

type ApiResult<T> = Result<T, (axum::http::StatusCode, Json<QueryErrorResponse>)>;

fn require_pg(state: &AppState) -> ApiResult<&sqlx::PgPool> {
    state.pg_pool.as_ref().ok_or_else(|| {
        (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            Json(QueryErrorResponse {
                error: "PostgreSQL not configured".into(),
            }),
        )
    })
}

fn internal_error(e: impl std::fmt::Display) -> (axum::http::StatusCode, Json<QueryErrorResponse>) {
    (
        axum::http::StatusCode::INTERNAL_SERVER_ERROR,
        Json(QueryErrorResponse {
            error: e.to_string(),
        }),
    )
}

fn not_found(resource: &str, id: Uuid) -> (axum::http::StatusCode, Json<QueryErrorResponse>) {
    (
        axum::http::StatusCode::NOT_FOUND,
        Json(QueryErrorResponse {
            error: format!("{} not found: {}", resource, id),
        }),
    )
}

// ── Pipeline CRUD ────────────────────────────────────────────────

/// List all pipelines with step counts.
pub async fn sp_pipelines_list(
    State(state): State<Arc<AppState>>,
) -> ApiResult<Json<Vec<SpPipelineListItem>>> {
    let pool = require_pg(&state)?;

    let rows = sqlx::query_as::<_, SpPipeline>(
        "SELECT id, name, description, created_at, updated_at
         FROM sp_pipelines ORDER BY created_at DESC",
    )
    .fetch_all(pool)
    .await
    .map_err(internal_error)?;

    // Batch-fetch step counts
    let ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();
    let counts: std::collections::HashMap<Uuid, i64> = if !ids.is_empty() {
        sqlx::query_as::<_, (Uuid, i64)>(
            "SELECT pipeline_id, COUNT(*) FROM sp_pipeline_steps
             WHERE pipeline_id = ANY($1) GROUP BY pipeline_id",
        )
        .bind(&ids)
        .fetch_all(pool)
        .await
        .map_err(internal_error)?
        .into_iter()
        .collect()
    } else {
        std::collections::HashMap::new()
    };

    let items = rows
        .into_iter()
        .map(|p| {
            let step_count = counts.get(&p.id).copied().unwrap_or(0);
            SpPipelineListItem {
                pipeline: p,
                step_count,
            }
        })
        .collect();

    Ok(Json(items))
}

/// Create a pipeline with steps (atomic transaction).
pub async fn sp_pipelines_create(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreatePipelineRequest>,
) -> ApiResult<(axum::http::StatusCode, Json<SpPipelineWithSteps>)> {
    let pool = require_pg(&state)?;
    let mut tx = pool.begin().await.map_err(internal_error)?;

    let pipeline = sqlx::query_as::<_, SpPipeline>(
        "INSERT INTO sp_pipelines (name, description) VALUES ($1, $2)
         RETURNING id, name, description, created_at, updated_at",
    )
    .bind(&req.name)
    .bind(&req.description)
    .fetch_one(&mut *tx)
    .await
    .map_err(internal_error)?;

    let mut steps = Vec::with_capacity(req.steps.len());
    for s in &req.steps {
        let step = sqlx::query_as::<_, SpPipelineStep>(
            "INSERT INTO sp_pipeline_steps
                (pipeline_id, agent_id, step_order, input_mapping, output_mapping, parallel_group, data_source_id)
             VALUES ($1, $2, $3, $4, $5, $6, $7)
             RETURNING id, pipeline_id, agent_id, step_order, input_mapping, output_mapping, parallel_group, data_source_id",
        )
        .bind(pipeline.id)
        .bind(s.agent_id)
        .bind(s.step_order)
        .bind(&s.input_mapping)
        .bind(&s.output_mapping)
        .bind(s.parallel_group)
        .bind(s.data_source_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(internal_error)?;
        steps.push(step);
    }

    tx.commit().await.map_err(internal_error)?;

    Ok((
        axum::http::StatusCode::CREATED,
        Json(SpPipelineWithSteps { pipeline, steps }),
    ))
}

/// Get a single pipeline with all its steps.
pub async fn sp_pipelines_get(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<SpPipelineWithSteps>> {
    let pool = require_pg(&state)?;

    let pipeline = sqlx::query_as::<_, SpPipeline>(
        "SELECT id, name, description, created_at, updated_at
         FROM sp_pipelines WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(internal_error)?
    .ok_or_else(|| not_found("Pipeline", id))?;

    let steps = sqlx::query_as::<_, SpPipelineStep>(
        "SELECT id, pipeline_id, agent_id, step_order, input_mapping, output_mapping, parallel_group, data_source_id
         FROM sp_pipeline_steps WHERE pipeline_id = $1 ORDER BY step_order",
    )
    .bind(id)
    .fetch_all(pool)
    .await
    .map_err(internal_error)?;

    Ok(Json(SpPipelineWithSteps { pipeline, steps }))
}

/// Update a pipeline and optionally replace its steps (atomic transaction).
pub async fn sp_pipelines_update(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdatePipelineRequest>,
) -> ApiResult<Json<SpPipelineWithSteps>> {
    let pool = require_pg(&state)?;
    let mut tx = pool.begin().await.map_err(internal_error)?;

    // Update pipeline fields
    let pipeline = sqlx::query_as::<_, SpPipeline>(
        "UPDATE sp_pipelines SET
            name = COALESCE($2, name),
            description = COALESCE($3, description),
            updated_at = now()
         WHERE id = $1
         RETURNING id, name, description, created_at, updated_at",
    )
    .bind(id)
    .bind(&req.name)
    .bind(&req.description)
    .fetch_optional(&mut *tx)
    .await
    .map_err(internal_error)?
    .ok_or_else(|| not_found("Pipeline", id))?;

    // If steps are provided, replace all existing steps
    let steps = if let Some(new_steps) = &req.steps {
        sqlx::query("DELETE FROM sp_pipeline_steps WHERE pipeline_id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(internal_error)?;

        let mut steps = Vec::with_capacity(new_steps.len());
        for s in new_steps {
            let step = sqlx::query_as::<_, SpPipelineStep>(
                "INSERT INTO sp_pipeline_steps
                    (pipeline_id, agent_id, step_order, input_mapping, output_mapping, parallel_group, data_source_id)
                 VALUES ($1, $2, $3, $4, $5, $6, $7)
                 RETURNING id, pipeline_id, agent_id, step_order, input_mapping, output_mapping, parallel_group, data_source_id",
            )
            .bind(id)
            .bind(s.agent_id)
            .bind(s.step_order)
            .bind(&s.input_mapping)
            .bind(&s.output_mapping)
            .bind(s.parallel_group)
            .bind(s.data_source_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(internal_error)?;
            steps.push(step);
        }
        steps
    } else {
        // No step changes — return existing steps
        sqlx::query_as::<_, SpPipelineStep>(
            "SELECT id, pipeline_id, agent_id, step_order, input_mapping, output_mapping, parallel_group, data_source_id
             FROM sp_pipeline_steps WHERE pipeline_id = $1 ORDER BY step_order",
        )
        .bind(id)
        .fetch_all(&mut *tx)
        .await
        .map_err(internal_error)?
    };

    tx.commit().await.map_err(internal_error)?;

    Ok(Json(SpPipelineWithSteps { pipeline, steps }))
}

/// Delete a pipeline (cascade deletes steps via FK).
pub async fn sp_pipelines_delete(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> ApiResult<axum::http::StatusCode> {
    let pool = require_pg(&state)?;

    let result = sqlx::query("DELETE FROM sp_pipelines WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await
        .map_err(internal_error)?;

    if result.rows_affected() == 0 {
        return Err(not_found("Pipeline", id));
    }

    Ok(axum::http::StatusCode::NO_CONTENT)
}

// ══════════════════════════════════════════════════════════════════
//  Data Sources
// ══════════════════════════════════════════════════════════════════

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

fn bad_request(msg: impl Into<String>) -> (axum::http::StatusCode, Json<QueryErrorResponse>) {
    (
        axum::http::StatusCode::BAD_REQUEST,
        Json(QueryErrorResponse { error: msg.into() }),
    )
}

/// GET /sp/data-sources — list all data sources.
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

/// POST /sp/data-sources — create a new data source.
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

/// GET /sp/data-sources/:id — get a single data source.
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

/// PUT /sp/data-sources/:id — update a data source.
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

/// DELETE /sp/data-sources/:id — delete a data source.
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

/// POST /sp/data-sources/:id/test — placeholder connection test.
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

/// POST /sp/data-sources/upload — multipart file upload.
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

// ══════════════════════════════════════════════════════════════════
//  Deliveries
// ══════════════════════════════════════════════════════════════════

const VALID_CHANNELS: &[&str] = &["email", "webhook", "telegram"];

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct SpDelivery {
    pub id: Uuid,
    pub schedule_id: Option<Uuid>,
    pub channel: String,
    pub config_json: serde_json::Value,
    pub enabled: bool,
}

#[derive(Debug, Deserialize)]
pub struct CreateDeliveryRequest {
    pub schedule_id: Uuid,
    pub channel: String,
    pub config_json: serde_json::Value,
    pub enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateDeliveryRequest {
    pub channel: Option<String>,
    pub config_json: Option<serde_json::Value>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct DeliveryListQuery {
    pub schedule_id: Option<Uuid>,
}

fn validate_channel(channel: &str) -> ApiResult<()> {
    if VALID_CHANNELS.contains(&channel) {
        Ok(())
    } else {
        Err(bad_request(format!(
            "Invalid channel '{}'. Must be one of: {}",
            channel,
            VALID_CHANNELS.join(", ")
        )))
    }
}

/// GET /sp/deliveries — list deliveries, optionally filtered by schedule_id.
pub async fn sp_deliveries_list(
    State(state): State<Arc<AppState>>,
    axum::extract::Query(q): axum::extract::Query<DeliveryListQuery>,
) -> ApiResult<Json<Vec<SpDelivery>>> {
    let pool = require_pg(&state)?;
    let rows = if let Some(sid) = q.schedule_id {
        sqlx::query_as::<_, SpDelivery>(
            "SELECT id, schedule_id, channel, config_json, enabled \
             FROM sp_deliveries WHERE schedule_id = $1 ORDER BY id",
        )
        .bind(sid)
        .fetch_all(pool)
        .await
    } else {
        sqlx::query_as::<_, SpDelivery>(
            "SELECT id, schedule_id, channel, config_json, enabled \
             FROM sp_deliveries ORDER BY id",
        )
        .fetch_all(pool)
        .await
    };
    rows.map(Json).map_err(internal_error)
}

/// POST /sp/deliveries — create a delivery configuration.
pub async fn sp_deliveries_create(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateDeliveryRequest>,
) -> ApiResult<(axum::http::StatusCode, Json<SpDelivery>)> {
    validate_channel(&req.channel)?;
    let pool = require_pg(&state)?;
    let enabled = req.enabled.unwrap_or(true);

    let row = sqlx::query_as::<_, SpDelivery>(
        "INSERT INTO sp_deliveries (schedule_id, channel, config_json, enabled) \
         VALUES ($1, $2, $3, $4) \
         RETURNING id, schedule_id, channel, config_json, enabled",
    )
    .bind(req.schedule_id)
    .bind(&req.channel)
    .bind(&req.config_json)
    .bind(enabled)
    .fetch_one(pool)
    .await
    .map_err(internal_error)?;

    Ok((axum::http::StatusCode::CREATED, Json(row)))
}

/// PUT /sp/deliveries/:id — update a delivery configuration.
pub async fn sp_deliveries_update(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateDeliveryRequest>,
) -> ApiResult<Json<SpDelivery>> {
    if let Some(ref ch) = req.channel {
        validate_channel(ch)?;
    }
    let pool = require_pg(&state)?;

    let row = sqlx::query_as::<_, SpDelivery>(
        "UPDATE sp_deliveries SET \
            channel     = COALESCE($2, channel), \
            config_json = COALESCE($3, config_json), \
            enabled     = COALESCE($4, enabled) \
         WHERE id = $1 \
         RETURNING id, schedule_id, channel, config_json, enabled",
    )
    .bind(id)
    .bind(&req.channel)
    .bind(&req.config_json)
    .bind(req.enabled)
    .fetch_optional(pool)
    .await
    .map_err(internal_error)?
    .ok_or_else(|| not_found("Delivery", id))?;

    Ok(Json(row))
}

/// DELETE /sp/deliveries/:id — delete a delivery configuration.
pub async fn sp_deliveries_delete(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> ApiResult<axum::http::StatusCode> {
    let pool = require_pg(&state)?;

    let result = sqlx::query("DELETE FROM sp_deliveries WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await
        .map_err(internal_error)?;

    if result.rows_affected() == 0 {
        return Err(not_found("Delivery", id));
    }

    Ok(axum::http::StatusCode::NO_CONTENT)
}

/// POST /sp/deliveries/:id/test — test a delivery channel (placeholder).
pub async fn sp_deliveries_test(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let pool = require_pg(&state)?;

    let delivery = sqlx::query_as::<_, SpDelivery>(
        "SELECT id, schedule_id, channel, config_json, enabled \
         FROM sp_deliveries WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(internal_error)?
    .ok_or_else(|| not_found("Delivery", id))?;

    Ok(Json(serde_json::json!({
        "status": "ok",
        "message": format!("Test for '{}' channel — not yet implemented", delivery.channel),
        "delivery_id": delivery.id,
        "channel": delivery.channel,
    })))
}

// ══════════════════════════════════════════════════════════════════
//  Schedules
// ══════════════════════════════════════════════════════════════════

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct SpSchedule {
    pub id: Uuid,
    pub pipeline_id: Uuid,
    pub cron_expression: String,
    pub timezone: String,
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_run_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_run_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Extended schedule with joined pipeline name for list views.
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct SpScheduleWithPipeline {
    pub id: Uuid,
    pub pipeline_id: Uuid,
    pub cron_expression: String,
    pub timezone: String,
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_run_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_run_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Pipeline name from the joined sp_pipelines table.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pipeline_name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateScheduleRequest {
    pub pipeline_id: Uuid,
    pub cron_expression: String,
    pub timezone: Option<String>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateScheduleRequest {
    pub cron_expression: Option<String>,
    pub timezone: Option<String>,
    pub enabled: Option<bool>,
}

// ── Cron validation ─────────────────────────────────────────────

/// Validate a 5-field cron expression (minute hour day-of-month month day-of-week).
/// Accepts standard tokens: *, numbers, ranges (1-5), lists (1,3,5), steps (*/5).
fn validate_cron(expr: &str) -> Result<(), String> {
    let fields: Vec<&str> = expr.split_whitespace().collect();
    if fields.len() != 5 {
        return Err(format!(
            "Cron expression must have exactly 5 fields (minute hour day month weekday), got {}",
            fields.len()
        ));
    }
    let field_names = ["minute", "hour", "day-of-month", "month", "day-of-week"];
    for (i, field) in fields.iter().enumerate() {
        if !is_valid_cron_field(field) {
            return Err(format!(
                "Invalid cron field '{}' at position {} ({})",
                field, i, field_names[i]
            ));
        }
    }
    Ok(())
}

/// Check if a single cron field token is syntactically valid.
/// Supports: `*`, `*/N`, `N`, `N-M`, `N-M/S`, comma-separated lists of the above.
fn is_valid_cron_field(field: &str) -> bool {
    if field.is_empty() {
        return false;
    }
    for part in field.split(',') {
        if !is_valid_cron_atom(part) {
            return false;
        }
    }
    true
}

fn is_valid_cron_atom(atom: &str) -> bool {
    if atom.is_empty() {
        return false;
    }
    let (range_part, step_part) = match atom.split_once('/') {
        Some((r, s)) => (r, Some(s)),
        None => (atom, None),
    };
    if let Some(step) = step_part {
        if step.is_empty() || !step.chars().all(|c| c.is_ascii_digit()) {
            return false;
        }
    }
    if range_part == "*" {
        return true;
    }
    if let Some((lo, hi)) = range_part.split_once('-') {
        is_cron_value(lo) && is_cron_value(hi)
    } else {
        is_cron_value(range_part)
    }
}

fn is_cron_value(v: &str) -> bool {
    if v.is_empty() {
        return false;
    }
    if v.chars().all(|c| c.is_ascii_digit()) {
        return true;
    }
    // Named day/month (3-letter: MON, TUE, JAN, FEB, etc.)
    v.len() == 3 && v.chars().all(|c| c.is_ascii_alphabetic())
}

// ── Schedule CRUD ───────────────────────────────────────────────

/// GET /sp/schedules — list all schedules (with pipeline name).
pub async fn sp_schedules_list(
    State(state): State<Arc<AppState>>,
) -> ApiResult<Json<Vec<SpScheduleWithPipeline>>> {
    let pool = require_pg(&state)?;
    let rows = sqlx::query_as::<_, SpScheduleWithPipeline>(
        r#"SELECT s.id, s.pipeline_id, s.cron_expression, s.timezone,
                  s.enabled, s.last_run_at, s.next_run_at, s.created_at,
                  p.name AS pipeline_name
           FROM sp_schedules s
           LEFT JOIN sp_pipelines p ON p.id = s.pipeline_id
           ORDER BY s.created_at DESC"#,
    )
    .fetch_all(pool)
    .await
    .map_err(internal_error)?;
    Ok(Json(rows))
}

/// POST /sp/schedules — create a new schedule.
pub async fn sp_schedules_create(
    State(state): State<Arc<AppState>>,
    Json(input): Json<CreateScheduleRequest>,
) -> ApiResult<(axum::http::StatusCode, Json<SpSchedule>)> {
    let pool = require_pg(&state)?;

    validate_cron(&input.cron_expression).map_err(|msg| {
        bad_request(msg)
    })?;

    let tz = input.timezone.as_deref().unwrap_or("UTC");
    let enabled = input.enabled.unwrap_or(true);

    let row = sqlx::query_as::<_, SpSchedule>(
        r#"INSERT INTO sp_schedules (pipeline_id, cron_expression, timezone, enabled)
           VALUES ($1, $2, $3, $4)
           RETURNING id, pipeline_id, cron_expression, timezone, enabled,
                     last_run_at, next_run_at, created_at"#,
    )
    .bind(input.pipeline_id)
    .bind(&input.cron_expression)
    .bind(tz)
    .bind(enabled)
    .fetch_one(pool)
    .await
    .map_err(internal_error)?;

    Ok((axum::http::StatusCode::CREATED, Json(row)))
}

/// PUT /sp/schedules/:id — update a schedule (enable/disable, change cron, change timezone).
pub async fn sp_schedules_update(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Json(input): Json<UpdateScheduleRequest>,
) -> ApiResult<Json<SpSchedule>> {
    let pool = require_pg(&state)?;

    if let Some(ref cron) = input.cron_expression {
        validate_cron(cron).map_err(|msg| bad_request(msg))?;
    }

    let row = sqlx::query_as::<_, SpSchedule>(
        r#"UPDATE sp_schedules
           SET cron_expression = COALESCE($2, cron_expression),
               timezone        = COALESCE($3, timezone),
               enabled         = COALESCE($4, enabled)
           WHERE id = $1
           RETURNING id, pipeline_id, cron_expression, timezone, enabled,
                     last_run_at, next_run_at, created_at"#,
    )
    .bind(id)
    .bind(input.cron_expression.as_deref())
    .bind(input.timezone.as_deref())
    .bind(input.enabled)
    .fetch_optional(pool)
    .await
    .map_err(internal_error)?
    .ok_or_else(|| not_found("Schedule", id))?;

    Ok(Json(row))
}

/// DELETE /sp/schedules/:id — delete a schedule.
pub async fn sp_schedules_delete(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> ApiResult<axum::http::StatusCode> {
    let pool = require_pg(&state)?;

    let result = sqlx::query("DELETE FROM sp_schedules WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await
        .map_err(internal_error)?;

    if result.rows_affected() == 0 {
        return Err(not_found("Schedule", id));
    }

    Ok(axum::http::StatusCode::NO_CONTENT)
}

// ══════════════════════════════════════════════════════════════════
//  Runs & Reports
// ══════════════════════════════════════════════════════════════════

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct SpRun {
    pub id: Uuid,
    pub pipeline_id: Option<Uuid>,
    pub schedule_id: Option<Uuid>,
    pub status: String,
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub error: Option<String>,
    pub trigger_type: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize)]
pub struct SpRunWithSteps {
    #[serde(flatten)]
    pub run: SpRun,
    pub steps: Vec<SpStepResult>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct SpStepResult {
    pub id: Uuid,
    pub run_id: Uuid,
    pub step_id: Option<Uuid>,
    pub agent_id: Option<Uuid>,
    pub input_data: Option<serde_json::Value>,
    pub output_data: Option<serde_json::Value>,
    pub tokens_used: Option<i32>,
    pub duration_ms: Option<i32>,
    pub status: String,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct SpReport {
    pub id: Uuid,
    pub run_id: Uuid,
    pub title: String,
    pub content_html: Option<String>,
    pub content_json: Option<serde_json::Value>,
    pub render_blocks: Option<serde_json::Value>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize)]
pub struct TriggerRunRequest {
    pub pipeline_id: Uuid,
}

#[derive(Debug, Deserialize)]
pub struct RunListQuery {
    pub pipeline_id: Option<Uuid>,
    pub status: Option<String>,
}

/// GET /sp/runs — list runs with optional pipeline_id and status filters.
pub async fn sp_runs_list(
    State(state): State<Arc<AppState>>,
    axum::extract::Query(q): axum::extract::Query<RunListQuery>,
) -> ApiResult<Json<Vec<SpRun>>> {
    let pool = require_pg(&state)?;

    let rows = sqlx::query_as::<_, SpRun>(
        "SELECT id, pipeline_id, schedule_id, status, started_at, completed_at,
                error, trigger_type, created_at
         FROM sp_runs
         WHERE ($1::uuid IS NULL OR pipeline_id = $1)
           AND ($2::text  IS NULL OR status = $2)
         ORDER BY created_at DESC
         LIMIT 100",
    )
    .bind(q.pipeline_id)
    .bind(&q.status)
    .fetch_all(pool)
    .await
    .map_err(internal_error)?;

    Ok(Json(rows))
}

/// GET /sp/runs/:id — get a run with all step results.
pub async fn sp_runs_get(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<SpRunWithSteps>> {
    let pool = require_pg(&state)?;

    let run = sqlx::query_as::<_, SpRun>(
        "SELECT id, pipeline_id, schedule_id, status, started_at, completed_at,
                error, trigger_type, created_at
         FROM sp_runs WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(internal_error)?
    .ok_or_else(|| not_found("Run", id))?;

    let steps = sqlx::query_as::<_, SpStepResult>(
        "SELECT id, run_id, step_id, agent_id, input_data, output_data,
                tokens_used, duration_ms, status
         FROM sp_step_results WHERE run_id = $1
         ORDER BY id",
    )
    .bind(id)
    .fetch_all(pool)
    .await
    .map_err(internal_error)?;

    Ok(Json(SpRunWithSteps { run, steps }))
}

/// POST /sp/runs — trigger a manual pipeline run (creates a pending record).
pub async fn sp_runs_create(
    State(state): State<Arc<AppState>>,
    Json(req): Json<TriggerRunRequest>,
) -> ApiResult<(axum::http::StatusCode, Json<SpRun>)> {
    let pool = require_pg(&state)?;

    let run = sqlx::query_as::<_, SpRun>(
        "INSERT INTO sp_runs (pipeline_id, status, trigger_type)
         VALUES ($1, 'pending', 'manual')
         RETURNING id, pipeline_id, schedule_id, status, started_at, completed_at,
                   error, trigger_type, created_at",
    )
    .bind(req.pipeline_id)
    .fetch_one(pool)
    .await
    .map_err(internal_error)?;

    Ok((axum::http::StatusCode::CREATED, Json(run)))
}

/// DELETE /sp/runs/:id — cancel or delete a pipeline run.
pub async fn sp_runs_delete(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> ApiResult<axum::http::StatusCode> {
    let pool = require_pg(&state)?;

    let result = sqlx::query("DELETE FROM sp_runs WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await
        .map_err(internal_error)?;

    if result.rows_affected() == 0 {
        return Err(not_found("Run", id));
    }

    Ok(axum::http::StatusCode::NO_CONTENT)
}

/// GET /sp/reports — list all reports (newest first).
pub async fn sp_reports_list(
    State(state): State<Arc<AppState>>,
) -> ApiResult<Json<Vec<SpReport>>> {
    let pool = require_pg(&state)?;

    let rows = sqlx::query_as::<_, SpReport>(
        "SELECT id, run_id, title, content_html, content_json, render_blocks, created_at
         FROM sp_reports
         ORDER BY created_at DESC
         LIMIT 100",
    )
    .fetch_all(pool)
    .await
    .map_err(internal_error)?;

    Ok(Json(rows))
}

/// GET /sp/reports/:id — get a single report.
pub async fn sp_reports_get(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<SpReport>> {
    let pool = require_pg(&state)?;

    let report = sqlx::query_as::<_, SpReport>(
        "SELECT id, run_id, title, content_html, content_json, render_blocks, created_at
         FROM sp_reports WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(internal_error)?
    .ok_or_else(|| not_found("Report", id))?;

    Ok(Json(report))
}

// ══════════════════════════════════════════════════════════════════
//  Agents
// ══════════════════════════════════════════════════════════════════

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct SpAgent {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub system_prompt: String,
    pub model: String,
    pub skills_config: serde_json::Value,
    pub mcp_servers_config: serde_json::Value,
    pub tools_config: serde_json::Value,
    pub template_id: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateAgentRequest {
    pub name: String,
    pub description: Option<String>,
    pub system_prompt: String,
    pub model: Option<String>,
    pub skills_config: Option<serde_json::Value>,
    pub mcp_servers_config: Option<serde_json::Value>,
    pub tools_config: Option<serde_json::Value>,
    pub template_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateAgentRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub system_prompt: Option<String>,
    pub model: Option<String>,
    pub skills_config: Option<serde_json::Value>,
    pub mcp_servers_config: Option<serde_json::Value>,
    pub tools_config: Option<serde_json::Value>,
    pub template_id: Option<String>,
}

/// GET /sp/agents — list all agents.
pub async fn sp_agents_list(
    State(state): State<Arc<AppState>>,
) -> ApiResult<Json<Vec<SpAgent>>> {
    let pool = require_pg(&state)?;
    let rows = sqlx::query_as::<_, SpAgent>(
        "SELECT id, name, description, system_prompt, model, \
                skills_config, mcp_servers_config, tools_config, template_id, \
                created_at, updated_at \
         FROM sp_agents ORDER BY created_at DESC",
    )
    .fetch_all(pool)
    .await
    .map_err(internal_error)?;
    Ok(Json(rows))
}

/// POST /sp/agents — create a new agent.
pub async fn sp_agents_create(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateAgentRequest>,
) -> ApiResult<(axum::http::StatusCode, Json<SpAgent>)> {
    let pool = require_pg(&state)?;
    let empty_arr = serde_json::json!([]);
    let row = sqlx::query_as::<_, SpAgent>(
        "INSERT INTO sp_agents \
            (name, description, system_prompt, model, skills_config, \
             mcp_servers_config, tools_config, template_id) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8) \
         RETURNING *",
    )
    .bind(&req.name)
    .bind(&req.description)
    .bind(&req.system_prompt)
    .bind(req.model.as_deref().unwrap_or("claude-sonnet-4-6"))
    .bind(req.skills_config.as_ref().unwrap_or(&empty_arr))
    .bind(req.mcp_servers_config.as_ref().unwrap_or(&empty_arr))
    .bind(req.tools_config.as_ref().unwrap_or(&empty_arr))
    .bind(&req.template_id)
    .fetch_one(pool)
    .await
    .map_err(internal_error)?;
    Ok((axum::http::StatusCode::CREATED, Json(row)))
}

/// GET /sp/agents/:id — get agent by ID.
pub async fn sp_agents_get(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<SpAgent>> {
    let pool = require_pg(&state)?;
    let row = sqlx::query_as::<_, SpAgent>("SELECT * FROM sp_agents WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("Agent", id))?;
    Ok(Json(row))
}

/// PUT /sp/agents/:id — update an agent.
pub async fn sp_agents_update(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateAgentRequest>,
) -> ApiResult<Json<SpAgent>> {
    let pool = require_pg(&state)?;
    let row = sqlx::query_as::<_, SpAgent>(
        "UPDATE sp_agents SET \
            name = COALESCE($2, name), \
            description = COALESCE($3, description), \
            system_prompt = COALESCE($4, system_prompt), \
            model = COALESCE($5, model), \
            skills_config = COALESCE($6, skills_config), \
            mcp_servers_config = COALESCE($7, mcp_servers_config), \
            tools_config = COALESCE($8, tools_config), \
            template_id = COALESCE($9, template_id), \
            updated_at = now() \
         WHERE id = $1 \
         RETURNING *",
    )
    .bind(id)
    .bind(&req.name)
    .bind(&req.description)
    .bind(&req.system_prompt)
    .bind(&req.model)
    .bind(&req.skills_config)
    .bind(&req.mcp_servers_config)
    .bind(&req.tools_config)
    .bind(&req.template_id)
    .fetch_optional(pool)
    .await
    .map_err(internal_error)?
    .ok_or_else(|| not_found("Agent", id))?;
    Ok(Json(row))
}

/// DELETE /sp/agents/:id — delete an agent.
pub async fn sp_agents_delete(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> ApiResult<axum::http::StatusCode> {
    let pool = require_pg(&state)?;
    let result = sqlx::query("DELETE FROM sp_agents WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await
        .map_err(internal_error)?;
    if result.rows_affected() == 0 {
        return Err(not_found("Agent", id));
    }
    Ok(axum::http::StatusCode::NO_CONTENT)
}

// ── YAML Schema Types for Import/Export ────────────────────────

/// Envelope header for all SP YAML documents, following the project's
/// existing `apiVersion` / `kind` / `metadata` pattern from `data/rules/`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpYamlEnvelope {
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    pub kind: SpYamlKind,
    pub metadata: SpYamlMetadata,
    pub spec: serde_yaml::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SpYamlKind {
    SpAgent,
    SpPipeline,
    SpDataSource,
    SpSchedule,
    SpDelivery,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpYamlMetadata {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

// ── Per-kind spec types ─────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpAgentSpec {
    pub model: Option<String>,
    pub system_prompt: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skills_config: Vec<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mcp_servers_config: Vec<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools_config: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpPipelineSpec {
    #[serde(default)]
    pub steps: Vec<SpPipelineStepSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpPipelineStepSpec {
    pub step_order: i32,
    /// Agent referenced by name (not UUID) for portability.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_source_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parallel_group: Option<i32>,
    #[serde(default = "default_yaml_map")]
    pub input_mapping: serde_json::Value,
    #[serde(default = "default_yaml_map")]
    pub output_mapping: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpDataSourceSpec {
    pub source_type: String,
    pub config: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpScheduleSpec {
    /// Pipeline referenced by name (not UUID) for portability.
    pub pipeline_name: String,
    pub cron_expression: String,
    #[serde(default = "default_utc")]
    pub timezone: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpDeliverySpec {
    /// Schedule referenced by name (not UUID) for portability.
    pub schedule_name: String,
    pub channel: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub config: serde_json::Value,
}

fn default_yaml_map() -> serde_json::Value {
    serde_json::json!({})
}
fn default_utc() -> String {
    "UTC".to_string()
}
fn default_true() -> bool {
    true
}

// ── Export endpoint ──────────────────────────────────────────────

/// GET /sp/export — export all SP configuration as multi-document YAML.
pub async fn sp_export(
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, (axum::http::StatusCode, Json<QueryErrorResponse>)> {
    let pool = require_pg(&state)?;
    let mut docs: Vec<String> = Vec::new();

    // 1. Agents
    let agents = sqlx::query_as::<_, SpAgent>(
        "SELECT id, name, description, system_prompt, model,
                skills_config, mcp_servers_config, tools_config,
                template_id, created_at, updated_at
         FROM sp_agents ORDER BY name",
    )
    .fetch_all(pool)
    .await
    .map_err(internal_error)?;

    for a in &agents {
        let skills: Vec<serde_json::Value> =
            serde_json::from_value(a.skills_config.clone()).unwrap_or_default();
        let mcp: Vec<serde_json::Value> =
            serde_json::from_value(a.mcp_servers_config.clone()).unwrap_or_default();
        let tools: Vec<serde_json::Value> =
            serde_json::from_value(a.tools_config.clone()).unwrap_or_default();

        let doc = SpYamlEnvelope {
            api_version: "v1".into(),
            kind: SpYamlKind::SpAgent,
            metadata: SpYamlMetadata {
                name: a.name.clone(),
                description: a.description.clone(),
                tags: vec![],
            },
            spec: serde_yaml::to_value(&SpAgentSpec {
                model: Some(a.model.clone()),
                system_prompt: a.system_prompt.clone(),
                template_id: a.template_id.clone(),
                skills_config: skills,
                mcp_servers_config: mcp,
                tools_config: tools,
            })
            .map_err(|e| internal_error(e))?,
        };
        docs.push(serde_yaml::to_string(&doc).map_err(|e| internal_error(e))?);
    }

    // 2. Data sources
    let sources = sqlx::query_as::<_, SpDataSource>(
        "SELECT id, name, source_type, config_json, created_at, updated_at
         FROM sp_data_sources ORDER BY name",
    )
    .fetch_all(pool)
    .await
    .map_err(internal_error)?;

    for ds in &sources {
        let doc = SpYamlEnvelope {
            api_version: "v1".into(),
            kind: SpYamlKind::SpDataSource,
            metadata: SpYamlMetadata {
                name: ds.name.clone(),
                description: None,
                tags: vec![],
            },
            spec: serde_yaml::to_value(&SpDataSourceSpec {
                source_type: ds.source_type.clone(),
                config: ds.config_json.clone(),
            })
            .map_err(|e| internal_error(e))?,
        };
        docs.push(serde_yaml::to_string(&doc).map_err(|e| internal_error(e))?);
    }

    // 3. Pipelines (with steps, agents/sources resolved to names)
    let agent_map: HashMap<Uuid, String> = agents.iter().map(|a| (a.id, a.name.clone())).collect();
    let source_map: HashMap<Uuid, String> = sources.iter().map(|s| (s.id, s.name.clone())).collect();

    let pipelines = sqlx::query_as::<_, SpPipeline>(
        "SELECT id, name, description, created_at, updated_at FROM sp_pipelines ORDER BY name",
    )
    .fetch_all(pool)
    .await
    .map_err(internal_error)?;

    for p in &pipelines {
        let steps = sqlx::query_as::<_, SpPipelineStep>(
            "SELECT id, pipeline_id, agent_id, step_order, input_mapping,
                    output_mapping, parallel_group, data_source_id
             FROM sp_pipeline_steps WHERE pipeline_id = $1 ORDER BY step_order",
        )
        .bind(p.id)
        .fetch_all(pool)
        .await
        .map_err(internal_error)?;

        let step_specs: Vec<SpPipelineStepSpec> = steps
            .iter()
            .map(|s| SpPipelineStepSpec {
                step_order: s.step_order,
                agent_name: s.agent_id.and_then(|id| agent_map.get(&id).cloned()),
                data_source_name: s.data_source_id.and_then(|id| source_map.get(&id).cloned()),
                parallel_group: s.parallel_group,
                input_mapping: s.input_mapping.clone(),
                output_mapping: s.output_mapping.clone(),
            })
            .collect();

        let doc = SpYamlEnvelope {
            api_version: "v1".into(),
            kind: SpYamlKind::SpPipeline,
            metadata: SpYamlMetadata {
                name: p.name.clone(),
                description: p.description.clone(),
                tags: vec![],
            },
            spec: serde_yaml::to_value(&SpPipelineSpec { steps: step_specs })
                .map_err(|e| internal_error(e))?,
        };
        docs.push(serde_yaml::to_string(&doc).map_err(|e| internal_error(e))?);
    }

    // 4. Schedules (pipeline resolved to name)
    let pipeline_map: HashMap<Uuid, String> =
        pipelines.iter().map(|p| (p.id, p.name.clone())).collect();

    let schedules = sqlx::query_as::<_, SpSchedule>(
        "SELECT id, pipeline_id, cron_expression, timezone, enabled,
                last_run_at, next_run_at, created_at
         FROM sp_schedules ORDER BY created_at",
    )
    .fetch_all(pool)
    .await
    .map_err(internal_error)?;

    let mut schedule_name_map: HashMap<Uuid, String> = HashMap::new();

    for sch in &schedules {
        let sched_name = format!(
            "{}-schedule",
            pipeline_map
                .get(&sch.pipeline_id)
                .cloned()
                .unwrap_or_else(|| sch.id.to_string())
        );
        schedule_name_map.insert(sch.id, sched_name.clone());

        let doc = SpYamlEnvelope {
            api_version: "v1".into(),
            kind: SpYamlKind::SpSchedule,
            metadata: SpYamlMetadata {
                name: sched_name,
                description: None,
                tags: vec![],
            },
            spec: serde_yaml::to_value(&SpScheduleSpec {
                pipeline_name: pipeline_map
                    .get(&sch.pipeline_id)
                    .cloned()
                    .unwrap_or_else(|| sch.pipeline_id.to_string()),
                cron_expression: sch.cron_expression.clone(),
                timezone: sch.timezone.clone(),
                enabled: sch.enabled,
            })
            .map_err(|e| internal_error(e))?,
        };
        docs.push(serde_yaml::to_string(&doc).map_err(|e| internal_error(e))?);
    }

    // 5. Deliveries (schedule resolved to name)
    let deliveries = sqlx::query_as::<_, SpDelivery>(
        "SELECT id, schedule_id, channel, config_json, enabled FROM sp_deliveries ORDER BY id",
    )
    .fetch_all(pool)
    .await
    .map_err(internal_error)?;

    for d in &deliveries {
        let sched_name = d
            .schedule_id
            .and_then(|sid| schedule_name_map.get(&sid).cloned())
            .unwrap_or_else(|| "unknown".into());

        let doc = SpYamlEnvelope {
            api_version: "v1".into(),
            kind: SpYamlKind::SpDelivery,
            metadata: SpYamlMetadata {
                name: format!("{}-{}", sched_name, d.channel),
                description: None,
                tags: vec![],
            },
            spec: serde_yaml::to_value(&SpDeliverySpec {
                schedule_name: sched_name,
                channel: d.channel.clone(),
                enabled: d.enabled,
                config: d.config_json.clone(),
            })
            .map_err(|e| internal_error(e))?,
        };
        docs.push(serde_yaml::to_string(&doc).map_err(|e| internal_error(e))?);
    }

    // Join with YAML document separators
    let yaml_output = docs.join("---\n");

    Ok((
        [(header::CONTENT_TYPE, "application/x-yaml")],
        yaml_output,
    ))
}

// ── Import endpoint ──────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct SpImportRequest {
    pub yaml: String,
    /// If true, overwrite existing resources with same name. Default: false.
    #[serde(default)]
    pub overwrite: bool,
}

#[derive(Debug, Serialize)]
pub struct SpImportResult {
    pub created: Vec<SpImportedResource>,
    pub updated: Vec<SpImportedResource>,
    pub skipped: Vec<SpImportedResource>,
    pub errors: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct SpImportedResource {
    pub kind: String,
    pub name: String,
}

/// POST /sp/import — import SP configuration from multi-document YAML.
///
/// Resources are created in dependency order:
/// agents → data sources → pipelines → schedules → deliveries.
/// Names are used as keys for cross-references (not UUIDs).
pub async fn sp_import(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SpImportRequest>,
) -> ApiResult<Json<SpImportResult>> {
    let pool = require_pg(&state)?;

    // Parse multi-document YAML
    let mut envelopes: Vec<SpYamlEnvelope> = Vec::new();
    for doc_str in req.yaml.split("\n---") {
        let trimmed = doc_str.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') && !trimmed.contains("apiVersion") {
            continue;
        }
        match serde_yaml::from_str::<SpYamlEnvelope>(trimmed) {
            Ok(env) => envelopes.push(env),
            Err(e) => {
                // Skip unparseable fragments (e.g. comment-only blocks)
                info!("Skipping YAML fragment: {}", e);
            }
        }
    }

    let mut result = SpImportResult {
        created: vec![],
        updated: vec![],
        skipped: vec![],
        errors: vec![],
    };

    // Sort by dependency order
    let order = |k: &SpYamlKind| match k {
        SpYamlKind::SpAgent => 0,
        SpYamlKind::SpDataSource => 1,
        SpYamlKind::SpPipeline => 2,
        SpYamlKind::SpSchedule => 3,
        SpYamlKind::SpDelivery => 4,
    };
    envelopes.sort_by_key(|e| order(&e.kind));

    // Name → UUID maps for cross-reference resolution
    let mut agent_ids: HashMap<String, Uuid> = HashMap::new();
    let mut source_ids: HashMap<String, Uuid> = HashMap::new();
    let mut pipeline_ids: HashMap<String, Uuid> = HashMap::new();
    let mut schedule_ids: HashMap<String, Uuid> = HashMap::new();

    // Pre-populate maps from existing DB rows
    let existing_agents: Vec<(Uuid, String)> =
        sqlx::query_as("SELECT id, name FROM sp_agents")
            .fetch_all(pool)
            .await
            .map_err(internal_error)?;
    for (id, name) in &existing_agents {
        agent_ids.insert(name.clone(), *id);
    }

    let existing_sources: Vec<(Uuid, String)> =
        sqlx::query_as("SELECT id, name FROM sp_data_sources")
            .fetch_all(pool)
            .await
            .map_err(internal_error)?;
    for (id, name) in &existing_sources {
        source_ids.insert(name.clone(), *id);
    }

    let existing_pipelines: Vec<(Uuid, String)> =
        sqlx::query_as("SELECT id, name FROM sp_pipelines")
            .fetch_all(pool)
            .await
            .map_err(internal_error)?;
    for (id, name) in &existing_pipelines {
        pipeline_ids.insert(name.clone(), *id);
    }

    // Note: schedules don't have names in the DB, so we track by
    // our import-generated names in schedule_ids.

    for envelope in &envelopes {
        let name = &envelope.metadata.name;
        match envelope.kind {
            SpYamlKind::SpAgent => {
                let spec: SpAgentSpec = serde_yaml::from_value(envelope.spec.clone())
                    .map_err(|e| bad_request(format!("Invalid SpAgent spec '{}': {}", name, e)))?;

                if let Some(existing_id) = agent_ids.get(name) {
                    if req.overwrite {
                        sqlx::query(
                            "UPDATE sp_agents SET description=$1, system_prompt=$2, model=$3,
                             skills_config=$4, mcp_servers_config=$5, tools_config=$6,
                             template_id=$7, updated_at=now() WHERE id=$8",
                        )
                        .bind(&envelope.metadata.description)
                        .bind(&spec.system_prompt)
                        .bind(spec.model.as_deref().unwrap_or("claude-sonnet-4-6"))
                        .bind(serde_json::to_value(&spec.skills_config).unwrap_or_default())
                        .bind(serde_json::to_value(&spec.mcp_servers_config).unwrap_or_default())
                        .bind(serde_json::to_value(&spec.tools_config).unwrap_or_default())
                        .bind(&spec.template_id)
                        .bind(existing_id)
                        .execute(pool)
                        .await
                        .map_err(internal_error)?;
                        result.updated.push(SpImportedResource {
                            kind: "SpAgent".into(),
                            name: name.clone(),
                        });
                    } else {
                        result.skipped.push(SpImportedResource {
                            kind: "SpAgent".into(),
                            name: name.clone(),
                        });
                    }
                } else {
                    let row: (Uuid,) = sqlx::query_as(
                        "INSERT INTO sp_agents (name, description, system_prompt, model,
                         skills_config, mcp_servers_config, tools_config, template_id)
                         VALUES ($1,$2,$3,$4,$5,$6,$7,$8) RETURNING id",
                    )
                    .bind(name)
                    .bind(&envelope.metadata.description)
                    .bind(&spec.system_prompt)
                    .bind(spec.model.as_deref().unwrap_or("claude-sonnet-4-6"))
                    .bind(serde_json::to_value(&spec.skills_config).unwrap_or_default())
                    .bind(serde_json::to_value(&spec.mcp_servers_config).unwrap_or_default())
                    .bind(serde_json::to_value(&spec.tools_config).unwrap_or_default())
                    .bind(&spec.template_id)
                    .fetch_one(pool)
                    .await
                    .map_err(internal_error)?;
                    agent_ids.insert(name.clone(), row.0);
                    result.created.push(SpImportedResource {
                        kind: "SpAgent".into(),
                        name: name.clone(),
                    });
                }
            }

            SpYamlKind::SpDataSource => {
                let spec: SpDataSourceSpec =
                    serde_yaml::from_value(envelope.spec.clone()).map_err(|e| {
                        bad_request(format!("Invalid SpDataSource spec '{}': {}", name, e))
                    })?;

                if let Some(existing_id) = source_ids.get(name) {
                    if req.overwrite {
                        sqlx::query(
                            "UPDATE sp_data_sources SET source_type=$1, config_json=$2,
                             updated_at=now() WHERE id=$3",
                        )
                        .bind(&spec.source_type)
                        .bind(&spec.config)
                        .bind(existing_id)
                        .execute(pool)
                        .await
                        .map_err(internal_error)?;
                        result.updated.push(SpImportedResource {
                            kind: "SpDataSource".into(),
                            name: name.clone(),
                        });
                    } else {
                        result.skipped.push(SpImportedResource {
                            kind: "SpDataSource".into(),
                            name: name.clone(),
                        });
                    }
                } else {
                    let row: (Uuid,) = sqlx::query_as(
                        "INSERT INTO sp_data_sources (name, source_type, config_json)
                         VALUES ($1,$2,$3) RETURNING id",
                    )
                    .bind(name)
                    .bind(&spec.source_type)
                    .bind(&spec.config)
                    .fetch_one(pool)
                    .await
                    .map_err(internal_error)?;
                    source_ids.insert(name.clone(), row.0);
                    result.created.push(SpImportedResource {
                        kind: "SpDataSource".into(),
                        name: name.clone(),
                    });
                }
            }

            SpYamlKind::SpPipeline => {
                let spec: SpPipelineSpec =
                    serde_yaml::from_value(envelope.spec.clone()).map_err(|e| {
                        bad_request(format!("Invalid SpPipeline spec '{}': {}", name, e))
                    })?;

                let pipeline_id = if let Some(existing_id) = pipeline_ids.get(name) {
                    if req.overwrite {
                        sqlx::query(
                            "UPDATE sp_pipelines SET description=$1, updated_at=now() WHERE id=$2",
                        )
                        .bind(&envelope.metadata.description)
                        .bind(existing_id)
                        .execute(pool)
                        .await
                        .map_err(internal_error)?;
                        // Delete old steps before re-creating
                        sqlx::query("DELETE FROM sp_pipeline_steps WHERE pipeline_id = $1")
                            .bind(existing_id)
                            .execute(pool)
                            .await
                            .map_err(internal_error)?;
                        result.updated.push(SpImportedResource {
                            kind: "SpPipeline".into(),
                            name: name.clone(),
                        });
                        *existing_id
                    } else {
                        result.skipped.push(SpImportedResource {
                            kind: "SpPipeline".into(),
                            name: name.clone(),
                        });
                        continue;
                    }
                } else {
                    let row: (Uuid,) = sqlx::query_as(
                        "INSERT INTO sp_pipelines (name, description) VALUES ($1,$2) RETURNING id",
                    )
                    .bind(name)
                    .bind(&envelope.metadata.description)
                    .fetch_one(pool)
                    .await
                    .map_err(internal_error)?;
                    pipeline_ids.insert(name.clone(), row.0);
                    result.created.push(SpImportedResource {
                        kind: "SpPipeline".into(),
                        name: name.clone(),
                    });
                    row.0
                };

                // Insert steps with name → UUID resolution
                for step in &spec.steps {
                    let agent_id = step
                        .agent_name
                        .as_ref()
                        .and_then(|n| agent_ids.get(n).copied());
                    let ds_id = step
                        .data_source_name
                        .as_ref()
                        .and_then(|n| source_ids.get(n).copied());

                    sqlx::query(
                        "INSERT INTO sp_pipeline_steps
                         (pipeline_id, agent_id, step_order, input_mapping,
                          output_mapping, parallel_group, data_source_id)
                         VALUES ($1,$2,$3,$4,$5,$6,$7)",
                    )
                    .bind(pipeline_id)
                    .bind(agent_id)
                    .bind(step.step_order)
                    .bind(&step.input_mapping)
                    .bind(&step.output_mapping)
                    .bind(step.parallel_group)
                    .bind(ds_id)
                    .execute(pool)
                    .await
                    .map_err(internal_error)?;
                }
            }

            SpYamlKind::SpSchedule => {
                let spec: SpScheduleSpec =
                    serde_yaml::from_value(envelope.spec.clone()).map_err(|e| {
                        bad_request(format!("Invalid SpSchedule spec '{}': {}", name, e))
                    })?;

                let pip_id = pipeline_ids.get(&spec.pipeline_name).copied().ok_or_else(
                    || {
                        bad_request(format!(
                            "Schedule '{}' references unknown pipeline '{}'",
                            name, spec.pipeline_name
                        ))
                    },
                )?;

                let row: (Uuid,) = sqlx::query_as(
                    "INSERT INTO sp_schedules (pipeline_id, cron_expression, timezone, enabled)
                     VALUES ($1,$2,$3,$4) RETURNING id",
                )
                .bind(pip_id)
                .bind(&spec.cron_expression)
                .bind(&spec.timezone)
                .bind(spec.enabled)
                .fetch_one(pool)
                .await
                .map_err(internal_error)?;
                schedule_ids.insert(name.clone(), row.0);
                result.created.push(SpImportedResource {
                    kind: "SpSchedule".into(),
                    name: name.clone(),
                });
            }

            SpYamlKind::SpDelivery => {
                let spec: SpDeliverySpec =
                    serde_yaml::from_value(envelope.spec.clone()).map_err(|e| {
                        bad_request(format!("Invalid SpDelivery spec '{}': {}", name, e))
                    })?;

                let sched_id = schedule_ids.get(&spec.schedule_name).copied().ok_or_else(
                    || {
                        bad_request(format!(
                            "Delivery '{}' references unknown schedule '{}'",
                            name, spec.schedule_name
                        ))
                    },
                )?;

                sqlx::query(
                    "INSERT INTO sp_deliveries (schedule_id, channel, config_json, enabled)
                     VALUES ($1,$2,$3,$4)",
                )
                .bind(sched_id)
                .bind(&spec.channel)
                .bind(&spec.config)
                .bind(spec.enabled)
                .execute(pool)
                .await
                .map_err(internal_error)?;
                result.created.push(SpImportedResource {
                    kind: "SpDelivery".into(),
                    name: name.clone(),
                });
            }
        }
    }

    Ok(Json(result))
}
