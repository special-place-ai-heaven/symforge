use std::collections::{HashMap, HashSet};

use tree_sitter::Node;

use crate::domain::{LanguageId, ReferenceKind, ReferenceRecord};

mod imports;
mod member_key;
#[cfg(test)]
mod rebinding_tests;
use imports::{collect_export, collect_imports, finish_imports, python_value_context};
/// Collect JSX invocations and statically recognizable callable values. Using
/// the parsed node kinds keeps JSX out of the plain TypeScript grammar.
pub(super) fn extract(
    root: Node<'_>,
    source: &str,
    language: LanguageId,
    is_tsx: bool,
    refs: &mut Vec<ReferenceRecord>,
) {
    walk(root, source, language, is_tsx, refs, None);
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct ImportedValue {
    pub(crate) module: String,
    pub(crate) exported: String,
    pub(crate) reference: ReferenceRecord,
}

type ByteRange = (u32, u32);

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct CallableFacts {
    pub(crate) uses: Vec<ImportedValue>,
    // Use token range -> lexical declaration's core range in the same file.
    #[serde(serialize_with = "sorted_map")]
    pub(crate) locals: HashMap<ByteRange, ByteRange>,
    // Exported spelling -> (canonical declaration name, declaration core range).
    #[serde(serialize_with = "sorted_map")]
    pub(crate) exports: HashMap<String, (String, ByteRange)>,
    // Separate from callbacks: class values are joined only for exact class selectors.
    #[serde(serialize_with = "sorted_map")]
    pub(crate) class_exports: HashMap<String, (String, ByteRange)>,
}

fn sorted_map<K: Ord + serde::Serialize, V: serde::Serialize, S: serde::Serializer>(
    map: &HashMap<K, V>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let mut entries: Vec<_> = map.iter().collect();
    entries.sort_unstable_by(|left, right| left.0.cmp(right.0));
    serializer.collect_map(entries)
}

pub(crate) fn facts(root: Node<'_>, source: &str, language: LanguageId) -> CallableFacts {
    let mut facts = CallableFacts::default();
    walk(
        root,
        source,
        language,
        false,
        &mut Vec::new(),
        Some(&mut facts),
    );
    facts
}

fn walk(
    root: Node<'_>,
    source: &str,
    language: LanguageId,
    is_tsx: bool,
    refs: &mut Vec<ReferenceRecord>,
    facts: Option<&mut CallableFacts>,
) {
    let python = language == LanguageId::Python;
    let jsx = language == LanguageId::JavaScript || is_tsx;
    let src = source.as_bytes();
    let mut bindings = Bindings::new();
    let mut methods = Bindings::new();
    let mut candidates = Vec::new();
    let mut imports = imports::Imports::default();
    let mut opaque_import_scopes = HashSet::new();
    let mut python_directives = Vec::new();
    let mut js_writes = Vec::new();
    let mut exports = Vec::new();

    // Use a cursor instead of recursion so deeply nested input doesn't consume
    // the Rust stack. Collect bindings before resolution to allow forward refs.
    let mut cursor = root.walk();
    loop {
        let node = cursor.node();
        if node.is_named() && !node.is_missing() {
            if python
                && facts.is_some()
                && matches!(
                    node.kind(),
                    "assignment" | "augmented_assignment" | "delete_statement"
                )
            {
                let target = node
                    .child_by_field_name("left")
                    .or_else(|| node.named_child(0));
                if let Some(target) = target {
                    visit_pattern(target, true, |member| {
                        if let Some(name) = member
                            .child_by_field_name("attribute")
                            .and_then(|n| n.utf8_text(src).ok())
                        {
                            imports.written_members.insert(name);
                        }
                    });
                }
            }
            match node.kind() {
                "jsx_opening_element" | "jsx_self_closing_element" if jsx && facts.is_none() => {
                    if let Some((name, qualifier)) = jsx_component(node, src) {
                        push_ref(refs, name, src, qualifier, ReferenceKind::Call);
                    }
                }
                "function_definition"
                | "function_declaration"
                | "generator_function_declaration"
                | "method_definition" => {
                    let method_class = enclosing_method_class(node, python);
                    if let Some(name) = node.child_by_field_name("name") {
                        if let Some(class) = method_class {
                            // JS static methods use a different receiver. This
                            // pass only infers the instance receiver.
                            if python || !has_child_kind(node, "static") {
                                let callable = if python {
                                    !python_decorated(node, src, &["property", "setter", "deleter"])
                                } else {
                                    !has_child_kind(node, "get")
                                        && !has_child_kind(node, "set")
                                        && name.utf8_text(src).is_ok_and(|text| {
                                            text != "constructor" && !text.contains('\\')
                                        })
                                };
                                let definition = if python {
                                    node.parent()
                                        .filter(|n| n.kind() == "decorated_definition")
                                        .unwrap_or(node)
                                } else {
                                    node
                                };
                                bind(
                                    &mut methods,
                                    class,
                                    name,
                                    callable.then_some(definition),
                                    src,
                                );
                                let plain = name.kind() == "property_identifier"
                                    && name.utf8_text(src).is_ok_and(|text| !text.contains('\\'));
                                if !python && !plain && name.kind() != "private_property_identifier"
                                {
                                    block_member_name(&mut methods, class, name, src);
                                }
                            }
                            if python {
                                bind(&mut bindings, class, name, None, src);
                            }
                        } else if node.kind() != "method_definition"
                            && let Some(scope) = enclosing_scope(node, python)
                        {
                            let callable = !python
                                || node
                                    .parent()
                                    .is_none_or(|p| p.kind() != "decorated_definition");
                            bind(&mut bindings, scope, name, callable.then_some(node), src);
                            if callable && facts.is_some() {
                                collect_export(root, node, name, python, src, &mut exports);
                            }
                        }
                    }
                    bind_parameters(&mut bindings, node, src);
                }
                "field_definition" | "public_field_definition" if !python => {
                    let name = node
                        .child_by_field_name("property")
                        .or_else(|| node.child_by_field_name("name"));
                    if let (Some(name), Some(class)) = (name, enclosing_method_class(node, false))
                        && !has_child_kind(node, "static")
                    {
                        block_member_name(&mut methods, class, name, src);
                    }
                }
                "arrow_function" | "function_expression" | "generator_function" | "lambda" => {
                    bind_parameters(&mut bindings, node, src);
                    if let Some(name) = node.child_by_field_name("name") {
                        bind(&mut bindings, node, name, Some(node), src);
                    }
                }
                "assignment" | "assignment_expression" | "variable_declarator" => {
                    let var_decl = node.kind() == "variable_declarator";
                    let lhs = node.child_by_field_name(if var_decl { "name" } else { "left" });
                    let rhs = node
                        .child_by_field_name(if var_decl { "value" } else { "right" })
                        .map(unparen);
                    if !var_decl && rhs.is_some() {
                        block_method_writes(&mut methods, lhs, src, python);
                    }
                    let hoisted = !python
                        && var_decl
                        && node
                            .parent()
                            .is_some_and(|p| p.kind() == "variable_declaration");
                    let lexical_write = !python && !var_decl && facts.is_some();
                    if lexical_write {
                        js_writes.extend(lhs);
                    } else if let Some(lhs) = lhs
                        && let Some(scope) = binding_scope(node, python, hoisted)
                    {
                        let callable = rhs.is_some_and(|n| {
                            matches!(
                                n.kind(),
                                "arrow_function"
                                    | "function_expression"
                                    | "generator_function"
                                    | "lambda"
                            )
                        });
                        if lhs.kind() == "identifier" {
                            let definition = sole_declaration(node);
                            bind(
                                &mut bindings,
                                scope,
                                lhs,
                                callable.then_some(definition),
                                src,
                            );
                            if python && scope.kind() == "class_definition" {
                                // A class namespace assignment can replace a method.
                                bind(&mut methods, scope, lhs, None, src);
                            }
                            if callable && facts.is_some() && var_decl {
                                collect_export(root, node, lhs, python, src, &mut exports);
                            }
                        } else {
                            block_pattern(&mut bindings, scope, lhs, src);
                        }
                    }
                    candidates.extend(rhs.filter(|n| is_candidate(*n)));
                }
                "argument_list" | "arguments"
                    if node.parent().is_some_and(|p| {
                        matches!(p.kind(), "call" | "call_expression" | "new_expression")
                    }) =>
                {
                    candidates.extend(
                        node.named_children(&mut node.walk())
                            .filter_map(|arg| {
                                if arg.kind() == "keyword_argument" {
                                    arg.child_by_field_name("value")
                                } else {
                                    Some(arg)
                                }
                            })
                            .map(unparen)
                            .filter(|n| is_candidate(*n)),
                    );
                }
                "list" | "tuple" | "set" | "expression_list" | "array" => {
                    candidates.extend(
                        node.named_children(&mut node.walk())
                            .map(unparen)
                            .filter(|n| is_candidate(*n) && (!python || python_value_context(*n))),
                    );
                }
                "pair" => {
                    candidates.extend(
                        node.child_by_field_name("value")
                            .map(unparen)
                            .filter(|n| is_candidate(*n)),
                    );
                }
                "shorthand_property_identifier" if !python => candidates.push(node),
                "jsx_expression"
                    if !python && node.parent().is_some_and(|p| p.kind() == "jsx_attribute") =>
                {
                    candidates.extend(
                        node.named_children(&mut node.walk())
                            .find(|n| n.kind() != "comment")
                            .map(unparen)
                            .filter(|n| is_candidate(*n)),
                    );
                }
                "import_from_statement" | "import_statement" => {
                    if let Some(scope) = enclosing_scope(node, python) {
                        if python && has_child_kind(node, "wildcard_import") {
                            opaque_import_scopes.insert(scope);
                        }
                        collect_imports(node, scope, python, src, &mut bindings, &mut imports);
                    }
                }
                "import_alias" if !python => {
                    if let Some(local) = node.named_child(0).filter(|n| n.kind() == "identifier")
                        && let Some(scope) = enclosing_scope(node, python)
                    {
                        bind(&mut bindings, scope, local, None, src);
                    }
                }
                "class" if !python => {
                    // A named class expression binds its name only inside itself.
                    if let Some(name) = node.child_by_field_name("name") {
                        bind(&mut bindings, node, name, None, src);
                    }
                }
                "type_alias_statement" if python => {
                    if let Some(target) = node
                        .child_by_field_name("left")
                        .and_then(|n| n.named_child(0))
                    {
                        let name = if target.kind() == "generic_type" {
                            target.named_child(0)
                        } else {
                            Some(target)
                        };
                        if let Some(name) = name.filter(|n| n.kind() == "identifier")
                            && let Some(scope) = enclosing_scope(node, true)
                        {
                            bind(&mut bindings, scope, name, None, src);
                        }
                    }
                }
                "class_definition" if python => {
                    if let Some(name) = node.child_by_field_name("name")
                        && let Some(scope) = enclosing_scope(node, true)
                    {
                        if facts.is_some() && node.parent() == Some(root) {
                            bind_kind(
                                &mut bindings,
                                scope,
                                name,
                                Binding::Class(byte_range(node)),
                                src,
                            );
                            collect_export(root, node, name, true, src, &mut exports);
                        } else {
                            bind(&mut bindings, scope, name, None, src);
                        }
                    }
                }
                "class_declaration"
                | "abstract_class_declaration"
                | "enum_declaration"
                | "internal_module"
                | "function_signature" => {
                    if let Some(name) = node.child_by_field_name("name")
                        && let Some(scope) = enclosing_scope(node, python)
                    {
                        bind(&mut bindings, scope, name, None, src);
                    }
                }
                "for_statement" if python => {
                    let target = node.child_by_field_name("left");
                    block_method_writes(&mut methods, target, src, python);
                    if let (Some(target), Some(scope)) = (target, enclosing_scope(node, python)) {
                        block_pattern(&mut bindings, scope, target, src);
                    }
                }
                "for_in_statement" if !python => {
                    // for-in/of stores the declaration keyword on the loop itself.
                    let keyword = node.child_by_field_name("kind");
                    let target = node.child_by_field_name("left");
                    if keyword.is_none() {
                        block_method_writes(&mut methods, target, src, python);
                    }
                    if keyword.is_none() && facts.is_some() {
                        js_writes.extend(target);
                    } else {
                        let hoisted = keyword.is_some_and(|n| n.kind() == "var");
                        let scope = if hoisted {
                            binding_scope(node, false, true)
                        } else {
                            Some(node)
                        };
                        if let (Some(lhs), Some(scope)) = (target, scope) {
                            block_pattern(&mut bindings, scope, lhs, src);
                        }
                    }
                }
                "augmented_assignment"
                | "augmented_assignment_expression"
                | "update_expression" => {
                    let lhs = node
                        .child_by_field_name("left")
                        .or_else(|| node.child_by_field_name("argument"));
                    block_method_writes(&mut methods, lhs, src, python);
                    if !python && facts.is_some() {
                        js_writes.extend(lhs);
                    } else if let Some(lhs) = lhs
                        && let Some(scope) = enclosing_scope(node, python)
                    {
                        block_pattern(&mut bindings, scope, lhs, src);
                    }
                }
                "named_expression" | "as_pattern" | "case_clause" if python => {
                    if let Some(scope) = enclosing_scope(node, true) {
                        match node.kind() {
                            "named_expression" => {
                                if let Some(target) = node.child_by_field_name("name") {
                                    block_pattern(&mut bindings, scope, target, src);
                                }
                            }
                            "as_pattern" => {
                                let target = node.child_by_field_name("alias");
                                block_method_writes(&mut methods, target, src, python);
                                if let Some(target) = target {
                                    block_pattern(&mut bindings, scope, target, src);
                                }
                            }
                            _ => {
                                for pattern in node
                                    .named_children(&mut node.walk())
                                    .filter(|n| n.kind() == "case_pattern")
                                {
                                    block_case_pattern(&mut bindings, scope, pattern, src);
                                }
                            }
                        }
                    }
                }
                "global_statement" | "nonlocal_statement" if python => {
                    if let Some(scope) = enclosing_scope(node, true) {
                        for name in node
                            .named_children(&mut node.walk())
                            .filter(|n| n.kind() == "identifier")
                        {
                            python_directives.push((
                                scope,
                                name,
                                node.kind() == "global_statement",
                            ));
                        }
                    }
                }
                "delete_statement" if python => {
                    if let Some(scope) = enclosing_scope(node, python) {
                        for name in node.named_children(&mut node.walk()) {
                            block_method_writes(&mut methods, Some(name), src, python);
                            block_pattern(&mut bindings, scope, name, src);
                        }
                    }
                }
                "catch_clause" => {
                    if let Some(binding) = node.child_by_field_name("parameter") {
                        block_pattern(&mut bindings, node, binding, src);
                    }
                }
                _ => {}
            }
        }
        if cursor.goto_first_child() {
            continue;
        }
        while !cursor.goto_next_sibling() {
            if !cursor.goto_parent() {
                apply_python_directives(root, &python_directives, &mut bindings, src);
                if let Some(facts) = facts {
                    for pattern in js_writes {
                        visit_pattern(pattern, false, |name| {
                            let Ok(text) = name.utf8_text(src) else {
                                return;
                            };
                            // An assignment does not declare a new block local.
                            // Invalidate the visible binding, but never its namesake.
                            if let Some(scope) = lexical_scopes(name, false).find(|scope| {
                                bindings
                                    .get(scope)
                                    .is_some_and(|names| names.contains_key(text))
                            }) {
                                bind(&mut bindings, scope, name, None, src);
                            }
                        });
                    }
                    finish_values(
                        candidates
                            .iter()
                            .copied()
                            .filter(|&candidate| !python || python_value_context(candidate)),
                        &bindings,
                        &methods,
                        src,
                        python,
                        &opaque_import_scopes,
                        |name, _, definition| {
                            facts.locals.insert(byte_range(name), definition);
                        },
                    );
                    finish_imports(
                        candidates,
                        &bindings,
                        &imports,
                        &opaque_import_scopes,
                        src,
                        python,
                        facts,
                    );
                    for (exported, name, definition) in exports {
                        if opaque_import_scopes.contains(&root) {
                            continue;
                        }
                        let exports = match bindings.get(&root).and_then(|map| map.get(name)) {
                            Some(Binding::Callable(range)) if *range == definition => {
                                &mut facts.exports
                            }
                            Some(Binding::Class(range)) if *range == definition => {
                                &mut facts.class_exports
                            }
                            _ => continue,
                        };
                        exports.insert(exported.to_owned(), (name.to_owned(), definition));
                    }
                } else {
                    let mut seen: HashSet<_> = refs.iter().map(|r| r.byte_range).collect();
                    finish_values(
                        candidates,
                        &bindings,
                        &methods,
                        src,
                        python,
                        &opaque_import_scopes,
                        |name, qualifier, _| {
                            if seen.insert(byte_range(name)) {
                                push_ref(refs, name, src, qualifier, ReferenceKind::ValueUse);
                            }
                        },
                    );
                }
                return;
            }
        }
    }
}

fn jsx_component<'tree, 'src>(
    node: Node<'tree>,
    src: &'src [u8],
) -> Option<(Node<'tree>, Option<&'src str>)> {
    let tag = node.child_by_field_name("name")?;
    let text = tag.utf8_text(src).ok()?;
    match tag.kind() {
        // Simple lowercase and hyphenated JSX names denote intrinsic elements.
        "identifier" if !text.contains('-') && !text.as_bytes().first()?.is_ascii_lowercase() => {
            Some((tag, None))
        }
        // A member is a value expression even when its leaf is lowercase.
        "member_expression" => Some((tag.child_by_field_name("property")?, Some(text))),
        _ => None, // fragments, namespace tags and malformed tags
    }
}

// A non-callable or conflicting binding stops lexical lookup rather than
// falling through to a callable with the same name in an outer scope.
#[derive(PartialEq, Eq)]
enum Binding {
    Callable(ByteRange),
    Class(ByteRange),
    Imported,
    Parameter,
    Blocked,
}
type Bindings<'tree, 'src> = HashMap<Node<'tree>, HashMap<&'src str, Binding>>;

fn bind<'tree, 'src>(
    bindings: &mut Bindings<'tree, 'src>,
    scope: Node<'tree>,
    name: Node<'tree>,
    definition: Option<Node<'tree>>,
    src: &'src [u8],
) {
    bind_kind(
        bindings,
        scope,
        name,
        definition.map_or(Binding::Blocked, |node| Binding::Callable(byte_range(node))),
        src,
    );
}

fn bind_kind<'tree, 'src>(
    bindings: &mut Bindings<'tree, 'src>,
    scope: Node<'tree>,
    name: Node<'tree>,
    binding: Binding,
    src: &'src [u8],
) {
    if let Ok(text) = name.utf8_text(src) {
        bindings
            .entry(scope)
            .or_default()
            .entry(text)
            .and_modify(|known| *known = Binding::Blocked)
            .or_insert(binding);
    }
}

fn bind_parameters<'tree, 'src>(
    bindings: &mut Bindings<'tree, 'src>,
    function: Node<'tree>,
    src: &'src [u8],
) {
    if let Some(param) = function
        .child_by_field_name("parameter")
        .filter(|n| n.kind() == "identifier")
    {
        bind_kind(bindings, function, param, Binding::Parameter, src);
    }
    if let Some(params) = function.child_by_field_name("parameters") {
        for param in params.named_children(&mut params.walk()) {
            if let Some(name) = parameter_name(param) {
                bind_kind(bindings, function, name, Binding::Parameter, src);
            } else if let Some(pattern) = param.child_by_field_name("pattern") {
                block_pattern(bindings, function, pattern, src);
            } else {
                block_pattern(bindings, function, param, src);
            }
        }
    }
}

fn block_pattern<'tree, 'src>(
    bindings: &mut Bindings<'tree, 'src>,
    scope: Node<'tree>,
    pattern: Node<'tree>,
    src: &'src [u8],
) {
    visit_pattern(pattern, false, |name| {
        bind(bindings, scope, name, None, src)
    });
}

fn block_method_writes<'tree, 'src>(
    methods: &mut Bindings<'tree, 'src>,
    target: Option<Node<'tree>>,
    src: &'src [u8],
    python: bool,
) {
    let Some(target) = target.filter(|n| n.kind() != "identifier") else {
        return;
    };
    visit_pattern(target, true, |member| {
        let Some((name, _receiver, class, _method)) = receiver_member(member, src, python) else {
            return;
        };
        // A constructor or sibling method can change what this receiver sees.
        // Don't infer execution order or a replacement callable; block this
        // member throughout the class without touching lexical namesakes.
        if python {
            bind(methods, class, name, None, src);
        } else {
            block_member_name(methods, class, name, src);
        }
    });
}

fn block_member_name<'tree, 'src>(
    methods: &mut Bindings<'tree, 'src>,
    class: Node<'tree>,
    name: Node<'tree>,
    src: &'src [u8],
) {
    match name.kind() {
        "property_identifier" | "private_property_identifier" => match name.utf8_text(src) {
            Ok(text) if text.contains('\\') => {
                let key = member_key::identifier_name(text);
                block_class_methods(methods, class, key.as_deref(), src);
            }
            _ => bind(methods, class, name, None, src),
        },
        "number" => {}
        "computed_property_name" => match name.named_child(0) {
            Some(key) => block_member_name(methods, class, unparen(key), src),
            None => block_class_methods(methods, class, None, src),
        },
        _ => {
            let key = member_key::literal_key(name, src);
            block_class_methods(methods, class, key.as_deref(), src);
        }
    }
}

fn block_class_methods<'tree, 'src>(
    methods: &mut Bindings<'tree, 'src>,
    class: Node<'tree>,
    key: Option<&str>,
    src: &'src [u8],
) {
    let Some(body) = class.child_by_field_name("body") else {
        return;
    };
    for method in body
        .named_children(&mut body.walk())
        .filter(|n| n.kind() == "method_definition")
        .filter_map(|n| n.child_by_field_name("name"))
    {
        if key.is_none_or(|key| {
            method.kind() == "property_identifier" && method.utf8_text(src) == Ok(key)
        }) {
            bind(methods, class, method, None, src);
        }
    }
}

fn sole_declaration(node: Node<'_>) -> Node<'_> {
    node.parent()
        .filter(|parent| {
            node.kind() == "variable_declarator"
                && matches!(
                    parent.kind(),
                    "lexical_declaration" | "variable_declaration"
                )
                && parent
                    .named_children(&mut parent.walk())
                    .filter(|n| n.kind() == "variable_declarator")
                    .count()
                    == 1
        })
        .unwrap_or(node)
}

fn is_class_field(node: Node<'_>) -> bool {
    matches!(node.kind(), "field_definition" | "public_field_definition")
        && node.parent().is_some_and(|n| n.kind() == "class_body")
}

fn visit_pattern<'tree>(
    pattern: Node<'tree>,
    include_members: bool,
    mut visit: impl FnMut(Node<'tree>),
) {
    let mut todo = vec![pattern];
    while let Some(node) = todo.pop() {
        match node.kind() {
            "identifier" | "shorthand_property_identifier_pattern" if !include_members => {
                visit(node);
            }
            "attribute" | "member_expression" | "subscript_expression" if include_members => {
                visit(node)
            }
            "parenthesized_expression" if include_members => {
                let inner = unparen(node);
                if inner != node {
                    todo.push(inner);
                }
            }
            "tuple_pattern"
            | "list_pattern"
            | "pattern_list"
            | "expression_list"
            | "tuple"
            | "list"
            | "object_pattern"
            | "array_pattern"
            | "rest_pattern"
            | "list_splat_pattern"
            | "dictionary_splat_pattern"
            | "as_pattern_target" => todo.extend(node.named_children(&mut node.walk())),
            "pair_pattern" => todo.extend(node.child_by_field_name("value")),
            "assignment_pattern" | "object_assignment_pattern" => {
                todo.extend(node.child_by_field_name("left"))
            }
            _ => {}
        }
    }
}

fn block_case_pattern<'tree, 'src>(
    bindings: &mut Bindings<'tree, 'src>,
    scope: Node<'tree>,
    pattern: Node<'tree>,
    src: &'src [u8],
) {
    let mut todo = vec![pattern];
    while let Some(node) = todo.pop() {
        match node.kind() {
            // A bare name captures, a qualified name is a value pattern.
            "dotted_name" if node.named_child_count() == 1 => {
                if let Some(name) = node.named_child(0) {
                    bind(bindings, scope, name, None, src);
                }
            }
            "identifier" => bind(bindings, scope, node, None, src),
            "case_pattern" | "union_pattern" | "tuple_pattern" | "list_pattern" | "as_pattern"
            | "splat_pattern" => todo.extend(node.named_children(&mut node.walk())),
            "class_pattern" => todo.extend(node.named_children(&mut node.walk()).skip(1)),
            "keyword_pattern" => todo.extend(node.named_child(1)),
            "dict_pattern" => todo.extend(
                node.named_children(&mut node.walk())
                    .filter(|n| matches!(n.kind(), "case_pattern" | "splat_pattern")),
            ),
            _ => {}
        }
    }
}

fn apply_python_directives<'tree, 'src>(
    root: Node<'tree>,
    directives: &[(Node<'tree>, Node<'tree>, bool)],
    bindings: &mut Bindings<'tree, 'src>,
    src: &'src [u8],
) {
    for &(scope, name, global) in directives {
        let Ok(text) = name.utf8_text(src) else {
            continue;
        };
        // A directive is not, by itself, a write. When the scope also has a
        // binding operation, don't let other users assume the outer value.
        if bindings
            .get(&scope)
            .is_some_and(|names| names.contains_key(text))
        {
            if global {
                bind(bindings, root, name, None, src);
            } else {
                for outer in lexical_scopes(scope, true).filter(|n| is_function(n.kind())) {
                    if bindings
                        .get(&outer)
                        .is_some_and(|names| names.contains_key(text))
                    {
                        bind(bindings, outer, name, None, src);
                    }
                }
            }
        }
    }
    for &(scope, name, _) in directives {
        bind(bindings, scope, name, None, src);
    }
}

fn parameter_name(param: Node<'_>) -> Option<Node<'_>> {
    if param.kind() == "identifier" {
        return Some(param);
    }
    param
        .child_by_field_name("name")
        .or_else(|| param.child_by_field_name("pattern"))
        .or_else(|| param.child_by_field_name("left"))
        .or_else(|| {
            matches!(
                param.kind(),
                "typed_parameter"
                    | "list_splat_pattern"
                    | "dictionary_splat_pattern"
                    | "rest_pattern"
            )
            .then(|| param.named_child(0))
            .flatten()
        })
        .filter(|n| n.kind() == "identifier")
}

fn unparen(mut node: Node<'_>) -> Node<'_> {
    while node.kind() == "parenthesized_expression" {
        let Some(child) = node
            .named_children(&mut node.walk())
            .find(|n| n.kind() != "comment")
        else {
            break;
        };
        node = child;
    }
    node
}

fn is_candidate(node: Node<'_>) -> bool {
    matches!(
        node.kind(),
        "identifier" | "attribute" | "member_expression"
    )
}

fn has_child_kind(node: Node<'_>, kind: &str) -> bool {
    node.children(&mut node.walk()).any(|n| n.kind() == kind)
}

fn python_decorated(node: Node<'_>, src: &[u8], decorators: &[&str]) -> bool {
    node.parent()
        .filter(|n| n.kind() == "decorated_definition")
        .is_some_and(|parent| {
            parent.named_children(&mut parent.walk()).any(|n| {
                n.kind() == "decorator"
                    && n.utf8_text(src).ok().is_some_and(|text| {
                        let name = text.trim_start_matches('@').trim();
                        decorators.contains(&name.rsplit('.').next().unwrap_or(name))
                    })
            })
        })
}

fn is_function(kind: &str) -> bool {
    matches!(
        kind,
        "function_definition"
            | "function_declaration"
            | "function_expression"
            | "method_definition"
            | "arrow_function"
            | "generator_function"
            | "generator_function_declaration"
            | "lambda"
    )
}

fn enclosing_scope(node: Node<'_>, python: bool) -> Option<Node<'_>> {
    std::iter::successors(node.parent(), |n| n.parent()).find(|n| {
        is_function(n.kind())
            || if python {
                matches!(n.kind(), "module" | "class_definition")
            } else {
                matches!(
                    n.kind(),
                    "program"
                        | "statement_block"
                        | "for_statement"
                        | "for_in_statement"
                        | "catch_clause"
                        | "switch_body"
                        | "class"
                        | "class_static_block"
                        | "internal_module"
                )
            }
    })
}

fn binding_scope(node: Node<'_>, python: bool, hoisted: bool) -> Option<Node<'_>> {
    if hoisted {
        std::iter::successors(node.parent(), |n| n.parent()).find(|n| {
            is_function(n.kind())
                || matches!(
                    n.kind(),
                    "program" | "class_static_block" | "internal_module"
                )
        })
    } else {
        enclosing_scope(node, python)
    }
}

fn enclosing_method_class(node: Node<'_>, python: bool) -> Option<Node<'_>> {
    let mut parent = node.parent()?;
    if python && parent.kind() == "decorated_definition" {
        parent = parent.parent()?;
    }
    if python && node.kind() == "function_definition" && parent.kind() == "block" {
        parent.parent().filter(|n| n.kind() == "class_definition")
    } else if !python
        && (node.kind() == "method_definition" || is_class_field(node))
        && parent.kind() == "class_body"
    {
        parent.parent()
    } else {
        None
    }
}

fn resolve_binding<'tree>(
    value: Node<'tree>,
    name: &str,
    bindings: &Bindings<'tree, '_>,
    python: bool,
    opaque_scopes: &HashSet<Node<'tree>>,
) -> Option<ByteRange> {
    lexical_scopes(value, python).find_map(|scope| {
        if opaque_scopes.contains(&scope) {
            return Some(None);
        }
        Some(match bindings.get(&scope)?.get(name)? {
            Binding::Callable(definition) => Some(*definition),
            _ => None,
        })
    })?
}

fn lexical_scopes<'tree>(value: Node<'tree>, python: bool) -> impl Iterator<Item = Node<'tree>> {
    std::iter::successors(enclosing_scope(value, python), move |n| {
        enclosing_scope(*n, python)
    })
    .enumerate()
    // Class locals participate in their own body, not in nested scopes.
    .filter_map(|(depth, scope)| {
        (depth == 0 || scope.kind() != "class_definition").then_some(scope)
    })
}

fn finish_values<'tree>(
    candidates: impl IntoIterator<Item = Node<'tree>>,
    bindings: &Bindings<'tree, '_>,
    methods: &Bindings<'tree, '_>,
    src: &[u8],
    python: bool,
    opaque_scopes: &HashSet<Node<'tree>>,
    mut emit: impl FnMut(Node<'tree>, Option<&str>, ByteRange),
) {
    for value in candidates {
        match value.kind() {
            "identifier" | "shorthand_property_identifier" => {
                let Ok(name) = value.utf8_text(src) else {
                    continue;
                };
                if let Some(definition) =
                    resolve_binding(value, name, bindings, python, opaque_scopes)
                {
                    emit(value, None, definition);
                }
            }
            "attribute" | "member_expression" => {
                if let Some((name, qualifier, definition)) =
                    resolve_method(value, bindings, methods, src, python)
                {
                    emit(name, Some(&qualifier), definition);
                }
            }
            _ => {}
        }
    }
}

fn receiver_member<'tree>(
    value: Node<'tree>,
    src: &[u8],
    python: bool,
) -> Option<(Node<'tree>, Node<'tree>, Node<'tree>, Node<'tree>)> {
    let name = match (python, value.kind()) {
        (true, "attribute") => value.child_by_field_name("attribute")?,
        (false, "member_expression") => value.child_by_field_name("property")?,
        (false, "subscript_expression") => unparen(value.child_by_field_name("index")?),
        _ => return None,
    };
    let receiver = unparen(value.child_by_field_name("object")?);
    let enclosing = std::iter::successors(value.parent(), |n| n.parent()).find(|n| {
        (is_function(n.kind()) && (python || n.kind() != "arrow_function"))
            || matches!(n.kind(), "class_definition" | "class_body")
            || (!python && is_class_field(*n))
    })?;
    if enclosing.kind() == "method_definition"
        && enclosing
            .child_by_field_name("parameters")
            .is_none_or(|params| value.start_byte() < params.start_byte())
    {
        return None;
    }
    if is_class_field(enclosing)
        && enclosing.child_by_field_name("value").is_none_or(|init| {
            init.start_byte() > value.start_byte() || init.end_byte() < value.end_byte()
        })
    {
        return None;
    }
    let class = enclosing_method_class(enclosing, python)?;
    if python {
        let receiver = receiver.utf8_text(src).ok()?;
        let first = enclosing
            .child_by_field_name("parameters")?
            .named_child(0)?;
        let implicit_classmethod =
            enclosing.child_by_field_name("name")?.utf8_text(src).ok()? == "__init_subclass__";
        let classmethod =
            implicit_classmethod || python_decorated(enclosing, src, &["classmethod"]);
        if receiver != if classmethod { "cls" } else { "self" }
            || python_decorated(enclosing, src, &["staticmethod"])
            || parameter_name(first)?.utf8_text(src).ok()? != receiver
        {
            return None;
        }
    } else if receiver.kind() != "this" || has_child_kind(enclosing, "static") {
        return None;
    }
    Some((name, receiver, class, enclosing))
}

fn resolve_method<'tree>(
    value: Node<'tree>,
    bindings: &Bindings<'tree, '_>,
    methods: &Bindings<'tree, '_>,
    src: &[u8],
    python: bool,
) -> Option<(Node<'tree>, String, ByteRange)> {
    let (name, receiver, class, enclosing) = receiver_member(value, src, python)?;
    if python
        && bindings.get(&enclosing)?.get(receiver.utf8_text(src).ok()?) != Some(&Binding::Parameter)
    {
        return None;
    }
    let method = name.utf8_text(src).ok()?;
    let Binding::Callable(definition) = methods.get(&class)?.get(method)? else {
        return None;
    };
    let class_name = class.child_by_field_name("name")?.utf8_text(src).ok()?;
    Some((name, format!("{class_name}.{method}"), *definition))
}

fn byte_range(node: Node<'_>) -> ByteRange {
    (node.start_byte() as u32, node.end_byte() as u32)
}

fn push_ref(
    refs: &mut Vec<ReferenceRecord>,
    node: Node<'_>,
    src: &[u8],
    qualifier: Option<&str>,
    kind: ReferenceKind,
) {
    if let Ok(name) = node.utf8_text(src)
        && !name.is_empty()
    {
        refs.push(ReferenceRecord {
            name: name.to_owned(),
            qualified_name: qualifier.map(str::to_owned),
            kind,
            byte_range: byte_range(node),
            line_range: (
                node.start_position().row as u32,
                node.end_position().row as u32,
            ),
            enclosing_symbol_index: None,
        });
    }
}
