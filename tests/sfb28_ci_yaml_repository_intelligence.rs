// Server-only integration test: depends on a `#[cfg(feature = "server")]`
// module (protocol/daemon/cli/sidecar/watcher/analytics). Gating the whole
// file keeps `--no-default-features --features embed --all-targets` compiling.
#![cfg(feature = "server")]

use std::fs;
use std::path::{Path, PathBuf};

use symforge::domain::{FileOutcome, LanguageId, SymbolKind, SymbolRecord};
use symforge::live_index::LiveIndex;
use symforge::parsing::process_file;
use symforge::protocol::format;
use tempfile::TempDir;

const CORPUS_ROOT: &str = "tests/fixtures/repository_intelligence/ci_yaml";
const NORMAL_FIXTURE: &str = "normal/github_ci.yml";
const LARGE_FIXTURE: &str = "large/matrix_release.yml";
const MALFORMED_FIXTURE: &str = "malformed/unclosed_step.yml";
const EMPTY_FIXTURE: &str = "edge/empty.yml";
const COMMENTS_ONLY_FIXTURE: &str = "edge/comments_only.yml";

fn fixture_path(relative: &str) -> PathBuf {
    Path::new(CORPUS_ROOT).join(relative)
}

fn fixture_bytes(relative: &str) -> Vec<u8> {
    fs::read(fixture_path(relative)).expect("read CI/YAML fixture")
}

fn process_fixture(relative: &str) -> symforge::domain::FileProcessingResult {
    let bytes = fixture_bytes(relative);
    process_file(relative, &bytes, LanguageId::Yaml)
}

fn symbol_named<'a>(symbols: &'a [SymbolRecord], name: &str) -> &'a SymbolRecord {
    symbols
        .iter()
        .find(|symbol| symbol.name == name)
        .unwrap_or_else(|| {
            panic!(
                "missing symbol {name}; got {:?}",
                symbols
                    .iter()
                    .map(|symbol| symbol.name.as_str())
                    .collect::<Vec<_>>()
            )
        })
}

fn assert_fact(result: &symforge::domain::FileProcessingResult, name: &str, line: u32) {
    let symbol = symbol_named(&result.symbols, name);
    assert_eq!(symbol.kind, SymbolKind::Key, "{name} should be a key fact");
    assert_eq!(
        symbol.line_range.0 + 1,
        line,
        "{name} should resolve to source line {line}, got {:?}",
        symbol.line_range
    );
}

fn assert_no_ci_facts(result: &symforge::domain::FileProcessingResult) {
    assert!(
        result
            .symbols
            .iter()
            .all(|symbol| !symbol.name.starts_with("ci.workflow.")),
        "edge/malformed CI/YAML files must not invent workflow facts: {:?}",
        result
            .symbols
            .iter()
            .map(|symbol| symbol.name.as_str())
            .collect::<Vec<_>>()
    );
}

struct IndexedWorkflowProject {
    _dir: TempDir,
    shared: symforge::live_index::SharedIndex,
}

impl IndexedWorkflowProject {
    fn new() -> Self {
        let dir = TempDir::new().expect("temp workflow project");
        write_fixture(dir.path(), "workflows/ci.yml", NORMAL_FIXTURE);
        write_fixture(dir.path(), "workflows/release.yml", LARGE_FIXTURE);

        let shared = LiveIndex::load(dir.path()).expect("load CI/YAML fixture project");
        Self { _dir: dir, shared }
    }
}

fn write_fixture(root: &Path, destination: &str, source_fixture: &str) {
    let destination = root.join(destination);
    fs::create_dir_all(destination.parent().expect("workflow parent"))
        .expect("create workflow dir");
    fs::write(destination, fixture_bytes(source_fixture)).expect("write workflow fixture");
}

#[test]
fn normal_ci_yaml_emits_workflow_facts_with_source_ranges() {
    let result = process_fixture(NORMAL_FIXTURE);
    assert_eq!(result.outcome, FileOutcome::Processed);

    for (name, line) in [
        ("ci.workflow.name=CI", 1),
        ("ci.workflow.trigger.push", 4),
        ("ci.workflow.env.CARGO_TERM_COLOR=always", 13),
        ("ci.workflow.job.rust.needs=conventional_commits", 30),
        ("ci.workflow.job.rust.runs-on=ubuntu-latest", 31),
        (
            "ci.workflow.job.rust.step[1].uses=dtolnay/rust-toolchain@master",
            35,
        ),
        ("ci.workflow.job.rust.step[2].run=cargo check", 39),
        ("ci.workflow.job.npm.step[2].working-directory=npm", 52),
        ("ci.workflow.job.npm.step[2].run=npm test", 53),
    ] {
        assert_fact(&result, name, line);
    }
}

#[test]
fn large_ci_yaml_emits_permission_and_matrix_facts_with_bounded_arrays() {
    let result = process_fixture(LARGE_FIXTURE);
    assert_eq!(result.outcome, FileOutcome::Processed);

    for (name, line) in [
        ("ci.workflow.name=Matrix Release", 1),
        ("ci.workflow.permission.contents=write", 13),
        (
            "ci.workflow.job.matrix_release.strategy.fail-fast=false",
            20,
        ),
        (
            "ci.workflow.job.matrix_release.strategy.matrix.include[0].target=x86_64-unknown-linux-gnu",
            25,
        ),
        (
            "ci.workflow.job.matrix_release.strategy.matrix.include[19].target=wasm32-wasip1",
            101,
        ),
        (
            "ci.workflow.job.matrix_release.step[2].run=cargo build --release --target ${{ matrix.target }}",
            117,
        ),
        (
            "ci.workflow.job.matrix_release.step[4].uses=actions/upload-artifact@v4",
            122,
        ),
    ] {
        assert_fact(&result, name, line);
    }

    assert!(
        result.symbols.iter().all(|symbol| !symbol
            .name
            .starts_with("ci.workflow.job.matrix_release.strategy.matrix.include[20]")),
        "CI/YAML matrix facts should honor the existing array cap"
    );
}

#[test]
fn ci_yaml_facts_are_searchable_outline_and_resolvable_through_existing_surfaces() {
    let project = IndexedWorkflowProject::new();
    let index = project.shared.read();

    let symbol_search = format::search_symbols_result(&index, "ci.workflow.job.rust.step[2].run");
    assert!(
        symbol_search.contains("ci.workflow.job.rust.step[2].run=cargo check"),
        "search_symbols should surface CI/YAML fact symbols: {symbol_search}"
    );
    assert!(
        symbol_search.contains("workflows/ci.yml"),
        "search_symbols should identify the workflow file: {symbol_search}"
    );

    let outline = format::file_outline(&index, "workflows/ci.yml");
    assert!(
        outline.contains("ci.workflow.job.npm.step[2].working-directory=npm"),
        "get_file_context/file_outline should explain workflow facts: {outline}"
    );

    let detail = format::symbol_detail(
        &index,
        "workflows/ci.yml",
        "ci.workflow.job.npm.step[2].working-directory=npm",
        Some("key"),
    );
    assert!(
        detail.contains("working-directory: npm"),
        "get_symbol should resolve CI/YAML facts to source bytes: {detail}"
    );
    assert!(
        detail.contains("[key, lines 52-52"),
        "get_symbol should report the source line for repeated step keys: {detail}"
    );

    let text = format::search_text_result(&index, "actions/upload-artifact@v4");
    assert!(
        text.contains("workflows/release.yml") && text.contains("122:"),
        "search_text should still find workflow content through the existing surface: {text}"
    );
}

#[test]
fn malformed_and_empty_ci_yaml_do_not_invent_workflow_facts() {
    let malformed = process_fixture(MALFORMED_FIXTURE);
    assert!(matches!(malformed.outcome, FileOutcome::Failed { .. }));
    assert_no_ci_facts(&malformed);

    for fixture in [EMPTY_FIXTURE, COMMENTS_ONLY_FIXTURE] {
        let result = process_fixture(fixture);
        assert_eq!(result.outcome, FileOutcome::Processed);
        assert_no_ci_facts(&result);
    }
}
