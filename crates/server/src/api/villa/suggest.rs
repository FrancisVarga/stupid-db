//! POST /api/villa/suggest — LLM-driven layout suggestion endpoint.
//!
//! Accepts a user message + current layout, calls the LLM with a tool-use prompt,
//! and returns validated `LayoutAction`s for the frontend to apply.

use std::path::Path;
use std::sync::Arc;

use axum::extract::State;
use axum::Json;
use tracing::{debug, error, info};

use crate::api::QueryErrorResponse;
use crate::state::AppState;

use super::types::{
    LayoutAction, VillaSuggestRequest, VillaSuggestResponse, WidgetConfig, WidgetType,
};

/// Path to the layout planner system prompt template (relative to workspace root).
const LAYOUT_PLANNER_TEMPLATE_PATH: &str = "data/villa/prompts/layout-planner-system.md";

/// Placeholder replaced with a brief data summary.
const DATA_SUMMARY_PLACEHOLDER: &str = "<<<data_summary>>>";

/// Placeholder replaced with the current layout JSON.
const CURRENT_LAYOUT_PLACEHOLDER: &str = "<<<current_layout>>>";

/// Allowed widget types for validation.
const ALLOWED_WIDGET_TYPES: &[&str] = &[
    "stats-card",
    "time-series",
    "data-table",
    "force-graph",
    "bar-chart",
    "scatter-plot",
    "heatmap",
    "sankey",
    "treemap",
    "anomaly-chart",
    "trend-chart",
    "page-rank",
    "degree-chart",
];

/// Allowed endpoint prefixes for data sources.
const ALLOWED_ENDPOINT_PREFIXES: &[&str] = &["/api/", "/stats", "/compute/", "/graph/"];

/// POST /api/villa/suggest
///
/// Sends the user's message and current layout to the LLM, which returns
/// layout actions (add/remove/resize/move widgets) as structured JSON.
#[utoipa::path(
    post,
    path = "/api/villa/suggest",
    tag = "Villa",
    request_body = VillaSuggestRequest,
    responses(
        (status = 200, description = "Layout suggestions", body = VillaSuggestResponse),
        (status = 503, description = "LLM not configured or data not ready", body = QueryErrorResponse),
        (status = 500, description = "LLM or parse error", body = QueryErrorResponse),
    )
)]
pub async fn suggest(
    State(state): State<Arc<AppState>>,
    Json(req): Json<VillaSuggestRequest>,
) -> Result<Json<VillaSuggestResponse>, (axum::http::StatusCode, Json<QueryErrorResponse>)> {
    // 1. Ensure LLM query generator is available (it owns the provider).
    let qg = state.query_generator.as_ref().ok_or_else(|| {
        error!("Villa suggest called but LLM query generator is not configured");
        (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            Json(QueryErrorResponse {
                error: "LLM provider not configured. Set LLM_PROVIDER and API keys.".into(),
            }),
        )
    })?;

    // 2. Load system prompt template from disk.
    let template = load_template(LAYOUT_PLANNER_TEMPLATE_PATH).map_err(|e| {
        error!("Failed to load layout planner template: {e}");
        (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(QueryErrorResponse { error: e }),
        )
    })?;

    // 3. Build data summary from graph stats.
    let data_summary = build_data_summary(&state).await;

    // 4. Serialize current layout to JSON for the placeholder.
    let current_layout_json = serde_json::to_string_pretty(&req.current_layout)
        .unwrap_or_else(|_| "[]".to_string());

    // 5. Fill placeholders in the template.
    let system_prompt = template
        .replace(DATA_SUMMARY_PLACEHOLDER, &data_summary)
        .replace(CURRENT_LAYOUT_PLACEHOLDER, &current_layout_json);

    info!(
        message = %req.message,
        widgets = req.current_layout.len(),
        "Villa suggest: sending layout request to LLM"
    );

    // 6. Call the LLM via QueryGenerator's underlying provider.
    let messages = vec![
        stupid_llm::Message {
            role: stupid_llm::Role::System,
            content: system_prompt,
        },
        stupid_llm::Message {
            role: stupid_llm::Role::User,
            content: req.message.clone(),
        },
    ];

    let response = qg
        .provider()
        .complete(messages, 0.4, 2048)
        .await
        .map_err(|e| {
            error!("LLM call failed for villa suggest: {e}");
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(QueryErrorResponse {
                    error: format!("LLM error: {e}"),
                }),
            )
        })?;

    debug!("Villa suggest LLM response: {response}");

    // 7. Parse the tool call response into actions + explanation.
    let (actions, explanation) = parse_suggest_response(&response);

    // 8. Validate each action.
    let valid_actions: Vec<LayoutAction> = actions
        .into_iter()
        .filter(|a| validate_action(a))
        .collect();

    info!(
        valid = valid_actions.len(),
        "Villa suggest: returning validated actions"
    );

    Ok(Json(VillaSuggestResponse {
        actions: valid_actions,
        explanation,
    }))
}

/// Build a brief data summary string from the current graph state.
async fn build_data_summary(state: &AppState) -> String {
    let graph = state.graph.read().await;
    let stats = graph.stats();
    format!(
        "System has {} nodes and {} edges in the knowledge graph.",
        stats.node_count, stats.edge_count
    )
}

/// Load the prompt template from disk, validating that both placeholders exist.
fn load_template(path: &str) -> Result<String, String> {
    let path = Path::new(path);
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("failed to read prompt template at {}: {e}", path.display()))?;

    if !content.contains(DATA_SUMMARY_PLACEHOLDER) {
        return Err(format!(
            "template at {} missing '{DATA_SUMMARY_PLACEHOLDER}' placeholder",
            path.display()
        ));
    }
    if !content.contains(CURRENT_LAYOUT_PLACEHOLDER) {
        return Err(format!(
            "template at {} missing '{CURRENT_LAYOUT_PLACEHOLDER}' placeholder",
            path.display()
        ));
    }

    Ok(content)
}

/// Parse the LLM response into (actions, explanation).
///
/// The LLM is instructed to call `suggest_layout` with `{actions: [...], explanation: "..."}`.
/// We extract the JSON (handling markdown code blocks) and deserialize.
/// On failure, returns empty actions with a helpful explanation.
fn parse_suggest_response(response: &str) -> (Vec<LayoutAction>, String) {
    let json_str = extract_json(response);

    // Try to parse as the tool call format: { actions: [...], explanation: "..." }
    #[derive(serde::Deserialize)]
    struct ToolCallResponse {
        actions: Vec<LayoutAction>,
        explanation: String,
    }

    match serde_json::from_str::<ToolCallResponse>(json_str) {
        Ok(parsed) => (parsed.actions, parsed.explanation),
        Err(e) => {
            error!("Failed to parse LLM suggest response: {e}");
            debug!("Raw LLM response: {response}");
            (
                vec![],
                "I couldn't generate layout suggestions for that request. \
                 Could you try rephrasing?"
                    .to_string(),
            )
        }
    }
}

/// Validate a single layout action.
///
/// - For "add" actions: widget type must be in the allowed list, and
///   the data source endpoint must start with an allowed prefix.
/// - Other actions (remove, resize, move) are always valid.
fn validate_action(action: &LayoutAction) -> bool {
    if let Some(ref widget) = action.widget {
        if !validate_widget(widget) {
            return false;
        }
    }
    true
}

/// Validate widget type and data source endpoint.
fn validate_widget(widget: &WidgetConfig) -> bool {
    // Validate widget type
    let type_str = match &widget.widget_type {
        WidgetType::StatsCard => "stats-card",
        WidgetType::TimeSeries => "time-series",
        WidgetType::DataTable => "data-table",
        WidgetType::ForceGraph => "force-graph",
        WidgetType::BarChart => "bar-chart",
        WidgetType::ScatterPlot => "scatter-plot",
        WidgetType::Heatmap => "heatmap",
        WidgetType::Sankey => "sankey",
        WidgetType::Treemap => "treemap",
        WidgetType::AnomalyChart => "anomaly-chart",
        WidgetType::TrendChart => "trend-chart",
        WidgetType::PageRank => "page-rank",
        WidgetType::DegreeChart => "degree-chart",
    };
    if !ALLOWED_WIDGET_TYPES.contains(&type_str) {
        error!(widget_type = type_str, "Invalid widget type");
        return false;
    }

    // Validate endpoint prefix
    let endpoint = &widget.data_source.endpoint;
    let valid_endpoint = ALLOWED_ENDPOINT_PREFIXES
        .iter()
        .any(|prefix| endpoint.starts_with(prefix));
    if !valid_endpoint {
        error!(endpoint = %endpoint, "Invalid data source endpoint prefix");
        return false;
    }

    true
}

/// Extract JSON from an LLM response, handling markdown code blocks.
fn extract_json(response: &str) -> &str {
    let trimmed = response.trim();

    // Handle ```json ... ``` blocks
    if let Some(start) = trimmed.find("```json") {
        let json_start = start + 7;
        if let Some(end) = trimmed[json_start..].find("```") {
            return trimmed[json_start..json_start + end].trim();
        }
    }

    // Handle ``` ... ``` blocks
    if let Some(start) = trimmed.find("```") {
        let json_start = start + 3;
        let after_tick = &trimmed[json_start..];
        let content_start = after_tick.find('\n').map_or(0, |n| n + 1);
        if let Some(end) = after_tick[content_start..].find("```") {
            return after_tick[content_start..content_start + end].trim();
        }
    }

    // Try raw JSON (starts with {)
    if let Some(start) = trimmed.find('{') {
        if let Some(end) = trimmed.rfind('}') {
            return &trimmed[start..=end];
        }
    }

    trimmed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_json_from_code_block() {
        let input = "Here:\n```json\n{\"actions\": [], \"explanation\": \"test\"}\n```";
        assert_eq!(
            extract_json(input),
            r#"{"actions": [], "explanation": "test"}"#
        );
    }

    #[test]
    fn extract_json_raw() {
        let input = r#"{"actions": [], "explanation": "test"}"#;
        assert_eq!(extract_json(input), input);
    }

    #[test]
    fn parse_valid_response() {
        let input = r#"{"actions": [{"action": "add", "widget": {"id": "test-1", "type": "stats-card", "title": "Test", "dataSource": {"type": "api", "endpoint": "/api/stats"}, "layout": {"x": 0, "y": 0, "w": 3, "h": 2}}}], "explanation": "Added a stats card."}"#;
        let (actions, explanation) = parse_suggest_response(input);
        assert_eq!(actions.len(), 1);
        assert_eq!(explanation, "Added a stats card.");
    }

    #[test]
    fn parse_invalid_response_returns_empty() {
        let (actions, explanation) = parse_suggest_response("not json at all");
        assert!(actions.is_empty());
        assert!(explanation.contains("couldn't generate"));
    }

    #[test]
    fn validate_valid_widget() {
        let widget = WidgetConfig {
            id: "test-1".to_string(),
            widget_type: WidgetType::StatsCard,
            title: "Test".to_string(),
            data_source: super::super::types::DataSourceConfig {
                source_type: super::super::types::DataSourceType::Api,
                endpoint: "/api/stats".to_string(),
                params: None,
                ws_message_type: None,
                refresh_interval: Some(30),
            },
            layout: super::super::types::LayoutPosition {
                x: 0,
                y: 0,
                w: 3,
                h: 2,
                min_w: None,
                min_h: None,
            },
            props: None,
        };
        assert!(validate_widget(&widget));
    }

    #[test]
    fn reject_invalid_endpoint() {
        let widget = WidgetConfig {
            id: "test-1".to_string(),
            widget_type: WidgetType::StatsCard,
            title: "Test".to_string(),
            data_source: super::super::types::DataSourceConfig {
                source_type: super::super::types::DataSourceType::Api,
                endpoint: "https://evil.com/steal".to_string(),
                params: None,
                ws_message_type: None,
                refresh_interval: None,
            },
            layout: super::super::types::LayoutPosition {
                x: 0,
                y: 0,
                w: 3,
                h: 2,
                min_w: None,
                min_h: None,
            },
            props: None,
        };
        assert!(!validate_widget(&widget));
    }

    /// Resolve the template path relative to the workspace root.
    fn workspace_template_path() -> String {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let workspace_root = std::path::Path::new(manifest_dir)
            .parent()
            .unwrap()
            .parent()
            .unwrap();
        workspace_root
            .join(LAYOUT_PLANNER_TEMPLATE_PATH)
            .to_string_lossy()
            .into_owned()
    }

    #[test]
    fn template_file_exists_and_has_placeholders() {
        let path = workspace_template_path();
        let template = load_template(&path)
            .expect("layout planner template must exist");
        assert!(template.contains("suggest_layout"));
        assert!(template.contains("stats-card"));
    }
}
