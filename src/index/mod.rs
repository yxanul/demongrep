use anyhow::Result;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use colored::Colorize;
use indicatif::{ProgressBar, ProgressStyle};
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::chunker::SemanticChunker;
use crate::database::DatabaseManager;
use crate::embed::{EmbeddingService, ModelType};
use crate::file::FileWalker;
use crate::fts::FtsStore;
use crate::vectordb::VectorStore;

/// Get the database path for indexing
fn get_index_db_path(path: Option<PathBuf>, global: bool) -> Result<PathBuf> {
    let project_path = path.unwrap_or_else(|| PathBuf::from("."));
    let canonical_path = project_path.canonicalize()?;

    if global {
        // Global mode: use home directory with project hash
        let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?;
        
        // Create hash of canonical path
        let mut hasher = DefaultHasher::new();
        canonical_path.hash(&mut hasher);
        let hash = hasher.finish();
        
        let global_base = home.join(".demongrep").join("stores");
        std::fs::create_dir_all(&global_base)?;
        
        let db_path = global_base.join(format!("{:x}", hash));
        
        // Save project mapping for later reference
        save_project_mapping(&canonical_path, &db_path)?;
        
        Ok(db_path)
    } else {
        // Local mode: use project directory
        Ok(canonical_path.join(".demongrep.db"))
    }
}

/// Get all database paths to search (local + global)
pub fn get_search_db_paths(path: Option<PathBuf>) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    
    let project_path = path.unwrap_or_else(|| PathBuf::from("."));
    let canonical_path = project_path.canonicalize()?;
    
    // 1. Check local database
    let local_db = canonical_path.join(".demongrep.db");
    if local_db.exists() {
        paths.push(local_db);
    }
    
    // 2. Check global database
    if let Some(home) = dirs::home_dir() {
        let mut hasher = DefaultHasher::new();
        canonical_path.hash(&mut hasher);
        let hash = hasher.finish();
        
        let global_db = home.join(".demongrep").join("stores").join(format!("{:x}", hash));
        if global_db.exists() {
            paths.push(global_db);
        }
    }
    
    Ok(paths)
}

/// Save project -> database mapping
fn save_project_mapping(project_path: &Path, db_path: &Path) -> Result<()> {
    let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?;
    let config_dir = home.join(".demongrep");
    std::fs::create_dir_all(&config_dir)?;
    
    let mapping_file = config_dir.join("projects.json");
    
    // Load existing mappings
    let mut mappings: std::collections::HashMap<String, String> = if mapping_file.exists() {
        serde_json::from_str(&std::fs::read_to_string(&mapping_file)?)?
    } else {
        std::collections::HashMap::new()
    };
    
    // Add new mapping
    mappings.insert(
        project_path.to_string_lossy().to_string(),
        db_path.to_string_lossy().to_string()
    );
    
    // Write back
    std::fs::write(&mapping_file, serde_json::to_string_pretty(&mappings)?)?;
    
    Ok(())
}

/// Find databases for a project by name (searches in projects.json)
fn find_project_databases(project_name: &str) -> Result<Vec<PathBuf>> {
    let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?;
    let mapping_file = home.join(".demongrep").join("projects.json");
    
    if !mapping_file.exists() {
        return Ok(Vec::new());
    }
    
    let content = std::fs::read_to_string(&mapping_file)?;
    let mappings: std::collections::HashMap<String, String> = serde_json::from_str(&content)?;
    
    let mut found_paths = Vec::new();
    
    // Search for matching project (by name or full path)
    for (project_path, db_path_str) in mappings {
        // Match by full path or by directory name
        let matches = project_path.contains(project_name) || 
                     PathBuf::from(&project_path)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .map(|n| n.contains(project_name))
                        .unwrap_or(false);
        
        if matches {
            let db_path = PathBuf::from(&db_path_str);
            if db_path.exists() {
                found_paths.push(db_path);
            }
            
            // Also check for local database at project path
            let project_pb = PathBuf::from(&project_path);
            if project_pb.exists() {
                let local_db = project_pb.join(".demongrep.db");
                if local_db.exists() {
                    found_paths.push(local_db);
                }
            }
        }
    }
    
    Ok(found_paths)
}

/// Remove a project from the projects.json mapping
/// Remove entries from projects.json for deleted database paths
fn cleanup_project_mappings(deleted_db_paths: &[PathBuf]) -> Result<()> {
    let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?;
    let mapping_file = home.join(".demongrep").join("projects.json");
    
    if !mapping_file.exists() {
        return Ok(());
    }
    
    let content = std::fs::read_to_string(&mapping_file)?;
    let mut mappings: std::collections::HashMap<String, String> = serde_json::from_str(&content)?;
    
    // Remove entries where the database path matches any deleted path
    let original_len = mappings.len();
    mappings.retain(|_project_path, db_path_str| {
        let db_path = PathBuf::from(db_path_str.as_str());
        // Keep entries that DON'T match any deleted path
        !deleted_db_paths.iter().any(|deleted| deleted == &db_path)
    });
    
    // Only write back if we actually removed something
    if mappings.len() < original_len {
        std::fs::write(&mapping_file, serde_json::to_string_pretty(&mappings)?)?;
    }
    
    Ok(())
}

/// Remove project from mapping by name (legacy - used when --project flag is passed)
fn remove_from_project_mapping(project_name: &str) -> Result<()> {
    let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?;
    let mapping_file = home.join(".demongrep").join("projects.json");
    
    if !mapping_file.exists() {
        return Ok(());
    }
    
    let content = std::fs::read_to_string(&mapping_file)?;
    let mut mappings: std::collections::HashMap<String, String> = serde_json::from_str(&content)?;
    
    // Remove matching projects
    mappings.retain(|project_path, _| {
        let matches = project_path.contains(project_name) || 
                     PathBuf::from(project_path)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .map(|n| n.contains(project_name))
                        .unwrap_or(false);
        !matches  // Keep if NOT matching
    });
    
    // Write back
    std::fs::write(&mapping_file, serde_json::to_string_pretty(&mappings)?)?;
    
    Ok(())
}

/// Index a repository
pub async fn index(path: Option<PathBuf>, dry_run: bool, _force: bool, global: bool, model: Option<ModelType>) -> Result<()> {
    let project_path = path.clone().unwrap_or_else(|| PathBuf::from("."));
    let canonical_path = project_path.canonicalize()?;
    
    // Check for existing databases (local and global)
    let local_db_path = canonical_path.join(".demongrep.db");
    let global_db_path = if let Some(home) = dirs::home_dir() {
        let mut hasher = DefaultHasher::new();
        canonical_path.hash(&mut hasher);
        let hash = hasher.finish();
        Some(home.join(".demongrep").join("stores").join(format!("{:x}", hash)))
    } else {
        None
    };
    
    let local_exists = local_db_path.exists();
    let global_exists = global_db_path.as_ref().map(|p| p.exists()).unwrap_or(false);
    
    // Enforce exclusivity: can't have both local AND global
    if local_exists && global_exists {
        println!("\n{}", "⚠️  Both local and global databases exist!".yellow());
        println!("   Local:  {}", local_db_path.display());
        if let Some(ref gp) = global_db_path {
            println!("   Global: {}", gp.display());
        }
        println!("\n{}", "Please run 'demongrep clear' first to choose which one to keep".bright_yellow());
        return Err(anyhow::anyhow!("Cannot have both local and global databases"));
    }
    
    // If user requests global but local exists, error
    if global && local_exists {
        println!("\n{}", "⚠️  Local database already exists!".yellow());
        println!("   Local: {}", local_db_path.display());
        println!("\n{}", "Cannot create global database when local exists.".yellow());
        println!("   Run {} first to remove local database", "demongrep clear".bright_cyan());
        return Err(anyhow::anyhow!("Local database already exists"));
    }
    
    // If user requests local but global exists, error
    if !global && global_exists {
        println!("\n{}", "⚠️  Global database already exists!".yellow());
        if let Some(ref gp) = global_db_path {
            println!("   Global: {}", gp.display());
        }
        println!("\n{}", "Cannot create local database when global exists.".yellow());
        println!("   • Use {} to update the global database, or", "demongrep index --global".bright_cyan());
        println!("   • Run {} first to remove global database", "demongrep clear --global".bright_cyan());
        return Err(anyhow::anyhow!("Global database already exists"));
    }
    
    let db_path = get_index_db_path(Some(canonical_path.clone()), global)?;
    let model_type = model.unwrap_or_default();

    println!("{}", "🚀 Demongrep Indexer".bright_cyan().bold());
    println!("{}", "=".repeat(60));
    println!("📂 Project: {}", project_path.display());
    println!("💾 Database: {}", db_path.display());
    if global {
        println!("🌍 Mode: Global (shared across workspaces)");
    } else {
        println!("📍 Mode: Local (project-specific)");
    }
    println!("🧠 Model: {} ({} dims)", model_type.name(), model_type.dimensions());

    if dry_run {
        println!("\n{}", "🔍 DRY RUN MODE".bright_yellow());
    }

    // Check if this is incremental or full index
    let is_incremental = db_path.exists();
    
    if is_incremental {
        println!("🔄 Mode: Incremental (updating existing database)");
    } else {
        println!("🆕 Mode: Full (creating new database)");
    }

    // Phase 1: File Discovery
    println!("\n{}", "Phase 1: File Discovery".bright_cyan());
    println!("{}", "-".repeat(60));

    let start = Instant::now();
    let walker = FileWalker::new(project_path.clone());
    let (files, stats) = walker.walk()?;
    let discovery_duration = start.elapsed();

    println!("✅ Found {} indexable files in {:?}", files.len(), discovery_duration);
    println!("   Total files scanned: {}", stats.total_files);
    println!("   Binary/skipped: {}", stats.skipped_binary);
    println!("   Total size: {:.2} MB", stats.total_size_mb());

    if files.is_empty() {
        println!("\n{}", "No files to index!".yellow());
        return Ok(());
    }

    if dry_run {
        println!("\n{}", "Dry run complete!".green());
        return Ok(());
    }

    // Open or create database
    let mut store = VectorStore::new(&db_path, model_type.dimensions())?;
    
    // Check database metadata for model changes
    if is_incremental {
        let db_meta = store.get_db_metadata(model_type.name(), model_type.dimensions())?;
        if db_meta.model_name != model_type.name() || db_meta.dimensions != model_type.dimensions() {
            println!("\n{}", "⚠️  Model changed! Full re-index required.".yellow());
            println!("   Old: {} ({} dims)", db_meta.model_name, db_meta.dimensions);
            println!("   New: {} ({} dims)", model_type.name(), model_type.dimensions());
            println!("\n   Run {} first", "demongrep clear".bright_cyan());
            return Err(anyhow::anyhow!("Model mismatch - clear database first"));
        }
    }
    
    // Determine which files need indexing
    let mut files_to_index = Vec::new();
    let mut files_to_delete = Vec::new();
    let mut unchanged_count = 0;
    
    if is_incremental {
        println!("\n{}", "🔍 Checking for changes...".bright_cyan());
        
        // Check each discovered file
        for file in &files {
            match store.check_file_needs_reindex(&file.path) {
                Ok((needs_reindex, old_chunk_ids)) => {
                    if needs_reindex {
                        files_to_index.push((file.clone(), old_chunk_ids));
                    } else {
                        unchanged_count += 1;
                    }
                }
                Err(_) => {
                    // Error checking file, index it
                    files_to_index.push((file.clone(), vec![]));
                }
            }
        }
        
        // Find deleted files
        let deleted = store.find_deleted_files()?;
        for (path, chunk_ids) in deleted {
            files_to_delete.push((PathBuf::from(path), chunk_ids));
        }
        
        println!("   📊 Status:");
        println!("      Unchanged: {}", unchanged_count);
        println!("      Changed/New: {}", files_to_index.len());
        println!("      Deleted: {}", files_to_delete.len());
        
        if files_to_index.is_empty() && files_to_delete.is_empty() {
            println!("\n{}", "✅ Database is up to date! No changes detected.".green());
            return Ok(());
        }
    } else {
        // Full index - all files need indexing
        files_to_index = files.iter().map(|f| (f.clone(), vec![])).collect();
    }

    // Phase 2: Semantic Chunking
    println!("\n{}", "Phase 2: Semantic Chunking".bright_cyan());
    println!("{}", "-".repeat(60));

    let start = Instant::now();
    let mut chunker = SemanticChunker::new(100, 2000, 10);
    let mut all_chunks = Vec::new();

    let pb = ProgressBar::new(files_to_index.len() as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} {msg}")
            .unwrap()
            .progress_chars("█▓▒░ "),
    );

    let mut skipped_files = 0;
    for (file, _old_chunk_ids) in &files_to_index {
        pb.set_message(format!("{}", file.path.file_name().unwrap().to_string_lossy()));

        // Skip files that aren't valid UTF-8
        let source_code = match std::fs::read_to_string(&file.path) {
            Ok(content) => content,
            Err(_) => {
                skipped_files += 1;
                pb.inc(1);
                continue;
            }
        };

        let chunks = chunker.chunk_semantic(file.language, &file.path, &source_code)?;
        all_chunks.extend(chunks);

        pb.inc(1);
    }

    if skipped_files > 0 {
        println!("   ⚠️  Skipped {} files (invalid UTF-8)", skipped_files);
    }

    pb.finish_with_message("Done!");
    let chunking_duration = start.elapsed();

    println!("✅ Created {} chunks in {:?}", all_chunks.len(), chunking_duration);

    // Phase 3: Embedding Generation
    println!("\n{}", "Phase 3: Embedding Generation".bright_cyan());
    println!("{}", "-".repeat(60));

    let start = Instant::now();
    println!("🔄 Initializing embedding model...");

    let mut embedding_service = EmbeddingService::with_model(model_type)?;
    println!("✅ Model loaded: {} ({} dims)", embedding_service.model_name(), embedding_service.dimensions());

    let embedded_chunks = if all_chunks.is_empty() {
        vec![]
    } else {
        println!("\n🔄 Generating embeddings for {} chunks...", all_chunks.len());
        let chunks = embedding_service.embed_chunks(all_chunks)?;
        println!("✅ Generated {} embeddings in {:?}", chunks.len(), start.elapsed());
        println!("   Average: {:?} per chunk", start.elapsed() / chunks.len() as u32);
        
        // Show cache stats
        let cache_stats = embedding_service.cache_stats();
        println!("   Cache hit rate: {:.1}%", cache_stats.hit_rate() * 100.0);
        
        chunks
    };
    let embedding_duration = start.elapsed();

    // Phase 4: Vector Storage
    println!("\n{}", "Phase 4: Vector Storage".bright_cyan());
    println!("{}", "-".repeat(60));

    let start = Instant::now();
    
    // Database already opened earlier - just print status
    if !is_incremental {
        println!("✅ Database ready (newly created)");
    }

    // Delete old chunks from changed/deleted files
    if is_incremental {
        let mut chunks_to_delete = Vec::new();
        
        // Collect chunks from changed files
        for (_file, old_chunk_ids) in &files_to_index {
            chunks_to_delete.extend(old_chunk_ids);
        }
        
        // Collect chunks from deleted files
        for (_path, old_chunk_ids) in &files_to_delete {
            chunks_to_delete.extend(old_chunk_ids);
        }
        
        if !chunks_to_delete.is_empty() {
            println!("\n🗑️  Deleting {} old chunks...", chunks_to_delete.len());
            store.delete_chunks(&chunks_to_delete)?;
            println!("✅ Old chunks deleted");
        }
    }

    // Insert new chunks
    let chunk_ids = if !embedded_chunks.is_empty() {
        println!("\n🔄 Inserting {} chunks...", embedded_chunks.len());
        let ids = store.insert_chunks_with_ids(embedded_chunks.clone())?;
        println!("✅ Inserted {} chunks into vector store", ids.len());
        ids
    } else {
        vec![]
    };

    println!("\n🔄 Building vector index...");
    store.build_index()?;

    // Phase 4b: FTS Index
    println!("\n🔄 Updating full-text search index...");
    let mut fts_store = FtsStore::new(&db_path)?;

    // Delete old FTS entries
    if is_incremental {
        let mut fts_chunks_to_delete: Vec<u32> = Vec::new();
        for (_file, old_chunk_ids) in &files_to_index {
            fts_chunks_to_delete.extend(old_chunk_ids);
        }
        for (_path, old_chunk_ids) in &files_to_delete {
            fts_chunks_to_delete.extend(old_chunk_ids);
        }
        
        if !fts_chunks_to_delete.is_empty() {
            for chunk_id in fts_chunks_to_delete {
                let _ = fts_store.delete_chunk(chunk_id);
            }
            // Commit deletions before adding new entries
            fts_store.commit()?;
        }
    }

    // Add new FTS entries
    for (chunk, chunk_id) in embedded_chunks.iter().zip(chunk_ids.iter()) {
        fts_store.add_chunk(
            *chunk_id,
            &chunk.chunk.content,
            &chunk.chunk.path,
            chunk.chunk.signature.as_deref(),
            &format!("{:?}", chunk.chunk.kind),
            &chunk.chunk.string_literals,
        )?;
    }
    fts_store.commit()?;

    let fts_stats = fts_store.stats()?;
    println!("✅ FTS index updated ({} documents)", fts_stats.num_documents);

    let storage_duration = start.elapsed();

    println!("✅ Index updated in {:?}", storage_duration);
    
    // Update file metadata in VectorStore
    println!("\n🔄 Updating file metadata...");
    
    // Group chunks by file
    use std::collections::HashMap;
    let mut file_chunks: HashMap<PathBuf, Vec<u32>> = HashMap::new();
    
    for (i, chunk) in embedded_chunks.iter().enumerate() {
        let path = PathBuf::from(&chunk.chunk.path);
        file_chunks.entry(path).or_insert_with(Vec::new).push(chunk_ids[i]);
    }
    
    // Update metadata for changed files
    for (file, _) in &files_to_index {
        let chunk_ids_for_file = file_chunks.get(&file.path).cloned().unwrap_or_default();
        store.update_file_metadata(&file.path, chunk_ids_for_file)?;
    }
    
    // Remove metadata for deleted files
    for (path, _) in &files_to_delete {
        store.remove_file_metadata(&path)?;
    }
    
    // Save database metadata
    store.save_db_metadata(
        embedding_service.model_name(),
        embedding_service.dimensions(),
        !is_incremental // mark_full_index only on first index
    )?;
    
    println!("✅ File metadata saved");

    // Save model metadata (for backwards compatibility with tools that read metadata.json)
    // On incremental runs, unchanged chunks keep their old embeddings, so carry the
    // previous prefix version forward (missing = 0, i.e. built before prefixes existed).
    let prefix_version = if is_incremental {
        std::fs::read_to_string(db_path.join("metadata.json"))
            .ok()
            .and_then(|c| serde_json::from_str::<serde_json::Value>(&c).ok())
            .and_then(|j| j.get("prefix_version").and_then(|v| v.as_u64()))
            .unwrap_or(0)
    } else {
        crate::embed::PREFIX_VERSION
    };
    let metadata = serde_json::json!({
        "prefix_version": prefix_version,
        "model_short_name": embedding_service.model_short_name(),
        "model_name": embedding_service.model_name(),
        "dimensions": embedding_service.dimensions(),
        "indexed_at": chrono::Utc::now().to_rfc3339(),
    });
    std::fs::write(
        db_path.join("metadata.json"),
        serde_json::to_string_pretty(&metadata)?
    )?;
    println!("✅ Metadata saved");

    // Show final stats
    let db_stats = store.stats()?;
    println!("\n{}", "📊 Final Statistics".bright_green().bold());
    println!("{}", "=".repeat(60));
    println!("   Total chunks: {}", db_stats.total_chunks);
    println!("   Total files: {}", db_stats.total_files);
    println!("   Indexed: {}", if db_stats.indexed { "✅ Yes" } else { "❌ No" });
    println!("   Dimensions: {}", db_stats.dimensions);

    // Calculate database size
    let mut total_size = 0u64;
    for entry in std::fs::read_dir(&db_path)? {
        let entry = entry?;
        total_size += entry.metadata()?.len();
    }
    println!("   Database size: {:.2} MB", total_size as f64 / (1024.0 * 1024.0));

    // Total time
    let total_duration = discovery_duration + chunking_duration + embedding_duration + storage_duration;
    println!("\n{}", "⏱️  Timing Breakdown".bright_green());
    println!("{}", "-".repeat(60));
    println!("   File discovery:      {:?}", discovery_duration);
    println!("   Semantic chunking:   {:?}", chunking_duration);
    println!("   Embedding generation:{:?}", embedding_duration);
    println!("   Vector storage:      {:?}", storage_duration);
    println!("   {}", format!("Total:               {:?}", total_duration).bold());

    println!("\n{}", "✨ Indexing complete!".bright_green().bold());
    println!("   Run {} to search your codebase", "demongrep search <query>".bright_cyan());

    Ok(())
}

/// List all indexed repositories
pub async fn list() -> Result<()> {
    println!("{}", "📚 Indexed Repositories".bright_cyan().bold());
    println!("{}", "=".repeat(60));

    // Check current directory
    let current_dir = std::env::current_dir()?;
    let db_paths = get_search_db_paths(Some(current_dir.clone()))?;
    
    if db_paths.is_empty() {
        println!("\n{}", "No databases found for current directory".yellow());
    } else {
        println!("\n{}", "Current Directory:".bright_green());
        for db_path in &db_paths {
            let db_type = if db_path.ends_with(".demongrep.db") { "Local" } else { "Global" };
            println!("\n   {} Database:", db_type);
            print_repo_stats(&current_dir, db_path)?;
        }
    }
    
    // List all global databases
    if let Some(home) = dirs::home_dir() {
        let global_stores = home.join(".demongrep").join("stores");
        if global_stores.exists() {
            let mapping_file = home.join(".demongrep").join("projects.json");
            if mapping_file.exists() {
                if let Ok(content) = std::fs::read_to_string(&mapping_file) {
                    if let Ok(mappings) = serde_json::from_str::<std::collections::HashMap<String, String>>(&content) {
                        if !mappings.is_empty() {
                            println!("\n{}", "All Global Databases:".bright_green());
                            for (project, db) in mappings {
                                println!("\n   📂 {}", project);
                                if let Ok(db_path) = PathBuf::from(&db).canonicalize() {
                                    print_repo_stats(&PathBuf::from(&project), &db_path)?;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

/// Show statistics about the vector database - REFACTORED to use DatabaseManager
pub async fn stats(path: Option<PathBuf>) -> Result<()> {
    // Load all databases using DatabaseManager
    let db_manager = match DatabaseManager::load(path) {
        Ok(manager) => manager,
        Err(_) => {
            println!("{}", "❌ No database found!".red());
            println!("   Run {} or {} first", 
                "demongrep index".bright_cyan(),
                "demongrep index --global".bright_cyan()
            );
            return Ok(());
        }
    };

    // Show database info
    db_manager.print_info();
    println!();

    // Get combined statistics
    let combined = db_manager.combined_stats()?;

    // Print combined statistics
    println!("{}", "📊 Combined Statistics".bright_cyan().bold());
    println!("{}", "=".repeat(60));
    println!("\n{}", "Overall:".bright_green());
    println!("   Total chunks: {}", combined.total_chunks);
    println!("   Total files: {}", combined.total_files);
    println!("   Indexed: {}", if combined.indexed { "✅ Yes" } else { "❌ No" });
    println!("   Dimensions: {}", combined.dimensions);

    // Show breakdown if both databases exist
    if db_manager.database_count() > 1 {
        println!("\n{}", "Breakdown:".bright_green());
        if combined.local_chunks > 0 {
            println!("   📍 Local:  {} chunks from {} files", combined.local_chunks, combined.local_files);
        }
        if combined.global_chunks > 0 {
            println!("   🌍 Global: {} chunks from {} files", combined.global_chunks, combined.global_files);
        }
    }

    // Calculate total database size
    let mut total_size = 0u64;
    for db_path in db_manager.database_paths() {
        for entry in std::fs::read_dir(db_path)? {
            let entry = entry?;
            total_size += entry.metadata()?.len();
        }
    }

    println!("\n{}", "Storage:".bright_green());
    println!("   Total database size: {:.2} MB", total_size as f64 / (1024.0 * 1024.0));
    if combined.total_chunks > 0 {
        println!("   Average per chunk: {:.2} KB", (total_size as f64 / combined.total_chunks as f64) / 1024.0);
    }

    Ok(())
}

/// Clear the vector database
pub async fn clear(path: Option<PathBuf>, yes: bool, project: Option<String>) -> Result<()> {
    let db_paths = if let Some(project_name) = &project {
        // Look up project in projects.json
        find_project_databases(project_name)?
    } else {
        // Use current directory
        get_search_db_paths(path)?
    };
    
    if db_paths.is_empty() {
        println!("{}", "❌ No database found!".red());
        if let Some(proj) = &project {
            println!("   Project '{}' not found in global registry", proj);
            println!("   Run {} to see all indexed projects", "demongrep list".bright_cyan());
        }
        return Ok(());
    }

    println!("{}", "🗑️  Clear Database".bright_yellow().bold());
    println!("{}", "=".repeat(60));
    
    for db_path in &db_paths {
        let db_type = if db_path.ends_with(".demongrep.db") { "Local" } else { "Global" };
        println!("💾 {} Database: {}", db_type, db_path.display());
    }

    if !yes {
        println!("\n{}", "⚠️  This will delete all indexed data from these databases!".yellow());
        print!("Are you sure? (y/N): ");
        use std::io::{self, Write};
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        if !input.trim().eq_ignore_ascii_case("y") {
            println!("{}", "Cancelled.".dimmed());
            return Ok(());
        }
    }

    // Track which paths we're deleting for cleanup
    let mut deleted_global_dbs = Vec::new();
    
    for db_path in db_paths {
        let db_type = if db_path.ends_with(".demongrep.db") { "Local" } else { "Global" };
        println!("\n🔄 Removing {} database...", db_type);
        
        // Track global databases for projects.json cleanup
        if !db_path.ends_with(".demongrep.db") {
            deleted_global_dbs.push(db_path.clone());
        }
        
        std::fs::remove_dir_all(&db_path)?;
        println!("{}", format!("✅ {} database cleared!", db_type).green());
    }

    // Clean up projects.json for any deleted global databases
    if !deleted_global_dbs.is_empty() {
        if let Err(e) = cleanup_project_mappings(&deleted_global_dbs) {
            eprintln!("{}", format!("⚠️  Warning: Could not clean up projects.json: {}", e).yellow());
        } else {
            println!("\n✅ Cleaned up global registry");
        }
    }
    
    // If we cleared by project name, also show a message
    if project.is_some() {
        // Already cleaned up above
    }

    Ok(())
}

/// Helper to print repository stats
fn print_repo_stats(_repo_path: &Path, db_path: &Path) -> Result<()> {
    // Try to load stats
    match VectorStore::new(db_path, 384) {
        Ok(store) => {
            match store.stats() {
                Ok(stats) => {
                    println!("      {} chunks in {} files", stats.total_chunks, stats.total_files);
                }
                Err(_) => {
                    println!("      {}", "Could not load stats".dimmed());
                }
            }
        }
        Err(_) => {
            println!("      {}", "Could not open database".dimmed());
        }
    }

    Ok(())
}
