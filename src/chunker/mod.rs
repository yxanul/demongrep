use anyhow::Result;
use sha2::{Digest, Sha256};
use std::path::Path;

mod grammar;
mod parser;
mod tree_sitter;
mod fallback;
mod dedup;
mod extractor;
mod semantic;

pub use grammar::{GrammarManager, GrammarStats};
pub use parser::{CodeParser, ParsedCode};
pub use tree_sitter::TreeSitterChunker;
pub use dedup::ChunkDeduplicator;
pub use semantic::SemanticChunker;

/// Represents a chunk of code with metadata
#[derive(Debug, Clone)]
pub struct Chunk {
    /// The actual content of the chunk
    pub content: String,

    /// Starting line number (0-indexed)
    pub start_line: usize,

    /// Ending line number (0-indexed)
    pub end_line: usize,

    /// Type of chunk
    pub kind: ChunkKind,

    /// Context breadcrumbs (e.g., ["File: main.rs", "Class: Server", "Function: handle_request"])
    pub context: Vec<String>,

    /// File path this chunk belongs to
    pub path: String,

    /// Function/method signature (if applicable)
    /// Example: "fn sort<T: Ord>(items: Vec<T>) -> Vec<T>"
    pub signature: Option<String>,

    /// Extracted docstring/documentation comment
    pub docstring: Option<String>,

    /// Whether this chunk is complete (not split)
    pub is_complete: bool,

    /// If this chunk was split, which part is it? (0, 1, 2...)
    pub split_index: Option<usize>,

    /// Content hash for deduplication
    pub hash: String,
}

impl Chunk {
    /// Create a new chunk with basic information
    pub fn new(
        content: String,
        start_line: usize,
        end_line: usize,
        kind: ChunkKind,
        path: String,
    ) -> Self {
        let hash = Self::compute_hash(&content);

        Self {
            content,
            start_line,
            end_line,
            kind,
            context: Vec::new(),
            path,
            signature: None,
            docstring: None,
            is_complete: true,
            split_index: None,
            hash,
        }
    }

    /// Compute SHA-256 hash of content for deduplication
    pub fn compute_hash(content: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(content.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    /// Check if this chunk is likely a duplicate based on hash
    pub fn is_duplicate_of(&self, other: &Chunk) -> bool {
        self.hash == other.hash
    }

    /// Get the number of lines in this chunk
    pub fn line_count(&self) -> usize {
        self.end_line.saturating_sub(self.start_line)
    }

    /// Get the size of this chunk in bytes
    pub fn size_bytes(&self) -> usize {
        self.content.len()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChunkKind {
    Function,      // Standalone function
    Class,         // Class definition (non-Rust languages)
    Method,        // Method within class/impl
    Struct,        // Struct definition (Rust)
    Enum,          // Enum definition
    Trait,         // Trait definition (Rust)
    Interface,     // Interface (TypeScript, Java)
    Impl,          // Impl block (Rust)
    Mod,           // Module definition
    TypeAlias,     // Type alias
    Const,         // Constant
    Static,        // Static variable
    Block,         // Gap/unstructured code
    Anchor,        // File-level summary chunk
    Other,         // Catch-all
}

/// Trait for chunking strategies
pub trait Chunker: Send + Sync {
    /// Chunk a file into semantic pieces
    fn chunk_file(&self, path: &Path, content: &str) -> Result<Vec<Chunk>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunker() {
        // TODO: Add tests
    }
}
