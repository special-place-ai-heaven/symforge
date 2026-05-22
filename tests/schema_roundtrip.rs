//! Schema roundtrip tests — MCP contract parity.
//!
//! For every tool exposed by SymForgeServer, this module verifies that:
//!
//! 1. The tool appears in `tool_definitions()` with a non-empty `input_schema`.
//! 2. The `input_schema` is valid JSON Schema (has `"type": "object"` at root).
//! 3. The schema round-trips: `serde_json::to_value → serde_json::from_value`
//!    on the schema yields an identical value (no lossy transformations).
//! 4. A minimal valid JSON payload (all required fields present, optionals omitted)
//!    deserializes successfully into the tool's input struct.
//! 5. Required properties declared in the schema match what the Rust struct actually
//!    requires (no silent drift between schema and deserialization).
//!
//! These tests catch schema regressions that would break MCP client compatibility
//! without needing a live server.

use serde_json::{Map, Value, json};
use symforge::protocol::SymForgeServer;
use symforge::protocol::tools::*;

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Find a tool definition by name from the server's advertised tool list.
fn tool_schema(name: &str) -> Map<String, Value> {
    let tools = SymForgeServer::tool_definitions();
    let tool = tools
        .iter()
        .find(|t| t.name.as_ref() == name)
        .unwrap_or_else(|| panic!("tool '{}' not found in tool_definitions()", name));
    tool.input_schema.as_ref().clone()
}

/// Assert schema is a valid JSON Schema object with `"type": "object"`.
fn assert_object_schema(schema: &Map<String, Value>, tool_name: &str) {
    assert_eq!(
        schema.get("type"),
        Some(&Value::String("object".to_string())),
        "{tool_name}: input_schema must have \"type\": \"object\""
    );
}

/// Assert schema round-trips through serde_json without data loss.
fn assert_schema_roundtrip(schema: &Map<String, Value>, tool_name: &str) {
    let value = Value::Object(schema.clone());
    let serialized = serde_json::to_string(&value).expect("serialize schema");
    let deserialized: Value = serde_json::from_str(&serialized).expect("deserialize schema");
    assert_eq!(
        value, deserialized,
        "{tool_name}: schema does not roundtrip through JSON serialization"
    );
}

/// Return the `required` array from a schema, or empty vec if absent.
fn schema_required_fields(schema: &Map<String, Value>) -> Vec<String> {
    schema
        .get("required")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

/// Return all property names from a schema.
fn schema_property_names(schema: &Map<String, Value>) -> Vec<String> {
    schema
        .get("properties")
        .and_then(|v| v.as_object())
        .map(|obj| obj.keys().cloned().collect())
        .unwrap_or_default()
}

/// Build a minimal JSON object with only required fields populated with
/// type-appropriate zero values.
fn minimal_payload(schema: &Map<String, Value>) -> Value {
    let required = schema_required_fields(schema);
    let properties = schema
        .get("properties")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();

    let mut obj = serde_json::Map::new();
    for field in &required {
        if let Some(prop_schema) = properties.get(field) {
            let default_value = default_for_schema(prop_schema);
            obj.insert(field.clone(), default_value);
        }
    }
    Value::Object(obj)
}

/// Produce a type-appropriate default value from a JSON Schema property.
fn default_for_schema(schema: &Value) -> Value {
    match schema.get("type").and_then(|t| t.as_str()) {
        Some("string") => Value::String(String::new()),
        Some("integer") | Some("number") => json!(0),
        Some("boolean") => json!(false),
        Some("array") => json!([]),
        Some("object") => json!({}),
        _ => Value::Null,
    }
}

// ─── Per-tool roundtrip tests ────────────────────────────────────────────────

macro_rules! schema_roundtrip_test {
    ($test_name:ident, $tool_name:expr, $input_type:ty) => {
        #[test]
        fn $test_name() {
            let schema = tool_schema($tool_name);

            // 1. Valid object schema
            assert_object_schema(&schema, $tool_name);

            // 2. Roundtrip
            assert_schema_roundtrip(&schema, $tool_name);

            // 3. Has properties
            let props = schema_property_names(&schema);
            assert!(
                !props.is_empty(),
                "{}: schema should declare at least one property",
                $tool_name
            );

            // 4. Minimal payload deserializes
            let payload = minimal_payload(&schema);
            let result: Result<$input_type, _> =
                serde_json::from_value(payload.clone());
            if let Err(e) = result {
                panic!(
                    "{}: minimal payload {payload} failed to deserialize: {e}",
                    $tool_name
                );
            }

            // 5. Required fields in schema are actually required by serde
            //    (attempt deserialization with each required field removed)
            let required = schema_required_fields(&schema);
            for field in &required {
                let mut incomplete = payload.as_object().unwrap().clone();
                incomplete.remove(field);
                let bad: Result<$input_type, _> =
                    serde_json::from_value(Value::Object(incomplete));
                if bad.is_ok() {
                    panic!(
                        "{}: field '{}' is declared required in schema but serde accepts without it",
                        $tool_name, field
                    );
                }
            }
        }
    };
}

schema_roundtrip_test!(roundtrip_get_symbol, "get_symbol", GetSymbolInput);
schema_roundtrip_test!(
    roundtrip_search_symbols,
    "search_symbols",
    SearchSymbolsInput
);
schema_roundtrip_test!(roundtrip_search_text, "search_text", SearchTextInput);
schema_roundtrip_test!(roundtrip_search_files, "search_files", SearchFilesInput);

#[test]
fn search_files_debug_ranking_schema_and_roundtrip() {
    let schema = tool_schema("search_files");
    let properties = schema
        .get("properties")
        .and_then(Value::as_object)
        .expect("search_files schema properties");
    assert!(
        properties.contains_key("debug_ranking"),
        "search_files schema must advertise debug_ranking"
    );

    let input: SearchFilesInput =
        serde_json::from_value(json!({"query": "routes", "debug_ranking": true}))
            .expect("debug_ranking payload deserializes");
    assert_eq!(input.debug_ranking, Some(true));

    let roundtrip = serde_json::to_value(&input).expect("serialize SearchFilesInput");
    assert_eq!(roundtrip.get("debug_ranking"), Some(&Value::Bool(true)));
}

schema_roundtrip_test!(roundtrip_index_folder, "index_folder", IndexFolderInput);

#[test]
fn index_folder_idempotency_key_is_optional_and_roundtrips() {
    let schema = tool_schema("index_folder");
    let props = schema_property_names(&schema);
    assert!(
        props.iter().any(|name| name == "idempotency_key"),
        "index_folder schema should advertise optional idempotency_key"
    );
    assert!(
        !schema_required_fields(&schema)
            .iter()
            .any(|name| name == "idempotency_key"),
        "index_folder idempotency_key must be optional for backward compatibility"
    );

    let old_payload: IndexFolderInput =
        serde_json::from_value(json!({ "path": "src" })).expect("old payload deserializes");
    assert_eq!(old_payload.idempotency_key, None);

    let new_payload: IndexFolderInput =
        serde_json::from_value(json!({ "path": "src", "idempotency_key": "schema-key" }))
            .expect("new payload deserializes");
    assert_eq!(new_payload.idempotency_key.as_deref(), Some("schema-key"));

    let roundtrip = serde_json::to_value(&new_payload).expect("serialize IndexFolderInput");
    assert_eq!(
        roundtrip.get("idempotency_key"),
        Some(&Value::String("schema-key".to_string()))
    );
}

#[test]
fn edit_mutation_idempotency_key_is_optional_in_tool_schemas() {
    for tool_name in [
        "replace_symbol_body",
        "insert_symbol",
        "delete_symbol",
        "edit_within_symbol",
        "batch_edit",
        "batch_rename",
        "batch_insert",
    ] {
        let schema = tool_schema(tool_name);
        let props = schema_property_names(&schema);
        assert!(
            props.iter().any(|name| name == "idempotency_key"),
            "{tool_name} schema should advertise optional idempotency_key"
        );
        assert!(
            !schema_required_fields(&schema)
                .iter()
                .any(|name| name == "idempotency_key"),
            "{tool_name} idempotency_key must be optional for backward compatibility"
        );
    }
}

schema_roundtrip_test!(roundtrip_what_changed, "what_changed", WhatChangedInput);
schema_roundtrip_test!(
    roundtrip_get_file_content,
    "get_file_content",
    GetFileContentInput
);

#[test]
fn get_file_content_schema_includes_max_tokens() {
    let schema = tool_schema("get_file_content");
    let props = schema_property_names(&schema);
    assert!(
        props.iter().any(|name| name == "max_tokens"),
        "get_file_content schema should advertise max_tokens"
    );
}

schema_roundtrip_test!(
    roundtrip_find_references,
    "find_references",
    FindReferencesInput
);
schema_roundtrip_test!(
    roundtrip_find_dependents,
    "find_dependents",
    FindDependentsInput
);
schema_roundtrip_test!(roundtrip_get_repo_map, "get_repo_map", GetRepoMapInput);
schema_roundtrip_test!(
    roundtrip_get_file_context,
    "get_file_context",
    GetFileContextInput
);
schema_roundtrip_test!(
    roundtrip_get_symbol_context,
    "get_symbol_context",
    GetSymbolContextInput
);
schema_roundtrip_test!(
    roundtrip_analyze_file_impact,
    "analyze_file_impact",
    AnalyzeFileImpactInput
);
schema_roundtrip_test!(roundtrip_inspect_match, "inspect_match", InspectMatchInput);
schema_roundtrip_test!(roundtrip_explore, "explore", ExploreInput);
schema_roundtrip_test!(roundtrip_ask, "ask", SmartQueryInput);
schema_roundtrip_test!(roundtrip_edit_plan, "edit_plan", EditPlanInput);
schema_roundtrip_test!(
    roundtrip_investigation_suggest,
    "investigation_suggest",
    InvestigationInput
);
schema_roundtrip_test!(roundtrip_diff_symbols, "diff_symbols", DiffSymbolsInput);

// ─── Aggregate sanity checks ────────────────────────────────────────────────

#[test]
fn all_tools_have_object_schemas() {
    for tool in SymForgeServer::tool_definitions() {
        let schema = tool.input_schema.as_ref();
        assert_eq!(
            schema.get("type"),
            Some(&Value::String("object".to_string())),
            "tool '{}' must have input_schema type=object",
            tool.name
        );
    }
}

#[test]
fn all_tool_schemas_roundtrip() {
    for tool in SymForgeServer::tool_definitions() {
        let schema = tool.input_schema.as_ref();
        let value = Value::Object(schema.clone());
        let rt: Value = serde_json::from_str(&serde_json::to_string(&value).unwrap()).unwrap();
        assert_eq!(
            value, rt,
            "tool '{}' schema does not survive JSON roundtrip",
            tool.name
        );
    }
}

#[test]
fn no_parameterized_tool_has_empty_properties() {
    // Tools with zero parameters (like context_inventory) are allowed to have
    // empty properties. This test catches tools that *should* have properties
    // but accidentally lost them.
    let zero_param_tools = [
        "context_inventory",
        "conventions",
        "health",
        "health_compact",
    ];
    for tool in SymForgeServer::tool_definitions() {
        if zero_param_tools.contains(&tool.name.as_ref()) {
            continue;
        }
        let schema = tool.input_schema.as_ref();
        let has_properties = schema
            .get("properties")
            .and_then(|v| v.as_object())
            .map(|p| !p.is_empty())
            .unwrap_or(false);
        assert!(
            has_properties,
            "tool '{}' has no properties in input_schema — is the schema missing?",
            tool.name
        );
    }
}
