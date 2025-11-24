use anyhow::{anyhow, Result};
use colored::Colorize;
use std::path::PathBuf;
use std::time::Instant;

use crate::embed::EmbeddingService;
use crate::vectordb::VectorStore;

/// Get the database path for a given project directory
fn get_db_path(path: Option<PathBuf>) -> Result<PathBuf> {
    let project_path = path.unwrap_or_else(|| PathBuf::from("."));
    let canonical_path = project_path.canonicalize()?;

    Ok(canonical_path.join(".demongrep.db"))
}

/// Search the codebase
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
) -> Result<()> {
    let db_path = get_db_path(path)?;

    if !db_path.exists() {
        println!("{}", "❌ No database found!".red());
        println!("   Run {} first", "demongrep index".bright_cyan());
        return Ok(());
    }

    if sync {
        println!("{}", "🔄 Syncing database (not implemented yet)...".yellow());
        // TODO: Implement incremental re-indexing
    }

    // Load database
    let start = Instant::now();
    let store = VectorStore::new(&db_path, 384)?; // TODO: Get dimensions from metadata
    let load_duration = start.elapsed();

    // Initialize embedding service
    let start = Instant::now();
    let mut embedding_service = EmbeddingService::new()?;
    let model_load_duration = start.elapsed();

    // Embed query
    let start = Instant::now();
    let query_embedding = embedding_service.embed_query(query)?;
    let embed_duration = start.elapsed();

    // Search
    let start = Instant::now();
    let results = store.search(&query_embedding, max_results)?;
    let search_duration = start.elapsed();

    // Output results
    if json {
        // TODO: Implement JSON output
        println!("JSON output not implemented yet");
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
        println!("   Database load: {:?}", load_duration);
        println!("   Model load:    {:?}", model_load_duration);
        println!("   Query embed:   {:?}", embed_duration);
        println!("   Search:        {:?}", search_duration);
        println!("   Total:         {:?}", load_duration + model_load_duration + embed_duration + search_duration);
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
        println!("\n   {}:", "Content".bright_yellow());
        for line in result.content.lines().take(10) {
            println!("   │ {}", line.dimmed());
        }
        if result.content.lines().count() > 10 {
            println!("   │ {}", "...".dimmed());
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
