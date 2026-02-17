//! Graph query tool — stub implementation.
//!
//! TODO: Wire to actual GraphStore via Arc<GraphStore> once dependency injection
//! is set up. Currently returns mock data for schema validation and testing.

use async_trait::async_trait;
use serde_json::Value;
use tracing::debug;

use crate::tool::{Tool, ToolContext, ToolDefinition, ToolError, ToolResult};

/// Query the in-memory property graph for entities, relationships, or search.
pub struct GraphQueryTool;

#[async_trait]
impl Tool for GraphQueryTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "graph_query".to_string(),
            description:
                "Query the knowledge graph for entities, relationships, or text search results."
                    .to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query_type": {
                        "type": "string",
                        "enum": ["entities", "relationships", "search"],
                        "description": "Type of graph query to perform"
                    },
                    "entity_type": {
                        "type": "string",
                        "description": "Filter by entity type (e.g., 'person', 'organization')"
                    },
                    "name": {
                        "type": "string",
                        "description": "Entity name or search query"
                    }
                },
                "required": ["query_type"]
            }),
        }
    }

    async fn execute(&self, input: Value, _context: &ToolContext) -> Result<ToolResult, ToolError> {
        let query_type = input
            .get("query_type")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidInput("missing 'query_type' field".to_string()))?;

        let entity_type = input.get("entity_type").and_then(|v| v.as_str());
        let name = input.get("name").and_then(|v| v.as_str());

        debug!(
            query_type = query_type,
            entity_type = entity_type,
            name = name,
            "executing graph query (stub)"
        );

        // TODO: Wire to actual GraphStore via Arc<GraphStore>
        let result = match query_type {
            "entities" => {
                let filter_msg = entity_type
                    .map(|t| format!(" of type '{t}'"))
                    .unwrap_or_default();
                serde_json::json!({
                    "query_type": "entities",
                    "stub": true,
                    "message": format!("Would return entities{filter_msg} from GraphStore"),
                    "results": []
                })
            }
            "relationships" => {
                let target_msg = name
                    .map(|n| format!(" for entity '{n}'"))
                    .unwrap_or_default();
                serde_json::json!({
                    "query_type": "relationships",
                    "stub": true,
                    "message": format!("Would return relationships{target_msg} from GraphStore"),
                    "results": []
                })
            }
            "search" => {
                let query_msg = name.unwrap_or("(empty)");
                serde_json::json!({
                    "query_type": "search",
                    "stub": true,
                    "message": format!("Would search graph for '{query_msg}'"),
                    "results": []
                })
            }
            other => {
                return Err(ToolError::InvalidInput(format!(
                    "unknown query_type '{other}', expected one of: entities, relationships, search"
                )));
            }
        };

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
    async fn test_entities_query() {
        let tool = GraphQueryTool;
        let result = tool
            .execute(
                serde_json::json!({"query_type": "entities", "entity_type": "person"}),
                &test_context(),
            )
            .await
            .unwrap();

        assert!(!result.is_error);
        let parsed: Value = serde_json::from_str(&result.content).unwrap();
        assert_eq!(parsed["query_type"], "entities");
        assert_eq!(parsed["stub"], true);
    }

    #[tokio::test]
    async fn test_search_query() {
        let tool = GraphQueryTool;
        let result = tool
            .execute(
                serde_json::json!({"query_type": "search", "name": "test query"}),
                &test_context(),
            )
            .await
            .unwrap();

        assert!(!result.is_error);
        let parsed: Value = serde_json::from_str(&result.content).unwrap();
        assert_eq!(parsed["query_type"], "search");
    }

    #[tokio::test]
    async fn test_invalid_query_type() {
        let tool = GraphQueryTool;
        let err = tool
            .execute(
                serde_json::json!({"query_type": "invalid"}),
                &test_context(),
            )
            .await
            .unwrap_err();

        assert!(matches!(err, ToolError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn test_definition() {
        let tool = GraphQueryTool;
        let def = tool.definition();
        assert_eq!(def.name, "graph_query");
    }
}
