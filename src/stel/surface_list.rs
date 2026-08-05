//! Production compact-surface `tools/list` entries (Phase 1 S3).

use std::borrow::Cow;
use std::sync::Arc;

use rmcp::model::Tool;
use schemars::{JsonSchema, schema_for};
use serde_json::{Map, Value, json};

use super::surface::CompactSurfaceTool;
use super::types::{StelEditRequest, StelRequest, StelStatusRequest};

/// Drop serialized bytes no MCP client needs: the per-tool `$schema`
/// declaration and the `"default": null` schemars emits for every optional
/// field. Both are inert for routing and for validation, and together they are
/// ~8% of the `tools/list` payload on the full surface.
pub(crate) fn strip_schema_noise(root: &mut Map<String, Value>) {
    // Root-only: a nested `$schema` key would be a struct FIELD name under
    // `properties`, not the JSON Schema keyword.
    root.remove("$schema");
    for value in root.values_mut() {
        strip_null_defaults(value);
    }
}

fn strip_null_defaults(value: &mut Value) {
    match value {
        Value::Object(map) => {
            // Safe against a field literally named `default`: under
            // `properties` that entry's value is a schema object, never null.
            map.retain(|key, val| !(key == "default" && val.is_null()));
            for child in map.values_mut() {
                strip_null_defaults(child);
            }
        }
        Value::Array(items) => items.iter_mut().for_each(strip_null_defaults),
        _ => {}
    }
}

fn schema_object<T: JsonSchema>() -> Arc<Map<String, Value>> {
    let schema = schema_for!(T);
    let value = serde_json::to_value(schema).expect("STEL input schema must serialize");
    let mut root = value
        .as_object()
        .expect("STEL input schema root must be a JSON object")
        .clone();
    strip_schema_noise(&mut root);
    Arc::new(root)
}

fn surface_tool(
    name: &'static str,
    description: &'static str,
    input_schema: Arc<Map<String, Value>>,
) -> Tool {
    let mut tool = Tool::default();
    tool.name = Cow::Borrowed(name);
    tool.description = Some(Cow::Borrowed(description));
    tool.input_schema = input_schema;
    tool
}

/// Production `tools/list` for `SYMFORGE_SURFACE=compact` (A-019 compact-3).
pub fn compact_surface_tools() -> Vec<Tool> {
    let mut symforge = surface_tool(
        CompactSurfaceTool::Symforge.as_str(),
        "STEL read/explore facade — natural-language code intelligence with token economics.",
        schema_object::<StelRequest>(),
    );
    let mut annotations = rmcp::model::ToolAnnotations::default();
    annotations.read_only_hint = Some(true);
    annotations.open_world_hint = Some(false);
    symforge.annotations = Some(annotations);

    vec![
        symforge,
        surface_tool(
            CompactSurfaceTool::SymforgeEdit.as_str(),
            "STEL structural edit facade — symbol-aware edits with economics gate.",
            schema_object::<StelEditRequest>(),
        ),
        surface_tool(
            CompactSurfaceTool::Status.as_str(),
            "STEL trust envelope and index health summary.",
            schema_object::<StelStatusRequest>(),
        ),
    ]
}

/// UTF-8 JSON byte length of the production compact `tools/list` payload.
pub fn compact_surface_list_schema_bytes() -> (usize, usize) {
    let tools = compact_surface_tools();
    let payload = json!({ "tools": tools });
    let bytes = serde_json::to_string(&payload)
        .expect("compact surface tools must serialize")
        .len();
    (tools.len(), bytes)
}

/// Byte length of production `symforge_edit` input schema alone (A-025 gate).
pub fn symforge_edit_schema_bytes() -> usize {
    let edit = compact_surface_tools()
        .into_iter()
        .find(|t| t.name == CompactSurfaceTool::SymforgeEdit.as_str())
        .expect("symforge_edit compact surface tool");
    serde_json::to_string(&edit.input_schema)
        .expect("symforge_edit schema serializes")
        .len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stel::surface::{COMPACT_SURFACE_TOOL_COUNT, COMPACT_TOOL_NAMES};

    #[test]
    fn compact_surface_exposes_a019_tool_names() {
        let tools = compact_surface_tools();
        assert_eq!(tools.len(), COMPACT_SURFACE_TOOL_COUNT);
        let names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();
        assert_eq!(names.as_slice(), COMPACT_TOOL_NAMES);
    }

    #[test]
    fn compact_surface_annotations_are_honest() {
        let tools = compact_surface_tools();
        let symforge = tools
            .iter()
            .find(|tool| tool.name.as_ref() == "symforge")
            .expect("compact symforge tool");
        let annotations = symforge
            .annotations
            .as_ref()
            .expect("compact symforge annotations");
        assert_eq!(annotations.read_only_hint, Some(true));
        assert_eq!(annotations.open_world_hint, Some(false));

        for name in ["symforge_edit", "status"] {
            let read_only_hint = tools
                .iter()
                .find(|tool| tool.name.as_ref() == name)
                .and_then(|tool| tool.annotations.as_ref())
                .and_then(|annotations| annotations.read_only_hint);
            assert_ne!(read_only_hint, Some(true), "{name} must not be read-only");
        }
    }

    #[test]
    fn compact_surface_is_three_tools_under_h1_budget() {
        let (count, bytes) = compact_surface_list_schema_bytes();
        assert_eq!(count, 3, "compact surface must expose exactly 3 tools");
        assert!(
            bytes <= 5000,
            "compact tools/list JSON must be <= 5000 B (H1); got {bytes} B"
        );
    }

    /// `project` and `working_directory` are routing fields the daemon supplies
    /// or peeks before decode. They must stay OUT of the advertised compact
    /// schema (A-025 byte budget) while still deserializing — unhiding either
    /// one silently blows the budget, and dropping deserialization silently
    /// sends the edit to the wrong repository.
    #[test]
    fn routing_fields_are_schema_hidden_but_still_deserialized() {
        let edit = compact_surface_tools()
            .into_iter()
            .find(|t| t.name == CompactSurfaceTool::SymforgeEdit.as_str())
            .expect("symforge_edit compact surface tool");
        let schema = serde_json::to_string(&edit.input_schema).expect("schema serializes");
        for field in ["project", "working_directory"] {
            assert!(
                !schema.contains(field),
                "{field} must stay hidden from the compact symforge_edit schema"
            );
        }

        let request: crate::stel::StelEditRequest = serde_json::from_value(serde_json::json!({
            "path": "src/lib.rs",
            "project": "sibling-repo",
            "working_directory": "/tmp/worktree",
        }))
        .expect("routing fields must still deserialize");
        assert_eq!(request.project.as_deref(), Some("sibling-repo"));
        assert_eq!(request.working_directory.as_deref(), Some("/tmp/worktree"));
    }

    #[test]
    fn symforge_edit_schema_under_a025_budget() {
        let bytes = symforge_edit_schema_bytes();
        assert!(
            bytes <= 1500,
            "symforge_edit input_schema must be <= 1500 B (A-025); got {bytes} B"
        );
    }

    /// The stripper must take the two inert keywords and NOTHING else. A struct
    /// field may legitimately be named `default` or `$schema`; under
    /// `properties` those are field names, not keywords, and must survive.
    #[test]
    fn strip_schema_noise_drops_keywords_but_spares_field_names() {
        let mut root = match serde_json::json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "properties": {
                "path": { "type": "string", "default": null },
                "depth": { "type": "integer", "default": 3 },
                "default": { "type": "string" },
                "$schema": { "type": "string" }
            }
        }) {
            Value::Object(map) => map,
            other => panic!("fixture must be an object, got {other:?}"),
        };

        strip_schema_noise(&mut root);

        assert!(
            !root.contains_key("$schema"),
            "root $schema keyword must go"
        );
        let props = root["properties"]
            .as_object()
            .expect("properties survives stripping");
        assert!(
            !props["path"].as_object().unwrap().contains_key("default"),
            "`default: null` is schemars boilerplate and must go"
        );
        assert_eq!(
            props["depth"]["default"], 3,
            "a REAL default value must survive"
        );
        assert!(
            props.contains_key("default") && props.contains_key("$schema"),
            "fields NAMED `default`/`$schema` are not keywords and must survive"
        );
    }

    /// Guards the win: if schemars (or a future rmcp) starts re-emitting these,
    /// the payload silently regrows and this fails instead.
    #[test]
    fn compact_surface_carries_no_inert_schema_bytes() {
        fn assert_no_null_default(value: &Value, tool: &str) {
            match value {
                Value::Object(map) => {
                    for (key, child) in map {
                        assert!(
                            !(key == "default" && child.is_null()),
                            "{tool}: `default: null` survived stripping"
                        );
                        assert_no_null_default(child, tool);
                    }
                }
                Value::Array(items) => items.iter().for_each(|v| assert_no_null_default(v, tool)),
                _ => {}
            }
        }

        for tool in compact_surface_tools() {
            let name = tool.name.as_ref();
            assert!(
                !tool.input_schema.contains_key("$schema"),
                "{name}: root $schema survived stripping"
            );
            for value in tool.input_schema.values() {
                assert_no_null_default(value, name);
            }
        }
    }
}
