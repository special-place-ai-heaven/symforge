use tree_sitter::Node;

use super::{
    NO_DOC_SPEC, SymbolSink, collect_symbols, find_first_named_child, push_named_symbol,
    push_symbol, walk_children,
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
        "function_definition" => Some(SymbolKind::Function),
        "class_definition" => Some(SymbolKind::Class),
        "decorated_definition" => {
            // Use the decorated_definition's full range (includes decorators)
            // but extract name/kind from the inner function/class definition.
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                let inner_kind = match child.kind() {
                    "function_definition" => Some(SymbolKind::Function),
                    "class_definition" => Some(SymbolKind::Class),
                    _ => None,
                };
                if let Some(k) = inner_kind
                    && let Some(name) = find_name(&child, source)
                {
                    let mut sink = SymbolSink::new(source, sort_order, symbols, &NO_DOC_SPEC);
                    push_symbol(node, name, k, depth, &mut sink);
                    // Recurse into the inner definition's children (nested classes/methods)
                    walk_children(
                        &child,
                        source,
                        depth,
                        sort_order,
                        symbols,
                        Some(k),
                        walk_node,
                    );
                }
            }
            return;
        }
        _ => None,
    };

    {
        let mut sink = SymbolSink::new(source, sort_order, symbols, &NO_DOC_SPEC);
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

fn find_name(node: &Node, source: &str) -> Option<String> {
    find_first_named_child(node, source, &["identifier"])
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::LanguageId;
    use crate::parsing::inline_tests::inline_test;
    use tree_sitter::Parser;

    inline_test!(
        python_inline_test_extracts_function,
        LanguageId::Python,
        r#"
def inline_python_probe():
    pass
"#,
        [(SymbolKind::Function, "inline_python_probe")]
    );

    fn parse_python(source: &str) -> Vec<SymbolRecord> {
        let mut parser = Parser::new();
        let lang: tree_sitter::Language = tree_sitter_python::LANGUAGE.into();
        parser.set_language(&lang).expect("set Python language");
        let tree = parser.parse(source, None).expect("parse Python source");
        extract_symbols(&tree.root_node(), source)
    }

    #[test]
    fn test_python_decorated_function() {
        let source = r#"
@app.route("/api")
def handler():
    pass
"#;
        let symbols = parse_python(source);
        let handler = symbols.iter().find(|s| s.name == "handler");
        assert!(
            handler.is_some(),
            "should extract decorated function, got: {:?}",
            symbols
        );
        assert_eq!(handler.unwrap().kind, SymbolKind::Function);
        // Byte range should include the decorator
        let src_decorator_start = source.find('@').unwrap();
        assert_eq!(
            handler.unwrap().byte_range.0 as usize,
            src_decorator_start,
            "byte range should start at decorator"
        );
    }

    #[test]
    fn test_python_decorated_class() {
        let source = r#"
@dataclass
class Config:
    name: str
"#;
        let symbols = parse_python(source);
        let config = symbols.iter().find(|s| s.name == "Config");
        assert!(
            config.is_some(),
            "should extract decorated class, got: {:?}",
            symbols
        );
        assert_eq!(config.unwrap().kind, SymbolKind::Class);
    }

    #[test]
    fn test_python_plain_function_still_works() {
        let source = "def foo():\n    pass\n";
        let symbols = parse_python(source);
        let foo = symbols.iter().find(|s| s.name == "foo");
        assert!(foo.is_some(), "should extract plain function");
        assert_eq!(foo.unwrap().kind, SymbolKind::Function);
    }
}
