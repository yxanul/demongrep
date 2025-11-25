# demongrep

Fast, local semantic code search powered by Rust. A high-performance alternative to osgrep.

## Features

- **Semantic Search**: Natural language queries like "where do we handle authentication?"
- **Hybrid Search**: RRF fusion of vector similarity + Tantivy BM25 full-text search
- **Neural Reranking**: Jina Reranker v1 Turbo cross-encoder for improved accuracy
- **Smart Chunking**: Tree-sitter based AST-aware code chunking with 15+ chunk types
- **Context Windows**: Surrounding code context (3 lines before/after each chunk)
- **Rich Metadata**: Extracts signatures, docstrings, and context breadcrumbs
- **Local Embeddings**: ONNX-powered embedding with fastembed (no API calls)
- **Fast Vector Search**: arroy + LMDB for sub-second search after model load
- **Live File Watching**: Incremental re-indexing on file changes
- **HTTP Server Mode**: Background server with REST API
- **MCP Server**: Claude Code integration via Model Context Protocol
- **JSON Output**: Machine-readable output for scripting and AI agents
- **Multi-Language**: Rust, Python, TypeScript, JavaScript (full AST support)
- **Beautiful CLI**: Colored output, progress bars, multiple output modes
- **Single Binary**: No Node.js required, just a static Rust binary

## Installation

### Prerequisites

**Linux (Ubuntu/Debian):**
```bash
sudo apt-get install protobuf-compiler libssl-dev pkg-config
```

**macOS:**
```bash
brew install protobuf openssl
```

**Windows:**
```bash
winget install -e --id Google.Protobuf
```

### Building

```bash
git clone https://github.com/yourusername/demongrep
cd demongrep
cargo build --release
```

The binary will be at `target/release/demongrep`.

## Quick Start

```bash
# Index current directory
demongrep index

# Search with natural language
demongrep search "where do we handle authentication?"

# Run background server with live file watching
demongrep serve --port 3333
```

## Usage

### Index a Codebase

```bash
# Index current directory
demongrep index

# Index specific path
demongrep index /path/to/project

# Preview what would be indexed
demongrep index --dry-run

# Force re-index (clear and rebuild)
demongrep index --force

# Use specific embedding model
demongrep index --model minilm-l6-q
```

### Search with Natural Language

```bash
# Basic search (hybrid by default: vector + BM25 with RRF fusion)
demongrep search "where do we handle authentication?"

# Enable neural reranking for better accuracy
demongrep search "error handling" --rerank

# Show relevance scores and timing
demongrep search "error handling" --scores

# Show full content with context windows
demongrep search "database queries" --content

# File paths only (like grep -l)
demongrep search "vector embeddings" --compact

# Limit results
demongrep search "parsing" --max-results 10 --per-file 2

# Filter to specific directory
demongrep search "tests" --filter-path src/chunker

# JSON output for scripting/agents
demongrep search "authentication" --json

# Vector-only search (disable hybrid BM25)
demongrep search "query" --vector-only

# Sync database before search (re-index changed files)
demongrep search "my query" --sync
```

### Background Server

```bash
# Start server with live file watching
demongrep serve --port 3333

# Server endpoints:
#   GET  /health - Health check
#   GET  /status - Index statistics
#   POST /search - Search API
```

**Search API:**
```bash
curl -X POST http://localhost:3333/search \
  -H "Content-Type: application/json" \
  -d '{"query": "authentication", "limit": 10}'
```

### MCP Server (Claude Code Integration)

```bash
# Start MCP server for Claude Code
demongrep mcp

# MCP server with specific project path
demongrep mcp /path/to/project
```

**Available MCP Tools:**
- `semantic_search(query, limit)` - Semantic code search
- `get_file_chunks(path)` - Get all chunks from a file
- `index_status()` - Check index health and stats

**Claude Code Configuration** (`~/.claude/claude_desktop_config.json`):
```json
{
  "mcpServers": {
    "demongrep": {
      "command": "/path/to/demongrep",
      "args": ["mcp", "/path/to/project"]
    }
  }
}
```

### Manage Database

```bash
# Show statistics
demongrep stats

# List indexed repositories
demongrep list

# Clear database
demongrep clear
demongrep clear -y  # Skip confirmation
```

### Other Commands

```bash
# Check installation health
demongrep doctor

# Download embedding models
demongrep setup
```

## Performance

### Benchmarks vs osgrep

Tested on [sharkdp/bat](https://github.com/sharkdp/bat) repository (~400 files):

| Metric | demongrep | osgrep |
|--------|-----------|--------|
| **Index Time** | 69s | 120s |
| **Search Accuracy** | 83% | 0% |
| **Speedup** | 1.7x faster | - |

### Embedding Model Benchmarks

Tested on demongrep's codebase (~607 chunks, 9 semantic queries):

| Model | Accuracy | Query Time | Index Time |
|-------|----------|------------|------------|
| AllMiniLML6V2Q | **100%** | 1.8ms | 25s |
| JinaEmbeddingsV2BaseCode | 89% | 10.5ms | 74s |
| BGESmallENV15 (default) | 89% | ~2ms | ~30s |

**Recommendations:**
- Best accuracy: `minilm-l6-q` (AllMiniLML6V2Q)
- Code-specific: `jina-code` (JinaEmbeddingsV2BaseCode)
- Balanced default: `bge-small` (BGESmallENV15)

## Available Models

| Model | Dimensions | Quality | Speed |
|-------|------------|---------|-------|
| `bge-small` (default) | 384 | Good | Fast |
| `minilm-l6` | 384 | Best | Fastest |
| `minilm-l6-q` | 384 | Best | Fastest (quantized) |
| `jina-code` | 768 | Excellent for code | Medium |
| `bge-base` | 768 | Better | Medium |
| `mxbai-large` | 1024 | High quality | Slower |

Use `--model <name>` with index/search commands.

## Environment Variables

| Variable | Description |
|----------|-------------|
| `DEMONGREP_BATCH_SIZE` | Override embedding batch size (default: auto) |
| `RUST_LOG` | Logging level (`demongrep=debug`, etc.) |

## How It Works

### 1. File Discovery
- Walks directory respecting `.gitignore`
- Supports custom `.demongrepignore` and `.osgrepignore` files
- Detects 21+ languages from file extensions
- Skips binary files automatically

### 2. Semantic Chunking
- Parses code with tree-sitter (native Rust, not WASM)
- Extracts functions, classes, methods, structs, enums, traits, impls
- Preserves signatures, docstrings, and context breadcrumbs
- Falls back to sliding window for unsupported languages

### 3. Embedding Generation
- Uses fastembed with ONNX Runtime (CPU-optimized)
- Batched processing with adaptive batch sizes
- SHA-256 content-based change detection

### 4. Vector Storage
- arroy for approximate nearest neighbor search
- LMDB for ACID transactions and persistence
- Single `.demongrep.db/` directory per project

### 5. Incremental Updates
- Two-level change detection: mtime + SHA-256 hash
- Tracks chunk IDs per file for efficient deletion
- Live file watching with 300ms debouncing

## Architecture

```
demongrep/
├── src/
│   ├── main.rs           # Entry point
│   ├── cli/              # CLI commands (clap)
│   ├── file/             # File discovery, language detection, binary detection
│   ├── chunker/          # Tree-sitter semantic chunking
│   │   ├── semantic.rs   # AST-based chunking
│   │   ├── extractor.rs  # Language-specific extractors
│   │   ├── grammar.rs    # Tree-sitter grammar manager
│   │   └── fallback.rs   # Sliding window fallback
│   ├── embed/            # ONNX embedding service
│   ├── vectordb/         # arroy + LMDB vector store
│   ├── cache/            # File metadata store (hash tracking)
│   ├── watch/            # File watcher (notify crate)
│   ├── server/           # HTTP server (axum)
│   ├── search/           # Search with --sync support
│   └── index/            # Indexing workflow
└── examples/             # Demo programs
```

## Comparison with osgrep

| Feature | osgrep | demongrep |
|---------|--------|-----------|
| Language | TypeScript | Rust |
| Tree-sitter | WASM (downloads) | Native (compiled-in) |
| Startup | ~100ms | <1ms |
| Parse speed | 1x | 3-5x faster |
| Chunk types | 3 | 15+ |
| Signatures | No | Yes |
| Docstrings | Inline | Separate field |
| Change detection | SHA256 | mtime + SHA256 |
| Live watching | Yes | Yes |
| HTTP server | Yes | Yes |
| Vector DB | LanceDB | arroy + LMDB |
| Hybrid search | Yes (RRF) | Yes (RRF) |
| Reranking | Yes | Yes (Jina Reranker) |
| MCP server | No | Yes |
| JSON output | Yes | Yes |
| Context windows | No | Yes |
| Build deps | npm, cmake | cargo only |

## Development

```bash
# Debug build
cargo build

# Release build
cargo build --release

# Run tests
cargo test

# Format code
cargo fmt

# Lint
cargo clippy
```

### Logging

```bash
RUST_LOG=demongrep=debug cargo run -- search "query"
RUST_LOG=demongrep::embed=trace cargo run -- index
```

## License

Apache-2.0

## Contributing

Contributions welcome! See [TODO.md](TODO.md) for planned features.
