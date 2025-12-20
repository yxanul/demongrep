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

pub use semantic::SemanticChunker;

/// Default number of context lines before/after a chunk
pub const DEFAULT_CONTEXT_LINES: usize = 3;

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

    /// Lines of code immediately before this chunk (for context)
    pub context_prev: Option<String>,

    /// Lines of code immediately after this chunk (for context)
    pub context_next: Option<String>,

    /// Extracted string literals for better search (e.g., "API-VERSION", "2")
    pub string_literals: Vec<String>,
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
            context_prev: None,
            context_next: None,
            string_literals: Vec::new(),
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

    /// Extract string literals from content for better search
    /// Extracts strings from common patterns like "string", 'string', `string`
    pub fn extract_string_literals(content: &str) -> Vec<String> {
        let mut literals = Vec::new();
        let mut chars = content.chars().peekable();
        
        while let Some(ch) = chars.next() {
            if ch == '"' || ch == '\'' || ch == '`' {
                let quote = ch;
                let mut literal = String::new();
                let mut escaped = false;
                
                while let Some(ch) = chars.next() {
                    if escaped {
                        escaped = false;
                        literal.push(ch);
                    } else if ch == '\\' {
                        escaped = true;
                    } else if ch == quote {
                        // End of string literal
                        if !literal.trim().is_empty() && literal.len() < 100 {
                            literals.push(literal);
                        }
                        break;
                    } else {
                        literal.push(ch);
                    }
                }
            }
        }
        
        literals
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

    #[test]
    fn test_extract_string_literals() {
        let code = r#"
            let x = "hello";
            let y = 'world';
            let headers = [("API-VERSION", "2")];
            let msg = `template string`;
        "#;
        
        let literals = Chunk::extract_string_literals(code);
        
        assert!(literals.contains(&"hello".to_string()));
        assert!(literals.contains(&"world".to_string()));
        assert!(literals.contains(&"API-VERSION".to_string()));
        assert!(literals.contains(&"2".to_string()));
        assert!(literals.contains(&"template string".to_string()));
        
        assert_eq!(literals.len(), 5);
    }

    #[test]
    fn test_extract_string_literals_with_escapes() {
        let code = "let msg = \"Hello \\\"World\\\"!\";";
        
        let literals = Chunk::extract_string_literals(code);
        
        assert_eq!(literals.len(), 1);
        assert_eq!(literals[0], "Hello \"World\"!");
    }
}
