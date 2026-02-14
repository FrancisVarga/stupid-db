use async_trait::async_trait;
use serde_json::json;
use tracing::debug;

use crate::provider::{LlmError, LlmProvider, Message, Role};

pub struct ClaudeProvider {
    client: reqwest::Client,
    api_key: String,
    model: String,
}

impl ClaudeProvider {
    pub fn new(api_key: String, model: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key,
            model,
        }
    }
}

#[async_trait]
impl LlmProvider for ClaudeProvider {
    async fn complete(
        &self,
        messages: Vec<Message>,
        temperature: f32,
        max_tokens: u32,
    ) -> Result<String, LlmError> {
        let url = "https://api.anthropic.com/v1/messages";

        // Claude API uses separate system parameter
        let system_msg = messages
            .iter()
            .find(|m| matches!(m.role, Role::System))
            .map(|m| m.content.clone());

        let api_messages: Vec<serde_json::Value> = messages
            .iter()
            .filter(|m| !matches!(m.role, Role::System))
            .map(|m| {
                json!({
                    "role": match m.role {
                        Role::User => "user",
                        Role::Assistant => "assistant",
                        Role::System => unreachable!(),
                    },
                    "content": m.content,
                })
            })
            .collect();

        let mut body = json!({
            "model": self.model,
            "messages": api_messages,
            "temperature": temperature,
            "max_tokens": max_tokens,
        });

        if let Some(system) = system_msg {
            body["system"] = json!(system);
        }

        debug!("Claude request to {}", url);

        let response = self
            .client
            .post(url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
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
        let content = resp["content"][0]["text"]
            .as_str()
            .ok_or_else(|| LlmError::ParseError("missing content[0].text".into()))?
            .to_string();

        Ok(content)
    }
}
