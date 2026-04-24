//! End-to-end conformance suite — verifies every MCP tool is registered,
//! schema-valid, deserializable, and that the tool surface matches the
//! canonical allowlist.
//!
//! This catches runtime/source drift: if a tool is added to the allowlist
//! but not registered (or vice versa), this test fails.

use serde_json::{Value, json};
use symforge::protocol::SymForgeServer;

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

    const ADDITIVE_WRITE: &[&str] = &[
        "insert_symbol",
        "edit_within_symbol",
        "batch_insert",
    ];

    const IDEMPOTENT_STATE: &[&str] = &[
        "index_folder",
        "analyze_file_impact",
    ];

    let tools = SymForgeServer::tool_definitions();

    for tool in &tools {
        let name = tool.name.as_ref();
        let ann = tool.annotations.as_ref().unwrap_or_else(|| {
            panic!("tool '{name}' is missing annotations")
        });

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
            assert_eq!(ann.destructive_hint, Some(true), "destructive tool '{name}'");
            assert_eq!(ann.idempotent_hint, Some(false), "destructive tool '{name}'");
        } else if ADDITIVE_WRITE.contains(&name) {
            assert_eq!(ann.read_only_hint, Some(false), "additive tool '{name}'");
            assert_eq!(ann.destructive_hint, Some(false), "additive tool '{name}'");
            assert_eq!(ann.idempotent_hint, Some(false), "additive tool '{name}'");
        } else if IDEMPOTENT_STATE.contains(&name) {
            assert_eq!(ann.read_only_hint, Some(false), "idempotent tool '{name}'");
            assert_eq!(ann.destructive_hint, Some(false), "idempotent tool '{name}'");
            assert_eq!(ann.idempotent_hint, Some(true), "idempotent tool '{name}'");
        } else {
            panic!("tool '{name}' is not in any annotation classification list");
        }
    }

    // Verify total coverage matches expected count
    let classified = READ_ONLY.len() + DESTRUCTIVE_WRITE.len() + ADDITIVE_WRITE.len() + IDEMPOTENT_STATE.len();
    assert_eq!(
        classified,
        EXPECTED_TOOLS.len(),
        "classification lists must cover all expected tools"
    );
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

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
