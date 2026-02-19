//! Prompt template CRUD endpoints.
//!
//! SRP: manage externalized LLM prompt templates stored in PostgreSQL.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::state::AppState;

use super::QueryErrorResponse;

// ── Response types ─────────────────────────────────────────────

#[derive(Serialize, utoipa::ToSchema)]
pub struct PromptSummary {
    pub name: String,
    pub description: String,
    pub placeholders: Vec<String>,
    pub updated_at: String,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct PromptDetail {
    pub name: String,
    pub content: String,
    pub description: String,
    pub placeholders: Vec<String>,
    pub updated_at: String,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct UpdatePromptRequest {
    pub content: String,
    #[serde(default)]
    pub description: Option<String>,
}

// ── Endpoints ─────────────────────────────────────────────────

/// List all prompts (without full content).
pub async fn prompts_list(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<PromptSummary>>, (axum::http::StatusCode, Json<QueryErrorResponse>)> {
    let pool = state.pg_pool.as_ref().ok_or_else(|| {
        (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            Json(QueryErrorResponse {
                error: "PostgreSQL not configured".to_string(),
            }),
        )
    })?;

    let rows = sqlx::query_as::<_, (String, String, Vec<String>, chrono::DateTime<chrono::Utc>)>(
        "SELECT name, description, placeholders, updated_at FROM prompts ORDER BY name",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| {
        (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(QueryErrorResponse {
                error: format!("Failed to list prompts: {e}"),
            }),
        )
    })?;

    let prompts = rows
        .into_iter()
        .map(|(name, description, placeholders, updated_at)| PromptSummary {
            name,
            description,
            placeholders,
            updated_at: updated_at.to_rfc3339(),
        })
        .collect();

    Ok(Json(prompts))
}

/// Get a single prompt by name (includes full content).
pub async fn prompts_get(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<PromptDetail>, (axum::http::StatusCode, Json<QueryErrorResponse>)> {
    let pool = state.pg_pool.as_ref().ok_or_else(|| {
        (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            Json(QueryErrorResponse {
                error: "PostgreSQL not configured".to_string(),
            }),
        )
    })?;

    let row = sqlx::query_as::<_, (String, String, String, Vec<String>, chrono::DateTime<chrono::Utc>)>(
        "SELECT name, content, description, placeholders, updated_at FROM prompts WHERE name = $1",
    )
    .bind(&name)
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(QueryErrorResponse {
                error: format!("Failed to fetch prompt: {e}"),
            }),
        )
    })?;

    match row {
        Some((name, content, description, placeholders, updated_at)) => Ok(Json(PromptDetail {
            name,
            content,
            description,
            placeholders,
            updated_at: updated_at.to_rfc3339(),
        })),
        None => Err((
            axum::http::StatusCode::NOT_FOUND,
            Json(QueryErrorResponse {
                error: format!("Prompt not found: {name}"),
            }),
        )),
    }
}

/// Update a prompt's content (writes to Postgres only, not back to file).
pub async fn prompts_update(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(req): Json<UpdatePromptRequest>,
) -> Result<Json<PromptDetail>, (axum::http::StatusCode, Json<QueryErrorResponse>)> {
    let pool = state.pg_pool.as_ref().ok_or_else(|| {
        (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            Json(QueryErrorResponse {
                error: "PostgreSQL not configured".to_string(),
            }),
        )
    })?;

    // Parse placeholders from the new content.
    let placeholders = extract_placeholders(&req.content);

    let row = sqlx::query_as::<_, (String, String, String, Vec<String>, chrono::DateTime<chrono::Utc>)>(
        "UPDATE prompts SET content = $2, placeholders = $3, description = COALESCE($4, description), updated_at = NOW()
         WHERE name = $1
         RETURNING name, content, description, placeholders, updated_at",
    )
    .bind(&name)
    .bind(&req.content)
    .bind(&placeholders)
    .bind(&req.description)
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(QueryErrorResponse {
                error: format!("Failed to update prompt: {e}"),
            }),
        )
    })?;

    match row {
        Some((name, content, description, placeholders, updated_at)) => Ok(Json(PromptDetail {
            name,
            content,
            description,
            placeholders,
            updated_at: updated_at.to_rfc3339(),
        })),
        None => Err((
            axum::http::StatusCode::NOT_FOUND,
            Json(QueryErrorResponse {
                error: format!("Prompt not found: {name}"),
            }),
        )),
    }
}

// ── Helpers ───────────────────────────────────────────────────

/// Extract `<<<placeholder>>>` patterns from template content.
pub(crate) fn extract_placeholders(content: &str) -> Vec<String> {
    let mut placeholders = Vec::new();
    let mut remaining = content;
    while let Some(start) = remaining.find("<<<") {
        let after = &remaining[start + 3..];
        if let Some(end) = after.find(">>>") {
            let name = &after[..end];
            if !name.is_empty() && !placeholders.contains(&name.to_string()) {
                placeholders.push(name.to_string());
            }
            remaining = &after[end + 3..];
        } else {
            break;
        }
    }
    placeholders
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_placeholders() {
        let content = "Hello <<<name>>>, your <<<schema>>> is ready. Also <<<name>>> again.";
        let result = extract_placeholders(content);
        assert_eq!(result, vec!["name", "schema"]);
    }

    #[test]
    fn test_extract_placeholders_empty() {
        let result = extract_placeholders("No placeholders here");
        assert!(result.is_empty());
    }

    #[test]
    fn test_extract_placeholders_partial() {
        let result = extract_placeholders("<<<incomplete no closing");
        assert!(result.is_empty());
    }
}
