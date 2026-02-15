pub mod batcher;
pub mod cache;
pub mod ollama;
pub mod openai;
pub mod template;
pub mod traits;

pub use batcher::EmbeddingBatcher;
pub use cache::EmbeddingCache;
pub use ollama::OllamaEmbedder;
pub use openai::OpenAiEmbedder;
pub use traits::{Embedder, EmbeddingError};
