---
name: agent-system
description: LLM-powered agentic loop with tool execution, permission system, and streaming
---

# Agent System

## Overview

The agent system provides an LLM-powered tool execution loop with streaming support, permission management, and MCP protocol integration.

## AgenticLoop

The core orchestrator in `crates/tool-runtime/src/runtime.rs`:

```rust
pub struct AgenticLoop {
    provider: Arc<dyn ToolAwareLlmProvider>,
    registry: Arc<ToolRegistry>,
    permission_checker: Arc<dyn PermissionChecker>,
    max_iterations: usize,   // Default: 10
    temperature: f32,
    max_tokens: u32,
}
```

### Execution Flow

1. Add user message to conversation
2. For each iteration (0..max_iterations):
   a. Stream LLM response with tool definitions
   b. Collect events: TextDelta, ToolCalls
   c. If StopReason::ToolUse → execute tools
   d. Send ToolExecutionStart + ToolExecutionResult events
   e. Add results to conversation, continue
3. Send final MessageEnd event

## Tool Interface

```rust
pub trait Tool: Send + Sync {
    fn definition(&self) -> ToolDefinition;
    async fn execute(&self, input: Value, context: &ToolContext)
        -> Result<ToolResult, ToolError>;
}
```

### Built-in Tools (6)

| Tool | Type | Purpose |
|------|------|---------|
| BashExecuteTool | System | Execute shell commands |
| FileReadTool | System | Read file contents |
| FileWriteTool | System | Write/append to files |
| GraphQueryTool | Domain | Query knowledge graph |
| RuleListTool | Domain | List anomaly rules |
| RuleEvaluateTool | Domain | Evaluate rule on data |

## Permission System

```rust
pub enum PermissionLevel {
    AutoApprove,           // Execute immediately
    RequireConfirmation,   // Ask user first
    Deny,                  // Block execution
}
```

Configured per-tool in the CLI via `~/.stupid-cli.toml`.

## LLM Provider Chain

```
LlmProvider (basic completion, crates/llm)
  → LlmProviderAdapter (implements SimpleLlmProvider)
    → LlmProviderBridge (non-streaming → streaming adapter)
      → ToolAwareLlmProvider (used by AgenticLoop)
```

## Streaming Events

```rust
pub enum StreamEvent {
    TextDelta { text },
    ToolCallStart { id, name },
    ToolCallDelta { id, arguments_delta },
    ToolCallEnd { id },
    ToolExecutionStart { id, name },
    ToolExecutionResult { id, content, is_error },
    MessageEnd { stop_reason },
    Error { message },
}
```

## MCP Server

Exposes ToolRegistry over JSON-RPC 2.0:

```rust
pub struct McpServer {
    registry: ToolRegistry,
    // Supports stdio and channel transports
}
```

## Adding a New Tool

1. Implement `Tool` trait in `crates/tool-runtime/src/tools/`
2. Define `ToolDefinition` with JSON Schema for input
3. Register in `ToolRegistry` during server/CLI initialization
4. Add permission default in policy
5. Test with mock provider
