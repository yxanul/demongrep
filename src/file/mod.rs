use anyhow::Result;
use ignore::WalkBuilder;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

mod binary;
mod language;

pub use binary::is_binary_file;
pub use language::Language;

/// Information about a discovered file
#[derive(Debug, Clone)]
pub struct FileInfo {
    pub path: PathBuf,
    pub language: Language,
    pub size: u64,
}

/// Statistics about walked files
#[derive(Debug, Default, Clone)]
pub struct WalkStats {
    pub total_files: usize,
    pub indexable_files: usize,
    pub skipped_binary: usize,
    pub skipped_ignored: usize,
    pub files_by_language: HashMap<Language, usize>,
    pub total_size_bytes: u64,
}

impl WalkStats {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_file(&mut self, file: &FileInfo) {
        self.indexable_files += 1;
        self.total_size_bytes += file.size;
        *self.files_by_language.entry(file.language).or_insert(0) += 1;
    }

    pub fn add_skipped_binary(&mut self) {
        self.skipped_binary += 1;
    }

    pub fn total_size_mb(&self) -> f64 {
        self.total_size_bytes as f64 / (1024.0 * 1024.0)
    }

    pub fn print_summary(&self) {
        info!("File discovery complete:");
        info!("  Total files found: {}", self.total_files);
        info!("  Indexable files: {}", self.indexable_files);
        info!("  Binary/skipped: {}", self.skipped_binary);
        info!("  Total size: {:.2} MB", self.total_size_mb());

        if !self.files_by_language.is_empty() {
            info!("  Files by language:");
            let mut langs: Vec<_> = self.files_by_language.iter().collect();
            langs.sort_by(|a, b| b.1.cmp(a.1)); // Sort by count descending
            for (lang, count) in langs.iter().take(10) {
                info!("    {}: {}", lang.name(), count);
            }
        }
    }
}

/// Smart file walker that respects .gitignore and .demongrepignore
pub struct FileWalker {
    root: PathBuf,
    respect_gitignore: bool,
    include_hidden: bool,
}

impl FileWalker {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            respect_gitignore: true,
            include_hidden: false,
        }
    }

    /// Set whether to respect .gitignore files (default: true)
    pub fn respect_gitignore(mut self, respect: bool) -> Self {
        self.respect_gitignore = respect;
        self
    }

    /// Set whether to include hidden files (default: false)
    pub fn include_hidden(mut self, include: bool) -> Self {
        self.include_hidden = include;
        self
    }

    /// Walk files, returning detailed file information
    pub fn walk(&self) -> Result<(Vec<FileInfo>, WalkStats)> {
        let mut files = Vec::new();
        let mut stats = WalkStats::new();

        debug!("Starting file walk in: {}", self.root.display());

        let mut builder = WalkBuilder::new(&self.root);
        builder
            .git_ignore(self.respect_gitignore)
            .git_global(self.respect_gitignore)
            .git_exclude(self.respect_gitignore)
            .hidden(!self.include_hidden)
            .add_custom_ignore_filename(".demongrepignore")
            .add_custom_ignore_filename(".osgrepignore"); // Compatibility with osgrep

        for result in builder.build() {
            match result {
                Ok(entry) => {
                    stats.total_files += 1;

                    // Only process files (not directories)
                    let file_type = entry.file_type();
                    if file_type.is_none() || !file_type.unwrap().is_file() {
                        continue;
                    }

                    let path = entry.path();

                    // Check if file should be skipped
                    if self.should_skip(path) {
                        stats.add_skipped_binary();
                        debug!("Skipping file: {}", path.display());
                        continue;
                    }

                    // Get file info
                    let language = Language::from_path(path);

                    // Skip unknown/non-indexable files
                    if !language.is_indexable() {
                        stats.add_skipped_binary();
                        continue;
                    }

                    let size = entry.metadata().ok().map(|m| m.len()).unwrap_or(0);

                    let file_info = FileInfo {
                        path: path.to_path_buf(),
                        language,
                        size,
                    };

                    stats.add_file(&file_info);
                    files.push(file_info);
                }
                Err(err) => {
                    warn!("Error walking file: {}", err);
                }
            }
        }

        stats.print_summary();

        Ok((files, stats))
    }

    /// Walk files, returning just the paths (simpler API)
    pub fn walk_paths(&self) -> Result<Vec<PathBuf>> {
        let (files, _) = self.walk()?;
        Ok(files.into_iter().map(|f| f.path).collect())
    }

    /// Check if a file should be skipped
    fn should_skip(&self, path: &Path) -> bool {
        // Check for vendor/generated directories in path
        if self.is_in_excluded_dir(path) {
            return true;
        }

        // Check if file is binary
        is_binary_file(path)
    }

    /// Check if path is in an excluded directory
    fn is_in_excluded_dir(&self, path: &Path) -> bool {
        path.components().any(|c| {
            matches!(
                c.as_os_str().to_str().unwrap_or(""),
                // Build artifacts
                "node_modules" | "target" | "dist" | "build" | "out"
                // Version control
                | ".git" | ".svn" | ".hg"
                // Python
                | "__pycache__" | ".pytest_cache" | ".tox" | "venv" | ".venv"
                // Ruby
                | "vendor" | ".bundle"
                // Java
                | ".gradle" | ".m2"
                // IDE
                | ".idea" | ".vscode" | ".vs"
                // Other
                | "coverage" | ".nyc_output" | ".cache"
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_file_walker_basic() {
        let dir = TempDir::new().unwrap();

        // Create some test files
        fs::write(dir.path().join("test.rs"), "fn main() {}").unwrap();
        fs::write(dir.path().join("test.py"), "print('hello')").unwrap();
        fs::write(dir.path().join("README.md"), "# Test").unwrap();

        let walker = FileWalker::new(dir.path());
        let (files, stats) = walker.walk().unwrap();

        assert_eq!(files.len(), 3);
        assert_eq!(stats.indexable_files, 3);
    }

    #[test]
    fn test_skip_binary_files() {
        let dir = TempDir::new().unwrap();

        // Create text file
        fs::write(dir.path().join("test.txt"), "hello world").unwrap();

        // Create binary file
        fs::write(dir.path().join("test.bin"), &[0u8, 1, 2, 3, 255]).unwrap();

        let walker = FileWalker::new(dir.path());
        let (files, stats) = walker.walk().unwrap();

        // Should only get the text file
        assert_eq!(files.len(), 1);
        assert!(stats.skipped_binary > 0);
    }

    #[test]
    fn test_language_detection() {
        let dir = TempDir::new().unwrap();

        fs::write(dir.path().join("main.rs"), "fn main() {}").unwrap();
        fs::write(dir.path().join("script.py"), "pass").unwrap();
        fs::write(dir.path().join("app.js"), "console.log()").unwrap();

        let walker = FileWalker::new(dir.path());
        let (files, stats) = walker.walk().unwrap();

        assert_eq!(files.len(), 3);
        assert_eq!(stats.files_by_language.get(&Language::Rust), Some(&1));
        assert_eq!(stats.files_by_language.get(&Language::Python), Some(&1));
        assert_eq!(stats.files_by_language.get(&Language::JavaScript), Some(&1));
    }

    #[test]
    fn test_excluded_directories() {
        let dir = TempDir::new().unwrap();

        // Create file in excluded directory
        let node_modules = dir.path().join("node_modules");
        fs::create_dir(&node_modules).unwrap();
        fs::write(node_modules.join("package.js"), "test").unwrap();

        // Create normal file
        fs::write(dir.path().join("index.js"), "test").unwrap();

        let walker = FileWalker::new(dir.path());
        let (files, _) = walker.walk().unwrap();

        // Should only get index.js, not the node_modules file
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path.file_name().unwrap(), "index.js");
    }
}
