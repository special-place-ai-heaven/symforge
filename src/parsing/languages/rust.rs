use tree_sitter::Node;

use super::{
    DocCommentSpec, SymbolSink, collect_symbols, find_first_named_child, push_named_symbol,
    walk_children,
};

pub(super) const DOC_SPEC: DocCommentSpec = DocCommentSpec {
    comment_node_types: &["line_comment", "block_comment"],
    doc_prefixes: Some(&["///", "//!", "/**", "/*!"]),
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
        "function_item" => Some(if is_impl_function_item(node) {
            SymbolKind::Method
        } else {
            SymbolKind::Function
        }),
        "struct_item" => Some(SymbolKind::Struct),
        "enum_item" => Some(SymbolKind::Enum),
        "trait_item" => Some(SymbolKind::Trait),
        "impl_item" => Some(SymbolKind::Impl),
        "const_item" => Some(SymbolKind::Constant),
        "static_item" => Some(SymbolKind::Variable),
        "mod_item" => Some(SymbolKind::Module),
        "type_item" => Some(SymbolKind::Type),
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

fn is_impl_function_item(node: &Node) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };

    if parent.kind() == "impl_item" {
        return true;
    }

    parent.kind() == "declaration_list"
        && parent
            .parent()
            .is_some_and(|grandparent| grandparent.kind() == "impl_item")
}

fn find_name(node: &Node, source: &str) -> Option<String> {
    // For impl items, construct "impl Type" or "impl Trait for Type"
    if node.kind() == "impl_item" {
        return extract_impl_name(node, source);
    }

    find_first_named_child(node, source, &["name", "identifier", "type_identifier"])
}

fn extract_impl_name(node: &Node, source: &str) -> Option<String> {
    let mut cursor = node.walk();
    let children: Vec<_> = node.children(&mut cursor).collect();

    let mut trait_name = None;
    let mut type_name = None;
    let mut found_for = false;

    for child in &children {
        match child.kind() {
            "type_identifier" | "scoped_type_identifier" | "generic_type" => {
                let text = child.utf8_text(source.as_bytes()).unwrap_or("").to_string();
                if found_for {
                    type_name = Some(text);
                } else if trait_name.is_none() {
                    trait_name = Some(text);
                } else {
                    type_name = Some(text);
                }
            }
            "for" => {
                found_for = true;
            }
            _ => {}
        }
    }

    if found_for && let (Some(tr), Some(ty)) = (&trait_name, &type_name) {
        return Some(format!("impl {tr} for {ty}"));
    }

    trait_name.map(|n| format!("impl {n}"))
}

#[cfg(test)]
mod tests {
    use crate::domain::{LanguageId, SymbolKind};
    use crate::parsing::inline_tests::inline_test;

    inline_test!(
        rust_inline_test_extracts_function,
        LanguageId::Rust,
        r#"
pub fn inline_rust_probe() {}
"#,
        [(SymbolKind::Function, "inline_rust_probe")]
    );

    inline_test!(
        rust_inline_test_extracts_impl_method,
        LanguageId::Rust,
        r#"
pub struct Greeter;

impl Greeter {
    pub fn greet(&self) {}
}
"#,
        [
            (SymbolKind::Struct, "Greeter"),
            (SymbolKind::Impl, "impl Greeter"),
            (SymbolKind::Method, "greet"),
        ]
    );
}
