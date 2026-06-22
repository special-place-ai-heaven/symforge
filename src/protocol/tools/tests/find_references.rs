use super::{make_file, make_live_index_ready, make_server, make_symbol};
use crate::domain::SymbolKind;
use rmcp::handler::server::wrapper::Parameters;

#[tokio::test]
async fn qualified_call_via_full_path_returned() {
    let target = make_file(
        "src/module.rs",
        b"pub struct TypeA;\nimpl TypeA { pub fn new() -> Self { Self } }\n",
        vec![make_symbol("TypeA", SymbolKind::Struct, 1, 1)],
    );
    let caller = make_file(
        "src/caller.rs",
        b"fn build() { let _ = crate::module::TypeA::new(); }\n",
        vec![make_symbol("build", SymbolKind::Function, 1, 1)],
    );
    let server = make_server(make_live_index_ready(vec![target, caller]));

    let result = server
        .find_references(Parameters(super::super::FindReferencesInput {
            name: "TypeA".to_string(),
            kind: None,
            path: None,
            symbol_kind: None,
            symbol_line: None,
            limit: None,
            max_per_file: None,
            compact: None,
            mode: None,
            direction: None,
            estimate: None,
            max_tokens: None,
            project: None,
            projects: None,
        }))
        .await;

    assert!(
        result.contains("src/caller.rs"),
        "expected fully-qualified TypeA usage in caller file: {result}"
    );
    assert!(
        result.contains("crate::module::TypeA::new()"),
        "expected call-site context in output: {result}"
    );
    assert!(
        result.contains("[qualified-path scan: confident]"),
        "expected confidence label for byte-scanned hit: {result}"
    );
}
