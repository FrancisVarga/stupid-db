//! Pipeline CRUD endpoints.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use sqlx::types::Uuid;

use crate::state::AppState;

use super::common::{default_json_object, internal_error, not_found, require_pg, ApiResult};

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

// ── Handlers ─────────────────────────────────────────────────────

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
        // No step changes -- return existing steps
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
