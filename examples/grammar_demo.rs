use demongrep::chunker::{CodeParser, GrammarManager};
use demongrep::file::Language;

fn main() {
    println!("=== Grammar Management System Demo ===\n");

    // Create grammar manager
    let manager = GrammarManager::new();

    println!("📚 Grammar Manager initialized");
    println!("   Supported languages: {}", manager.supported_languages().len());

    // Show supported languages
    println!("\n✅ Supported languages:");
    for lang in manager.supported_languages() {
        println!("   - {}", lang.name());
    }

    // Show stats before loading
    let stats = manager.stats();
    println!("\n📊 Initial stats:");
    println!("   Cached grammars: {}", stats.cached_grammars);
    println!("   Supported languages: {}", stats.supported_languages);

    // Load Rust grammar
    println!("\n🦀 Loading Rust grammar...");
    let rust_grammar = manager.get_grammar(Language::Rust);
    println!("   Result: {}", if rust_grammar.is_some() { "✅ Success" } else { "❌ Failed" });

    // Load Python grammar
    println!("\n🐍 Loading Python grammar...");
    let python_grammar = manager.get_grammar(Language::Python);
    println!("   Result: {}", if python_grammar.is_some() { "✅ Success" } else { "❌ Failed" });

    // Try unsupported language
    println!("\n📝 Trying unsupported language (Markdown)...");
    let markdown_grammar = manager.get_grammar(Language::Markdown);
    println!("   Result: {}", if markdown_grammar.is_some() { "✅ Loaded" } else { "❌ Not supported (expected)" });

    // Show stats after loading
    let stats_after = manager.stats();
    println!("\n📊 Stats after loading:");
    println!("   Cached grammars: {}", stats_after.cached_grammars);
    println!("   Cache hit rate: {}/{} loaded",
             stats_after.cached_grammars,
             stats_after.supported_languages);

    // Test caching - load Rust again
    println!("\n♻️  Testing cache (loading Rust again)...");
    let rust_grammar2 = manager.get_grammar(Language::Rust);
    println!("   Result: {}", if rust_grammar2.is_some() { "✅ From cache" } else { "❌ Failed" });

    // Verify same Arc
    if let (Some(g1), Some(g2)) = (rust_grammar, rust_grammar2) {
        println!("   Same Arc: {}", if std::sync::Arc::ptr_eq(&g1, &g2) { "✅ Yes (cached)" } else { "❌ No" });
    }

    // Demo: Parse some Rust code
    println!("\n🔍 Parsing Rust code...");
    let mut parser = CodeParser::new();

    let rust_code = r#"
fn hello_world() {
    println!("Hello, world!");
}

fn add(a: i32, b: i32) -> i32 {
    a + b
}

struct Point {
    x: f64,
    y: f64,
}
    "#;

    match parser.parse(Language::Rust, rust_code) {
        Ok(parsed) => {
            println!("   ✅ Parse successful!");
            println!("   Has errors: {}", parsed.has_errors());
            println!("   Root node kind: {}", parsed.root_node().kind());

            // Find functions
            let functions = parsed.find_nodes_by_type("function_item");
            println!("   Functions found: {}", functions.len());
        }
        Err(e) => {
            println!("   ❌ Parse failed: {}", e);
        }
    }

    // Demo: Parse Python code
    println!("\n🐍 Parsing Python code...");

    let python_code = r#"
def hello():
    print("Hello from Python!")

class Calculator:
    def add(self, a, b):
        return a + b
    "#;

    match parser.parse(Language::Python, python_code) {
        Ok(parsed) => {
            println!("   ✅ Parse successful!");
            println!("   Has errors: {}", parsed.has_errors());
            println!("   Root node kind: {}", parsed.root_node().kind());

            // Find functions
            let functions = parsed.find_nodes_by_type("function_definition");
            println!("   Functions found: {}", functions.len());
        }
        Err(e) => {
            println!("   ❌ Parse failed: {}", e);
        }
    }

    // Pre-load all grammars
    println!("\n⚡ Pre-loading all grammars...");
    manager.preload_all();
    let final_stats = manager.stats();
    println!("   Cached grammars: {}/{}",
             final_stats.cached_grammars,
             final_stats.supported_languages);

    println!("\n✅ Demo complete!\n");
}
