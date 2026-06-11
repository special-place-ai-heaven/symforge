use symforge::domain::{FileOutcome, LanguageId, SymbolKind};
use symforge::parsing::process_file;
use tree_sitter::Parser;

#[test]
fn test_rust_grammar_loads_and_parses() {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_rust::LANGUAGE.into())
        .expect("failed to load Rust grammar");
    let tree = parser
        .parse("fn main() {}", None)
        .expect("parse returned None");
    assert!(!tree.root_node().kind().is_empty());
}

#[test]
fn test_python_grammar_loads_and_parses() {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_python::LANGUAGE.into())
        .expect("failed to load Python grammar");
    let tree = parser
        .parse("def hello(): pass", None)
        .expect("parse returned None");
    assert!(!tree.root_node().kind().is_empty());
}

#[test]
fn test_javascript_grammar_loads_and_parses() {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_javascript::LANGUAGE.into())
        .expect("failed to load JavaScript grammar");
    let tree = parser
        .parse("function hello() {}", None)
        .expect("parse returned None");
    assert!(!tree.root_node().kind().is_empty());
}

#[test]
fn test_typescript_grammar_loads_and_parses() {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
        .expect("failed to load TypeScript grammar");
    let tree = parser
        .parse("function hello(): void {}", None)
        .expect("parse returned None");
    assert!(!tree.root_node().kind().is_empty());
}

#[test]
fn test_java_grammar_loads_and_parses() {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_java::LANGUAGE.into())
        .expect("failed to load Java grammar");
    let tree = parser
        .parse("public class App { public void run() {} }", None)
        .expect("parse returned None");
    assert!(!tree.root_node().kind().is_empty());
    assert!(!tree.root_node().has_error());
}

#[test]
fn test_go_grammar_loads_and_parses() {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_go::LANGUAGE.into())
        .expect("failed to load Go grammar");
    let tree = parser
        .parse("package main\nfunc main() {}", None)
        .expect("parse returned None");
    assert!(!tree.root_node().kind().is_empty());
}

#[test]
fn test_c_grammar_loads_and_parses() {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_c::LANGUAGE.into())
        .expect("failed to load C grammar — possible ABI mismatch");
    let source = "struct Point { int x; int y; };\nint add(int a, int b) { return a + b; }";
    let tree = parser.parse(source, None).expect("parse returned None");
    assert!(!tree.root_node().kind().is_empty());
    assert!(
        !tree.root_node().has_error(),
        "C source should parse without syntax errors"
    );

    // Verify symbols extracted via process_file
    let result = process_file("test.c", source.as_bytes(), LanguageId::C);
    assert_eq!(result.outcome, FileOutcome::Processed);
    assert!(
        result
            .symbols
            .iter()
            .any(|s| s.kind == SymbolKind::Struct && s.name == "Point"),
        "should extract Point struct, symbols: {:?}",
        result.symbols
    );
    assert!(
        result
            .symbols
            .iter()
            .any(|s| s.kind == SymbolKind::Function && s.name == "add"),
        "should extract add function, symbols: {:?}",
        result.symbols
    );
}

#[test]
fn test_cpp_grammar_loads_and_parses() {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_cpp::LANGUAGE.into())
        .expect("failed to load C++ grammar — possible ABI mismatch");
    let source = "namespace myns {\n  class Foo { public: void bar(); };\n  void Foo::bar() { }\n}";
    let tree = parser.parse(source, None).expect("parse returned None");
    assert!(!tree.root_node().kind().is_empty());
    assert!(
        !tree.root_node().has_error(),
        "C++ source should parse without syntax errors"
    );

    // Verify symbols extracted via process_file
    let result = process_file("test.cpp", source.as_bytes(), LanguageId::Cpp);
    assert_eq!(result.outcome, FileOutcome::Processed);
    assert!(
        result
            .symbols
            .iter()
            .any(|s| s.kind == SymbolKind::Module && s.name == "myns"),
        "should extract myns namespace, symbols: {:?}",
        result.symbols
    );
    assert!(
        result
            .symbols
            .iter()
            .any(|s| s.kind == SymbolKind::Class && s.name == "Foo"),
        "should extract Foo class, symbols: {:?}",
        result.symbols
    );
}

// --- New language grammar tests (Phase 07-04) ---

#[test]
fn test_csharp_grammar_loads_and_parses() {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_c_sharp::LANGUAGE.into())
        .expect("failed to load C# grammar — possible ABI mismatch");
    let source = "public class Greeter { public void Hello() {} }";
    let tree = parser.parse(source, None).expect("parse returned None");
    assert!(!tree.root_node().kind().is_empty());

    let result = process_file("test.cs", source.as_bytes(), LanguageId::CSharp);
    assert_eq!(result.outcome, FileOutcome::Processed);
    assert!(
        result
            .symbols
            .iter()
            .any(|s| s.kind == SymbolKind::Class && s.name == "Greeter"),
        "should extract Greeter class, symbols: {:?}",
        result.symbols
    );
}

#[test]
fn test_ruby_grammar_loads_and_parses() {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_ruby::LANGUAGE.into())
        .expect("failed to load Ruby grammar — possible ABI mismatch");
    let source = "class Animal\n  def speak\n  end\nend";
    let tree = parser.parse(source, None).expect("parse returned None");
    assert!(!tree.root_node().kind().is_empty());

    let result = process_file("test.rb", source.as_bytes(), LanguageId::Ruby);
    assert_eq!(result.outcome, FileOutcome::Processed);
    assert!(
        result
            .symbols
            .iter()
            .any(|s| s.kind == SymbolKind::Class && s.name == "Animal"),
        "should extract Animal class, symbols: {:?}",
        result.symbols
    );
    assert!(
        result
            .symbols
            .iter()
            .any(|s| s.kind == SymbolKind::Method && s.name == "speak"),
        "should extract speak method, symbols: {:?}",
        result.symbols
    );
}

#[test]
fn test_kotlin_grammar_loads_and_parses() {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_kotlin_sg::LANGUAGE.into())
        .expect("failed to load Kotlin grammar — possible ABI mismatch");
    let source = "class Greeter { fun greet() { } }";
    let tree = parser.parse(source, None).expect("parse returned None");
    assert!(!tree.root_node().kind().is_empty());

    let result = process_file("test.kt", source.as_bytes(), LanguageId::Kotlin);
    assert!(
        matches!(
            result.outcome,
            FileOutcome::Processed | FileOutcome::PartialParse { .. }
        ),
        "Kotlin should be Processed or PartialParse, got: {:?}",
        result.outcome
    );
    assert!(
        result.symbols.iter().any(|s| s.name == "Greeter"),
        "should extract Greeter, symbols: {:?}",
        result.symbols
    );
}

#[test]
fn test_dart_grammar_loads_and_parses() {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_dart_orchard::LANGUAGE.into())
        .expect("failed to load Dart grammar — possible ABI mismatch");
    let source = "class Animal { void speak() {} }";
    let tree = parser.parse(source, None).expect("parse returned None");
    assert!(!tree.root_node().kind().is_empty());

    let result = process_file("test.dart", source.as_bytes(), LanguageId::Dart);
    assert_eq!(result.outcome, FileOutcome::Processed);
    assert!(
        result
            .symbols
            .iter()
            .any(|s| s.kind == SymbolKind::Class && s.name == "Animal"),
        "should extract Animal class, symbols: {:?}",
        result.symbols
    );
}

#[test]
fn test_elixir_grammar_loads_and_parses() {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_elixir::LANGUAGE.into())
        .expect("failed to load Elixir grammar — possible ABI mismatch");
    let source = "defmodule Greeter do\n  def greet do\n    :ok\n  end\nend";
    let tree = parser.parse(source, None).expect("parse returned None");
    assert!(!tree.root_node().kind().is_empty());

    let result = process_file("test.ex", source.as_bytes(), LanguageId::Elixir);
    assert_eq!(result.outcome, FileOutcome::Processed);
    assert!(
        result
            .symbols
            .iter()
            .any(|s| s.kind == SymbolKind::Module && s.name == "Greeter"),
        "should extract Greeter module, symbols: {:?}",
        result.symbols
    );
}

#[test]
fn test_php_grammar_loads_and_parses() {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_php::LANGUAGE_PHP.into())
        .expect("failed to load PHP grammar");
    let source = "<?php class Foo { public function bar() {} }";
    let tree = parser.parse(source, None).expect("parse returned None");
    assert!(!tree.root_node().kind().is_empty());

    let result = process_file("test.php", source.as_bytes(), LanguageId::Php);
    assert!(
        matches!(
            result.outcome,
            FileOutcome::Processed | FileOutcome::PartialParse { .. }
        ),
        "PHP should parse successfully: {:?}",
        result.outcome
    );
    assert!(
        result
            .symbols
            .iter()
            .any(|s| s.kind == SymbolKind::Class && s.name == "Foo"),
        "should extract Foo class, symbols: {:?}",
        result.symbols
    );
}

#[test]
fn test_swift_grammar_loads_and_parses() {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_swift::LANGUAGE.into())
        .expect("failed to load Swift grammar");
    let source = "class Foo { func bar() -> Int { return 0 } }";
    let tree = parser.parse(source, None).expect("parse returned None");
    assert!(!tree.root_node().kind().is_empty());

    let result = process_file("test.swift", source.as_bytes(), LanguageId::Swift);
    assert!(
        matches!(
            result.outcome,
            FileOutcome::Processed | FileOutcome::PartialParse { .. }
        ),
        "Swift should parse successfully: {:?}",
        result.outcome
    );
    assert!(
        result
            .symbols
            .iter()
            .any(|s| s.kind == SymbolKind::Class && s.name == "Foo"),
        "should extract Foo class, symbols: {:?}",
        result.symbols
    );
}

#[test]
fn test_perl_grammar_loads_and_parses() {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_perl::LANGUAGE.into())
        .expect("failed to load Perl grammar");
    let source = "sub greet { print \"hello\\n\"; }";
    let tree = parser.parse(source, None).expect("parse returned None");
    assert!(!tree.root_node().kind().is_empty());

    let result = process_file("test.pl", source.as_bytes(), LanguageId::Perl);
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
