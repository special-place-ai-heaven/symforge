use tree_sitter::Node;

use super::{
    DocCommentSpec, SymbolSink, collect_symbols, find_first_named_child, push_named_symbol,
    push_symbol, walk_children,
};

pub(super) const DOC_SPEC: DocCommentSpec = DocCommentSpec {
    comment_node_types: &["comment"],
    doc_prefixes: Some(&["/**"]),
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
        "function_declaration" | "generator_function_declaration" => Some(SymbolKind::Function),
        "class_declaration" | "abstract_class_declaration" => Some(SymbolKind::Class),
        "method_definition" => Some(SymbolKind::Method),
        "interface_declaration" => Some(SymbolKind::Interface),
        "type_alias_declaration" => Some(SymbolKind::Type),
        "enum_declaration" => Some(SymbolKind::Enum),
        "module" | "internal_module" => Some(SymbolKind::Module),
        "public_field_definition" | "field_definition" => Some(SymbolKind::Variable),
        "export_statement" => {
            // `export default <anonymous-expr>` — emit a "default" symbol since
            // recursion alone won't extract anonymous arrow/function/class expressions.
            let mut cursor = node.walk();
            let has_default = node.children(&mut cursor).any(|c| c.kind() == "default");
            if has_default {
                let mut cursor2 = node.walk();
                for child in node.children(&mut cursor2) {
                    match child.kind() {
                        "arrow_function" | "function_expression" | "generator_function" => {
                            let mut sink = SymbolSink::new(source, sort_order, symbols, &DOC_SPEC);
                            push_symbol(
                                node,
                                "default".to_string(),
                                SymbolKind::Function,
                                depth,
                                &mut sink,
                            );
                            break;
                        }
                        "class" => {
                            let mut sink = SymbolSink::new(source, sort_order, symbols, &DOC_SPEC);
                            push_symbol(
                                node,
                                "default".to_string(),
                                SymbolKind::Class,
                                depth,
                                &mut sink,
                            );
                            break;
                        }
                        _ => {}
                    }
                }
            }
            walk_children(node, source, depth, sort_order, symbols, None, walk_node);
            return;
        }
        "lexical_declaration" | "variable_declaration" => {
            extract_variable_declarations(node, source, depth, sort_order, symbols);
            return;
        }
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

fn extract_variable_declarations(
    node: &Node,
    source: &str,
    depth: u32,
    sort_order: &mut u32,
    symbols: &mut Vec<SymbolRecord>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "variable_declarator"
            && let Some(name) = find_name(&child, source)
        {
            let kind = if has_function_initializer(&child) {
                SymbolKind::Function
            } else if is_const_declaration(node) {
                SymbolKind::Constant
            } else {
                SymbolKind::Variable
            };
            let mut sink = SymbolSink::new(source, sort_order, symbols, &DOC_SPEC);
            push_symbol(&child, name, kind, depth, &mut sink);
        }
    }
}

fn has_function_initializer(declarator: &Node) -> bool {
    let mut cursor = declarator.walk();
    for child in declarator.children(&mut cursor) {
        match child.kind() {
            "arrow_function" | "function_expression" | "generator_function" => return true,
            _ => {}
        }
    }
    false
}

fn is_const_declaration(node: &Node) -> bool {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "const" {
            return true;
        }
    }
    false
}

fn find_name(node: &Node, source: &str) -> Option<String> {
    find_first_named_child(
        node,
        source,
        &["identifier", "type_identifier", "property_identifier"],
    )
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
        typescript_inline_test_extracts_interface,
        LanguageId::TypeScript,
        r#"
interface InlineTypeScriptProbe {
  id: string;
}
"#,
        [(SymbolKind::Interface, "InlineTypeScriptProbe")]
    );

    fn parse_ts(source: &str) -> Vec<SymbolRecord> {
        let mut parser = Parser::new();
        let lang: tree_sitter::Language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
        parser.set_language(&lang).expect("set TS language");
        let tree = parser.parse(source, None).expect("parse TS source");
        extract_symbols(&tree.root_node(), source)
    }

    #[test]
    fn test_ts_arrow_function_is_function_kind() {
        let source = "const handler = (req: Request, res: Response) => { return res.json({}); };";
        let symbols = parse_ts(source);
        let handler = symbols.iter().find(|s| s.name == "handler");
        assert!(
            handler.is_some(),
            "should extract arrow function, got: {:?}",
            symbols
        );
        assert_eq!(
            handler.unwrap().kind,
            SymbolKind::Function,
            "arrow function should be Function, not Constant"
        );
    }

    #[test]
    fn test_ts_const_non_function_is_constant() {
        let source = "const MAX_SIZE: number = 100;";
        let symbols = parse_ts(source);
        let max = symbols.iter().find(|s| s.name == "MAX_SIZE");
        assert!(max.is_some());
        assert_eq!(max.unwrap().kind, SymbolKind::Constant);
    }
}
