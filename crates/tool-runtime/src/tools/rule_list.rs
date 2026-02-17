//! Rule listing tool — stub implementation.
//!
//! TODO: Wire to actual RuleLoader via Arc<RuleLoader> once dependency injection
//! is set up. Currently returns mock data for schema validation and testing.

use async_trait::async_trait;
use serde_json::Value;
use tracing::debug;

use crate::tool::{Tool, ToolContext, ToolDefinition, ToolError, ToolResult};

/// List rules from the rule catalog, optionally filtered by kind.
pub struct RuleListTool;

/// Valid rule kinds matching the rules-unification system.
const VALID_KINDS: &[&str] = &[
    "anomaly", "schema", "feature", "scoring", "trend", "pattern",
];

#[async_trait]
impl Tool for RuleListTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "rule_list".to_string(),
            description: "List loaded rules, optionally filtered by kind.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "kind": {
                        "type": "string",
                        "enum": ["anomaly", "schema", "feature", "scoring", "trend", "pattern"],
                        "description": "Filter by rule kind"
                    }
                }
            }),
        }
    }

    async fn execute(&self, input: Value, _context: &ToolContext) -> Result<ToolResult, ToolError> {
        let kind = input.get("kind").and_then(|v| v.as_str());

        if let Some(k) = kind {
            if !VALID_KINDS.contains(&k) {
                return Err(ToolError::InvalidInput(format!(
                    "unknown kind '{k}', expected one of: {}",
                    VALID_KINDS.join(", ")
                )));
            }
        }

        debug!(kind = kind, "listing rules (stub)");

        // TODO: Wire to actual RuleLoader via Arc<RuleLoader>
        let result = serde_json::json!({
            "stub": true,
            "kind_filter": kind,
            "message": format!(
                "Would return rules{} from RuleLoader",
                kind.map(|k| format!(" of kind '{k}'")).unwrap_or_default()
            ),
            "rules": []
        });

        Ok(ToolResult {
            tool_call_id: String::new(),
            content: serde_json::to_string_pretty(&result)
                .map_err(|e| ToolError::ExecutionFailed(format!("JSON serialization failed: {e}")))?,
            is_error: false,
        })
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

    #[tokio::test]
    async fn test_list_all_rules() {
        let tool = RuleListTool;
        let result = tool
            .execute(serde_json::json!({}), &test_context())
            .await
            .unwrap();

        assert!(!result.is_error);
        let parsed: Value = serde_json::from_str(&result.content).unwrap();
        assert_eq!(parsed["stub"], true);
        assert!(parsed["kind_filter"].is_null());
    }

    #[tokio::test]
    async fn test_list_by_kind() {
        let tool = RuleListTool;
        let result = tool
            .execute(
                serde_json::json!({"kind": "anomaly"}),
                &test_context(),
            )
            .await
            .unwrap();

        assert!(!result.is_error);
        let parsed: Value = serde_json::from_str(&result.content).unwrap();
        assert_eq!(parsed["kind_filter"], "anomaly");
    }

    #[tokio::test]
    async fn test_invalid_kind() {
        let tool = RuleListTool;
        let err = tool
            .execute(
                serde_json::json!({"kind": "invalid"}),
                &test_context(),
            )
            .await
            .unwrap_err();

        assert!(matches!(err, ToolError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn test_definition() {
        let tool = RuleListTool;
        let def = tool.definition();
        assert_eq!(def.name, "rule_list");
    }
}
