//! Integration tests for the Villa suggest endpoint.
//!
//! Since `stupid-server` is a binary crate (no lib.rs), we test the JSON
//! contract by defining mirror types and validating serialization roundtrips.
//! LLM-backed tests are `#[ignore]`d for CI — run with `cargo nextest run -- --ignored`.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

// ── Mirror types matching the Villa JSON contract ─────────────────

/// All 13 valid widget types (kebab-case, matching WidgetType enum in types.rs).
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

/// Allowed endpoint prefixes (must match suggest.rs ALLOWED_ENDPOINT_PREFIXES).
const ALLOWED_ENDPOINT_PREFIXES: &[&str] = &["/api/", "/stats", "/compute/"];

/// Path to the layout planner prompt template.
const LAYOUT_PLANNER_TEMPLATE_PATH: &str = "data/villa/prompts/layout-planner-system.md";

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VillaSuggestRequest {
    message: String,
    current_layout: Vec<WidgetConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    conversation_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VillaSuggestResponse {
    actions: Vec<LayoutAction>,
    explanation: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LayoutAction {
    action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    widget_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    widget: Option<WidgetConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    dimensions: Option<Dimensions>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WidgetConfig {
    id: String,
    #[serde(rename = "type")]
    widget_type: String,
    title: String,
    data_source: DataSourceConfig,
    #[serde(skip_serializing_if = "Option::is_none")]
    layout: Option<LayoutPosition>,
    #[serde(skip_serializing_if = "Option::is_none")]
    props: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DataSourceConfig {
    #[serde(rename = "type")]
    source_type: String,
    endpoint: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ws_message_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    refresh_interval: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LayoutPosition {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    min_w: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    min_h: Option<i32>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Dimensions {
    w: i32,
    h: i32,
}

// ── Helpers ───────────────────────────────────────────────────────

fn make_widget(widget_type: &str, endpoint: &str) -> WidgetConfig {
    WidgetConfig {
        id: format!("test-{widget_type}-1"),
        widget_type: widget_type.to_string(),
        title: format!("Test {widget_type}"),
        data_source: DataSourceConfig {
            source_type: "api".to_string(),
            endpoint: endpoint.to_string(),
            params: None,
            ws_message_type: None,
            refresh_interval: Some(30),
        },
        layout: Some(LayoutPosition {
            x: 0,
            y: 0,
            w: 3,
            h: 2,
            min_w: None,
            min_h: None,
        }),
        props: None,
    }
}

fn make_add_action(widget: WidgetConfig) -> LayoutAction {
    LayoutAction {
        action: "add".to_string(),
        widget_id: None,
        widget: Some(widget),
        dimensions: None,
    }
}

fn make_remove_action(widget_id: &str) -> LayoutAction {
    LayoutAction {
        action: "remove".to_string(),
        widget_id: Some(widget_id.to_string()),
        widget: None,
        dimensions: None,
    }
}

fn make_resize_action(widget_id: &str, w: i32, h: i32) -> LayoutAction {
    LayoutAction {
        action: "resize".to_string(),
        widget_id: Some(widget_id.to_string()),
        widget: None,
        dimensions: Some(Dimensions { w, h }),
    }
}

/// Resolve a path relative to the cargo workspace root.
fn workspace_root() -> std::path::PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    Path::new(manifest_dir)
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

// ── Unit tests (always run) ──────────────────────────────────────

#[test]
fn test_parse_valid_suggest_response() {
    let response = VillaSuggestResponse {
        actions: vec![make_add_action(make_widget("stats-card", "/api/stats"))],
        explanation: "Added a stats card.".to_string(),
    };

    // Serialize → deserialize roundtrip proves JSON contract works
    let json = serde_json::to_string(&response).unwrap();
    let parsed: VillaSuggestResponse = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed.actions.len(), 1);
    assert_eq!(parsed.actions[0].action, "add");
    let widget = parsed.actions[0].widget.as_ref().unwrap();
    assert_eq!(widget.widget_type, "stats-card");
    assert_eq!(widget.data_source.endpoint, "/api/stats");
    assert_eq!(parsed.explanation, "Added a stats card.");
}

#[test]
fn test_parse_invalid_response() {
    // Garbage input should fail deserialization
    let garbage = "this is not json at all!!!";
    let result = serde_json::from_str::<VillaSuggestResponse>(garbage);
    assert!(result.is_err(), "garbage input must not parse as valid response");

    // Valid JSON but wrong shape should also fail
    let wrong_shape = r#"{"foo": "bar"}"#;
    let result = serde_json::from_str::<VillaSuggestResponse>(wrong_shape);
    assert!(result.is_err(), "wrong-shape JSON must not parse");
}

#[test]
fn test_validate_known_widget_types() {
    // All 13 widget types should serialize correctly in kebab-case
    for &wt in ALLOWED_WIDGET_TYPES {
        let widget = make_widget(wt, "/api/stats");
        let json = serde_json::to_string(&widget).unwrap();
        let parsed: WidgetConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(
            parsed.widget_type, wt,
            "widget type '{wt}' must roundtrip correctly"
        );
    }

    assert_eq!(
        ALLOWED_WIDGET_TYPES.len(),
        13,
        "must have exactly 13 widget types"
    );
}

#[test]
fn test_reject_unknown_widget_type() {
    // Build a response with an unknown widget type — validation should catch this
    let widget = make_widget("pie-chart", "/api/stats");
    assert!(
        !ALLOWED_WIDGET_TYPES.contains(&widget.widget_type.as_str()),
        "pie-chart must NOT be in the allowed widget types"
    );

    let widget = make_widget("radar-chart", "/api/stats");
    assert!(
        !ALLOWED_WIDGET_TYPES.contains(&widget.widget_type.as_str()),
        "radar-chart must NOT be in the allowed widget types"
    );
}

#[test]
fn test_reject_external_endpoint() {
    // Endpoints pointing to external servers must be rejected by validation
    let external_endpoints = [
        "https://evil.com/steal",
        "http://attacker.io/data",
        "//cdn.example.com/widget.js",
        "ftp://files.example.com/data",
    ];

    for ep in &external_endpoints {
        let valid = ALLOWED_ENDPOINT_PREFIXES
            .iter()
            .any(|prefix| ep.starts_with(prefix));
        assert!(
            !valid,
            "external endpoint '{ep}' must be rejected by prefix validation"
        );
    }

    // Valid internal endpoints should pass
    let valid_endpoints = ["/api/stats", "/api/graph/nodes", "/stats", "/compute/trends"];
    for ep in &valid_endpoints {
        let valid = ALLOWED_ENDPOINT_PREFIXES
            .iter()
            .any(|prefix| ep.starts_with(prefix));
        assert!(
            valid,
            "internal endpoint '{ep}' must be accepted by prefix validation"
        );
    }
}

#[test]
fn test_template_has_placeholders() {
    let template_path = workspace_root().join(LAYOUT_PLANNER_TEMPLATE_PATH);
    assert!(
        template_path.exists(),
        "layout planner template must exist at {}",
        template_path.display()
    );

    let content = std::fs::read_to_string(&template_path)
        .unwrap_or_else(|e| panic!("failed to read template: {e}"));

    assert!(
        content.contains("<<<data_summary>>>"),
        "template must contain <<<data_summary>>> placeholder"
    );
    assert!(
        content.contains("<<<current_layout>>>"),
        "template must contain <<<current_layout>>> placeholder"
    );
}

#[test]
fn test_template_has_all_widget_types() {
    let template_path = workspace_root().join(LAYOUT_PLANNER_TEMPLATE_PATH);
    let content = std::fs::read_to_string(&template_path)
        .unwrap_or_else(|e| panic!("failed to read template: {e}"));

    for &wt in ALLOWED_WIDGET_TYPES {
        assert!(
            content.contains(wt),
            "template must document widget type '{wt}'"
        );
    }

    // Template should mention the suggest_layout tool
    assert!(
        content.contains("suggest_layout"),
        "template must reference the suggest_layout tool"
    );
}

#[test]
fn test_request_serialization_roundtrip() {
    let request = VillaSuggestRequest {
        message: "Show me system stats".to_string(),
        current_layout: vec![make_widget("stats-card", "/api/stats")],
        conversation_id: Some("conv-123".to_string()),
    };

    let json = serde_json::to_string(&request).unwrap();
    let parsed: VillaSuggestRequest = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed.message, "Show me system stats");
    assert_eq!(parsed.current_layout.len(), 1);
    assert_eq!(parsed.conversation_id.as_deref(), Some("conv-123"));
}

#[test]
fn test_all_action_kinds_serialize() {
    let actions = vec![
        make_add_action(make_widget("stats-card", "/api/stats")),
        make_remove_action("system-stats-1"),
        make_resize_action("entity-graph-1", 9, 7),
        LayoutAction {
            action: "move".to_string(),
            widget_id: Some("event-trends-1".to_string()),
            widget: None,
            dimensions: None,
        },
    ];

    let response = VillaSuggestResponse {
        actions,
        explanation: "Mixed actions.".to_string(),
    };

    let json = serde_json::to_string(&response).unwrap();
    let parsed: VillaSuggestResponse = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed.actions.len(), 4);
    assert_eq!(parsed.actions[0].action, "add");
    assert_eq!(parsed.actions[1].action, "remove");
    assert_eq!(parsed.actions[2].action, "resize");
    assert_eq!(parsed.actions[3].action, "move");

    // Verify resize dimensions
    let dims = parsed.actions[2].dimensions.as_ref().unwrap();
    assert_eq!(dims.w, 9);
    assert_eq!(dims.h, 7);
}

#[test]
fn test_prompt_test_cases_yaml_loadable() {
    let yaml_path = workspace_root().join("data/villa/tests/prompt-test-cases.yaml");
    assert!(
        yaml_path.exists(),
        "prompt test cases YAML must exist at {}",
        yaml_path.display()
    );

    let content = std::fs::read_to_string(&yaml_path)
        .unwrap_or_else(|e| panic!("failed to read test cases YAML: {e}"));

    // Parse as generic YAML to validate structure
    let doc: serde_yaml::Value =
        serde_yaml::from_str(&content).expect("test cases must be valid YAML");

    let test_cases = doc["test_cases"]
        .as_sequence()
        .expect("test_cases must be a sequence");

    // Should have at least 10 test cases (5 happy + 3 ambiguous + 3 remove + 4 edge)
    assert!(
        test_cases.len() >= 10,
        "expected at least 10 test cases, found {}",
        test_cases.len()
    );

    // Each test case must have id, input, and expected_actions
    for (i, tc) in test_cases.iter().enumerate() {
        assert!(
            tc["id"].as_str().is_some(),
            "test case {i} must have a string 'id'"
        );
        assert!(
            tc["input"].as_str().is_some(),
            "test case {i} must have a string 'input'"
        );
        assert!(
            tc["expected_actions"].is_sequence() || tc["expected_actions"].is_null(),
            "test case {i} must have 'expected_actions' as array or empty"
        );
    }
}

// ── Integration tests (require LLM + reqwest, gated behind feature) ──
//
// These tests require:
//   1. `reqwest` in [dev-dependencies] with feature "llm-integration"
//   2. A running stupid-server with LLM provider configured
//
// Run with:
//   cargo nextest run -p stupid-server --features llm-integration -- --ignored
//
// To set up:
//   1. Add to Cargo.toml [dev-dependencies]: reqwest = { version = "0.12", features = ["json"] }
//   2. Add to Cargo.toml [features]: llm-integration = ["dep:reqwest"]
//   3. Start the server: cargo run -p stupid-server
//   4. Run: cargo nextest run -p stupid-server --features llm-integration -- --ignored

#[cfg(feature = "llm-integration")]
mod llm_tests {
    use super::*;

    fn llm_test_base_url() -> String {
        std::env::var("VILLA_TEST_BASE_URL")
            .unwrap_or_else(|_| "http://localhost:3001".to_string())
    }

    #[ignore]
    #[tokio::test]
    async fn test_llm_show_me_stats() {
        let client = reqwest::Client::new();
        let base_url = llm_test_base_url();

        let request = VillaSuggestRequest {
            message: "Show me system stats".to_string(),
            current_layout: vec![],
            conversation_id: None,
        };

        let resp = client
            .post(format!("{base_url}/api/villa/suggest"))
            .json(&request)
            .send()
            .await
            .expect("failed to reach server");

        if resp.status().as_u16() == 503 {
            eprintln!("LLM not configured, skipping test");
            return;
        }

        assert!(
            resp.status().is_success(),
            "suggest returned {}",
            resp.status()
        );
        let body: VillaSuggestResponse = resp.json().await.unwrap();
        assert!(!body.actions.is_empty(), "expected at least one action");
        assert_eq!(body.actions[0].action, "add");
        let widget = body.actions[0]
            .widget
            .as_ref()
            .expect("add action must have widget");
        assert_eq!(widget.widget_type, "stats-card");
    }

    #[ignore]
    #[tokio::test]
    async fn test_llm_show_me_everything() {
        let client = reqwest::Client::new();
        let base_url = llm_test_base_url();

        let request = VillaSuggestRequest {
            message: "Show me everything".to_string(),
            current_layout: vec![],
            conversation_id: None,
        };

        let resp = client
            .post(format!("{base_url}/api/villa/suggest"))
            .json(&request)
            .send()
            .await
            .expect("failed to reach server");

        if resp.status().as_u16() == 503 {
            eprintln!("LLM not configured, skipping test");
            return;
        }

        assert!(resp.status().is_success());
        let body: VillaSuggestResponse = resp.json().await.unwrap();
        assert!(
            body.actions.len() >= 2 && body.actions.len() <= 3,
            "expected 2-3 add actions, got {}",
            body.actions.len()
        );
        for action in &body.actions {
            assert_eq!(action.action, "add");
        }
    }

    #[ignore]
    #[tokio::test]
    async fn test_llm_remove_widget() {
        let client = reqwest::Client::new();
        let base_url = llm_test_base_url();

        let request = VillaSuggestRequest {
            message: "Remove the stats card".to_string(),
            current_layout: vec![make_widget("stats-card", "/api/stats")],
            conversation_id: None,
        };

        let resp = client
            .post(format!("{base_url}/api/villa/suggest"))
            .json(&request)
            .send()
            .await
            .expect("failed to reach server");

        if resp.status().as_u16() == 503 {
            eprintln!("LLM not configured, skipping test");
            return;
        }

        assert!(resp.status().is_success());
        let body: VillaSuggestResponse = resp.json().await.unwrap();
        assert!(!body.actions.is_empty(), "expected remove action");
        assert_eq!(body.actions[0].action, "remove");
    }

    #[ignore]
    #[tokio::test]
    async fn test_llm_unsupported_pie_chart() {
        let client = reqwest::Client::new();
        let base_url = llm_test_base_url();

        let request = VillaSuggestRequest {
            message: "Add a pie chart".to_string(),
            current_layout: vec![],
            conversation_id: None,
        };

        let resp = client
            .post(format!("{base_url}/api/villa/suggest"))
            .json(&request)
            .send()
            .await
            .expect("failed to reach server");

        if resp.status().as_u16() == 503 {
            eprintln!("LLM not configured, skipping test");
            return;
        }

        assert!(resp.status().is_success());
        let body: VillaSuggestResponse = resp.json().await.unwrap();
        assert!(
            body.actions.is_empty(),
            "pie chart is not supported — expected empty actions, got {}",
            body.actions.len()
        );
        assert!(
            !body.explanation.is_empty(),
            "explanation should explain why no actions were taken"
        );
    }

    #[ignore]
    #[tokio::test]
    async fn test_llm_resize_graph() {
        let client = reqwest::Client::new();
        let base_url = llm_test_base_url();

        let request = VillaSuggestRequest {
            message: "Make the graph bigger".to_string(),
            current_layout: vec![make_widget("force-graph", "/api/graph/edges")],
            conversation_id: None,
        };

        let resp = client
            .post(format!("{base_url}/api/villa/suggest"))
            .json(&request)
            .send()
            .await
            .expect("failed to reach server");

        if resp.status().as_u16() == 503 {
            eprintln!("LLM not configured, skipping test");
            return;
        }

        assert!(resp.status().is_success());
        let body: VillaSuggestResponse = resp.json().await.unwrap();
        assert!(!body.actions.is_empty(), "expected resize action");
        assert_eq!(body.actions[0].action, "resize");
    }
}
