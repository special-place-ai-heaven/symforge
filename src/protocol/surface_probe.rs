//! Non-shipping L0 surface probe for Phase 0 §12A schema-byte measurement (A-005 / A-025).
//!
//! When `SYMFORGE_SURFACE=compact`, `tools/list` advertises three STEL-shaped tools
//! with draft schemas from `docs/stel-schema.md`. Does not implement STEL execution.

use std::borrow::Cow;
use std::sync::Arc;

use rmcp::model::Tool;
use serde_json::{Map, Value, json};

use super::SymForgeServer;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceProfile {
    Full,
    Compact,
}

pub fn surface_profile_from_env() -> SurfaceProfile {
    match std::env::var("SYMFORGE_SURFACE")
        .ok()
        .map(|v| v.to_ascii_lowercase())
    {
        Some(ref s) if s == "compact" => SurfaceProfile::Compact,
        _ => SurfaceProfile::Full,
    }
}

pub fn list_tools_for_profile(profile: SurfaceProfile) -> Vec<Tool> {
    match profile {
        SurfaceProfile::Full => SymForgeServer::tool_router().list_all(),
        SurfaceProfile::Compact => compact_probe_tools(),
    }
}

fn schema_object(value: Value) -> Arc<Map<String, Value>> {
    Arc::new(
        value
            .as_object()
            .expect("compact probe schema must be a JSON object")
            .clone(),
    )
}

fn probe_tool(name: &'static str, description: &'static str, input_schema: Value) -> Tool {
    let mut tool = Tool::default();
    tool.name = Cow::Borrowed(name);
    tool.description = Some(Cow::Borrowed(description));
    tool.input_schema = schema_object(input_schema);
    tool
}

/// Three-tool compact surface for H1 / A-005 measurement (not STEL runtime).
pub fn compact_probe_tools() -> Vec<Tool> {
    vec![
        probe_tool(
            "symforge",
            "STEL read/explore facade (Phase 0 measurement probe)",
            json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string" },
                    "intent": {
                        "type": "string",
                        "enum": ["auto", "orient", "find", "read", "trace", "impact", "meta"]
                    },
                    "path": { "type": "string" },
                    "symbol": { "type": "string" },
                    "max_tokens": { "type": "integer", "minimum": 64 },
                    "preview": { "type": "boolean" }
                },
                "required": ["query"]
            }),
        ),
        probe_tool(
            "symforge_edit",
            "STEL structural edit facade (Phase 0 measurement probe)",
            json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "symbol": { "type": "string" },
                    "body": { "type": "string" },
                    "intent": { "type": "string", "enum": ["edit"] }
                },
                "required": ["path"]
            }),
        ),
        probe_tool(
            "status",
            "STEL trust envelope / index health (Phase 0 measurement probe)",
            json!({
                "type": "object",
                "properties": {
                    "detail": { "type": "string", "enum": ["compact", "full"] }
                }
            }),
        ),
    ]
}

/// Byte length of `symforge_edit` input schema alone (A-025).
pub fn symforge_edit_schema_bytes() -> usize {
    let edit = compact_probe_tools()
        .into_iter()
        .find(|t| t.name == "symforge_edit")
        .expect("symforge_edit probe tool");
    serde_json::to_string(&edit.input_schema)
        .expect("edit schema serializes")
        .len()
}

/// UTF-8 JSON byte length of `tools/list` payload for the given profile.
pub fn list_tools_schema_bytes(profile: SurfaceProfile) -> (usize, usize) {
    let tools = list_tools_for_profile(profile);
    let payload = json!({ "tools": tools });
    let bytes = serde_json::to_string(&payload)
        .expect("compact probe tools must serialize")
        .len();
    (tools.len(), bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_surface_is_three_tools_under_h1_budget() {
        let (count, bytes) = list_tools_schema_bytes(SurfaceProfile::Compact);
        assert_eq!(count, 3, "compact probe must expose exactly 3 tools");
        assert!(
            bytes <= 5000,
            "compact tools/list JSON must be <= 5000 B (H1); got {bytes} B"
        );
    }

    #[test]
    fn symforge_edit_schema_under_a025_budget() {
        let tools = compact_probe_tools();
        let edit = tools
            .iter()
            .find(|t| t.name == "symforge_edit")
            .expect("symforge_edit must exist");
        let bytes = serde_json::to_string(&edit.input_schema)
            .expect("edit schema must serialize")
            .len();
        assert!(
            bytes <= 1500,
            "symforge_edit input_schema must be <= 1500 B (A-025); got {bytes} B"
        );
    }

    #[test]
    fn full_profile_lists_legacy_tool_count() {
        let tools = list_tools_for_profile(SurfaceProfile::Full);
        assert!(
            tools.len() >= 30,
            "full surface must expose legacy tool count; got {}",
            tools.len()
        );
    }
}
