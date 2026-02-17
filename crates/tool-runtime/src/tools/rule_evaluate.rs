//! Rule evaluation tool — stub implementation.
//!
//! TODO: Wire to actual rule evaluation engine once dependency injection
//! is set up. Currently returns mock evaluation results.

use async_trait::async_trait;
use serde_json::Value;
use tracing::debug;

use crate::tool::{Tool, ToolContext, ToolDefinition, ToolError, ToolResult};

/// Evaluate a named rule against provided data.
pub struct RuleEvaluateTool;

#[async_trait]
impl Tool for RuleEvaluateTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "rule_evaluate".to_string(),
            description: "Evaluate a rule against provided data and return the result.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "rule_name": {
                        "type": "string",
                        "description": "Name of the rule to evaluate"
                    },
                    "data": {
                        "type": "object",
                        "description": "Data object to evaluate the rule against"
                    }
                },
                "required": ["rule_name", "data"]
            }),
        }
    }

    async fn execute(&self, input: Value, _context: &ToolContext) -> Result<ToolResult, ToolError> {
        let rule_name = input
            .get("rule_name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidInput("missing 'rule_name' field".to_string()))?;

        let data = input
            .get("data")
            .ok_or_else(|| ToolError::InvalidInput("missing 'data' field".to_string()))?;

        if !data.is_object() {
            return Err(ToolError::InvalidInput(
                "'data' must be a JSON object".to_string(),
            ));
        }

        debug!(
            rule_name = rule_name,
            data_fields = data.as_object().map(|o| o.len()).unwrap_or(0),
            "evaluating rule (stub)"
        );

        // TODO: Wire to actual rule evaluation
        let result = serde_json::json!({
            "stub": true,
            "rule_name": rule_name,
            "message": format!("Would evaluate rule '{rule_name}' against provided data"),
            "matched": false,
            "score": 0.0,
            "details": {}
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
    async fn test_evaluate_rule() {
        let tool = RuleEvaluateTool;
        let result = tool
            .execute(
                serde_json::json!({
                    "rule_name": "high_amount",
                    "data": {"amount": 9999, "currency": "USD"}
                }),
                &test_context(),
            )
            .await
            .unwrap();

        assert!(!result.is_error);
        let parsed: Value = serde_json::from_str(&result.content).unwrap();
        assert_eq!(parsed["rule_name"], "high_amount");
        assert_eq!(parsed["stub"], true);
    }

    #[tokio::test]
    async fn test_missing_rule_name() {
        let tool = RuleEvaluateTool;
        let err = tool
            .execute(
                serde_json::json!({"data": {"x": 1}}),
                &test_context(),
            )
            .await
            .unwrap_err();

        assert!(matches!(err, ToolError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn test_missing_data() {
        let tool = RuleEvaluateTool;
        let err = tool
            .execute(
                serde_json::json!({"rule_name": "test"}),
                &test_context(),
            )
            .await
            .unwrap_err();

        assert!(matches!(err, ToolError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn test_data_must_be_object() {
        let tool = RuleEvaluateTool;
        let err = tool
            .execute(
                serde_json::json!({"rule_name": "test", "data": "not an object"}),
                &test_context(),
            )
            .await
            .unwrap_err();

        assert!(matches!(err, ToolError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn test_definition() {
        let tool = RuleEvaluateTool;
        let def = tool.definition();
        assert_eq!(def.name, "rule_evaluate");
    }
}
