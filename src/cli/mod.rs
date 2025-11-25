use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

use crate::embed::ModelType;

/// Fast, local semantic code search powered by Rust
#[derive(Parser, Debug)]
#[command(name = "demongrep")]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// Enable verbose output
    #[arg(short, long, global = true)]
    pub verbose: bool,

    /// Suppress informational output (only show results/errors)
    #[arg(short, long, global = true)]
    pub quiet: bool,

    /// Override default store name
    #[arg(long, global = true)]
    pub store: Option<String>,

    /// Embedding model to use (e.g., bge-small, minilm-l6-q, jina-code)
    /// Available: minilm-l6, minilm-l6-q, minilm-l12, minilm-l12-q, paraphrase-minilm,
    ///            bge-small, bge-small-q, bge-base, nomic-v1, nomic-v1.5, nomic-v1.5-q,
    ///            jina-code, e5-multilingual, mxbai-large, modernbert-large
    #[arg(long, global = true)]
    pub model: Option<String>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Search the codebase using natural language
    Search {
        /// Search query (e.g., "where do we handle authentication?")
        query: String,

        /// Maximum total results to return
        #[arg(short = 'm', long, default_value = "25")]
        max_results: usize,

        /// Maximum matches to show per file
        #[arg(long, default_value = "1")]
        per_file: usize,

        /// Show full chunk content instead of snippets
        #[arg(short, long)]
        content: bool,

        /// Show relevance scores
        #[arg(long)]
        scores: bool,

        /// Show file paths only (like grep -l)
        #[arg(long)]
        compact: bool,

        /// Force re-index changed files before searching
        #[arg(short, long)]
        sync: bool,

        /// Output JSON for agents
        #[arg(long)]
        json: bool,

        /// Path to search in (defaults to current directory)
        #[arg(long)]
        path: Option<PathBuf>,

        /// Use vector-only search (disable hybrid FTS)
        #[arg(long)]
        vector_only: bool,

        /// RRF k parameter for score fusion (default 20)
        #[arg(long, default_value = "20")]
        rrf_k: f32,

        /// Enable neural reranking for better accuracy (uses Jina Reranker)
        #[arg(long)]
        rerank: bool,

        /// Number of top results to rerank (default 50)
        #[arg(long, default_value = "50")]
        rerank_top: usize,

        /// Filter results to files under this path (e.g., "src/")
        #[arg(long)]
        filter_path: Option<String>,
    },

    /// Index the repository
    Index {
        /// Path to index (defaults to current directory)
        path: Option<PathBuf>,

        /// Show what would be indexed without actually indexing
        #[arg(long)]
        dry_run: bool,

        /// Force full re-index
        #[arg(short, long)]
        force: bool,
    },

    /// Run a background server with live file watching
    Serve {
        /// Port to listen on
        #[arg(short, long, default_value = "4444")]
        port: u16,

        /// Path to serve (defaults to current directory)
        path: Option<PathBuf>,
    },

    /// List all indexed repositories
    List,

    /// Show statistics about the vector database
    Stats {
        /// Path to show stats for (defaults to current directory)
        path: Option<PathBuf>,
    },

    /// Clear the vector database
    Clear {
        /// Path to clear (defaults to current directory)
        path: Option<PathBuf>,

        /// Skip confirmation prompt
        #[arg(short = 'y', long)]
        yes: bool,
    },

    /// Check installation health
    Doctor,

    /// Download embedding models
    Setup {
        /// Model to download (defaults to mxbai-embed-xsmall-v1)
        #[arg(long)]
        model: Option<String>,
    },

    /// Start MCP server for Claude Code integration
    Mcp {
        /// Path to project (defaults to current directory)
        path: Option<PathBuf>,
    },
}

pub async fn run() -> Result<()> {
    let cli = Cli::parse();

    // Parse model from CLI flag
    let model_type = cli.model.as_ref().and_then(|m| ModelType::from_str(m));
    if cli.model.is_some() && model_type.is_none() {
        eprintln!("Unknown model: '{}'. Available models:", cli.model.as_ref().unwrap());
        eprintln!("  minilm-l6, minilm-l6-q, minilm-l12, minilm-l12-q, paraphrase-minilm");
        eprintln!("  bge-small, bge-small-q, bge-base, nomic-v1, nomic-v1.5, nomic-v1.5-q");
        eprintln!("  jina-code, e5-multilingual, mxbai-large, modernbert-large");
        std::process::exit(1);
    }

    // Set quiet mode if requested
    if cli.quiet {
        crate::output::set_quiet(true);
    }

    match cli.command {
        Commands::Search {
            query,
            max_results,
            per_file,
            content,
            scores,
            compact,
            sync,
            json,
            path,
            vector_only,
            rrf_k,
            rerank,
            rerank_top,
            filter_path,
        } => {
            // Auto-enable quiet mode for JSON output
            if json {
                crate::output::set_quiet(true);
            }
            crate::search::search(
                &query,
                max_results,
                per_file,
                content,
                scores,
                compact,
                sync,
                json,
                path,
                filter_path,
                model_type,
                vector_only,
                rrf_k,
                rerank,
                rerank_top,
            )
            .await
        }
        Commands::Index {
            path,
            dry_run,
            force,
        } => crate::index::index(path, dry_run, force, model_type).await,
        Commands::Serve { port, path } => crate::server::serve(port, path).await,
        Commands::List => crate::index::list().await,
        Commands::Stats { path } => crate::index::stats(path).await,
        Commands::Clear { path, yes } => crate::index::clear(path, yes).await,
        Commands::Doctor => crate::cli::doctor::run().await,
        Commands::Setup { model } => crate::cli::setup::run(model).await,
        Commands::Mcp { path } => crate::mcp::run_mcp_server(path).await,
    }
}

mod doctor;
mod setup;
