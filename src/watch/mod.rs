use anyhow::Result;
use std::path::PathBuf;

/// File watcher for incremental indexing
pub struct FileWatcher {
    root: PathBuf,
}

impl FileWatcher {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub async fn watch(&self) -> Result<()> {
        // TODO: Implement file watching with notify
        Ok(())
    }
}
