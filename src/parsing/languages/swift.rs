use tree_sitter::Node;

use super::{
    DocCommentSpec, SymbolSink, collect_symbols, find_first_named_child, push_named_symbol,
    walk_children,
};

pub(super) const DOC_SPEC: DocCommentSpec = DocCommentSpec {
    comment_node_types: &["comment", "multiline_comment"],
    doc_prefixes: Some(&["///", "/**"]),
    custom_doc_check: None,
};
use crate::domain::{SymbolKind, SymbolRecord};

pub fn extract_symbols(node: &Node, source: &str) -> Vec<SymbolRecord> {
    collect_symbols(node, source, walk_node)
}

fn walk_node(
    node: &Node,
    source: &str,
    depth: u32,
    sort_order: &mut u32,
    symbols: &mut Vec<SymbolRecord>,
) {
    let kind = match node.kind() {
        "function_declaration" => Some(SymbolKind::Function),
        "class_declaration" => Some(classify_swift_class(node)),
        "struct_declaration" => Some(SymbolKind::Struct),
        "enum_declaration" => Some(SymbolKind::Enum),
        "protocol_declaration" => Some(SymbolKind::Interface),
        "extension_declaration" => Some(SymbolKind::Impl),
        _ => None,
    };

    {
        let mut sink = SymbolSink::new(source, sort_order, symbols, &DOC_SPEC);
        push_named_symbol(
            node,
            depth,
            kind,
            |node, source, _| find_name(node, source),
            &mut sink,
        );
    }
    walk_children(node, source, depth, sort_order, symbols, kind, walk_node);
}

/// tree-sitter-swift v0.7.1 maps class, struct, enum, and extension all to
/// `class_declaration`. Distinguish them by inspecting children:
/// - `enum_class_body` child → Enum
/// - `name` is `user_type` (not `type_identifier`) → extension (Impl)
/// - otherwise → Class
fn classify_swift_class(node: &Node) -> SymbolKind {
    let mut cursor = node.walk();
    let mut has_enum_body = false;
    let mut first_name_kind: Option<&str> = None;
    for child in node.children(&mut cursor) {
        match child.kind() {
            "enum_class_body" => has_enum_body = true,
            "user_type" | "type_identifier" if first_name_kind.is_none() => {
                first_name_kind = Some(if child.kind() == "user_type" {
                    "user_type"
                } else {
                    "type_identifier"
                });
            }
            _ => {}
        }
    }
    if has_enum_body {
        return SymbolKind::Enum;
    }
    // Extensions have `user_type` as their first name-like child, while
    // classes and structs use `type_identifier` directly.
    if first_name_kind == Some("user_type") {
        return SymbolKind::Impl;
    }
    SymbolKind::Class
}

fn find_name(node: &Node, source: &str) -> Option<String> {
    // `user_type` is needed for extensions where the name is
    // `(user_type (type_identifier))` rather than a direct `type_identifier`.
    find_first_named_child(
        node,
        source,
        &["simple_identifier", "type_identifier", "user_type"],
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use crate::domain::{FileOutcome, LanguageId, SymbolKind};
    use crate::parsing::process_file;

    #[test]
    fn test_swift_process_file_extracts_class_and_function() {
        let source = b"class Foo { func bar() -> Int { return 0 } }";
        let result = process_file("test.swift", source, LanguageId::Swift);
        assert!(
            matches!(
                result.outcome,
                FileOutcome::Processed | FileOutcome::PartialParse { .. }
            ),
            "Swift should parse successfully: {:?}",
            result.outcome
        );
        assert!(
            result
                .symbols
                .iter()
                .any(|s| s.kind == SymbolKind::Class && s.name == "Foo"),
            "should extract Foo class, symbols: {:?}",
            result.symbols
        );
    }

    #[test]
    fn test_swift_extension_extracted_as_impl() {
        let source = b"protocol Drawable {}\nclass MyClass {}\nextension MyClass: Drawable {}";
        let result = process_file("test.swift", source, LanguageId::Swift);
        assert!(
            matches!(
                result.outcome,
                FileOutcome::Processed | FileOutcome::PartialParse { .. }
            ),
            "Swift should parse successfully: {:?}",
            result.outcome
        );
        // class MyClass → Class, extension MyClass: Drawable → Impl
        assert!(
            result
                .symbols
                .iter()
                .any(|s| s.kind == SymbolKind::Class && s.name == "MyClass"),
            "should extract class as Class, symbols: {:?}",
            result.symbols
        );
        assert!(
            result
                .symbols
                .iter()
                .any(|s| s.kind == SymbolKind::Impl && s.name == "MyClass"),
            "should extract extension as Impl, symbols: {:?}",
            result.symbols
        );
    }

    #[test]
    fn test_swift_protocol_extracted_as_interface() {
        let source = b"protocol Drawable {}";
        let result = process_file("test.swift", source, LanguageId::Swift);
        assert!(
            matches!(
                result.outcome,
                FileOutcome::Processed | FileOutcome::PartialParse { .. }
            ),
            "Swift should parse successfully: {:?}",
            result.outcome
        );
        assert!(
            result
                .symbols
                .iter()
                .any(|s| s.kind == SymbolKind::Interface && s.name == "Drawable"),
            "should extract protocol as Interface, symbols: {:?}",
            result.symbols
        );
    }

    /// Diagnostic test: prints the s-expression tree for key Swift constructs so
    /// we can see exactly what node kinds tree-sitter-swift v0.7.1 emits.
    /// Run with: cargo test swift_sexp_diagnostic -- --nocapture
    #[test]
    fn swift_sexp_diagnostic() {
        let mut parser = tree_sitter::Parser::new();
        let lang = tree_sitter_swift::LANGUAGE;
        parser.set_language(&lang.into()).expect("set language");

        // Test the exact combined source from the extension test
        let combined = "protocol Drawable {}\nclass MyClass {}\nextension MyClass: Drawable {}";
        let tree = parser.parse(combined.as_bytes(), None).unwrap();
        println!("--- COMBINED SOURCE");
        println!("SEXP: {}", tree.root_node().to_sexp());
        println!();

        let cases = [
            "extension String: CustomStringConvertible {}",
            "protocol Drawable { func draw() }",
            "enum Direction { case north, south }",
            "struct Point { var x: Int; var y: Int }",
            "class Vehicle { var speed: Int = 0 }",
        ];
        for src in &cases {
            let tree = parser.parse(src.as_bytes(), None).unwrap();
            println!("--- SOURCE: {src}");
            println!("SEXP: {}", tree.root_node().to_sexp());
            println!();
        }
    }

    #[test]
    fn test_swift_enum_extracted_as_enum() {
        let source = b"enum Direction { case north }";
        let result = process_file("test.swift", source, LanguageId::Swift);
        assert!(
            matches!(
                result.outcome,
                FileOutcome::Processed | FileOutcome::PartialParse { .. }
            ),
            "Swift should parse successfully: {:?}",
            result.outcome
        );
        assert!(
            result
                .symbols
                .iter()
                .any(|s| s.kind == SymbolKind::Enum && s.name == "Direction"),
            "should extract enum as Enum, symbols: {:?}",
            result.symbols
        );
    }
}
