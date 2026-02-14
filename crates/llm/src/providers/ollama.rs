use async_trait::async_trait;
use serde_json::json;
use tracing::debug;

use crate::provider::{LlmError, LlmProvider, Message, Role};

pub struct OllamaProvider {
    client: reqwest::Client,
    url: String,
    model: String,
}

impl OllamaProvider {
    pub fn new(url: String, model: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            url,
            model,
        }
    }
}

#[async_trait]
impl LlmProvider for OllamaProvider {
    async fn complete(
        &self,
        messages: Vec<Message>,
        temperature: f32,
        _max_tokens: u32,
    ) -> Result<String, LlmError> {
        let url = format!("{}/api/chat", self.url);

        let api_messages: Vec<serde_json::Value> = messages
            .iter()
            .map(|m| {
                json!({
                    "role": match m.role {
                        Role::System => "system",
                        Role::User => "user",
                        Role::Assistant => "assistant",
                    },
                    "content": m.content,
                })
            })
            .collect();

        let body = json!({
            "model": self.model,
            "messages": api_messages,
            "stream": false,
            "options": {
                "temperature": temperature,
            },
        });

        debug!("Ollama request to {}", url);

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
        let content = resp["message"]["content"]
            .as_str()
            .ok_or_else(|| LlmError::ParseError("missing message.content".into()))?
            .to_string();

        Ok(content)
    }
}
