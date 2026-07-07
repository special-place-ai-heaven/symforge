use tree_sitter::Node;

use super::{
    DocCommentSpec, SymbolSink, collect_symbols, find_first_named_child, push_named_symbol,
    push_symbol, walk_children,
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
        "macro_definition" => Some(SymbolKind::Other),
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
    // Dogfood #3 (2026-07-06): a module-level macro invocation like
    // `define_id_type!(ProjectId)` declares names whose definitions are
    // synthesized at compile time — invisible to name search unless the
    // argument identifiers are indexed as trust-flagged `macro-generated`.
    if node.kind() == "macro_invocation" && is_module_level(node) {
        let mut sink = SymbolSink::new(source, sort_order, symbols, &DOC_SPEC);
        push_macro_generated_names(node, source, depth, &mut sink);
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

/// True for items directly at module scope (crate root or a `mod` body).
/// A top-level `foo!(...);` parses as `(expression_statement (macro_invocation))`,
/// so one `expression_statement` hop is allowed — but only when the statement
/// itself sits at module scope. Function-body macro calls (`format!`,
/// `println!`) live under a `block` and must never produce symbols.
fn is_module_level(node: &Node) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    match parent.kind() {
        "source_file" | "declaration_list" => true,
        "expression_statement" => parent
            .parent()
            .is_some_and(|grandparent| matches!(grandparent.kind(), "source_file" | "declaration_list")),
        _ => false,
    }
}

/// Index identifier argument tokens of a module-level macro invocation as
/// `macro-generated` symbols (dogfood #3). Cheap heuristic by design: the
/// kind label is the trust flag — the index has the declared NAME, not the
/// synthesized body. A token_tree is a FLAT token list, so a path argument
/// like `std::cell::Cell` would otherwise yield `std`, `cell`, `Cell` as
/// separate "symbols". Precision filter: accept an identifier only when its
/// immediate preceding token starts the tree or a top-level arg
/// (`(` `[` `{` `,`) AND its immediate following token ends the arg
/// (`,` `)` `]` `}`). This keeps `ProjectId` in `define_id_type!(ProjectId)`
/// and `Alpha`/`Beta` in `declare_pair!(Alpha, Beta, ...)` while rejecting
/// `::`-adjacent path segments and identifiers nested in larger expressions.
/// Under-capture is the safe direction. Capped and deduplicated.
fn push_macro_generated_names(node: &Node, source: &str, depth: u32, sink: &mut SymbolSink<'_, '_>) {
    const MAX_NAMES_PER_INVOCATION: usize = 8;
    let mut cursor = node.walk();
    let Some(token_tree) = node
        .children(&mut cursor)
        .find(|child| child.kind() == "token_tree")
    else {
        return;
    };
    // Flat token list — index neighbors directly to check arg boundaries.
    let mut tree_cursor = token_tree.walk();
    let tokens: Vec<Node> = token_tree.children(&mut tree_cursor).collect();
    let mut seen: Vec<String> = Vec::new();
    for (i, token) in tokens.iter().enumerate() {
        if seen.len() >= MAX_NAMES_PER_INVOCATION {
            break;
        }
        if !matches!(token.kind(), "identifier" | "type_identifier") {
            continue;
        }
        // Preceding token must start the tree or a top-level arg.
        let prev_ok = i == 0 || tokens.get(i - 1).is_some_and(|prev| matches!(prev.kind(), "(" | "[" | "{" | ","));
        // Following token must end the arg (or terminate the tree).
        let next_ok = tokens.get(i + 1).is_none_or(|next| matches!(next.kind(), "," | ")" | "]" | "}"));
        if !prev_ok || !next_ok {
            continue;
        }
        let Ok(name) = token.utf8_text(source.as_bytes()) else {
            continue;
        };
        if name.is_empty() || seen.iter().any(|existing| existing == name) {
            continue;
        }
        seen.push(name.to_string());
        // Anchor to the whole invocation so get_symbol returns the declaring
        // line (`define_id_type!(ProjectId);`), not a bare token.
        push_symbol(node, name.to_string(), SymbolKind::MacroGenerated, depth, sink);
    }
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

    inline_test!(
        rust_inline_test_extracts_macro_rules,
        LanguageId::Rust,
        r#"macro_rules! cfg_if {
    () => {};
}"#,
        [(SymbolKind::Other, "cfg_if")]
    );

    // Dogfood #3 (2026-07-06): names declared by module-level macro
    // invocations are indexed as trust-flagged `macro-generated` symbols.
    inline_test!(
        rust_inline_test_extracts_macro_generated_names,
        LanguageId::Rust,
        r#"
define_id_type!(ProjectId);
"#,
        [(SymbolKind::MacroGenerated, "ProjectId")]
    );

    inline_test!(
        rust_inline_test_macro_generated_names_dedup_multiple_args,
        LanguageId::Rust,
        r#"
declare_pair!(Alpha, Beta, Alpha);
"#,
        [
            (SymbolKind::MacroGenerated, "Alpha"),
            (SymbolKind::MacroGenerated, "Beta"),
        ]
    );

    // Function-body macro calls must never produce symbols — only
    // module-level invocations declare.
    inline_test!(
        rust_inline_test_function_body_macro_produces_no_symbols,
        LanguageId::Rust,
        r#"
fn caller() {
    println!("{}", some_value);
}
"#,
        [(SymbolKind::Function, "caller")]
    );

    // Qualified path argument: a flat token_tree splits `std::cell::Cell`
    // into sibling tokens `std`, `::`, `cell`, `::`, `Cell`. The precision
    // filter rejects every `::`-adjacent segment — no debris captured.
    inline_test!(
        rust_inline_test_macro_generated_rejects_qualified_path,
        LanguageId::Rust,
        r#"
foo!(std::cell::Cell);
"#,
        []
    );

    // Multi-arg invocation: each top-level identifier is bounded by
    // `(`/`,` before and `,`/`)` after, so both are captured.
    inline_test!(
        rust_inline_test_macro_generated_multi_arg,
        LanguageId::Rust,
        r#"
m!(Alpha, Beta);
"#,
        [
            (SymbolKind::MacroGenerated, "Alpha"),
            (SymbolKind::MacroGenerated, "Beta"),
        ]
    );

    // Module-level invocations inside a `mod` body still declare.
    inline_test!(
        rust_inline_test_macro_generated_inside_mod,
        LanguageId::Rust,
        r#"
mod ids {
    define_id_type!(SessionId);
}
"#,
        [
            (SymbolKind::Module, "ids"),
            (SymbolKind::MacroGenerated, "SessionId"),
        ]
    );
}
