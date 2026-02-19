//! Schedule CRUD endpoints with cron expression validation.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use sqlx::types::Uuid;

use crate::state::AppState;

use super::common::{bad_request, internal_error, not_found, require_pg, ApiResult};

// ── Types ────────────────────────────────────────────────────────

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
pub(crate) fn validate_cron(expr: &str) -> Result<(), String> {
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

// ── Handlers ─────────────────────────────────────────────────────

/// GET /sp/schedules -- list all schedules (with pipeline name).
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

/// POST /sp/schedules -- create a new schedule.
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

/// PUT /sp/schedules/:id -- update a schedule (enable/disable, change cron, change timezone).
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

/// DELETE /sp/schedules/:id -- delete a schedule.
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
