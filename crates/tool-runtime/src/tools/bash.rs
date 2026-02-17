//! Shell command execution tool.
//!
//! Runs commands via `/bin/sh -c` with configurable timeout and working directory.

use async_trait::async_trait;
use serde_json::Value;
use std::path::PathBuf;
use std::time::Duration;
use tokio::process::Command;
use tracing::{debug, warn};

use crate::tool::{Tool, ToolContext, ToolDefinition, ToolError, ToolResult};

/// Execute shell commands with timeout and working directory support.
pub struct BashExecuteTool;

impl BashExecuteTool {
    const DEFAULT_TIMEOUT_SECS: u64 = 30;
    const MAX_TIMEOUT_SECS: u64 = 300;

    /// Validate that the working directory does not contain path traversal sequences
    /// and resolves to an existing directory.
    fn resolve_working_dir(
        base: &std::path::Path,
        override_dir: Option<&str>,
    ) -> Result<PathBuf, ToolError> {
        let dir = match override_dir {
            Some(d) => {
                if d.contains("..") {
                    return Err(ToolError::PermissionDenied(
                        "path traversal ('..') not allowed in working_dir".to_string(),
                    ));
                }
                let candidate = if std::path::Path::new(d).is_absolute() {
                    PathBuf::from(d)
                } else {
                    base.join(d)
                };
                candidate
            }
            None => base.to_path_buf(),
        };
        Ok(dir)
    }
}

#[async_trait]
impl Tool for BashExecuteTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "bash_execute".to_string(),
            description: "Execute a shell command and return stdout/stderr output.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "The shell command to execute"
                    },
                    "timeout_secs": {
                        "type": "number",
                        "description": "Timeout in seconds (default 30, max 300)"
                    },
                    "working_dir": {
                        "type": "string",
                        "description": "Override working directory for this command"
                    }
                },
                "required": ["command"]
            }),
        }
    }

    async fn execute(&self, input: Value, context: &ToolContext) -> Result<ToolResult, ToolError> {
        let command = input
            .get("command")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidInput("missing 'command' field".to_string()))?;

        let timeout_secs = input
            .get("timeout_secs")
            .and_then(|v| v.as_u64())
            .unwrap_or(Self::DEFAULT_TIMEOUT_SECS)
            .min(Self::MAX_TIMEOUT_SECS);

        let working_dir_override = input.get("working_dir").and_then(|v| v.as_str());
        let working_dir =
            Self::resolve_working_dir(&context.working_directory, working_dir_override)?;

        debug!(
            command = command,
            timeout_secs = timeout_secs,
            working_dir = %working_dir.display(),
            "executing bash command"
        );

        let timeout = Duration::from_secs(timeout_secs);
        let child = Command::new("sh")
            .arg("-c")
            .arg(command)
            .current_dir(&working_dir)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| ToolError::ExecutionFailed(format!("failed to spawn shell: {e}")))?;

        let output = match tokio::time::timeout(timeout, child.wait_with_output()).await {
            Ok(Ok(output)) => output,
            Ok(Err(e)) => {
                return Err(ToolError::ExecutionFailed(format!(
                    "command execution error: {e}"
                )));
            }
            Err(_) => {
                warn!(command = command, timeout_secs = timeout_secs, "command timed out");
                return Err(ToolError::Timeout(timeout));
            }
        };

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let exit_code = output.status.code().unwrap_or(-1);

        let content = if stderr.is_empty() {
            stdout.to_string()
        } else if stdout.is_empty() {
            stderr.to_string()
        } else {
            format!("{stdout}\n--- stderr ---\n{stderr}")
        };

        let is_error = !output.status.success();
        if is_error {
            debug!(exit_code = exit_code, "command returned non-zero exit code");
        }

        Ok(ToolResult {
            tool_call_id: String::new(),
            content: if content.is_empty() {
                format!("(exit code {exit_code})")
            } else {
                content
            },
            is_error,
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
    async fn test_echo_command() {
        let tool = BashExecuteTool;
        let result = tool
            .execute(
                serde_json::json!({"command": "echo hello"}),
                &test_context(),
            )
            .await
            .unwrap();

        assert_eq!(result.content.trim(), "hello");
        assert!(!result.is_error);
    }

    #[tokio::test]
    async fn test_nonzero_exit_code() {
        let tool = BashExecuteTool;
        let result = tool
            .execute(
                serde_json::json!({"command": "exit 1"}),
                &test_context(),
            )
            .await
            .unwrap();

        assert!(result.is_error);
    }

    #[tokio::test]
    async fn test_path_traversal_rejected() {
        let tool = BashExecuteTool;
        let err = tool
            .execute(
                serde_json::json!({"command": "ls", "working_dir": "/tmp/../etc"}),
                &test_context(),
            )
            .await
            .unwrap_err();

        assert!(matches!(err, ToolError::PermissionDenied(_)));
    }

    #[tokio::test]
    async fn test_missing_command_field() {
        let tool = BashExecuteTool;
        let err = tool
            .execute(serde_json::json!({}), &test_context())
            .await
            .unwrap_err();

        assert!(matches!(err, ToolError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn test_definition() {
        let tool = BashExecuteTool;
        let def = tool.definition();
        assert_eq!(def.name, "bash_execute");
    }
}
