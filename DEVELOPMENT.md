# Development Guide

## Prerequisites

1. **Rust** (stable, 1.75+)
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```

2. **Protocol Buffers Compiler** (protoc)
   - Required for LanceDB
   - See [README.md](README.md#protocol-buffers-compiler-protoc) for installation instructions

3. **C++ Compiler** (for some dependencies)
   - Windows: Visual Studio Build Tools
   - macOS: Xcode Command Line Tools
   - Linux: gcc/g++

## Building

```bash
# Debug build (faster compilation)
cargo build

# Release build (optimized)
cargo build --release

# Check without building
cargo check

# Run tests
cargo test

# Run benchmarks (when implemented)
cargo bench
```

## Project Structure

```
src/
├── main.rs              # Entry point, CLI dispatcher
├── cli/                 # Command-line interface
│   ├── mod.rs          # CLI structure with clap
│   ├── doctor.rs       # Health check command
│   └── setup.rs        # Model download command
├── config/             # Configuration
│   └── mod.rs          # Config structs, defaults, persistence
├── file/               # File operations
│   ├── mod.rs          # File walker
│   └── binary.rs       # Binary file detection
├── chunker/            # Code chunking
│   ├── mod.rs          # Chunker trait
│   ├── tree_sitter.rs  # AST-based chunking
│   └── fallback.rs     # Sliding window fallback
├── embed/              # Embedding models
│   └── mod.rs          # ONNX-based embedder
├── rerank/             # Result reranking
│   └── mod.rs          # RRF and cross-encoder reranking
├── vectordb/           # Vector database
│   ├── mod.rs          # VectorDB trait
│   └── lancedb.rs      # LanceDB implementation
├── cache/              # Caching layer
│   └── mod.rs          # LRU cache with memory awareness
├── index/              # Indexing logic
│   └── mod.rs          # Index creation and management
├── search/             # Search implementation
│   └── mod.rs          # Search and ranking
├── watch/              # File watching
│   └── mod.rs          # Incremental update watcher
├── server/             # HTTP server
│   └── mod.rs          # Axum-based server
└── bench/              # Benchmarking utilities
    └── mod.rs
```

## Development Workflow

### Phase 1: Core Infrastructure (Current)
- [x] Project setup
- [x] CLI skeleton
- [x] Module structure
- [ ] Configuration loading/saving
- [ ] File walker with .gitignore/.demongrepignore

### Phase 2: Chunking
- [ ] Tree-sitter grammar loading
- [ ] AST-based chunking for Rust/Python/TypeScript/JavaScript
- [ ] Fallback chunking for unsupported languages
- [ ] Chunk deduplication

### Phase 3: Embedding
- [ ] Download models from HuggingFace Hub
- [ ] ONNX Runtime integration
- [ ] Batched inference
- [ ] CPU-only implementation first
- [ ] GPU support (CUDA/DirectML) later

### Phase 4: Vector Storage
- [ ] LanceDB integration
- [ ] Index creation/deletion
- [ ] Batch insertion
- [ ] Vector similarity search

### Phase 5: Search (MVP)
- [ ] Basic vector search
- [ ] Result formatting
- [ ] CLI search command

### Phase 6: Incremental Indexing
- [ ] File-to-chunk mapping
- [ ] Change detection
- [ ] Delta updates
- [ ] File watching with notify

### Phase 7: Advanced Search
- [ ] Tantivy integration for BM25
- [ ] RRF combination of vector + BM25
- [ ] Cross-encoder reranking
- [ ] Result highlighting

### Phase 8: Server Mode
- [ ] Axum HTTP server
- [ ] Health endpoint
- [ ] Search endpoint
- [ ] Server lock file
- [ ] Integration with file watcher

### Phase 9: Optimization
- [ ] GPU acceleration
- [ ] Memory-aware caching
- [ ] Parallel processing with rayon
- [ ] Binary size optimization

### Phase 10: Benchmarking
- [ ] Model quality benchmarks (precision@k, NDCG)
- [ ] Speed benchmarks
- [ ] Memory benchmarks
- [ ] Scalability tests
- [ ] Comparison with osgrep

## Testing

```bash
# Run all tests
cargo test

# Run specific test
cargo test test_name

# Run tests with output
cargo test -- --nocapture

# Run integration tests only
cargo test --test '*'
```

## Code Style

We use `rustfmt` and `clippy`:

```bash
# Format code
cargo fmt

# Check formatting
cargo fmt --check

# Run clippy
cargo clippy

# Run clippy with pedantic checks
cargo clippy -- -W clippy::pedantic
```

## Debugging

### Logging

Set the `RUST_LOG` environment variable:

```bash
# Info level
RUST_LOG=demongrep=info cargo run

# Debug level
RUST_LOG=demongrep=debug cargo run

# Trace level (very verbose)
RUST_LOG=demongrep=trace cargo run

# Module-specific
RUST_LOG=demongrep::embed=debug cargo run
```

### Common Issues

#### 1. protoc not found
```
Error: Could not find `protoc`
```

**Solution**: Install Protocol Buffers compiler (see README.md)

#### 2. ONNX Runtime errors
If you see ONNX Runtime errors, ensure you have:
- Visual C++ Redistributable (Windows)
- libc++ (Linux)

#### 3. Tree-sitter build errors
Tree-sitter requires a C compiler. Ensure you have:
- MSVC (Windows)
- gcc/clang (Linux/macOS)

## Performance Profiling

### Flamegraph

```bash
cargo install flamegraph
cargo flamegraph --bin demongrep -- search "query"
```

### Criterion Benchmarks

```bash
cargo bench
```

Results will be in `target/criterion/`.

## Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Add tests
5. Run `cargo fmt` and `cargo clippy`
6. Submit a pull request

## Next Steps

See [README.md](README.md) for the full roadmap and feature list.
