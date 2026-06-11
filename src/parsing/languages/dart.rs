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
    let kind = match node.kind() {
        "function_signature" => Some(SymbolKind::Function),
        "class_definition" => Some(SymbolKind::Class),
        "enum_declaration" => Some(SymbolKind::Enum),
        "method_signature" => Some(SymbolKind::Method),
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
    find_first_named_child(node, source, &["identifier", "type_identifier"])
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{FileOutcome, LanguageId, SymbolKind};
    use crate::parsing::process_file;
    use tree_sitter::Parser;

    fn parse_dart(source: &str) -> Vec<SymbolRecord> {
        let mut parser = Parser::new();
        let lang: tree_sitter::Language = tree_sitter_dart_orchard::LANGUAGE.into();
        parser.set_language(&lang).expect("set Dart language");
        let tree = parser.parse(source, None).expect("parse Dart source");
        extract_symbols(&tree.root_node(), source)
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

    /// Baseline regression fixture for the tree-sitter-dart grammar: a realistic
    /// file with an import directive, a class, and an in-class method. Guards
    /// symbol extraction across grammar version bumps.
    ///
    /// Invariants asserted are deliberately version-tolerant so they hold on the
    /// current 0.0.4 grammar and let us detect a *regression* on a bump:
    ///   1. the leading `import` directive parses cleanly (Processed outcome),
    ///   2. the `Calculator` class is extracted by name,
    ///   3. at least one in-class member symbol is extracted.
    ///
    /// Note: 0.0.4 parses the concrete method `int add(...)` as a
    /// `function_signature` and (imperfectly) names it after the return type
    /// `int` rather than `add`. That is an existing grammar/extractor limitation,
    /// not something this test should hard-code as correct; it only asserts that
    /// a member symbol survives so a grammar bump that drops member extraction
    /// entirely is caught.
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
        // Parse must succeed cleanly (process_file reports Processed, not a
        // parse failure) even with the leading import directive present.
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

        // At least one in-class member (the method, however the grammar models
        // it) must be extracted. A bump that regresses to extracting only the
        // class shell would fail here.
        let member = symbols.iter().find(|s| s.depth > class.unwrap().depth);
        assert!(
            member.is_some(),
            "should extract at least one in-class member symbol, got: {:?}",
            symbols
        );
    }

    /// Dart 3 syntax must parse cleanly: sealed classes, records, and switch
    /// expressions (all GA since Dart 3.0, May 2023). The previous grammar
    /// (tree-sitter-dart 0.0.4) returned parse errors on every one of these,
    /// silently degrading symbols for any modern Dart/Flutter file — the
    /// reason for the switch to tree-sitter-dart-orchard.
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
        let classes: Vec<&str> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Class)
            .map(|s| s.name.as_str())
            .collect();
        assert!(
            classes.contains(&"Shape") && classes.contains(&"Circle"),
            "sealed class Shape and class Circle must be extracted, got: {classes:?}"
        );
    }
}
