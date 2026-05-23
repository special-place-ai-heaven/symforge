use symforge::domain::{FileOutcome, LanguageId, SymbolKind};
use symforge::parsing::process_file;
use tree_sitter::Parser;

const RAW_REFERENCE_FIXTURE: &str = include_str!("fixtures/rust/raw_references.rs");

#[test]
fn rust_raw_reference_fixture_parses_without_tree_sitter_errors() {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_rust::LANGUAGE.into())
        .expect("failed to load Rust grammar");

    let tree = parser
        .parse(RAW_REFERENCE_FIXTURE, None)
        .expect("parse returned None");

    assert!(
        !tree.root_node().has_error(),
        "Rust 2024 raw-reference fixture should parse cleanly"
    );
}

#[test]
fn rust_raw_reference_fixture_processes_without_partial_parse() {
    let result = process_file(
        "tests/fixtures/rust/raw_references.rs",
        RAW_REFERENCE_FIXTURE.as_bytes(),
        LanguageId::Rust,
    );

    assert_eq!(result.outcome, FileOutcome::Processed);
    assert!(
        result
            .symbols
            .iter()
            .any(|symbol| symbol.kind == SymbolKind::Function
                && symbol.name == "raw_reference_examples"),
        "expected raw_reference_examples function, symbols: {:?}",
        result.symbols
    );
}
