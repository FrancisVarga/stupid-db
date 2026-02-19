//! YAML schema types for Stille Post configuration import/export.
//!
//! Follows the project's existing `apiVersion` / `kind` / `metadata`
//! envelope pattern from `data/rules/`.

use serde::{Deserialize, Serialize};

// ── Envelope ─────────────────────────────────────────────────────

/// Envelope header for all SP YAML documents.
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

// ── Import/export result types ──────────────────────────────────

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
