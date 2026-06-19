use tree_sitter::Node;

use super::{DocCommentSpec, SymbolSink, collect_symbols, push_named_symbol, walk_children};

pub(super) const DOC_SPEC: DocCommentSpec = DocCommentSpec {
    comment_node_types: &["comment"],
    doc_prefixes: None,
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
        // ts-parser-perl 1.1.x: subs and class methods are *_declaration_statement.
        // Keep the legacy ganezdragon kinds harmless for forward-compat.
        "subroutine_declaration_statement"
        | "method_declaration_statement"
        | "function_definition"
        | "function_definition_without_sub" => Some(SymbolKind::Function),
        // `package Foo;` and `class Foo {...}` both define a module-like scope.
        "package_statement" | "class_statement" => Some(SymbolKind::Module),
        _ => None,
    };

    {
        let mut sink = SymbolSink::new(source, sort_order, symbols, &DOC_SPEC);
        push_named_symbol(node, depth, kind, find_name, &mut sink);
    }
    walk_children(node, source, depth, sort_order, symbols, kind, walk_node);
}

fn find_name(node: &Node, source: &str, kind: SymbolKind) -> Option<String> {
    // ts-parser-perl 1.1.x exposes the defined name via the `name:` field on
    // subroutine/method/package/class statements:
    //   (subroutine_declaration_statement name: (bareword) ...)   -> "greet"
    //   (package_statement name: (package))                       -> "MyApp::Module"
    //   (class_statement name: (package) (block (method_declaration_statement
    //       name: (bareword) ...)))                               -> "Point" / "render"
    // Prefer the field; the named child is the canonical name node.
    if let Some(name_node) = node.child_by_field_name("name")
        && let Ok(text) = name_node.utf8_text(source.as_bytes())
    {
        let text = text.trim();
        if !text.is_empty() {
            return Some(text.to_string());
        }
    }

    // Fallback: child-scan over the name-bearing node kinds. Covers both the
    // ts-parser-perl kinds (`bareword`, `package`) and the legacy ganezdragon
    // kinds (`name`, `identifier`, `package_name`, `subroutine_name`), so a
    // grammar without the `name:` field still resolves.
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if matches!(
            child.kind(),
            "bareword" | "package" | "name" | "identifier" | "package_name"
        ) {
            return Some(child.utf8_text(source.as_bytes()).unwrap_or("").to_string());
        }
        if kind == SymbolKind::Function && child.kind() == "subroutine_name" {
            return Some(child.utf8_text(source.as_bytes()).unwrap_or("").to_string());
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use crate::domain::{FileOutcome, LanguageId, SymbolKind};
    use crate::parsing::process_file;

    #[test]
    fn test_perl_process_file_extracts_subroutine() {
        let source = b"sub greet { print \"hello\\n\"; }";
        let result = process_file("test.pl", source, LanguageId::Perl);
        assert!(
            matches!(
                result.outcome,
                FileOutcome::Processed | FileOutcome::PartialParse { .. }
            ),
            "Perl should parse successfully: {:?}",
            result.outcome
        );
        assert!(
            result
                .symbols
                .iter()
                .any(|s| s.kind == SymbolKind::Function && s.name == "greet"),
            "should extract greet subroutine, symbols: {:?}",
            result.symbols
        );
    }

    #[test]
    fn test_perl_package_extracted_as_module() {
        let source = b"package MyApp::Module;\n\nsub new { return bless {}, shift; }";
        let result = process_file("test.pl", source, LanguageId::Perl);
        assert!(
            matches!(
                result.outcome,
                FileOutcome::Processed | FileOutcome::PartialParse { .. }
            ),
            "Perl should parse successfully: {:?}",
            result.outcome
        );
        assert!(
            result
                .symbols
                .iter()
                .any(|s| s.kind == SymbolKind::Module && s.name == "MyApp::Module"),
            "should extract package as Module, symbols: {:?}",
            result.symbols
        );
    }

    /// ts-parser-perl recovers the `class Foo { method bar {...} }` construct
    /// that the old ganezdragon grammar ERROR-noded: `class Point` must surface
    /// as a Module and `method render` as a Function.
    #[test]
    fn test_perl_class_and_method_extracted() {
        let source = b"class Point {\n    method render { return 1; }\n}";
        let result = process_file("test.pl", source, LanguageId::Perl);
        assert!(
            matches!(
                result.outcome,
                FileOutcome::Processed | FileOutcome::PartialParse { .. }
            ),
            "Perl class should parse: {:?}",
            result.outcome
        );
        assert!(
            result
                .symbols
                .iter()
                .any(|s| s.kind == SymbolKind::Module && s.name == "Point"),
            "should extract class Point as Module, symbols: {:?}",
            result.symbols
        );
        assert!(
            result
                .symbols
                .iter()
                .any(|s| s.kind == SymbolKind::Function && s.name == "render"),
            "should extract method render as Function, symbols: {:?}",
            result.symbols
        );
    }
}
