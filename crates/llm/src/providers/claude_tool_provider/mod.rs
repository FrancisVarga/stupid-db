//! Claude (Anthropic API) implementation of [`ToolAwareLlmProvider`].
//!
//! Supports streaming tool use via SSE, translating between the Anthropic Messages
//! API format and the provider-agnostic [`StreamEvent`] / [`ConversationMessage`] types.

mod sse;
mod streaming;
mod translate;

pub use self::streaming::ClaudeToolProvider;

#[cfg(test)]
mod tests;
