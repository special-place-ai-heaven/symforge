pub mod ast_grep;
pub mod config_extractors;
#[cfg(test)]
mod inline_tests;
pub mod languages;
pub mod xref;

use std::collections::HashMap;
use std::panic;

use tree_sitter::Parser;

use tree_sitter::Node;

use crate::domain::{
    FileClassification, FileOutcome, FileProcessingResult, LanguageId, ParseDiagnostic,
    ReferenceRecord, SymbolRecord,
};
use crate::hash::digest_hex;

type ParseSourceOutput = (
    Vec<SymbolRecord>,
    bool,
    Option<ParseDiagnostic>,
    Vec<ReferenceRecord>,
    HashMap<String, String>,
);

pub fn process_file(
    relative_path: &str,
    bytes: &[u8],
    language: LanguageId,
) -> FileProcessingResult {
    process_file_with_classification(
        relative_path,
        bytes,
        language,
        FileClassification::for_code_path(relative_path),
    )
}

pub fn process_file_with_classification(
    relative_path: &str,
    bytes: &[u8],
    language: LanguageId,
    classification: FileClassification,
) -> FileProcessingResult {
    let byte_len = bytes.len() as u64;
    let content_hash = digest_hex(bytes);

    // Config files use native parsers, not tree-sitter.
    if config_extractors::is_config_language(&language) {
        let result = config_extractors::extractor_for(&language).map(|e| e.extract(bytes));
        let (symbols, outcome, parse_diagnostic) = match result {
            Some(r) => {
                let (outcome, parse_diagnostic) = match r.outcome {
                    config_extractors::ExtractionOutcome::Ok => (FileOutcome::Processed, None),
                    config_extractors::ExtractionOutcome::Partial(diagnostic) => (
                        FileOutcome::PartialParse {
                            warning: diagnostic.summary(),
                        },
                        Some(diagnostic),
                    ),
                    config_extractors::ExtractionOutcome::Failed(diagnostic) => (
                        FileOutcome::Failed {
                            error: diagnostic.summary(),
                        },
                        Some(diagnostic),
                    ),
                };
                (r.symbols, outcome, parse_diagnostic)
            }
            None => (vec![], FileOutcome::Processed, None),
        };
        return FileProcessingResult {
            relative_path: relative_path.to_string(),
            language,
            classification,
            outcome,
            parse_diagnostic,
            symbols,
            byte_len,
            content_hash,
            references: vec![],
            alias_map: HashMap::new(),
        };
    }

    let source = String::from_utf8_lossy(bytes);

    let parse_result = panic::catch_unwind(|| parse_source(&source, &language));

    match parse_result {
        Ok(Ok((symbols, has_error, diagnostic, references, alias_map))) => {
            let outcome = if has_error {
                let warning = diagnostic.as_ref().map(|d| d.summary()).unwrap_or_else(|| {
                    "tree-sitter reported syntax errors in the parse tree".to_string()
                });
                FileOutcome::PartialParse { warning }
            } else {
                FileOutcome::Processed
            };
            FileProcessingResult {
                relative_path: relative_path.to_string(),
                language,
                classification,
                outcome,
                parse_diagnostic: diagnostic,
                symbols,
                byte_len,
                content_hash,
                references,
                alias_map,
            }
        }
        Ok(Err(err)) => FileProcessingResult {
            relative_path: relative_path.to_string(),
            language,
            classification,
            outcome: FileOutcome::Failed {
                error: err.to_string(),
            },
            parse_diagnostic: None,
            symbols: vec![],
            byte_len,
            content_hash,
            references: vec![],
            alias_map: HashMap::new(),
        },
        Err(_panic) => FileProcessingResult {
            relative_path: relative_path.to_string(),
            language,
            classification,
            outcome: FileOutcome::Failed {
                error: "tree-sitter parser panicked during parsing".to_string(),
            },
            parse_diagnostic: None,
            symbols: vec![],
            byte_len,
            content_hash,
            references: vec![],
            alias_map: HashMap::new(),
        },
    }
}

/// Walk the tree-sitter tree and collect info about the first ERROR or MISSING node.
/// Returns (message, line, column, byte_span) for building a `ParseDiagnostic`.
fn collect_first_error_node(root: &Node, source: &str) -> Option<(String, u32, u32, (u32, u32))> {
    let mut cursor = root.walk();
    let mut stack = vec![*root];
    while let Some(node) = stack.pop() {
        if node.is_error() || node.is_missing() {
            let start = node.start_position();
            let snippet_start = node.start_byte();
            // Clamp the 40-byte snippet window down to the nearest UTF-8 char
            // boundary — tree-sitter reports byte offsets, and `snippet_start +
            // 40` can land mid-multibyte-char, which would panic str slicing.
            let mut snippet_end = node.end_byte().min(snippet_start + 40).min(source.len());
            while snippet_end > snippet_start && !source.is_char_boundary(snippet_end) {
                snippet_end -= 1;
            }
            let snippet = &source[snippet_start..snippet_end];
            let kind = if node.is_missing() {
                "missing"
            } else {
                "error"
            };
            let message = format!("syntax {kind} near `{}`", snippet.replace('\n', "\\n"));
            return Some((
                message,
                start.row as u32 + 1,    // 1-based line
                start.column as u32 + 1, // 1-based column
                (node.start_byte() as u32, node.end_byte() as u32),
            ));
        }
        // Push children in reverse so we visit left-to-right via the stack.
        cursor.reset(node);
        if cursor.goto_first_child() {
            let mut children = vec![cursor.node()];
            while cursor.goto_next_sibling() {
                children.push(cursor.node());
            }
            stack.extend(children.into_iter().rev());
        }
    }
    None
}

pub(crate) fn parse_source(
    source: &str,
    language: &LanguageId,
) -> Result<ParseSourceOutput, String> {
    let mut parser = Parser::new();

    let ts_language = match language {
        LanguageId::Rust => tree_sitter_rust::LANGUAGE.into(),
        LanguageId::Python => tree_sitter_python::LANGUAGE.into(),
        LanguageId::JavaScript => tree_sitter_javascript::LANGUAGE.into(),
        LanguageId::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        LanguageId::Go => tree_sitter_go::LANGUAGE.into(),
        LanguageId::Java => tree_sitter_java::LANGUAGE.into(),
        LanguageId::C => tree_sitter_c::LANGUAGE.into(),
        LanguageId::Cpp => tree_sitter_cpp::LANGUAGE.into(),
        LanguageId::CSharp => tree_sitter_c_sharp::LANGUAGE.into(),
        LanguageId::Ruby => tree_sitter_ruby::LANGUAGE.into(),
        LanguageId::Php => tree_sitter_php::LANGUAGE_PHP.into(),
        LanguageId::Swift => tree_sitter_swift::LANGUAGE.into(),
        LanguageId::Perl => tree_sitter_perl::LANGUAGE.into(),
        LanguageId::Kotlin => tree_sitter_kotlin_sg::LANGUAGE.into(),
        LanguageId::Dart => tree_sitter_dart::language(),
        LanguageId::Elixir => tree_sitter_elixir::LANGUAGE.into(),
        LanguageId::Json
        | LanguageId::Toml
        | LanguageId::Yaml
        | LanguageId::Markdown
        | LanguageId::Env => unreachable!("config types are handled before parse_source"),
        LanguageId::Html => tree_sitter_html::LANGUAGE.into(),
        LanguageId::Css => tree_sitter_css::LANGUAGE.into(),
        LanguageId::Scss => tree_sitter_scss::language(),
    };

    parser
        .set_language(&ts_language)
        .map_err(|e| format!("failed to set language: {e}"))?;

    let tree = parser
        .parse(source, None)
        .ok_or_else(|| "tree-sitter parse returned None".to_string())?;

    let root = tree.root_node();
    let has_error = root.has_error();
    let symbols = languages::extract_symbols(&root, source, language);
    let (references, alias_map) = xref::extract_references(&root, source, language);

    let diagnostic = if has_error {
        collect_first_error_node(&root, source).map(|(message, line, column, span)| {
            ParseDiagnostic {
                parser: "tree-sitter".to_string(),
                message,
                line: Some(line),
                column: Some(column),
                byte_span: Some(span),
                fallback_used: false,
            }
        })
    } else {
        None
    };

    Ok((symbols, has_error, diagnostic, references, alias_map))
}

/// Extract symbol name → body-hash pairs from source code using tree-sitter.
///
/// Used by `diff_symbols` to compare symbol-level changes between git refs.
/// Falls back to `None` for unsupported or config languages so callers can
/// use the legacy regex extractor.
pub fn extract_symbols_for_diff(source: &str, path: &str) -> Option<Vec<(String, String)>> {
    let ext = path.rsplit('.').next().unwrap_or("");
    let language = LanguageId::from_extension(ext)?;
    if config_extractors::is_config_language(&language) {
        return None; // Config files don't go through tree-sitter.
    }
    let result = panic::catch_unwind(|| parse_source(source, &language));
    let (symbols, ..) = match result {
        Ok(Ok(output)) => output,
        _ => return None,
    };
    let pairs: Vec<(String, String)> = symbols
        .iter()
        .map(|sym| {
            let (start, end) = sym.byte_range;
            let body = &source[start as usize..end as usize];
            (sym.name.clone(), crate::hash::digest_hex(body.as_bytes()))
        })
        .collect();
    Some(pairs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{FileOutcome, LanguageId, SymbolKind};

    #[test]
    fn test_process_file_rust_extracts_function() {
        let source = b"fn hello() { }";
        let result = process_file("test.rs", source, LanguageId::Rust);
        assert_eq!(result.outcome, FileOutcome::Processed);
        assert!(!result.symbols.is_empty());
        assert_eq!(result.symbols[0].name, "hello");
        assert_eq!(result.symbols[0].kind, SymbolKind::Function);
    }

    // Regression guard: tree-sitter-rust must parse `&raw` as a borrow of a
    // variable named `raw`, not as the start of `&raw const`/`&raw mut`.
    // Originally guarded an in-tree byte-rewrite workaround (043b884); now
    // pins the upgraded parser (9f7ff32) against future grammar regressions.
    #[test]
    fn test_process_file_rust_accepts_borrowed_raw_identifier() {
        let source = b"fn main() { let raw = 1; let _x = &raw; }";
        let result = process_file("test.rs", source, LanguageId::Rust);
        assert_eq!(result.outcome, FileOutcome::Processed);
    }

    // Regression guard: tree-sitter-rust must still recognize the raw-borrow
    // syntax `&raw const value` after the upgrade.
    #[test]
    fn test_process_file_rust_preserves_raw_borrow_syntax() {
        let source = b"fn main() { let value = 1; let _ptr = &raw const value; }";
        let result = process_file("test.rs", source, LanguageId::Rust);
        assert_eq!(result.outcome, FileOutcome::Processed);
    }

    #[test]
    fn test_process_file_python_extracts_function() {
        let source = b"def greet():\n    pass";
        let result = process_file("test.py", source, LanguageId::Python);
        assert_eq!(result.outcome, FileOutcome::Processed);
        assert!(!result.symbols.is_empty());
        assert_eq!(result.symbols[0].name, "greet");
        assert_eq!(result.symbols[0].kind, SymbolKind::Function);
    }

    #[test]
    fn test_process_file_javascript_extracts_function() {
        let source = b"function doStuff() { }";
        let result = process_file("test.js", source, LanguageId::JavaScript);
        assert_eq!(result.outcome, FileOutcome::Processed);
        assert!(!result.symbols.is_empty());
        assert_eq!(result.symbols[0].name, "doStuff");
        assert_eq!(result.symbols[0].kind, SymbolKind::Function);
    }

    #[test]
    fn test_process_file_typescript_extracts_interface() {
        let source = b"interface Greeter { greet(): void; }";
        let result = process_file("test.ts", source, LanguageId::TypeScript);
        assert_eq!(result.outcome, FileOutcome::Processed);
        let interface = result
            .symbols
            .iter()
            .find(|s| s.kind == SymbolKind::Interface);
        assert!(interface.is_some());
        assert_eq!(interface.unwrap().name, "Greeter");
    }

    #[test]
    fn test_process_file_go_extracts_function() {
        let source = b"package main\nfunc main() { }";
        let result = process_file("test.go", source, LanguageId::Go);
        assert_eq!(result.outcome, FileOutcome::Processed);
        assert!(!result.symbols.is_empty());
        assert_eq!(result.symbols[0].name, "main");
        assert_eq!(result.symbols[0].kind, SymbolKind::Function);
    }

    #[test]
    fn test_process_file_partial_parse() {
        let source = b"fn broken( { }";
        let result = process_file("bad.rs", source, LanguageId::Rust);
        assert!(matches!(result.outcome, FileOutcome::PartialParse { .. }));
    }

    #[test]
    fn test_process_file_partial_parse_has_diagnostic() {
        let source = b"fn broken( { }";
        let result = process_file("bad.rs", source, LanguageId::Rust);
        assert!(matches!(result.outcome, FileOutcome::PartialParse { .. }));
        let diag = result
            .parse_diagnostic
            .expect("should have a diagnostic for partial parse");
        assert_eq!(diag.parser, "tree-sitter");
        assert!(diag.line.is_some(), "diagnostic should have a line number");
        assert!(
            diag.column.is_some(),
            "diagnostic should have a column number"
        );
        assert!(
            diag.byte_span.is_some(),
            "diagnostic should have a byte span"
        );
        assert!(
            diag.message.contains("syntax"),
            "message should describe the error"
        );
    }

    #[test]
    fn test_process_file_partial_parse_diagnostic_pins_location() {
        // Tightens the ParseDiagnostic contract that validate_file_syntax relies on:
        // a partial parse must pinpoint the actually-broken line, not just return
        // Some(1) for everything. The source below has two clean lines followed
        // by a broken one, so a regression that loses multi-line tracking or
        // hardcodes line 1 would slip past the existing is_some()-only test but
        // fail here.
        let source = b"fn foo() {}\nfn bar() {}\nfn broken( { }";
        let result = process_file("multi.rs", source, LanguageId::Rust);

        assert!(
            matches!(result.outcome, FileOutcome::PartialParse { .. }),
            "source with a line-3 syntax error should be partial-parsed; got {:?}",
            result.outcome
        );

        let diag = result
            .parse_diagnostic
            .expect("partial parse must attach a ParseDiagnostic");

        assert_eq!(diag.parser, "tree-sitter");
        assert!(
            !diag.fallback_used,
            "tree-sitter parses must not set fallback_used; that flag is reserved \
             for config extractors that recover via a secondary parser"
        );

        // Line is 1-based and must track the actual error row. Line 3 is where
        // `fn broken( { }` lives (bytes 24..).
        let line = diag.line.expect("diagnostic must carry a line number");
        assert!(
            line >= 3,
            "error is on line 3 of the source; diagnostic reported line {line}"
        );

        let column = diag.column.expect("diagnostic must carry a column number");
        assert!(column >= 1, "columns are 1-based; got {column}");

        // Byte span must be ordered and inside the source, and must land on
        // line 3 (which starts at byte 24: "fn foo() {}\n" + "fn bar() {}\n").
        // Note: tree-sitter MISSING nodes are zero-width (start == end), so we
        // allow span_start == span_end but require start <= end.
        let (span_start, span_end) = diag
            .byte_span
            .expect("diagnostic must carry a byte span for downstream editors");
        assert!(
            span_start <= span_end,
            "byte_span must be ordered; got {span_start}..{span_end}"
        );
        assert!(
            (span_end as usize) <= source.len(),
            "byte_span must fit inside source (len {}); got end {span_end}",
            source.len()
        );
        assert!(
            span_start >= 24,
            "byte_span should point at line 3 content (starts at byte 24); got {span_start}"
        );

        // location_display is what validate_file_syntax / get_file_context use
        // to render "(line X, column Y)" in tool output. Both must flow through.
        let loc = diag
            .location_display()
            .expect("location_display must render when both line and column are present");
        assert!(
            loc.contains(&format!("line {line}")),
            "location_display must include the line; got {loc}"
        );
        assert!(
            loc.contains(&format!("column {column}")),
            "location_display must include the column; got {loc}"
        );

        // summary() is what feeds into FileOutcome::PartialParse { warning } —
        // pin that the warning carries the structured location, not just the bare
        // message, so the index-health "partial files" path shows actionable info.
        let summary = diag.summary();
        assert!(
            summary.contains("tree-sitter:"),
            "summary should prefix with parser name; got {summary}"
        );
        assert!(
            summary.contains(&format!("line {line}")),
            "summary should include location for downstream display; got {summary}"
        );
    }

    #[test]
    fn test_process_file_computes_content_hash() {
        let source = b"fn foo() {}";
        let result = process_file("hash_test.rs", source, LanguageId::Rust);
        assert!(!result.content_hash.is_empty());
        assert_eq!(result.content_hash, digest_hex(source));
    }

    #[test]
    fn test_process_file_byte_len() {
        let source = b"fn bar() {}";
        let result = process_file("len.rs", source, LanguageId::Rust);
        assert_eq!(result.byte_len, source.len() as u64);
    }

    #[test]
    fn test_process_file_preserves_relative_path() {
        let result = process_file("src/lib.rs", b"fn x() {}", LanguageId::Rust);
        assert_eq!(result.relative_path, "src/lib.rs");
    }

    #[test]
    fn test_process_file_never_panics_on_adversarial_input() {
        // Verifies the catch_unwind safety net: process_file must ALWAYS
        // return a FileProcessingResult regardless of input, never propagate a panic.
        let cases: &[(&[u8], &str, LanguageId)] = &[
            (b"\xff\xfe\x00\x01", "binary.rs", LanguageId::Rust),
            (b"", "empty.py", LanguageId::Python),
            (&[0u8; 10000], "zeros.js", LanguageId::JavaScript),
            (b"\n\n\n\n\n", "newlines.ts", LanguageId::TypeScript),
            ("\u{200b}\u{200b}".as_bytes(), "zwsp.go", LanguageId::Go),
            (
                b"\0\0\0fn main() {}\0\0",
                "null_padded.rs",
                LanguageId::Rust,
            ),
        ];

        for &(source, path, ref lang) in cases {
            let result = process_file(path, source, lang.clone());
            assert_eq!(result.relative_path, path);
            assert_eq!(result.byte_len, source.len() as u64);
            assert!(!result.content_hash.is_empty());
        }
    }

    #[test]
    fn test_process_file_ruby_extracts_method() {
        let source = b"def hello\n  puts 'hi'\nend";
        let result = process_file("app.rb", source, LanguageId::Ruby);
        assert_eq!(result.outcome, FileOutcome::Processed);
        assert!(
            !result.symbols.is_empty(),
            "should have symbols for Ruby source"
        );
    }

    #[test]
    fn test_process_file_is_idempotent_across_re_parses() {
        // Incremental re-indexing in src/watcher/ assumes `process_file` is
        // idempotent: identical bytes must yield an identical
        // `FileProcessingResult`. A subtle non-determinism (e.g. HashMap
        // iteration order leaking into the symbol/reference Vec ordering)
        // would cause phantom xref churn on every touch of an unchanged file.
        //
        // Exercises multiple languages plus a partial-parse case so both the
        // normal extractor path and the `collect_first_error_node` diagnostic
        // path are pinned.
        let rust_src: &[u8] = b"use std::collections::HashMap;\n\
            use std::fmt::Display;\n\
            \n\
            pub struct Cache<K, V> {\n\
                store: HashMap<K, V>,\n\
            }\n\
            \n\
            impl<K: std::hash::Hash + Eq, V> Cache<K, V> {\n\
                pub fn new() -> Self { Self { store: HashMap::new() } }\n\
                pub fn insert(&mut self, k: K, v: V) -> Option<V> { self.store.insert(k, v) }\n\
            }\n\
            \n\
            pub fn make() -> Cache<String, u32> { Cache::new() }\n";

        let python_src: &[u8] = b"import os\n\
            import sys\n\
            \n\
            class Greeter:\n\
                def __init__(self, name):\n\
                    self.name = name\n\
            \n\
                def greet(self):\n\
                    return f\"hello {self.name}\"\n\
            \n\
            def main():\n\
                Greeter(\"world\").greet()\n";

        let ts_src: &[u8] = b"interface Greeter { greet(): string; }\n\
            export class Hello implements Greeter {\n\
                constructor(private name: string) {}\n\
                greet() { return `hi ${this.name}`; }\n\
            }\n";

        // Syntactically broken Rust — exercises the partial-parse path so the
        // diagnostic (line/column/byte_span) must also be deterministic.
        let broken_src: &[u8] = b"fn broken( { }";

        let cases: &[(&[u8], &str, LanguageId)] = &[
            (rust_src, "cache.rs", LanguageId::Rust),
            (python_src, "greet.py", LanguageId::Python),
            (ts_src, "hello.ts", LanguageId::TypeScript),
            (broken_src, "broken.rs", LanguageId::Rust),
        ];

        for &(source, path, ref lang) in cases {
            let first = process_file(path, source, lang.clone());
            let second = process_file(path, source, lang.clone());
            let third = process_file(path, source, lang.clone());
            assert_eq!(
                first, second,
                "{path}: identical bytes must yield identical FileProcessingResult (run 1 vs 2)"
            );
            assert_eq!(
                second, third,
                "{path}: identical bytes must yield identical FileProcessingResult (run 2 vs 3)"
            );
        }
    }

    // --- Parser resilience audit (parse_source / collect_first_error_node) ---
    //
    // The adversarial `process_file` test above proves the `catch_unwind` safety
    // net holds end-to-end. The tests below probe `parse_source` directly (no
    // catch_unwind wrapper) so a panic in the parse path fails the test instead
    // of being silently downgraded to a `Failed` outcome.

    #[test]
    fn test_parse_source_zero_bytes() {
        let result = parse_source("", &LanguageId::Rust)
            .expect("parse_source must handle empty input without error");
        let (symbols, _has_error, _diagnostic, references, alias_map) = result;
        assert!(symbols.is_empty(), "empty source has no symbols");
        assert!(references.is_empty(), "empty source has no references");
        assert!(alias_map.is_empty(), "empty source has no aliases");
    }

    #[test]
    fn test_parse_source_null_bytes_only() {
        // 4 KiB of NUL: valid UTF-8, no valid tokens in any grammar.
        // Exercises parse_source + collect_first_error_node (if has_error) on a
        // file that tree-sitter must reject-as-syntax-error rather than crash.
        let source: String = "\0".repeat(4096);
        let result = parse_source(&source, &LanguageId::Rust)
            .expect("parse_source must handle null-byte input without error");
        let (_symbols, _has_error, _diagnostic, _references, _aliases) = result;
    }

    #[test]
    fn test_parse_source_wide_multibyte_error_region_no_panic() {
        // Regression probe for collect_first_error_node:
        //   let snippet_end = node.end_byte().min(snippet_start + 40);
        //   let snippet = &source[snippet_start..snippet_end];
        //
        // If a multi-byte UTF-8 char straddles byte `snippet_start + 40`, naive
        // slicing panics ("byte index is not a char boundary"). 100 × '€'
        // (3 bytes each = 300 bytes, all non-ASCII) is not a valid Rust token
        // stream, so tree-sitter must produce one or more ERROR nodes that
        // could span > 40 bytes of multi-byte content.
        let source: String = "€".repeat(100);
        parse_source(&source, &LanguageId::Rust)
            .expect("parse_source must not panic on wide multi-byte error region");
    }

    #[test]
    fn test_parse_source_mixed_multibyte_error_boundary_no_panic() {
        // Similar char-boundary probe but with a grammar-level syntax error
        // interleaved with multi-byte text — guarantees an ERROR node whose
        // [start, start+40) window crosses a UTF-8 char boundary.
        let mut source = String::new();
        source.push_str("struct S { ");
        for _ in 0..14 {
            source.push('€');
        }
        source.push(' ');
        parse_source(&source, &LanguageId::Rust)
            .expect("parse_source must not panic when error snippet spans multi-byte chars");
    }

    #[test]
    fn test_process_file_deeply_nested_expression_no_stack_blow() {
        // Stack-blow probe: 10 000 nested parens. Per-language extractors walk
        // the AST recursively (see `walk_node` → `walk_children` → `walk_node`
        // in src/parsing/languages/rust.rs L19-50). Default Rust test-thread
        // stacks (~2 MiB) overflow around ~6 k frames, so the probe runs on a
        // dedicated thread with a 16 MiB stack to verify the parse logic itself
        // terminates. A real stack-blow on this depth would abort the test
        // process; we want to observe it here rather than in production.
        const DEPTH: usize = 10_000;
        let handle = std::thread::Builder::new()
            .stack_size(16 * 1024 * 1024)
            .spawn(|| {
                let mut source = String::with_capacity(DEPTH * 2 + 32);
                source.push_str("fn f() -> i32 { ");
                for _ in 0..DEPTH {
                    source.push('(');
                }
                source.push('1');
                for _ in 0..DEPTH {
                    source.push(')');
                }
                source.push_str(" }");
                let result = process_file("deep.rs", source.as_bytes(), LanguageId::Rust);
                assert_eq!(result.byte_len, source.len() as u64);
                assert!(!result.content_hash.is_empty());
            })
            .expect("spawn stack-blow probe thread");
        handle.join().expect("deep-nesting probe must not panic");
    }

    #[test]
    fn test_process_file_deeply_nested_expression_on_default_stack_no_panic() {
        // Companion to `test_process_file_deeply_nested_expression_no_stack_blow`
        // above — same input, but runs on the default test-thread stack (~2 MiB
        // on Linux/macOS, ~1 MiB on Windows) without the 16 MiB override. The
        // previous swarm round flagged the real risk: daemon-proxy sessions and
        // other caller threads with default-sized stacks would overflow on
        // adversarial input. The `MAX_AST_WALK_DEPTH` cap in
        // `src/parsing/languages/mod.rs` silently truncates the AST walk when
        // recursion reaches the cap, so this now returns cleanly.
        //
        // This test doesn't assert anything about the resulting symbol count —
        // depth-capped partial walks are allowed to drop inner symbols, matching
        // the rest of the parser's partial-parse philosophy. It only asserts
        // that the call returns *at all* without crashing the test process.
        const DEPTH: usize = 10_000;
        let mut source = String::with_capacity(DEPTH * 2 + 32);
        source.push_str("fn f() -> i32 { ");
        for _ in 0..DEPTH {
            source.push('(');
        }
        source.push('1');
        for _ in 0..DEPTH {
            source.push(')');
        }
        source.push_str(" }");
        let result = process_file("deep_default.rs", source.as_bytes(), LanguageId::Rust);
        assert_eq!(result.byte_len, source.len() as u64);
        assert!(!result.content_hash.is_empty());
    }
}
