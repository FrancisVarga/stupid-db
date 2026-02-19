//! Villa layout engine — request/response types mirroring the TypeScript schema.

use serde::{Deserialize, Serialize};

// ── Widget type ─────────────────────────────────────────────────

/// Discriminant for dashboard widget kinds.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "kebab-case")]
pub enum WidgetType {
    StatsCard,
    TimeSeries,
    DataTable,
    ForceGraph,
    BarChart,
    ScatterPlot,
    Heatmap,
    Sankey,
    Treemap,
    AnomalyChart,
    TrendChart,
    PageRank,
    DegreeChart,
}

// ── Data source ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DataSourceConfig {
    #[serde(rename = "type")]
    pub source_type: DataSourceType,
    pub endpoint: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<std::collections::HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ws_message_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_interval: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum DataSourceType {
    Api,
    Websocket,
}

// ── Layout position ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LayoutPosition {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_w: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_h: Option<i32>,
}

// ── Widget config ───────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WidgetConfig {
    pub id: String,
    #[serde(rename = "type")]
    pub widget_type: WidgetType,
    pub title: String,
    pub data_source: DataSourceConfig,
    pub layout: LayoutPosition,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Object)]
    pub props: Option<serde_json::Value>,
}

// ── Layout action ───────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LayoutAction {
    pub action: LayoutActionKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub widget_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub widget: Option<WidgetConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dimensions: Option<Dimensions>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum LayoutActionKind {
    Add,
    Remove,
    Resize,
    Move,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct Dimensions {
    pub w: i32,
    pub h: i32,
}

// ── Request / response ──────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct VillaSuggestRequest {
    pub message: String,
    pub current_layout: Vec<WidgetConfig>,
    #[allow(dead_code)] // Will be used for conversation history tracking
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conversation_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct VillaSuggestResponse {
    pub actions: Vec<LayoutAction>,
    pub explanation: String,
}

#[allow(dead_code)] // Used by future chat history endpoints
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    pub id: String,
    pub role: ChatRole,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actions: Option<Vec<LayoutAction>>,
    pub timestamp: u64,
}

#[allow(dead_code)] // Used by future chat history endpoints
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum ChatRole {
    User,
    Assistant,
}
