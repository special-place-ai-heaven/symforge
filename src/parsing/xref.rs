use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use streaming_iterator::StreamingIterator;
use tree_sitter::{Language, Node, Query, QueryCursor};

use crate::domain::{LanguageId, ReferenceKind, ReferenceRecord};

// ---------------------------------------------------------------------------
// Per-language query strings
// ---------------------------------------------------------------------------

const RUST_XREF_QUERY: &str = r#"
; Simple function calls: foo()
(call_expression function: (identifier) @ref.call)

; Qualified calls: Vec::new()  — capture the scoped_identifier for qualified_name too
(call_expression function: (scoped_identifier name: (identifier) @ref.call) @ref.qualified_call)

; Method calls: self.push()
(call_expression function: (field_expression field: (field_identifier) @ref.method_call))

; Macro invocations: println!()
(macro_invocation macro: (identifier) @ref.macro)

; Use declarations: use std::collections::HashMap
(use_declaration argument: (identifier) @ref.import)
(use_declaration argument: (scoped_identifier) @ref.import)
(use_declaration argument: (use_list) @ref.import)
(use_declaration argument: (scoped_use_list) @ref.import)

; Import with alias: use HashMap as Map
(use_as_clause path: (_) @import.original alias: (identifier) @import.alias)

; Type references (in function params, return types, struct fields, etc.)
(type_identifier) @ref.type

; Trait implementations: impl Trait for Struct
(impl_item trait: (type_identifier) @ref.implements type: (type_identifier) @ref.implements_target)
(impl_item trait: (scoped_type_identifier) @ref.implements type: (type_identifier) @ref.implements_target)
(impl_item trait: (generic_type) @ref.implements type: (type_identifier) @ref.implements_target)
"#;

// Focused query that collects the *definition* names of top-level / nested
// `const` and `static` items in a Rust file, e.g. `const FOO: u8 = 1;` →
// captures `FOO`.  Used to build the same-file const/static name set that
// gates value-position reference resolution (see `extract_rust_value_refs`).
const RUST_CONST_DEF_QUERY: &str = r#"
(const_item name: (identifier) @const.def)
(static_item name: (identifier) @const.def)
"#;

// Focused query that captures every bare value-position `(identifier)`.  On its
// own this is extremely noisy (it matches call targets, pattern bindings,
// parameters, etc.), so callers MUST gate each hit against the same-file
// const/static definition-name set and dedupe against ranges already captured
// by the main query (see `extract_rust_value_refs`).
const RUST_VALUE_IDENT_QUERY: &str = r#"
(identifier) @ref.value
"#;

const PYTHON_XREF_QUERY: &str = r#"
; Function calls: foo(x)
(call function: (identifier) @ref.call)

; Method calls: obj.method()
(call function: (attribute attribute: (identifier) @ref.method_call))

; Import statements: import os
(import_statement name: (dotted_name (identifier) @ref.import))

; From imports: from os import path
(import_from_statement module_name: (dotted_name (identifier) @ref.import))
(import_from_statement name: (dotted_name (identifier) @ref.import))

; Import alias: import numpy as np
(aliased_import name: (dotted_name (identifier) @import.original) alias: (identifier) @import.alias)

; Bare type identifiers in annotation positions
(type (identifier) @ref.type)

; Subscript type annotations: QuerySet[Model]
(type (subscript value: (identifier) @ref.type))
(type (subscript subscript: (identifier) @ref.type))
(subscript value: (identifier) @ref.type)
(subscript subscript: (identifier) @ref.type)
(type (generic_type (identifier) @ref.type))
(generic_type (identifier) @ref.type)
(type (generic_type (type_parameter (type (identifier) @ref.type))))
(generic_type (type_parameter (type (identifier) @ref.type)))

; Attribute types in annotations: models.QuerySet
(type (attribute attribute: (identifier) @ref.type))

; Attribute chains: models.Model.__hash__
(attribute object: (attribute attribute: (identifier) @ref.type))

; Subscript with attribute base (legacy grammar)
(type (subscript value: (attribute attribute: (identifier) @ref.type)))
(subscript value: (attribute attribute: (identifier) @ref.type))

; Class inheritance — attribute base: class Foo(models.Model)
(class_definition
  superclasses: (argument_list
    (attribute attribute: (identifier) @ref.implements)))

; Class inheritance — bare base with implementor: class Foo(Bar)
(class_definition
  name: (identifier) @ref.implements_target
  superclasses: (argument_list (identifier) @ref.implements))
"#;

// Captures identifier/type-like tokens in call argument lists for a second pass
// (isinstance(x, Model), ForeignKey(Model), etc.). Gated in Rust by uppercase
// PEP-8 class-name heuristic and dedupe against the main query.
const PYTHON_VALUE_TYPE_IDENT_QUERY: &str = r#"
(argument_list (identifier) @ref.value)
(argument_list (attribute attribute: (identifier) @ref.value_attr))
"#;

const PYTHON_STRING_TYPE_QUERY: &str = r#"
(argument_list (string) @ref.string)
"#;

const JS_XREF_QUERY: &str = r#"
; Function calls: foo()
(call_expression function: (identifier) @ref.call)

; Method calls: obj.method()
(call_expression function: (member_expression property: (property_identifier) @ref.method_call))

; Constructor: new Foo()
(new_expression constructor: (identifier) @ref.call)

; Import statements: import React from 'react'
(import_statement source: (string) @ref.import)

; Import specifiers: import { Component } from 'react'
(import_specifier name: (identifier) @ref.import)
"#;

const TS_XREF_QUERY: &str = r#"
; Function calls: foo()
(call_expression function: (identifier) @ref.call)

; Method calls: obj.method()
(call_expression function: (member_expression property: (property_identifier) @ref.method_call))

; Constructor: new Foo()
(new_expression constructor: (identifier) @ref.call)

; Import statements: import React from 'react'
(import_statement source: (string) @ref.import)

; Import specifiers: import { Component } from 'react'
(import_specifier name: (identifier) @ref.import)

; TypeScript type references: param: MyType
(type_identifier) @ref.type

; Class implements: class Foo implements Bar
(class_declaration
  name: (type_identifier) @ref.implements_target
  (class_heritage
    (implements_clause
      (type_identifier) @ref.implements)))

; Class extends: class Foo extends Bar
(class_declaration
  name: (type_identifier) @ref.implements_target
  (class_heritage
    (extends_clause
      value: (identifier) @ref.implements)))
"#;

const GO_XREF_QUERY: &str = r#"
; Simple function calls: foo()
(call_expression function: (identifier) @ref.call)

; Selector calls: fmt.Println()
(call_expression function: (selector_expression field: (field_identifier) @ref.method_call))

; Import specs: import "fmt"
(import_spec path: (interpreted_string_literal) @ref.import)

; Type references
(type_identifier) @ref.type
"#;

const JAVA_XREF_QUERY: &str = r#"
; Method invocations: obj.println()
(method_invocation name: (identifier) @ref.call)

; Object creation: new ArrayList()
(object_creation_expression type: (type_identifier) @ref.call)

; Import declarations: import java.util.ArrayList
(import_declaration (scoped_identifier) @ref.import)

; Type references
(type_identifier) @ref.type

; Class implements: class Foo implements Bar
(class_declaration
  name: (identifier) @ref.implements_target
  interfaces: (super_interfaces
    (type_list (type_identifier) @ref.implements)))

; Class extends: class Foo extends Bar
(class_declaration
  name: (identifier) @ref.implements_target
  superclass: (superclass (type_identifier) @ref.implements))
"#;

const C_XREF_QUERY: &str = r#"
; Function calls: foo()
(call_expression function: (identifier) @ref.call)

; Method/field calls: obj->method() or obj.field()
(call_expression function: (field_expression field: (field_identifier) @ref.method_call))

; #include imports: "header.h" or <stdio.h>
(preproc_include path: (string_literal) @ref.import)
(preproc_include path: (system_lib_string) @ref.import)

; Type identifiers: MyStruct, typedef'd names
(type_identifier) @ref.type
"#;

const CPP_XREF_QUERY: &str = r#"
; Function calls: foo()
(call_expression function: (identifier) @ref.call)

; Method calls: obj.method() or obj->method()
(call_expression function: (field_expression field: (field_identifier) @ref.method_call))

; Qualified calls: std::sort(), Foo::bar() — capture the qualified_identifier as
; @ref.qualified_call so the type/namespace head (`Foo`) is recoverable for D13
; head-match recall (find_references("Foo") must see `Foo::bar()`/`Foo::create()`
; static-call & construction sites, which are keyed under the leaf `bar`).
; Scoped to call position to mirror the Rust rule and avoid double-capturing the
; leaf for non-call qualified identifiers.
(call_expression function: (qualified_identifier name: (identifier) @ref.call) @ref.qualified_call)

; #include imports
(preproc_include path: (string_literal) @ref.import)
(preproc_include path: (system_lib_string) @ref.import)

; Type identifiers
(type_identifier) @ref.type

; Template instantiation: vector<int>
(template_type name: (type_identifier) @ref.type)

; Using declarations: using std::string
(using_declaration (qualified_identifier) @import.original)

; Class inheritance: class Foo : public Bar
(class_specifier
  name: (type_identifier) @ref.implements_target
  (base_class_clause
    (type_identifier) @ref.implements))
"#;

const CSHARP_XREF_QUERY: &str = r#"
; Method invocations: obj.Method() — capture the method name
(invocation_expression
  (member_access_expression name: (identifier) @ref.method_call))

; Simple function calls: foo()
(invocation_expression
  function: (identifier) @ref.call)

; Object creation: new Foo()
(object_creation_expression type: (identifier) @ref.call)

; Using directives: keep the full namespace/type text when present.
(using_directive (qualified_name) @ref.import)
(using_directive (alias_qualified_name) @ref.import)
(using_directive (generic_name) @ref.import)
(using_directive (identifier) @ref.import)

; Type references in DI-style constructor params, fields, properties, and return types.
(parameter type: (type) @ref.type)
(variable_declaration type: (type) @ref.type)
(property_declaration type: (type) @ref.type)

; Class implements/extends: class Foo : IBar, Base
(class_declaration
  name: (identifier) @ref.implements_target
  (base_list (identifier) @ref.implements))
(class_declaration
  name: (identifier) @ref.implements_target
  (base_list (qualified_name) @ref.implements))
(class_declaration
  name: (identifier) @ref.implements_target
  (base_list (generic_name) @ref.implements))
"#;

const RUBY_XREF_QUERY: &str = r#"
; Method calls: foo() or object.method()
(call receiver: (_) method: (identifier) @ref.method_call)
(call method: (identifier) @ref.call)

; Require/require_relative
(call method: (identifier) @ref.import
  (#match? @ref.import "^require"))

; Constant references: MyClass, MY_CONST
(constant) @ref.type

; Class inheritance: class Foo < Bar
(class
  name: (constant) @ref.implements_target
  (superclass (constant) @ref.implements))
(class
  name: (constant) @ref.implements_target
  (superclass (scope_resolution) @ref.implements))
"#;

const KOTLIN_XREF_QUERY: &str = r#"
; Function/method calls: foo() or obj.method()
(call_expression (simple_identifier) @ref.call)
(navigation_expression (simple_identifier) @ref.method_call)

; Import directives
(import_header (identifier) @ref.import)

; Type references
(user_type (type_identifier) @ref.type)
"#;

const DART_XREF_QUERY: &str = r#"
; nielsenko tree-sitter-dart (0.2.0) call shapes, mirroring upstream
; tags.scm reference queries.

; Plain function calls: print(x), Circle(2.0)
(call_expression
  function: (identifier) @ref.call)

; Method calls through a receiver: c.area(), obj?.method()
(call_expression
  function: (member_expression property: (identifier) @ref.method_call))
(call_expression
  function: (null_aware_member_expression property: (identifier) @ref.method_call))

; Import directives: import 'dart:core'
; import_specification -> configurable_uri -> uri -> string_literal
(import_specification (configurable_uri (uri (string_literal) @ref.import)))

; Type identifiers
(type_identifier) @ref.type
"#;

const ELIXIR_XREF_QUERY: &str = r#"
; Function calls: Module.function()
(call target: (dot left: (alias) right: (identifier) @ref.method_call))

; Local function calls: my_func()
(call target: (identifier) @ref.call)

; alias/import/use module references — capture the module alias
(call target: (identifier)
  (arguments (alias) @ref.import))
"#;

const PHP_XREF_QUERY: &str = r#"
; Function calls: foo() — function: can be qualified_name or variable_name
(function_call_expression
  function: (qualified_name (name) @ref.call))

; Method calls: $obj->method()
(member_call_expression
  name: (name) @ref.method_call)

; Static calls: Foo::bar()
(scoped_call_expression
  name: (name) @ref.method_call)

; Type references in class extends/implements
(named_type (name) @ref.type)

; Class implements: class Foo implements Bar
(class_declaration
  name: (name) @ref.implements_target
  (class_interface_clause (name) @ref.implements))

; Class extends: class Foo extends Bar
(class_declaration
  name: (name) @ref.implements_target
  (base_clause (name) @ref.implements))
"#;

const SWIFT_XREF_QUERY: &str = r#"
; Navigation expression method calls: obj.method()
(navigation_expression
  (simple_identifier) @ref.method_call)

; Import declarations: import Foundation
(import_declaration
  (identifier) @ref.import)

; Type references
(user_type
  (type_identifier) @ref.type)

; Protocol conformance: class Foo: Bar, Protocol
(class_declaration
  name: (type_identifier) @ref.implements_target
  (inheritance_specifier
    (user_type (type_identifier) @ref.implements)))
"#;

const PERL_XREF_QUERY: &str = r#"
; Method calls: $obj->method()
(method_invocation
  function_name: (identifier) @ref.method_call)

; require statements
(require_statement
  package_name: (package_name) @ref.import)
"#;

// ---------------------------------------------------------------------------
// OnceLock-cached compiled queries
// ---------------------------------------------------------------------------

static RUST_QUERY: OnceLock<Query> = OnceLock::new();
static RUST_CONST_DEF_QUERY_C: OnceLock<Query> = OnceLock::new();
static RUST_VALUE_IDENT_QUERY_C: OnceLock<Query> = OnceLock::new();
static PYTHON_QUERY: OnceLock<Query> = OnceLock::new();
static PYTHON_VALUE_TYPE_IDENT_QUERY_C: OnceLock<Query> = OnceLock::new();
static PYTHON_STRING_TYPE_QUERY_C: OnceLock<Query> = OnceLock::new();
static JS_QUERY: OnceLock<Query> = OnceLock::new();
static TS_QUERY: OnceLock<Query> = OnceLock::new();
// A tree-sitter Query binds to a specific Language's node-kind IDs. The TSX
// grammar (`LANGUAGE_TSX`) is a distinct Language from `LANGUAGE_TYPESCRIPT`,
// so the same xref query source must be compiled and cached separately for it;
// reusing the TS-compiled query against a TSX tree yields wrong/empty matches.
static TSX_QUERY: OnceLock<Query> = OnceLock::new();
static GO_QUERY: OnceLock<Query> = OnceLock::new();
static JAVA_QUERY: OnceLock<Query> = OnceLock::new();
static C_QUERY: OnceLock<Query> = OnceLock::new();
static CPP_QUERY: OnceLock<Query> = OnceLock::new();
static CSHARP_QUERY: OnceLock<Query> = OnceLock::new();
static RUBY_QUERY: OnceLock<Query> = OnceLock::new();
static KOTLIN_QUERY: OnceLock<Query> = OnceLock::new();
static DART_QUERY: OnceLock<Query> = OnceLock::new();
static ELIXIR_QUERY: OnceLock<Query> = OnceLock::new();
static PHP_QUERY: OnceLock<Query> = OnceLock::new();
static SWIFT_QUERY: OnceLock<Query> = OnceLock::new();
static PERL_QUERY: OnceLock<Query> = OnceLock::new();

// ponytail: these ~17 per-language query getters look like collapsible
// boilerplate (audit 2026-06-17, Tier 4a). A `OnceLock<Query>`-table collapse was
// implemented and fully verified (all 21 langs byte-identical, 3016 tests green)
// and then REJECTED 2026-06-18: it saved ~0 production lines, the 17-arm dispatch
// match is irreducible regardless, and it traded "a language cannot be mis-bound"
// (robust by construction) for "a language is mis-bound if the slot/enum order
// ever drifts" (guarded only by a test). For load-bearing symbol resolution —
// where a wrong query silently corrupts xref and breaks LLM trust — the explicit,
// impossible-to-misalign form is the superior one. Do NOT collapse without a
// reason that outweighs that trade.
fn rust_query(lang: &Language) -> &'static Query {
    RUST_QUERY.get_or_init(|| Query::new(lang, RUST_XREF_QUERY).expect("valid rust xref query"))
}

fn rust_const_def_query(lang: &Language) -> &'static Query {
    RUST_CONST_DEF_QUERY_C
        .get_or_init(|| Query::new(lang, RUST_CONST_DEF_QUERY).expect("valid rust const-def query"))
}

fn rust_value_ident_query(lang: &Language) -> &'static Query {
    RUST_VALUE_IDENT_QUERY_C.get_or_init(|| {
        Query::new(lang, RUST_VALUE_IDENT_QUERY).expect("valid rust value-ident query")
    })
}

fn python_query(lang: &Language) -> &'static Query {
    PYTHON_QUERY
        .get_or_init(|| Query::new(lang, PYTHON_XREF_QUERY).expect("valid python xref query"))
}

fn python_value_type_ident_query(lang: &Language) -> &'static Query {
    PYTHON_VALUE_TYPE_IDENT_QUERY_C.get_or_init(|| {
        Query::new(lang, PYTHON_VALUE_TYPE_IDENT_QUERY)
            .expect("valid python value-type ident query")
    })
}

fn python_string_type_query(lang: &Language) -> &'static Query {
    PYTHON_STRING_TYPE_QUERY_C.get_or_init(|| {
        Query::new(lang, PYTHON_STRING_TYPE_QUERY).expect("valid python string-type query")
    })
}

fn js_query(lang: &Language) -> &'static Query {
    JS_QUERY.get_or_init(|| Query::new(lang, JS_XREF_QUERY).expect("valid js xref query"))
}

fn ts_query(lang: &Language) -> &'static Query {
    TS_QUERY.get_or_init(|| Query::new(lang, TS_XREF_QUERY).expect("valid ts xref query"))
}

/// The TSX-grammar counterpart of [`ts_query`]. Compiled against `LANGUAGE_TSX`
/// and cached independently of the plain TypeScript query.
fn tsx_query(lang: &Language) -> &'static Query {
    TSX_QUERY.get_or_init(|| Query::new(lang, TS_XREF_QUERY).expect("valid tsx xref query"))
}

fn go_query(lang: &Language) -> &'static Query {
    GO_QUERY.get_or_init(|| Query::new(lang, GO_XREF_QUERY).expect("valid go xref query"))
}

fn java_query(lang: &Language) -> &'static Query {
    JAVA_QUERY.get_or_init(|| Query::new(lang, JAVA_XREF_QUERY).expect("valid java xref query"))
}

fn c_query(lang: &Language) -> &'static Query {
    C_QUERY.get_or_init(|| Query::new(lang, C_XREF_QUERY).expect("valid c xref query"))
}

fn cpp_query(lang: &Language) -> &'static Query {
    CPP_QUERY.get_or_init(|| Query::new(lang, CPP_XREF_QUERY).expect("valid cpp xref query"))
}

fn csharp_query(lang: &Language) -> &'static Query {
    CSHARP_QUERY
        .get_or_init(|| Query::new(lang, CSHARP_XREF_QUERY).expect("valid csharp xref query"))
}

fn ruby_query(lang: &Language) -> &'static Query {
    RUBY_QUERY.get_or_init(|| Query::new(lang, RUBY_XREF_QUERY).expect("valid ruby xref query"))
}

fn kotlin_query(lang: &Language) -> &'static Query {
    KOTLIN_QUERY
        .get_or_init(|| Query::new(lang, KOTLIN_XREF_QUERY).expect("valid kotlin xref query"))
}

fn dart_query(lang: &Language) -> &'static Query {
    DART_QUERY.get_or_init(|| Query::new(lang, DART_XREF_QUERY).expect("valid dart xref query"))
}

fn elixir_query(lang: &Language) -> &'static Query {
    ELIXIR_QUERY
        .get_or_init(|| Query::new(lang, ELIXIR_XREF_QUERY).expect("valid elixir xref query"))
}

fn php_query(lang: &Language) -> &'static Query {
    PHP_QUERY.get_or_init(|| Query::new(lang, PHP_XREF_QUERY).expect("valid php xref query"))
}

fn swift_query(lang: &Language) -> &'static Query {
    SWIFT_QUERY.get_or_init(|| Query::new(lang, SWIFT_XREF_QUERY).expect("valid swift xref query"))
}

fn perl_query(lang: &Language) -> &'static Query {
    PERL_QUERY.get_or_init(|| Query::new(lang, PERL_XREF_QUERY).expect("valid perl xref query"))
}

fn split_top_level_rust_items(input: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;

    for (idx, ch) in input.char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                parts.push(input[start..idx].trim());
                start = idx + ch.len_utf8();
            }
            _ => {}
        }
    }

    if start <= input.len() {
        parts.push(input[start..].trim());
    }

    parts.into_iter().filter(|part| !part.is_empty()).collect()
}

fn rust_grouped_import_parts(input: &str) -> Option<(&str, &str)> {
    let trimmed = input.trim();
    let brace_start = trimmed.find('{')?;
    let brace_end = trimmed.rfind('}')?;
    if brace_end <= brace_start {
        return None;
    }

    let prefix = trimmed[..brace_start].trim_end().trim_end_matches("::");
    let inner = &trimmed[brace_start + 1..brace_end];
    Some((prefix, inner))
}

/// Whether an Elixir `@ref.import` alias node belongs to a genuine module
/// directive (`alias`/`import`/`use`/`require`) rather than an arbitrary call
/// that happens to take a module alias as its first argument (e.g.
/// `raise ArgumentError`, `socket "...", UserSocket`).
///
/// The import query captures the `(alias)` inside `(call target: (identifier)
/// (arguments (alias)))`. We walk up to the enclosing `call` node, read its
/// `target` field text, and accept only the directive keywords. Returns `false`
/// when the enclosing call or its target cannot be resolved, so a non-directive
/// call never produces a spurious `Import` reference.
fn elixir_import_target_is_directive(alias_node: Node, source_bytes: &[u8]) -> bool {
    const DIRECTIVES: [&str; 4] = ["alias", "import", "use", "require"];

    // Walk up to the nearest `call` ancestor (alias -> arguments -> call).
    let mut node = alias_node;
    while node.kind() != "call" {
        match node.parent() {
            Some(parent) => node = parent,
            None => return false,
        }
    }

    node.child_by_field_name("target")
        .and_then(|target| target.utf8_text(source_bytes).ok())
        .map(|text| DIRECTIVES.contains(&text.trim()))
        .unwrap_or(false)
}

fn expand_rust_import_paths(input: &str) -> Vec<String> {
    let trimmed = input.trim();
    let alias_free = trimmed
        .split_once(" as ")
        .map(|(original, _)| original.trim())
        .unwrap_or(trimmed);

    if let Some((prefix, inner)) = rust_grouped_import_parts(alias_free) {
        return split_top_level_rust_items(inner)
            .into_iter()
            .flat_map(|item| {
                let combined = if prefix.is_empty() {
                    item.to_string()
                } else {
                    format!("{prefix}::{item}")
                };
                expand_rust_import_paths(&combined)
            })
            .collect();
    }

    vec![alias_free.to_string()]
}

fn push_import_reference(
    references: &mut Vec<ReferenceRecord>,
    import_text: &str,
    import_node: Node,
) {
    let full_text = import_text.trim();
    let name = if full_text.contains('/') && !full_text.contains("::") {
        // JS/TS path imports like '../utils/helpers' — take the last path segment
        full_text.split('/').next_back().unwrap_or(full_text)
    } else {
        // Reduce a qualified import to its final segment so name-based
        // find_references matches it: strip `::` then `.` (namespace.Class ->
        // Class, module path 'pkg.sub' -> sub), mirroring the `/` path-segment
        // handling above. `qualified_name` below retains the full dotted path.
        let after_colons = full_text.rsplit("::").next().unwrap_or(full_text);
        after_colons.rsplit('.').next().unwrap_or(after_colons)
    };
    let name = name.trim_matches('"').trim_matches('\'').to_string();

    if name.is_empty() {
        return;
    }

    let qualified_name = if full_text.contains("::") || full_text.contains('.') {
        Some(full_text.to_string())
    } else {
        None
    };
    let start = import_node.start_position();
    let end = import_node.end_position();
    references.push(ReferenceRecord {
        name,
        qualified_name,
        kind: ReferenceKind::Import,
        byte_range: (
            import_node.start_byte() as u32,
            import_node.end_byte() as u32,
        ),
        line_range: (start.row as u32, end.row as u32),
        enclosing_symbol_index: None,
    });
}

fn python_attribute_qualified_name(node: Node, source_bytes: &[u8]) -> Option<String> {
    if node.kind() == "attribute" {
        return node
            .utf8_text(source_bytes)
            .ok()
            .map(|text| text.trim().to_string());
    }
    node.parent()
        .filter(|parent| parent.kind() == "attribute")
        .and_then(|parent| parent.utf8_text(source_bytes).ok())
        .map(|text| text.trim().to_string())
}

fn python_name_looks_like_type(name: &str) -> bool {
    name.chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_uppercase())
}

/// Resolve uppercase class-name tokens in call argument lists (e.g.
/// `isinstance(x, Model)`, `ForeignKey(Model)`).
fn extract_python_value_refs(
    root: &Node,
    source: &str,
    ts_language: &Language,
    existing: &[ReferenceRecord],
) -> Vec<ReferenceRecord> {
    let source_bytes = source.as_bytes();
    let existing_ranges: HashSet<(u32, u32)> = existing.iter().map(|r| r.byte_range).collect();
    let value_query = python_value_type_ident_query(ts_language);
    let value_capture_names = value_query.capture_names();
    let mut out: Vec<ReferenceRecord> = Vec::new();
    let mut emitted_ranges: HashSet<(u32, u32)> = HashSet::new();

    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(value_query, *root, source_bytes);
    while let Some(m) = {
        matches.advance();
        matches.get()
    } {
        let mut value_ident: Option<Node> = None;
        let mut value_attr: Option<Node> = None;

        for capture in m.captures {
            match value_capture_names[capture.index as usize] {
                "ref.value" => value_ident = Some(capture.node),
                "ref.value_attr" => value_attr = Some(capture.node),
                _ => {}
            }
        }

        let (node, qualified_name) = if let Some(attr_node) = value_attr {
            let Some(parent) = attr_node.parent().filter(|n| n.kind() == "attribute") else {
                continue;
            };
            (
                attr_node,
                parent
                    .utf8_text(source_bytes)
                    .ok()
                    .map(|text| text.trim().to_string()),
            )
        } else if let Some(ident_node) = value_ident {
            (ident_node, None)
        } else {
            continue;
        };

        let range = (node.start_byte() as u32, node.end_byte() as u32);
        if existing_ranges.contains(&range) || emitted_ranges.contains(&range) {
            continue;
        }

        let Ok(text) = node.utf8_text(source_bytes) else {
            continue;
        };
        let name = text.trim();
        if name.is_empty() || !python_name_looks_like_type(name) {
            continue;
        }

        let start = node.start_position();
        let end = node.end_position();
        out.push(ReferenceRecord {
            name: name.to_string(),
            qualified_name,
            kind: ReferenceKind::ValueUse,
            byte_range: range,
            line_range: (start.row as u32, end.row as u32),
            enclosing_symbol_index: None,
        });
        emitted_ranges.insert(range);
    }

    out
}

fn python_string_type_name(raw: &str) -> Option<(String, Option<String>)> {
    let trimmed = raw.trim();
    let unquoted = trimmed
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .or_else(|| {
            trimmed
                .strip_prefix('\'')
                .and_then(|s| s.strip_suffix('\''))
        })?;
    let unquoted = unquoted.trim();
    if unquoted.is_empty() {
        return None;
    }
    if python_name_looks_like_type(unquoted) {
        return Some((unquoted.to_string(), None));
    }
    if let Some((prefix, name)) = unquoted.rsplit_once('.')
        && python_name_looks_like_type(name)
    {
        return Some((name.to_string(), Some(format!("{prefix}.{name}"))));
    }
    None
}

/// Resolve class-name tokens in string call arguments (migration labels, mock.patch paths).
fn extract_python_string_type_refs(
    root: &Node,
    source: &str,
    ts_language: &Language,
    existing: &[ReferenceRecord],
) -> Vec<ReferenceRecord> {
    let source_bytes = source.as_bytes();
    let existing_ranges: HashSet<(u32, u32)> = existing.iter().map(|r| r.byte_range).collect();
    let string_query = python_string_type_query(ts_language);
    let string_capture_names = string_query.capture_names();
    let mut out: Vec<ReferenceRecord> = Vec::new();
    let mut emitted_ranges: HashSet<(u32, u32)> = HashSet::new();

    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(string_query, *root, source_bytes);
    while let Some(m) = {
        matches.advance();
        matches.get()
    } {
        for capture in m.captures {
            if string_capture_names[capture.index as usize] != "ref.string" {
                continue;
            }
            let node = capture.node;
            let range = (node.start_byte() as u32, node.end_byte() as u32);
            if existing_ranges.contains(&range) || emitted_ranges.contains(&range) {
                continue;
            }
            let Ok(text) = node.utf8_text(source_bytes) else {
                continue;
            };
            let Some((name, qualified_name)) = python_string_type_name(text) else {
                continue;
            };
            let start = node.start_position();
            let end = node.end_position();
            out.push(ReferenceRecord {
                name,
                qualified_name,
                kind: ReferenceKind::ValueUse,
                byte_range: range,
                line_range: (start.row as u32, end.row as u32),
                enclosing_symbol_index: None,
            });
            emitted_ranges.insert(range);
        }
    }

    out
}

/// Resolve bare value-position references to same-file `const`/`static` items.
///
/// The main Rust query only captures identifiers in *call*, *type*, *import*,
/// *macro*, or *qualified-path* positions. A `const`/`static` used as a plain
/// value — iterated in `for x in CONST`, passed as `CONST.contains(..)`, or
/// handed to a function as a bare argument — is just an `(identifier)` node and
/// is therefore invisible to the main query. This pass closes that gap with
/// strict, precision-first calibration:
///
/// 1. It builds the set of `const`/`static` definition names declared *in this
///    same file*. A bare identifier is emitted ONLY when its exact text is in
///    that set, so ordinary locals, parameters, struct fields, and function
///    names are never reported (a `let foo` is not a reference to `const FOO`,
///    and case is significant). If the file declares no consts/statics the pass
///    does nothing.
/// 2. It skips the definition sites themselves (the identifier in
///    `const FOO: .. = ..;` is not a self-reference).
/// 3. It dedupes against every range already captured by the main query, so a
///    name already recorded as a call/type/import/macro/qualified-path hit is
///    not double-counted, and it dedupes within this pass.
///
/// Precision over recall: a cross-file bare use whose target const is not
/// defined in the current file is intentionally *not* resolved here (we have no
/// whole-repo symbol table at extraction time). Missing such a use is
/// acceptable — `find_references` prints a `search_text` fallback hint on an
/// empty result — whereas a false positive would poison a high-traffic tool.
fn extract_rust_value_refs(
    root: &Node,
    source: &str,
    ts_language: &Language,
    existing: &[ReferenceRecord],
) -> Vec<ReferenceRecord> {
    let source_bytes = source.as_bytes();

    // Step 1: collect same-file const/static definition names and their
    // definition-site byte ranges (so we can skip the definition itself).
    let def_query = rust_const_def_query(ts_language);
    let def_capture_names = def_query.capture_names();
    let mut def_names: HashSet<String> = HashSet::new();
    let mut def_ranges: HashSet<(u32, u32)> = HashSet::new();
    {
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(def_query, *root, source_bytes);
        while let Some(m) = {
            matches.advance();
            matches.get()
        } {
            for capture in m.captures {
                if def_capture_names[capture.index as usize] != "const.def" {
                    continue;
                }
                let node = capture.node;
                if let Ok(text) = node.utf8_text(source_bytes) {
                    let trimmed = text.trim();
                    if !trimmed.is_empty() {
                        def_names.insert(trimmed.to_string());
                        def_ranges.insert((node.start_byte() as u32, node.end_byte() as u32));
                    }
                }
            }
        }
    }

    // No consts/statics defined here → nothing this pass can resolve.
    if def_names.is_empty() {
        return Vec::new();
    }

    // Ranges already captured by the main query (any kind), used for dedup so a
    // name already recorded as a call/type/import/macro/qualified hit is not
    // emitted again as a value use.
    let existing_ranges: HashSet<(u32, u32)> = existing.iter().map(|r| r.byte_range).collect();

    // Step 2: scan every bare value-position identifier and emit a reference
    // only for those resolving to a known same-file const/static.
    let value_query = rust_value_ident_query(ts_language);
    let value_capture_names = value_query.capture_names();
    let mut out: Vec<ReferenceRecord> = Vec::new();
    let mut emitted_ranges: HashSet<(u32, u32)> = HashSet::new();

    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(value_query, *root, source_bytes);
    while let Some(m) = {
        matches.advance();
        matches.get()
    } {
        for capture in m.captures {
            if value_capture_names[capture.index as usize] != "ref.value" {
                continue;
            }
            let node = capture.node;
            let range = (node.start_byte() as u32, node.end_byte() as u32);

            // Skip the definition site itself.
            if def_ranges.contains(&range) {
                continue;
            }
            // Skip anything already captured by the main query or this pass.
            if existing_ranges.contains(&range) || emitted_ranges.contains(&range) {
                continue;
            }

            let Ok(text) = node.utf8_text(source_bytes) else {
                continue;
            };
            let name = text.trim();
            // Exact, case-sensitive match against same-file const/static names.
            if name.is_empty() || !def_names.contains(name) {
                continue;
            }

            let start = node.start_position();
            let end = node.end_position();
            out.push(ReferenceRecord {
                name: name.to_string(),
                qualified_name: None,
                kind: ReferenceKind::ValueUse,
                byte_range: range,
                line_range: (start.row as u32, end.row as u32),
                enclosing_symbol_index: None,
            });
            emitted_ranges.insert(range);
        }
    }

    out
}

// ---------------------------------------------------------------------------
// Main extraction function
// ---------------------------------------------------------------------------

/// Collect the byte ranges of every `macro_invocation` node in a Rust tree.
///
/// tree-sitter parses macro arguments as an opaque `token_tree` (raw tokens),
/// so call/type/import queries never fire inside a macro body. Callers use
/// these ranges to scope a text-based recovery scan to macro interiors only,
/// where whole-file scanning would otherwise be too imprecise.
fn collect_rust_macro_ranges(root: &Node) -> Vec<(u32, u32)> {
    let mut ranges: Vec<(u32, u32)> = Vec::new();
    let mut cursor = root.walk();
    let mut stack: Vec<Node> = vec![*root];
    while let Some(node) = stack.pop() {
        if node.kind() == "macro_invocation" {
            ranges.push((node.start_byte() as u32, node.end_byte() as u32));
        }
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }
    ranges
}

/// Extract cross-references from a parsed tree-sitter tree.
///
/// Returns `(Vec<ReferenceRecord>, HashMap<String, String>)` where the HashMap
/// is the alias map (alias → original) built from import-as patterns.
///
/// Note: `enclosing_symbol_index` is NOT set here — it is assigned later in
/// `IndexedFile::from_parse_result` where both symbols and references are available.
pub fn extract_references(
    root: &Node,
    source: &str,
    language: &LanguageId,
    is_tsx: bool,
) -> (Vec<ReferenceRecord>, HashMap<String, String>) {
    let source_bytes = source.as_bytes();

    let (query, ts_language) = match language {
        LanguageId::Rust => {
            let lang: Language = tree_sitter_rust::LANGUAGE.into();
            (rust_query(&lang), lang)
        }
        LanguageId::Python => {
            let lang: Language = tree_sitter_python::LANGUAGE.into();
            (python_query(&lang), lang)
        }
        LanguageId::JavaScript => {
            let lang: Language = tree_sitter_javascript::LANGUAGE.into();
            (js_query(&lang), lang)
        }
        LanguageId::TypeScript => {
            // `.tsx` uses the TSX grammar; the xref query MUST be compiled
            // against whichever grammar parsed the tree so node-kind IDs line up
            // (TS and TSX are distinct tree-sitter Languages with separate query
            // caches — see `tsx_query`).
            if is_tsx {
                let lang: Language = tree_sitter_typescript::LANGUAGE_TSX.into();
                (tsx_query(&lang), lang)
            } else {
                let lang: Language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
                (ts_query(&lang), lang)
            }
        }
        LanguageId::Go => {
            let lang: Language = tree_sitter_go::LANGUAGE.into();
            (go_query(&lang), lang)
        }
        LanguageId::Java => {
            let lang: Language = tree_sitter_java::LANGUAGE.into();
            (java_query(&lang), lang)
        }
        LanguageId::C => {
            let lang: Language = tree_sitter_c::LANGUAGE.into();
            (c_query(&lang), lang)
        }
        LanguageId::Cpp => {
            let lang: Language = tree_sitter_cpp::LANGUAGE.into();
            (cpp_query(&lang), lang)
        }
        LanguageId::CSharp => {
            let lang: Language = tree_sitter_c_sharp::LANGUAGE.into();
            (csharp_query(&lang), lang)
        }
        LanguageId::Ruby => {
            let lang: Language = tree_sitter_ruby::LANGUAGE.into();
            (ruby_query(&lang), lang)
        }
        LanguageId::Kotlin => {
            let lang: Language = tree_sitter_kotlin_sg::LANGUAGE.into();
            (kotlin_query(&lang), lang)
        }
        LanguageId::Dart => {
            let lang: Language = tree_sitter_dart::LANGUAGE.into();
            (dart_query(&lang), lang)
        }
        LanguageId::Elixir => {
            let lang: Language = tree_sitter_elixir::LANGUAGE.into();
            (elixir_query(&lang), lang)
        }
        LanguageId::Php => {
            let lang: Language = tree_sitter_php::LANGUAGE_PHP.into();
            (php_query(&lang), lang)
        }
        LanguageId::Swift => {
            let lang: Language = tree_sitter_swift::LANGUAGE.into();
            (swift_query(&lang), lang)
        }
        LanguageId::Perl => {
            let lang: Language = tree_sitter_perl::LANGUAGE.into();
            (perl_query(&lang), lang)
        }
        LanguageId::Json
        | LanguageId::Toml
        | LanguageId::Yaml
        | LanguageId::Markdown
        | LanguageId::Env => unreachable!("config types are handled before extract_references"),
        LanguageId::Html | LanguageId::Css | LanguageId::Scss => {
            return (vec![], HashMap::new());
        }
    };

    // Safety: the query and parse tree use the same grammar version because both are produced by
    // the same linked tree-sitter crate. The OnceLock ensures the query is compiled once and reused.
    // `ts_language` is reused below for the Rust const/static value-reference pass.
    let _ = &ts_language;

    let capture_names = query.capture_names();
    let mut cursor = QueryCursor::new();
    let mut references: Vec<ReferenceRecord> = Vec::new();
    let mut alias_map: HashMap<String, String> = HashMap::new();

    // StreamingIterator: use advance()/get() pattern instead of for..in
    let mut matches = cursor.matches(query, *root, source_bytes);
    while let Some(m) = {
        matches.advance();
        matches.get()
    } {
        let mut ref_call: Option<Node> = None;
        let mut ref_qualified_call: Option<Node> = None;
        let mut ref_method_call: Option<Node> = None;
        let mut ref_import: Option<Node> = None;
        let mut ref_type: Option<Node> = None;
        let mut ref_macro: Option<Node> = None;
        let mut import_original: Option<Node> = None;
        let mut import_alias: Option<Node> = None;
        let mut ref_implements: Option<Node> = None;
        let mut ref_implements_target: Option<Node> = None;

        for capture in m.captures {
            let name = capture_names[capture.index as usize];
            let node = capture.node;
            match name {
                "ref.call" => {
                    ref_call = Some(node);
                }
                "ref.qualified_call" => {
                    ref_qualified_call = Some(node);
                }
                "ref.method_call" => {
                    ref_method_call = Some(node);
                }
                "ref.import" => {
                    ref_import = Some(node);
                }
                "ref.type" => {
                    ref_type = Some(node);
                }
                "ref.macro" => {
                    ref_macro = Some(node);
                }
                "import.original" => {
                    import_original = Some(node);
                }
                "import.alias" => {
                    import_alias = Some(node);
                }
                "ref.implements" => {
                    ref_implements = Some(node);
                }
                "ref.implements_target" => {
                    ref_implements_target = Some(node);
                }
                _ => {}
            }
        }

        // Handle alias pair — build alias_map and skip generating a reference
        if let (Some(orig_node), Some(alias_node)) = (import_original, import_alias) {
            if let (Ok(orig_text), Ok(alias_text)) = (
                orig_node.utf8_text(source_bytes),
                alias_node.utf8_text(source_bytes),
            ) {
                // Extract the last segment of the original path (e.g. "HashMap" from "std::collections::HashMap")
                let orig_name = orig_text
                    .split("::")
                    .last()
                    .unwrap_or(orig_text)
                    .trim()
                    .to_string();
                let alias_str = alias_text.trim().to_string();
                alias_map.insert(alias_str, orig_name);
            }
            continue;
        }

        // Process ref.call (potentially with qualified_call overlay)
        if let Some(call_node) = ref_call
            && let Ok(name_text) = call_node.utf8_text(source_bytes)
        {
            let name = name_text.trim().to_string();
            let qualified_name = if let Some(qual_node) = ref_qualified_call {
                qual_node
                    .utf8_text(source_bytes)
                    .ok()
                    .map(|t| t.trim().to_string())
            } else {
                None
            };

            let start = call_node.start_position();
            let end = call_node.end_position();
            references.push(ReferenceRecord {
                name,
                qualified_name,
                kind: ReferenceKind::Call,
                byte_range: (call_node.start_byte() as u32, call_node.end_byte() as u32),
                line_range: (start.row as u32, end.row as u32),
                enclosing_symbol_index: None,
            });
        }

        // Method call
        if let Some(method_node) = ref_method_call
            && let Ok(name_text) = method_node.utf8_text(source_bytes)
        {
            let name = name_text.trim().to_string();
            let start = method_node.start_position();
            let end = method_node.end_position();
            references.push(ReferenceRecord {
                name,
                qualified_name: None,
                kind: ReferenceKind::Call,
                byte_range: (
                    method_node.start_byte() as u32,
                    method_node.end_byte() as u32,
                ),
                line_range: (start.row as u32, end.row as u32),
                enclosing_symbol_index: None,
            });
        }

        // Import
        if let Some(import_node) = ref_import
            && let Ok(name_text) = import_node.utf8_text(source_bytes)
            // Elixir guard: the import query `(call target: (identifier)
            // (arguments (alias) @ref.import))` cannot distinguish a real
            // directive (`alias`/`import`/`use`/`require`) from any other call
            // that takes a module as its first argument (`raise ArgumentError`,
            // `socket "...", UserSocket`). Tree-sitter text predicates are NOT
            // applied by `QueryCursor::matches`, so constrain it here: only keep
            // the import when the enclosing call's target is a directive keyword.
            && (*language != LanguageId::Elixir
                || elixir_import_target_is_directive(import_node, source_bytes))
        {
            let import_texts = match language {
                LanguageId::Rust => expand_rust_import_paths(name_text),
                _ => vec![name_text.trim().to_string()],
            };

            for import_text in import_texts {
                push_import_reference(&mut references, &import_text, import_node);
            }
        }

        // Type reference
        if let Some(type_node) = ref_type
            && let Ok(name_text) = type_node.utf8_text(source_bytes)
        {
            let name = name_text.trim().to_string();
            if !name.is_empty() {
                let qualified_name = python_attribute_qualified_name(type_node, source_bytes);
                let start = type_node.start_position();
                let end = type_node.end_position();
                references.push(ReferenceRecord {
                    name,
                    qualified_name,
                    kind: ReferenceKind::TypeUsage,
                    byte_range: (type_node.start_byte() as u32, type_node.end_byte() as u32),
                    line_range: (start.row as u32, end.row as u32),
                    enclosing_symbol_index: None,
                });
            }
        }

        // Macro use
        if let Some(macro_node) = ref_macro
            && let Ok(name_text) = macro_node.utf8_text(source_bytes)
        {
            let name = name_text.trim().to_string();
            if !name.is_empty() {
                let start = macro_node.start_position();
                let end = macro_node.end_position();
                references.push(ReferenceRecord {
                    name,
                    qualified_name: None,
                    kind: ReferenceKind::MacroUse,
                    byte_range: (macro_node.start_byte() as u32, macro_node.end_byte() as u32),
                    line_range: (start.row as u32, end.row as u32),
                    enclosing_symbol_index: None,
                });
            }
        }

        // Implements / inherits relationship
        if let Some(impl_node) = ref_implements
            && let Ok(impl_text) = impl_node.utf8_text(source_bytes)
        {
            let trait_name = impl_text.trim().to_string();
            if !trait_name.is_empty() {
                let qualified_name = if let Some(target_node) = ref_implements_target {
                    target_node
                        .utf8_text(source_bytes)
                        .ok()
                        .map(|text| text.trim().to_string())
                        .filter(|implementor| !implementor.is_empty())
                } else {
                    python_attribute_qualified_name(impl_node, source_bytes)
                };

                let start = impl_node.start_position();
                let end = impl_node.end_position();
                references.push(ReferenceRecord {
                    name: trait_name,
                    qualified_name,
                    kind: ReferenceKind::Implements,
                    byte_range: (impl_node.start_byte() as u32, impl_node.end_byte() as u32),
                    line_range: (start.row as u32, end.row as u32),
                    enclosing_symbol_index: None,
                });
            }
        }
    }

    // --- Rust macro-body fallback ---
    //
    // tree-sitter parses Rust macro bodies as `token_tree` (raw tokens), so
    // `call_expression` / `scoped_identifier` queries never fire inside macros
    // like `format!(... crate::hash::digest_hex(...))`.  Fall back to a text
    // scan that recovers two shapes whose byte positions are not already
    // covered by a tree-sitter capture:
    //   * qualified calls (`a::b::name(`) — recovered file-wide, since the `::`
    //     prefix makes a false positive vanishingly unlikely; and
    //   * plain calls (`name(`) — recovered ONLY inside a macro invocation's
    //     token tree, where whole-file scanning would be too imprecise
    //     (it would collide with definitions and ordinary already-captured calls).
    if *language == LanguageId::Rust {
        let captured_call_ranges: Vec<(u32, u32)> = references
            .iter()
            .filter(|r| r.kind == ReferenceKind::Call)
            .map(|r| r.byte_range)
            .collect();

        // Byte ranges of macro invocations, used to scope plain-call recovery.
        let macro_ranges = collect_rust_macro_ranges(root);

        // Scan for `ident(` patterns; classify each as qualified (file-wide) or
        // plain (macro-scoped) below.
        let bytes = source.as_bytes();
        let len = bytes.len();
        let mut i = 0;
        while i < len {
            // Quick check: find `(` which may terminate a qualified call.
            if bytes[i] != b'(' {
                i += 1;
                continue;
            }
            let paren_pos = i;

            // Walk backwards over optional whitespace before `(`.
            let mut j = paren_pos;
            while j > 0 && matches!(bytes[j - 1], b' ' | b'\t') {
                j -= 1;
            }
            let name_end = j;

            // Walk backwards over the trailing identifier (the function name).
            while j > 0 && (bytes[j - 1].is_ascii_alphanumeric() || bytes[j - 1] == b'_') {
                j -= 1;
            }
            let name_start = j;
            if name_start == name_end {
                i += 1;
                continue;
            }

            // Plain (unqualified) call: not preceded by `::`. Recover it ONLY
            // when the call sits inside a macro token tree — elsewhere the main
            // tree-sitter query already captured every real call, and a naked
            // `ident(` file-wide would also match definitions (`fn name(`).
            if j < 2 || bytes[j - 1] != b':' || bytes[j - 2] != b':' {
                let byte_start = name_start as u32;
                let byte_end = name_end as u32;

                // Only inside a macro invocation.
                let in_macro = macro_ranges
                    .iter()
                    .any(|&(ms, me)| ms <= byte_start && byte_end <= me);
                // Skip the macro name itself (`format` in `format!(`): the token
                // immediately before is `!`, and it is already a MacroUse ref.
                let is_macro_name = name_end < bytes.len() && bytes[name_end] == b'!';
                // Skip if already captured by a tree-sitter query match.
                let already = captured_call_ranges.iter().any(|&(cs, ce)| {
                    (cs <= byte_start && byte_end <= ce) || (byte_start <= cs && cs < byte_end)
                });
                // Rough string-literal guard: even count of `"` before the name.
                // ponytail: quote-parity heuristic, not a lexer — precision-favoring
                // (matches the qualified-call fallback above; a name after an odd
                // number of earlier quotes is skipped rather than risk a phantom
                // ref). Upgrade to a real tokenizer only if a macro-heavy fixture
                // shows a real miss. Known limitation logged in the verify-tools harness.
                let outside_string = source[..name_start].matches('"').count().is_multiple_of(2);

                if in_macro && !is_macro_name && !already && outside_string {
                    let name_text = &source[name_start..name_end];
                    if !name_text.is_empty() {
                        let line =
                            bytes[..name_start].iter().filter(|&&b| b == b'\n').count() as u32;
                        references.push(ReferenceRecord {
                            name: name_text.to_string(),
                            qualified_name: None,
                            kind: ReferenceKind::Call,
                            byte_range: (byte_start, byte_end),
                            line_range: (line, line),
                            enclosing_symbol_index: None,
                        });
                    }
                }

                i += 1;
                continue;
            }

            // Walk further back collecting `segment::` prefixes.
            let mut path_start = j - 2;
            loop {
                let seg_end = path_start;
                while path_start > 0
                    && (bytes[path_start - 1].is_ascii_alphanumeric()
                        || bytes[path_start - 1] == b'_')
                {
                    path_start -= 1;
                }
                if path_start == seg_end {
                    // No identifier before `::` — stop.
                    path_start = seg_end;
                    break;
                }
                // Another `::` before this segment?
                if path_start >= 2 && bytes[path_start - 1] == b':' && bytes[path_start - 2] == b':'
                {
                    path_start -= 2;
                } else {
                    break;
                }
            }

            let qualified_text = &source[path_start..name_end];
            let name_text = &source[name_start..name_end];

            // Must have at least one `::` (two segments).
            if !qualified_text.contains("::")
                || name_text.is_empty()
                // Skip if inside a string literal (very rough: odd number of `"` before).
                || !source[..path_start].matches('"').count().is_multiple_of(2)
            {
                i += 1;
                continue;
            }

            let byte_start = name_start as u32;
            let byte_end = name_end as u32;

            // Skip if already captured by a tree-sitter query match.
            let already = captured_call_ranges.iter().any(|&(cs, ce)| {
                (cs <= byte_start && byte_end <= ce) || (byte_start <= cs && cs < byte_end)
            });

            if !already {
                let line = bytes[..path_start].iter().filter(|&&b| b == b'\n').count() as u32;
                references.push(ReferenceRecord {
                    name: name_text.to_string(),
                    qualified_name: Some(qualified_text.to_string()),
                    kind: ReferenceKind::Call,
                    byte_range: (byte_start, byte_end),
                    line_range: (line, line),
                    enclosing_symbol_index: None,
                });
            }

            i += 1;
        }
    }

    // --- Rust const/static value-position references ---
    //
    // The main query never captures a `const`/`static` used as a bare value
    // (iterated in a `for` loop, passed to `.contains(..)`, handed as a plain
    // argument), so resolve those here against same-file const/static defs.
    // Strict precision gating lives in `extract_rust_value_refs`.
    if *language == LanguageId::Rust {
        let value_refs = extract_rust_value_refs(root, source, &ts_language, &references);
        references.extend(value_refs);
    }

    if *language == LanguageId::Python {
        let value_refs = extract_python_value_refs(root, source, &ts_language, &references);
        references.extend(value_refs);
        let string_refs = extract_python_string_type_refs(root, source, &ts_language, &references);
        references.extend(string_refs);
    }

    (references, alias_map)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Parser;

    fn parse_and_extract(
        source: &str,
        language: LanguageId,
    ) -> (Vec<ReferenceRecord>, HashMap<String, String>) {
        parse_and_extract_flavored(source, language, false)
    }

    /// Like [`parse_and_extract`] but selects the TSX grammar when `is_tsx`.
    fn parse_and_extract_flavored(
        source: &str,
        language: LanguageId,
        is_tsx: bool,
    ) -> (Vec<ReferenceRecord>, HashMap<String, String>) {
        let mut parser = Parser::new();
        let ts_language: Language = match &language {
            LanguageId::Rust => tree_sitter_rust::LANGUAGE.into(),
            LanguageId::Python => tree_sitter_python::LANGUAGE.into(),
            LanguageId::JavaScript => tree_sitter_javascript::LANGUAGE.into(),
            LanguageId::TypeScript if is_tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
            LanguageId::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            LanguageId::Go => tree_sitter_go::LANGUAGE.into(),
            LanguageId::Java => tree_sitter_java::LANGUAGE.into(),
            LanguageId::C => tree_sitter_c::LANGUAGE.into(),
            LanguageId::Cpp => tree_sitter_cpp::LANGUAGE.into(),
            LanguageId::CSharp => tree_sitter_c_sharp::LANGUAGE.into(),
            LanguageId::Ruby => tree_sitter_ruby::LANGUAGE.into(),
            LanguageId::Kotlin => tree_sitter_kotlin_sg::LANGUAGE.into(),
            LanguageId::Dart => tree_sitter_dart::LANGUAGE.into(),
            LanguageId::Elixir => tree_sitter_elixir::LANGUAGE.into(),
            LanguageId::Php => tree_sitter_php::LANGUAGE_PHP.into(),
            LanguageId::Swift => tree_sitter_swift::LANGUAGE.into(),
            LanguageId::Perl => tree_sitter_perl::LANGUAGE.into(),
            LanguageId::Json
            | LanguageId::Toml
            | LanguageId::Yaml
            | LanguageId::Markdown
            | LanguageId::Env
            | LanguageId::Html
            | LanguageId::Css
            | LanguageId::Scss => {
                unreachable!("config/frontend languages don't use tree-sitter xref extraction")
            }
        };
        parser.set_language(&ts_language).expect("set language");
        let tree = parser.parse(source, None).expect("parse");
        let root = tree.root_node();
        extract_references(&root, source, &language, is_tsx)
    }

    fn find_ref<'a>(refs: &'a [ReferenceRecord], name: &str) -> Option<&'a ReferenceRecord> {
        refs.iter().find(|r| r.name == name)
    }

    fn has_ref(refs: &[ReferenceRecord], name: &str, kind: ReferenceKind) -> bool {
        refs.iter().any(|r| r.name == name && r.kind == kind)
    }

    // --- Rust ---

    #[test]
    fn test_rust_call_expression_simple() {
        let (refs, _) = parse_and_extract("fn main() { foo(); }", LanguageId::Rust);
        assert!(
            has_ref(&refs, "foo", ReferenceKind::Call),
            "refs: {:?}",
            refs
        );
    }

    #[test]
    fn test_rust_scoped_identifier_qualified_call() {
        let (refs, _) = parse_and_extract("fn main() { Vec::new(); }", LanguageId::Rust);
        let r = find_ref(&refs, "new");
        assert!(r.is_some(), "should find 'new' call, refs: {:?}", refs);
        let r = r.unwrap();
        assert_eq!(r.kind, ReferenceKind::Call);
        assert!(r.qualified_name.is_some(), "should have qualified_name");
        let qname = r.qualified_name.as_deref().unwrap();
        assert!(
            qname.contains("Vec"),
            "qualified_name should contain Vec, got: {qname}"
        );
    }

    #[test]
    fn test_rust_method_call() {
        let (refs, _) = parse_and_extract("fn main() { self.items.push(x); }", LanguageId::Rust);
        assert!(
            has_ref(&refs, "push", ReferenceKind::Call),
            "refs: {:?}",
            refs
        );
    }

    #[test]
    fn test_rust_macro_invocation() {
        let (refs, _) = parse_and_extract(r#"fn main() { println!("hi"); }"#, LanguageId::Rust);
        assert!(
            has_ref(&refs, "println", ReferenceKind::MacroUse),
            "refs: {:?}",
            refs
        );
    }

    #[test]
    fn test_rust_qualified_call_inside_macro_body() {
        // tree-sitter parses macro bodies as token_tree (raw tokens), so the
        // call_expression query never fires.  The text-based fallback should
        // still capture the qualified call.
        let src = r#"fn project_key() -> String {
    format!("project-{}", crate::hash::digest_hex(b"abc"))
}"#;
        let (refs, _) = parse_and_extract(src, LanguageId::Rust);
        let call = refs
            .iter()
            .find(|r| r.name == "digest_hex" && r.kind == ReferenceKind::Call);
        assert!(
            call.is_some(),
            "should find digest_hex call inside macro body, refs: {:?}",
            refs
        );
        let call = call.unwrap();
        assert_eq!(
            call.qualified_name.as_deref(),
            Some("crate::hash::digest_hex"),
            "should have full qualified name"
        );
    }

    #[test]
    fn test_rust_qualified_call_outside_macro_not_duplicated() {
        // A normal qualified call should be captured by tree-sitter AND the
        // fallback should not add a duplicate.
        let src = "fn main() { crate::hash::digest_hex(b\"abc\"); }";
        let (refs, _) = parse_and_extract(src, LanguageId::Rust);
        let calls: Vec<_> = refs
            .iter()
            .filter(|r| r.name == "digest_hex" && r.kind == ReferenceKind::Call)
            .collect();
        assert_eq!(
            calls.len(),
            1,
            "should have exactly one Call ref (no duplicates), got: {:?}",
            calls
        );
    }

    #[test]
    fn test_rust_plain_call_inside_macro_body() {
        // tree-sitter parses macro bodies as token_tree (raw tokens), so a PLAIN
        // (unqualified) call inside a macro like `format!("{}", tally(xs))` never
        // fires the call_expression query.  The macro-body fallback must recover it
        // even without a `::` qualifier, or renames leave dangling call sites.
        let src = r#"fn describe(xs: &[i64]) -> String {
        format!("total={}", tally(xs))
    }"#;
        let (refs, _) = parse_and_extract(src, LanguageId::Rust);
        let call = refs
            .iter()
            .find(|r| r.name == "tally" && r.kind == ReferenceKind::Call);
        assert!(
            call.is_some(),
            "should find plain `tally` call inside macro body, refs: {:?}",
            refs
        );
        // A plain (unqualified) call carries no qualified name.
        assert_eq!(call.unwrap().qualified_name, None);
    }

    #[test]
    fn test_rust_plain_call_inside_macro_not_duplicated() {
        // A normal call captured by tree-sitter must NOT be duplicated by the
        // macro-body plain-call fallback.
        let src = "fn main() { tally(); }";
        let (refs, _) = parse_and_extract(src, LanguageId::Rust);
        let calls: Vec<_> = refs
            .iter()
            .filter(|r| r.name == "tally" && r.kind == ReferenceKind::Call)
            .collect();
        assert_eq!(
            calls.len(),
            1,
            "should have exactly one Call ref (no duplicates), got: {:?}",
            calls
        );
    }

    #[test]
    fn test_rust_use_declaration_import() {
        let (refs, _) = parse_and_extract("use std::collections::HashMap;", LanguageId::Rust);
        // The import captures scoped_identifier for the whole use path
        assert!(
            refs.iter().any(|r| r.kind == ReferenceKind::Import),
            "should have at least one Import ref, refs: {:?}",
            refs
        );
    }

    #[test]
    fn test_rust_grouped_use_declaration_expands_imports() {
        let (refs, _) = parse_and_extract("use crate::{daemon, other};", LanguageId::Rust);
        assert!(
            refs.iter().any(|r| {
                r.kind == ReferenceKind::Import
                    && r.name == "daemon"
                    && r.qualified_name.as_deref() == Some("crate::daemon")
            }),
            "grouped Rust imports should include crate::daemon, refs: {:?}",
            refs
        );
        assert!(
            refs.iter().any(|r| {
                r.kind == ReferenceKind::Import
                    && r.name == "other"
                    && r.qualified_name.as_deref() == Some("crate::other")
            }),
            "grouped Rust imports should include crate::other, refs: {:?}",
            refs
        );
    }

    #[test]
    fn test_rust_use_as_clause_populates_alias_map() {
        let source = "use std::collections::HashMap as Map;";
        let (_, alias_map) = parse_and_extract(source, LanguageId::Rust);
        assert!(
            alias_map.contains_key("Map"),
            "alias_map should contain 'Map', got: {:?}",
            alias_map
        );
        assert_eq!(alias_map.get("Map").map(|s| s.as_str()), Some("HashMap"));
    }

    #[test]
    fn test_rust_type_identifier() {
        let (refs, _) = parse_and_extract("fn foo(param: MyStruct) {}", LanguageId::Rust);
        assert!(
            has_ref(&refs, "MyStruct", ReferenceKind::TypeUsage),
            "refs: {:?}",
            refs
        );
    }

    /// D13 recall: a qualified-call construction site `MinimalFilter::new()` is
    /// keyed in `reverse_index` under the leaf `new`, but retains the full
    /// `qualified_name` (`MinimalFilter::new`). This asserts the extraction shape
    /// the `find_references` head-match branch relies on. The struct-literal
    /// `MinimalFilter { .. }` is ALREADY captured here as a `TypeUsage` keyed
    /// under the head via the unscoped `(type_identifier)` rule -- so NO extra
    /// struct_expression capture is needed (it would only duplicate).
    #[test]
    fn test_rust_qualified_call_retains_head_and_struct_literal_keyed_under_head() {
        let src = r#"
struct MinimalFilter { a: u8 }
fn use_it(p: MinimalFilter) {
    let a: MinimalFilter = MinimalFilter::new();
    let b = MinimalFilter { a: 1 };
    let _ = (a, b, p);
}
"#;
        let (refs, _) = parse_and_extract(src, LanguageId::Rust);

        // The constructor call is keyed under the leaf, with the head retained.
        let ctor = refs
            .iter()
            .find(|r| r.name == "new" && r.kind == ReferenceKind::Call)
            .expect("MinimalFilter::new() should be captured as a Call");
        assert_eq!(
            ctor.qualified_name.as_deref(),
            Some("MinimalFilter::new"),
            "constructor call must retain the type head in qualified_name"
        );

        // The struct literal IS already a head-keyed TypeUsage (no extra capture).
        let literal_count = refs
            .iter()
            .filter(|r| {
                r.name == "MinimalFilter"
                    && r.kind == ReferenceKind::TypeUsage
                    && r.line_range.0 == 4
            })
            .count();
        assert_eq!(
            literal_count, 1,
            "struct literal MinimalFilter {{..}} on line 4 must be captured exactly once under the head, got refs: {:?}",
            refs
        );
    }

    #[test]
    fn test_rust_impl_trait_for_struct() {
        let source = r#"
            struct MyStruct;
            trait Display {}
            impl Display for MyStruct {}
        "#;
        let (refs, _) = parse_and_extract(source, LanguageId::Rust);
        let impl_refs: Vec<_> = refs
            .iter()
            .filter(|r| r.kind == ReferenceKind::Implements)
            .collect();
        assert_eq!(
            impl_refs.len(),
            1,
            "should find 1 implements ref, got: {:?}",
            impl_refs
        );
        assert_eq!(impl_refs[0].name, "Display");
        assert_eq!(impl_refs[0].qualified_name.as_deref(), Some("MyStruct"));
    }

    #[test]
    fn test_rust_impl_scoped_trait_for_struct() {
        let source = r#"
            struct Foo;
            impl std::fmt::Display for Foo {
                fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { Ok(()) }
            }
        "#;
        let (refs, _) = parse_and_extract(source, LanguageId::Rust);
        let impl_refs: Vec<_> = refs
            .iter()
            .filter(|r| r.kind == ReferenceKind::Implements)
            .collect();
        assert_eq!(
            impl_refs.len(),
            1,
            "should find 1 implements ref, got: {:?}",
            impl_refs
        );
        assert!(
            impl_refs[0].name.contains("Display"),
            "trait name should contain Display, got: {}",
            impl_refs[0].name
        );
        assert_eq!(impl_refs[0].qualified_name.as_deref(), Some("Foo"));
    }

    // --- Rust const/static value-position references (issue #257) ---

    #[test]
    fn test_rust_const_used_in_for_loop_and_contains() {
        // SYMFORGE_TOOL_NAMES-style: a const iterated in `for ... in CONST` and
        // queried via `CONST.contains(..)` must both surface as ValueUse refs.
        let source = r#"
const TOOL_NAMES: &[&str] = &["a", "b"];

fn iterate() {
    for name in TOOL_NAMES {
        let _ = name;
    }
}

fn membership(candidate: &str) -> bool {
    TOOL_NAMES.contains(&candidate)
}
"#;
        let (refs, _) = parse_and_extract(source, LanguageId::Rust);
        let value_refs: Vec<_> = refs
            .iter()
            .filter(|r| r.name == "TOOL_NAMES" && r.kind == ReferenceKind::ValueUse)
            .collect();
        assert_eq!(
            value_refs.len(),
            2,
            "for-loop use and .contains() use should both surface as ValueUse, refs: {:?}",
            refs
        );
        // The definition site itself must NOT be reported as a reference.
        assert!(
            !refs
                .iter()
                .any(|r| r.name == "TOOL_NAMES" && r.line_range.0 == 1),
            "the const definition site must not be a reference, refs: {:?}",
            refs
        );
    }

    #[test]
    fn test_rust_static_used_as_bare_value_arg() {
        // A `static` handed to a function as a bare value argument must surface.
        let source = r#"
static MAX_RETRIES: u32 = 5;

fn configure(limit: u32) {
    let _ = limit;
}

fn run() {
    configure(MAX_RETRIES);
}
"#;
        let (refs, _) = parse_and_extract(source, LanguageId::Rust);
        assert!(
            has_ref(&refs, "MAX_RETRIES", ReferenceKind::ValueUse),
            "bare value-arg use of a static should surface as ValueUse, refs: {:?}",
            refs
        );
    }

    #[test]
    fn test_rust_value_ref_does_not_report_local_var_named_like_const() {
        // False-positive guard: a lowercase local `let foo`/parameter `foo`
        // must NOT be reported as a reference to an unrelated `const FOO`.
        // Case discipline + exact same-file def-name matching makes this safe.
        let source = r#"
const FOO: u32 = 1;

fn unrelated(foo: u32) -> u32 {
    let bar = foo + 1;
    bar
}
"#;
        let (refs, _) = parse_and_extract(source, LanguageId::Rust);
        // The lowercase `foo` (param + uses) must produce zero ValueUse refs.
        assert!(
            !refs
                .iter()
                .any(|r| r.name == "foo" && r.kind == ReferenceKind::ValueUse),
            "lowercase local `foo` must not be a reference to const FOO, refs: {:?}",
            refs
        );
        // And FOO is never used, so it should produce no ValueUse refs at all.
        assert!(
            !refs
                .iter()
                .any(|r| r.name == "FOO" && r.kind == ReferenceKind::ValueUse),
            "unused const FOO should have no value references, refs: {:?}",
            refs
        );
    }

    #[test]
    fn test_rust_value_ref_requires_same_file_definition() {
        // Precision guard: a SCREAMING_SNAKE_CASE identifier that is NOT defined
        // as a const/static in this file must not be reported (no whole-repo
        // symbol table at extraction time — we resolve same-file only).
        let source = r#"
fn run() {
    let _ = EXTERNAL_CONST;
}
"#;
        let (refs, _) = parse_and_extract(source, LanguageId::Rust);
        assert!(
            !refs.iter().any(|r| r.kind == ReferenceKind::ValueUse),
            "an identifier with no same-file const/static def must not surface as ValueUse, refs: {:?}",
            refs
        );
    }

    #[test]
    fn test_rust_value_ref_not_duplicated_with_call_capture() {
        // Dedup guard: if a const-named identifier is *also* captured by the
        // main query (here a function call shares the byte range of nothing,
        // but a same-named call must not collide), we must not double-count.
        // Use a const whose name is also referenced once as a bare value — it
        // should appear exactly once as ValueUse, never as Call.
        let source = r#"
const LIMIT: u32 = 10;

fn check(v: u32) -> bool {
    v < LIMIT
}
"#;
        let (refs, _) = parse_and_extract(source, LanguageId::Rust);
        let limit_refs: Vec<_> = refs.iter().filter(|r| r.name == "LIMIT").collect();
        assert_eq!(
            limit_refs.len(),
            1,
            "LIMIT should be referenced exactly once, refs: {:?}",
            refs
        );
        assert_eq!(
            limit_refs[0].kind,
            ReferenceKind::ValueUse,
            "the LIMIT value use should be kind ValueUse, refs: {:?}",
            refs
        );
    }

    #[test]
    fn test_rust_existing_call_and_type_refs_unchanged_with_value_pass() {
        // Regression: the value pass must not perturb call/type/import capture.
        let source = r#"
use std::collections::HashMap;

const SIZE: usize = 4;

fn build(map: HashMap<String, usize>) {
    let _ = map;
    helper(SIZE);
}

fn helper(n: usize) {
    let _ = n;
}
"#;
        let (refs, _) = parse_and_extract(source, LanguageId::Rust);
        assert!(
            has_ref(&refs, "helper", ReferenceKind::Call),
            "call ref to helper must remain, refs: {:?}",
            refs
        );
        assert!(
            has_ref(&refs, "HashMap", ReferenceKind::TypeUsage),
            "type ref to HashMap must remain, refs: {:?}",
            refs
        );
        assert!(
            refs.iter().any(|r| r.kind == ReferenceKind::Import),
            "import ref must remain, refs: {:?}",
            refs
        );
        assert!(
            has_ref(&refs, "SIZE", ReferenceKind::ValueUse),
            "const SIZE used as a bare arg should surface as ValueUse, refs: {:?}",
            refs
        );
    }

    #[test]
    fn test_python_class_inheritance() {
        let source = "class Dog(Animal):\n    pass";
        let (refs, _) = parse_and_extract(source, LanguageId::Python);
        let impl_refs: Vec<_> = refs
            .iter()
            .filter(|r| r.kind == ReferenceKind::Implements)
            .collect();
        assert_eq!(
            impl_refs.len(),
            1,
            "should find 1 implements ref, got: {:?}",
            impl_refs
        );
        assert_eq!(impl_refs[0].name, "Animal");
        assert_eq!(impl_refs[0].qualified_name.as_deref(), Some("Dog"));
    }

    #[test]
    fn test_ts_class_implements() {
        let source = "class Foo implements Bar {}";
        let (refs, _) = parse_and_extract(source, LanguageId::TypeScript);
        let impl_refs: Vec<_> = refs
            .iter()
            .filter(|r| r.kind == ReferenceKind::Implements)
            .collect();
        assert!(
            impl_refs
                .iter()
                .any(|r| r.name == "Bar" && r.qualified_name.as_deref() == Some("Foo")),
            "should find Foo implements Bar, got: {:?}",
            impl_refs
        );
    }

    #[test]
    fn test_csharp_class_base_list() {
        let source = "class MyService : IService {}";
        let (refs, _) = parse_and_extract(source, LanguageId::CSharp);
        let impl_refs: Vec<_> = refs
            .iter()
            .filter(|r| r.kind == ReferenceKind::Implements)
            .collect();
        assert!(
            impl_refs
                .iter()
                .any(|r| r.name == "IService" && r.qualified_name.as_deref() == Some("MyService")),
            "should find MyService : IService, got: {:?}",
            impl_refs
        );
    }

    #[test]
    fn test_java_class_implements() {
        let source = "class Foo implements Runnable {}";
        let (refs, _) = parse_and_extract(source, LanguageId::Java);
        let impl_refs: Vec<_> = refs
            .iter()
            .filter(|r| r.kind == ReferenceKind::Implements)
            .collect();
        assert!(
            impl_refs
                .iter()
                .any(|r| r.name == "Runnable" && r.qualified_name.as_deref() == Some("Foo")),
            "should find Foo implements Runnable, got: {:?}",
            impl_refs
        );
    }

    // --- Python ---

    #[test]
    fn test_python_function_call() {
        let (refs, _) = parse_and_extract("process(data)", LanguageId::Python);
        assert!(
            has_ref(&refs, "process", ReferenceKind::Call),
            "refs: {:?}",
            refs
        );
    }

    #[test]
    fn test_python_import_statement() {
        let (refs, _) = parse_and_extract("import os", LanguageId::Python);
        assert!(
            refs.iter().any(|r| r.kind == ReferenceKind::Import),
            "should have Import ref, refs: {:?}",
            refs
        );
    }

    #[test]
    fn test_python_from_import() {
        let (refs, _) = parse_and_extract("from os import path", LanguageId::Python);
        assert!(
            refs.iter().any(|r| r.kind == ReferenceKind::Import),
            "should have Import ref, refs: {:?}",
            refs
        );
    }

    #[test]
    fn test_python_type_annotation() {
        let (refs, _) = parse_and_extract("def foo(x: MyType): pass", LanguageId::Python);
        assert!(
            has_ref(&refs, "MyType", ReferenceKind::TypeUsage),
            "refs: {:?}",
            refs
        );
    }

    #[test]
    fn test_python_models_model_class_inheritance() {
        let (refs, _) = parse_and_extract(
            "class Permission(models.Model):\n    pass",
            LanguageId::Python,
        );
        let model_ref = refs.iter().find(|r| r.name == "Model");
        assert!(
            model_ref.is_some(),
            "models.Model base should surface Model ref, refs: {:?}",
            refs
        );
        assert_eq!(
            model_ref.unwrap().qualified_name.as_deref(),
            Some("models.Model")
        );
        assert_eq!(model_ref.unwrap().kind, ReferenceKind::Implements);
    }

    #[test]
    fn test_python_subscript_type_annotation() {
        let (refs, _) = parse_and_extract(
            "def f(qs: QuerySet[Model]) -> None:\n    pass",
            LanguageId::Python,
        );
        assert!(
            has_ref(&refs, "QuerySet", ReferenceKind::TypeUsage),
            "refs: {:?}",
            refs
        );
        assert!(
            has_ref(&refs, "Model", ReferenceKind::TypeUsage),
            "refs: {:?}",
            refs
        );
    }

    #[test]
    fn test_python_value_type_in_call_args() {
        let (refs, _) = parse_and_extract(
            "def g():\n    return isinstance(x, Model)",
            LanguageId::Python,
        );
        assert!(
            has_ref(&refs, "Model", ReferenceKind::ValueUse),
            "refs: {:?}",
            refs
        );
    }

    #[test]
    fn test_python_foreign_key_model_arg() {
        let (refs, _) = parse_and_extract(
            "field = models.ForeignKey(Model, on_delete=CASCADE)",
            LanguageId::Python,
        );
        assert!(
            has_ref(&refs, "Model", ReferenceKind::ValueUse),
            "refs: {:?}",
            refs
        );
    }

    #[test]
    fn test_python_model_attribute_chain() {
        let (refs, _) = parse_and_extract("__hash__ = models.Model.__hash__", LanguageId::Python);
        assert!(
            has_ref(&refs, "Model", ReferenceKind::TypeUsage),
            "refs: {:?}",
            refs
        );
    }

    #[test]
    fn test_python_string_model_migration_arg() {
        let (refs, _) = parse_and_extract(
            "migrations.RenameField(\"Model\", \"old\", \"new\")",
            LanguageId::Python,
        );
        assert!(
            has_ref(&refs, "Model", ReferenceKind::ValueUse),
            "refs: {:?}",
            refs
        );
    }

    #[test]
    fn test_python_mock_patch_dotted_model_string() {
        let (refs, _) =
            parse_and_extract("mock.patch(\"django.db.models.Model\")", LanguageId::Python);
        let model_ref = refs.iter().find(|r| r.name == "Model");
        assert!(
            model_ref.is_some(),
            "mock.patch dotted path should surface Model, refs: {:?}",
            refs
        );
        assert_eq!(
            model_ref.unwrap().qualified_name.as_deref(),
            Some("django.db.models.Model")
        );
    }

    // --- JavaScript ---

    #[test]
    fn test_js_call_expression() {
        let (refs, _) = parse_and_extract("fetch(url);", LanguageId::JavaScript);
        assert!(
            has_ref(&refs, "fetch", ReferenceKind::Call),
            "refs: {:?}",
            refs
        );
    }

    #[test]
    fn test_js_import_statement() {
        let (refs, _) = parse_and_extract("import React from 'react';", LanguageId::JavaScript);
        assert!(
            refs.iter().any(|r| r.kind == ReferenceKind::Import),
            "should have Import ref, refs: {:?}",
            refs
        );
    }

    #[test]
    fn test_jsx_import_extracted_under_plain_js_grammar() {
        // Regression: `.jsx` resolves to LanguageId::JavaScript and is NOT a TSX
        // path, so it runs through the plain (non-flavored) JavaScript grammar.
        // tree-sitter-javascript 0.25 parses JSX natively — unlike `.ts`, the JSX
        // here is NOT a partial parse — so the import reference for the component
        // rendered in the fragment must still be captured.
        let source = "import { Row } from './Row';\n\
export const Table = ({ rows }) => (\n\
  <>{rows.map((r) => <Row key={r.id} value={r.value} />)}</>\n\
);\n";
        let (refs, _) = parse_and_extract(source, LanguageId::JavaScript);
        assert!(
            refs.iter().any(|r| r.kind == ReferenceKind::Import),
            "JSX under the plain JavaScript grammar must still capture the import; refs: {:?}",
            refs
        );
    }

    // --- TypeScript ---

    #[test]
    fn test_ts_type_references() {
        let source = "interface Foo { bar: MyInterface; }";
        let (refs, _) = parse_and_extract(source, LanguageId::TypeScript);
        assert!(
            refs.iter().any(|r| r.kind == ReferenceKind::TypeUsage),
            "should have TypeUsage ref, refs: {:?}",
            refs
        );
    }

    #[test]
    fn test_tsx_references_extracted_under_tsx_grammar() {
        // JSX only parses under the TSX grammar. With the plain TypeScript
        // grammar this source is a partial parse and the import reference is
        // lost; the TSX-flavored extraction recovers it.
        let source = "import { App } from './App';\nexport const root = () => <App />;\n";
        let (refs, _) = parse_and_extract_flavored(source, LanguageId::TypeScript, true);
        assert!(
            refs.iter().any(|r| r.kind == ReferenceKind::Import),
            "TSX import must be captured under the TSX grammar; refs: {:?}",
            refs
        );
    }

    // --- Go ---

    #[test]
    fn test_go_call_expression() {
        let source = "package main\nimport \"fmt\"\nfunc main() { fmt.Println(\"hi\") }";
        let (refs, _) = parse_and_extract(source, LanguageId::Go);
        assert!(
            has_ref(&refs, "Println", ReferenceKind::Call),
            "refs: {:?}",
            refs
        );
    }

    #[test]
    fn test_go_import() {
        let source = "package main\nimport \"fmt\"";
        let (refs, _) = parse_and_extract(source, LanguageId::Go);
        assert!(
            refs.iter().any(|r| r.kind == ReferenceKind::Import),
            "should have Import ref, refs: {:?}",
            refs
        );
    }

    // --- Java ---

    #[test]
    fn test_java_method_invocation() {
        let source = "class A { void f() { System.out.println(\"hi\"); } }";
        let (refs, _) = parse_and_extract(source, LanguageId::Java);
        assert!(
            has_ref(&refs, "println", ReferenceKind::Call),
            "refs: {:?}",
            refs
        );
    }

    #[test]
    fn test_java_import() {
        let source = "import java.util.ArrayList;";
        let (refs, _) = parse_and_extract(source, LanguageId::Java);
        assert!(
            refs.iter().any(|r| r.kind == ReferenceKind::Import),
            "should have Import ref, refs: {:?}",
            refs
        );
    }

    // --- C ---

    #[test]
    fn test_c_call_ref() {
        let source = "void foo() { bar(); }";
        let (refs, _) = parse_and_extract(source, LanguageId::C);
        assert!(
            has_ref(&refs, "bar", ReferenceKind::Call),
            "refs: {:?}",
            refs
        );
    }

    #[test]
    fn test_c_include_ref() {
        let source = "#include <stdio.h>\nvoid foo() {}";
        let (refs, _) = parse_and_extract(source, LanguageId::C);
        assert!(
            refs.iter().any(|r| r.kind == ReferenceKind::Import),
            "should have Import ref for #include, refs: {:?}",
            refs
        );
    }

    #[test]
    fn test_c_type_ref() {
        let source = "void foo(MyStruct *s) {}";
        let (refs, _) = parse_and_extract(source, LanguageId::C);
        assert!(
            has_ref(&refs, "MyStruct", ReferenceKind::TypeUsage),
            "refs: {:?}",
            refs
        );
    }

    #[test]
    fn test_c_field_method_call_ref() {
        let source = "void foo(MyObj *obj) { obj->method(); }";
        let (refs, _) = parse_and_extract(source, LanguageId::C);
        assert!(
            refs.iter()
                .any(|r| r.kind == ReferenceKind::Call && r.name == "method"),
            "should have method call ref, refs: {:?}",
            refs
        );
    }

    // --- C++ ---

    #[test]
    fn test_cpp_method_call_ref() {
        let source = "void foo(Foo *f) { f->bar(); }";
        let (refs, _) = parse_and_extract(source, LanguageId::Cpp);
        assert!(
            refs.iter()
                .any(|r| r.kind == ReferenceKind::Call && r.name == "bar"),
            "should have method call ref, refs: {:?}",
            refs
        );
    }

    #[test]
    fn test_cpp_qualified_ref() {
        let source = "void foo() { std::sort(v.begin(), v.end()); }";
        let (refs, _) = parse_and_extract(source, LanguageId::Cpp);
        // qualified_identifier captures: "sort" from std::sort
        assert!(
            refs.iter()
                .any(|r| r.kind == ReferenceKind::Call && r.name == "sort"),
            "should have qualified call ref for sort, refs: {:?}",
            refs
        );
    }

    /// D13: a C++ qualified call `Foo::create()` must retain the head (`Foo`) in
    /// `qualified_name` so the find_references head-match branch can recall it under
    /// the type. Before the `@ref.qualified_call` envelope was added to the C++
    /// query, this carried `qualified_name=None` and the static-call/construction
    /// site was invisible to find_references("Foo").
    #[test]
    fn test_cpp_qualified_call_retains_head() {
        let source = "void f() { Foo::create(); }";
        let (refs, _) = parse_and_extract(source, LanguageId::Cpp);
        let call = refs
            .iter()
            .find(|r| r.name == "create" && r.kind == ReferenceKind::Call)
            .expect("Foo::create() should be captured as a Call, refs: {refs:?}");
        assert_eq!(
            call.qualified_name.as_deref(),
            Some("Foo::create"),
            "C++ qualified call must retain the type/namespace head in qualified_name"
        );
    }

    #[test]
    fn test_cpp_template_type_ref() {
        let source = "void foo(std::vector<MyType> v) {}";
        let (refs, _) = parse_and_extract(source, LanguageId::Cpp);
        assert!(
            has_ref(&refs, "MyType", ReferenceKind::TypeUsage),
            "refs: {:?}",
            refs
        );
    }

    #[test]
    fn test_cpp_using_declaration_import() {
        let source = "using std::string;";
        let (refs, _) = parse_and_extract(source, LanguageId::Cpp);
        // using_declaration with @import.original does NOT produce a separate ref.import
        // (there's no @import.alias to pair with), so the qualified_identifier is
        // processed as import.original but the alias pair is incomplete, so it's skipped.
        // The qualified_identifier capture also matches @ref.call via qualified_identifier pattern.
        // Just verify no panic and query parses correctly.
        let _ = refs; // query compiled and ran without panic
    }

    #[test]
    fn test_cpp_include_ref() {
        let source = "#include <vector>\nvoid foo() {}";
        let (refs, _) = parse_and_extract(source, LanguageId::Cpp);
        assert!(
            refs.iter().any(|r| r.kind == ReferenceKind::Import),
            "should have Import ref for #include, refs: {:?}",
            refs
        );
    }

    // --- Cross-language coverage ---

    #[test]
    fn test_all_languages_produce_at_least_one_ref_from_nontrivial_source() {
        let cases: &[(&str, LanguageId)] = &[
            ("fn main() { println!(\"hi\"); foo(); }", LanguageId::Rust),
            (
                "import os\ndef main():\n    os.path.join('a', 'b')",
                LanguageId::Python,
            ),
            (
                "import React from 'react';\nfetch('/api');",
                LanguageId::JavaScript,
            ),
            (
                "import { Component } from 'react';\nconst x: MyType = null;",
                LanguageId::TypeScript,
            ),
            (
                "package main\nimport \"fmt\"\nfunc main() { fmt.Println(\"hi\") }",
                LanguageId::Go,
            ),
            (
                "import java.util.ArrayList;\nclass A { void f() { new ArrayList(); } }",
                LanguageId::Java,
            ),
            ("#include <stdio.h>\nvoid foo() { bar(); }", LanguageId::C),
            (
                "#include <vector>\nvoid foo() { std::sort(v.begin(), v.end()); }",
                LanguageId::Cpp,
            ),
            // New languages (Phase 07-04)
            (
                "using System;\npublic class App { void Run() { Console.WriteLine(\"hi\"); } }",
                LanguageId::CSharp,
            ),
            (
                "require 'json'\nclass App\n  def run\n    puts 'hi'\n  end\nend",
                LanguageId::Ruby,
            ),
            (
                "import kotlin.io.*\nfun main() { println(\"hi\") }",
                LanguageId::Kotlin,
            ),
            (
                "import 'dart:io';\nvoid main() { print('hello'); }",
                LanguageId::Dart,
            ),
            (
                "defmodule App do\n  alias MyLib.Helper\n  def run, do: :ok\nend",
                LanguageId::Elixir,
            ),
        ];

        for (source, lang) in cases {
            let (refs, _) = parse_and_extract(source, lang.clone());
            assert!(
                !refs.is_empty(),
                "language {:?} should produce refs from non-trivial source, got none",
                lang
            );
        }
    }

    #[test]
    fn test_empty_source_produces_empty_refs_all_languages() {
        let languages = [
            LanguageId::Rust,
            LanguageId::Python,
            LanguageId::JavaScript,
            LanguageId::TypeScript,
            LanguageId::Go,
            LanguageId::Java,
            LanguageId::C,
            LanguageId::Cpp,
            LanguageId::CSharp,
            LanguageId::Ruby,
            LanguageId::Kotlin,
            LanguageId::Dart,
            LanguageId::Elixir,
        ];
        for lang in languages {
            let (refs, alias_map) = parse_and_extract("", lang.clone());
            assert!(
                refs.is_empty(),
                "language {:?} should produce no refs from empty source",
                lang
            );
            assert!(
                alias_map.is_empty(),
                "language {:?} should produce no aliases from empty source",
                lang
            );
        }
    }

    #[test]
    fn test_query_compilation_cached_onclock() {
        // Call extract_references twice for Rust — both calls should return consistent results
        // The query is compiled once (OnceLock) and reused.
        let source = "fn main() { foo(); }";
        let (refs1, _) = parse_and_extract(source, LanguageId::Rust);
        let (refs2, _) = parse_and_extract(source, LanguageId::Rust);
        assert_eq!(
            refs1.len(),
            refs2.len(),
            "same source should produce same number of refs regardless of cache state"
        );
    }

    // --- C# ---

    #[test]
    fn test_csharp_call_ref() {
        let source = "public class App { void Run() { Console.WriteLine(\"hi\"); } }";
        let (refs, _) = parse_and_extract(source, LanguageId::CSharp);
        assert!(
            refs.iter().any(|r| r.kind == ReferenceKind::Call),
            "should have at least one Call ref, refs: {:?}",
            refs
        );
    }

    #[test]
    fn test_csharp_import_ref() {
        let source = "using System.Collections.Generic;\npublic class App {}";
        let (refs, _) = parse_and_extract(source, LanguageId::CSharp);
        assert!(
            refs.iter().any(|r| r.kind == ReferenceKind::Import),
            "should have Import ref for using directive, refs: {:?}",
            refs
        );
    }

    #[test]
    fn test_csharp_import_ref_preserves_qualified_namespace() {
        let source = "using CeRegistry.Core.Services;\npublic class App {}";
        let (refs, _) = parse_and_extract(source, LanguageId::CSharp);
        let import_ref = refs
            .iter()
            .find(|r| r.kind == ReferenceKind::Import)
            .expect("should capture C# using directive");
        assert_eq!(
            import_ref.qualified_name.as_deref(),
            Some("CeRegistry.Core.Services")
        );
    }

    #[test]
    fn test_import_ref_reduces_dotted_name_to_final_segment() {
        // A dotted import reduces its simple `name` to the final segment so
        // name-based find_references matches it, while `qualified_name` retains the
        // full dotted path. Previously a dead `.or_else(split('.'))` branch left the
        // full dotted string as the simple name.
        let source = "using CeRegistry.Core.Services;\npublic class App {}";
        let (refs, _) = parse_and_extract(source, LanguageId::CSharp);
        let import_ref = refs
            .iter()
            .find(|r| r.kind == ReferenceKind::Import)
            .expect("should capture C# using directive");
        assert_eq!(import_ref.name, "Services");
        assert_eq!(
            import_ref.qualified_name.as_deref(),
            Some("CeRegistry.Core.Services")
        );
    }

    #[test]
    fn test_csharp_type_refs_include_constructor_params_and_fields() {
        let source = r#"
using CeRegistry.Core.Services;

public class PacketsController
{
    private readonly IMinioService _minio;

    public PacketsController(IMinioService minioService)
    {
        _minio = minioService;
    }
}
"#;
        let (refs, _) = parse_and_extract(source, LanguageId::CSharp);
        let type_refs: Vec<_> = refs
            .iter()
            .filter(|r| r.kind == ReferenceKind::TypeUsage && r.name == "IMinioService")
            .collect();
        assert_eq!(
            type_refs.len(),
            2,
            "field type and constructor parameter type should both be tracked, refs: {:?}",
            refs
        );
    }

    // --- Ruby ---

    #[test]
    fn test_ruby_call_ref() {
        let source = "def run\n  puts 'hello'\nend";
        let (refs, _) = parse_and_extract(source, LanguageId::Ruby);
        assert!(
            refs.iter().any(|r| r.kind == ReferenceKind::Call),
            "should have at least one Call ref, refs: {:?}",
            refs
        );
    }

    #[test]
    fn test_ruby_import_ref() {
        let source = "require 'json'\ndef run; end";
        let (refs, _) = parse_and_extract(source, LanguageId::Ruby);
        assert!(
            refs.iter().any(|r| r.kind == ReferenceKind::Import),
            "should have Import ref for require, refs: {:?}",
            refs
        );
    }

    // --- Kotlin ---

    #[test]
    fn test_kotlin_call_ref() {
        let source = "fun main() { println(\"hello\") }";
        let (refs, _) = parse_and_extract(source, LanguageId::Kotlin);
        assert!(
            refs.iter().any(|r| r.kind == ReferenceKind::Call),
            "should have at least one Call ref, refs: {:?}",
            refs
        );
    }

    #[test]
    fn test_kotlin_import_ref() {
        let source = "import kotlin.io.println\nfun main() {}";
        let (refs, _) = parse_and_extract(source, LanguageId::Kotlin);
        assert!(
            refs.iter().any(|r| r.kind == ReferenceKind::Import),
            "should have Import ref, refs: {:?}",
            refs
        );
    }

    // --- Dart ---

    /// Plain calls and receiver method calls must both produce Call refs
    /// with the callee name, mirroring the nielsenko grammar's
    /// `call_expression(function:)` / `member_expression(property:)` shapes.
    #[test]
    fn test_dart_call_refs() {
        let source = "void main() { print('x'); var s = 'y'.toUpperCase(); }";
        let (refs, _) = parse_and_extract(source, LanguageId::Dart);
        let calls: Vec<&str> = refs
            .iter()
            .filter(|r| r.kind == ReferenceKind::Call)
            .map(|r| r.name.as_str())
            .collect();
        assert!(
            calls.contains(&"print"),
            "plain function call must yield a Call ref named print, refs: {:?}",
            refs
        );
        assert!(
            calls.contains(&"toUpperCase"),
            "receiver method call must yield a Call ref named toUpperCase, refs: {:?}",
            refs
        );
    }

    #[test]
    fn test_dart_import_ref() {
        let source = "import 'dart:io';\nvoid main() {}";
        let (refs, _) = parse_and_extract(source, LanguageId::Dart);
        assert!(
            refs.iter().any(|r| r.kind == ReferenceKind::Import),
            "should have Import ref for dart import, refs: {:?}",
            refs
        );
    }

    #[test]
    fn test_dart_type_ref() {
        let source = "class Foo { MyType field; }";
        let (refs, _) = parse_and_extract(source, LanguageId::Dart);
        assert!(
            refs.iter().any(|r| r.kind == ReferenceKind::TypeUsage),
            "should have TypeUsage ref, refs: {:?}",
            refs
        );
    }

    // --- Elixir ---

    #[test]
    fn test_elixir_call_ref() {
        let source = "def run do\n  IO.puts(\"hello\")\nend";
        let (refs, _) = parse_and_extract(source, LanguageId::Elixir);
        assert!(
            refs.iter().any(|r| r.kind == ReferenceKind::Call),
            "should have at least one Call ref, refs: {:?}",
            refs
        );
    }

    #[test]
    fn test_elixir_import_ref() {
        let source = "defmodule App do\n  alias MyLib.Helper\nend";
        let (refs, _) = parse_and_extract(source, LanguageId::Elixir);
        assert!(
            refs.iter().any(|r| r.kind == ReferenceKind::Import),
            "should have Import ref for alias, refs: {:?}",
            refs
        );
    }

    /// SF-STRESS-021 regression: `raise ArgumentError` parses as the same
    /// `(call target: (identifier) (arguments (alias)))` shape as `alias Foo`,
    /// so the import query matched it and mislabeled `ArgumentError` as an
    /// Import (polluting conventions common-imports and find_references). The
    /// extraction-site directive guard must reject non-directive call targets.
    #[test]
    fn test_elixir_raise_is_not_an_import() {
        let source = "defmodule App do\n  def run do\n    raise ArgumentError\n  end\nend";
        let (refs, _) = parse_and_extract(source, LanguageId::Elixir);
        assert!(
            !refs.iter().any(|r| r.kind == ReferenceKind::Import),
            "`raise ArgumentError` must NOT produce an Import ref, refs: {:?}",
            refs
        );
    }

    /// The directive guard keeps `import`/`use`/`require` (not just `alias`)
    /// classified as imports.
    #[test]
    fn test_elixir_use_and_import_are_imports() {
        let source = "defmodule App do\n  use GenServer\n  import Enum\n  require Logger\nend";
        let (refs, _) = parse_and_extract(source, LanguageId::Elixir);
        let import_count = refs
            .iter()
            .filter(|r| r.kind == ReferenceKind::Import)
            .count();
        assert!(
            import_count >= 3,
            "use/import/require are all directives -> Import refs, refs: {:?}",
            refs
        );
    }
}
