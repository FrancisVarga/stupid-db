//! Delivery configuration CRUD endpoints.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use sqlx::types::Uuid;

use crate::state::AppState;

use super::common::{bad_request, internal_error, not_found, require_pg, ApiResult};

// ── Types ────────────────────────────────────────────────────────

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

// ── Handlers ─────────────────────────────────────────────────────

/// GET /sp/deliveries -- list deliveries, optionally filtered by schedule_id.
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

/// POST /sp/deliveries -- create a delivery configuration.
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

/// PUT /sp/deliveries/:id -- update a delivery configuration.
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

/// DELETE /sp/deliveries/:id -- delete a delivery configuration.
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

/// POST /sp/deliveries/:id/test -- test a delivery channel (placeholder).
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
