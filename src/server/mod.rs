use anyhow::Result;
use std::path::PathBuf;

/// Run the background server
pub async fn serve(port: u16, path: Option<PathBuf>) -> Result<()> {
    println!("🚀 Starting server on port {}...", port);

    // TODO: Implement HTTP server with axum

    Ok(())
}
