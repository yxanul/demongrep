# demongrep

**Fast, local semantic code search powered by Rust.**

Search your codebase using natural language queries like *"where do we handle authentication?"* — all running locally with no API calls.

## Features

- **Semantic Search** — Natural language queries that understand code meaning
- **Hybrid Search** — Combines vector similarity + BM25 full-text search with RRF fusion
- **Neural Reranking** — Optional second-pass reranking with Jina Reranker for higher accuracy
- **Smart Chunking** — Tree-sitter AST-aware chunking that preserves functions, classes, methods
- **Context Windows** — Shows surrounding code (3 lines before/after) for better understanding
- **Local & Private** — All processing happens locally using ONNX models, no data leaves your machine
- **Fast** — Sub-second search after initial model load, incremental indexing
- **Multiple Interfaces** — CLI, HTTP server, and MCP server for Claude Code integration

---

## Table of Contents

- [Installation](#installation)
- [Quick Start](#quick-start)
- [Command Reference](#command-reference)
  - [search](#search)
  - [index](#index)
  - [serve](#serve)
  - [mcp](#mcp)
  - [stats](#stats)
  - [clear](#clear)
  - [list](#list)
  - [doctor](#doctor)
  - [setup](#setup)
- [Global Options](#global-options)
- [Search Modes](#search-modes)
- [MCP Server (Claude Code)](#mcp-server-claude-code-integration)
- [HTTP Server API](#http-server-api)
- [Database Management](#database-management)
- [Supported Languages](#supported-languages)
- [Embedding Models](#embedding-models)
- [Configuration](#configuration)
- [How It Works](#how-it-works)
- [Troubleshooting](#troubleshooting)

---

## Installation

### Prerequisites

#### Linux (Ubuntu/Debian)
```bash
sudo apt-get update
sudo apt-get install -y build-essential protobuf-compiler libssl-dev pkg-config
```

#### Linux (Fedora/RHEL)
```bash
sudo dnf install -y gcc protobuf-compiler openssl-devel pkg-config
```

#### macOS
```bash
brew install protobuf openssl pkg-config
```

#### Windows
```powershell
# Using winget
winget install -e --id Google.Protobuf

# Or using chocolatey
choco install protoc
```

### From crates.io

```bash
cargo install demongrep --locked
```

### Building from Source

```bash
# Clone the repository
git clone https://github.com/yxanul/demongrep.git
cd demongrep

# Build release binary (--locked uses the exact dependency versions in Cargo.lock)
cargo build --release --locked

# The binary is at target/release/demongrep
# Optionally, copy to your PATH:
sudo cp target/release/demongrep /usr/local/bin/
```

### Verify Installation

```bash
demongrep --version
demongrep doctor  # Check system health
```

---

## Quick Start

```bash
# 1. Navigate to your project
cd /path/to/your/project

# 2. Index the codebase (first time only, ~30-60s for medium projects)
demongrep index

# 3. Search with natural language
demongrep search "where do we handle authentication?"

# 4. Search with better accuracy (slower)
demongrep search "error handling" --rerank
```

---

## Command Reference

### search

Search the codebase using natural language queries.

```bash
demongrep search <QUERY> [OPTIONS]
```

#### Arguments

| Argument | Description |
|----------|-------------|
| `<QUERY>` | Natural language search query (e.g., "where do we handle authentication?") |

#### Options

| Option | Short | Default | Description |
|--------|-------|---------|-------------|
| `--max-results` | `-m` | 25 | Maximum total results to return |
| `--per-file` | | 1 | Maximum matches to show per file |
| `--content` | `-c` | | Show full chunk content instead of snippets |
| `--scores` | | | Show relevance scores and timing information |
| `--compact` | | | Show file paths only (like `grep -l`) |
| `--sync` | `-s` | | Re-index changed files before searching |
| `--json` | | | Output results as JSON (for scripting/agents) |
| `--path` | | `.` | Path to search in |
| `--filter-path` | | | Only show results from files under this path (e.g., `src/`) |
| `--vector-only` | | | Disable hybrid search, use vector similarity only |
| `--rerank` | | | Enable neural reranking for better accuracy (~1.7s extra) |
| `--rerank-top` | | 50 | Number of candidates to rerank |
| `--rrf-k` | | 20 | RRF fusion parameter (higher = more weight to rank position) |

#### Examples

```bash
# Basic search
demongrep search "database connection pooling"

# Show full code content with context
demongrep search "error handling" --content

# Get JSON output for scripting
demongrep search "authentication" --json -m 10

# Search only in src/api directory
demongrep search "validation" --filter-path src/api

# High-accuracy search with reranking
demongrep search "complex algorithm" --rerank

# Quick search with scores
demongrep search "config loading" --scores

# Re-index changed files, then search
demongrep search "new feature" --sync

# File paths only
demongrep search "tests" --compact
```

---

### index

Index a codebase for semantic search.

```bash
demongrep index [PATH] [OPTIONS]
```

#### Arguments

| Argument | Description |
|----------|-------------|
| `[PATH]` | Path to index (defaults to current directory) |

#### Options

| Option | Short | Description |
|--------|-------|-------------|
| `--dry-run` | | Preview what would be indexed without indexing |
| `--force` | `-f` | Delete existing index and rebuild from scratch |

#### Examples

```bash
# Index current directory
demongrep index

# Index a specific project
demongrep index /path/to/project

# Preview files to be indexed
demongrep index --dry-run

# Force complete re-index (delete and rebuild)
demongrep index --force

# Index with a specific model
demongrep index --model jina-code
```

#### What Gets Indexed

- All text files respecting `.gitignore`
- Custom ignore patterns from `.demongrepignore` or `.osgrepignore`
- Skips binary files, `node_modules/`, `.git/`, etc.

#### Index Location

The index is stored in `.demongrep.db/` directory inside your project root.

---

### serve

Run an HTTP server with live file watching for continuous indexing.

```bash
demongrep serve [PATH] [OPTIONS]
```

#### Options

| Option | Short | Default | Description |
|--------|-------|---------|-------------|
| `--port` | `-p` | 4444 | Port to listen on |

#### Examples

```bash
# Start server on default port (4444)
demongrep serve

# Start server on custom port
demongrep serve --port 3333

# Serve a specific project
demongrep serve /path/to/project --port 8080
```

The server automatically re-indexes files when they change (with 300ms debouncing).

---

### mcp

Start an MCP (Model Context Protocol) server for Claude Code integration.

```bash
demongrep mcp [PATH]
```

#### Arguments

| Argument | Description |
|----------|-------------|
| `[PATH]` | Path to project (defaults to current directory) |

See [MCP Server section](#mcp-server-claude-code-integration) for detailed setup.

---

### stats

Show statistics about the indexed database.

```bash
demongrep stats [PATH]
```

#### Output

```
📊 Database Statistics
============================================================
💾 Database: /path/to/project/.demongrep.db

Vector Store:
   Total chunks: 731
   Total files: 45
   Indexed: ✅ Yes
   Dimensions: 384

Storage:
   Database size: 12.34 MB
   Avg per chunk: 17.28 KB
```

---

### clear

Delete the index database.

```bash
demongrep clear [PATH] [OPTIONS]
```

#### Options

| Option | Short | Description |
|--------|-------|-------------|
| `--yes` | `-y` | Skip confirmation prompt |

#### Examples

```bash
# Clear with confirmation
demongrep clear

# Clear without confirmation (for scripts)
demongrep clear -y

# Clear a specific project's index
demongrep clear /path/to/project -y
```

---

### list

List all indexed repositories (searches for `.demongrep.db` directories).

```bash
demongrep list
```

---

### doctor

Check installation health and system requirements.

```bash
demongrep doctor
```

---

### setup

Pre-download embedding models.

```bash
demongrep setup [OPTIONS]
```

#### Options

| Option | Description |
|--------|-------------|
| `--model` | Specific model to download (defaults to default model) |

---

## Global Options

These options work with all commands:

| Option | Short | Description |
|--------|-------|-------------|
| `--verbose` | `-v` | Enable verbose/debug output |
| `--quiet` | `-q` | Suppress informational output (only results/errors) |
| `--model` | | Override embedding model |
| `--store` | | Override store name |
| `--help` | `-h` | Show help |
| `--version` | `-V` | Show version |

---

## Search Modes

demongrep supports three search modes with different accuracy/speed tradeoffs:

### 1. Hybrid Search (Default)

Combines vector similarity with BM25 full-text search using Reciprocal Rank Fusion (RRF).

```bash
demongrep search "query"
```

- **Speed**: ~75ms
- **Best for**: Most queries, balances semantic understanding with keyword matching

### 2. Vector-Only Search

Pure semantic similarity search using embeddings.

```bash
demongrep search "query" --vector-only
```

- **Speed**: ~72ms
- **Best for**: Conceptual queries where exact keywords don't matter

### 3. Hybrid + Neural Reranking

Two-stage search: hybrid retrieval followed by cross-encoder reranking.

```bash
demongrep search "query" --rerank
```

- **Speed**: ~1.8s (adds ~1.7s for reranking)
- **Best for**: When accuracy matters more than speed

---

## MCP Server (Claude Code Integration)

demongrep can act as an MCP server, allowing Claude Code to search your codebase.

### Setup

1. **Build demongrep** and note the binary path:
   ```bash
   cargo build --release
   # Binary at: /path/to/demongrep/target/release/demongrep
   ```

2. **Index your project**:
   ```bash
   cd /path/to/your/project
   demongrep index
   ```

3. **Configure Claude Code**

   Edit `~/.config/claude-code/config.json` (Linux/Mac) or the appropriate config location:

   ```json
   {
     "mcpServers": {
       "demongrep": {
         "command": "/absolute/path/to/demongrep",
         "args": ["mcp", "/absolute/path/to/your/project"]
       }
     }
   }
   ```

4. **Restart Claude Code**

### Available MCP Tools

| Tool | Parameters | Description |
|------|------------|-------------|
| `semantic_search` | `query`, `limit` | Search code semantically |
| `get_file_chunks` | `path` | Get all indexed chunks from a file |
| `index_status` | | Check if index exists and get stats |

### Example MCP Usage in Claude Code

Once configured, Claude Code can use commands like:
- *"Search for authentication handling"*
- *"Find all chunks in src/auth.rs"*
- *"Check if the index is ready"*

---

## HTTP Server API

### Endpoints

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/health` | Health check (returns `{"status": "ok"}`) |
| GET | `/status` | Index statistics |
| POST | `/search` | Search the codebase |

### Search API

**Request:**
```bash
curl -X POST http://localhost:4444/search \
  -H "Content-Type: application/json" \
  -d '{
    "query": "authentication",
    "limit": 10
  }'
```

**Response:**
```json
{
  "results": [
    {
      "path": "src/auth/handler.rs",
      "start_line": 45,
      "end_line": 67,
      "kind": "Function",
      "content": "pub fn authenticate(...) { ... }",
      "score": 0.89,
      "signature": "fn authenticate(credentials: &Credentials) -> Result<User>"
    }
  ],
  "query": "authentication",
  "total_results": 1
}
```

---

## Database Management

### Index Location

Each project has its own index at `<project_root>/.demongrep.db/`

### Re-indexing

```bash
# Incremental update (only changed files)
demongrep search "query" --sync

# Or explicitly re-index
demongrep index

# Full rebuild (delete and recreate)
demongrep index --force
```

### Delete Index

```bash
# Interactive
demongrep clear

# Non-interactive
demongrep clear -y

# Or manually
rm -rf .demongrep.db/
```

### Check Index Status

```bash
demongrep stats
```

---

## Supported Languages

### Full Semantic Chunking (Tree-sitter AST)

These languages have full AST-aware chunking that extracts functions, classes, methods, etc.:

| Language | Extensions |
|----------|------------|
| Rust | `.rs` |
| Python | `.py`, `.pyw`, `.pyi` |
| JavaScript | `.js`, `.mjs`, `.cjs` |
| TypeScript | `.ts`, `.mts`, `.cts`, `.tsx`, `.jsx` |

### Indexed (Line-based Chunking)

These languages are indexed with fallback line-based chunking:

| Language | Extensions |
|----------|------------|
| Go | `.go` |
| Java | `.java` |
| C | `.c`, `.h` |
| C++ | `.cpp`, `.cc`, `.cxx`, `.hpp` |
| C# | `.cs` |
| Ruby | `.rb`, `.rake` |
| PHP | `.php` |
| Swift | `.swift` |
| Kotlin | `.kt`, `.kts` |
| Shell | `.sh`, `.bash`, `.zsh` |
| Markdown | `.md`, `.markdown`, `.txt` |
| JSON | `.json` |
| YAML | `.yaml`, `.yml` |
| TOML | `.toml` |
| SQL | `.sql` |
| HTML | `.html`, `.htm` |
| CSS | `.css`, `.scss`, `.sass`, `.less` |

---

## Embedding Models

### Available Models

| Name | ID | Dimensions | Speed | Quality | Best For |
|------|-----|------------|-------|---------|----------|
| MiniLM-L6 | `minilm-l6` | 384 | Fastest | Excellent | General use |
| MiniLM-L6 (Q) | `minilm-l6-q` | 384 | Fastest | Excellent | **Default** |
| MiniLM-L12 | `minilm-l12` | 384 | Fast | Better | Higher quality |
| MiniLM-L12 (Q) | `minilm-l12-q` | 384 | Fast | Better | Higher quality |
| BGE Small | `bge-small` | 384 | Fast | Good | General use |
| BGE Small (Q) | `bge-small-q` | 384 | Fast | Good | General use |
| BGE Base | `bge-base` | 768 | Medium | Better | Higher quality |
| BGE Large | `bge-large` | 1024 | Slow | Best | Highest quality |
| Jina Code | `jina-code` | 768 | Medium | Excellent | **Code-specific** |
| Nomic v1.5 | `nomic-v1.5` | 768 | Medium | Good | Long context |
| E5 Multilingual | `e5-multilingual` | 384 | Fast | Good | Non-English code |
| MxBai Large | `mxbai-large` | 1024 | Slow | Excellent | High quality |

### Changing Models

```bash
# Index with specific model
demongrep index --model jina-code

# Search must use same model as index
demongrep search "query" --model jina-code
```

**Note:** The model used for indexing is saved in metadata. If you search with a different model, you may get poor results. Use `--force` to re-index with a new model.

### Query & Document Prefixes

Some retrieval models are trained with task prefixes and lose accuracy without them. demongrep adds them automatically:

| Models | Query prefix | Document prefix |
|--------|--------------|-----------------|
| Nomic (`nomic-v1`, `nomic-v1.5`, `nomic-v1.5-q`), ModernBERT (`modernbert-large`) | `search_query: ` | `search_document: ` |
| E5 (`e5-multilingual`) | `query: ` | `passage: ` |
| BGE (`bge-*`), MxBai (`mxbai-large`) | `Represent this sentence for searching relevant passages: ` | *(none)* |
| MiniLM, Jina Code | *(none)* | *(none)* |

If you built a Nomic, E5 or ModernBERT index before prefixes were added, `demongrep search` will warn you. Run `demongrep index --force` to rebuild it.

---

## Configuration

### Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `DEMONGREP_BATCH_SIZE` | Embedding batch size | Auto (based on model) |
| `RUST_LOG` | Logging level | `demongrep=info` |

### Ignore Files

Create `.demongrepignore` in your project root:

```gitignore
# Ignore test fixtures
**/fixtures/**
**/testdata/**

# Ignore generated code
**/generated/**
*.generated.ts

# Ignore large files
*.min.js
*.bundle.js
```

demongrep also respects `.gitignore` and `.osgrepignore` files.

---

## How It Works

### 1. File Discovery
- Walks directory respecting `.gitignore` and custom ignore files
- Detects language from file extensions
- Skips binary files automatically

### 2. Semantic Chunking
- Parses code with tree-sitter (native Rust implementation)
- Extracts semantic units: functions, classes, methods, structs, traits, impls
- Preserves metadata: signatures, docstrings, context breadcrumbs
- Falls back to line-based chunking for unsupported languages

### 3. Embedding Generation
- Uses fastembed with ONNX Runtime (CPU-optimized)
- Batched processing for efficiency
- SHA-256 content hashing for change detection

### 4. Vector Storage
- arroy for approximate nearest neighbor search
- LMDB for ACID transactions and persistence
- Single `.demongrep.db/` directory per project

### 5. Search
- Query embedding → Vector search → BM25 search → RRF fusion → (Optional) Reranking

---

## Troubleshooting

### "No database found"

```bash
# Index the project first
demongrep index
```

### Search returns poor results

1. **Check if index is stale:**
   ```bash
   demongrep search "query" --sync
   ```

2. **Try different search mode:**
   ```bash
   demongrep search "query" --rerank
   ```

3. **Rebuild index:**
   ```bash
   demongrep index --force
   ```

### Model mismatch warning

If you indexed with one model and search with another:
```bash
# Re-index with the model you want to use
demongrep index --force --model minilm-l6-q
```

### Out of memory during indexing

```bash
# Reduce batch size
DEMONGREP_BATCH_SIZE=32 demongrep index
```

### Server won't start (port in use)

```bash
# Use a different port
demongrep serve --port 5555
```

---

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

### Debug Logging

```bash
RUST_LOG=demongrep=debug demongrep search "query"
RUST_LOG=demongrep::embed=trace demongrep index
```

---

## License

Apache-2.0

---

## Contributing

Contributions welcome! See [TODO.md](TODO.md) for planned features.
