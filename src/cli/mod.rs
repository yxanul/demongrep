use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

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

    /// Override default store name
    #[arg(long, global = true)]
    pub store: Option<String>,
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
}

pub async fn run() -> Result<()> {
    let cli = Cli::parse();

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
        } => {
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
            )
            .await
        }
        Commands::Index {
            path,
            dry_run,
            force,
        } => crate::index::index(path, dry_run, force).await,
        Commands::Serve { port, path } => crate::server::serve(port, path).await,
        Commands::List => crate::index::list().await,
        Commands::Stats { path } => crate::index::stats(path).await,
        Commands::Clear { path, yes } => crate::index::clear(path, yes).await,
        Commands::Doctor => crate::cli::doctor::run().await,
        Commands::Setup { model } => crate::cli::setup::run(model).await,
    }
}

mod doctor;
mod setup;
