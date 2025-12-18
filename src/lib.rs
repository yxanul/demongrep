// Allow dead code - this is a library with public APIs for future use
#![allow(dead_code)]

pub mod config;
pub mod chunker;
pub mod embed;
pub mod rerank;
pub mod vectordb;
pub mod cache;
pub mod index;
pub mod search;
pub mod watch;
pub mod server;
pub mod bench;
pub mod file;
pub mod fts;
pub mod mcp;
pub mod output;

// Re-export commonly used types
pub use config::Config;
pub use file::{FileInfo, FileWalker, Language, WalkStats};
pub use chunker::{Chunk, ChunkKind, Chunker};
pub use embed::{EmbeddingService, EmbeddedChunk, ModelType, CacheStats};
pub use vectordb::{VectorStore, SearchResult, StoreStats};
pub use fts::{FtsStore, FtsResult};
