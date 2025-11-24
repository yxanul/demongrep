# demongrep

Fast, local semantic code search powered by Rust - a high-performance alternative to osgrep.

## Features (Planned)

- **Semantic Search**: Natural language queries like "where do we handle authentication?"
- **Incremental Indexing**: Only re-index changed files, not the entire codebase
- **Smart Chunking**: Tree-sitter based AST-aware code chunking
- **Multi-Model Support**: Pluggable embedding models (mxbai-embed-xsmall-v1, nomic-embed, bge-small)
- **GPU Acceleration**: CUDA and DirectML support via ONNX Runtime
- **Memory-Aware Cache**: LRU cache with configurable memory limits
- **VectorDB Abstraction**: Support for LanceDB, Qdrant, and Milvus
- **Hybrid Search**: Combines vector similarity with BM25/FTS using RRF
- **Single Binary**: No Node.js required, just a static Rust binary
- **Background Server**: Hot daemon with file watching for instant results

## Installation Prerequisites

Before building demongrep, you need to install some system dependencies:

### Protocol Buffers Compiler (protoc)

LanceDB requires the Protocol Buffers compiler. Install it based on your OS:

**Windows:**
```bash
# Using winget
winget install -e --id Google.Protobuf

# Or download from:
# https://github.com/protocolbuffers/protobuf/releases
```

**macOS:**
```bash
brew install protobuf
```

**Linux:**
```bash
# Ubuntu/Debian
sudo apt-get install protobuf-compiler

# Fedora
sudo dnf install protobuf-compiler

# Arch Linux
sudo pacman -S protobuf
```

Verify installation:
```bash
protoc --version
```

## Building

```bash
git clone https://github.com/yourusername/demongrep
cd demongrep
cargo build --release
```

## Usage (Coming Soon)

```bash
# Index current directory
demongrep index

# Search with natural language
demongrep search "where do we handle authentication?"

# Run background server
demongrep serve

# Download embedding models
demongrep setup

# Check health
demongrep doctor

# List indexed repositories
demongrep list
```

## Project Structure

```
demongrep/
├── src/
│   ├── main.rs              # Entry point
│   ├── cli/                 # CLI commands & argument parsing
│   ├── config/              # Configuration management
│   ├── file/                # File walking and filtering
│   ├── chunker/             # Tree-sitter based code chunking
│   ├── embed/               # Embedding models (ONNX)
│   ├── rerank/              # Reranking strategies (RRF, cross-encoder)
│   ├── vectordb/            # Vector database abstraction
│   ├── cache/               # LRU cache for embeddings
│   ├── index/               # Indexing logic
│   ├── search/              # Search and ranking
│   ├── watch/               # File watching for incremental updates
│   ├── server/              # HTTP server
│   └── bench/               # Benchmarking framework
├── benches/                 # Criterion benchmarks
├── tests/                   # Integration tests
└── Cargo.toml
```

## Architecture

### Key Improvements Over osgrep

1. **Incremental Indexing**: Track file-to-chunk mappings, only re-index changed files
2. **Smart Chunking**: AST-aware boundaries that preserve complete functions/classes
3. **Memory-Aware Cache**: Moka-based LRU with memory limits and hit-rate tracking
4. **GPU Support**: ONNX Runtime with CUDA/DirectML acceleration
5. **Multi-Backend**: Abstraction layer supports multiple vector databases
6. **Comprehensive Benchmarks**: Quality metrics (precision@k, NDCG) + performance
7. **Zero Dependencies**: Single static binary, no Node.js runtime

### Components

#### Chunking Strategy
- Uses tree-sitter to identify semantic boundaries (functions, classes, methods)
- Preserves docstrings with code
- Metadata-rich chunks (knows chunk type: function, class, etc.)
- Fallback for unsupported languages (sliding window)

#### Embedding
- ONNX Runtime for model inference
- Batched processing for efficiency
- Deduplication of identical chunks (common boilerplate)
- Support for multiple models via HuggingFace Hub

#### Vector Database
- Pluggable backend system
- LanceDB: Fast embedded vector DB
- Qdrant: High-performance vector search engine
- Milvus: Scalable vector database

#### Search
- Vector similarity search
- Hybrid search with BM25 (via Tantivy)
- Reciprocal Rank Fusion (RRF) for result combination
- Optional cross-encoder reranking

## Development Status

🚧 **Currently in early development** 🚧

- [x] Project structure and module skeleton
- [x] CLI framework with clap
- [x] Configuration system
- [x] File walker with .gitignore support
- [ ] Tree-sitter integration
- [ ] ONNX embedding pipeline
- [ ] LanceDB integration
- [ ] Search implementation
- [ ] Incremental indexing
- [ ] File watching
- [ ] HTTP server
- [ ] Benchmarking suite

## Benchmarking Plan

Once implemented, we'll benchmark:

1. **Model Comparison**: Quality (precision@k, NDCG) vs Speed vs Memory
2. **Device Comparison**: CPU vs CUDA vs DirectML
3. **VectorDB Comparison**: Index time, query latency, memory usage
4. **Scalability**: 100K LOC, 1M LOC, 30M LOC repositories

## Inspiration

This project builds upon ideas from [osgrep](https://github.com/Ryandonofrio3/osgrep) and [mgrep](https://github.com/mixedbread-ai/mgrep), reimagining semantic code search in Rust for maximum performance.

## License

Apache-2.0

## Contributing

Contributions welcome! This is an early-stage project with lots of opportunities to contribute.
