use super::embedder::FastEmbedder;
use crate::chunker::Chunk;
use anyhow::Result;
use std::sync::{Arc, Mutex};

/// Statistics for embedding operations
#[derive(Debug, Clone, Default)]
pub struct EmbeddingStats {
    pub total_chunks: usize,
    pub embedded_chunks: usize,
    pub cached_chunks: usize,
    pub failed_chunks: usize,
    pub total_time_ms: u128,
}

impl EmbeddingStats {
    pub fn cache_hit_rate(&self) -> f32 {
        if self.total_chunks == 0 {
            return 0.0;
        }
        self.cached_chunks as f32 / self.total_chunks as f32
    }

    pub fn success_rate(&self) -> f32 {
        if self.total_chunks == 0 {
            return 0.0;
        }
        self.embedded_chunks as f32 / self.total_chunks as f32
    }

    pub fn chunks_per_second(&self) -> f32 {
        if self.total_time_ms == 0 {
            return 0.0;
        }
        (self.embedded_chunks as f32 / self.total_time_ms as f32) * 1000.0
    }
}

/// Chunk with its embedding
#[derive(Debug, Clone)]
pub struct EmbeddedChunk {
    pub chunk: Chunk,
    pub embedding: Vec<f32>,
}

impl EmbeddedChunk {
    pub fn new(chunk: Chunk, embedding: Vec<f32>) -> Self {
        Self { chunk, embedding }
    }

    /// Calculate cosine similarity with another embedded chunk
    pub fn similarity(&self, other: &EmbeddedChunk) -> f32 {
        cosine_similarity(&self.embedding, &other.embedding)
    }

    /// Calculate cosine similarity with a query embedding
    pub fn similarity_to(&self, query_embedding: &[f32]) -> f32 {
        cosine_similarity(&self.embedding, query_embedding)
    }
}

/// Batch processor for embedding chunks efficiently
pub struct BatchEmbedder {
    pub embedder: Arc<Mutex<FastEmbedder>>,
    batch_size: usize,
}

impl BatchEmbedder {
    /// Create a new batch embedder
    pub fn new(embedder: Arc<Mutex<FastEmbedder>>) -> Self {
        Self {
            embedder,
            batch_size: 32, // Default batch size
        }
    }

    /// Create with custom batch size
    pub fn with_batch_size(embedder: Arc<Mutex<FastEmbedder>>, batch_size: usize) -> Self {
        Self {
            embedder,
            batch_size,
        }
    }

    /// Embed a batch of chunks
    pub fn embed_chunks(&mut self, chunks: Vec<Chunk>) -> Result<Vec<EmbeddedChunk>> {
        if chunks.is_empty() {
            return Ok(Vec::new());
        }

        let total = chunks.len();
        println!("📊 Embedding {} chunks (batch size: {})...", total, self.batch_size);

        let start = std::time::Instant::now();
        let mut embedded_chunks = Vec::with_capacity(total);

        // Process in batches
        for (batch_idx, chunk_batch) in chunks.chunks(self.batch_size).enumerate() {
            let batch_start = batch_idx * self.batch_size;
            let batch_end = (batch_start + chunk_batch.len()).min(total);

            println!(
                "   Batch {}/{}: chunks {}-{}",
                batch_idx + 1,
                (total + self.batch_size - 1) / self.batch_size,
                batch_start + 1,
                batch_end
            );

            // Prepare texts for embedding
            let texts: Vec<String> = chunk_batch
                .iter()
                .map(|chunk| self.prepare_text(chunk))
                .collect();

            // Generate embeddings
            let embeddings = self.embedder.lock().unwrap().embed_batch(texts)?;

            // Combine chunks with embeddings
            for (chunk, embedding) in chunk_batch.iter().zip(embeddings.into_iter()) {
                embedded_chunks.push(EmbeddedChunk::new(chunk.clone(), embedding));
            }
        }

        let elapsed = start.elapsed();
        println!(
            "✅ Embedded {} chunks in {:.2}s ({:.1} chunks/sec)",
            total,
            elapsed.as_secs_f32(),
            total as f32 / elapsed.as_secs_f32()
        );

        Ok(embedded_chunks)
    }

    /// Embed a single chunk
    pub fn embed_chunk(&mut self, chunk: Chunk) -> Result<EmbeddedChunk> {
        let text = self.prepare_text(&chunk);
        let embedding = self.embedder.lock().unwrap().embed_one(&text)?;
        Ok(EmbeddedChunk::new(chunk, embedding))
    }

    /// Prepare chunk text for embedding
    ///
    /// Combines different chunk metadata for better embeddings:
    /// - Context breadcrumbs
    /// - Signature (if available)
    /// - Docstring (if available)
    /// - Content
    fn prepare_text(&self, chunk: &Chunk) -> String {
        let mut parts = Vec::new();

        // Add context breadcrumbs (e.g., "File: main.rs > Class: Server")
        if !chunk.context.is_empty() {
            let context = chunk.context.join(" > ");
            parts.push(format!("Context: {}", context));
        }

        // Add signature if available (e.g., "fn process(data: Vec<T>) -> Result<T>")
        if let Some(sig) = &chunk.signature {
            parts.push(format!("Signature: {}", sig));
        }

        // Add docstring if available
        if let Some(doc) = &chunk.docstring {
            // Clean up docstring
            let cleaned = clean_docstring(doc);
            if !cleaned.is_empty() {
                parts.push(format!("Documentation: {}", cleaned));
            }
        }

        // Add main content
        parts.push(format!("Code:\n{}", chunk.content));

        parts.join("\n")
    }

    /// Get embedding dimensions
    pub fn dimensions(&self) -> usize {
        self.embedder.lock().unwrap().dimensions()
    }

    /// Get embedder (locks mutex and returns copy of embedder for reading)
    pub fn embedder_info(&self) -> (String, usize) {
        let embedder = self.embedder.lock().unwrap();
        (embedder.model_name().to_string(), embedder.dimensions())
    }
}

/// Clean docstring by removing comment markers
fn clean_docstring(doc: &str) -> String {
    doc.lines()
        .map(|line| {
            let trimmed = line.trim();
            // Remove common comment markers
            trimmed
                .strip_prefix("///")
                .or_else(|| trimmed.strip_prefix("//!"))
                .or_else(|| trimmed.strip_prefix("//"))
                .or_else(|| trimmed.strip_prefix("*"))
                .or_else(|| trimmed.strip_prefix("\""))
                .unwrap_or(trimmed)
                .trim()
        })
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Calculate cosine similarity between two vectors
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }

    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let mag_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let mag_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if mag_a == 0.0 || mag_b == 0.0 {
        return 0.0;
    }

    dot / (mag_a * mag_b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunker::ChunkKind;

    #[test]
    fn test_embedding_stats() {
        let stats = EmbeddingStats {
            total_chunks: 100,
            embedded_chunks: 80,
            cached_chunks: 20,
            failed_chunks: 0,
            total_time_ms: 1000,
        };

        assert_eq!(stats.cache_hit_rate(), 0.2);
        assert_eq!(stats.success_rate(), 0.8);
        assert_eq!(stats.chunks_per_second(), 80.0);
    }

    #[test]
    fn test_clean_docstring() {
        let rust_doc = "/// This is a doc comment\n/// with multiple lines";
        assert_eq!(clean_docstring(rust_doc), "This is a doc comment with multiple lines");

        let python_doc = "\"\"\"This is a Python docstring\"\"\"";
        assert_eq!(clean_docstring(python_doc), "This is a Python docstring");

        let jsdoc = "/**\n * JSDoc comment\n * with multiple lines\n */";
        assert_eq!(clean_docstring(jsdoc), "JSDoc comment with multiple lines");
    }

    #[test]
    fn test_prepare_text() {
        let embedder = Arc::new(FastEmbedder::new().unwrap_or_else(|_| {
            // For tests, create a mock if real embedder fails
            panic!("Cannot create embedder in test");
        }));

        let batch = BatchEmbedder::new(embedder);

        let mut chunk = Chunk::new(
            "fn test() { println!(\"test\"); }".to_string(),
            0,
            1,
            ChunkKind::Function,
            "test.rs".to_string(),
        );
        chunk.context = vec!["File: test.rs".to_string(), "Function: test".to_string()];
        chunk.signature = Some("fn test()".to_string());
        chunk.docstring = Some("/// Test function".to_string());

        let text = batch.prepare_text(&chunk);

        assert!(text.contains("Context: File: test.rs > Function: test"));
        assert!(text.contains("Signature: fn test()"));
        assert!(text.contains("Documentation: Test function"));
        assert!(text.contains("Code:"));
    }

    #[test]
    fn test_cosine_similarity() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 0.001);

        let c = vec![1.0, 0.0, 0.0];
        let d = vec![0.0, 1.0, 0.0];
        assert!((cosine_similarity(&c, &d) - 0.0).abs() < 0.001);

        let e = vec![1.0, 1.0, 0.0];
        let f = vec![1.0, 0.0, 0.0];
        let sim = cosine_similarity(&e, &f);
        assert!(sim > 0.7 && sim < 0.72); // Should be ~1/sqrt(2)
    }

    #[test]
    #[ignore] // Requires model
    fn test_batch_embedder() {
        let embedder = Arc::new(FastEmbedder::new().unwrap());
        let batch = BatchEmbedder::new(embedder);

        let chunks = vec![
            Chunk::new(
                "fn main() {}".to_string(),
                0,
                1,
                ChunkKind::Function,
                "test.rs".to_string(),
            ),
            Chunk::new(
                "struct Point { x: i32, y: i32 }".to_string(),
                2,
                3,
                ChunkKind::Struct,
                "test.rs".to_string(),
            ),
        ];

        let embedded = batch.embed_chunks(chunks).unwrap();
        assert_eq!(embedded.len(), 2);

        for emb_chunk in &embedded {
            assert_eq!(emb_chunk.embedding.len(), 384);
        }
    }
}
