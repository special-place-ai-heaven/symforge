//! Production compact-surface `tools/list` entries (Phase 1 S3).

use std::borrow::Cow;
use std::sync::Arc;

use rmcp::model::Tool;
use schemars::{JsonSchema, schema_for};
use serde_json::{Map, Value, json};

use super::surface::CompactSurfaceTool;
use super::types::{StelEditRequest, StelRequest, StelStatusRequest};

fn schema_object<T: JsonSchema>() -> Arc<Map<String, Value>> {
    let schema = schema_for!(T);
    let value = serde_json::to_value(schema).expect("STEL input schema must serialize");
    Arc::new(
        value
            .as_object()
            .expect("STEL input schema root must be a JSON object")
            .clone(),
    )
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

    #[test]
    fn symforge_edit_schema_under_a025_budget() {
        let bytes = symforge_edit_schema_bytes();
        assert!(
            bytes <= 1500,
            "symforge_edit input_schema must be <= 1500 B (A-025); got {bytes} B"
        );
    }
}
