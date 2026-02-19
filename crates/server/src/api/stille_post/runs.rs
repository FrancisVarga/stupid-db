//! Run and report CRUD endpoints.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use sqlx::types::Uuid;

use crate::state::AppState;

use super::common::{internal_error, not_found, require_pg, ApiResult};

// ── Types ────────────────────────────────────────────────────────

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

// ── Handlers ─────────────────────────────────────────────────────

/// GET /sp/runs -- list runs with optional pipeline_id and status filters.
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

/// GET /sp/runs/:id -- get a run with all step results.
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

/// POST /sp/runs -- trigger a manual pipeline run (creates a pending record).
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

/// DELETE /sp/runs/:id -- cancel or delete a pipeline run.
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

/// GET /sp/reports -- list all reports (newest first).
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

/// GET /sp/reports/:id -- get a single report.
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
