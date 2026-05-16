automod::dir!("src/parsing/languages");

use tree_sitter::Node;

use crate::domain::{LanguageId, SymbolKind, SymbolRecord};

/// Per-language configuration for detecting doc comments.
pub(super) struct DocCommentSpec {
    /// Tree-sitter node type names that could be doc comments.
    pub comment_node_types: &'static [&'static str],
    /// Text prefixes that distinguish doc from regular comments.
    /// `None` = all comments of matching node types are doc comments.
    pub doc_prefixes: Option<&'static [&'static str]>,
    /// Optional custom check for non-comment doc patterns (e.g., Elixir `@doc`).
    pub custom_doc_check: Option<fn(&Node, &str) -> bool>,
}

/// Spec for languages with no doc comment detection (Python, Dart).
pub(super) const NO_DOC_SPEC: DocCommentSpec = DocCommentSpec {
    comment_node_types: &[],
    doc_prefixes: None,
    custom_doc_check: None,
};

/// Walk backward through `node`'s preceding siblings to find attached doc comments.
/// Returns `Some((earliest_start_byte, latest_end_byte))` or `None`.
pub(super) fn scan_doc_range(
    node: &Node,
    source: &str,
    spec: &DocCommentSpec,
) -> Option<(u32, u32)> {
    if spec.comment_node_types.is_empty() && spec.custom_doc_check.is_none() {
        return None;
    }

    let mut earliest_start: Option<u32> = None;
    let mut latest_end: Option<u32> = None;
    let mut next_start_byte = node.start_byte();
    let mut sibling_opt = node.prev_sibling();

    while let Some(sibling) = sibling_opt {
        let is_comment_node = spec.comment_node_types.contains(&sibling.kind());
        let is_custom_doc = spec
            .custom_doc_check
            .is_some_and(|check| check(&sibling, source));

        if !is_comment_node && !is_custom_doc {
            break;
        }

        // Blank line check: strip trailing whitespace from the sibling's
        // span to find the content end, then count newlines between it and
        // the next item's start. 2+ newlines means a blank line.
        // This handles both single-line comments (where some grammars include
        // trailing newlines in the span) and multi-line block comments correctly.
        let mut content_end = sibling.end_byte();
        while content_end > sibling.start_byte()
            && source.as_bytes()[content_end - 1].is_ascii_whitespace()
        {
            content_end -= 1;
        }
        let between = &source.as_bytes()[content_end..next_start_byte];
        if between.iter().filter(|&&b| b == b'\n').count() >= 2 {
            break;
        }

        // If doc_prefixes is set, check the text prefix.
        if is_comment_node && let Some(prefixes) = spec.doc_prefixes {
            let text_start = sibling.start_byte();
            let text_end = sibling.end_byte();
            if text_end <= source.len() {
                let text = &source[text_start..text_end];
                let trimmed = text.trim_start();
                if !prefixes.iter().any(|p| trimmed.starts_with(p)) {
                    break;
                }
            }
        }

        let sb = sibling.start_byte() as u32;
        let eb = sibling.end_byte() as u32;
        earliest_start = Some(earliest_start.map_or(sb, |prev| prev.min(sb)));
        if latest_end.is_none() {
            latest_end = Some(eb);
        }

        next_start_byte = sibling.start_byte();
        sibling_opt = sibling.prev_sibling();
    }

    earliest_start.map(|start| (start, latest_end.unwrap()))
}

type WalkNodeFn = fn(&Node, &str, u32, &mut u32, &mut Vec<SymbolRecord>);

pub fn extract_symbols(node: &Node, source: &str, language: &LanguageId) -> Vec<SymbolRecord> {
    match language {
        LanguageId::Rust => rust::extract_symbols(node, source),
        LanguageId::Python => python::extract_symbols(node, source),
        LanguageId::JavaScript => javascript::extract_symbols(node, source),
        LanguageId::TypeScript => typescript::extract_symbols(node, source),
        LanguageId::Go => go::extract_symbols(node, source),
        LanguageId::Java => java::extract_symbols(node, source),
        LanguageId::C => c::extract_symbols(node, source),
        LanguageId::Cpp => cpp::extract_symbols(node, source),
        LanguageId::CSharp => csharp::extract_symbols(node, source),
        LanguageId::Ruby => ruby::extract_symbols(node, source),
        LanguageId::Php => php::extract_symbols(node, source),
        LanguageId::Swift => swift::extract_symbols(node, source),
        LanguageId::Kotlin => kotlin::extract_symbols(node, source),
        LanguageId::Dart => dart::extract_symbols(node, source),
        LanguageId::Perl => perl::extract_symbols(node, source),
        LanguageId::Elixir => elixir::extract_symbols(node, source),
        LanguageId::Json
        | LanguageId::Toml
        | LanguageId::Yaml
        | LanguageId::Markdown
        | LanguageId::Env => unreachable!("config types are handled before parse_source"),
        LanguageId::Html => html::extract_symbols(node, source),
        LanguageId::Css => css::extract_symbols(node, source),
        LanguageId::Scss => scss::extract_symbols(node, source),
    }
}

pub(super) fn collect_symbols(node: &Node, source: &str, walk: WalkNodeFn) -> Vec<SymbolRecord> {
    let mut symbols = Vec::new();
    let mut sort_order = 0u32;
    walk(node, source, 0, &mut sort_order, &mut symbols);
    symbols
}

pub(super) struct SymbolSink<'a, 'b> {
    source: &'a str,
    sort_order: &'b mut u32,
    symbols: &'b mut Vec<SymbolRecord>,
    doc_spec: &'a DocCommentSpec,
}

impl<'a, 'b> SymbolSink<'a, 'b> {
    pub(super) fn new(
        source: &'a str,
        sort_order: &'b mut u32,
        symbols: &'b mut Vec<SymbolRecord>,
        doc_spec: &'a DocCommentSpec,
    ) -> Self {
        Self {
            source,
            sort_order,
            symbols,
            doc_spec,
        }
    }
}

pub(super) fn push_symbol(
    node: &Node,
    name: String,
    kind: SymbolKind,
    depth: u32,
    sink: &mut SymbolSink<'_, '_>,
) {
    let doc_byte_range = scan_doc_range(node, sink.source, sink.doc_spec);
    let byte_range = (node.start_byte() as u32, node.end_byte() as u32);
    sink.symbols.push(SymbolRecord {
        name,
        kind,
        depth,
        sort_order: *sink.sort_order,
        byte_range,
        line_range: (
            node.start_position().row as u32,
            node.end_position().row as u32,
        ),
        doc_byte_range,
        item_byte_range: Some(byte_range),
    });
    *sink.sort_order += 1;
}

pub(super) fn push_named_symbol<F>(
    node: &Node,
    depth: u32,
    kind: Option<SymbolKind>,
    find_name: F,
    sink: &mut SymbolSink<'_, '_>,
) -> bool
where
    F: FnOnce(&Node, &str, SymbolKind) -> Option<String>,
{
    let Some(symbol_kind) = kind else {
        return false;
    };
    let Some(name) = find_name(node, sink.source, symbol_kind) else {
        return false;
    };
    push_symbol(node, name, symbol_kind, depth, sink);
    true
}

pub(super) fn next_child_depth(kind: Option<SymbolKind>, depth: u32) -> u32 {
    if kind.is_some() { depth + 1 } else { depth }
}

/// Check whether `node` has a direct child with the given tree-sitter node kind.
/// Used to distinguish definitions (which have a body) from type-reference usages.
pub(super) fn has_child_kind(node: &Node, kind: &str) -> bool {
    let mut cursor = node.walk();
    node.children(&mut cursor).any(|c| c.kind() == kind)
}

/// Maximum recursive AST walk depth. Past this, the per-language
/// `walk_children` helpers bail out instead of descending further. Guards
/// against stack overflow on pathologically deep inputs (thousands of
/// nested parens, unterminated macro expansions, hostile source files) when
/// the walk runs on a thread stack smaller than the dedicated 16 MiB
/// indexing probe thread — notably the default-stack daemon proxy sessions
/// and Cargo's default test threads.
///
/// Empirically (see `test_process_file_deeply_nested_expression_no_stack_blow`)
/// a 10 000-deep parse needs roughly 15 MiB of stack. That implies ~1.5 KiB
/// per recursive frame. At a 1024-frame cap we use ~1.5 MiB of stack in the
/// worst case, leaving headroom on a 2 MiB default thread and on the 3 MiB
/// Windows minimum configured in `store::MIN_INDEXING_THREAD_STACK_BYTES`.
///
/// Real code never comes close: even heavily-nested generated Rust or
/// deeply-curried Haskell tops out around 300 levels. Hitting the cap means
/// the input is adversarial or generated; silently truncating the walk is
/// the conservative outcome — the parser still produces symbols for
/// everything it reached, partial-parse style.
pub(super) const MAX_AST_WALK_DEPTH: u32 = 1024;

thread_local! {
    static AST_WALK_DEPTH: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// RAII guard for a single recursive frame of the AST walk. `enter_ast_walk_frame`
/// returns `None` once the thread-local depth counter reaches
/// `MAX_AST_WALK_DEPTH`; the caller must then return without recursing.
/// Dropping the guard decrements the counter, so sibling branches see the
/// correct depth.
pub(super) struct AstWalkFrame;

impl Drop for AstWalkFrame {
    fn drop(&mut self) {
        AST_WALK_DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
    }
}

pub(super) fn enter_ast_walk_frame() -> Option<AstWalkFrame> {
    AST_WALK_DEPTH.with(|d| {
        let current = d.get();
        if current >= MAX_AST_WALK_DEPTH {
            None
        } else {
            d.set(current + 1);
            Some(AstWalkFrame)
        }
    })
}

pub(super) fn walk_children(
    node: &Node,
    source: &str,
    depth: u32,
    sort_order: &mut u32,
    symbols: &mut Vec<SymbolRecord>,
    kind: Option<SymbolKind>,
    walk: WalkNodeFn,
) {
    let Some(_frame) = enter_ast_walk_frame() else {
        // Recursion cap reached — stop descending to avoid a stack
        // overflow on adversarially deep input. Anything collected so far
        // stays; this mirrors partial-parse semantics used elsewhere in
        // the parser.
        return;
    };
    let child_depth = next_child_depth(kind, depth);
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk(&child, source, child_depth, sort_order, symbols);
    }
}

pub(super) fn find_first_named_child(
    node: &Node,
    source: &str,
    child_kinds: &[&str],
) -> Option<String> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child_kinds.iter().any(|kind| child.kind() == *kind) {
            return Some(child.utf8_text(source.as_bytes()).unwrap_or("").to_string());
        }
    }
    None
}

/// Extract the at-rule name: text from the node start up to (but not
/// including) the opening `{`, trimmed. Shared by CSS and SCSS extractors.
pub(super) fn at_rule_name(node: &Node, source: &str) -> String {
    let text = node.utf8_text(source.as_bytes()).unwrap_or("");
    match text.find('{') {
        Some(pos) => text[..pos].trim().to_string(),
        None => text.trim().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use tree_sitter::Parser;

    use super::*;
    use crate::domain::SymbolKind;

    fn parse_rust(source: &str) -> tree_sitter::Tree {
        let mut parser = Parser::new();
        let lang: tree_sitter::Language = tree_sitter_rust::LANGUAGE.into();
        parser.set_language(&lang).expect("set rust grammar");
        parser.parse(source, None).expect("parse rust source")
    }

    fn first_named_descendant<'a>(node: &'a Node<'a>, kind: &str) -> Node<'a> {
        let mut cursor = node.walk();
        node.children(&mut cursor)
            .find(|child| child.kind() == kind)
            .expect("expected descendant")
    }

    #[test]
    fn test_push_named_symbol_records_metadata_and_advances_sort_order() {
        let source = "fn hello() {}\n";
        let tree = parse_rust(source);
        let root = tree.root_node();
        let function = first_named_descendant(&root, "function_item");

        let mut symbols = Vec::new();
        let mut sort_order = 0u32;

        let pushed = {
            let mut sink = SymbolSink::new(source, &mut sort_order, &mut symbols, &NO_DOC_SPEC);
            push_named_symbol(
                &function,
                2,
                Some(SymbolKind::Function),
                |node, source, _kind| find_first_named_child(node, source, &["identifier"]),
                &mut sink,
            )
        };

        assert!(pushed);
        assert_eq!(sort_order, 1);
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "hello");
        assert_eq!(symbols[0].kind, SymbolKind::Function);
        assert_eq!(symbols[0].depth, 2);
        assert_eq!(symbols[0].sort_order, 0);
        assert_eq!(symbols[0].line_range, (0, 0));
        assert_eq!(symbols[0].byte_range, (0, function.end_byte() as u32));
    }

    #[test]
    fn test_next_child_depth_only_increments_for_symbol_parents() {
        assert_eq!(next_child_depth(Some(SymbolKind::Function), 3), 4);
        assert_eq!(next_child_depth(None, 3), 3);
    }

    #[test]
    fn test_find_first_named_child_returns_first_matching_kind() {
        let source = "struct Example;\n";
        let tree = parse_rust(source);
        let root = tree.root_node();
        let item = first_named_descendant(&root, "struct_item");

        let found = find_first_named_child(&item, source, &["type_identifier", "identifier"]);

        assert_eq!(found.as_deref(), Some("Example"));
    }

    // --- scan_doc_range tests ---

    fn parse_go(source: &str) -> tree_sitter::Tree {
        let mut parser = Parser::new();
        let lang: tree_sitter::Language = tree_sitter_go::LANGUAGE.into();
        parser.set_language(&lang).expect("set go grammar");
        parser.parse(source, None).expect("parse go source")
    }

    const RUST_DOC_SPEC: DocCommentSpec = DocCommentSpec {
        comment_node_types: &["line_comment", "block_comment"],
        doc_prefixes: Some(&["///", "//!", "/**", "/*!"]),
        custom_doc_check: None,
    };

    const GO_DOC_SPEC: DocCommentSpec = DocCommentSpec {
        comment_node_types: &["comment"],
        doc_prefixes: None,
        custom_doc_check: None,
    };

    #[test]
    fn test_scan_doc_range_rust_doc_comments() {
        let source = "/// Doc line 1\n/// Doc line 2\npub fn foo() {}\n";
        let tree = parse_rust(source);
        let root = tree.root_node();
        let function = first_named_descendant(&root, "function_item");

        let range = scan_doc_range(&function, source, &RUST_DOC_SPEC);

        assert!(range.is_some(), "expected doc range for /// comments");
        let (start, end) = range.unwrap();
        let doc_text = &source[start as usize..end as usize];
        assert!(
            doc_text.contains("Doc line 1"),
            "should contain first doc line"
        );
        assert!(
            doc_text.contains("Doc line 2"),
            "should contain second doc line"
        );
    }

    #[test]
    fn test_scan_doc_range_regular_comment_not_captured() {
        let source = "// Regular comment\npub fn foo() {}\n";
        let tree = parse_rust(source);
        let root = tree.root_node();
        let function = first_named_descendant(&root, "function_item");

        let range = scan_doc_range(&function, source, &RUST_DOC_SPEC);

        assert!(
            range.is_none(),
            "regular // comment should not be captured as doc"
        );
    }

    #[test]
    fn test_scan_doc_range_blank_line_stops_scan() {
        let source = "/// Detached doc\n\n/// Attached doc\npub fn foo() {}\n";
        let tree = parse_rust(source);
        let root = tree.root_node();
        let function = first_named_descendant(&root, "function_item");

        let range = scan_doc_range(&function, source, &RUST_DOC_SPEC);

        assert!(range.is_some(), "expected doc range for attached comment");
        let (start, end) = range.unwrap();
        let doc_text = &source[start as usize..end as usize];
        assert!(
            doc_text.contains("Attached doc"),
            "should contain attached doc"
        );
        assert!(
            !doc_text.contains("Detached doc"),
            "should NOT contain detached doc"
        );
    }

    #[test]
    fn test_scan_doc_range_no_doc_spec_returns_none() {
        let source = "/// Doc comment\npub fn foo() {}\n";
        let tree = parse_rust(source);
        let root = tree.root_node();
        let function = first_named_descendant(&root, "function_item");

        let range = scan_doc_range(&function, source, &NO_DOC_SPEC);

        assert!(range.is_none(), "NO_DOC_SPEC should always return None");
    }

    #[test]
    fn test_scan_doc_range_all_adjacent_comments_go_style() {
        let source = "// Package doc\n// More doc\nfunc Foo() {}\n";
        let tree = parse_go(source);
        let root = tree.root_node();
        let function = root
            .children(&mut root.walk())
            .find(|child| child.kind() == "function_declaration")
            .expect("expected function_declaration");

        let range = scan_doc_range(&function, source, &GO_DOC_SPEC);

        assert!(range.is_some(), "expected doc range for Go comments");
        let (start, end) = range.unwrap();
        let doc_text = &source[start as usize..end as usize];
        assert!(
            doc_text.contains("Package doc"),
            "should contain first doc line"
        );
        assert!(
            doc_text.contains("More doc"),
            "should contain second doc line"
        );
    }

    // --- extract_symbols integration tests ---

    #[test]
    fn test_extract_symbols_rust_populates_doc_range() {
        // "/// My function\n" = 16 bytes (0..16)
        // "/// Does stuff\n"  = 15 bytes (16..31)
        // "pub fn foo() {}\n" = 16 bytes (31..47)
        let source = "/// My function\n/// Does stuff\npub fn foo() {}\n";
        let mut parser = Parser::new();
        let lang: tree_sitter::Language = tree_sitter_rust::LANGUAGE.into();
        parser.set_language(&lang).expect("set rust grammar");
        let tree = parser.parse(source, None).expect("parse");
        let symbols = extract_symbols(&tree.root_node(), source, &crate::domain::LanguageId::Rust);
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "foo");
        let doc_range = symbols[0]
            .doc_byte_range
            .expect("should have doc_byte_range");
        let doc_text = &source[doc_range.0 as usize..doc_range.1 as usize];
        assert!(
            doc_text.contains("/// My function"),
            "missing first doc line"
        );
        assert!(
            doc_text.contains("/// Does stuff"),
            "missing second doc line"
        );
    }

    #[test]
    fn test_extract_symbols_python_no_doc_range() {
        // Python # comments are never doc comments.
        let source = "# A comment\ndef foo():\n    pass\n";
        let mut parser = Parser::new();
        let lang: tree_sitter::Language = tree_sitter_python::LANGUAGE.into();
        parser.set_language(&lang).expect("set python grammar");
        let tree = parser.parse(source, None).expect("parse");
        let symbols = extract_symbols(
            &tree.root_node(),
            source,
            &crate::domain::LanguageId::Python,
        );
        assert_eq!(symbols.len(), 1);
        assert!(
            symbols[0].doc_byte_range.is_none(),
            "Python # comment should not be detected as doc"
        );
    }

    #[test]
    fn test_extract_symbols_java_javadoc() {
        // "/** Javadoc */\n" = 15 bytes (0..15)
        // "class Foo {}\n"   = 13 bytes (15..28)
        let source = "/** Javadoc */\nclass Foo {}\n";
        let mut parser = Parser::new();
        let lang: tree_sitter::Language = tree_sitter_java::LANGUAGE.into();
        parser.set_language(&lang).expect("set java grammar");
        let tree = parser.parse(source, None).expect("parse");
        let symbols = extract_symbols(&tree.root_node(), source, &crate::domain::LanguageId::Java);
        let class_sym = symbols
            .iter()
            .find(|s| s.name == "Foo")
            .expect("should find Foo");
        let doc_range = class_sym.doc_byte_range.expect("should have Javadoc range");
        let doc_text = &source[doc_range.0 as usize..doc_range.1 as usize];
        assert!(doc_text.contains("/** Javadoc */"), "missing javadoc text");
    }

    #[test]
    fn test_scan_doc_range_multiline_block_comment() {
        // Multi-line Javadoc spanning 3 rows should be captured
        let source = "/**\n * Javadoc\n */\nclass Foo {}\n";
        let mut parser = Parser::new();
        let lang: tree_sitter::Language = tree_sitter_java::LANGUAGE.into();
        parser.set_language(&lang).expect("set java grammar");
        let tree = parser.parse(source, None).expect("parse");
        let symbols = extract_symbols(&tree.root_node(), source, &crate::domain::LanguageId::Java);
        let class_sym = symbols
            .iter()
            .find(|s| s.name == "Foo")
            .expect("should find Foo");
        let doc_range = class_sym
            .doc_byte_range
            .expect("multi-line block comment should be captured");
        let doc_text = &source[doc_range.0 as usize..doc_range.1 as usize];
        assert!(
            doc_text.contains("Javadoc"),
            "multi-line block comment text missing"
        );
    }

    #[test]
    fn test_abi_smoke_html_grammar() {
        let mut parser = Parser::new();
        let lang: tree_sitter::Language = tree_sitter_html::LANGUAGE.into();
        parser.set_language(&lang).expect("set HTML language");
        let tree = parser
            .parse("<div></div>", None)
            .expect("parse HTML snippet");
        assert!(!tree.root_node().has_error(), "root should not be error");
    }

    #[test]
    fn test_abi_smoke_css_grammar() {
        let mut parser = Parser::new();
        let lang: tree_sitter::Language = tree_sitter_css::LANGUAGE.into();
        parser.set_language(&lang).expect("set CSS language");
        let tree = parser
            .parse(".a { color: red; }", None)
            .expect("parse CSS snippet");
        assert!(!tree.root_node().has_error(), "root should not be error");
    }

    #[test]
    fn test_abi_smoke_scss_grammar() {
        let mut parser = Parser::new();
        let lang: tree_sitter::Language = tree_sitter_scss::language();
        parser.set_language(&lang).expect("set SCSS language");
        let tree = parser.parse("$x: 1;", None).expect("parse SCSS snippet");
        assert!(!tree.root_node().has_error(), "root should not be error");
    }

    // --- CR1: non-ASCII doc comment tests ---

    #[test]
    fn test_scan_doc_range_non_ascii_cjk_no_panic() {
        // CJK characters are multi-byte; tree-sitter returns byte offsets that
        // must not be used as char indices into &str.
        let source = "/// 日本語ドキュメント\npub fn foo() {}\n";
        let tree = parse_rust(source);
        let root = tree.root_node();
        let function = first_named_descendant(&root, "function_item");
        // Must not panic; a doc range should be returned.
        let range = scan_doc_range(&function, source, &RUST_DOC_SPEC);
        assert!(
            range.is_some(),
            "expected doc range for non-ASCII Rust doc comment"
        );
        let (start, end) = range.unwrap();
        let doc_bytes = &source.as_bytes()[start as usize..end as usize];
        let doc_text = std::str::from_utf8(doc_bytes).expect("doc range should be valid UTF-8");
        assert!(
            doc_text.contains("日本語"),
            "doc text should contain CJK chars"
        );
    }

    #[test]
    fn test_scan_doc_range_non_ascii_emoji_no_panic() {
        // Emoji are 4-byte sequences; same risk as CJK.
        let source = "/// emoji 🦀 docs\npub fn foo() {}\n";
        let tree = parse_rust(source);
        let root = tree.root_node();
        let function = first_named_descendant(&root, "function_item");
        let range = scan_doc_range(&function, source, &RUST_DOC_SPEC);
        assert!(
            range.is_some(),
            "expected doc range for emoji Rust doc comment"
        );
        let (start, end) = range.unwrap();
        let doc_bytes = &source.as_bytes()[start as usize..end as usize];
        let doc_text = std::str::from_utf8(doc_bytes).expect("doc range should be valid UTF-8");
        assert!(doc_text.contains("🦀"), "doc text should contain emoji");
    }
}
