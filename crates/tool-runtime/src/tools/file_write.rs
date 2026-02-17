//! File writing tool with parent directory creation.

use async_trait::async_trait;
use serde_json::Value;
use std::path::{Path, PathBuf};
use tracing::debug;

use crate::tool::{Tool, ToolContext, ToolDefinition, ToolError, ToolResult};

/// Write or create files, creating parent directories as needed.
pub struct FileWriteTool;

impl FileWriteTool {
    /// Resolve a path and verify it does not contain traversal sequences.
    fn safe_resolve(base: &Path, requested: &str) -> Result<PathBuf, ToolError> {
        if requested.contains("..") {
            return Err(ToolError::PermissionDenied(
                "path traversal ('..') not allowed".to_string(),
            ));
        }

        let candidate = if Path::new(requested).is_absolute() {
            PathBuf::from(requested)
        } else {
            base.join(requested)
        };

        Ok(candidate)
    }
}

#[async_trait]
impl Tool for FileWriteTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "file_write".to_string(),
            description: "Write content to a file, creating parent directories if needed."
                .to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "File path to write (relative to working directory or absolute)"
                    },
                    "content": {
                        "type": "string",
                        "description": "Content to write to the file"
                    }
                },
                "required": ["path", "content"]
            }),
        }
    }

    async fn execute(&self, input: Value, context: &ToolContext) -> Result<ToolResult, ToolError> {
        let path_str = input
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidInput("missing 'path' field".to_string()))?;

        let content = input
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidInput("missing 'content' field".to_string()))?;

        let path = Self::safe_resolve(&context.working_directory, path_str)?;

        debug!(path = %path.display(), bytes = content.len(), "writing file");

        // Create parent directories
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| {
                ToolError::ExecutionFailed(format!(
                    "failed to create parent directories for '{}': {e}",
                    path.display()
                ))
            })?;
        }

        tokio::fs::write(&path, content).await.map_err(|e| {
            ToolError::ExecutionFailed(format!("failed to write '{}': {e}", path.display()))
        })?;

        Ok(ToolResult {
            tool_call_id: String::new(),
            content: format!("Wrote {} bytes to {}", content.len(), path.display()),
            is_error: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_write_file() {
        let dir = tempfile::tempdir().unwrap();
        let tool = FileWriteTool;
        let ctx = ToolContext {
            working_directory: dir.path().to_path_buf(),
        };

        let result = tool
            .execute(
                serde_json::json!({"path": "output.txt", "content": "hello world"}),
                &ctx,
            )
            .await
            .unwrap();

        assert!(!result.is_error);
        assert!(result.content.contains("11 bytes"));

        let written = tokio::fs::read_to_string(dir.path().join("output.txt"))
            .await
            .unwrap();
        assert_eq!(written, "hello world");
    }

    #[tokio::test]
    async fn test_write_creates_parent_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let tool = FileWriteTool;
        let ctx = ToolContext {
            working_directory: dir.path().to_path_buf(),
        };

        let result = tool
            .execute(
                serde_json::json!({"path": "sub/dir/file.txt", "content": "nested"}),
                &ctx,
            )
            .await
            .unwrap();

        assert!(!result.is_error);

        let written = tokio::fs::read_to_string(dir.path().join("sub/dir/file.txt"))
            .await
            .unwrap();
        assert_eq!(written, "nested");
    }

    #[tokio::test]
    async fn test_path_traversal_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let tool = FileWriteTool;
        let ctx = ToolContext {
            working_directory: dir.path().to_path_buf(),
        };

        let err = tool
            .execute(
                serde_json::json!({"path": "../escape.txt", "content": "bad"}),
                &ctx,
            )
            .await
            .unwrap_err();

        assert!(matches!(err, ToolError::PermissionDenied(_)));
    }

    #[tokio::test]
    async fn test_definition() {
        let tool = FileWriteTool;
        let def = tool.definition();
        assert_eq!(def.name, "file_write");
    }
}
