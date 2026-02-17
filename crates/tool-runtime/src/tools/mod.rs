//! Built-in tool implementations for the agentic runtime.
//!
//! Tools are divided into two categories:
//! - **System tools** (`bash`, `file_read`, `file_write`): Direct OS interaction
//! - **Domain tools** (`graph_query`, `rule_list`, `rule_evaluate`): Stub implementations
//!   that will be wired to actual stores once dependency injection is set up

pub mod bash;
pub mod file_read;
pub mod file_write;
pub mod graph_query;
pub mod rule_list;
pub mod rule_evaluate;

pub use bash::BashExecuteTool;
pub use file_read::FileReadTool;
pub use file_write::FileWriteTool;
pub use graph_query::GraphQueryTool;
pub use rule_list::RuleListTool;
pub use rule_evaluate::RuleEvaluateTool;
