use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};

use super::traits::{Embedder, EmbeddingError};

/// OpenAI-compatible embedding backend.
pub struct OpenAiEmbedder {
    client: Client,
    api_key: String,
    model: String,
    base_url: String,
    dimensions: usize,
}

impl OpenAiEmbedder {
    pub fn new(
        api_key: String,
        model: String,
        base_url: Option<String>,
        dimensions: usize,
    ) -> Self {
        Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(120))
                .build()
                .unwrap_or_else(|_| Client::new()),
            api_key,
            model,
            base_url: base_url.unwrap_or_else(|| "https://api.openai.com".to_string()),
            dimensions,
        }
    }
}

#[derive(Serialize)]
struct EmbedRequest {
    model: String,
    input: Vec<String>,
}

#[derive(Deserialize)]
struct EmbedResponse {
    data: Vec<EmbedItem>,
}

#[derive(Deserialize)]
struct EmbedItem {
    embedding: Vec<f32>,
    index: usize,
}

#[async_trait]
impl Embedder for OpenAiEmbedder {
    async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, EmbeddingError> {
        let request = EmbedRequest {
            model: self.model.clone(),
            input: texts.iter().map(|t| t.to_string()).collect(),
        };

        let response = self
            .client
            .post(format!("{}/v1/embeddings", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(EmbeddingError::Api(format!("{status}: {body}")));
        }

        let mut resp: EmbedResponse = response.json().await?;

        // Sort by index to maintain input order.
        resp.data.sort_by_key(|item| item.index);

        let embeddings: Vec<Vec<f32>> = resp.data.into_iter().map(|item| item.embedding).collect();

        // Validate dimensions on first vector.
        if let Some(first) = embeddings.first() {
            if first.len() != self.dimensions {
                return Err(EmbeddingError::DimensionMismatch {
                    expected: self.dimensions,
                    actual: first.len(),
                });
            }
        }

        Ok(embeddings)
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }
}
