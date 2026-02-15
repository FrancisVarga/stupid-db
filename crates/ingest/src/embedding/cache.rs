use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::num::NonZeroUsize;

use lru::LruCache;

/// LRU cache mapping text hash to embedding vector.
pub struct EmbeddingCache {
    cache: LruCache<u64, Vec<f32>>,
    hits: u64,
    misses: u64,
}

impl EmbeddingCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            cache: LruCache::new(
                NonZeroUsize::new(capacity).unwrap_or(NonZeroUsize::new(1).unwrap()),
            ),
            hits: 0,
            misses: 0,
        }
    }

    fn hash_text(text: &str) -> u64 {
        let mut hasher = DefaultHasher::new();
        text.hash(&mut hasher);
        hasher.finish()
    }

    /// Look up a cached embedding by text.
    pub fn get(&mut self, text: &str) -> Option<Vec<f32>> {
        let key = Self::hash_text(text);
        if let Some(vec) = self.cache.get(&key) {
            self.hits += 1;
            Some(vec.clone())
        } else {
            self.misses += 1;
            None
        }
    }

    /// Store an embedding for a text.
    pub fn put(&mut self, text: &str, embedding: Vec<f32>) {
        let key = Self::hash_text(text);
        self.cache.put(key, embedding);
    }

    pub fn hits(&self) -> u64 {
        self.hits
    }

    pub fn misses(&self) -> u64 {
        self.misses
    }

    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64
        }
    }

    pub fn len(&self) -> usize {
        self.cache.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_hit_and_miss() {
        let mut cache = EmbeddingCache::new(100);

        assert!(cache.get("hello").is_none());
        assert_eq!(cache.misses(), 1);
        assert_eq!(cache.hits(), 0);

        cache.put("hello", vec![1.0, 2.0, 3.0]);
        let result = cache.get("hello").unwrap();
        assert_eq!(result, vec![1.0, 2.0, 3.0]);
        assert_eq!(cache.hits(), 1);
    }

    #[test]
    fn cache_eviction() {
        let mut cache = EmbeddingCache::new(2);

        cache.put("a", vec![1.0]);
        cache.put("b", vec![2.0]);
        cache.put("c", vec![3.0]); // evicts "a"

        assert!(cache.get("a").is_none());
        assert!(cache.get("b").is_some());
        assert!(cache.get("c").is_some());
    }

    #[test]
    fn hit_rate_calculation() {
        let mut cache = EmbeddingCache::new(100);
        assert_eq!(cache.hit_rate(), 0.0);

        cache.put("x", vec![1.0]);
        cache.get("x"); // hit
        cache.get("y"); // miss
        assert!((cache.hit_rate() - 0.5).abs() < f64::EPSILON);
    }
}
