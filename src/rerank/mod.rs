use anyhow::Result;

/// Reranking strategies
pub trait Reranker: Send + Sync {
    /// Rerank search results based on query
    fn rerank(&self, query: &str, results: Vec<ScoredChunk>) -> Result<Vec<ScoredChunk>>;
}

#[derive(Debug, Clone)]
pub struct ScoredChunk {
    pub chunk_id: String,
    pub score: f32,
}

/// Reciprocal Rank Fusion (RRF) reranker
pub struct RrfReranker {
    k: f32,
}

impl RrfReranker {
    pub fn new(k: f32) -> Self {
        Self { k }
    }
}

impl Reranker for RrfReranker {
    fn rerank(&self, _query: &str, results: Vec<ScoredChunk>) -> Result<Vec<ScoredChunk>> {
        // TODO: Implement RRF
        Ok(results)
    }
}
