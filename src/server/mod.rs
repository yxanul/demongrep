use anyhow::Result;
use anyhow::anyhow;
use axum::{
    extract::{Json, State},
    http::StatusCode,
    routing::{get, post},
    Router,
};
use colored::Colorize;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::RwLock;

use crate::cache::FileMetaStore;
use crate::chunker::SemanticChunker;
use crate::embed::{EmbeddingService, ModelType};
use crate::file::FileWalker;
use crate::index::get_search_db_paths;
use crate::vectordb::VectorStore;
use crate::watch::{FileEvent, FileWatcher};

#[allow(dead_code)]
/// Database entry with its metadata
struct DatabaseEntry {
    store: VectorStore,
    db_path: PathBuf,
    db_type: DatabaseType,
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(dead_code)]
enum DatabaseType {
    Local,
    Global,
}

impl DatabaseType {
    #[allow(dead_code)]
    fn name(&self) -> &str {
        match self {
            DatabaseType::Local => "Local",
            DatabaseType::Global => "Global",
        }
    }
}

/// Shared server state with multi-database support
struct ServerState {
    /// Primary (local) database - can be written to via file watching
    local_store: Option<RwLock<VectorStore>>,
    local_db_path: Option<PathBuf>,
    
    /// Global database - read-only for searching
    global_store: Option<RwLock<VectorStore>>,
    #[allow(dead_code)]
    global_db_path: Option<PathBuf>,
    
    /// Shared services
    embedding_service: Mutex<EmbeddingService>,
    chunker: Mutex<SemanticChunker>,
    
    /// File metadata (only for local database)
    file_meta: Option<RwLock<FileMetaStore>>,
    
    /// Project root (for file watching)
    root: PathBuf,
}

impl ServerState {
    /// Search across all available databases
    async fn search_all(&self, query_embedding: &[f32], limit: usize) -> Result<Vec<crate::vectordb::SearchResult>> {
        let mut all_results = Vec::new();
        
        // Search local database
        if let Some(ref local_store) = self.local_store {
            let store = local_store.read().await;
            match store.search(query_embedding, limit) {
                Ok(mut results) => {
                    all_results.append(&mut results);
                }
                Err(e) => {
                    eprintln!("Warning: Local database search failed: {}", e);
                }
            }
        }
        
        // Search global database
        if let Some(ref global_store) = self.global_store {
            let store = global_store.read().await;
            match store.search(query_embedding, limit) {
                Ok(mut results) => {
                    all_results.append(&mut results);
                }
                Err(e) => {
                    eprintln!("Warning: Global database search failed: {}", e);
                }
            }
        }
        
        // Sort by score and limit
        all_results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        all_results.truncate(limit);
        
        Ok(all_results)
    }
    
    /// Get combined statistics
    async fn get_combined_stats(&self) -> CombinedStats {
        let mut total_chunks = 0;
        let mut total_files = 0;
        let mut local_chunks = 0;
        let mut local_files = 0;
        let mut global_chunks = 0;
        let mut global_files = 0;
        
        if let Some(ref local_store) = self.local_store {
            let store = local_store.read().await;
            if let Ok(stats) = store.stats() {
                local_chunks = stats.total_chunks;
                local_files = stats.total_files;
                total_chunks += stats.total_chunks;
                total_files += stats.total_files;
            }
        }
        
        if let Some(ref global_store) = self.global_store {
            let store = global_store.read().await;
            if let Ok(stats) = store.stats() {
                global_chunks = stats.total_chunks;
                global_files = stats.total_files;
                total_chunks += stats.total_chunks;
                total_files += stats.total_files;
            }
        }
        
        CombinedStats {
            total_chunks,
            total_files,
            local_chunks,
            local_files,
            global_chunks,
            global_files,
        }
    }
}

struct CombinedStats {
    total_chunks: usize,
    total_files: usize,
    local_chunks: usize,
    local_files: usize,
    global_chunks: usize,
    global_files: usize,
}

/// Search request body
#[derive(Debug, Deserialize)]
struct SearchRequest {
    query: String,
    #[serde(default = "default_limit")]
    limit: usize,
    #[serde(default)]
    path: Option<String>,
}

fn default_limit() -> usize {
    25
}

/// Search response
#[derive(Debug, Serialize)]
struct SearchResponse {
    results: Vec<SearchResult>,
    query: String,
    took_ms: u64,
    databases_searched: usize,
}

#[derive(Debug, Serialize)]
struct SearchResult {
    path: String,
    content: String,
    start_line: usize,
    end_line: usize,
    kind: String,
    score: f32,
    database: String,
}

/// Health check response
#[derive(Debug, Serialize)]
struct HealthResponse {
    status: String,
    total_files: usize,
    total_chunks: usize,
    local_files: usize,
    local_chunks: usize,
    global_files: usize,
    global_chunks: usize,
    model: String,
    databases_available: usize,
}

/// Index status response
#[derive(Debug, Serialize)]
struct StatusResponse {
    total_files: usize,
    total_chunks: usize,
    local_files: usize,
    local_chunks: usize,
    global_files: usize,
    global_chunks: usize,
    model: String,
    dimensions: usize,
    databases_available: usize,
}

/// Run the background server with live file watching and dual-database support
///
/// Improvements over osgrep:
/// 1. Native Rust HTTP server (axum) - faster than Node.js
/// 2. Built-in file watching with native notify crate
/// 3. Two-level change detection (mtime + hash)
/// 4. Tracks chunk IDs for efficient incremental updates
/// 5. **Dual-database support**: Searches both local and global databases
pub async fn serve(port: u16, path: Option<PathBuf>) -> Result<()> {
    let root = path.clone().unwrap_or_else(|| PathBuf::from(".")).canonicalize()?;

    println!("{}", "🚀 Demongrep Server".bright_cyan().bold());
    println!("{}", "=".repeat(60));
    println!("📂 Root: {}", root.display());
    println!("🌐 Port: {}", port);

    // Get all available database paths
    let db_paths = get_search_db_paths(path)?;
    
    if db_paths.is_empty() {
        println!("\n{}", "❌ No databases found!".red());
        println!("   Run {} or {} first", 
            "demongrep index".bright_cyan(),
            "demongrep index --global".bright_cyan()
        );
        return Err(anyhow!("No databases found"));
    }

    // Identify local and global databases
    let mut local_db_path: Option<PathBuf> = None;
    let mut global_db_path: Option<PathBuf> = None;
    
    for db_path in db_paths {
        if db_path.ends_with(".demongrep.db") {
            local_db_path = Some(db_path);
        } else {
            global_db_path = Some(db_path);
        }
    }

    println!("\n{}", "📚 Available Databases:".bright_green());
    if let Some(ref path) = local_db_path {
        println!("   📍 Local:  {}", path.display());
    }
    if let Some(ref path) = global_db_path {
        println!("   🌍 Global: {}", path.display());
    }

    // Initialize embedding service
    let model_type = ModelType::default();
    println!("\n🔄 Loading embedding model...");
    let embedding_service = EmbeddingService::with_model(model_type)?;
    let dimensions = embedding_service.dimensions();
    println!("   Model: {} ({} dims)", model_type.name(), dimensions);

    // Load local database (if exists)
    let (local_store, file_meta) = if let Some(ref local_path) = local_db_path {
        let file_meta = FileMetaStore::load_or_create(local_path, model_type.short_name(), dimensions)?;
        let store = VectorStore::new(local_path, dimensions)?;
        let stats = store.stats()?;
        
        if stats.total_chunks == 0 {
            println!("\n{}", "📦 Local database empty, performing initial index...".yellow());
            let (store, file_meta) = initial_index(
                root.clone(),
                local_path.clone(),
                model_type,
            ).await?;
            (Some(store), Some(file_meta))
        } else {
            println!("   ✅ Local: {} chunks from {} files", stats.total_chunks, stats.total_files);
            (Some(store), Some(file_meta))
        }
    } else {
        (None, None)
    };

    // Load global database (if exists) - read-only
    let global_store = if let Some(ref global_path) = global_db_path {
        match VectorStore::new(global_path, dimensions) {
            Ok(store) => {
                let stats = store.stats()?;
                println!("   ✅ Global: {} chunks from {} files", stats.total_chunks, stats.total_files);
                Some(store)
            }
            Err(e) => {
                eprintln!("   ⚠️  Could not load global database: {}", e);
                None
            }
        }
    } else {
        None
    };

    // Build server state
    let state = Arc::new(ServerState {
        local_store: local_store.map(RwLock::new),
        local_db_path,
        global_store: global_store.map(RwLock::new),
        global_db_path,
        embedding_service: Mutex::new(embedding_service),
        chunker: Mutex::new(SemanticChunker::new(100, 2000, 10)),
        file_meta: file_meta.map(RwLock::new),
        root: root.clone(),
    });

    start_server(state, port, root).await
}

async fn initial_index(
    root: PathBuf,
    db_path: PathBuf,
    model_type: ModelType,
) -> Result<(VectorStore, FileMetaStore)> {
    // Clear existing database if any
    if db_path.exists() {
        std::fs::remove_dir_all(&db_path)?;
    }

    // File discovery
    let walker = FileWalker::new(root.clone());
    let (files, _stats) = walker.walk()?;
    println!("  Found {} files", files.len());

    if files.is_empty() {
        let store = VectorStore::new(&db_path, model_type.dimensions())?;
        let file_meta = FileMetaStore::new(model_type.short_name().to_string(), model_type.dimensions());
        return Ok((store, file_meta));
    }

    // Chunking
    let mut chunker = SemanticChunker::new(100, 2000, 10);
    let mut all_chunks = Vec::new();
    let mut file_chunks: HashMap<String, Vec<crate::chunker::Chunk>> = HashMap::new();

    for file in &files {
        let source_code = match std::fs::read_to_string(&file.path) {
            Ok(content) => content,
            Err(_) => continue,
        };
        let chunks = chunker.chunk_semantic(file.language, &file.path, &source_code)?;
        let path_str = file.path.to_string_lossy().to_string();
        file_chunks.insert(path_str, chunks.clone());
        all_chunks.extend(chunks);
    }
    println!("  Created {} chunks", all_chunks.len());

    // Embedding
    let mut embedding_service = EmbeddingService::with_model(model_type)?;
    let embedded_chunks = embedding_service.embed_chunks(all_chunks)?;
    println!("  Generated {} embeddings", embedded_chunks.len());

    // Storage
    let mut store = VectorStore::new(&db_path, model_type.dimensions())?;
    let chunk_ids = store.insert_chunks_with_ids(embedded_chunks)?;
    store.build_index()?;

    // Build file metadata
    let mut file_meta = FileMetaStore::new(model_type.short_name().to_string(), model_type.dimensions());

    let mut chunk_id_iter = chunk_ids.iter();
    for file in &files {
        let path_str = file.path.to_string_lossy().to_string();
        if let Some(chunks) = file_chunks.get(&path_str) {
            let ids: Vec<u32> = chunk_id_iter.by_ref().take(chunks.len()).copied().collect();
            file_meta.update_file(&file.path, ids)?;
        }
    }
    file_meta.mark_full_index();
    file_meta.save(&db_path)?;

    println!("  ✅ Initial index complete");

    Ok((store, file_meta))
}

async fn start_server(state: Arc<ServerState>, port: u16, root: PathBuf) -> Result<()> {
    // Check if we have a local database BEFORE building router
    let has_local_store = state.local_store.is_some();
    
    // Start file watcher in background (only if we have a local database)
    if has_local_store {
        let watcher_state = state.clone();
        let watcher_root = root.clone();
        tokio::spawn(async move {
            if let Err(e) = run_file_watcher(watcher_state, watcher_root).await {
                eprintln!("File watcher error: {}", e);
            }
        });
    } else {
        println!("\n{}", "ℹ️  No local database - file watching disabled".dimmed());
    }

    // Build HTTP router
    let app = Router::new()
        .route("/health", get(health_handler))
        .route("/status", get(status_handler))
        .route("/search", post(search_handler))
        .with_state(state);

    let addr = format!("127.0.0.1:{}", port);
    println!("\n{}", "🌐 Server ready!".bright_green().bold());
    println!("  Health: http://{}/health", addr);
    println!("  Search: POST http://{}/search", addr);
    if has_local_store {
        println!("\n{}", "👀 Watching for file changes in local database...".dimmed());
    }

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn run_file_watcher(state: Arc<ServerState>, root: PathBuf) -> Result<()> {
    let mut watcher = FileWatcher::new(root);
    watcher.start(300)?; // 300ms debounce

    loop {
        let events = watcher.wait_for_events(Duration::from_secs(1));

        if events.is_empty() {
            continue;
        }

        println!("\n📁 {} file change(s) detected", events.len());

        for event in events {
            match event {
                FileEvent::Modified(path) => {
                    if let Err(e) = handle_file_modified(&state, &path).await {
                        eprintln!("  ❌ Error processing {}: {}", path.display(), e);
                    }
                }
                FileEvent::Deleted(path) => {
                    if let Err(e) = handle_file_deleted(&state, &path).await {
                        eprintln!("  ❌ Error processing deletion {}: {}", path.display(), e);
                    }
                }
                FileEvent::Renamed(from, to) => {
                    // Treat as delete + create
                    let _ = handle_file_deleted(&state, &from).await;
                    let _ = handle_file_modified(&state, &to).await;
                }
            }
        }

        // Rebuild index after changes (only for local database)
        if let Some(ref local_store) = state.local_store {
            let mut store = local_store.write().await;
            if !store.is_indexed() {
                println!("  🔨 Rebuilding local index...");
                store.build_index()?;
                println!("  ✅ Index updated");
            }
        }

        // Save metadata (only for local database)
        if let (Some(ref file_meta), Some(ref db_path)) = (&state.file_meta, &state.local_db_path) {
            let file_meta = file_meta.read().await;
            file_meta.save(db_path)?;
        }
    }
}

async fn handle_file_modified(state: &ServerState, path: &PathBuf) -> Result<()> {
    // Only handle files in local database
    let file_meta = state.file_meta.as_ref()
        .ok_or_else(|| anyhow!("No local database available"))?;
    
    // Check if file needs re-indexing
    let file_meta_read: tokio::sync::RwLockReadGuard<'_, FileMetaStore> = file_meta.read().await;
    let (needs_reindex, old_chunk_ids) = file_meta_read.check_file(path)?;
    drop(file_meta_read);

    if !needs_reindex {
        return Ok(());
    }

    println!("  📝 Re-indexing: {}", path.display());

    // Delete old chunks if any
    if !old_chunk_ids.is_empty() {
        if let Some(ref local_store) = state.local_store {
            let mut store = local_store.write().await;
            store.delete_chunks(&old_chunk_ids)?;
        }
    }

    // Read and chunk file
    let source_code = std::fs::read_to_string(path)?;
    let language = crate::file::Language::from_path(path);

    let chunks = {
        let mut chunker = state.chunker.lock().unwrap();
        chunker.chunk_semantic(language, path, &source_code)?
    };

    if chunks.is_empty() {
        // Update metadata with no chunks
        let mut file_meta_write: tokio::sync::RwLockWriteGuard<'_, FileMetaStore> = file_meta.write().await;
        file_meta_write.update_file(path, vec![])?;
        return Ok(());
    }

    // Embed chunks
    let embedded_chunks = {
        let mut embedding_service = state.embedding_service.lock().unwrap();
        embedding_service.embed_chunks(chunks)?
    };

    // Insert into store
    let chunk_ids = if let Some(ref local_store) = state.local_store {
        let mut store = local_store.write().await;
        store.insert_chunks_with_ids(embedded_chunks)?
    } else {
        vec![]
    };

    // Update metadata
    let mut file_meta_write: tokio::sync::RwLockWriteGuard<'_, FileMetaStore> = file_meta.write().await;
    file_meta_write.update_file(path, chunk_ids)?;

    Ok(())
}

async fn handle_file_deleted(state: &ServerState, path: &PathBuf) -> Result<()> {
    // Only handle files in local database
    let file_meta = state.file_meta.as_ref()
        .ok_or_else(|| anyhow!("No local database available"))?;
    
    let mut file_meta_write: tokio::sync::RwLockWriteGuard<'_, FileMetaStore> = file_meta.write().await;

    if let Some(meta) = file_meta_write.remove_file(path) {
        if !meta.chunk_ids.is_empty() {
            println!("  🗑️  Removing: {} ({} chunks)", path.display(), meta.chunk_ids.len());
            if let Some(ref local_store) = state.local_store {
                let mut store = local_store.write().await;
                store.delete_chunks(&meta.chunk_ids)?;
            }
        }
    }

    Ok(())
}

// HTTP Handlers

async fn health_handler(
    State(state): State<Arc<ServerState>>,
) -> Json<HealthResponse> {
    let stats = state.get_combined_stats().await;
    
    let model_name = if let Some(ref file_meta) = state.file_meta {
        let meta = file_meta.read().await;
        meta.model_name.clone()
    } else {
        ModelType::default().name().to_string()
    };
    
    let databases_available = 
        (if state.local_store.is_some() { 1 } else { 0 }) +
        (if state.global_store.is_some() { 1 } else { 0 });

    Json(HealthResponse {
        status: "ready".to_string(),
        total_files: stats.total_files,
        total_chunks: stats.total_chunks,
        local_files: stats.local_files,
        local_chunks: stats.local_chunks,
        global_files: stats.global_files,
        global_chunks: stats.global_chunks,
        model: model_name,
        databases_available,
    })
}

async fn status_handler(
    State(state): State<Arc<ServerState>>,
) -> Json<StatusResponse> {
    let stats = state.get_combined_stats().await;
    
    let (model_name, dimensions) = if let Some(ref file_meta) = state.file_meta {
        let meta = file_meta.read().await;
        (meta.model_name.clone(), meta.dimensions)
    } else {
        let model = ModelType::default();
        (model.name().to_string(), model.dimensions())
    };
    
    let databases_available = 
        (if state.local_store.is_some() { 1 } else { 0 }) +
        (if state.global_store.is_some() { 1 } else { 0 });

    Json(StatusResponse {
        total_files: stats.total_files,
        total_chunks: stats.total_chunks,
        local_files: stats.local_files,
        local_chunks: stats.local_chunks,
        global_files: stats.global_files,
        global_chunks: stats.global_chunks,
        model: model_name,
        dimensions,
        databases_available,
    })
}

async fn search_handler(
    State(state): State<Arc<ServerState>>,
    Json(req): Json<SearchRequest>,
) -> Result<Json<SearchResponse>, (StatusCode, String)> {
    let start = std::time::Instant::now();

    // Embed query
    let query_embedding = {
        let mut embedding_service = state.embedding_service.lock().unwrap();
        embedding_service.embed_query(&req.query)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    };

    // Search across all databases
    let results = state.search_all(&query_embedding, req.limit).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    
    let databases_searched = 
        (if state.local_store.is_some() { 1 } else { 0 }) +
        (if state.global_store.is_some() { 1 } else { 0 });

    // Convert to response format
    let search_results: Vec<SearchResult> = results
        .into_iter()
        .filter(|r| {
            // Filter by path if specified
            if let Some(ref path_filter) = req.path {
                r.path.contains(path_filter)
            } else {
                true
            }
        })
        .map(|r| {
            // Determine which database this result came from
            let database = if let Some(ref _local_path) = state.local_db_path {
                if r.path.starts_with(state.root.to_str().unwrap_or("")) {
                    "local".to_string()
                } else {
                    "global".to_string()
                }
            } else {
                "global".to_string()
            };
            
            // Make path relative to root
            let rel_path = r.path.strip_prefix(state.root.to_str().unwrap_or(""))
                .unwrap_or(&r.path)
                .trim_start_matches('/')
                .to_string();

            SearchResult {
                path: rel_path,
                content: truncate_content(&r.content, 200),
                start_line: r.start_line,
                end_line: r.end_line,
                kind: r.kind,
                score: r.score,
                database,
            }
        })
        .collect();

    let took_ms = start.elapsed().as_millis() as u64;

    Ok(Json(SearchResponse {
        results: search_results,
        query: req.query,
        took_ms,
        databases_searched,
    }))
}

fn truncate_content(content: &str, max_len: usize) -> String {
    if content.len() <= max_len {
        content.to_string()
    } else {
        format!("{}...", &content[..max_len])
    }
}
