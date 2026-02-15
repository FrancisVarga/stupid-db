use super::traits::{Embedder, EmbeddingError};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};

/// Embedder backed by a local Ollama instance.
pub struct OllamaEmbedder {
    client: Client,
    url: String,
    model: String,
    dimensions: usize,
}

impl OllamaEmbedder {
    pub fn new(url: String, model: String, dimensions: usize) -> Self {
        Self {
            client: Client::new(),
            url,
            model,
            dimensions,
        }
    }
}

#[derive(Serialize)]
struct OllamaEmbedRequest {
    model: String,
    input: Vec<String>,
}

#[derive(Deserialize)]
struct OllamaEmbedResponse {
    embeddings: Vec<Vec<f32>>,
}

#[async_trait]
impl Embedder for OllamaEmbedder {
    async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, EmbeddingError> {
        let request = OllamaEmbedRequest {
            model: self.model.clone(),
            input: texts.iter().map(|s| s.to_string()).collect(),
        };

        let response = self
            .client
            .post(format!("{}/api/embed", self.url))
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(EmbeddingError::Api(format!("{status}: {body}")));
        }

        let parsed: OllamaEmbedResponse = response.json().await?;

        Ok(parsed.embeddings)
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }
}
