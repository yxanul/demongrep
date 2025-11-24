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

use anyhow::Result;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "demongrep=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    info!("Starting demongrep v{}", env!("CARGO_PKG_VERSION"));

    // Parse CLI and execute command
    cli::run().await
}
