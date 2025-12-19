use anyhow::{anyhow, Result};
use colored::Colorize;
use serde::Serialize;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::cache::FileMetaStore;
use crate::chunker::SemanticChunker;
use crate::embed::{EmbeddingService, ModelType};
use crate::file::FileWalker;
use crate::fts::FtsStore;
use crate::index::get_search_db_paths;
use crate::rerank::{rrf_fusion, vector_only, FusedResult, NeuralReranker, DEFAULT_RRF_K};
use crate::vectordb::VectorStore;

/// JSON output format for search results
#[derive(Serialize)]
struct JsonOutput {
    query: String,
    results: Vec<JsonResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    timing: Option<JsonTiming>,
}

#[derive(Serialize)]
struct JsonResult {
    path: String,
    start_line: usize,
    end_line: usize,
    kind: String,
    content: String,
    score: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    signature: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    context_prev: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    context_next: Option<String>,
}

#[derive(Serialize)]
struct JsonTiming {
    total_ms: u64,
    embed_ms: u64,
    search_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    rerank_ms: Option<u64>,
}



/// Read model metadata from database
fn read_metadata(db_path: &PathBuf) -> Option<(String, usize)> {
    let metadata_path = db_path.join("metadata.json");
    if let Ok(content) = std::fs::read_to_string(&metadata_path) {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
            let model = json.get("model_short_name")?.as_str()?.to_string();
            let dims = json.get("dimensions")?.as_u64()? as usize;
            return Some((model, dims));
        }
    }
    None
}

/// Search the codebase (searches both local and global databases)
#[allow(clippy::too_many_arguments)]
pub async fn search(
    query: &str,
    max_results: usize,
    per_file: usize,
    content: bool,
    scores: bool,
    compact: bool,
    sync: bool,
    json: bool,
    path: Option<PathBuf>,
    filter_path: Option<String>,
    model_override: Option<ModelType>,
    vector_only_mode: bool,
    rrf_k: f32,
    rerank: bool,
    rerank_top: usize,
) -> Result<()> {
    // Get all database paths (local + global)
    let db_paths = get_search_db_paths(path.clone())?;
    
    if db_paths.is_empty() {
        println!("{}", "❌ No database found!".red());
        println!("   Run {} or {} first", 
            "demongrep index".bright_cyan(),
            "demongrep index --global".bright_cyan()
        );
        return Ok(());
    }
    
    // Show which databases we're searching (unless in JSON mode)
    if !json && db_paths.len() > 1 {
        println!("{}", "🔍 Searching in multiple databases...".dimmed());
        for db_path in &db_paths {
            let db_type = if db_path.ends_with(".demongrep.db") { "Local" } else { "Global" };
            println!("   {} {}", db_type, db_path.display().to_string().dimmed());
        }
        println!();
    }

    // Collect all results from all databases
    let mut all_results: Vec<crate::vectordb::SearchResult> = Vec::new();
    let mut total_embed_duration = Duration::ZERO;
    let mut total_search_duration = Duration::ZERO;
    let mut total_load_duration = Duration::ZERO;
    let mut model_load_duration = Duration::ZERO;
    
    // We'll use the first database's model/dimensions, or override
    let (model_type, dimensions) = if let Some(override_model) = model_override {
        (override_model, override_model.dimensions())
    } else if let Some((model_name, dims)) = read_metadata(&db_paths[0]) {
        if let Some(mt) = ModelType::from_str(&model_name) {
            (mt, dims)
        } else {
            eprintln!("{}", "⚠️  Unknown model in metadata, using default".yellow());
            (ModelType::default(), 384)
        }
    } else {
        (ModelType::default(), 384)
    };
    
    // Initialize embedding service once (shared across all databases)
    let start = Instant::now();
    let mut embedding_service = EmbeddingService::with_model(model_type)?;
    model_load_duration = start.elapsed();
    
    // Embed query once
    let start = Instant::now();
    let query_embedding = embedding_service.embed_query(query)?;
    total_embed_duration = start.elapsed();
    
    // Search in each database
    for db_path in db_paths {

        // Perform sync if requested
        if sync {
            if !json {
                let db_type = if db_path.ends_with(".demongrep.db") { "Local" } else { "Global" };
                println!("{}", format!("🔄 Syncing {} database...", db_type).yellow());
            }
            sync_database(&db_path, model_type)?;
        }
        
        // Load this database
        let start = Instant::now();
        let store = VectorStore::new(&db_path, dimensions)?;
        total_load_duration += start.elapsed();
        
        // Search in this database
        let start = Instant::now();
        let retrieval_limit = if vector_only_mode { max_results } else { 200 };
        let vector_results = store.search(&query_embedding, retrieval_limit)?;

        let fused_results: Vec<FusedResult> = if vector_only_mode {
            vector_only(&vector_results)
        } else {
            match FtsStore::open_readonly(&db_path) {
                Ok(fts_store) => {
                    let fts_results = fts_store.search(query, retrieval_limit)?;
                    rrf_fusion(&vector_results, &fts_results, rrf_k)
                }
                Err(_) => {
                    if !json {
                        eprintln!("{}", "⚠️  FTS index not found, using vector-only search".yellow());
                    }
                    vector_only(&vector_results)
                }
            }
        };
        
        // Map fused results back to full SearchResult
        let chunk_id_to_result: std::collections::HashMap<u32, &crate::vectordb::SearchResult> =
            vector_results.iter().map(|r| (r.id, r)).collect();
        
        let take_count = if rerank { rerank_top.min(fused_results.len()) } else { max_results };
        
        for fused in fused_results.iter().take(take_count) {
            if let Some(result) = chunk_id_to_result.get(&fused.chunk_id) {
                let mut r = (*result).clone();
                r.score = fused.rrf_score;
                all_results.push(r);
            } else {
                if let Ok(Some(mut result)) = store.get_chunk_as_result(fused.chunk_id) {
                    result.score = fused.rrf_score;
                    all_results.push(result);
                }
            }
        }
        
        total_search_duration += start.elapsed();
    }
    
    // Deduplicate results by (path, start_line, end_line) and keep highest score
    let mut seen: std::collections::HashMap<(String, usize, usize), usize> = std::collections::HashMap::new();
    let mut results: Vec<crate::vectordb::SearchResult> = Vec::new();
    
    for result in all_results {
        let key = (result.path.clone(), result.start_line, result.end_line);
        if let Some(&idx) = seen.get(&key) {
            // Already have this result, keep the one with higher score
            if result.score > results[idx].score {
                results[idx] = result;
            }
        } else {
            seen.insert(key, results.len());
            results.push(result);
        }
    }
    
    // Sort by score
    results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());

    // Neural reranking (if enabled)
    let mut rerank_duration = Duration::ZERO;
    if rerank && !results.is_empty() {
        let start = Instant::now();
        match NeuralReranker::new() {
            Ok(mut reranker) => {
                let documents: Vec<String> = results.iter().map(|r| r.content.clone()).collect();
                let rrf_scores: Vec<f32> = results.iter().map(|r| r.score).collect();
                match reranker.rerank_and_blend(query, &documents, &rrf_scores) {
                    Ok(reranked) => {
                        let mut reordered: Vec<crate::vectordb::SearchResult> = Vec::with_capacity(results.len());
                        for (idx, score) in reranked {
                            let mut result = results[idx].clone();
                            result.score = score;
                            reordered.push(result);
                        }
                        results = reordered;
                        if !json {
                            println!("{}", "✅ Neural reranking applied".green());
                        }
                    }
                    Err(e) => {
                        if !json {
                            eprintln!("{}", format!("⚠️  Reranking failed: {}", e).yellow());
                        }
                    }
                }
            }
            Err(e) => {
                if !json {
                    eprintln!("{}", format!("⚠️  Could not load reranker: {}", e).yellow());
                }
            }
        }
        rerank_duration = start.elapsed();
    }

    // Filter by path if specified
    if let Some(ref filter) = filter_path {
        let filter_normalized = filter.trim_start_matches("./");
        results.retain(|r| {
            let path_normalized = r.path.trim_start_matches("./");
            path_normalized.starts_with(filter_normalized)
        });
    }

    // Truncate to max_results after reranking and filtering
    results.truncate(max_results);

    // Output results
    if json {
        let json_results: Vec<JsonResult> = results
            .iter()
            .map(|r| JsonResult {
                path: r.path.clone(),
                start_line: r.start_line,
                end_line: r.end_line,
                kind: r.kind.clone(),
                content: r.content.clone(),
                score: r.score,
                signature: r.signature.clone(),
                context_prev: r.context_prev.clone(),
                context_next: r.context_next.clone(),
            })
            .collect();

        let timing = if scores {
            Some(JsonTiming {
                total_ms: (total_load_duration + model_load_duration + total_embed_duration + total_search_duration + rerank_duration).as_millis() as u64,
                embed_ms: total_embed_duration.as_millis() as u64,
                search_ms: total_search_duration.as_millis() as u64,
                rerank_ms: if rerank { Some(rerank_duration.as_millis() as u64) } else { None },
            })
        } else {
            None
        };

        let output = JsonOutput {
            query: query.to_string(),
            results: json_results,
            timing,
        };

        println!("{}", serde_json::to_string(&output)?);
        return Ok(());
    }

    if compact {
        // Show only file paths (like grep -l)
        let mut seen_files = std::collections::HashSet::new();
        for result in &results {
            if !seen_files.contains(&result.path) {
                println!("{}", result.path);
                seen_files.insert(result.path.clone());
            }
        }
        return Ok(());
    }

    // Standard output
    println!("{}", "🔍 Search Results".bright_cyan().bold());
    println!("{}", "=".repeat(60));
    println!("Query: \"{}\"", query.bright_yellow());
    println!("Found {} results", results.len());
    println!();

    if scores {
        println!("Timing:");
        println!("   Database load: {:?}", total_load_duration);
        println!("   Model load:    {:?}", model_load_duration);
        println!("   Query embed:   {:?}", total_embed_duration);
        println!("   Search:        {:?}", total_search_duration);
        if rerank {
            println!("   Reranking:     {:?}", rerank_duration);
        }
        println!("   Total:         {:?}", total_load_duration + model_load_duration + total_embed_duration + total_search_duration + rerank_duration);
        println!();
    }

    // Check if no results
    if results.is_empty() {
        println!("{}", "No matches found.".dimmed());
        println!("Try:");
        println!("  - Using different keywords");
        println!("  - Making your query more general");
        println!("  - Running {} if the codebase changed", "demongrep index --force".bright_cyan());
        return Ok(());
    }

    // Group results by file if per_file > 0
    if per_file > 0 && per_file < max_results {
        let mut by_file: std::collections::HashMap<String, Vec<_>> = std::collections::HashMap::new();

        for result in results {
            by_file.entry(result.path.clone()).or_default().push(result);
        }

        let mut files: Vec<_> = by_file.into_iter().collect();
        files.sort_by(|a, b| {
            b.1.iter().map(|r| r.score).fold(0.0f32, f32::max)
                .partial_cmp(&a.1.iter().map(|r| r.score).fold(0.0f32, f32::max))
                .unwrap()
        });

        for (_file_path, mut file_results) in files {
            file_results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
            file_results.truncate(per_file);

            for (idx, result) in file_results.iter().enumerate() {
                print_result(result, idx == 0, content, scores)?;
            }
        }
    } else {
        // Show all results
        for result in &results {
            print_result(result, true, content, scores)?;
        }
    }

    Ok(())
}

/// Sync database by re-indexing changed files
fn sync_database(db_path: &PathBuf, model_type: ModelType) -> Result<()> {
    let project_path = db_path.parent().unwrap_or(std::path::Path::new("."));

    // Load file metadata store
    let mut file_meta = FileMetaStore::load_or_create(db_path, model_type.short_name(), model_type.dimensions())?;

    // Walk the file system
    let walker = FileWalker::new(project_path.to_path_buf());
    let (files, _stats) = walker.walk()?;

    // Initialize services
    let mut embedding_service = EmbeddingService::with_model(model_type)?;
    let mut chunker = SemanticChunker::new(100, 2000, 10);
    let mut store = VectorStore::new(db_path, model_type.dimensions())?;

    let mut changes = 0;

    // Check for changed files
    for file in &files {
        let (needs_reindex, old_chunk_ids) = file_meta.check_file(&file.path)?;

        if !needs_reindex {
            continue;
        }

        changes += 1;
        println!("  📝 {}", file.path.display());

        // Delete old chunks
        if !old_chunk_ids.is_empty() {
            store.delete_chunks(&old_chunk_ids)?;
        }

        // Read and chunk file
        let source_code = match std::fs::read_to_string(&file.path) {
            Ok(content) => content,
            Err(_) => continue,
        };

        let chunks = chunker.chunk_semantic(file.language, &file.path, &source_code)?;

        if chunks.is_empty() {
            file_meta.update_file(&file.path, vec![])?;
            continue;
        }

        // Embed and insert
        let embedded_chunks = embedding_service.embed_chunks(chunks)?;
        let chunk_ids = store.insert_chunks_with_ids(embedded_chunks)?;
        file_meta.update_file(&file.path, chunk_ids)?;
    }

    // Check for deleted files
    let deleted_files = file_meta.find_deleted_files();
    for (path, chunk_ids) in &deleted_files {
        changes += 1;
        println!("  🗑️  {} (deleted)", path);
        if !chunk_ids.is_empty() {
            store.delete_chunks(chunk_ids)?;
        }
        file_meta.remove_file(std::path::Path::new(path));
    }

    // Rebuild index if changes were made
    if changes > 0 {
        println!("  🔨 Rebuilding index...");
        store.build_index()?;
        file_meta.save(db_path)?;
        println!("  ✅ {} file(s) synced", changes);
    } else {
        println!("  ✅ Already up to date");
    }

    Ok(())
}

fn print_result(
    result: &crate::vectordb::SearchResult,
    show_file: bool,
    show_content: bool,
    show_scores: bool,
) -> Result<()> {
    if show_file {
        println!("{}", "─".repeat(60));
        let file_display = format!("📄 {}", result.path);
        println!("{}", file_display.bright_green());
    }

    // Show location and kind
    let location = format!(
        "   Lines {}-{} • {}",
        result.start_line,
        result.end_line,
        result.kind
    );
    println!("{}", location.dimmed());

    // Show signature if available
    if let Some(sig) = &result.signature {
        println!("   {}", sig.bright_cyan());
    }

    // Show score if requested
    if show_scores {
        let score_color = if result.score > 0.8 {
            "green"
        } else if result.score > 0.6 {
            "yellow"
        } else {
            "red"
        };

        let score_text = format!("   Score: {:.3}", result.score);
        println!("{}", match score_color {
            "green" => score_text.green(),
            "yellow" => score_text.yellow(),
            _ => score_text.red(),
        });
    }

    // Show context if available
    if let Some(ctx) = &result.context {
        println!("   Context: {}", ctx.dimmed());
    }

    // Show content if requested
    if show_content {
        // Show context before (if available)
        if let Some(ctx_prev) = &result.context_prev {
            println!("\n   {}:", "Context (before)".dimmed());
            for line in ctx_prev.lines() {
                println!("   │ {}", line.bright_black());
            }
        }

        println!("\n   {}:", "Content".bright_yellow());
        for line in result.content.lines().take(10) {
            println!("   │ {}", line.dimmed());
        }
        if result.content.lines().count() > 10 {
            println!("   │ {}", "...".dimmed());
        }

        // Show context after (if available)
        if let Some(ctx_next) = &result.context_next {
            println!("\n   {}:", "Context (after)".dimmed());
            for line in ctx_next.lines() {
                println!("   │ {}", line.bright_black());
            }
        }
    } else {
        // Show a snippet
        let snippet: String = result
            .content
            .lines()
            .take(3)
            .collect::<Vec<_>>()
            .join(" ");

        let snippet = if snippet.len() > 100 {
            format!("{}...", &snippet[..100])
        } else {
            snippet
        };

        println!("   {}", snippet.dimmed());
    }

    println!();

    Ok(())
}
