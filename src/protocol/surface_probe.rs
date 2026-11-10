//! Non-shipping L0 surface probe for Phase 0 §12A schema-byte measurement (A-005 / A-025).
//!
//! Frozen hand-written schemas for A-005 / A-019 measurement artifacts. Production
//! `SYMFORGE_SURFACE=compact` `tools/list` is served from [`crate::stel::compact_surface_tools`]
//! (Phase 1 S3). This module remains for meta-1 A/B, full-surface listing, and byte
//! replay against pinned Phase 0 evidence.

use std::borrow::Cow;
use std::future::Future;
use std::sync::Arc;

use rmcp::model::Tool;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Map, Value, json};

use super::SymForgeServer;

pub use crate::stel::surface::{
    SurfaceProfile, surface_label_from_str, surface_profile_from_label, surface_profile_label,
};

pub fn surface_profile_from_env() -> SurfaceProfile {
    // Default surface is FULL (spike-gate verdict 2026-07-03: no target harness
    // rejected the then-36-tool full `tools/list`; see docs/reviews). The current
    // full surface has 40 tools. `SYMFORGE_SURFACE=compact`
    // is the documented opt-in escape hatch (the compact-3 facade) for token-
    // sensitive setups; `meta` keeps its explicit measurement meaning. Only this
    // fallback arm changed — explicit `full`/`meta`/`compact` values are unchanged,
    // and the D23 connection-surface task-local still takes precedence over env.
    match std::env::var("SYMFORGE_SURFACE")
        .ok()
        .map(|v| v.to_ascii_lowercase())
    {
        Some(ref s) if s == "compact" => SurfaceProfile::Compact,
        Some(ref s) if s == "meta" => SurfaceProfile::Meta,
        _ => SurfaceProfile::Full,
    }
}

/// HTTP header the front-end adapter stamps on every proxied daemon tool call
/// with the surface actually served on THIS connection (D23).
///
/// Out-of-band from the tool params JSON on purpose: the params carry each
/// tool's own typed input, one of which (`get_file_content`) is
/// `#[serde(deny_unknown_fields)]` — an injected params key would make an older
/// daemon's decode reject the call. A header degrades cleanly in BOTH version-
/// skew directions: an old daemon never reads it, a new daemon falls back to its
/// own env when an old adapter never sends it.
pub const CONNECTION_SURFACE_HEADER: &str = "x-symforge-connection-surface";

tokio::task_local! {
    /// Surface served on the connection whose proxied tool call the daemon is
    /// currently rendering (D23). Bound for the duration of ONE daemon-side tool
    /// dispatch by [`with_connection_surface`] from the adapter-supplied
    /// [`CONNECTION_SURFACE_HEADER`]; unset on direct/daemon-less serving, where
    /// this process's env IS the connection surface.
    static CONNECTION_SURFACE: SurfaceProfile;
}

/// Surface to render surface-aware empty-index guards against: the proxied
/// connection surface when a daemon-side proxied call is in flight, else this
/// process's own env (`SYMFORGE_SURFACE`).
///
/// This is the D23 fix point. The empty-index recovery hint must name only
/// tools callable on the AGENT's surface; in the daemon-proxy topology that is
/// the adapter's surface, not the daemon process's env (a compact adapter proxied
/// through a full-env daemon must not be told to call the gated `index_folder`).
pub fn connection_surface_or_env() -> SurfaceProfile {
    CONNECTION_SURFACE
        .try_with(|profile| *profile)
        .unwrap_or_else(|_| surface_profile_from_env())
}

/// Run `future` with the connection surface bound for a daemon-side proxied tool
/// call. `None` (an older adapter, or an unrecognized header) leaves it unbound
/// so [`connection_surface_or_env`] falls back to env — the pre-D23 behavior.
///
/// ponytail: bound to the FUTURE, not the thread — correct across the `.await`s
/// the handler makes on its own task. Guards render inline in each handler, never
/// in a `tokio::spawn`ed sub-task (task-locals do not propagate into those); if a
/// future guard site is ever spawned, thread the profile explicitly instead.
pub async fn with_connection_surface<F, T>(profile: Option<SurfaceProfile>, future: F) -> T
where
    F: Future<Output = T>,
{
    match profile {
        Some(profile) => CONNECTION_SURFACE.scope(profile, future).await,
        None => future.await,
    }
}

/// Central compact-surface dispatch gate (P1-A / FR-008 enforcement).
///
/// `tools/list` already hides legacy tools on the compact surface, but hiding is
/// not enforcement: a client can still name a legacy tool at `tools/call`. This
/// is the pure decision used by the production `ServerHandler::call_tool` (shared
/// by stdio and the HTTP `/mcp` path): on [`SurfaceProfile::Compact`], any tool
/// name NOT in the advertised compact-3 set ([`crate::stel::surface::COMPACT_TOOL_NAMES`])
/// is rejected. `Full` and `Meta` are never gated here — the server default is
/// full, and `SYMFORGE_SURFACE=full` remains an explicit alias for the same
/// profile.
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
                "tool '{tool_name}' not available on compact surface; unset SYMFORGE_SURFACE or set SYMFORGE_SURFACE=full for the full tool surface"
            ),
            None,
        ));
    }
    Ok(())
}

/// Upper bound for the `initialize` instructions string (SF-ADOPT-04): it is
/// paid on every session, so it stays a routing hint, not a manual.
pub const SERVER_INSTRUCTIONS_MAX_BYTES: usize = 600;

/// Full-surface routing hint. Names only tools the full surface lists.
pub const FULL_SURFACE_INSTRUCTIONS: &str = "SymForge indexes this repository. Start with search_symbols to find a function or type, get_symbol to read its body (pass targets[] to fetch several in one call), and find_references for callers and usages. Use search_text for code text. For exact literal match counts your own grep is fine. Read raw files with get_file_content only for docs, configs or exact bytes. On a daemon with several open projects, the project/projects parameters select one by ID or alias (never a path); omit both for the active project.";

/// Compact-surface routing hint. Names only the three compact tools.
pub const COMPACT_SURFACE_INSTRUCTIONS: &str = "SymForge indexes this repository. Ask code questions (find, read, trace, impact) with symforge, edit a symbol with symforge_edit, and check index state with status. For exact literal match counts your own grep is fine.";

/// Instructions for the surface this process serves. The meta probe is a
/// measurement surface and gets none.
pub fn server_instructions(profile: SurfaceProfile) -> Option<&'static str> {
    match profile {
        SurfaceProfile::Full => Some(FULL_SURFACE_INSTRUCTIONS),
        SurfaceProfile::Compact => Some(COMPACT_SURFACE_INSTRUCTIONS),
        SurfaceProfile::Meta => None,
    }
}

pub fn list_tools_for_profile(profile: SurfaceProfile) -> Vec<Tool> {
    match profile {
        SurfaceProfile::Full => SymForgeServer::tool_router()
            .list_all()
            .into_iter()
            .filter(|tool| tool.name.as_ref() != "symforge")
            .map(|mut tool| {
                // The `#[tool]` macro builds these schemas via schemars, so they
                // carry the same inert `$schema` / `"default": null` bytes the
                // compact surface strips in `schema_object`.
                let mut schema = (*tool.input_schema).clone();
                crate::stel::surface_list::strip_schema_noise(&mut schema);
                shorten_project_selector_text(&mut schema);
                tool.input_schema = Arc::new(schema);
                tool
            })
            .collect(),
        SurfaceProfile::Compact => compact_probe_tools(),
        SurfaceProfile::Meta => meta_probe_tools(),
    }
}

/// One-line text for the `project` selector; the full explanation lives in the
/// server instructions (`get_info`) instead of being repeated on ~30 tools.
const PROJECT_SELECTOR_TEXT: &str =
    "Open-project ID or alias (not a path); omit for the active project.";
/// One-line text for the `projects` multi-project selector.
const PROJECTS_SELECTOR_TEXT: &str =
    "Open-project IDs/aliases, or [\"*\"] for all; daemon only; exclusive with `project`.";

/// SF-TE-05: replace the per-tool `project` / `projects` descriptions with one
/// line each. Only the top-level `properties` are touched, and only the
/// `description` string, so the property set and `required` are unchanged.
fn shorten_project_selector_text(schema: &mut Map<String, Value>) {
    let Some(Value::Object(props)) = schema.get_mut("properties") else {
        return;
    };
    for (name, text) in [
        ("project", PROJECT_SELECTOR_TEXT),
        ("projects", PROJECTS_SELECTOR_TEXT),
    ] {
        if let Some(Value::Object(field)) = props.get_mut(name)
            && field.contains_key("description")
        {
            field.insert("description".to_string(), Value::from(text));
        }
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

/// Phase 0 A-019 measurement input. `_probe_*` fields are schema-hidden but
/// production-reachable through the source-mutation-safe facade allowlist.
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
        "Phase 0 facade relay requires `_probe_legacy_tool` (A-019 measurement relay)".to_string()
    })?;
    let legacy_args = input.probe_legacy_args.clone().ok_or_else(|| {
        "Phase 0 facade relay requires `_probe_legacy_args` (A-019 measurement relay)".to_string()
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
    fn full_profile_lists_current_tool_count() {
        let tools = list_tools_for_profile(SurfaceProfile::Full);
        assert_eq!(
            tools.len(),
            40,
            "full surface count changed; update the public inventory with any intentional change"
        );
        assert!(
            !tools.iter().any(|t| t.name == "symforge"),
            "full surface must not advertise symforge facade"
        );
    }

    /// SF-TE-05: the default full surface is paid for by every session before any
    /// call. Measured with the same method as `scripts/measure-schema-bytes.ps1`
    /// (UTF-8 bytes of the serialized `{"tools": [...]}` payload).
    #[test]
    fn full_surface_tools_list_under_byte_budget() {
        // Measured 69,631 B before list-time slimming, 63,106 B after (40 tools).
        // The 30,000 B target needs the ~10-tool core default profile; this budget
        // guards the full profile against regrowth. Lower it, never raise it
        // without a reviewed reason.
        const FULL_SURFACE_BUDGET_BYTES: usize = 63_500;
        let (count, bytes) = list_tools_schema_bytes(SurfaceProfile::Full);
        eprintln!("full surface tools/list: {count} tools, {bytes} B");
        assert!(
            bytes <= FULL_SURFACE_BUDGET_BYTES,
            "full tools/list JSON must be <= {FULL_SURFACE_BUDGET_BYTES} B; got {bytes} B"
        );
    }

    /// SF-TE-05: list-time slimming rewrites only what is ADVERTISED. Every tool
    /// keeps its property set and `required` list, no `null` / numeric-format noise
    /// survives, and the server still decodes explicit `null` and absent optionals.
    #[test]
    fn schema_slimming_preserves_accepted_inputs() {
        let raw: std::collections::HashMap<String, Arc<Map<String, Value>>> =
            SymForgeServer::tool_router()
                .list_all()
                .into_iter()
                .map(|t| (t.name.to_string(), t.input_schema))
                .collect();
        for tool in list_tools_for_profile(SurfaceProfile::Full) {
            let name = tool.name.as_ref();
            let before = &raw[name];
            let after = &tool.input_schema;
            let keys = |s: &Map<String, Value>| -> Vec<String> {
                s.get("properties")
                    .and_then(Value::as_object)
                    .map(|p| p.keys().cloned().collect())
                    .unwrap_or_default()
            };
            assert_eq!(keys(before), keys(after), "{name}: property set changed");
            assert_eq!(
                before.get("required"),
                after.get("required"),
                "{name}: required changed"
            );
            let text = serde_json::to_string(after.as_ref()).unwrap();
            for noise in [r#""null""#, r#""format":"uint"#, r#""minimum":0"#] {
                assert!(!text.contains(noise), "{name}: `{noise}` survived slimming");
            }
        }

        // Server-side decode is serde, independent of the advertised schema:
        // explicit null and absence both decode to None.
        let text: crate::protocol::tools::SearchTextInput = serde_json::from_value(json!({
            "query": "x", "limit": null, "include_tests": null, "group_by": null,
            "project": null, "projects": null
        }))
        .expect("explicit nulls still decode");
        assert!(text.limit.is_none() && text.projects.is_none() && text.group_by.is_none());
        let _: crate::protocol::tools::SearchTextInput =
            serde_json::from_value(json!({ "query": "x" })).expect("absent optionals decode");
        let status: crate::stel::types::StelStatusRequest =
            serde_json::from_value(json!({ "detail": null })).expect("null anyOf field decodes");
        assert!(status.detail.is_none());
        let knowledge: crate::protocol::tools::SearchKnowledgeInput =
            serde_json::from_value(json!({ "query": "x", "authority_scope": null, "limit": null }))
                .expect("null anyOf field decodes");
        assert!(knowledge.authority_scope.is_none());
    }

    /// SF-ADOPT-04 snapshot: exact text, byte cap, and every tool name the text
    /// mentions is callable on that surface.
    #[test]
    fn server_instructions_snapshot_cap_and_surface_names() {
        assert_eq!(
            server_instructions(SurfaceProfile::Full),
            Some(
                "SymForge indexes this repository. Start with search_symbols to find a function or type, get_symbol to read its body (pass targets[] to fetch several in one call), and find_references for callers and usages. Use search_text for code text. For exact literal match counts your own grep is fine. Read raw files with get_file_content only for docs, configs or exact bytes. On a daemon with several open projects, the project/projects parameters select one by ID or alias (never a path); omit both for the active project."
            )
        );
        assert_eq!(
            server_instructions(SurfaceProfile::Compact),
            Some(
                "SymForge indexes this repository. Ask code questions (find, read, trace, impact) with symforge, edit a symbol with symforge_edit, and check index state with status. For exact literal match counts your own grep is fine."
            )
        );
        assert_eq!(server_instructions(SurfaceProfile::Meta), None);

        let names = |tools: Vec<Tool>| -> Vec<String> {
            tools.into_iter().map(|t| t.name.to_string()).collect()
        };
        let full = names(list_tools_for_profile(SurfaceProfile::Full));
        let compact = names(crate::stel::compact_surface_tools());
        let every: Vec<&String> = full.iter().chain(compact.iter()).collect();
        for (profile, served) in [
            (SurfaceProfile::Full, &full),
            (SurfaceProfile::Compact, &compact),
        ] {
            let text = server_instructions(profile).unwrap();
            assert!(
                text.len() <= SERVER_INSTRUCTIONS_MAX_BYTES,
                "{profile:?}: {} B",
                text.len()
            );
            let words: Vec<&str> = text
                .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .collect();
            for name in &every {
                if words.contains(&name.as_str()) {
                    assert!(
                        served.contains(name),
                        "{profile:?} names off-surface tool `{name}`"
                    );
                }
            }
        }
        assert!(
            compact.len() < full.len()
                && server_instructions(SurfaceProfile::Compact).unwrap().len()
                    < server_instructions(SurfaceProfile::Full).unwrap().len()
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

    #[test]
    fn surface_profile_from_label_roundtrips_canonical_and_rejects_junk() {
        assert_eq!(
            surface_profile_from_label("full"),
            Some(SurfaceProfile::Full)
        );
        assert_eq!(
            surface_profile_from_label("compact"),
            Some(SurfaceProfile::Compact)
        );
        assert_eq!(
            surface_profile_from_label("meta"),
            Some(SurfaceProfile::Meta)
        );
        // Non-canonical input (wrong case, arbitrary text, empty) must NOT resolve —
        // it falls back to the daemon's own env instead of trusting adapter garbage.
        assert_eq!(surface_profile_from_label("FULL"), None);
        assert_eq!(surface_profile_from_label("nonsense"), None);
        assert_eq!(surface_profile_from_label(""), None);
    }

    #[tokio::test]
    async fn connection_surface_scope_binds_then_unbinds() {
        // Inside the scope, the bound connection profile is what the guard renderer
        // reads — regardless of this process's env.
        let bound = with_connection_surface(Some(SurfaceProfile::Compact), async {
            connection_surface_or_env()
        })
        .await;
        assert_eq!(bound, SurfaceProfile::Compact);

        // `None` leaves it unbound: `connection_surface_or_env` reads env, and reading
        // an unset task-local must not panic (uses `try_with`, not `with`).
        let unbound = with_connection_surface(None, async { connection_surface_or_env() }).await;
        assert_eq!(unbound, surface_profile_from_env());
    }
}
