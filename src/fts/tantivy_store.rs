//! Tantivy-based full-text search store
//!
//! Provides BM25 full-text search for hybrid search with RRF fusion.

use anyhow::{anyhow, Result};
use std::path::Path;
use tantivy::{
    collector::TopDocs,
    directory::MmapDirectory,
    query::QueryParser,
    schema::{Field, Schema, STORED, STRING, TEXT, NumericOptions, Value},
    Index, IndexReader, IndexWriter, IndexSettings, TantivyDocument, Term,
};

/// Result from FTS search
#[derive(Debug, Clone)]
pub struct FtsResult {
    /// Chunk ID that matches
    pub chunk_id: u32,
    /// BM25 score from Tantivy
    pub score: f32,
}

/// Full-text search store using Tantivy
pub struct FtsStore {
    index: Index,
    reader: IndexReader,
    writer: Option<IndexWriter>,
    #[allow(dead_code)]
    schema: Schema,
    // Field handles
    chunk_id_field: Field,
    content_field: Field,
    path_field: Field,
    signature_field: Field,
    kind_field: Field,
    string_literals_field: Field,
}

impl FtsStore {
    /// Create or open an FTS index at the given path
    pub fn new(db_path: &Path) -> Result<Self> {
        let fts_path = db_path.join("fts");
        std::fs::create_dir_all(&fts_path)?;

        // Build schema
        let mut schema_builder = Schema::builder();

        // Chunk ID - stored and indexed for retrieval and deletion
        let chunk_id_field = schema_builder.add_u64_field(
            "chunk_id",
            NumericOptions::default().set_indexed().set_stored(),
        );

        // Content - full text indexed for BM25 search
        let content_field = schema_builder.add_text_field("content", TEXT);

        // Path - stored and string indexed for filtering
        let path_field = schema_builder.add_text_field("path", STRING | STORED);

        // Signature - indexed for function/method name search
        let signature_field = schema_builder.add_text_field("signature", TEXT);

        // Kind - stored for filtering (function, class, etc)
        let kind_field = schema_builder.add_text_field("kind", STRING | STORED);

        // String literals - indexed for literal value search
        let string_literals_field = schema_builder.add_text_field("string_literals", TEXT);

        let schema = schema_builder.build();

        // Open or create index
        let index = if fts_path.join("meta.json").exists() {
            Index::open_in_dir(&fts_path)?
        } else {
            let dir = MmapDirectory::open(&fts_path)?;
            Index::create(dir, schema.clone(), IndexSettings::default())?
        };

        // Create reader for searching
        let reader = index.reader()?;

        Ok(Self {
            index,
            reader,
            writer: None,
            schema,
            chunk_id_field,
            content_field,
            path_field,
            signature_field,
            kind_field,
            string_literals_field,
        })
    }

    /// Open FTS store in read-only mode (for search)
    pub fn open_readonly(db_path: &Path) -> Result<Self> {
        let fts_path = db_path.join("fts");

        if !fts_path.join("meta.json").exists() {
            return Err(anyhow!("FTS index not found at {:?}", fts_path));
        }

        let index = Index::open_in_dir(&fts_path)?;
        let schema = index.schema();

        let chunk_id_field = schema.get_field("chunk_id")
            .map_err(|_| anyhow!("Missing chunk_id field"))?;
        let content_field = schema.get_field("content")
            .map_err(|_| anyhow!("Missing content field"))?;
        let path_field = schema.get_field("path")
            .map_err(|_| anyhow!("Missing path field"))?;
        let signature_field = schema.get_field("signature")
            .map_err(|_| anyhow!("Missing signature field"))?;
        let kind_field = schema.get_field("kind")
            .map_err(|_| anyhow!("Missing kind field"))?;
        let string_literals_field = schema.get_field("string_literals")
            .unwrap_or_else(|_| {
                // For backward compatibility with old indexes
                schema.get_field("content").unwrap()
            });

        let reader = index.reader()?;

        Ok(Self {
            index,
            reader,
            writer: None,
            schema,
            chunk_id_field,
            content_field,
            path_field,
            signature_field,
            kind_field,
            string_literals_field,
        })
    }

    /// Ensure writer is initialized for indexing
    fn ensure_writer(&mut self) -> Result<()> {
        if self.writer.is_none() {
            // 50MB heap for writer
            let writer = self.index.writer(50_000_000)?;
            self.writer = Some(writer);
        }
        Ok(())
    }

    /// Add a chunk to the FTS index
    pub fn add_chunk(
        &mut self,
        chunk_id: u32,
        content: &str,
        path: &str,
        signature: Option<&str>,
        kind: &str,
        string_literals: &[String],
    ) -> Result<()> {
        self.ensure_writer()?;

        // Copy field handles before mutable borrow
        let chunk_id_field = self.chunk_id_field;
        let content_field = self.content_field;
        let path_field = self.path_field;
        let signature_field = self.signature_field;
        let kind_field = self.kind_field;
        let string_literals_field = self.string_literals_field;

        let writer = self.writer.as_mut().unwrap();

        let mut doc = TantivyDocument::new();
        doc.add_u64(chunk_id_field, chunk_id as u64);
        doc.add_text(content_field, content);
        doc.add_text(path_field, path);
        doc.add_text(kind_field, kind);

        if let Some(sig) = signature {
            doc.add_text(signature_field, sig);
        }

        // Add string literals as a space-separated field for better search
        if !string_literals.is_empty() {
            let literals_text = string_literals.join(" ");
            doc.add_text(string_literals_field, literals_text);
        }

        writer.add_document(doc)?;
        Ok(())
    }

    /// Delete a chunk by ID
    pub fn delete_chunk(&mut self, chunk_id: u32) -> Result<()> {
        self.ensure_writer()?;
        let chunk_id_field = self.chunk_id_field;
        let writer = self.writer.as_mut().unwrap();
        let term = Term::from_field_u64(chunk_id_field, chunk_id as u64);
        writer.delete_term(term);
        Ok(())
    }

    /// Delete all chunks for a file path
    pub fn delete_by_path(&mut self, path: &str) -> Result<()> {
        self.ensure_writer()?;
        let path_field = self.path_field;
        let writer = self.writer.as_mut().unwrap();
        let term = Term::from_field_text(path_field, path);
        writer.delete_term(term);
        Ok(())
    }

    /// Commit pending changes
    pub fn commit(&mut self) -> Result<()> {
        if let Some(ref mut writer) = self.writer {
            writer.commit()?;
            // Reload reader to see changes
            self.reader.reload()?;
        }
        Ok(())
    }

    /// Search using BM25
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<FtsResult>> {
        let searcher = self.reader.searcher();

        // Parse query against content, signature, and string_literals fields
        let mut query_parser = QueryParser::for_index(
            &self.index,
            vec![self.content_field, self.signature_field, self.string_literals_field],
        );
        
        // Set conjunction mode (AND) by default for multi-term queries
        // This makes "embedding model" require BOTH terms to be present
        query_parser.set_conjunction_by_default();

        // Parse query, fall back to match-all on error
        let parsed_query = match query_parser.parse_query(query) {
            Ok(q) => q,
            Err(_) => {
                // Try escaping special characters
                let escaped = query
                    .replace([':', '(', ')', '[', ']', '{', '}', '^', '"', '~', '*', '?', '\\', '/'], " ");
                query_parser.parse_query(&escaped)?
            }
        };

        // Execute search
        let top_docs = searcher.search(&parsed_query, &TopDocs::with_limit(limit))?;

        // Convert to results
        let mut results = Vec::with_capacity(top_docs.len());
        for (score, doc_address) in top_docs {
            let doc: TantivyDocument = searcher.doc(doc_address)?;

            if let Some(chunk_id) = doc.get_first(self.chunk_id_field) {
                if let Some(id) = chunk_id.as_u64() {
                    results.push(FtsResult {
                        chunk_id: id as u32,
                        score,
                    });
                }
            }
        }

        Ok(results)
    }

    /// Get statistics about the index
    pub fn stats(&self) -> Result<FtsStats> {
        let searcher = self.reader.searcher();
        let num_docs = searcher.num_docs() as usize;

        Ok(FtsStats {
            num_documents: num_docs,
        })
    }

    /// Clear the entire index
    pub fn clear(&mut self) -> Result<()> {
        self.ensure_writer()?;
        let writer = self.writer.as_mut().unwrap();
        writer.delete_all_documents()?;
        writer.commit()?;
        self.reader.reload()?;
        Ok(())
    }
}

/// Statistics about the FTS index
#[derive(Debug, Clone)]
pub struct FtsStats {
    pub num_documents: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_fts_basic() -> Result<()> {
        let dir = tempdir()?;
        let db_path = dir.path().to_path_buf();

        let mut store = FtsStore::new(&db_path)?;

        // Add some chunks
        store.add_chunk(1, "fn hello_world() { println!(\"Hello!\"); }", "src/main.rs", Some("hello_world"), "function", &["Hello!".to_string()])?;
        store.add_chunk(2, "struct UserConfig { name: String, age: u32 }", "src/config.rs", Some("UserConfig"), "struct", &[])?;
        store.add_chunk(3, "fn process_data(data: Vec<u8>) -> Result<()>", "src/processor.rs", Some("process_data"), "function", &[])?;

        store.commit()?;

        // Search for hello
        let results = store.search("hello", 10)?;
        assert!(!results.is_empty());
        assert_eq!(results[0].chunk_id, 1);

        // Search for UserConfig
        let results = store.search("UserConfig", 10)?;
        assert!(!results.is_empty());
        assert_eq!(results[0].chunk_id, 2);

        // Search for process
        let results = store.search("process data", 10)?;
        assert!(!results.is_empty());
        assert_eq!(results[0].chunk_id, 3);

        Ok(())
    }

    #[test]
    fn test_fts_delete() -> Result<()> {
        let dir = tempdir()?;
        let db_path = dir.path().to_path_buf();

        let mut store = FtsStore::new(&db_path)?;

        store.add_chunk(1, "test content one", "file1.rs", None, "block", &[])?;
        store.add_chunk(2, "test content two", "file2.rs", None, "block", &[])?;
        store.commit()?;

        // Should find both
        let results = store.search("test content", 10)?;
        assert_eq!(results.len(), 2);

        // Delete one
        store.delete_chunk(1)?;
        store.commit()?;

        // Should find only one
        let results = store.search("test content", 10)?;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].chunk_id, 2);

        Ok(())
    }

    #[test]
    fn test_fts_string_literals() -> Result<()> {
        let dir = tempdir()?;
        let db_path = dir.path().to_path_buf();

        let mut store = FtsStore::new(&db_path)?;

        // Add chunks with string literals
        store.add_chunk(
            1, 
            "requestHeaders = [(\"API-VERSION\", \"2\")]", 
            "src/api.rs", 
            None, 
            "block",
            &["API-VERSION".to_string(), "2".to_string()],
        )?;
        store.add_chunk(
            2, 
            "const version = \"1.0\";", 
            "src/version.rs", 
            None, 
            "block",
            &["1.0".to_string()],
        )?;
        store.commit()?;

        // Search for "api-version 2" should find the first chunk
        let results = store.search("api-version 2", 10)?;
        assert!(!results.is_empty(), "Should find chunk with API-VERSION and 2");

        Ok(())
    }
}
