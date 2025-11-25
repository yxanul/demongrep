# TODO: Missing Features Roadmap

Features planned to achieve parity with osgrep and beyond.

## High Priority

### Claude Code Integration
MCP server for direct integration with Claude Code.

- [ ] Implement MCP (Model Context Protocol) server
- [ ] `demongrep mcp` command to start MCP server
- [ ] Tool: `semantic_search(query, limit)` - search codebase
- [ ] Tool: `get_file_chunks(path)` - get all chunks from a file
- [ ] Tool: `index_status()` - check index health
- [ ] Auto-discovery of `.demongrep.db` in project

**Why**: Enable Claude to use semantic code search during conversations.

## Medium Priority

### Server Authentication
Secure the HTTP server for multi-user environments.

- [ ] Generate random auth token on first `serve`
- [ ] Store token in `.demongrep.db/auth_token`
- [ ] Require `Authorization: Bearer <token>` header
- [ ] Add `--no-auth` flag for local-only use
- [ ] Lock file to prevent multiple server instances

### Custom Ignore File
Project-specific ignore patterns.

- [ ] Support `.demongrepignore` file (already partially implemented)
- [ ] Glob patterns: `*.test.ts`, `**/fixtures/**`
- [ ] Negation patterns: `!important.test.ts`
- [ ] Comment lines starting with `#`

### Context Windows
Include surrounding code for better understanding.

- [ ] Add `context_prev` and `context_next` fields to chunks
- [ ] Configurable context size (lines or tokens)
- [ ] Include in search results
- [ ] Useful for LLM consumption

## Low Priority

### Adaptive Throttling
Prevent resource exhaustion on large codebases.

- [ ] Monitor CPU usage during indexing
- [ ] Monitor memory usage
- [ ] Throttle embedding batch size dynamically
- [ ] Add `--max-memory` and `--max-cpu` flags
- [ ] Graceful degradation under load

### GPU Acceleration
Use CUDA/DirectML for faster embedding.

- [ ] Detect available GPU providers
- [ ] Add `--device gpu` flag
- [ ] CUDA support for NVIDIA GPUs
- [ ] DirectML support for Windows
- [ ] Benchmark GPU vs CPU performance

### Additional Languages
Extend tree-sitter support.

- [ ] Go (tree-sitter-go)
- [ ] Java (tree-sitter-java)
- [ ] C# (tree-sitter-c-sharp)
- [ ] PHP (tree-sitter-php)
- [ ] Ruby (tree-sitter-ruby)
- [ ] Swift (tree-sitter-swift)
- [ ] Kotlin (tree-sitter-kotlin)

### JSON Output Mode
Structured output for scripting.

- [ ] `--json` flag for all commands
- [ ] Machine-readable search results
- [ ] Consistent JSON schema
- [ ] Include all metadata (scores, chunks, timing)

### Parallel Embedding
Multi-threaded embedding for faster indexing.

- [ ] Use rayon for parallel chunk embedding
- [ ] Thread pool with configurable size
- [ ] Benchmark parallel vs sequential
- [ ] Memory-safe batch processing

## Nice to Have

### Result Highlighting
Show matched terms in results.

- [ ] Highlight query terms in snippets
- [ ] Use ANSI colors for terminal
- [ ] HTML highlighting for server API

### Watch Mode for CLI
Live search updates.

- [ ] `demongrep search --watch "query"`
- [ ] Re-run search when files change
- [ ] Clear and redraw results

### Database Compaction
Optimize storage over time.

- [ ] Remove orphaned chunks
- [ ] Rebuild vector index periodically
- [ ] Report storage savings

### Telemetry (opt-in)
Anonymous usage statistics.

- [ ] Track common queries (hashed)
- [ ] Track model usage
- [ ] Track performance metrics
- [ ] Strictly opt-in with `--telemetry`

## Completed

- [x] File discovery with .gitignore support
- [x] Tree-sitter semantic chunking (Rust, Python, TypeScript, JavaScript)
- [x] fastembed ONNX embedding (15+ models)
- [x] arroy + LMDB vector storage
- [x] CLI with multiple output modes
- [x] HTTP server with REST API
- [x] Live file watching with debouncing
- [x] Incremental indexing (mtime + hash)
- [x] `--sync` flag for search
- [x] Multiple embedding model support
- [x] Adaptive batch sizes
- [x] Hybrid Search (RRF) - Tantivy BM25 + vector similarity with RRF fusion
  - Hybrid search is default, use `--vector-only` to disable
  - Configurable RRF k parameter with `--rrf-k` (default 20)
- [x] Neural Reranking - Jina Reranker v1 Turbo cross-encoder
  - `--rerank` flag enables second-pass reranking
  - `--rerank-top N` controls how many results to rerank (default 50)
  - Score blending: 57.5% rerank + 42.5% RRF
