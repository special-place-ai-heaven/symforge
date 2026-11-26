use super::{
    Binding, Bindings, CallableFacts, ImportedValue, bind, has_child_kind, is_function,
    lexical_scopes,
};
use crate::domain::{ReferenceKind, ReferenceRecord};
use std::collections::{HashMap, HashSet};
use tree_sitter::Node;

pub(super) type Imports<'tree, 'src> =
    HashMap<(Node<'tree>, &'src str), (&'src str, &'src str, usize)>;

pub(super) fn collect_imports<'tree, 'src>(
    node: Node<'tree>,
    scope: Node<'tree>,
    python: bool,
    src: &'src [u8],
    bindings: &mut Bindings<'tree, 'src>,
    imports: &mut Imports<'tree, 'src>,
) {
    let mut add = |local: Node<'tree>, module: Option<&'src str>, exported: Option<&'src str>| {
        let target = module.zip(exported);
        bind(bindings, scope, local, target.is_some(), src);
        if let Some((module, exported)) = target
            && let Ok(local) = local.utf8_text(src)
        {
            imports.insert((scope, local), (module, exported, node.end_byte()));
        }
    };
    if python {
        // Only unconditional imports at the module level or directly in a function body.
        let module = node
            .child_by_field_name("module_name")
            .filter(|_| {
                node.parent() == Some(scope)
                    || node.parent().is_some_and(|p| {
                        p.kind() == "block" && scope.child_by_field_name("body") == Some(p)
                    })
            })
            .and_then(|n| n.utf8_text(src).ok());
        for item in node.children_by_field_name("name", &mut node.walk()) {
            let name = item.child_by_field_name("name").unwrap_or(item);
            let original = name.named_child(0).unwrap_or(name);
            let local = item.child_by_field_name("alias").unwrap_or(original);
            let exported = name.utf8_text(src).ok().filter(|name| !name.contains('.'));
            add(local, module, exported);
        }
    } else {
        let module = node
            .child_by_field_name("source")
            .and_then(|n| n.utf8_text(src).ok())
            .and_then(|s| s.get(1..s.len().checked_sub(1)?))
            .filter(|s| !s.contains('\\') && (s.starts_with("./") || s.starts_with("../")))
            .filter(|_| !has_child_kind(node, "type"));
        let Some(clause) = node
            .named_children(&mut node.walk())
            .find(|n| matches!(n.kind(), "import_clause" | "import_require_clause"))
        else {
            return;
        };
        if clause.kind() == "import_require_clause" {
            if let Some(local) = clause.named_child(0).filter(|n| n.kind() == "identifier") {
                add(local, None, None);
            }
            return;
        }
        for item in clause.named_children(&mut clause.walk()) {
            match item.kind() {
                "identifier" => add(item, module, Some("default")),
                "named_imports" => {
                    for spec in item
                        .named_children(&mut item.walk())
                        .filter(|n| n.kind() == "import_specifier")
                    {
                        if let Some(name) = spec.child_by_field_name("name") {
                            add(
                                spec.child_by_field_name("alias").unwrap_or(name),
                                module,
                                name.utf8_text(src).ok().filter(|_| {
                                    name.kind() == "identifier" && !has_child_kind(spec, "type")
                                }),
                            );
                        }
                    }
                }
                "namespace_import" => {
                    if let Some(local) = item.named_child(0) {
                        add(local, None, None);
                    }
                }
                _ => {}
            }
        }
    }
}

pub(super) fn collect_export<'src>(
    root: Node<'_>,
    node: Node<'_>,
    name: Node<'_>,
    python: bool,
    src: &'src [u8],
    exports: &mut Vec<(&'src str, &'src str, u32)>,
) {
    let Ok(text) = name.utf8_text(src) else {
        return;
    };
    let exported = if python {
        if node.parent() != Some(root) {
            return;
        }
        text
    } else {
        let Some(mut wrapper) = node.parent() else {
            return;
        };
        if node.kind() == "variable_declarator" {
            if !has_child_kind(wrapper, "const") {
                return;
            }
            let Some(parent) = wrapper.parent() else {
                return;
            };
            wrapper = parent;
        }
        if wrapper.kind() != "export_statement" || wrapper.parent() != Some(root) {
            return;
        }
        if has_child_kind(wrapper, "default") {
            "default"
        } else {
            text
        }
    };
    exports.push((exported, text, name.start_byte() as u32));
}

pub(super) fn finish_imports<'tree, 'src>(
    candidates: Vec<Node<'tree>>,
    bindings: &Bindings<'tree, 'src>,
    imports: &Imports<'tree, 'src>,
    opaque_scopes: &HashSet<Node<'tree>>,
    src: &'src [u8],
    python: bool,
    facts: &mut CallableFacts,
) {
    let mut seen = HashSet::new();
    for node in candidates {
        if !matches!(node.kind(), "identifier" | "shorthand_property_identifier") {
            continue;
        }
        let Ok(local) = node.utf8_text(src) else {
            continue;
        };
        if python && !python_value_context(node) {
            continue;
        }
        let Some(scope) = lexical_scopes(node, python)
            .find_map(|scope| {
                // A wildcard import may supply (or override) any local name.
                if opaque_scopes.contains(&scope) {
                    return Some(None);
                }
                bindings
                    .get(&scope)?
                    .get(local)
                    .map(|binding| (*binding == Binding::Callable).then_some(scope))
            })
            .flatten()
        else {
            continue;
        };
        let Some(&(module, exported, import_end)) = imports.get(&(scope, local)) else {
            continue;
        };
        if (python && node.start_byte() < import_end) || !seen.insert(node.start_byte()) {
            continue;
        }
        facts.uses.push(ImportedValue {
            module: module.to_owned(),
            exported: exported.to_owned(),
            reference: ReferenceRecord {
                name: local.to_owned(),
                qualified_name: None,
                kind: ReferenceKind::ValueUse,
                byte_range: (node.start_byte() as u32, node.end_byte() as u32),
                line_range: (
                    node.start_position().row as u32,
                    node.end_position().row as u32,
                ),
                enclosing_symbol_index: None,
            },
        });
    }
}

fn python_value_context(mut value: Node<'_>) -> bool {
    while let Some(parent) = value.parent() {
        // Comprehension targets have their own scope, and function/class
        // headers run outside the body. Do not infer imports in these contexts.
        if matches!(
            parent.kind(),
            "list_comprehension"
                | "set_comprehension"
                | "dictionary_comprehension"
                | "generator_expression"
        ) || ((is_function(parent.kind()) || parent.kind() == "class_definition")
            && parent.child_by_field_name("body") != Some(value))
        {
            return false;
        }
        value = parent;
    }
    true
}
