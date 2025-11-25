mod cli;
mod config;
mod chunker;
mod embed;
mod rerank;
mod vectordb;
mod cache;
mod index;
mod search;
mod watch;
mod server;
mod bench;
mod file;
mod fts;
mod mcp;
mod output;

use anyhow::Result;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<()> {
    // Check for quiet mode early (before tracing init)
    let args: Vec<String> = std::env::args().collect();
    let is_quiet = args.iter().any(|a| a == "-q" || a == "--quiet");
    let is_json = args.iter().any(|a| a == "--json");

    // Skip tracing in quiet mode or JSON output
    if !is_quiet && !is_json {
        // Initialize tracing
        tracing_subscriber::registry()
            .with(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| "demongrep=info".into()),
            )
            .with(tracing_subscriber::fmt::layer())
            .init();

        info!("Starting demongrep v{}", env!("CARGO_PKG_VERSION"));
    }

    // Parse CLI and execute command
    cli::run().await
}
