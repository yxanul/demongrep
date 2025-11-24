mod embedder;
mod batch;
mod cache;

pub use embedder::{FastEmbedder, ModelType};
pub use batch::{BatchEmbedder, EmbeddedChunk, EmbeddingStats, cosine_similarity};
pub use cache::{CachedBatchEmbedder, EmbeddingCache, CacheStats};

use anyhow::Result;
use std::sync::{Arc, Mutex};

/// High-level embedding service that combines all features
pub struct EmbeddingService {
    cached_embedder: CachedBatchEmbedder,
    model_type: ModelType,
}

impl EmbeddingService {
    /// Create a new embedding service with default model
    pub fn new() -> Result<Self> {
        Self::with_model(ModelType::default())
    }

    /// Create a new embedding service with specified model
    pub fn with_model(model_type: ModelType) -> Result<Self> {
        let embedder = FastEmbedder::with_model(model_type)?;
        let arc_embedder = Arc::new(Mutex::new(embedder));
        let batch_embedder = BatchEmbedder::new(arc_embedder);
        let cached_embedder = CachedBatchEmbedder::new(batch_embedder);

        Ok(Self {
            cached_embedder,
            model_type,
        })
    }

    /// Embed a batch of chunks with caching
    pub fn embed_chunks(&mut self, chunks: Vec<crate::chunker::Chunk>) -> Result<Vec<EmbeddedChunk>> {
        self.cached_embedder.embed_chunks(chunks)
    }

    /// Embed a single chunk with caching
    pub fn embed_chunk(&mut self, chunk: crate::chunker::Chunk) -> Result<EmbeddedChunk> {
        self.cached_embedder.embed_chunk(chunk)
    }

    /// Embed query text
    pub fn embed_query(&mut self, query: &str) -> Result<Vec<f32>> {
        // Access the batch embedder's embedder via mutex
        let embedder_arc = &self.cached_embedder.batch_embedder.embedder;
        embedder_arc.lock().unwrap().embed_one(query)
    }

    /// Get embedding dimensions
    pub fn dimensions(&self) -> usize {
        self.cached_embedder.dimensions()
    }

    /// Get model information
    pub fn model_name(&self) -> &str {
        self.model_type.name()
    }

    /// Get cache statistics
    pub fn cache_stats(&self) -> CacheStats {
        self.cached_embedder.cache_stats()
    }

    /// Clear the cache
    pub fn clear_cache(&self) {
        self.cached_embedder.clear_cache();
    }

    /// Search for most similar chunks to a query
    pub fn search<'a>(
        &self,
        query_embedding: &[f32],
        embedded_chunks: &'a [EmbeddedChunk],
        limit: usize,
    ) -> Vec<(&'a EmbeddedChunk, f32)> {
        let mut results: Vec<_> = embedded_chunks
            .iter()
            .map(|chunk| {
                let similarity = chunk.similarity_to(query_embedding);
                (chunk, similarity)
            })
            .collect();

        // Sort by similarity (descending)
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Take top N
        results.into_iter().take(limit).collect()
    }
}

impl Default for EmbeddingService {
    fn default() -> Self {
        Self::new().expect("Failed to create default embedding service")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunker::{Chunk, ChunkKind};

    #[test]
    fn test_model_type_default() {
        let model = ModelType::default();
        assert_eq!(model.dimensions(), 384);
    }

    #[test]
    #[ignore] // Requires model download
    fn test_embedding_service_creation() {
        let service = EmbeddingService::new();
        assert!(service.is_ok());

        let service = service.unwrap();
        assert_eq!(service.dimensions(), 384);
    }

    #[test]
    #[ignore] // Requires model
    fn test_embed_query() {
        let service = EmbeddingService::new().unwrap();
        let query_embedding = service.embed_query("find authentication code").unwrap();

        assert_eq!(query_embedding.len(), 384);
    }

    #[test]
    #[ignore] // Requires model
    fn test_embed_chunks_with_cache() {
        let service = EmbeddingService::new().unwrap();

        let chunks = vec![
            Chunk::new(
                "fn authenticate(user: &str) -> bool { true }".to_string(),
                0,
                1,
                ChunkKind::Function,
                "auth.rs".to_string(),
            ),
            Chunk::new(
                "fn hash_password(pwd: &str) -> String { pwd.to_string() }".to_string(),
                2,
                3,
                ChunkKind::Function,
                "auth.rs".to_string(),
            ),
        ];

        // First embedding - no cache
        let embedded1 = service.embed_chunks(chunks.clone()).unwrap();
        assert_eq!(embedded1.len(), 2);

        let stats1 = service.cache_stats();
        assert_eq!(stats1.size, 2);

        // Second embedding - should hit cache
        let embedded2 = service.embed_chunks(chunks.clone()).unwrap();
        assert_eq!(embedded2.len(), 2);

        let stats2 = service.cache_stats();
        assert!(stats2.hit_rate() > 0.0);
    }

    #[test]
    #[ignore] // Requires model
    fn test_search() {
        let service = EmbeddingService::new().unwrap();

        let chunks = vec![
            Chunk::new(
                "fn authenticate(user: &str) -> bool { check_credentials(user) }".to_string(),
                0,
                1,
                ChunkKind::Function,
                "auth.rs".to_string(),
            ),
            Chunk::new(
                "fn calculate_fibonacci(n: u32) -> u32 { if n <= 1 { n } else { calculate_fibonacci(n-1) + calculate_fibonacci(n-2) } }".to_string(),
                0,
                1,
                ChunkKind::Function,
                "math.rs".to_string(),
            ),
            Chunk::new(
                "fn hash_password(password: &str) -> String { sha256(password) }".to_string(),
                2,
                3,
                ChunkKind::Function,
                "auth.rs".to_string(),
            ),
        ];

        let embedded = service.embed_chunks(chunks).unwrap();

        // Search for authentication related code
        let query = "authentication and password hashing";
        let query_embedding = service.embed_query(query).unwrap();

        let results = service.search(&query_embedding, &embedded, 2);

        assert_eq!(results.len(), 2);
        // First two results should be auth-related (higher similarity)
        assert!(results[0].0.chunk.path.contains("auth"));
        assert!(results[0].1 > results[1].1); // Scores should be descending
    }
}
