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
        .set_language(&tree_sitter_dart::LANGUAGE.into())
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

// ---------------------------------------------------------------------------
// SF-STRESS-005: `.h` C-family header grammar disambiguation.
//
// `.h` maps to LanguageId::C by extension. Cross-platform repos fill `.h` with
// C++ (`class`/`namespace`/`::`), which the C grammar cannot parse at all — it
// extracts ZERO symbols (total loss). The parse pipeline now disambiguates a
// `.h` header from its content and adopts the C++ grammar (and C++ reported
// language) when it parses the header better than C.
// ---------------------------------------------------------------------------

/// Flutter app-template `flutter_window.h` shape: `class` + `::` + `std::`
/// qualified types. This is the corpus-proven SF-005 case (repo-owned, checked
/// into every Flutter desktop app).
const FLUTTER_WINDOW_H: &str = r#"#ifndef RUNNER_FLUTTER_WINDOW_H_
#define RUNNER_FLUTTER_WINDOW_H_

#include <flutter/dart_project.h>
#include <memory>

// A window that does nothing but host a Flutter view.
class FlutterWindow : public Win32Window {
 public:
  explicit FlutterWindow(const flutter::DartProject& project);
  virtual ~FlutterWindow();

 protected:
  bool OnCreate() override;
  void OnDestroy() override;

 private:
  flutter::DartProject project_;
  std::unique_ptr<flutter::FlutterViewController> flutter_controller_;
};

#endif  // RUNNER_FLUTTER_WINDOW_H_
"#;

#[test]
fn sf005_cpp_header_dot_h_extracts_class_symbol() {
    // `.h` arrives from discovery as LanguageId::C (extension-only).
    let result = process_file(
        "windows/runner/flutter_window.h",
        FLUTTER_WINDOW_H.as_bytes(),
        LanguageId::C,
    );

    // The C++ class symbol must be recovered (it was a 0-symbol total loss before).
    assert!(
        result
            .symbols
            .iter()
            .any(|s| s.kind == SymbolKind::Class && s.name == "FlutterWindow"),
        "C++ `.h` header must extract `class FlutterWindow`; got {:?} ({:?})",
        result.symbols,
        result.outcome
    );
    // It is genuinely C++, so it is reported as C++, not mislabeled C.
    assert_eq!(
        result.language,
        LanguageId::Cpp,
        "a C++ `.h` header parsed via the C++ grammar must be reported as C++"
    );
}

#[test]
fn sf005_ab_dot_h_vs_dot_hpp_yield_same_class_symbol() {
    // A/B: identical bytes routed via `.h` (C by extension) and `.hpp` (Cpp by
    // extension) must now both recover the class symbol. Before the fix the `.h`
    // route yielded zero symbols while `.hpp` yielded the class.
    let via_h = process_file(
        "flutter_window.h",
        FLUTTER_WINDOW_H.as_bytes(),
        LanguageId::C,
    );
    let via_hpp = process_file(
        "flutter_window.hpp",
        FLUTTER_WINDOW_H.as_bytes(),
        LanguageId::Cpp,
    );

    let class_name = |r: &symforge::domain::FileProcessingResult| {
        r.symbols
            .iter()
            .find(|s| s.kind == SymbolKind::Class)
            .map(|s| s.name.clone())
    };

    assert_eq!(
        class_name(&via_h),
        Some("FlutterWindow".to_string()),
        "`.h` route must recover the class (A/B parity with `.hpp`); got {:?}",
        via_h.symbols
    );
    assert_eq!(
        class_name(&via_h),
        class_name(&via_hpp),
        "`.h` and `.hpp` routes must extract the same class symbol; \
         .h={:?} .hpp={:?}",
        via_h.symbols,
        via_hpp.symbols
    );
}

#[test]
fn sf005_plain_c_header_stays_c() {
    // A genuine C header has no C++ markers and parses clean as C: it must stay
    // C and never be relabeled C++.
    let source = r#"#ifndef MYLIB_H
#define MYLIB_H

struct point {
    int x;
    int y;
};

int add(int a, int b);
void clear_point(struct point *p);

#endif
"#;
    let result = process_file("include/mylib.h", source.as_bytes(), LanguageId::C);

    assert_eq!(
        result.language,
        LanguageId::C,
        "a plain C header must stay C, not be relabeled C++"
    );
    assert_eq!(
        result.outcome,
        FileOutcome::Processed,
        "a plain C header must parse clean as C; got {:?}",
        result.parse_diagnostic
    );
    assert!(
        result
            .symbols
            .iter()
            .any(|s| s.kind == SymbolKind::Struct && s.name == "point"),
        "plain C header should extract `struct point`; got {:?}",
        result.symbols
    );
}

#[test]
fn sf005_objc_header_handled_honestly() {
    // Objective-C `.h` (`@interface`/`@property`/`#import`) has NO shipped
    // tree-sitter grammar. The honest behavior: it stays C (the extension
    // default), is NOT mislabeled C++, and surfaces as a partial parse rather
    // than faking a clean C++ result. This test documents that contract.
    let source = r#"#import <Foundation/Foundation.h>

@interface BKBook : NSObject
@property(nonatomic, copy, nullable) NSString *title;
@property(nonatomic, strong, nullable) NSNumber *pageCount;
@end
"#;
    let result = process_file("ios/api.h", source.as_bytes(), LanguageId::C);

    assert_ne!(
        result.language,
        LanguageId::Cpp,
        "an Objective-C header must NOT be silently relabeled C++ (no objc grammar ships)"
    );
    // Objective-C constructs do not parse cleanly under C, so the outcome is an
    // honest partial — we classify what cannot be parsed rather than pretend.
    assert!(
        matches!(result.outcome, FileOutcome::PartialParse { .. }),
        "an Objective-C header should surface as an honest partial parse; got {:?}",
        result.outcome
    );
}

// ---------------------------------------------------------------------------
// SF-STRESS-007: phantom symbols from inside string literals.
//
// On Windows checkouts (core.autocrlf=true) a backslash line continuation
// inside a C/C++ string literal is `\` + CR + LF. tree-sitter-cpp mis-lexes
// `\`+CR as an escape_sequence, the parse errors, and error recovery harvests
// embedded-DSL (HLSL shader) declarations from INSIDE the string as phantom
// C++ symbols at full confidence. The pipeline now reparses a byte-length
// preserving copy (`\`+CR+LF -> `\`+LF+CR) and adopts it when clean.
// ---------------------------------------------------------------------------

#[test]
fn sf007_string_literal_content_never_produces_symbols() {
    // A C++ function holds an HLSL shader in a backslash-continued string with
    // CRLF line endings. The shader text declares `struct VS_INPUT` and a
    // `main()` entry point — these must NEVER be extracted as C++ symbols.
    // Build the source with explicit CRLF + backslash continuations.
    let mut src = String::new();
    src.push_str("static bool CreateDeviceObjects() {\r\n");
    src.push_str("  static const char* vertexShader =\r\n");
    src.push_str("    \"cbuffer vertexBuffer : register(b0) \\\r\n");
    src.push_str("    struct VS_INPUT \\\r\n");
    src.push_str("    { float2 pos : POSITION; }; \\\r\n");
    src.push_str("    struct PS_INPUT \\\r\n");
    src.push_str("    { float4 pos : SV_POSITION; }; \\\r\n");
    src.push_str("    PS_INPUT main(VS_INPUT input) { return input; } \\\r\n");
    src.push_str("    \";\r\n");
    src.push_str("  return true;\r\n");
    src.push_str("}\r\n");
    src.push_str("void RealFunctionAfterString() {}\r\n");

    let result = process_file(
        "backends/imgui_impl_dx10.cpp",
        src.as_bytes(),
        LanguageId::Cpp,
    );

    let names: Vec<&str> = result.symbols.iter().map(|s| s.name.as_str()).collect();

    // No phantom symbols harvested from inside the string literal.
    for phantom in ["VS_INPUT", "PS_INPUT", "main"] {
        assert!(
            !names.contains(&phantom),
            "phantom symbol `{phantom}` was harvested from inside a string literal; \
             extracted symbols: {names:?}"
        );
    }
    // The real symbols around the string survive the recovery reparse.
    assert!(
        names.contains(&"CreateDeviceObjects"),
        "the real enclosing function must survive; got {names:?}"
    );
    assert!(
        names.contains(&"RealFunctionAfterString"),
        "the real trailing function must survive; got {names:?}"
    );
    // The recovery reparse is clean, so no partial-parse diagnostic remains.
    assert_eq!(
        result.outcome,
        FileOutcome::Processed,
        "backslash-CRLF recovery must yield a clean parse; got {:?}",
        result.parse_diagnostic
    );
}

#[test]
fn sf007_lf_string_literal_still_clean_no_phantoms() {
    // Control: the byte-identical LF variant already parses clean with zero
    // phantoms. The recovery path must not disturb it (no backslash-CRLF
    // present, so it is a plain clean parse).
    let src = "static bool CreateDeviceObjects() {\n  static const char* s =\n    \"struct VS_INPUT { float2 pos; }; PS_INPUT main(VS_INPUT i) { return i; }\";\n  return true;\n}\nvoid RealFn() {}\n";

    let result = process_file("shader_lf.cpp", src.as_bytes(), LanguageId::Cpp);
    let names: Vec<&str> = result.symbols.iter().map(|s| s.name.as_str()).collect();

    for phantom in ["VS_INPUT", "PS_INPUT", "main"] {
        assert!(
            !names.contains(&phantom),
            "LF control must have no phantom `{phantom}`; got {names:?}"
        );
    }
    assert!(names.contains(&"CreateDeviceObjects") && names.contains(&"RealFn"));
}
