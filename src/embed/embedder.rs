use anyhow::{anyhow, Result};
use fastembed::{EmbeddingModel as FastEmbedModel, InitOptions, TextEmbedding};

/// Available embedding models
#[derive(Debug, Clone, Copy)]
pub enum ModelType {
    /// BGE Small EN v1.5 - 384 dimensions, good balance of speed/quality
    BGESmallENV15,
    /// All-MiniLM-L6-v2 - 384 dimensions, fast and efficient
    AllMiniLML6V2,
    /// BGE Base EN v1.5 - 768 dimensions, higher quality
    BGEBaseENV15,
    /// mxbai-embed-large-v1 - 1024 dimensions, best quality
    MxbaiEmbedLargeV1,
}

impl ModelType {
    pub fn to_fastembed_model(&self) -> FastEmbedModel {
        match self {
            Self::BGESmallENV15 => FastEmbedModel::BGESmallENV15,
            Self::AllMiniLML6V2 => FastEmbedModel::AllMiniLML6V2,
            Self::BGEBaseENV15 => FastEmbedModel::BGEBaseENV15,
            Self::MxbaiEmbedLargeV1 => FastEmbedModel::MxbaiEmbedLargeV1,
        }
    }

    pub fn dimensions(&self) -> usize {
        match self {
            Self::BGESmallENV15 => 384,
            Self::AllMiniLML6V2 => 384,
            Self::BGEBaseENV15 => 768,
            Self::MxbaiEmbedLargeV1 => 1024,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::BGESmallENV15 => "BAAI/bge-small-en-v1.5",
            Self::AllMiniLML6V2 => "sentence-transformers/all-MiniLM-L6-v2",
            Self::BGEBaseENV15 => "BAAI/bge-base-en-v1.5",
            Self::MxbaiEmbedLargeV1 => "mixedbread-ai/mxbai-embed-large-v1",
        }
    }
}

impl Default for ModelType {
    fn default() -> Self {
        // Use BGE Small as default - good balance
        Self::BGESmallENV15
    }
}

/// Fast embedding model using fastembed library
pub struct FastEmbedder {
    model: TextEmbedding,
    model_type: ModelType,
}

impl FastEmbedder {
    /// Create a new embedder with default model
    pub fn new() -> Result<Self> {
        Self::with_model(ModelType::default())
    }

    /// Create a new embedder with specified model
    pub fn with_model(model_type: ModelType) -> Result<Self> {
        println!("📦 Loading embedding model: {}", model_type.name());
        println!("   Dimensions: {}", model_type.dimensions());

        let model = TextEmbedding::try_new(
            InitOptions::new(model_type.to_fastembed_model())
                .with_show_download_progress(true)
        )
            .map_err(|e| anyhow!("Failed to initialize embedding model: {}", e))?;

        println!("✅ Model loaded successfully!");

        Ok(Self { model, model_type })
    }

    /// Embed a batch of texts
    pub fn embed_batch(&mut self, texts: Vec<String>) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        // Convert strings to references
        let text_refs: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();

        // Generate embeddings
        let embeddings = self
            .model
            .embed(text_refs, None)
            .map_err(|e| anyhow!("Failed to generate embeddings: {}", e))?;

        Ok(embeddings)
    }

    /// Embed a single text
    pub fn embed_one(&mut self, text: &str) -> Result<Vec<f32>> {
        let embeddings = self.embed_batch(vec![text.to_string()])?;
        embeddings
            .into_iter()
            .next()
            .ok_or_else(|| anyhow!("No embedding generated"))
    }

    /// Get the dimensionality of embeddings
    pub fn dimensions(&self) -> usize {
        self.model_type.dimensions()
    }

    /// Get the model name
    pub fn model_name(&self) -> &str {
        self.model_type.name()
    }

    /// Get the model type
    pub fn model_type(&self) -> ModelType {
        self.model_type
    }
}

impl Default for FastEmbedder {
    fn default() -> Self {
        Self::new().expect("Failed to create default embedder")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_type_dimensions() {
        assert_eq!(ModelType::BGESmallENV15.dimensions(), 384);
        assert_eq!(ModelType::AllMiniLML6V2.dimensions(), 384);
        assert_eq!(ModelType::BGEBaseENV15.dimensions(), 768);
        assert_eq!(ModelType::MxbaiEmbedLargeV1.dimensions(), 1024);
    }

    #[test]
    fn test_model_type_names() {
        assert_eq!(
            ModelType::BGESmallENV15.name(),
            "BAAI/bge-small-en-v1.5"
        );
        assert_eq!(
            ModelType::AllMiniLML6V2.name(),
            "sentence-transformers/all-MiniLM-L6-v2"
        );
    }

    #[test]
    fn test_default_model() {
        let model = ModelType::default();
        assert_eq!(model.dimensions(), 384);
    }

    #[test]
    #[ignore] // Requires downloading model
    fn test_embedder_creation() {
        let embedder = FastEmbedder::new();
        assert!(embedder.is_ok());

        let embedder = embedder.unwrap();
        assert_eq!(embedder.dimensions(), 384);
    }

    #[test]
    #[ignore] // Requires model
    fn test_embed_single_text() {
        let embedder = FastEmbedder::new().unwrap();
        let embedding = embedder.embed_one("Hello, world!").unwrap();

        assert_eq!(embedding.len(), 384);
        // Check embedding is normalized (roughly unit length)
        let magnitude: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((magnitude - 1.0).abs() < 0.1);
    }

    #[test]
    #[ignore] // Requires model
    fn test_embed_batch() {
        let embedder = FastEmbedder::new().unwrap();
        let texts = vec![
            "Hello, world!".to_string(),
            "Rust is awesome".to_string(),
            "Code search with AI".to_string(),
        ];

        let embeddings = embedder.embed_batch(texts).unwrap();

        assert_eq!(embeddings.len(), 3);
        for embedding in embeddings {
            assert_eq!(embedding.len(), 384);
        }
    }

    #[test]
    #[ignore] // Requires model
    fn test_semantic_similarity() {
        let embedder = FastEmbedder::new().unwrap();

        let text1 = "The quick brown fox jumps over the lazy dog";
        let text2 = "A fast auburn fox leaps over a sleepy canine";
        let text3 = "Python is a programming language";

        let emb1 = embedder.embed_one(text1).unwrap();
        let emb2 = embedder.embed_one(text2).unwrap();
        let emb3 = embedder.embed_one(text3).unwrap();

        // Cosine similarity
        let sim_1_2 = cosine_similarity(&emb1, &emb2);
        let sim_1_3 = cosine_similarity(&emb1, &emb3);

        // Similar texts should have higher similarity
        assert!(sim_1_2 > sim_1_3);
        assert!(sim_1_2 > 0.7); // Should be quite similar
    }

    fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
        let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let mag_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let mag_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
        dot / (mag_a * mag_b)
    }
}
