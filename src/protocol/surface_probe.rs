//! Non-shipping L0 surface probe for Phase 0 §12A schema-byte measurement (A-005 / A-025).
//!
//! Frozen hand-written schemas for A-005 / A-019 measurement artifacts. Production
//! `SYMFORGE_SURFACE=compact` `tools/list` is served from [`crate::stel::compact_surface_tools`]
//! (Phase 1 S3). This module remains for meta-1 A/B, full-surface listing, and byte
//! replay against pinned Phase 0 evidence.

use std::borrow::Cow;
use std::sync::Arc;

use rmcp::model::Tool;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Map, Value, json};

use super::SymForgeServer;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceProfile {
    Full,
    Compact,
    Meta,
}

pub fn surface_profile_from_env() -> SurfaceProfile {
    // Default surface is compact-3 (v8 cutover, US2/FR-008). `SYMFORGE_SURFACE=full`
    // is the documented backward-compatible opt-out that restores the legacy
    // 32-tool surface; `meta` and `compact` keep their explicit meanings.
    match std::env::var("SYMFORGE_SURFACE")
        .ok()
        .map(|v| v.to_ascii_lowercase())
    {
        Some(ref s) if s == "full" => SurfaceProfile::Full,
        Some(ref s) if s == "meta" => SurfaceProfile::Meta,
        _ => SurfaceProfile::Compact,
    }
}

/// Canonical lowercase label (`full`/`compact`/`meta`) for a surface profile.
///
/// Single source of the wording used by the `status` readout AND threaded across
/// the adapter→daemon proxy boundary, so both processes agree on the exact
/// string (see `StelStatusRequest::connection_surface`).
pub fn surface_profile_label(profile: SurfaceProfile) -> &'static str {
    match profile {
        SurfaceProfile::Full => "full",
        SurfaceProfile::Meta => "meta",
        SurfaceProfile::Compact => "compact",
    }
}

/// Map a proxy-threaded connection-surface string back to a canonical static
/// label. Returns `None` for anything the adapter would never send, so an
/// unrecognized value falls back to the daemon's own env rather than echoing
/// arbitrary text into the trust readout.
pub fn surface_label_from_str(value: &str) -> Option<&'static str> {
    match value {
        "full" => Some("full"),
        "meta" => Some("meta"),
        "compact" => Some("compact"),
        _ => None,
    }
}

/// Central compact-surface dispatch gate (P1-A / FR-008 enforcement).
///
/// `tools/list` already hides legacy tools on the compact surface, but hiding is
/// not enforcement: a client can still name a legacy tool at `tools/call`. This
/// is the pure decision used by the production `ServerHandler::call_tool` (shared
/// by stdio and the HTTP `/mcp` path): on [`SurfaceProfile::Compact`], any tool
/// name NOT in the advertised compact-3 set ([`crate::stel::surface::COMPACT_TOOL_NAMES`])
/// is rejected. `Full` and `Meta` are never gated here, so the documented
/// `SYMFORGE_SURFACE=full` opt-out still reaches every legacy tool.
pub fn compact_surface_blocks(profile: SurfaceProfile, tool_name: &str) -> bool {
    profile == SurfaceProfile::Compact
        && !crate::stel::surface::COMPACT_TOOL_NAMES.contains(&tool_name)
}

/// Apply the compact-surface gate using the live `SYMFORGE_SURFACE` env profile.
///
/// Returns `Err(InvalidRequest)` when the call must be rejected, else `Ok(())`.
/// Used by the production `call_tool` and asserted directly by the surface
/// conformance test so the test exercises the real gate, not a copy.
pub fn enforce_compact_surface(tool_name: &str) -> Result<(), rmcp::ErrorData> {
    if compact_surface_blocks(surface_profile_from_env(), tool_name) {
        return Err(rmcp::ErrorData::invalid_request(
            format!(
                "tool '{tool_name}' not available on compact surface; set SYMFORGE_SURFACE=full"
            ),
            None,
        ));
    }
    Ok(())
}

pub fn list_tools_for_profile(profile: SurfaceProfile) -> Vec<Tool> {
    match profile {
        SurfaceProfile::Full => SymForgeServer::tool_router()
            .list_all()
            .into_iter()
            .filter(|tool| tool.name.as_ref() != "symforge")
            .collect(),
        SurfaceProfile::Compact => compact_probe_tools(),
        SurfaceProfile::Meta => meta_probe_tools(),
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

fn symforge_facade_schema() -> Value {
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
    })
}

/// Phase 0 A-019 battery input. `_probe_*` fields are harness-only (serde accepts; not in schema).
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct StelFacadeProbeInput {
    pub query: String,
    #[serde(default)]
    pub intent: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub symbol: Option<String>,
    #[serde(default, rename = "_probe_legacy_tool")]
    pub probe_legacy_tool: Option<String>,
    #[serde(default, rename = "_probe_legacy_args")]
    pub probe_legacy_args: Option<Value>,
}

/// Resolve a compact/meta `symforge` facade call to a legacy L3 tool for measurement relay.
pub fn resolve_facade_probe(input: &StelFacadeProbeInput) -> Result<(String, Value), String> {
    let legacy_tool = input.probe_legacy_tool.as_deref().ok_or_else(|| {
        "Phase 0 facade relay requires `_probe_legacy_tool` (A-019 battery harness only)"
            .to_string()
    })?;
    let legacy_args = input.probe_legacy_args.clone().ok_or_else(|| {
        "Phase 0 facade relay requires `_probe_legacy_args` (A-019 battery harness only)"
            .to_string()
    })?;
    Ok((legacy_tool.to_string(), legacy_args))
}

/// Single-tool meta-1 surface for A-019 L0 A/B (measurement only).
pub fn meta_probe_tools() -> Vec<Tool> {
    vec![probe_tool(
        "symforge",
        "STEL meta-tool facade (Phase 0 measurement probe)",
        symforge_facade_schema(),
    )]
}

/// Three-tool compact surface for H1 / A-005 measurement (not STEL runtime).
pub fn compact_probe_tools() -> Vec<Tool> {
    vec![
        probe_tool(
            "symforge",
            "STEL read/explore facade (Phase 0 measurement probe)",
            symforge_facade_schema(),
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
        assert!(
            !tools.iter().any(|t| t.name == "symforge"),
            "full surface must not advertise symforge facade"
        );
    }

    #[test]
    fn meta_surface_is_one_tool_under_h1_budget() {
        let (count, bytes) = list_tools_schema_bytes(SurfaceProfile::Meta);
        assert_eq!(count, 1, "meta-1 probe must expose exactly 1 tool");
        assert!(
            bytes <= 5000,
            "meta tools/list JSON must be <= 5000 B (H1); got {bytes} B"
        );
    }

    #[test]
    fn facade_probe_requires_harness_fields() {
        let input = StelFacadeProbeInput {
            query: "x".to_string(),
            intent: None,
            path: None,
            symbol: None,
            probe_legacy_tool: None,
            probe_legacy_args: None,
        };
        assert!(resolve_facade_probe(&input).is_err());
    }
}
