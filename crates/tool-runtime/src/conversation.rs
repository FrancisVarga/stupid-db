use crate::tool::{ToolCall, ToolResult};
use serde::{Deserialize, Serialize};

/// A message in the conversation history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConversationMessage {
    /// User's text input
    User(String),
    /// Assistant's response (may contain text and/or tool calls)
    Assistant(AssistantContent),
    /// Result of a tool execution
    ToolResult(ToolResult),
}

/// Content from the assistant that can contain mixed text and tool calls.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantContent {
    /// Text blocks in the response
    pub text: Option<String>,
    /// Tool calls requested by the assistant
    pub tool_calls: Vec<ToolCall>,
}

/// Manages conversation history with context window awareness.
pub struct Conversation {
    messages: Vec<ConversationMessage>,
    /// Maximum approximate token count before truncation
    max_tokens: usize,
    /// System prompt (always retained)
    system_prompt: Option<String>,
}

impl Conversation {
    pub fn new(max_tokens: usize) -> Self {
        Self {
            messages: Vec::new(),
            max_tokens,
            system_prompt: None,
        }
    }

    pub fn with_system_prompt(mut self, prompt: String) -> Self {
        self.system_prompt = Some(prompt);
        self
    }

    pub fn system_prompt(&self) -> Option<&str> {
        self.system_prompt.as_deref()
    }

    pub fn add_user_message(&mut self, text: String) {
        self.messages.push(ConversationMessage::User(text));
        self.maybe_truncate();
    }

    pub fn add_assistant_response(&mut self, content: AssistantContent) {
        self.messages.push(ConversationMessage::Assistant(content));
        self.maybe_truncate();
    }

    pub fn add_tool_result(&mut self, result: ToolResult) {
        self.messages.push(ConversationMessage::ToolResult(result));
    }

    pub fn messages(&self) -> &[ConversationMessage] {
        &self.messages
    }

    pub fn clear(&mut self) {
        self.messages.clear();
    }

    /// Approximate token count using character count / 4 heuristic.
    pub fn approximate_tokens(&self) -> usize {
        let char_count: usize = self
            .messages
            .iter()
            .map(|m| match m {
                ConversationMessage::User(text) => text.len(),
                ConversationMessage::Assistant(content) => {
                    content.text.as_ref().map_or(0, |t| t.len())
                        + content
                            .tool_calls
                            .iter()
                            .map(|tc| tc.input.to_string().len())
                            .sum::<usize>()
                }
                ConversationMessage::ToolResult(result) => result.content.len(),
            })
            .sum();
        char_count / 4
    }

    /// Drop oldest messages (keeping system prompt) when over token limit.
    fn maybe_truncate(&mut self) {
        while self.approximate_tokens() > self.max_tokens && self.messages.len() > 2 {
            // Keep at least the last 2 messages (current turn)
            self.messages.remove(0);
        }
    }
}

impl Default for Conversation {
    fn default() -> Self {
        Self::new(100_000) // 100k tokens default
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool::ToolCall;

    #[test]
    fn test_conversation_basic() {
        let mut conv = Conversation::new(100_000);
        conv.add_user_message("Hello".to_string());
        conv.add_assistant_response(AssistantContent {
            text: Some("Hi there!".to_string()),
            tool_calls: vec![],
        });

        assert_eq!(conv.messages().len(), 2);
    }

    #[test]
    fn test_conversation_with_tool_calls() {
        let mut conv = Conversation::new(100_000);
        conv.add_user_message("List files".to_string());
        conv.add_assistant_response(AssistantContent {
            text: None,
            tool_calls: vec![ToolCall {
                id: "call_1".to_string(),
                name: "bash_execute".to_string(),
                input: serde_json::json!({"command": "ls -la"}),
            }],
        });
        conv.add_tool_result(ToolResult {
            tool_call_id: "call_1".to_string(),
            content: "file1.txt\nfile2.txt".to_string(),
            is_error: false,
        });

        assert_eq!(conv.messages().len(), 3);
    }

    #[test]
    fn test_conversation_truncation() {
        let mut conv = Conversation::new(10); // Very small limit (~40 chars)
        for i in 0..100 {
            // Each message ~50 chars → ~12 tokens, well over the 10 limit
            conv.add_user_message(format!("This is a longer message number {} with padding text", i));
        }
        // Should have truncated significantly (keeps minimum 2)
        assert!(conv.messages().len() <= 4);
    }

    #[test]
    fn test_serialization() {
        let mut conv = Conversation::new(100_000);
        conv.add_user_message("test".to_string());
        let msg = &conv.messages()[0];
        let json = serde_json::to_string(msg).unwrap();
        let _roundtrip: ConversationMessage = serde_json::from_str(&json).unwrap();
    }
}
