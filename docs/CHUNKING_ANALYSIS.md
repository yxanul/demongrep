# Chunking Strategy Analysis: osgrep vs demongrep

## osgrep's Current Implementation

### Architecture Overview

**File**: `src/lib/chunker.ts` (455 lines)
**Approach**: Tree-sitter based semantic chunking with fallback

### Key Parameters
```typescript
MAX_CHUNK_LINES = 75       // Target ~512 tokens
MAX_CHUNK_CHARS = 2000
OVERLAP_LINES = 10         // Context preservation
OVERLAP_CHARS = 200        // For minified code
```

### Workflow

1. **Initialization**
   - Loads web-tree-sitter WASM runtime
   - Downloads grammars on-demand from GitHub releases
   - Caches in `~/.osgrep/grammars/`
   - Graceful degradation if offline

2. **Language Support**
   - TypeScript (.ts)
   - TSX/JSX (.tsx, .jsx)
   - Python (.py)
   - Go (.go)
   - **Only 4 languages!**

3. **Semantic Extraction**
   - Identifies: `function_declaration`, `method_definition`, `class_declaration`
   - Unwraps `export` statements to get actual definitions
   - Treats top-level arrow functions as definitions
   - Extracts name from multiple AST node types
   - Builds context breadcrumbs: `File: path > Class: Name > Function: method`

4. **Gap Handling**
   - Captures code between definitions as "block" chunks
   - Includes imports, module-level code, etc.
   - Ensures complete file coverage

5. **Chunk Splitting**
   - If chunk > 75 lines or > 2000 chars: split
   - Sliding window with 10-line overlap
   - Preserves header line (first non-empty) in splits
   - Character-based splitting for minified/single-line code

6. **Fallback Strategy**
   - Simple sliding window when tree-sitter unavailable
   - Same parameters (75 lines, 10 overlap)
   - Type: "block" for all chunks

### Auxiliary Features (chunk-utils.ts)

1. **Anchor Chunks**: Special file-level chunks with:
   - Top comments/docstrings
   - Import list
   - Export list
   - First 30 lines of code (preamble)
   - Gives model file-level context

2. **Helper Functions**:
   - `extractTopComments()` - Get file header comments
   - `extractImports()` - Parse import/require statements
   - `extractExports()` - Parse export statements
   - `formatChunkText()` - Add breadcrumb header

### Test Coverage

```typescript
// tests/chunking.test.ts
- Fallback splitting (overlapping chunks)
- Character-based splitting (minified code)
- Anchor chunk creation
- Chunk formatting
```

## What Works Well

### 1. Semantic Awareness ✅
- Extracts complete functions/classes
- Context breadcrumbs are valuable for search
- Gap handling ensures no code is lost

### 2. Graceful Degradation ✅
- Works offline after initial grammar download
- Fallback chunking if parsing fails
- No hard failures

### 3. Anchor Chunks ✅
- Clever idea: gives model file-level context
- Helps with "where is X imported?" questions
- Includes exports for API discovery

### 4. Smart Splitting ✅
- Handles both line-based and character-based content
- Overlap preserves context
- Header preservation helps with split functions

## Areas for Improvement

### 1. Limited Language Support ⚠️

**Current**: 4 languages (TypeScript, TSX, Python, Go)

**Improvement**: Support 10+ languages
- Rust (critical for this project!)
- Java, C, C++
- C#, Ruby, PHP
- Swift, Kotlin

**Impact**: High - Many codebases use multiple languages

### 2. Grammar Management 🔧

**Current**:
- Downloads from GitHub "latest" (no version pinning)
- Only downloads when needed (can fail if offline)
- No pre-download command

**Improvements**:
- Version-pinned grammars for reproducibility
- `demongrep setup` command to pre-download
- Bundle common grammars in release binary
- Native tree-sitter (faster than WASM)

**Impact**: Medium - Better UX, faster startup

### 3. Context Extraction 📝

**Current**:
- Basic context: `File > Class > Function`
- No type signatures
- No parameter info
- No docstring preservation

**Improvements**:
- Extract function signatures: `fn sort(items: Vec<T>) -> Vec<T>`
- Preserve docstrings with code blocks
- Include type annotations
- Track parent module/namespace

**Impact**: High - Better search quality

### 4. Chunk Metadata 📊

**Current**:
- Only: `content`, `startLine`, `endLine`, `type`, `context`
- No complexity tracking
- No split indicators

**Improvements**:
```rust
struct Chunk {
    content: String,
    start_line: usize,
    end_line: usize,
    kind: ChunkKind,
    context: Vec<String>,

    // New metadata
    signature: Option<String>,      // Function signature
    docstring: Option<String>,      // Extracted docstring
    is_complete: bool,              // True if not split
    split_index: Option<usize>,     // Which part of split (0, 1, 2...)
    complexity: Option<usize>,      // Cyclomatic complexity
    hash: String,                   // Content hash for dedup
}
```

**Impact**: High - Enables deduplication and better ranking

### 5. No Deduplication 💾

**Current**: Every chunk embedded separately, even duplicates

**Problem**: Common patterns embedded many times:
- Boilerplate (license headers, imports)
- Generated code (protobuf, GraphQL)
- Repeated utility functions

**Improvement**: Content-based deduplication
```rust
// Hash chunk content
let hash = sha256(&chunk.content);

// Check cache before embedding
if let Some(embedding) = embedding_cache.get(&hash) {
    return embedding.clone();  // Reuse
}
```

**Impact**: High - 10-30% reduction in embeddings

### 6. Fixed Overlap Strategy 📏

**Current**: Always 10 lines overlap

**Problem**:
- Might split mid-statement
- Doesn't consider AST boundaries

**Improvement**: Smart overlap
- Overlap at statement boundaries
- Preserve complete expressions
- Adjust overlap based on chunk type

**Impact**: Medium - Better chunk coherence

### 7. Limited Header Preservation 🔖

**Current**: Only preserves first non-empty line

**Problem**: Insufficient for understanding split functions

**Improvement**: Preserve full signature
```rust
// Original function
fn complicated_function(
    param1: Type1,
    param2: Type2,
) -> Result<Output> {
    // ... 200 lines ...
}

// Split chunk 2 should include:
// [Signature]
// fn complicated_function(param1: Type1, param2: Type2) -> Result<Output>
// [Continued from line 150]
// ... chunk content ...
```

**Impact**: Medium - Better context in split chunks

### 8. No Partial Parsing Recovery 🛠️

**Current**: If parsing fails anywhere, fall back to line-based

**Improvement**: Partial AST usage
- Parse as much as possible
- Use AST for parsed regions
- Fall back to line-based for unparsed regions
- Mix semantic + fallback chunks

**Impact**: Medium - More robust

### 9. Performance Considerations ⚡

**Current**: WASM tree-sitter
- Slower than native
- Runtime overhead

**Improvement**: Native Rust tree-sitter
- ~3-5x faster parsing
- No WASM overhead
- Better multi-threading

**Impact**: Medium - Faster indexing (especially on large repos)

### 10. Testing Coverage 🧪

**Current**: 2 basic tests
- Fallback splitting
- Character splitting

**Improvement**: Comprehensive test suite
- Per-language chunking tests
- Edge cases (syntax errors, incomplete code)
- Overlap verification
- Context breadcrumb correctness
- Deduplication
- Performance benchmarks

**Impact**: High - Catch regressions early

## demongrep Implementation Plan

### Phase 2.1: Grammar Management System

```rust
// src/chunker/grammar.rs
pub struct GrammarManager {
    cache_dir: PathBuf,
    grammars: DashMap<Language, tree_sitter::Language>,
}

impl GrammarManager {
    // Use compiled-in grammars (no downloads!)
    pub fn new() -> Self {
        // tree-sitter crates come with compiled grammars
        let mut manager = Self::default();

        // Pre-load common languages
        manager.add_language(Language::Rust, tree_sitter_rust::language());
        manager.add_language(Language::Python, tree_sitter_python::language());
        // ...

        manager
    }
}
```

**Advantages over osgrep:**
- No network requests needed
- Instant startup
- Version pinned to crate versions
- Native code (faster)

### Phase 2.2: Enhanced Chunk Structure

```rust
// src/chunker/mod.rs
#[derive(Debug, Clone)]
pub struct Chunk {
    pub content: String,
    pub start_line: usize,
    pub end_line: usize,
    pub kind: ChunkKind,
    pub context: Vec<String>,
    pub path: String,

    // Enhanced metadata
    pub signature: Option<String>,
    pub docstring: Option<String>,
    pub is_complete: bool,
    pub split_index: Option<usize>,
    pub hash: String,
}

impl Chunk {
    pub fn compute_hash(&self) -> String {
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        hasher.update(self.content.as_bytes());
        format!("{:x}", hasher.finalize())
    }
}
```

### Phase 2.3: Semantic Extraction with Type Info

```rust
// For Rust code
fn extract_function_signature(node: Node, source: &str) -> Option<String> {
    // Capture: fn name(params) -> return
    let name = node.child_by_field_name("name")?;
    let params = node.child_by_field_name("parameters")?;
    let return_type = node.child_by_field_name("return_type");

    let mut sig = format!("fn {}", name.utf8_text(source.as_bytes()).ok()?);
    sig.push_str(params.utf8_text(source.as_bytes()).ok()?);

    if let Some(ret) = return_type {
        sig.push_str(" -> ");
        sig.push_str(ret.utf8_text(source.as_bytes()).ok()?);
    }

    Some(sig)
}
```

### Phase 2.4: Anchor Chunks (Port from osgrep)

```rust
pub fn build_anchor_chunk(path: &Path, content: &str) -> Chunk {
    let lines: Vec<&str> = content.lines().collect();

    // Extract metadata
    let top_comments = extract_top_comments(&lines);
    let imports = extract_imports(&lines);
    let exports = extract_exports(&lines);
    let preamble = lines.iter().take(30).collect();

    // Build anchor content
    let mut sections = vec![
        format!("File: {}", path.display()),
    ];

    if !imports.is_empty() {
        sections.push(format!("Imports: {}", imports.join(", ")));
    }

    if !exports.is_empty() {
        sections.push(format!("Exports: {}", exports.join(", ")));
    }

    // ... combine sections

    Chunk {
        content: sections.join("\n\n"),
        kind: ChunkKind::Anchor,
        is_complete: true,
        // ...
    }
}
```

### Phase 2.5: Smart Deduplication

```rust
pub struct ChunkDeduplicator {
    seen: DashMap<String, usize>,  // hash -> first_occurrence_index
}

impl ChunkDeduplicator {
    pub fn deduplicate(&self, chunks: Vec<Chunk>) -> Vec<Chunk> {
        chunks.into_iter().enumerate().filter_map(|(idx, chunk)| {
            let hash = chunk.hash.clone();

            // First occurrence: keep it
            if self.seen.insert(hash.clone(), idx).is_none() {
                Some(chunk)
            } else {
                // Duplicate: skip
                None
            }
        }).collect()
    }
}
```

## Performance Comparison (Projected)

| Metric | osgrep (WASM TS) | demongrep (Native Rust) |
|--------|------------------|-------------------------|
| Parse speed | 1x (baseline) | 3-5x faster |
| Startup time | ~100ms | <10ms |
| Memory | Higher (JS GC) | Lower (precise control) |
| Languages | 4 | 10+ |
| Deduplication | No | Yes (~20% savings) |
| Native grammars | No | Yes |

## Conclusion

osgrep has a solid foundation:
- ✅ Semantic chunking works well
- ✅ Anchor chunks are clever
- ✅ Graceful degradation

demongrep improvements:
- 🚀 **10+ languages** (vs 4)
- 🚀 **Native tree-sitter** (3-5x faster)
- 🚀 **Content deduplication** (20-30% savings)
- 🚀 **Enhanced metadata** (signatures, docstrings)
- 🚀 **No network requests** (compiled-in grammars)
- 🚀 **Better testing** (comprehensive suite)

**Next**: Implement Phase 2.1 (Grammar Management System)
