# Semantic Chunking Design: osgrep Analysis & demongrep Improvements

## osgrep's Approach (Analysis)

### Strategy Overview

**Core Algorithm:**
1. Parse file with tree-sitter
2. Visit AST recursively
3. Identify definitions (functions, classes, methods)
4. Extract names and build context breadcrumbs
5. Capture gaps between definitions as "block" chunks
6. Classify chunks (function/class/other)
7. Split oversized chunks

### What osgrep Does Well ✅

1. **Export Unwrapping**
   ```typescript
   unwrapExport(node) {
     if (node.type === "export_statement") {
       return node.namedChildren[0];  // Get actual declaration
     }
     return node;
   }
   ```
   - Handles `export function foo() {}` correctly
   - Gets to the actual definition

2. **Arrow Function Detection**
   ```typescript
   isTopLevelValueDef(node) {
     if (node.type === "lexical_declaration") {
       if (text.includes("=>")) return true;  // const foo = () => {}
     }
   }
   ```
   - Recognizes modern JavaScript patterns
   - Treats arrow functions as definitions

3. **Context Breadcrumbs**
   ```typescript
   visit(node, stack) {
     if (isDefinition) {
       const context = [...stack, label];
       addChunk(node, context);
       visit(children, context);  // Pass down
     }
   }
   ```
   - Builds: `File: path > Class: Foo > Method: bar`
   - Maintains hierarchy

4. **Gap Handling**
   ```typescript
   if (child.startIndex > cursorIndex) {
     const gapText = content.slice(cursorIndex, child.startIndex);
     blockChunks.push({ content: gapText, type: "block" });
   }
   ```
   - Captures imports, module-level code
   - Ensures 100% file coverage

5. **Name Extraction Fallbacks**
   - Tries field access first: `node.childForFieldName("name")`
   - Falls back to identifier search
   - Regex as last resort

### What Can Be Improved ⚠️

1. **Language-Specific Logic Missing**

   **Problem:** All logic hardcoded for TypeScript/JavaScript
   ```typescript
   const isDefType = (t) => [
     "function_declaration",  // JS/TS
     "method_definition",     // JS/TS
     "class_declaration",     // JS/TS
   ].includes(t);
   ```

   **Improvement:** Per-language definition types
   ```rust
   match language {
       Language::Rust => vec![
           "function_item", "impl_item", "struct_item",
           "enum_item", "trait_item", "mod_item"
       ],
       Language::Python => vec![
           "function_definition", "class_definition"
       ],
       // ...
   }
   ```

2. **No Docstring Extraction**

   **Problem:** Just grabs entire node text
   ```typescript
   chunks.push({ content: node.text });  // Includes docs
   ```

   **Improvement:** Separate docs from code
   ```rust
   let (docstring, code) = extract_docs_and_code(node);
   chunk.docstring = docstring;
   chunk.content = code;
   ```

3. **No Signature Extraction**

   **Problem:** No separate signature field

   **Improvement:** Extract signatures for better context
   ```rust
   // For Rust
   signature: Some("fn sort<T: Ord>(items: Vec<T>) -> Vec<T>")

   // For Python
   signature: Some("def process(data: List[str]) -> Dict[str, int]")
   ```

4. **Simple Classification**

   **Problem:** Only function/class/other
   ```typescript
   if (t.includes("class")) return "class";
   if (isDefType(t)) return "function";
   return "other";
   ```

   **Improvement:** Rich classification
   ```rust
   enum ChunkKind {
       Function,   // fn foo()
       Method,     // impl block methods
       Class,      // (for non-Rust)
       Struct,     // struct Foo
       Enum,       // enum Bar
       Trait,      // trait Baz
       Impl,       // impl Foo
       Mod,        // mod utils
       Block,      // gaps
       Anchor,     // file summary
   }
   ```

5. **Regex Name Extraction**

   **Problem:** Fallback to regex
   ```typescript
   const match = node.text.match(/(?:class|function)\s+([A-Za-z0-9_$]+)/);
   ```

   **Improvement:** Use tree-sitter properly per language
   ```rust
   // Rust: Use field names
   node.child_by_field_name("name")?

   // Python: Search for identifier child
   node.children().find(|c| c.kind() == "identifier")?
   ```

6. **No Type Information**

   **Problem:** Doesn't extract types

   **Improvement:** Extract parameter/return types
   ```rust
   // For Rust
   parameters: Some("items: Vec<T>")
   return_type: Some("Vec<T>")
   generics: Some("<T: Ord>")
   ```

7. **Gap Text Always "block"**

   **Problem:** No differentiation of gap content

   **Improvement:** Classify gaps
   ```rust
   if gap_contains_imports() {
       kind: ChunkKind::Imports
   } else if gap_is_module_docs() {
       kind: ChunkKind::ModuleDocs
   } else {
       kind: ChunkKind::Block
   }
   ```

## demongrep's Improved Design

### Architecture

```rust
pub struct SemanticChunker {
    parser: CodeParser,
    config: ChunkConfig,
    extractors: HashMap<Language, Box<dyn LanguageExtractor>>,
}

trait LanguageExtractor {
    fn definition_types(&self) -> Vec<&'static str>;
    fn extract_name(&self, node: Node, source: &[u8]) -> Option<String>;
    fn extract_signature(&self, node: Node, source: &[u8]) -> Option<String>;
    fn extract_docstring(&self, node: Node, source: &[u8]) -> Option<String>;
    fn classify(&self, node: Node) -> ChunkKind;
}

struct RustExtractor;
struct PythonExtractor;
struct TypeScriptExtractor;
// ... more languages
```

### Per-Language Extractors

#### Rust Extractor

```rust
impl LanguageExtractor for RustExtractor {
    fn definition_types(&self) -> Vec<&'static str> {
        vec![
            "function_item",
            "struct_item",
            "enum_item",
            "impl_item",
            "trait_item",
            "type_item",
            "mod_item",
        ]
    }

    fn extract_name(&self, node: Node, source: &[u8]) -> Option<String> {
        // Rust has consistent "name" field
        node.child_by_field_name("name")?
            .utf8_text(source)
            .ok()
            .map(String::from)
    }

    fn extract_signature(&self, node: Node, source: &[u8]) -> Option<String> {
        match node.kind() {
            "function_item" => {
                // Extract: fn name<T>(params) -> Return
                let mut sig = String::from("fn ");

                if let Some(name) = node.child_by_field_name("name") {
                    sig.push_str(name.utf8_text(source).ok()?);
                }

                if let Some(params) = node.child_by_field_name("parameters") {
                    sig.push_str(params.utf8_text(source).ok()?);
                }

                if let Some(ret) = node.child_by_field_name("return_type") {
                    sig.push_str(" -> ");
                    sig.push_str(ret.utf8_text(source).ok()?);
                }

                Some(sig)
            }
            _ => None,
        }
    }

    fn extract_docstring(&self, node: Node, source: &[u8]) -> Option<String> {
        // Look for doc comments (///, /** */) before the node
        let start_byte = node.start_byte();

        // Search backwards for doc comments
        // ...
    }

    fn classify(&self, node: Node) -> ChunkKind {
        match node.kind() {
            "function_item" => ChunkKind::Function,
            "impl_item" => ChunkKind::Impl,
            "struct_item" => ChunkKind::Struct,
            "enum_item" => ChunkKind::Enum,
            "trait_item" => ChunkKind::Trait,
            "mod_item" => ChunkKind::Mod,
            _ => ChunkKind::Other,
        }
    }
}
```

#### Python Extractor

```rust
impl LanguageExtractor for PythonExtractor {
    fn definition_types(&self) -> Vec<&'static str> {
        vec!["function_definition", "class_definition"]
    }

    fn extract_name(&self, node: Node, source: &[u8]) -> Option<String> {
        // Python uses "name" field too
        node.child_by_field_name("name")?
            .utf8_text(source)
            .ok()
            .map(String::from)
    }

    fn extract_signature(&self, node: Node, source: &[u8]) -> Option<String> {
        match node.kind() {
            "function_definition" => {
                // Extract: def name(params) -> Return:
                let mut sig = String::from("def ");

                if let Some(name) = node.child_by_field_name("name") {
                    sig.push_str(name.utf8_text(source).ok()?);
                }

                if let Some(params) = node.child_by_field_name("parameters") {
                    sig.push_str(params.utf8_text(source).ok()?);
                }

                // Python 3.5+ return type annotation
                if let Some(ret) = node.child_by_field_name("return_type") {
                    sig.push_str(" -> ");
                    sig.push_str(ret.utf8_text(source).ok()?);
                }

                Some(sig)
            }
            "class_definition" => {
                // Extract: class Name(Base):
                let mut sig = String::from("class ");

                if let Some(name) = node.child_by_field_name("name") {
                    sig.push_str(name.utf8_text(source).ok()?);
                }

                if let Some(bases) = node.child_by_field_name("superclasses") {
                    sig.push_str(bases.utf8_text(source).ok()?);
                }

                Some(sig)
            }
            _ => None,
        }
    }

    fn extract_docstring(&self, node: Node, source: &[u8]) -> Option<String> {
        // Python: Look for string literal as first statement
        if let Some(body) = node.child_by_field_name("body") {
            for child in body.named_children(&mut body.walk()) {
                if child.kind() == "expression_statement" {
                    if let Some(string) = child.child(0) {
                        if string.kind() == "string" {
                            return string.utf8_text(source).ok().map(String::from);
                        }
                    }
                }
                break;  // Only check first statement
            }
        }
        None
    }

    fn classify(&self, node: Node) -> ChunkKind {
        match node.kind() {
            "function_definition" => {
                // Check if it's a method (inside class)
                if let Some(parent) = node.parent() {
                    if parent.kind() == "class_definition" {
                        return ChunkKind::Method;
                    }
                }
                ChunkKind::Function
            }
            "class_definition" => ChunkKind::Class,
            _ => ChunkKind::Other,
        }
    }
}
```

### Chunking Algorithm

```rust
impl SemanticChunker {
    pub fn chunk(&mut self, language: Language, path: &Path, content: &str)
        -> Result<Vec<Chunk>>
    {
        // 1. Parse the code
        let parsed = self.parser.parse(language, content)?;

        // 2. Get language-specific extractor
        let extractor = self.extractors.get(&language)
            .ok_or_else(|| anyhow!("No extractor for {}", language.name()))?;

        // 3. Visit AST and extract chunks
        let mut chunks = Vec::new();
        let mut gaps = Vec::new();

        let file_context = format!("File: {}", path.display());
        self.visit_node(
            parsed.root_node(),
            &parsed,
            &**extractor,
            &[file_context],
            &mut chunks,
            &mut gaps,
        );

        // 4. Combine definition chunks with gap chunks
        let mut all_chunks = chunks;
        all_chunks.extend(gaps);
        all_chunks.sort_by_key(|c| c.start_line);

        // 5. Split oversized chunks
        let final_chunks = all_chunks
            .into_iter()
            .flat_map(|c| self.split_if_needed(c))
            .collect();

        Ok(final_chunks)
    }

    fn visit_node(
        &self,
        node: Node,
        parsed: &ParsedCode,
        extractor: &dyn LanguageExtractor,
        context_stack: &[String],
        chunks: &mut Vec<Chunk>,
        gaps: &mut Vec<Chunk>,
    ) {
        // Check if this is a definition
        if extractor.definition_types().contains(&node.kind()) {
            // Extract metadata
            let name = extractor.extract_name(node, parsed.source().as_bytes());
            let signature = extractor.extract_signature(node, parsed.source().as_bytes());
            let docstring = extractor.extract_docstring(node, parsed.source().as_bytes());
            let kind = extractor.classify(node);

            // Build label
            let label = match (kind, &name) {
                (ChunkKind::Function, Some(n)) => format!("Function: {}", n),
                (ChunkKind::Class, Some(n)) => format!("Class: {}", n),
                (ChunkKind::Method, Some(n)) => format!("Method: {}", n),
                (ChunkKind::Struct, Some(n)) => format!("Struct: {}", n),
                (ChunkKind::Enum, Some(n)) => format!("Enum: {}", n),
                _ => format!("{:?}", kind),
            };

            // Build context
            let mut new_context = context_stack.to_vec();
            new_context.push(label);

            // Create chunk
            let content = parsed.node_text(node).unwrap_or("").to_string();
            let mut chunk = Chunk::new(
                content,
                node.start_position().row,
                node.end_position().row,
                kind,
                // path from context_stack
            );
            chunk.context = new_context.clone();
            chunk.signature = signature;
            chunk.docstring = docstring;

            chunks.push(chunk);

            // Visit children with updated context
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                self.visit_node(child, parsed, extractor, &new_context, chunks, gaps);
            }
        } else {
            // Not a definition, visit children
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                self.visit_node(child, parsed, extractor, context_stack, chunks, gaps);
            }
        }
    }
}
```

## Key Improvements Summary

| Feature | osgrep | demongrep |
|---------|--------|-----------|
| **Language support** | Hardcoded JS/TS | Pluggable extractors |
| **Docstrings** | Not extracted | Separate field |
| **Signatures** | Not extracted | Extracted per language |
| **Classification** | 3 types | 10+ types |
| **Type info** | No | Yes (params, returns) |
| **Name extraction** | Regex fallback | Tree-sitter fields |
| **Gap handling** | All "block" | Classified |

## Next: Implementation Plan

1. Create `LanguageExtractor` trait
2. Implement `RustExtractor`
3. Implement `PythonExtractor`
4. Implement `TypeScriptExtractor`
5. Create `SemanticChunker` with visiting logic
6. Add comprehensive tests
7. Benchmark vs osgrep
