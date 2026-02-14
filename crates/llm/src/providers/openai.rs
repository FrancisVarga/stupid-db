use async_trait::async_trait;
use serde_json::json;
use tracing::debug;

use crate::provider::{LlmError, LlmProvider, Message, Role};

pub struct OpenAiProvider {
    client: reqwest::Client,
    api_key: String,
    model: String,
    base_url: String,
}

impl OpenAiProvider {
    pub fn new(api_key: String, model: String, base_url: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key,
            model,
            base_url,
        }
    }
}

#[async_trait]
impl LlmProvider for OpenAiProvider {
    async fn complete(
        &self,
        messages: Vec<Message>,
        temperature: f32,
        max_tokens: u32,
    ) -> Result<String, LlmError> {
        let url = format!("{}/v1/chat/completions", self.base_url);

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
            "temperature": temperature,
            "max_tokens": max_tokens,
        });

        debug!("OpenAI request to {}", url);

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
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
        let content = resp["choices"][0]["message"]["content"]
            .as_str()
            .ok_or_else(|| LlmError::ParseError("missing choices[0].message.content".into()))?
            .to_string();

        Ok(content)
    }
}
