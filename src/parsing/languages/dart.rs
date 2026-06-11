use tree_sitter::Node;

use super::{
    NO_DOC_SPEC, SymbolSink, collect_symbols, find_first_named_child, push_named_symbol,
    walk_children,
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
    // Node kinds for the nielsenko tree-sitter-dart grammar (0.2.0).
    // Class-likes carry a `name` field; methods are `method_signature`
    // wrappers around function/getter/setter/operator signatures, so the
    // inner signature kinds only count as top-level functions when NOT
    // inside a method_signature (otherwise every method would be emitted
    // twice).
    let kind = match node.kind() {
        "class_declaration" | "mixin_declaration" | "extension_declaration"
        | "extension_type_declaration" => Some(SymbolKind::Class),
        "enum_declaration" => Some(SymbolKind::Enum),
        "method_signature"
        | "constructor_signature"
        | "constant_constructor_signature"
        | "factory_constructor_signature" => Some(SymbolKind::Method),
        "function_signature" | "getter_signature" | "setter_signature"
            if node
                .parent()
                .is_none_or(|p| p.kind() != "method_signature") =>
        {
            Some(SymbolKind::Function)
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
    // Prefer the grammar's explicit `name` field — the first-named-child
    // heuristic grabs the RETURN TYPE on signatures (`Widget build()` would
    // index as "Widget"). Descend one level when the field is a wrapper
    // node (extension_type_declaration names an `extension_type_name`).
    if let Some(name_node) = node.child_by_field_name("name") {
        match name_node.kind() {
            "identifier" | "type_identifier" => {
                return name_node
                    .utf8_text(source.as_bytes())
                    .ok()
                    .map(str::to_string);
            }
            _ => {
                if let Some(inner) = find_first_named_child(&name_node, source, &["identifier"]) {
                    return Some(inner);
                }
            }
        }
    }
    match node.kind() {
        // method_signature has no fields; the name lives on the wrapped
        // function/getter/setter signature's `name` field. No bare-identifier
        // fallback here: operator methods are nameless and the first
        // identifier under a signature is its return type.
        "method_signature" => {
            let mut cursor = node.walk();
            node.children(&mut cursor)
                .find_map(|child| child.child_by_field_name("name"))
                .and_then(|n| n.utf8_text(source.as_bytes()).ok().map(str::to_string))
        }
        _ => find_first_named_child(node, source, &["identifier", "type_identifier"]),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{FileOutcome, LanguageId, SymbolKind};
    use crate::parsing::process_file;
    use tree_sitter::Parser;

    fn parse_dart(source: &str) -> Vec<SymbolRecord> {
        let mut parser = Parser::new();
        let lang: tree_sitter::Language = tree_sitter_dart::LANGUAGE.into();
        parser.set_language(&lang).expect("set Dart language");
        let tree = parser.parse(source, None).expect("parse Dart source");
        extract_symbols(&tree.root_node(), source)
    }

    fn names_of(symbols: &[SymbolRecord], kind: SymbolKind) -> Vec<&str> {
        symbols
            .iter()
            .filter(|s| s.kind == kind)
            .map(|s| s.name.as_str())
            .collect()
    }

    #[test]
    fn test_dart_top_level_function() {
        let source = "void main() {}";
        let symbols = parse_dart(source);
        let func = symbols.iter().find(|s| s.kind == SymbolKind::Function);
        assert!(
            func.is_some(),
            "should extract top-level function, got: {:?}",
            symbols
        );
        assert_eq!(func.unwrap().name, "main");
    }

    #[test]
    fn test_dart_class_definition() {
        let source = "class Animal { void speak() {} }";
        let symbols = parse_dart(source);
        let cls = symbols.iter().find(|s| s.kind == SymbolKind::Class);
        assert!(cls.is_some(), "should extract class, got: {:?}", symbols);
        assert_eq!(cls.unwrap().name, "Animal");

        // The in-class method must be named after its identifier, not its
        // return type (the historical first-named-child misnaming bug).
        let method = symbols.iter().find(|s| s.kind == SymbolKind::Method);
        assert!(method.is_some(), "should extract method, got: {:?}", symbols);
        assert_eq!(method.unwrap().name, "speak");
    }

    #[test]
    fn test_dart_enum_declaration() {
        let source = "enum Color { red, green, blue }";
        let symbols = parse_dart(source);
        let e = symbols.iter().find(|s| s.kind == SymbolKind::Enum);
        assert!(e.is_some(), "should extract enum, got: {:?}", symbols);
        assert_eq!(e.unwrap().name, "Color");
    }

    #[test]
    fn test_dart_process_file_returns_processed() {
        let source = b"class Foo { void bar() {} }";
        let result = process_file("test.dart", source, LanguageId::Dart);
        assert_eq!(
            result.outcome,
            FileOutcome::Processed,
            "outcome: {:?}",
            result.outcome
        );
        assert!(!result.symbols.is_empty(), "should have symbols");
    }

    /// Baseline regression fixture: import + class + concrete method. Guards
    /// symbol extraction across grammar version bumps. On the nielsenko
    /// grammar with field-based naming, the method MUST be extracted as a
    /// Method named `add` — not as a Function named after its return type
    /// `int` (the 0.0.4-era misnaming this rewrite fixed).
    #[test]
    fn test_dart_class_with_import_and_method() {
        let source = "\
import 'dart:math';

class Calculator {
  int add(int a, int b) {
    return a + b;
  }
}
";
        let result = process_file("calculator.dart", source.as_bytes(), LanguageId::Dart);
        assert_eq!(
            result.outcome,
            FileOutcome::Processed,
            "import + class + method must parse cleanly, got: {:?}",
            result.outcome
        );

        let symbols = parse_dart(source);
        let class = symbols.iter().find(|s| s.kind == SymbolKind::Class);
        assert!(
            class.is_some(),
            "should extract the Calculator class, got: {:?}",
            symbols
        );
        assert_eq!(class.unwrap().name, "Calculator");

        let method = symbols.iter().find(|s| s.kind == SymbolKind::Method);
        assert!(
            method.is_some(),
            "should extract the add method, got: {:?}",
            symbols
        );
        assert_eq!(method.unwrap().name, "add");
        assert!(
            method.unwrap().depth > class.unwrap().depth,
            "method must nest under the class"
        );
    }

    /// Dart 3.0 syntax must parse cleanly: sealed classes, records, and
    /// switch expressions (GA since May 2023). The abandoned 0.0.4 grammar
    /// returned parse errors on every one of these.
    #[test]
    fn test_dart3_sealed_class_record_switch_expression_parse_clean() {
        let source = "\
sealed class Shape {}

class Circle extends Shape {
  final double radius;
  Circle(this.radius);
}

(double, double) center(Shape s) {
  return (0.0, 0.0);
}

double area(Shape shape) {
  return switch (shape) {
    Circle(radius: var r) => 3.14 * r * r,
    _ => 0.0,
  };
}
";
        let result = process_file("shapes.dart", source.as_bytes(), LanguageId::Dart);
        assert_eq!(
            result.outcome,
            FileOutcome::Processed,
            "Dart 3 sealed class + record + switch expression must parse cleanly, got: {:?}",
            result.outcome
        );
        assert!(
            result.parse_diagnostic.is_none(),
            "Dart 3 syntax must not produce a partial-parse diagnostic, got: {:?}",
            result.parse_diagnostic
        );

        let symbols = parse_dart(source);
        let classes = names_of(&symbols, SymbolKind::Class);
        assert!(
            classes.contains(&"Shape") && classes.contains(&"Circle"),
            "sealed class Shape and class Circle must be extracted, got: {classes:?}"
        );
    }

    /// Post-3.7 Dart syntax must parse cleanly: null-aware elements (3.8),
    /// dot shorthands (3.10), private named parameters (3.12), empty object
    /// patterns, and the unnamed `library;` directive. These are the exact
    /// real-world failure classes that disqualified the orchard 0.3.2 crate
    /// (2.6% of flutter/packages files); see
    /// docs/dart-parser-investigation.md.
    #[test]
    fn test_dart_3_8_to_3_12_failure_classes_parse_clean() {
        let source = "\
library;

sealed class Result<T> {}

class Ok<T> extends Result<T> {
  final T value;
  Ok(this.value);
}

enum Status { running, stopped }

class Config {
  final int _retries;
  Config({required this._retries});
}

String describe(Result<int> r) {
  return switch (r) {
    Ok<int>() => 'ok',
    _ => 'other',
  };
}

void main() {
  int? a;
  var xs = [1, ?a, 3];
  var m = {?'k': a};
  Status s = .running;
  var big = 1_000_000;
}
";
        let result = process_file("modern.dart", source.as_bytes(), LanguageId::Dart);
        assert_eq!(
            result.outcome,
            FileOutcome::Processed,
            "Dart 3.8-3.12 syntax must parse cleanly, got: {:?}",
            result.outcome
        );
        assert!(
            result.parse_diagnostic.is_none(),
            "Dart 3.8-3.12 syntax must not produce a partial-parse diagnostic, got: {:?}",
            result.parse_diagnostic
        );
    }

    /// Mixins, extensions, and extension types (3.3) must produce symbols.
    /// Before the field-based rewrite these kinds were unmapped — extension
    /// types produced ZERO symbols.
    #[test]
    fn test_dart_mixin_extension_and_extension_type_symbols() {
        let source = "\
mixin Walkable {
  void walk() {}
}

extension Doubling on int {
  int get doubled => this * 2;
}

extension type Meters(double value) {
  Meters plus(Meters other) => Meters(value + other.value);
}
";
        let symbols = parse_dart(source);
        let classes = names_of(&symbols, SymbolKind::Class);
        assert!(
            classes.contains(&"Walkable"),
            "mixin Walkable must be extracted, got: {classes:?}"
        );
        assert!(
            classes.contains(&"Doubling"),
            "extension Doubling must be extracted, got: {classes:?}"
        );
        assert!(
            classes.contains(&"Meters"),
            "extension type Meters must be extracted, got: {classes:?}"
        );

        let methods = names_of(&symbols, SymbolKind::Method);
        assert!(
            methods.contains(&"walk"),
            "mixin method walk must be extracted, got: {methods:?}"
        );
        assert!(
            methods.contains(&"doubled"),
            "extension getter doubled must be extracted as a member, got: {methods:?}"
        );
        assert!(
            methods.contains(&"plus"),
            "extension type method plus must be extracted, got: {methods:?}"
        );
    }

    /// Getters, setters, and constructors inside a class body are members;
    /// a nameless operator method must not be misnamed after its return
    /// type, and top-level getters count as functions.
    #[test]
    fn test_dart_getters_setters_constructors() {
        let source = "\
class Circle {
  double radius;
  Circle(this.radius);
  double get diameter => radius * 2;
  set diameter(double d) => radius = d / 2;
  Circle operator +(Circle other) => Circle(radius + other.radius);
}

int get answer => 42;
";
        let symbols = parse_dart(source);
        let methods = names_of(&symbols, SymbolKind::Method);
        assert!(
            methods.contains(&"Circle"),
            "constructor must be extracted, got: {methods:?}"
        );
        assert!(
            methods.contains(&"diameter"),
            "getter and setter must be extracted, got: {methods:?}"
        );
        assert!(
            !methods.contains(&"Circle ") && !names_of(&symbols, SymbolKind::Method).contains(&"double"),
            "operator method must not be misnamed after its return type, got: {methods:?}"
        );

        let functions = names_of(&symbols, SymbolKind::Function);
        assert!(
            functions.contains(&"answer"),
            "top-level getter must be extracted as a function, got: {functions:?}"
        );
    }
}
