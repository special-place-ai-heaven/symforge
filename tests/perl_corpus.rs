//! Program 016 — Perl fixture corpus integration tests.
//!
//! Run: `cargo test --features server --test perl_corpus -- --test-threads=1`
//! Bench: `cargo test --features server --test perl_corpus bench_ -- --ignored --nocapture`

use std::path::{Path, PathBuf};

use serde::Deserialize;
use symforge::domain::{FileOutcome, LanguageId, ReferenceKind, SymbolKind};
use symforge::parsing::process_file;
use tree_sitter::Parser;

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/perl")
}

#[derive(Debug, Deserialize)]
struct SymbolExpect {
    kind: String,
    name: String,
}

#[derive(Debug, Deserialize)]
struct RefExpect {
    kind: String,
    name: String,
    #[serde(default)]
    qualified_name: Option<String>,
    #[serde(default)]
    optional: bool,
}

#[derive(Debug, Deserialize)]
struct FixtureEntry {
    file: String,
    #[serde(default)]
    #[allow(dead_code)] // taxonomy tags for S1 sign-off / future bench reports
    construct_classes: Vec<String>,
    #[serde(default = "default_parse_expect")]
    parse_expect: String,
    #[serde(default)]
    symbols: Vec<SymbolExpect>,
    #[serde(default)]
    refs: Vec<RefExpect>,
}

fn default_parse_expect() -> String {
    "clean".into()
}

#[derive(Debug, Deserialize)]
struct Manifest {
    fixtures: Vec<FixtureEntry>,
}

fn load_manifest() -> Manifest {
    let raw = std::fs::read_to_string(fixture_dir().join("manifest.json")).expect("manifest.json");
    serde_json::from_str(&raw).expect("parse manifest.json")
}

fn parse_symbol_kind(s: &str) -> SymbolKind {
    match s {
        "Function" => SymbolKind::Function,
        "Module" => SymbolKind::Module,
        "Class" => SymbolKind::Class,
        "Interface" => SymbolKind::Interface,
        other => panic!("unknown symbol kind: {other}"),
    }
}

fn parse_ref_kind(s: &str) -> ReferenceKind {
    match s {
        "Call" => ReferenceKind::Call,
        "Import" => ReferenceKind::Import,
        "TypeUsage" => ReferenceKind::TypeUsage,
        other => panic!("unknown ref kind: {other}"),
    }
}

fn process_fixture(path: &Path, bytes: &[u8]) -> symforge::domain::FileProcessingResult {
    process_file(path.to_string_lossy().as_ref(), bytes, LanguageId::Perl)
}

fn tree_has_error(bytes: &[u8]) -> bool {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_perl::LANGUAGE.into())
        .expect("set perl language");
    let source = String::from_utf8_lossy(bytes);
    let tree = parser.parse(source.as_ref(), None).expect("parse");
    tree.root_node().has_error()
}

#[test]
fn test_corpus_minimum_fixture_count() {
    let manifest = load_manifest();
    assert!(
        manifest.fixtures.len() >= 20,
        "SC-001 requires >=20 fixtures, got {}",
        manifest.fixtures.len()
    );
}

#[test]
fn test_corpus_symbols_and_refs() {
    let manifest = load_manifest();
    let dir = fixture_dir();

    for fx in &manifest.fixtures {
        let path = dir.join(&fx.file);
        let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", fx.file));
        let result = process_fixture(&path, &bytes);

        match fx.parse_expect.as_str() {
            "clean" => assert!(
                matches!(
                    result.outcome,
                    FileOutcome::Processed | FileOutcome::PartialParse { .. }
                ),
                "{}: expected clean parse, got {:?}",
                fx.file,
                result.outcome
            ),
            "partial" => assert!(
                matches!(result.outcome, FileOutcome::PartialParse { .. }),
                "{}: expected partial",
                fx.file
            ),
            "error" => assert!(
                matches!(result.outcome, FileOutcome::Failed { .. }),
                "{}: expected failed",
                fx.file
            ),
            other => panic!("unknown parse_expect: {other}"),
        }

        for sym in &fx.symbols {
            let kind = parse_symbol_kind(&sym.kind);
            assert!(
                result
                    .symbols
                    .iter()
                    .any(|s| s.kind == kind && s.name == sym.name),
                "{}: missing symbol {} {:?}, got {:?}",
                fx.file,
                sym.name,
                sym.kind,
                result
                    .symbols
                    .iter()
                    .map(|s| format!("{:?}:{}", s.kind, s.name))
                    .collect::<Vec<_>>()
            );
        }

        for expect in &fx.refs {
            let kind = parse_ref_kind(&expect.kind);
            let found = result
                .references
                .iter()
                .find(|r| r.kind == kind && r.name == expect.name);
            if found.is_none() {
                if expect.optional {
                    continue;
                }
                panic!(
                    "{}: missing ref {} {:?}, got {:?}",
                    fx.file,
                    expect.name,
                    expect.kind,
                    result
                        .references
                        .iter()
                        .map(|r| format!("{:?}:{}:{:?}", r.kind, r.name, r.qualified_name))
                        .collect::<Vec<_>>()
                );
            }
            if let Some(qn) = &expect.qualified_name {
                assert_eq!(
                    found.unwrap().qualified_name.as_deref(),
                    Some(qn.as_str()),
                    "{}: ref {} qualified_name",
                    fx.file,
                    expect.name
                );
            }
        }
    }
}

#[test]
#[ignore = "016 S1 bench — run with --ignored; set PERL_CORPUS_WRITE_METRICS=1 to write JSON"]
fn bench_corpus_parse_metrics() {
    let manifest = load_manifest();
    let dir = fixture_dir();
    let mut clean = 0usize;
    let mut partial = 0usize;
    let mut error = 0usize;

    for fx in &manifest.fixtures {
        let bytes = std::fs::read(dir.join(&fx.file)).expect("read fixture");
        let result = process_fixture(Path::new(&fx.file), &bytes);
        let tree_error = tree_has_error(&bytes);

        match result.outcome {
            FileOutcome::Processed if !tree_error => clean += 1,
            FileOutcome::Processed => partial += 1,
            FileOutcome::PartialParse { .. } => partial += 1,
            FileOutcome::Failed { .. } => error += 1,
        }
    }

    let total = manifest.fixtures.len();
    let clean_pct = (clean as f64 / total as f64) * 100.0;
    println!(
        "perl corpus: total={total} clean={clean} partial={partial} error={error} clean_pct={clean_pct:.1}%"
    );

    if std::env::var("PERL_CORPUS_WRITE_METRICS").as_deref() == Ok("1") {
        let metrics = serde_json::json!({
            "fixture_count": total,
            "clean_parse_count": clean,
            "clean_parse_pct": clean_pct,
            "partial_parse_count": partial,
            "error_count": error,
            "measured_at": chrono::Utc::now().to_rfc3339(),
            "symforge_version": env!("CARGO_PKG_VERSION"),
            "grammar_version": "ts-parser-perl 1.1.3"
        });
        let out = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("docs/research/perl/corpus-metrics.json");
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).expect("mkdir docs/research/perl");
        }
        std::fs::write(&out, serde_json::to_string_pretty(&metrics).expect("json"))
            .expect("write corpus-metrics.json");
        println!("wrote {}", out.display());
    }
}
