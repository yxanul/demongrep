use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Global configuration for demongrep
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Root directory for demongrep data
    pub data_dir: PathBuf,

    /// Embedding model configuration
    pub embedding: EmbeddingConfig,

    /// Vector database configuration
    pub vectordb: VectorDbConfig,

    /// Indexing configuration
    pub indexing: IndexingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingConfig {
    /// Model name (e.g., "mxbai-embed-xsmall-v1")
    pub model: String,

    /// Device to use (cpu, cuda, directml)
    pub device: Device,

    /// Batch size for embedding
    pub batch_size: usize,

    /// Cache size in MB
    pub cache_size_mb: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Device {
    Cpu,
    Cuda,
    DirectML,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorDbConfig {
    /// Vector database backend
    pub backend: VectorDbType,

    /// Connection configuration (backend-specific)
    pub connection: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VectorDbType {
    LanceDb,
    Qdrant,
    Milvus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexingConfig {
    /// Maximum chunk size in lines
    pub max_chunk_lines: usize,

    /// Maximum chunk size in characters
    pub max_chunk_chars: usize,

    /// Overlap between chunks in lines
    pub overlap_lines: usize,

    /// Number of parallel workers
    pub workers: usize,
}

impl Config {
    /// Load configuration from default location or create default
    pub fn load() -> Result<Self> {
        // TODO: Load from ~/.demongrep/config.toml
        Ok(Self::default())
    }

    /// Get the data directory, creating it if necessary
    pub fn data_dir(&self) -> Result<PathBuf> {
        if !self.data_dir.exists() {
            std::fs::create_dir_all(&self.data_dir)?;
        }
        Ok(self.data_dir.clone())
    }
}

impl Default for Config {
    fn default() -> Self {
        let home = dirs::home_dir().expect("Could not find home directory");

        Self {
            data_dir: home.join(".demongrep"),
            embedding: EmbeddingConfig {
                model: "mxbai-embed-xsmall-v1".to_string(),
                device: Device::Cpu,
                batch_size: 32,
                cache_size_mb: 512,
            },
            vectordb: VectorDbConfig {
                backend: VectorDbType::LanceDb,
                connection: "data/vectordb".to_string(),
            },
            indexing: IndexingConfig {
                max_chunk_lines: 75,
                max_chunk_chars: 2000,
                overlap_lines: 10,
                workers: num_cpus::get(),
            },
        }
    }
}
