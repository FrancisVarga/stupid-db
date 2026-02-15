use std::sync::Arc;

use stupid_core::document::DocId;

use super::traits::{Embedder, EmbeddingError};

/// Collects (DocId, text) pairs and flushes when the batch is full.
pub struct EmbeddingBatcher {
    buffer: Vec<(DocId, String)>,
    batch_size: usize,
    embedder: Arc<dyn Embedder>,
}

impl EmbeddingBatcher {
    pub fn new(embedder: Arc<dyn Embedder>, batch_size: usize) -> Self {
        Self {
            buffer: Vec::with_capacity(batch_size),
            batch_size,
            embedder,
        }
    }

    /// Add a document to the batch. Returns embeddings if the batch is full (auto-flush).
    pub async fn add(
        &mut self,
        doc_id: DocId,
        text: String,
    ) -> Result<Option<Vec<(DocId, Vec<f32>)>>, EmbeddingError> {
        self.buffer.push((doc_id, text));
        if self.buffer.len() >= self.batch_size {
            Ok(Some(self.flush().await?))
        } else {
            Ok(None)
        }
    }

    /// Force-flush remaining items.
    pub async fn flush(&mut self) -> Result<Vec<(DocId, Vec<f32>)>, EmbeddingError> {
        if self.buffer.is_empty() {
            return Ok(Vec::new());
        }
        let batch: Vec<(DocId, String)> = self.buffer.drain(..).collect();
        let texts: Vec<&str> = batch.iter().map(|(_, t)| t.as_str()).collect();
        let embeddings = self.embedder.embed_batch(&texts).await?;

        Ok(batch
            .into_iter()
            .zip(embeddings)
            .map(|((id, _), emb)| (id, emb))
            .collect())
    }

    /// Number of items currently buffered.
    pub fn pending(&self) -> usize {
        self.buffer.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use uuid::Uuid;

    struct FakeEmbedder {
        call_count: AtomicUsize,
        dims: usize,
    }

    impl FakeEmbedder {
        fn new(dims: usize) -> Self {
            Self {
                call_count: AtomicUsize::new(0),
                dims,
            }
        }
    }

    #[async_trait]
    impl Embedder for FakeEmbedder {
        async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, EmbeddingError> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            Ok(texts.iter().map(|_| vec![0.0; self.dims]).collect())
        }

        fn dimensions(&self) -> usize {
            self.dims
        }
    }

    #[tokio::test]
    async fn flush_on_batch_size() {
        let embedder = Arc::new(FakeEmbedder::new(4));
        let mut batcher = EmbeddingBatcher::new(embedder.clone(), 3);

        assert!(batcher.add(Uuid::new_v4(), "a".into()).await.unwrap().is_none());
        assert!(batcher.add(Uuid::new_v4(), "b".into()).await.unwrap().is_none());
        assert_eq!(batcher.pending(), 2);

        let result = batcher.add(Uuid::new_v4(), "c".into()).await.unwrap();
        assert!(result.is_some());
        let embeddings = result.unwrap();
        assert_eq!(embeddings.len(), 3);
        assert_eq!(batcher.pending(), 0);
        assert_eq!(embedder.call_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn manual_flush() {
        let embedder = Arc::new(FakeEmbedder::new(4));
        let mut batcher = EmbeddingBatcher::new(embedder.clone(), 100);

        batcher.add(Uuid::new_v4(), "a".into()).await.unwrap();
        batcher.add(Uuid::new_v4(), "b".into()).await.unwrap();

        let result = batcher.flush().await.unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(batcher.pending(), 0);
    }

    #[tokio::test]
    async fn flush_empty_is_noop() {
        let embedder = Arc::new(FakeEmbedder::new(4));
        let mut batcher = EmbeddingBatcher::new(embedder.clone(), 10);

        let result = batcher.flush().await.unwrap();
        assert!(result.is_empty());
        assert_eq!(embedder.call_count.load(Ordering::SeqCst), 0);
    }
}
