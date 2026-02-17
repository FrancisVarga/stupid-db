//! File reading tool with line range support and binary detection.

use async_trait::async_trait;
use serde_json::Value;
use std::path::{Path, PathBuf};
use tracing::debug;

use crate::tool::{Tool, ToolContext, ToolDefinition, ToolError, ToolResult};

/// Read file contents with optional line offset and limit.
pub struct FileReadTool;

impl FileReadTool {
    /// Resolve a path and verify it stays within the working directory.
    fn safe_resolve(base: &Path, requested: &str) -> Result<PathBuf, ToolError> {
        let candidate = if Path::new(requested).is_absolute() {
            PathBuf::from(requested)
        } else {
            base.join(requested)
        };

        // Reject explicit traversal sequences before canonicalization
        if requested.contains("..") {
            return Err(ToolError::PermissionDenied(
                "path traversal ('..') not allowed".to_string(),
            ));
        }

        Ok(candidate)
    }

    /// Check if content appears to be binary (contains null bytes in first 8KB).
    fn is_binary(bytes: &[u8]) -> bool {
        let check_len = bytes.len().min(8192);
        bytes[..check_len].contains(&0)
    }
}

#[async_trait]
impl Tool for FileReadTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "file_read".to_string(),
            description: "Read file contents, optionally restricted to a line range.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "File path to read (relative to working directory or absolute)"
                    },
                    "offset": {
                        "type": "number",
                        "description": "Starting line number (1-based, default 1)"
                    },
                    "limit": {
                        "type": "number",
                        "description": "Maximum number of lines to return"
                    }
                },
                "required": ["path"]
            }),
        }
    }

    async fn execute(&self, input: Value, context: &ToolContext) -> Result<ToolResult, ToolError> {
        let path_str = input
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidInput("missing 'path' field".to_string()))?;

        let offset = input
            .get("offset")
            .and_then(|v| v.as_u64())
            .map(|v| v.max(1) as usize)
            .unwrap_or(1);

        let limit = input.get("limit").and_then(|v| v.as_u64()).map(|v| v as usize);

        let path = Self::safe_resolve(&context.working_directory, path_str)?;

        debug!(path = %path.display(), offset = offset, limit = ?limit, "reading file");

        let bytes = tokio::fs::read(&path).await.map_err(|e| {
            ToolError::ExecutionFailed(format!("failed to read '{}': {e}", path.display()))
        })?;

        if Self::is_binary(&bytes) {
            return Ok(ToolResult {
                tool_call_id: String::new(),
                content: format!("Binary file, {} bytes", bytes.len()),
                is_error: false,
            });
        }

        let content = String::from_utf8_lossy(&bytes);
        let lines: Vec<&str> = content.lines().collect();

        // offset is 1-based
        let start = (offset - 1).min(lines.len());
        let end = match limit {
            Some(l) => (start + l).min(lines.len()),
            None => lines.len(),
        };

        let selected: Vec<String> = lines[start..end]
            .iter()
            .enumerate()
            .map(|(i, line)| format!("{:>6}\t{}", start + i + 1, line))
            .collect();

        Ok(ToolResult {
            tool_call_id: String::new(),
            content: selected.join("\n"),
            is_error: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[tokio::test]
    async fn test_read_file() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        tokio::fs::write(&file_path, "line1\nline2\nline3\n")
            .await
            .unwrap();

        let tool = FileReadTool;
        let ctx = ToolContext {
            working_directory: dir.path().to_path_buf(),
        };
        let result = tool
            .execute(serde_json::json!({"path": "test.txt"}), &ctx)
            .await
            .unwrap();

        assert!(!result.is_error);
        assert!(result.content.contains("line1"));
        assert!(result.content.contains("line3"));
    }

    #[tokio::test]
    async fn test_read_with_offset_and_limit() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        tokio::fs::write(&file_path, "a\nb\nc\nd\ne\n").await.unwrap();

        let tool = FileReadTool;
        let ctx = ToolContext {
            working_directory: dir.path().to_path_buf(),
        };
        let result = tool
            .execute(
                serde_json::json!({"path": "test.txt", "offset": 2, "limit": 2}),
                &ctx,
            )
            .await
            .unwrap();

        assert!(!result.is_error);
        // Should contain lines 2 and 3 (b, c) but not a, d, e
        assert!(result.content.contains("b"));
        assert!(result.content.contains("c"));
        assert!(!result.content.contains("\ta\n"));
        assert!(!result.content.contains("\td\n"));
    }

    #[tokio::test]
    async fn test_binary_file_detection() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("binary.bin");
        tokio::fs::write(&file_path, b"hello\x00world")
            .await
            .unwrap();

        let tool = FileReadTool;
        let ctx = ToolContext {
            working_directory: dir.path().to_path_buf(),
        };
        let result = tool
            .execute(serde_json::json!({"path": "binary.bin"}), &ctx)
            .await
            .unwrap();

        assert!(!result.is_error);
        assert!(result.content.starts_with("Binary file"));
    }

    #[tokio::test]
    async fn test_path_traversal_rejected() {
        let tool = FileReadTool;
        let ctx = ToolContext {
            working_directory: PathBuf::from("/tmp"),
        };
        let err = tool
            .execute(serde_json::json!({"path": "../etc/passwd"}), &ctx)
            .await
            .unwrap_err();

        assert!(matches!(err, ToolError::PermissionDenied(_)));
    }

    #[tokio::test]
    async fn test_definition() {
        let tool = FileReadTool;
        let def = tool.definition();
        assert_eq!(def.name, "file_read");
    }
}
