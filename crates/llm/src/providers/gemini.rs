use async_trait::async_trait;
use serde_json::json;
use tracing::debug;

use crate::provider::{LlmError, LlmProvider, Message, Role};

pub struct GeminiProvider {
    client: reqwest::Client,
    api_key: String,
    model: String,
}

impl GeminiProvider {
    pub fn new(api_key: String, model: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key,
            model,
        }
    }

    /// Build the request body for the Gemini generateContent API.
    fn build_request_body(
        messages: &[Message],
        temperature: f32,
        max_tokens: u32,
    ) -> serde_json::Value {
        // Gemini uses a separate system_instruction field (like Claude)
        let system_msg = messages
            .iter()
            .find(|m| matches!(m.role, Role::System))
            .map(|m| m.content.clone());

        let contents: Vec<serde_json::Value> = messages
            .iter()
            .filter(|m| !matches!(m.role, Role::System))
            .map(|m| {
                json!({
                    "role": match m.role {
                        Role::User => "user",
                        Role::Assistant => "model",
                        Role::System => unreachable!(),
                    },
                    "parts": [{ "text": m.content }],
                })
            })
            .collect();

        let mut body = json!({
            "contents": contents,
            "generationConfig": {
                "temperature": temperature,
                "maxOutputTokens": max_tokens,
            },
        });

        if let Some(system) = system_msg {
            body["system_instruction"] = json!({
                "parts": [{ "text": system }],
            });
        }

        body
    }
}

#[async_trait]
impl LlmProvider for GeminiProvider {
    async fn complete(
        &self,
        messages: Vec<Message>,
        temperature: f32,
        max_tokens: u32,
    ) -> Result<String, LlmError> {
        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
            self.model, self.api_key,
        );

        let body = Self::build_request_body(&messages, temperature, max_tokens);

        debug!("Gemini request to model={}", self.model);

        let response = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        let status = response.status().as_u16();
        if status != 200 {
            let body = response.text().await.unwrap_or_default();
            return Err(LlmError::ApiError { status, body });
        }

        let resp: serde_json::Value = response.json().await?;
        let content = resp["candidates"][0]["content"]["parts"][0]["text"]
            .as_str()
            .ok_or_else(|| {
                LlmError::ParseError(
                    "missing candidates[0].content.parts[0].text".into(),
                )
            })?
            .to_string();

        Ok(content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{Message, Role};

    #[test]
    fn test_request_body_structure() {
        let messages = vec![
            Message { role: Role::System, content: "You are helpful.".into() },
            Message { role: Role::User, content: "Hello".into() },
            Message { role: Role::Assistant, content: "Hi there!".into() },
            Message { role: Role::User, content: "How are you?".into() },
        ];

        let body = GeminiProvider::build_request_body(&messages, 0.1, 4096);

        // System instruction is separate
        assert_eq!(
            body["system_instruction"]["parts"][0]["text"].as_str().unwrap(),
            "You are helpful.",
        );

        // Contents should not include system message
        let contents = body["contents"].as_array().unwrap();
        assert_eq!(contents.len(), 3);

        // First content is user
        assert_eq!(contents[0]["role"], "user");
        assert_eq!(contents[0]["parts"][0]["text"], "Hello");

        // Second content is model (not "assistant")
        assert_eq!(contents[1]["role"], "model");
        assert_eq!(contents[1]["parts"][0]["text"], "Hi there!");

        // Third content is user
        assert_eq!(contents[2]["role"], "user");
        assert_eq!(contents[2]["parts"][0]["text"], "How are you?");

        // Generation config
        let temp = body["generationConfig"]["temperature"].as_f64().unwrap();
        assert!((temp - 0.1).abs() < 1e-6, "temperature should be ~0.1, got {temp}");
        assert_eq!(body["generationConfig"]["maxOutputTokens"], 4096);
    }

    #[test]
    fn test_request_body_without_system() {
        let messages = vec![
            Message { role: Role::User, content: "Hello".into() },
        ];

        let body = GeminiProvider::build_request_body(&messages, 0.5, 2048);

        // No system_instruction field
        assert!(body.get("system_instruction").is_none());

        let contents = body["contents"].as_array().unwrap();
        assert_eq!(contents.len(), 1);
        assert_eq!(contents[0]["role"], "user");
    }
}
