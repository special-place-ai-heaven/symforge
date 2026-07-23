/// Integration and regression tests for config file parsing (Sprint 11).
///
/// Proves:
///   CFG-01: All 5 config file types are discovered and indexed via LiveIndex::load
///   CFG-02: JSON key paths extracted (nested dot notation)
///   CFG-03: TOML key paths extracted (nested dot notation)
///   CFG-04: Markdown sections extracted (dot-joined hierarchy)
///   CFG-05: .env variables extracted with SymbolKind::Variable
///   CFG-06: YAML nested keys extracted
///   CFG-07: Duplicate Markdown headers disambiguated with #N suffix
///   CFG-08: Literal-dot JSON keys escaped to ~1
///   CFG-09: Edit capability gating (TextEditSafe / StructuralEditSafe)
///   CFG-10: Malformed JSON produces Failed outcome via process_file
use std::fs;
use std::path::Path;

use symforge::{
    domain::{FileOutcome, LanguageId, SymbolKind},
    live_index::{IndexState, LiveIndex},
    parsing::{
        config_extractors::{EditCapability, extractor_for},
        process_file,
    },
};
use tempfile::tempdir;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn write_file(dir: &Path, name: &str, content: &str) {
    let path = dir.join(name);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, content).unwrap();
}

fn has_symbol(symbols: &[symforge::domain::SymbolRecord], name: &str) -> bool {
    symbols.iter().any(|s| s.name == name)
}

// ---------------------------------------------------------------------------
// CFG-01: All 5 config file types are discovered and indexed
// ---------------------------------------------------------------------------

#[test]
fn test_all_config_types_discovered_and_indexed() {
    let dir = tempdir().unwrap();

    write_file(dir.path(), "config.json", r#"{"name": "test"}"#);
    write_file(dir.path(), "config.toml", "[package]\nname = \"test\"\n");
    write_file(dir.path(), "config.yaml", "name: test\n");
    write_file(dir.path(), "README.md", "# Title\nSome content.\n");
    // `app.env` matches the environment-credentials sensitive-path rule
    // (any `*.env` file), so discovery demotes it to metadata-only for
    // secret-safety and it is NOT symbol-indexed. The Env extractor code path
    // is covered directly by `test_env_variables_extracted` via `process_file`.
    write_file(
        dir.path(),
        "app.env",
        "DATABASE_URL=postgres://localhost/db\n",
    );

    let shared = LiveIndex::load(dir.path()).unwrap();
    let index = shared.read();

    assert_eq!(
        index.index_state(),
        IndexState::Ready,
        "LiveIndex should be Ready after loading config files"
    );
    assert_eq!(
        index.file_count(),
        4,
        "json/toml/yaml/md are indexed; app.env is sensitive-path demoted"
    );

    assert!(
        index.get_file("config.json").is_some(),
        "config.json should be indexed"
    );
    assert!(
        index.get_file("config.toml").is_some(),
        "config.toml should be indexed"
    );
    assert!(
        index.get_file("config.yaml").is_some(),
        "config.yaml should be indexed"
    );
    assert!(
        index.get_file("README.md").is_some(),
        "README.md should be indexed"
    );
    assert!(
        index.get_file("app.env").is_none(),
        "app.env is sensitive-path demoted (environment-credentials) and not indexed"
    );
}

// ---------------------------------------------------------------------------
// CFG-02: JSON key paths extracted
// ---------------------------------------------------------------------------

#[test]
fn test_json_key_paths_extracted() {
    let content = br#"{
  "name": "my-app",
  "scripts": {
    "test": "jest",
    "build": "tsc"
  },
  "dependencies": {
    "express": "^4.18.0"
  }
}"#;

    let result = process_file("package.json", content, LanguageId::Json);
    assert_eq!(result.outcome, FileOutcome::Processed);

    let syms = &result.symbols;
    assert!(has_symbol(syms, "name"), "missing 'name'");
    assert!(has_symbol(syms, "scripts"), "missing 'scripts'");
    assert!(has_symbol(syms, "scripts.test"), "missing 'scripts.test'");
    assert!(has_symbol(syms, "scripts.build"), "missing 'scripts.build'");
    assert!(has_symbol(syms, "dependencies"), "missing 'dependencies'");
    assert!(
        has_symbol(syms, "dependencies.express"),
        "missing 'dependencies.express'"
    );

    // All config keys must have SymbolKind::Key
    for sym in syms {
        assert_eq!(
            sym.kind,
            SymbolKind::Key,
            "JSON symbol '{}' should have kind Key",
            sym.name
        );
    }
}

// ---------------------------------------------------------------------------
// CFG-03: TOML key paths extracted
// ---------------------------------------------------------------------------

#[test]
fn test_toml_key_paths_extracted() {
    let content =
        b"[package]\nname = \"my-crate\"\nversion = \"0.1.0\"\n\n[dependencies]\nserde = \"1.0\"\n";

    let result = process_file("Cargo.toml", content, LanguageId::Toml);
    assert_eq!(result.outcome, FileOutcome::Processed);

    let syms = &result.symbols;
    assert!(has_symbol(syms, "package"), "missing 'package'");
    assert!(has_symbol(syms, "package.name"), "missing 'package.name'");
    assert!(has_symbol(syms, "dependencies"), "missing 'dependencies'");
    assert!(
        has_symbol(syms, "dependencies.serde"),
        "missing 'dependencies.serde'"
    );

    for sym in syms {
        assert_eq!(
            sym.kind,
            SymbolKind::Key,
            "TOML symbol '{}' should have kind Key",
            sym.name
        );
    }
}

// ---------------------------------------------------------------------------
// CFG-04: Markdown sections extracted as dot-joined hierarchy
// ---------------------------------------------------------------------------

#[test]
fn test_markdown_sections_extracted() {
    let content = b"# Title\n\n## Getting Started\n\n### Prerequisites\n\nSome text.\n";

    let result = process_file("README.md", content, LanguageId::Markdown);
    assert_eq!(result.outcome, FileOutcome::Processed);

    let syms = &result.symbols;
    assert!(has_symbol(syms, "Title"), "missing 'Title'");
    assert!(
        has_symbol(syms, "Title.Getting Started"),
        "missing 'Title.Getting Started'"
    );
    assert!(
        has_symbol(syms, "Title.Getting Started.Prerequisites"),
        "missing 'Title.Getting Started.Prerequisites'"
    );

    for sym in syms {
        assert_eq!(
            sym.kind,
            SymbolKind::Section,
            "Markdown symbol '{}' should have kind Section",
            sym.name
        );
    }
}

// ---------------------------------------------------------------------------
// CFG-05: .env variables extracted with SymbolKind::Variable
// ---------------------------------------------------------------------------

#[test]
fn test_env_variables_extracted() {
    let content = b"DATABASE_URL=postgres://localhost/db\nPORT=3000\nSECRET_KEY=abc123\n";

    let result = process_file(".env", content, LanguageId::Env);
    assert_eq!(result.outcome, FileOutcome::Processed);

    let syms = &result.symbols;
    assert!(has_symbol(syms, "DATABASE_URL"), "missing 'DATABASE_URL'");
    assert!(has_symbol(syms, "PORT"), "missing 'PORT'");
    assert!(has_symbol(syms, "SECRET_KEY"), "missing 'SECRET_KEY'");

    for sym in syms {
        assert_eq!(
            sym.kind,
            SymbolKind::Variable,
            ".env symbol '{}' should have kind Variable",
            sym.name
        );
    }
}

// ---------------------------------------------------------------------------
// CFG-06: YAML nested keys extracted
// ---------------------------------------------------------------------------

#[test]
fn test_yaml_nested_keys_extracted() {
    let content = b"server:\n  host: localhost\n  port: 8080\ndatabase:\n  name: mydb\n";

    let result = process_file("config.yaml", content, LanguageId::Yaml);
    assert_eq!(result.outcome, FileOutcome::Processed);

    let syms = &result.symbols;
    assert!(has_symbol(syms, "server"), "missing 'server'");
    assert!(has_symbol(syms, "server.host"), "missing 'server.host'");
    assert!(has_symbol(syms, "server.port"), "missing 'server.port'");
    assert!(has_symbol(syms, "database"), "missing 'database'");
    assert!(has_symbol(syms, "database.name"), "missing 'database.name'");

    for sym in syms {
        assert_eq!(
            sym.kind,
            SymbolKind::Key,
            "YAML symbol '{}' should have kind Key",
            sym.name
        );
    }
}

// ---------------------------------------------------------------------------
// CFG-07: Duplicate Markdown headers disambiguated with #N suffix
// ---------------------------------------------------------------------------

#[test]
fn test_markdown_duplicate_headers_disambiguated() {
    // Two sibling ## Installation headers — the second should get #2 suffix.
    let content = b"# Guide\n\n## Installation\n\nFirst way.\n\n## Installation\n\nSecond way.\n";

    let result = process_file("guide.md", content, LanguageId::Markdown);
    assert_eq!(result.outcome, FileOutcome::Processed);

    let syms = &result.symbols;
    assert!(
        has_symbol(syms, "Guide.Installation"),
        "missing first 'Guide.Installation'"
    );
    assert!(
        has_symbol(syms, "Guide.Installation#2"),
        "missing disambiguated 'Guide.Installation#2'"
    );
}

// ---------------------------------------------------------------------------
// CFG-08: Literal-dot JSON key escaped to ~1
// ---------------------------------------------------------------------------

#[test]
fn test_json_literal_dot_key_escaped() {
    let content = br#"{"a.b": "value", "normal": 1}"#;

    let result = process_file("config.json", content, LanguageId::Json);
    assert_eq!(result.outcome, FileOutcome::Processed);

    let syms = &result.symbols;
    // Raw key "a.b" must be stored as "a~1b" (dot → ~1)
    assert!(
        has_symbol(syms, "a~1b"),
        "literal-dot key should be escaped to 'a~1b'; got: {:?}",
        syms.iter().map(|s| &s.name).collect::<Vec<_>>()
    );
    // Normal key is unaffected
    assert!(has_symbol(syms, "normal"), "missing 'normal'");
}

// ---------------------------------------------------------------------------
// CFG-09: Edit capability gating
// ---------------------------------------------------------------------------

#[test]
fn test_edit_capability_json_is_text_edit_safe() {
    let cap = extractor_for(&LanguageId::Json)
        .expect("JSON must have an extractor")
        .edit_capability();
    assert_eq!(
        cap,
        EditCapability::TextEditSafe,
        "JSON extractor should be TextEditSafe"
    );
}

#[test]
fn test_edit_capability_toml_is_structural_edit_safe() {
    let cap = extractor_for(&LanguageId::Toml)
        .expect("TOML must have an extractor")
        .edit_capability();
    assert_eq!(
        cap,
        EditCapability::StructuralEditSafe,
        "TOML extractor should be StructuralEditSafe"
    );
}

#[test]
fn test_edit_capability_yaml_is_text_edit_safe() {
    let cap = extractor_for(&LanguageId::Yaml)
        .expect("YAML must have an extractor")
        .edit_capability();
    assert_eq!(
        cap,
        EditCapability::TextEditSafe,
        "YAML extractor should be TextEditSafe"
    );
}

#[test]
fn test_edit_capability_markdown_is_text_edit_safe() {
    let cap = extractor_for(&LanguageId::Markdown)
        .expect("Markdown must have an extractor")
        .edit_capability();
    assert_eq!(
        cap,
        EditCapability::TextEditSafe,
        "Markdown extractor should be TextEditSafe"
    );
}

#[test]
fn test_edit_capability_env_is_structural_edit_safe() {
    let cap = extractor_for(&LanguageId::Env)
        .expect(".env must have an extractor")
        .edit_capability();
    assert_eq!(
        cap,
        EditCapability::StructuralEditSafe,
        ".env extractor should be StructuralEditSafe"
    );
}

// ---------------------------------------------------------------------------
// CFG-10: Malformed JSON produces Failed outcome
// ---------------------------------------------------------------------------

#[test]
fn test_malformed_json_produces_failed_outcome() {
    let result = process_file("bad.json", b"{invalid", LanguageId::Json);

    assert!(
        matches!(result.outcome, FileOutcome::Failed { .. }),
        "malformed JSON should produce FileOutcome::Failed, got: {:?}",
        result.outcome
    );
    assert!(
        result.symbols.is_empty(),
        "malformed JSON should produce no symbols"
    );
}

// ---------------------------------------------------------------------------
// Bonus regression: extractor_for returns None for non-config languages
// ---------------------------------------------------------------------------

#[test]
fn test_extractor_for_non_config_returns_none() {
    assert!(extractor_for(&LanguageId::Rust).is_none());
    assert!(extractor_for(&LanguageId::Python).is_none());
    assert!(extractor_for(&LanguageId::TypeScript).is_none());
}
