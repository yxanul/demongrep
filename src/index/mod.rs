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
pub async fn index(path: Option<PathBuf>, dry_run: bool, force: bool, global: bool, model: Option<ModelType>) -> Result<()> {
    let project_path = path.clone().unwrap_or_else(|| PathBuf::from("."));
    let db_path = get_index_db_path(path, global)?;
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

    // Check if database exists and handle force flag
    if db_path.exists() && !force {
        println!("\n{}", "⚠️  Database already exists!".yellow());
        println!("   Use --force to re-index");
        return Ok(());
    }

    // Clear existing database if forcing
    if db_path.exists() && force {
        println!("\n{}", "🗑️  Clearing existing database...".yellow());
        std::fs::remove_dir_all(&db_path)?;
    }

    // Phase 2: Semantic Chunking
    println!("\n{}", "Phase 2: Semantic Chunking".bright_cyan());
    println!("{}", "-".repeat(60));

    let start = Instant::now();
    let mut chunker = SemanticChunker::new(100, 2000, 10);
    let mut all_chunks = Vec::new();

    let pb = ProgressBar::new(files.len() as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} {msg}")
            .unwrap()
            .progress_chars("█▓▒░ "),
    );

    let mut skipped_files = 0;
    for file in &files {
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

    if all_chunks.is_empty() {
        println!("\n{}", "No chunks created!".yellow());
        return Ok(());
    }

    // Phase 3: Embedding Generation
    println!("\n{}", "Phase 3: Embedding Generation".bright_cyan());
    println!("{}", "-".repeat(60));

    let start = Instant::now();
    println!("🔄 Initializing embedding model...");

    let mut embedding_service = EmbeddingService::with_model(model_type)?;
    println!("✅ Model loaded: {} ({} dims)", embedding_service.model_name(), embedding_service.dimensions());

    println!("\n🔄 Generating embeddings for {} chunks...", all_chunks.len());
    let embedded_chunks = embedding_service.embed_chunks(all_chunks)?;
    let embedding_duration = start.elapsed();

    println!("✅ Generated {} embeddings in {:?}", embedded_chunks.len(), embedding_duration);
    println!("   Average: {:?} per chunk", embedding_duration / embedded_chunks.len() as u32);

    // Show cache stats
    let cache_stats = embedding_service.cache_stats();
    println!("   Cache hit rate: {:.1}%", cache_stats.hit_rate() * 100.0);

    // Phase 4: Vector Storage
    println!("\n{}", "Phase 4: Vector Storage".bright_cyan());
    println!("{}", "-".repeat(60));

    let start = Instant::now();
    println!("🔄 Creating vector database...");

    let mut store = VectorStore::new(&db_path, embedding_service.dimensions())?;
    println!("✅ Database created");

    println!("\n🔄 Inserting {} chunks...", embedded_chunks.len());
    let chunk_ids = store.insert_chunks_with_ids(embedded_chunks.clone())?;
    println!("✅ Inserted {} chunks into vector store", chunk_ids.len());

    println!("\n🔄 Building vector index...");
    store.build_index()?;

    // Phase 4b: FTS Index
    println!("\n🔄 Building full-text search index...");
    let mut fts_store = FtsStore::new(&db_path)?;

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
    println!("✅ FTS index built ({} documents)", fts_stats.num_documents);

    let storage_duration = start.elapsed();

    println!("✅ Index built in {:?}", storage_duration);

    // Save model metadata
    let metadata = serde_json::json!({
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

    for db_path in db_paths {
        let db_type = if db_path.ends_with(".demongrep.db") { "Local" } else { "Global" };
        println!("\n🔄 Removing {} database...", db_type);
        std::fs::remove_dir_all(&db_path)?;
        println!("{}", format!("✅ {} database cleared!", db_type).green());
    }

    // If we cleared by project name, remove from projects.json
    if let Some(project_name) = project {
        remove_from_project_mapping(&project_name)?;
        println!("\n✅ Removed '{}' from global registry", project_name);
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
