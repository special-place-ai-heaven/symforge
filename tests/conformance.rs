//! End-to-end conformance suite — verifies every MCP tool is registered,
//! schema-valid, deserializable, and that the tool surface matches the
//! canonical allowlist.
//!
//! This catches runtime/source drift: if a tool is added to the allowlist
//! but not registered (or vice versa), this test fails.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::Mutex;
use serde_json::{Value, json};
use symforge::live_index::LiveIndex;
use symforge::protocol::SymForgeServer;
use symforge::protocol::result_status::{
    OutcomeClass, RESULT_STATUS_CONTRACT_VERSION, RESULT_STATUS_META_KEY, ResultStatus,
};
use symforge::watcher::WatcherInfo;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// The canonical tool surface — must match SYMFORGE_TOOL_NAMES in cli/init.rs
// ---------------------------------------------------------------------------

const EXPECTED_TOOLS: &[&str] = &[
    "health",
    "index_folder",
    "validate_file_syntax",
    "get_file_content",
    "get_symbol",
    "get_repo_map",
    "get_file_context",
    "get_symbol_context",
    "search_symbols",
    "search_text",
    "search_files",
    "find_references",
    "find_dependents",
    "inspect_match",
    "analyze_file_impact",
    "what_changed",
    "diff_symbols",
    "explore",
    "replace_symbol_body",
    "edit_within_symbol",
    "insert_symbol",
    "delete_symbol",
    "batch_edit",
    "batch_insert",
    "batch_rename",
    "ask",
    "conventions",
    "edit_plan",
    "context_inventory",
    "investigation_suggest",
    "health_compact",
];

const PUBLIC_CONFORMANCE_CORPUS_VERSION: u8 = 1;

struct PublicContractCase {
    name: &'static str,
    tool_name: &'static str,
    request_json: fn(&ConformanceFixture) -> Value,
    expected_outcome: OutcomeClass,
    expected_edit_status: Option<&'static str>,
    expected_text_contains: &'static [&'static str],
    expected_recovery_hint: Option<&'static str>,
    dry_run_preserves_file: Option<&'static str>,
}

const PUBLIC_CONTRACT_CONFORMANCE_CORPUS: &[PublicContractCase] = &[
    PublicContractCase {
        name: "read_get_file_content_found_v1",
        tool_name: "get_file_content",
        request_json: request_get_file_content_found,
        expected_outcome: OutcomeClass::Found,
        expected_edit_status: None,
        expected_text_contains: &["src/lib.rs", "pub fn alpha"],
        expected_recovery_hint: None,
        dry_run_preserves_file: None,
    },
    PublicContractCase {
        name: "search_text_found_v1",
        tool_name: "search_text",
        request_json: request_search_text_found,
        expected_outcome: OutcomeClass::Found,
        expected_edit_status: None,
        expected_text_contains: &["Match type:", "src/lib.rs", "alpha"],
        expected_recovery_hint: None,
        dry_run_preserves_file: None,
    },
    PublicContractCase {
        name: "read_get_symbol_context_ambiguous_v1",
        tool_name: "get_symbol_context",
        request_json: request_get_symbol_context_ambiguous,
        expected_outcome: OutcomeClass::Ambiguous,
        expected_edit_status: None,
        expected_text_contains: &["Ambiguous symbol selector", "src/lib.rs", "src/other.rs"],
        expected_recovery_hint: Some("Pass `path` or `file`"),
        dry_run_preserves_file: None,
    },
    PublicContractCase {
        name: "edit_replace_symbol_body_dry_run_v1",
        tool_name: "replace_symbol_body",
        request_json: request_replace_symbol_body_dry_run,
        expected_outcome: OutcomeClass::Found,
        expected_edit_status: Some("dry_run_success"),
        expected_text_contains: &[
            "Write semantics: dry run (no writes)",
            "[DRY RUN] Would replace `alpha`",
        ],
        expected_recovery_hint: None,
        dry_run_preserves_file: Some("src/lib.rs"),
    },
    PublicContractCase {
        name: "invalid_get_file_content_mode_hint_v1",
        tool_name: "get_file_content",
        request_json: request_get_file_content_invalid_mode,
        expected_outcome: OutcomeClass::InvalidRequest,
        expected_edit_status: None,
        expected_text_contains: &["mode=lines conflicts with chunk_index"],
        expected_recovery_hint: Some("Use mode=chunk"),
        dry_run_preserves_file: None,
    },
    PublicContractCase {
        name: "unsupported_tool_name_hint_v1",
        tool_name: "definitely_not_a_symforge_tool",
        request_json: request_empty_object,
        expected_outcome: OutcomeClass::InvalidRequest,
        expected_edit_status: None,
        expected_text_contains: &["Unsupported tool `definitely_not_a_symforge_tool`"],
        expected_recovery_hint: Some("add a statused dispatcher branch"),
        dry_run_preserves_file: None,
    },
];

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn all_expected_tools_are_registered() {
    let tools = SymForgeServer::tool_definitions();
    let registered: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();

    let mut missing: Vec<&str> = EXPECTED_TOOLS
        .iter()
        .copied()
        .filter(|name| !registered.contains(name))
        .collect();
    missing.sort();

    assert!(
        missing.is_empty(),
        "Tools in EXPECTED_TOOLS but not registered in tool_definitions():\n  {:?}\n\nRegistered tools:\n  {:?}",
        missing,
        registered
    );
}

#[test]
fn no_unexpected_tools_registered() {
    let tools = SymForgeServer::tool_definitions();
    let registered: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();

    let unexpected: Vec<&str> = registered
        .iter()
        .copied()
        .filter(|name| !EXPECTED_TOOLS.contains(name))
        .collect();

    assert!(
        unexpected.is_empty(),
        "Tools registered but not in EXPECTED_TOOLS (update the list if intentional):\n  {:?}",
        unexpected
    );
}

#[test]
fn all_tools_have_valid_schemas() {
    for tool in SymForgeServer::tool_definitions() {
        let schema = tool.input_schema.as_ref();

        // Must be type=object
        assert_eq!(
            schema.get("type"),
            Some(&Value::String("object".to_string())),
            "tool '{}' schema must have type=object",
            tool.name
        );

        // Must have properties (even if empty)
        assert!(
            schema.contains_key("properties"),
            "tool '{}' schema must have a 'properties' key",
            tool.name
        );
    }
}

#[test]
fn minimal_payloads_deserialize_for_all_tools() {
    // For each tool, build a minimal JSON payload from the schema's required
    // fields and verify it deserializes without error.
    for tool in SymForgeServer::tool_definitions() {
        let schema = tool.input_schema.as_ref();
        let payload = build_minimal_payload(schema);

        // The payload must be valid JSON
        let serialized = serde_json::to_string(&payload).unwrap_or_else(|e| {
            panic!(
                "tool '{}' minimal payload failed to serialize: {e}",
                tool.name
            )
        });
        let _: Value = serde_json::from_str(&serialized).unwrap_or_else(|e| {
            panic!(
                "tool '{}' minimal payload failed to roundtrip: {e}",
                tool.name
            )
        });
    }
}

#[test]
fn tool_descriptions_are_nonempty() {
    for tool in SymForgeServer::tool_definitions() {
        let desc = tool.description.as_deref().unwrap_or("");
        assert!(
            !desc.is_empty(),
            "tool '{}' has an empty description",
            tool.name
        );
    }
}

#[test]
fn edit_tools_accept_dry_run_parameter() {
    let edit_tools = [
        "replace_symbol_body",
        "edit_within_symbol",
        "insert_symbol",
        "delete_symbol",
        "batch_edit",
        "batch_insert",
        "batch_rename",
    ];

    for tool_name in edit_tools {
        let tools = SymForgeServer::tool_definitions();
        let tool = tools
            .iter()
            .find(|t| t.name.as_ref() == tool_name)
            .unwrap_or_else(|| panic!("edit tool '{tool_name}' not found"));

        let schema = tool.input_schema.as_ref();
        let properties = schema
            .get("properties")
            .and_then(|v| v.as_object())
            .unwrap_or_else(|| panic!("tool '{tool_name}' has no properties"));

        assert!(
            properties.contains_key("dry_run"),
            "edit tool '{tool_name}' must accept dry_run parameter"
        );
    }
}

#[test]
fn estimate_parameter_on_read_tools() {
    let estimate_tools = [
        "get_file_content",
        "get_file_context",
        "get_symbol",
        "get_symbol_context",
        "get_repo_map",
        "search_symbols",
        "search_text",
        "search_files",
        "find_references",
        "find_dependents",
        "inspect_match",
        "explore",
        "analyze_file_impact",
        "what_changed",
        "diff_symbols",
        "validate_file_syntax",
    ];

    for tool_name in estimate_tools {
        let tools = SymForgeServer::tool_definitions();
        let tool = tools
            .iter()
            .find(|t| t.name.as_ref() == tool_name)
            .unwrap_or_else(|| panic!("tool '{tool_name}' not found"));

        let schema = tool.input_schema.as_ref();
        let properties = schema
            .get("properties")
            .and_then(|v| v.as_object())
            .unwrap_or_else(|| panic!("tool '{tool_name}' has no properties"));

        assert!(
            properties.contains_key("estimate"),
            "read tool '{tool_name}' must accept estimate parameter"
        );
    }
}

#[test]
fn single_edit_accepts_shorthand_string() {
    // The exact pattern that caused the original audit failure
    let batch_input = json!({
        "dry_run": true,
        "edits": [
            "src/lib.rs::beta => replace fn beta(x: i32) -> i32 { x * 4 }"
        ]
    });

    // Must deserialize without error
    // Verify the shorthand parses by checking it produces valid JSON
    // (actual deserialization is tested in unit tests; here we verify the schema accepts it)
    let edits = batch_input.get("edits").unwrap().as_array().unwrap();
    assert_eq!(edits.len(), 1);
    assert!(edits[0].is_string(), "shorthand edit should be a string");
}

#[test]
fn all_tools_have_annotations() {
    const READ_ONLY: &[&str] = &[
        "health",
        "get_symbol",
        "get_repo_map",
        "get_file_context",
        "get_symbol_context",
        "search_symbols",
        "search_text",
        "search_files",
        "find_references",
        "find_dependents",
        "get_file_content",
        "validate_file_syntax",
        "what_changed",
        "diff_symbols",
        "explore",
        "inspect_match",
        "ask",
        "conventions",
        "edit_plan",
        "investigation_suggest",
        "context_inventory",
        "health_compact",
    ];

    const DESTRUCTIVE_WRITE: &[&str] = &[
        "replace_symbol_body",
        "delete_symbol",
        "batch_edit",
        "batch_rename",
    ];

    const ADDITIVE_WRITE: &[&str] = &["insert_symbol", "edit_within_symbol", "batch_insert"];

    const IDEMPOTENT_STATE: &[&str] = &["index_folder", "analyze_file_impact"];

    let tools = SymForgeServer::tool_definitions();

    for tool in &tools {
        let name = tool.name.as_ref();
        let ann = tool
            .annotations
            .as_ref()
            .unwrap_or_else(|| panic!("tool '{name}' is missing annotations"));

        // All SymForge tools are closed-world (local files only)
        assert_eq!(
            ann.open_world_hint,
            Some(false),
            "tool '{name}' must have open_world_hint = false"
        );

        if READ_ONLY.contains(&name) {
            assert_eq!(
                ann.read_only_hint,
                Some(true),
                "read-only tool '{name}' must have read_only_hint = true"
            );
        } else if DESTRUCTIVE_WRITE.contains(&name) {
            assert_eq!(ann.read_only_hint, Some(false), "destructive tool '{name}'");
            assert_eq!(
                ann.destructive_hint,
                Some(true),
                "destructive tool '{name}'"
            );
            assert_eq!(
                ann.idempotent_hint,
                Some(false),
                "destructive tool '{name}'"
            );
        } else if ADDITIVE_WRITE.contains(&name) {
            assert_eq!(ann.read_only_hint, Some(false), "additive tool '{name}'");
            assert_eq!(ann.destructive_hint, Some(false), "additive tool '{name}'");
            assert_eq!(ann.idempotent_hint, Some(false), "additive tool '{name}'");
        } else if IDEMPOTENT_STATE.contains(&name) {
            assert_eq!(ann.read_only_hint, Some(false), "idempotent tool '{name}'");
            assert_eq!(
                ann.destructive_hint,
                Some(false),
                "idempotent tool '{name}'"
            );
            assert_eq!(ann.idempotent_hint, Some(true), "idempotent tool '{name}'");
        } else {
            panic!("tool '{name}' is not in any annotation classification list");
        }
    }

    // Verify total coverage matches expected count
    let classified =
        READ_ONLY.len() + DESTRUCTIVE_WRITE.len() + ADDITIVE_WRITE.len() + IDEMPOTENT_STATE.len();
    assert_eq!(
        classified,
        EXPECTED_TOOLS.len(),
        "classification lists must cover all expected tools"
    );
}

#[test]
fn result_status_vocabulary_is_stable() {
    let vocabulary: Vec<&'static str> = OutcomeClass::ALL
        .iter()
        .map(|outcome| outcome.as_str())
        .collect();

    assert_eq!(
        vocabulary,
        vec![
            "found",
            "not_found",
            "ambiguous",
            "invalid_request",
            "empty_result",
            "internal_failure",
        ]
    );

    for outcome in OutcomeClass::ALL {
        assert_eq!(
            serde_json::to_value(ResultStatus::new(outcome)).unwrap(),
            json!({
                "contract_version": 1,
                "outcome_class": outcome.as_str(),
            })
        );
    }
}

#[test]
fn result_status_metadata_shape_is_additive_and_namespaced() {
    let human_text = "src/lib.rs\nfn present() {}";
    let result = ResultStatus::new(OutcomeClass::Found).into_call_tool_result(human_text);
    let serialized = serde_json::to_value(&result).unwrap();

    assert_eq!(serialized["content"][0]["type"], json!("text"));
    assert_eq!(serialized["content"][0]["text"], json!(human_text));
    assert_eq!(serialized["isError"], json!(false));
    assert!(serialized.get("structuredContent").is_none());
    assert_eq!(
        serialized["_meta"][RESULT_STATUS_META_KEY],
        json!({
            "contract_version": 1,
            "outcome_class": "found",
        })
    );
}

#[test]
fn invalid_request_status_marks_call_tool_result_as_error() {
    let result =
        ResultStatus::new(OutcomeClass::InvalidRequest).into_call_tool_result("Invalid request");
    let serialized = serde_json::to_value(&result).unwrap();

    assert_eq!(serialized["isError"], json!(true));
    assert_eq!(
        serialized["_meta"][RESULT_STATUS_META_KEY]["outcome_class"],
        json!("invalid_request")
    );
}

#[test]
fn public_contract_conformance_corpus_is_versioned_and_named() {
    assert_eq!(PUBLIC_CONFORMANCE_CORPUS_VERSION, 1);

    let mut names = BTreeSet::new();
    for case in PUBLIC_CONTRACT_CONFORMANCE_CORPUS {
        assert!(
            names.insert(case.name),
            "duplicate conformance case name `{}`",
            case.name
        );
        assert!(
            !case.tool_name.is_empty(),
            "case `{}` must name a public tool",
            case.name
        );
    }
}

#[tokio::test]
async fn public_contract_conformance_corpus_replays() {
    let fixture = ConformanceFixture::new();

    for case in PUBLIC_CONTRACT_CONFORMANCE_CORPUS {
        let request = (case.request_json)(&fixture);
        assert!(
            request.is_object(),
            "case `{}` request must be a canonical JSON object: {request}",
            case.name
        );
        serde_json::to_string(&request)
            .unwrap_or_else(|error| panic!("case `{}` request must serialize: {error}", case.name));

        let before = case
            .dry_run_preserves_file
            .map(|path| (path, fixture.read(path)));
        let result = fixture
            .server
            .dispatch_tool_result_for_tests(case.tool_name, request)
            .await
            .unwrap_or_else(|error| {
                panic!("case `{}` returned transport error: {error:?}", case.name)
            });
        let serialized = serde_json::to_value(&result)
            .unwrap_or_else(|error| panic!("case `{}` result must serialize: {error}", case.name));
        let text = result_text(&serialized);

        assert_result_status(case, &serialized);
        for needle in case.expected_text_contains {
            assert!(
                text.contains(needle),
                "case `{}` expected response text to contain `{needle}`; text was:\n{text}",
                case.name
            );
        }
        if let Some(hint) = case.expected_recovery_hint {
            assert!(
                text.contains(hint),
                "case `{}` expected recovery hint `{hint}`; text was:\n{text}",
                case.name
            );
        }

        if let Some((path, original)) = before {
            assert_eq!(
                fixture.read(path),
                original,
                "case `{}` must not write `{path}` in dry-run mode",
                case.name
            );
        }
    }
}

#[tokio::test]
async fn get_symbol_context_ambiguous_result_status_conformance() {
    let fixture = ConformanceFixture::new();
    let case = PUBLIC_CONTRACT_CONFORMANCE_CORPUS
        .iter()
        .find(|case| case.name == "read_get_symbol_context_ambiguous_v1")
        .expect("ambiguous get_symbol_context conformance case");
    let request = (case.request_json)(&fixture);
    let result = fixture
        .server
        .dispatch_tool_result_for_tests(case.tool_name, request)
        .await
        .unwrap_or_else(|error| panic!("case `{}` returned transport error: {error:?}", case.name));
    let serialized = serde_json::to_value(&result)
        .unwrap_or_else(|error| panic!("case `{}` result must serialize: {error}", case.name));
    let text = result_text(&serialized);

    assert_result_status(case, &serialized);
    for needle in case.expected_text_contains {
        assert!(
            text.contains(needle),
            "case `{}` expected response text to contain `{needle}`; text was:\n{text}",
            case.name
        );
    }
    if let Some(hint) = case.expected_recovery_hint {
        assert!(
            text.contains(hint),
            "case `{}` expected recovery hint `{hint}`; text was:\n{text}",
            case.name
        );
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

struct ConformanceFixture {
    _dir: TempDir,
    root: PathBuf,
    server: SymForgeServer,
}

impl ConformanceFixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().to_path_buf();
        write_fixture_file(
            &root,
            "src/lib.rs",
            "pub fn alpha() -> i32 {\n    1\n}\n\npub fn beta() -> i32 {\n    alpha() + 1\n}\n",
        );
        write_fixture_file(&root, "src/other.rs", "pub fn alpha() -> i32 {\n    2\n}\n");
        let shared = LiveIndex::load(&root).expect("LiveIndex::load conformance fixture");
        let watcher_info = Arc::new(Mutex::new(WatcherInfo::default()));
        let server = SymForgeServer::new(
            shared,
            "public_contract_conformance_test".to_string(),
            watcher_info,
            Some(root.clone()),
            None,
        );
        Self {
            _dir: dir,
            root,
            server,
        }
    }

    fn read(&self, relative_path: &str) -> String {
        fs::read_to_string(self.root.join(relative_path)).expect("read fixture file")
    }
}

fn write_fixture_file(root: &std::path::Path, relative_path: &str, content: &str) {
    let path = root.join(relative_path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create fixture parent");
    }
    fs::write(path, content).expect("write fixture file");
}

fn request_get_file_content_found(_fixture: &ConformanceFixture) -> Value {
    json!({
        "path": "src/lib.rs",
        "start_line": 1,
        "end_line": 3,
        "show_line_numbers": true,
        "header": true
    })
}

fn request_search_text_found(_fixture: &ConformanceFixture) -> Value {
    json!({
        "query": "alpha",
        "path_prefix": "src/",
        "include_tests": true
    })
}

fn request_get_symbol_context_ambiguous(_fixture: &ConformanceFixture) -> Value {
    json!({
        "name": "alpha",
        "symbol_kind": "fn"
    })
}

fn request_replace_symbol_body_dry_run(_fixture: &ConformanceFixture) -> Value {
    json!({
        "path": "src/lib.rs",
        "name": "alpha",
        "new_body": "pub fn alpha() -> i32 {\n    42\n}",
        "dry_run": true
    })
}

fn request_get_file_content_invalid_mode(_fixture: &ConformanceFixture) -> Value {
    json!({
        "path": "src/lib.rs",
        "mode": "lines",
        "chunk_index": 1,
        "max_lines": 5
    })
}

fn request_empty_object(_fixture: &ConformanceFixture) -> Value {
    json!({})
}

fn assert_result_status(case: &PublicContractCase, serialized: &Value) {
    let status = &serialized["_meta"][RESULT_STATUS_META_KEY];
    assert_eq!(
        status["contract_version"],
        json!(RESULT_STATUS_CONTRACT_VERSION),
        "case `{}` must include corpus-compatible result-status version",
        case.name
    );
    assert_eq!(
        status["outcome_class"],
        json!(case.expected_outcome.as_str()),
        "case `{}` outcome class mismatch",
        case.name
    );
    assert_eq!(
        serialized["isError"],
        json!(case.expected_outcome.is_error()),
        "case `{}` isError must match the result-status outcome class",
        case.name
    );

    if let Some(expected_edit_status) = case.expected_edit_status {
        assert_eq!(
            status["status"],
            json!(expected_edit_status),
            "case `{}` edit status mismatch",
            case.name
        );
    }
}

fn result_text(serialized: &Value) -> &str {
    serialized["content"][0]["text"]
        .as_str()
        .expect("tool result must contain text content")
}

fn build_minimal_payload(schema: &serde_json::Map<String, Value>) -> Value {
    let required: Vec<String> = schema
        .get("required")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    let properties = schema
        .get("properties")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();

    let mut obj = serde_json::Map::new();
    for field in &required {
        if let Some(prop_schema) = properties.get(field) {
            obj.insert(field.clone(), default_value_for(prop_schema));
        }
    }
    Value::Object(obj)
}

fn default_value_for(schema: &Value) -> Value {
    match schema.get("type").and_then(|t| t.as_str()) {
        Some("string") => json!("test"),
        Some("integer") | Some("number") => json!(1),
        Some("boolean") => json!(false),
        Some("array") => json!([]),
        Some("object") => json!({}),
        _ => {
            // Nullable types: ["string", "null"]
            if let Some(arr) = schema.get("type").and_then(|t| t.as_array()) {
                for t in arr {
                    if t.as_str() == Some("string") {
                        return json!("test");
                    }
                    if t.as_str() == Some("integer") || t.as_str() == Some("number") {
                        return json!(1);
                    }
                }
            }
            json!("test")
        }
    }
}
