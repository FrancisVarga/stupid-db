//! Rule-builder agent tools — 5 tools that bridge the AI agent to the rules system.
//!
//! These tools call the server's REST API endpoints for rule management,
//! keeping the tool-runtime decoupled from the rules crate.
//!
//! Tools:
//! - `list_rules`: GET /rules → rule summaries
//! - `get_rule_yaml`: GET /rules/{id}/yaml → raw YAML
//! - `validate_rule`: POST /rules/validate → validation result
//! - `dry_run_rule`: POST /rules/dry-run → evaluation result
//! - `save_rule`: POST /rules → persist new rule

use async_trait::async_trait;
use serde_json::Value;
use tracing::debug;

use crate::tool::{Tool, ToolContext, ToolDefinition, ToolError, ToolResult};

/// Default server base URL for rule API calls.
const DEFAULT_API_BASE: &str = "http://localhost:3088";

/// Resolve the API base URL from environment or fallback to default.
fn api_base() -> String {
    std::env::var("STUPID_API_BASE").unwrap_or_else(|_| DEFAULT_API_BASE.to_string())
}

/// Shared HTTP client (lazy-initialized per call; reqwest::Client is cheap to clone).
fn http_client() -> reqwest::Client {
    reqwest::Client::new()
}

// ── list_rules ──────────────────────────────────────────────────────

/// List all rules with their ID, name, kind, enabled status, and description.
pub struct ListRulesTool;

#[async_trait]
impl Tool for ListRulesTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "list_rules".to_string(),
            description: "List all rules in the system. Returns an array of rule summaries with id, name, kind, enabled status, and description.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "kind": {
                        "type": "string",
                        "enum": ["AnomalyRule", "EntitySchema", "FeatureConfig", "ScoringConfig", "TrendConfig", "PatternConfig"],
                        "description": "Optional filter by rule kind"
                    }
                }
            }),
        }
    }

    async fn execute(&self, input: Value, _context: &ToolContext) -> Result<ToolResult, ToolError> {
        let kind = input.get("kind").and_then(|v| v.as_str());
        debug!(kind = kind, "list_rules");

        let mut url = format!("{}/rules", api_base());
        if let Some(k) = kind {
            url.push_str(&format!("?kind={}", k));
        }

        let response = http_client()
            .get(&url)
            .send()
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("HTTP request failed: {e}")))?;

        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("Failed to read response: {e}")))?;

        if !status.is_success() {
            return Ok(ToolResult {
                tool_call_id: String::new(),
                content: format!("Error listing rules (HTTP {}): {}", status, body),
                is_error: true,
            });
        }

        Ok(ToolResult {
            tool_call_id: String::new(),
            content: body,
            is_error: false,
        })
    }
}

// ── get_rule_yaml ───────────────────────────────────────────────────

/// Retrieve the full YAML source of a specific rule by ID.
pub struct GetRuleYamlTool;

#[async_trait]
impl Tool for GetRuleYamlTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "get_rule_yaml".to_string(),
            description: "Get the full YAML source of a rule by its ID. Returns the raw YAML string preserving comments and formatting.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "rule_id": {
                        "type": "string",
                        "description": "The rule's metadata.id value"
                    }
                },
                "required": ["rule_id"]
            }),
        }
    }

    async fn execute(&self, input: Value, _context: &ToolContext) -> Result<ToolResult, ToolError> {
        let rule_id = input
            .get("rule_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidInput("missing 'rule_id' field".to_string()))?;

        debug!(rule_id = rule_id, "get_rule_yaml");

        let url = format!("{}/rules/{}/yaml", api_base(), rule_id);
        let response = http_client()
            .get(&url)
            .send()
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("HTTP request failed: {e}")))?;

        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("Failed to read response: {e}")))?;

        if status.as_u16() == 404 {
            return Ok(ToolResult {
                tool_call_id: String::new(),
                content: format!("Rule '{}' not found", rule_id),
                is_error: true,
            });
        }

        if !status.is_success() {
            return Ok(ToolResult {
                tool_call_id: String::new(),
                content: format!("Error fetching rule (HTTP {}): {}", status, body),
                is_error: true,
            });
        }

        Ok(ToolResult {
            tool_call_id: String::new(),
            content: body,
            is_error: false,
        })
    }
}

// ── validate_rule ───────────────────────────────────────────────────

/// Validate YAML without saving — checks structure, types, and consistency.
pub struct ValidateRuleTool;

#[async_trait]
impl Tool for ValidateRuleTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "validate_rule".to_string(),
            description: "Validate a rule's YAML without saving it. Checks structure, types, references, and consistency. Returns whether the YAML is valid and any errors found.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "yaml": {
                        "type": "string",
                        "description": "The complete YAML rule definition to validate"
                    }
                },
                "required": ["yaml"]
            }),
        }
    }

    async fn execute(&self, input: Value, _context: &ToolContext) -> Result<ToolResult, ToolError> {
        let yaml = input
            .get("yaml")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidInput("missing 'yaml' field".to_string()))?;

        debug!(yaml_len = yaml.len(), "validate_rule");

        let url = format!("{}/rules/validate", api_base());
        let response = http_client()
            .post(&url)
            .header("content-type", "application/yaml")
            .body(yaml.to_string())
            .send()
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("HTTP request failed: {e}")))?;

        let body = response
            .text()
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("Failed to read response: {e}")))?;

        // Both 200 (valid) and 400 (invalid) are normal results — not tool errors.
        Ok(ToolResult {
            tool_call_id: String::new(),
            content: body,
            is_error: false,
        })
    }
}

// ── dry_run_rule ────────────────────────────────────────────────────

/// Test a rule against live data without persisting it.
pub struct DryRunRuleTool;

#[async_trait]
impl Tool for DryRunRuleTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "dry_run_rule".to_string(),
            description: "Dry-run a rule against live data without saving it. For AnomalyRules, evaluates against current entity data and returns matches found, evaluation time, and match details. Other rule kinds get validation-only results.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "yaml": {
                        "type": "string",
                        "description": "The complete YAML rule definition to test"
                    }
                },
                "required": ["yaml"]
            }),
        }
    }

    async fn execute(&self, input: Value, _context: &ToolContext) -> Result<ToolResult, ToolError> {
        let yaml = input
            .get("yaml")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidInput("missing 'yaml' field".to_string()))?;

        debug!(yaml_len = yaml.len(), "dry_run_rule");

        let url = format!("{}/rules/dry-run", api_base());
        let response = http_client()
            .post(&url)
            .header("content-type", "application/yaml")
            .body(yaml.to_string())
            .send()
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("HTTP request failed: {e}")))?;

        let body = response
            .text()
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("Failed to read response: {e}")))?;

        Ok(ToolResult {
            tool_call_id: String::new(),
            content: body,
            is_error: false,
        })
    }
}

// ── save_rule ───────────────────────────────────────────────────────

/// Persist a validated rule to the rules directory.
pub struct SaveRuleTool;

#[async_trait]
impl Tool for SaveRuleTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "save_rule".to_string(),
            description: "Save a rule to the rules directory. The YAML is validated, then persisted to disk. Returns success status with the rule ID and file path. Fails if a rule with the same ID already exists.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "yaml": {
                        "type": "string",
                        "description": "The complete YAML rule definition to save"
                    }
                },
                "required": ["yaml"]
            }),
        }
    }

    async fn execute(&self, input: Value, _context: &ToolContext) -> Result<ToolResult, ToolError> {
        let yaml = input
            .get("yaml")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidInput("missing 'yaml' field".to_string()))?;

        debug!(yaml_len = yaml.len(), "save_rule");

        let url = format!("{}/rules", api_base());
        let response = http_client()
            .post(&url)
            .header("content-type", "application/yaml")
            .body(yaml.to_string())
            .send()
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("HTTP request failed: {e}")))?;

        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|e| ToolError::ExecutionFailed(format!("Failed to read response: {e}")))?;

        if status.as_u16() == 201 {
            // Parse the response to build a clean save confirmation.
            let parsed: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
            let id = parsed["metadata"]["id"].as_str().unwrap_or("unknown");
            let kind = parsed["kind"].as_str().unwrap_or("unknown");
            let result = serde_json::json!({
                "success": true,
                "id": id,
                "kind": kind,
                "message": format!("Rule '{}' saved successfully", id),
            });
            Ok(ToolResult {
                tool_call_id: String::new(),
                content: serde_json::to_string_pretty(&result)
                    .unwrap_or_else(|_| body),
                is_error: false,
            })
        } else if status.as_u16() == 409 {
            let result = serde_json::json!({
                "success": false,
                "error": "Rule with this ID already exists. Use a different ID or update the existing rule.",
                "details": body,
            });
            Ok(ToolResult {
                tool_call_id: String::new(),
                content: serde_json::to_string_pretty(&result)
                    .unwrap_or_else(|_| body),
                is_error: true,
            })
        } else {
            Ok(ToolResult {
                tool_call_id: String::new(),
                content: format!("Error saving rule (HTTP {}): {}", status, body),
                is_error: true,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn test_context() -> ToolContext {
        ToolContext {
            working_directory: PathBuf::from("/tmp"),
        }
    }

    #[test]
    fn test_list_rules_definition() {
        let tool = ListRulesTool;
        let def = tool.definition();
        assert_eq!(def.name, "list_rules");
        assert!(def.description.contains("List all rules"));
    }

    #[test]
    fn test_get_rule_yaml_definition() {
        let tool = GetRuleYamlTool;
        let def = tool.definition();
        assert_eq!(def.name, "get_rule_yaml");
        assert!(def.description.contains("YAML"));
    }

    #[test]
    fn test_validate_rule_definition() {
        let tool = ValidateRuleTool;
        let def = tool.definition();
        assert_eq!(def.name, "validate_rule");
        assert!(def.description.contains("Validate"));
    }

    #[test]
    fn test_dry_run_rule_definition() {
        let tool = DryRunRuleTool;
        let def = tool.definition();
        assert_eq!(def.name, "dry_run_rule");
        assert!(def.description.contains("Dry-run"));
    }

    #[test]
    fn test_save_rule_definition() {
        let tool = SaveRuleTool;
        let def = tool.definition();
        assert_eq!(def.name, "save_rule");
        assert!(def.description.contains("Save"));
    }

    #[tokio::test]
    async fn test_get_rule_yaml_missing_input() {
        let tool = GetRuleYamlTool;
        let err = tool
            .execute(serde_json::json!({}), &test_context())
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn test_validate_rule_missing_input() {
        let tool = ValidateRuleTool;
        let err = tool
            .execute(serde_json::json!({}), &test_context())
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn test_dry_run_rule_missing_input() {
        let tool = DryRunRuleTool;
        let err = tool
            .execute(serde_json::json!({}), &test_context())
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn test_save_rule_missing_input() {
        let tool = SaveRuleTool;
        let err = tool
            .execute(serde_json::json!({}), &test_context())
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::InvalidInput(_)));
    }
}
