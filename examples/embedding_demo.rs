use demongrep::{
    chunker::SemanticChunker,
    embed::EmbeddingService,
    file::Language,
};
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Embedding Demo ===\n");

    // Step 1: Create semantic chunker
    println!("Step 1: Creating semantic chunker...");
    let mut chunker = SemanticChunker::new(100, 2000, 10);
    println!("✅ Chunker created\n");

    // Step 2: Create embedding service (this will download model on first run)
    println!("Step 2: Initializing embedding service...");
    let mut embedding_service = EmbeddingService::new()?;
    println!("   Model: {}", embedding_service.model_name());
    println!("   Dimensions: {}\n", embedding_service.dimensions());

    // Step 3: Create sample code chunks
    println!("Step 3: Creating code chunks...\n");
    let rust_code = r#"
use std::collections::HashMap;

/// Authenticates a user by checking their credentials
///
/// # Arguments
/// * `username` - The username to authenticate
/// * `password` - The password to verify
///
/// # Returns
/// * `true` if authentication succeeds, `false` otherwise
fn authenticate_user(username: &str, password: &str) -> bool {
    // In a real system, this would check against a database
    let valid_users: HashMap<&str, &str> = HashMap::from([
        ("alice", "secret123"),
        ("bob", "password456"),
    ]);

    if let Some(&stored_password) = valid_users.get(username) {
        stored_password == password
    } else {
        false
    }
}

/// Hashes a password using SHA-256
///
/// # Arguments
/// * `password` - The password to hash
///
/// # Returns
/// * The hashed password as a hex string
fn hash_password(password: &str) -> String {
    use sha2::{Sha256, Digest};
    let mut hasher = Sha256::new();
    hasher.update(password.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Calculates the Fibonacci number at position n
///
/// # Arguments
/// * `n` - The position in the Fibonacci sequence
///
/// # Returns
/// * The Fibonacci number at position n
fn fibonacci(n: u32) -> u64 {
    match n {
        0 => 0,
        1 => 1,
        _ => fibonacci(n - 1) + fibonacci(n - 2),
    }
}

/// Sorts a vector of items in place
///
/// # Arguments
/// * `items` - The vector to sort
fn sort_items<T: Ord>(items: &mut Vec<T>) {
    items.sort();
}

/// Processes user data for analytics
///
/// # Arguments
/// * `user_id` - The ID of the user
/// * `data` - The raw data to process
///
/// # Returns
/// * Processed analytics data
fn process_analytics(user_id: u32, data: Vec<f64>) -> Vec<f64> {
    // Calculate moving average
    data.iter()
        .enumerate()
        .map(|(i, &val)| {
            let start = i.saturating_sub(5);
            let window = &data[start..=i];
            let sum: f64 = window.iter().sum();
            sum / window.len() as f64
        })
        .collect()
}
"#;

    let path = Path::new("auth.rs");
    let chunks = chunker.chunk_semantic(Language::Rust, path, rust_code)?;
    println!("   Created {} chunks\n", chunks.len());

    // Show chunk details
    for (i, chunk) in chunks.iter().enumerate() {
        println!("Chunk {}: [{:?}] {}",
            i + 1,
            chunk.kind,
            chunk.signature.as_ref().unwrap_or(&"N/A".to_string()));
    }
    println!();

    // Step 4: Embed chunks
    println!("Step 4: Embedding chunks...\n");
    let embedded_chunks = embedding_service.embed_chunks(chunks)?;
    println!("✅ Embedded {} chunks\n", embedded_chunks.len());

    let cache_stats = embedding_service.cache_stats();
    println!("Cache stats:");
    println!("   Size: {} entries", cache_stats.size);
    println!("   Hit rate: {:.1}%\n", cache_stats.hit_rate() * 100.0);

    // Step 5: Query search
    println!("Step 5: Semantic Search\n");

    let queries = vec![
        "how to authenticate users and check passwords",
        "password hashing and encryption",
        "fibonacci and mathematical calculations",
        "sorting algorithms",
        "user analytics and data processing",
    ];

    for query in queries {
        println!("Query: \"{}\"", query);

        // Embed the query
        let query_embedding = embedding_service.embed_query(query)?;

        // Search for most similar chunks
        let results = embedding_service.search(&query_embedding, &embedded_chunks, 3);

        println!("   Top {} results:", results.len());
        for (i, (chunk, score)) in results.iter().enumerate() {
            println!("      {}. [{:?}] {}",
                i + 1,
                chunk.chunk.kind,
                chunk.chunk.signature.as_ref().unwrap_or(&format!("line {}-{}", chunk.chunk.start_line, chunk.chunk.end_line))
            );
            println!("         Similarity: {:.4}", score);
        }
        println!();
    }

    // Step 6: Test caching
    println!("Step 6: Testing cache...\n");

    let rust_code_2 = r#"
fn authenticate_user(username: &str, password: &str) -> bool {
    // Same function as before - should hit cache
    let valid_users: HashMap<&str, &str> = HashMap::from([
        ("alice", "secret123"),
        ("bob", "password456"),
    ]);

    if let Some(&stored_password) = valid_users.get(username) {
        stored_password == password
    } else {
        false
    }
}
"#;

    let chunks_2 = chunker.chunk_semantic(Language::Rust, Path::new("auth2.rs"), rust_code_2)?;
    println!("   Created {} chunks from second file", chunks_2.len());

    let embedded_chunks_2 = embedding_service.embed_chunks(chunks_2)?;
    println!("   Embedded {} chunks", embedded_chunks_2.len());

    let cache_stats_2 = embedding_service.cache_stats();
    println!("\nFinal cache stats:");
    println!("   Size: {} entries", cache_stats_2.size);
    println!("   Total requests: {}", cache_stats_2.total_requests());
    println!("   Hits: {}", cache_stats_2.hits);
    println!("   Misses: {}", cache_stats_2.misses);
    println!("   Hit rate: {:.1}%", cache_stats_2.hit_rate() * 100.0);

    println!("\n✅ Demo complete!");

    Ok(())
}
