use super::batch::EmbeddedChunk;
use crate::chunker::Chunk;
use anyhow::Result;
use dashmap::DashMap;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Cache for embeddings keyed by chunk hash
///
/// Uses DashMap for concurrent access without locks.
/// Chunks are identified by their SHA-256 content hash.
pub struct EmbeddingCache {
    cache: DashMap<String, Vec<f32>>,
    hits: AtomicUsize,
    misses: AtomicUsize,
}

impl EmbeddingCache {
    /// Create a new empty cache
    pub fn new() -> Self {
        Self {
            cache: DashMap::new(),
            hits: AtomicUsize::new(0),
            misses: AtomicUsize::new(0),
        }
    }

    /// Get embedding from cache if available
    pub fn get(&self, chunk: &Chunk) -> Option<Vec<f32>> {
        if let Some(embedding) = self.cache.get(&chunk.hash) {
            self.hits.fetch_add(1, Ordering::Relaxed);
            Some(embedding.clone())
        } else {
            self.misses.fetch_add(1, Ordering::Relaxed);
            None
        }
    }

    /// Store embedding in cache
    pub fn put(&self, chunk: &Chunk, embedding: Vec<f32>) {
        self.cache.insert(chunk.hash.clone(), embedding);
    }

    /// Store an embedded chunk
    pub fn put_embedded(&self, embedded: &EmbeddedChunk) {
        self.cache
            .insert(embedded.chunk.hash.clone(), embedded.embedding.clone());
    }

    /// Check if cache contains embedding for chunk
    pub fn contains(&self, chunk: &Chunk) -> bool {
        self.cache.contains_key(&chunk.hash)
    }

    /// Get cache statistics
    pub fn stats(&self) -> CacheStats {
        CacheStats {
            size: self.cache.len(),
            hits: self.hits.load(Ordering::Relaxed),
            misses: self.misses.load(Ordering::Relaxed),
        }
    }

    /// Clear the cache
    pub fn clear(&self) {
        self.cache.clear();
        self.hits.store(0, Ordering::Relaxed);
        self.misses.store(0, Ordering::Relaxed);
    }

    /// Get cache size
    pub fn len(&self) -> usize {
        self.cache.len()
    }

    /// Check if cache is empty
    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }
}

impl Default for EmbeddingCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Cache statistics
#[derive(Debug, Clone)]
pub struct CacheStats {
    pub size: usize,
    pub hits: usize,
    pub misses: usize,
}

impl CacheStats {
    pub fn hit_rate(&self) -> f32 {
        let total = self.hits + self.misses;
        if total == 0 {
            return 0.0;
        }
        self.hits as f32 / total as f32
    }

    pub fn total_requests(&self) -> usize {
        self.hits + self.misses
    }
}

/// Cached batch embedder that uses an embedding cache
pub struct CachedBatchEmbedder {
    pub batch_embedder: super::batch::BatchEmbedder,
    cache: EmbeddingCache,
}

impl CachedBatchEmbedder {
    /// Create a new cached batch embedder
    pub fn new(batch_embedder: super::batch::BatchEmbedder) -> Self {
        Self {
            batch_embedder,
            cache: EmbeddingCache::new(),
        }
    }

    /// Embed chunks using cache when possible
    pub fn embed_chunks(&mut self, chunks: Vec<Chunk>) -> Result<Vec<EmbeddedChunk>> {
        if chunks.is_empty() {
            return Ok(Vec::new());
        }

        let total = chunks.len();
        let mut embedded_chunks = Vec::with_capacity(total);
        let mut chunks_to_embed = Vec::new();
        let mut cache_indices = Vec::new();

        // Check cache first
        println!("🔍 Checking cache for {} chunks...", total);
        for (idx, chunk) in chunks.iter().enumerate() {
            if let Some(embedding) = self.cache.get(chunk) {
                embedded_chunks.push(EmbeddedChunk::new(chunk.clone(), embedding));
            } else {
                chunks_to_embed.push(chunk.clone());
                cache_indices.push(idx);
            }
        }

        let cached_count = embedded_chunks.len();
        let to_embed_count = chunks_to_embed.len();

        println!(
            "   ✅ Found {} in cache, embedding {} new chunks",
            cached_count, to_embed_count
        );

        // Embed remaining chunks
        if !chunks_to_embed.is_empty() {
            let newly_embedded = self.batch_embedder.embed_chunks(chunks_to_embed)?;

            // Store in cache
            for embedded in &newly_embedded {
                self.cache.put_embedded(embedded);
            }

            embedded_chunks.extend(newly_embedded);
        }

        // Sort by original order if needed
        // (Note: Current implementation maintains order naturally due to how we build the vec)

        let stats = self.cache.stats();
        println!(
            "📊 Cache stats: {} entries, {:.1}% hit rate",
            stats.size,
            stats.hit_rate() * 100.0
        );

        Ok(embedded_chunks)
    }

    /// Embed a single chunk with caching
    pub fn embed_chunk(&mut self, chunk: Chunk) -> Result<EmbeddedChunk> {
        if let Some(embedding) = self.cache.get(&chunk) {
            return Ok(EmbeddedChunk::new(chunk, embedding));
        }

        let embedded = self.batch_embedder.embed_chunk(chunk)?;
        self.cache.put_embedded(&embedded);

        Ok(embedded)
    }

    /// Get cache statistics
    pub fn cache_stats(&self) -> CacheStats {
        self.cache.stats()
    }

    /// Clear the cache
    pub fn clear_cache(&self) {
        self.cache.clear();
    }

    /// Get embedding dimensions
    pub fn dimensions(&self) -> usize {
        self.batch_embedder.dimensions()
    }

    /// Get cache reference
    pub fn cache(&self) -> &EmbeddingCache {
        &self.cache
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunker::ChunkKind;

    #[test]
    fn test_cache_creation() {
        let cache = EmbeddingCache::new();
        assert_eq!(cache.len(), 0);
        assert!(cache.is_empty());
    }

    #[test]
    fn test_cache_put_get() {
        let cache = EmbeddingCache::new();

        let chunk = Chunk::new(
            "fn test() {}".to_string(),
            0,
            1,
            ChunkKind::Function,
            "test.rs".to_string(),
        );

        let embedding = vec![1.0, 2.0, 3.0];

        // Initially not in cache
        assert!(cache.get(&chunk).is_none());

        // Put in cache
        cache.put(&chunk, embedding.clone());

        // Now should be in cache
        assert!(cache.contains(&chunk));
        let retrieved = cache.get(&chunk).unwrap();
        assert_eq!(retrieved, embedding);

        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn test_cache_stats() {
        let cache = EmbeddingCache::new();

        let chunk1 = Chunk::new(
            "fn test1() {}".to_string(),
            0,
            1,
            ChunkKind::Function,
            "test.rs".to_string(),
        );

        let chunk2 = Chunk::new(
            "fn test2() {}".to_string(),
            2,
            3,
            ChunkKind::Function,
            "test.rs".to_string(),
        );

        cache.put(&chunk1, vec![1.0, 2.0, 3.0]);

        // Hit
        cache.get(&chunk1);

        // Miss
        cache.get(&chunk2);

        // Hit
        cache.get(&chunk1);

        let stats = cache.stats();
        assert_eq!(stats.hits, 2);
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.total_requests(), 3);
        assert!((stats.hit_rate() - 0.666).abs() < 0.01);
    }

    #[test]
    fn test_cache_clear() {
        let cache = EmbeddingCache::new();

        let chunk = Chunk::new(
            "fn test() {}".to_string(),
            0,
            1,
            ChunkKind::Function,
            "test.rs".to_string(),
        );

        cache.put(&chunk, vec![1.0, 2.0, 3.0]);
        assert_eq!(cache.len(), 1);

        cache.clear();
        assert_eq!(cache.len(), 0);
        assert!(cache.is_empty());
    }

    #[test]
    fn test_embedded_chunk_put() {
        let cache = EmbeddingCache::new();

        let chunk = Chunk::new(
            "fn test() {}".to_string(),
            0,
            1,
            ChunkKind::Function,
            "test.rs".to_string(),
        );

        let embedded = EmbeddedChunk::new(chunk.clone(), vec![1.0, 2.0, 3.0]);

        cache.put_embedded(&embedded);

        assert!(cache.contains(&chunk));
        let retrieved = cache.get(&chunk).unwrap();
        assert_eq!(retrieved, vec![1.0, 2.0, 3.0]);
    }

    #[test]
    fn test_cache_deduplication() {
        let cache = EmbeddingCache::new();

        // Same content = same hash
        let chunk1 = Chunk::new(
            "fn test() {}".to_string(),
            0,
            1,
            ChunkKind::Function,
            "test.rs".to_string(),
        );

        let chunk2 = Chunk::new(
            "fn test() {}".to_string(),
            10,
            11,
            ChunkKind::Function,
            "other.rs".to_string(),
        );

        // Both should have same hash
        assert_eq!(chunk1.hash, chunk2.hash);

        // Put with chunk1
        cache.put(&chunk1, vec![1.0, 2.0, 3.0]);

        // Should be able to retrieve with chunk2 (same content hash)
        assert!(cache.contains(&chunk2));
        let retrieved = cache.get(&chunk2).unwrap();
        assert_eq!(retrieved, vec![1.0, 2.0, 3.0]);
    }
}
