//! Non-shipping L0 surface probe for Phase 0 §12A schema-byte measurement (A-005 / A-025).
//!
//! Frozen hand-written schemas for A-005 / A-019 measurement artifacts. Production
//! `SYMFORGE_SURFACE=compact` `tools/list` is served from [`crate::stel::compact_surface_tools`]
//! (Phase 1 S3). This module remains for meta-1 A/B, core and full listing, and byte
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
    SurfaceProfile, TOOL_GROUPS, TOOLS_ENV, ToolAllowList, surface_label_from_str,
    surface_profile_from_label, surface_profile_label,
};

/// `serve --tools` value. Takes precedence over [`TOOLS_ENV`] for that process;
/// a `OnceLock` instead of `set_var`, which is `unsafe` once threads exist.
static TOOLS_OVERRIDE: std::sync::OnceLock<String> = std::sync::OnceLock::new();

/// Install the `serve --tools` list for this process. A second call is ignored.
pub fn set_tools_override(spec: String) {
    let _ = TOOLS_OVERRIDE.set(spec);
}

/// The operator's per-process tool allow-list (issue #874): `serve --tools`,
/// else [`TOOLS_ENV`]. Read per call like `SYMFORGE_SURFACE`.
///
/// Fails closed: an explicit `--tools`, even blank, is a list (an empty one is
/// refused at startup), and a [`TOOLS_ENV`] value that is not UTF-8 is a list
/// with no valid name, never "unset". Only an absent or blank [`TOOLS_ENV`]
/// returns `None`, which leaves every surface exactly as it was.
pub fn tool_allow_list() -> Option<ToolAllowList> {
    if let Some(spec) = TOOLS_OVERRIDE.get() {
        return Some(ToolAllowList::parse(spec));
    }
    match std::env::var_os(TOOLS_ENV)?.into_string() {
        Ok(spec) if spec.trim().is_empty() => None,
        Ok(spec) => Some(ToolAllowList::parse(&spec)),
        Err(raw) => Some(ToolAllowList {
            allowed: Vec::new(),
            unknown: vec![format!("{} (not valid UTF-8)", raw.to_string_lossy())],
        }),
    }
}

/// Header the front end stamps on every proxied daemon tool call with the
/// allow-list served on THIS connection (comma-joined allowed names). The
/// daemon is shared by clients with different lists, so it never uses its own
/// env for a proxied call: [`connection_tools_from_header`] is authoritative.
pub const CONNECTION_TOOLS_HEADER: &str = "x-symforge-connection-tools";

tokio::task_local! {
    /// Allow-list of the connection whose proxied call the daemon is running.
    static CONNECTION_TOOLS: Option<ToolAllowList>;
}

/// Allow-list to enforce inside a handler: the proxied connection's on the
/// daemon side, else this process's own ([`tool_allow_list`]).
pub fn connection_allow_list() -> Option<ToolAllowList> {
    CONNECTION_TOOLS
        .try_with(Clone::clone)
        .unwrap_or_else(|_| tool_allow_list())
}

/// Front end: the header value for this process's allow-list, if any.
pub fn connection_tools_header_value() -> Option<String> {
    tool_allow_list().map(|list| list.allowed.join(","))
}

/// Daemon: the list a proxied call carries. Absent means no list; a value that
/// is not UTF-8 is a list with no valid name (fails closed).
pub fn connection_tools_from_header(value: Option<&[u8]>) -> Option<ToolAllowList> {
    value.map(|bytes| match std::str::from_utf8(bytes) {
        Ok(spec) => ToolAllowList::parse(spec),
        Err(_) => ToolAllowList {
            allowed: Vec::new(),
            unknown: Vec::new(),
        },
    })
}

/// Run a daemon-side proxied dispatch with the connection's allow-list bound.
pub async fn with_connection_tools<F, T>(list: Option<ToolAllowList>, future: F) -> T
where
    F: Future<Output = T>,
{
    CONNECTION_TOOLS.scope(list, future).await
}

/// Refusal text when `tool` may not run on this connection; `None` when it may.
/// `needed_by` names the resource or tool that needed `tool`, if not the call itself.
pub fn allow_list_refusal(tool: &str, needed_by: Option<&str>) -> Option<String> {
    let list = connection_allow_list()?;
    if list.allows(tool) {
        return None;
    }
    let callable = list.allowed.join(", ");
    let registered = tool == "symforge"
        || TOOL_GROUPS
            .iter()
            .any(|(_, members)| members.contains(&tool));
    Some(match needed_by {
        Some(caller) => format!(
            "{caller} needs the tool '{tool}', which is not in this server's tool allow-list \
             ({TOOLS_ENV}, or serve --tools). Tools callable here: {callable}."
        ),
        None if !registered => {
            format!("'{tool}' is not a tool of this server. Tools callable here: {callable}.")
        }
        None => format!(
            "tool '{tool}' is not in this server's tool allow-list ({TOOLS_ENV}, or serve \
             --tools). Tools callable here: {callable}. Use one of those; only the operator \
             can add '{tool}' to {TOOLS_ENV}."
        ),
    })
}

/// Precedence rule: a set allow-list filters the FULL surface, so a
/// `SYMFORGE_SURFACE` of `core`, `compact` or `meta` beside it is overridden.
/// That is never silent: this conflict is reported wherever unknown names are.
pub fn surface_env_conflict() -> Option<String> {
    tool_allow_list()?;
    let surface = std::env::var("SYMFORGE_SURFACE").ok()?.to_ascii_lowercase();
    matches!(surface.as_str(), "core" | "compact" | "meta")
        .then(|| format!("SYMFORGE_SURFACE={surface} is ignored because {TOOLS_ENV} is set"))
}

/// Startup check of the allow-list. `Err` when no listed name is available
/// (nothing would be served); otherwise the problems to report, empty when clean.
pub fn check_tool_allow_list() -> Result<Vec<String>, String> {
    let Some(list) = tool_allow_list() else {
        return Ok(Vec::new());
    };
    let groups: Vec<&str> = TOOL_GROUPS.iter().map(|(group, _)| *group).collect();
    if list.allowed.is_empty() && list.unknown.is_empty() {
        return Err(format!(
            "{TOOLS_ENV} (or serve --tools) is blank. Use tool names or the groups {}.",
            groups.join(", ")
        ));
    }
    if list.allowed.is_empty() {
        return Err(format!(
            "{TOOLS_ENV} names no available tool (unknown: {}). Use tool names or the groups {}.",
            list.unknown.join(", "),
            groups.join(", ")
        ));
    }
    let mut problems = Vec::new();
    if !list.unknown.is_empty() {
        problems.push(format!(
            "{TOOLS_ENV} has unknown names, ignored: {} (groups: {})",
            list.unknown.join(", "),
            groups.join(", ")
        ));
    }
    problems.extend(surface_env_conflict());
    Ok(problems)
}

/// `status` lines describing the allow-list; empty when unset.
pub fn allow_list_status_lines() -> Vec<String> {
    let Some(list) = tool_allow_list() else {
        return Vec::new();
    };
    let mut lines = vec![format!(
        "tools_allowed: {} of {}: {}",
        list.allowed.len(),
        crate::stel::surface::allow_list_tool_count(),
        list.allowed.join(", ")
    )];
    if !list.unknown.is_empty() {
        lines.push(format!(
            "tools_allowed_unknown: {}",
            list.unknown.join(", ")
        ));
    }
    lines.extend(
        surface_env_conflict().map(|conflict| format!("tools_allowed_conflict: {conflict}")),
    );
    lines
}

pub fn surface_profile_from_env() -> SurfaceProfile {
    // An allow-list filters the full surface (see `surface_env_conflict`).
    if tool_allow_list().is_some() {
        return SurfaceProfile::Full;
    }
    // Default surface is FULL (spike-gate verdict 2026-07-03: no target harness
    // rejected the then-36-tool full `tools/list`; see docs/reviews). The current
    // full surface has 40 tools. `SYMFORGE_SURFACE=compact`
    // is the documented opt-in escape hatch (the compact-3 facade) for token-
    // sensitive setups; `core` (SF-TE-05) lists only `CORE_TOOL_NAMES` and
    // refuses nothing; `meta` keeps its explicit measurement meaning. Unset and
    // unrecognised values resolve to full, and the D23 connection-surface
    // task-local still takes precedence over env.
    match std::env::var("SYMFORGE_SURFACE")
        .ok()
        .map(|v| v.to_ascii_lowercase())
    {
        Some(ref s) if s == "core" => SurfaceProfile::Core,
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
/// is rejected. `Core`, `Full` and `Meta` are never gated here: the server default
/// is full, and the opt-in core profile only lists fewer tools, it does not
/// refuse the unlisted ones.
pub fn compact_surface_blocks(profile: SurfaceProfile, tool_name: &str) -> bool {
    profile == SurfaceProfile::Compact
        && !crate::stel::surface::COMPACT_TOOL_NAMES.contains(&tool_name)
}

/// Apply the surface gates: the operator tool allow-list ([`TOOLS_ENV`]), then
/// the compact-surface gate using the live `SYMFORGE_SURFACE` env profile.
///
/// Returns `Err(InvalidRequest)` when the call must be rejected, else `Ok(())`.
/// Used by the production `call_tool` (stdio front and the HTTP `/mcp` path)
/// and asserted directly by the surface conformance test so the test exercises
/// the real gate, not a copy.
pub fn enforce_compact_surface(tool_name: &str) -> Result<(), rmcp::ErrorData> {
    if let Some(refusal) = allow_list_refusal(tool_name, None) {
        return Err(rmcp::ErrorData::invalid_request(refusal, None));
    }
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

/// Core-profile routing hint (opt-in profile). Names only core tools, and says in
/// one sentence that more exist and how the operator lists them.
pub const CORE_SURFACE_INSTRUCTIONS: &str = "SymForge indexes this repository. Start with search_symbols to find a function or type, get_symbol to read its body (pass targets[] to fetch several in one call), and find_references for callers and usages. Use search_text for code text. For exact literal match counts your own grep is fine. Read raw files with get_file_content only for docs, configs or exact bytes. The project/projects parameters select an open project by ID or alias, never a path. This is the core profile: tool text may name unlisted tools; the operator lists every tool with SYMFORGE_SURFACE=full.";

/// Instructions for the surface this process serves. The meta probe is a
/// measurement surface and gets none.
pub fn server_instructions(profile: SurfaceProfile) -> Option<&'static str> {
    match profile {
        SurfaceProfile::Core => Some(CORE_SURFACE_INSTRUCTIONS),
        SurfaceProfile::Full => Some(FULL_SURFACE_INSTRUCTIONS),
        SurfaceProfile::Compact => Some(COMPACT_SURFACE_INSTRUCTIONS),
        SurfaceProfile::Meta => None,
    }
}

/// Instructions `get_info` sends on this connection. Unset allow-list: exactly
/// [`server_instructions`]. Set: a generated line naming the callable tools
/// (the static hints may name tools this connection cannot call), shortened to
/// a count plus group names when the full form would exceed
/// [`SERVER_INSTRUCTIONS_MAX_BYTES`].
pub fn connection_instructions() -> Option<String> {
    match tool_allow_list() {
        None => server_instructions(surface_profile_from_env()).map(str::to_string),
        Some(list) => Some(allow_list_instructions(
            &list,
            surface_env_conflict().as_deref(),
        )),
    }
}

/// The generated allow-list instructions; always within the byte cap.
pub fn allow_list_instructions(list: &ToolAllowList, conflict: Option<&str>) -> String {
    let total = crate::stel::surface::allow_list_tool_count();
    let names = list.allowed.join(", ");
    let counted = format!(
        "{} of {total}, from the groups {}; status lists them",
        list.allowed.len(),
        list.touched_groups().join(", ")
    );
    let unknown_full = format!(" Ignored unknown names: {}.", list.unknown.join(", "));
    let unknown_short = format!(
        " {} unknown names were ignored; status lists them.",
        list.unknown.len()
    );
    let unknown_forms: [&str; 2] = if list.unknown.is_empty() {
        ["", ""]
    } else {
        [&unknown_full, &unknown_short]
    };
    let conflict = conflict.map(|c| format!(" {c}.")).unwrap_or_default();
    let render = |tools: &str, unknown: &str| {
        format!(
            "SymForge indexes this repository. Tools on this connection ({TOOLS_ENV}): {tools}. \
             Tool names in answers that are not listed here are not callable.{unknown}{conflict}"
        )
    };
    [
        (names.as_str(), unknown_forms[0]),
        (names.as_str(), unknown_forms[1]),
        (counted.as_str(), unknown_forms[0]),
        (counted.as_str(), unknown_forms[1]),
    ]
    .into_iter()
    .map(|(tools, unknown)| render(tools, unknown))
    .find(|text| text.len() <= SERVER_INSTRUCTIONS_MAX_BYTES)
    // The shortest form is bounded (counts, five group names, one fixed
    // conflict sentence), so this fallback is unreachable; it is the same form.
    .unwrap_or_else(|| render(&counted, unknown_forms[1]))
}

/// Tools `tools/list` advertises on this connection: the profile's list,
/// narrowed to the operator allow-list when one is set. Entries are the
/// profile's own, byte for byte.
pub fn advertised_tools() -> Vec<Tool> {
    let profile = surface_profile_from_env();
    let mut tools = match profile {
        SurfaceProfile::Compact => crate::stel::compact_surface_tools(),
        _ => list_tools_for_profile(profile),
    };
    if let Some(list) = tool_allow_list() {
        tools.retain(|tool| list.allows(tool.name.as_ref()));
    }
    tools
}

pub fn list_tools_for_profile(profile: SurfaceProfile) -> Vec<Tool> {
    match profile {
        SurfaceProfile::Full => slimmed_router_tools(|name| name != "symforge"),
        SurfaceProfile::Core => {
            slimmed_router_tools(|name| crate::stel::surface::CORE_TOOL_NAMES.contains(&name))
                .into_iter()
                .map(|mut tool| {
                    if let Some((_, text)) = CORE_DESCRIPTIONS
                        .iter()
                        .find(|(name, _)| *name == tool.name)
                    {
                        tool.description = Some(Cow::Borrowed(text));
                    }
                    tool
                })
                .collect()
        }
        SurfaceProfile::Compact => compact_probe_tools(),
        SurfaceProfile::Meta => meta_probe_tools(),
    }
}

/// Core-profile descriptions for the core tools whose full-surface text points
/// at a tool core does not list. Same guidance minus those pointers; the
/// full surface keeps the original text. A test keeps these free of
/// off-core names.
const CORE_DESCRIPTIONS: &[(&str, &str)] = &[
    (
        "get_symbol",
        "Batch-read symbols: targets[] returns several symbols' source plus docs in one call (or name+path for one). Not callers (find_references) or raw files (get_file_content).",
    ),
    (
        "search_symbols",
        "Find functions/types by name (kind and path_prefix filters; omit query to browse). Not code text (search_text).",
    ),
    (
        "index_folder",
        "Open or refresh a directory and make it ACTIVE for this connection (add=true opens without switching).",
    ),
    (
        "find_references",
        "Sites/imports/types; best-effort direct caller/site ranges; may be incomplete. Same-name ambiguity: scope path/symbol_kind. mode=implementations: trait implementors.",
    ),
    (
        "status",
        "STEL trust envelope plus index state summary. Prefer at session start.",
    ),
    (
        "replace_symbol_body",
        "Replace a symbol's entire definition; the index resolves the range (no prior read). Not a small inner edit (edit_within_symbol).",
    ),
    (
        "edit_within_symbol",
        "Find-and-replace inside one symbol's byte range only (replace_all=true for every hit). Not a whole-definition rewrite (replace_symbol_body).",
    ),
    (
        "batch_edit",
        "Apply many symbol-addressed edits atomically across files (all resolve or none write); dry_run=true previews. Not a single-symbol edit (replace_symbol_body).",
    ),
];

/// Registered router tools selected by `keep`, with list-time schema slimming.
fn slimmed_router_tools(keep: impl Fn(&str) -> bool) -> Vec<Tool> {
    SymForgeServer::tool_router()
        .list_all()
        .into_iter()
        .filter(|tool| keep(tool.name.as_ref()))
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
        .collect()
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

/// Test-only RAII guard for the process-global surface variables.
#[cfg(test)]
pub(crate) struct SurfaceEnvVarGuard {
    key: &'static str,
    previous: Option<std::ffi::OsString>,
}

#[cfg(test)]
#[allow(unsafe_code)] // test-only env guard; no concurrent env readers under --test-threads=1.
impl SurfaceEnvVarGuard {
    /// Set `key` to `value`, or remove it when `value` is `None`.
    pub(crate) fn set(key: &'static str, value: Option<&str>) -> Self {
        let previous = std::env::var_os(key);
        match value {
            // SAFETY: single-threaded test context; no concurrent env readers.
            Some(value) => unsafe { std::env::set_var(key, value) },
            // SAFETY: single-threaded test context; no concurrent env readers.
            None => unsafe { std::env::remove_var(key) },
        }
        Self { key, previous }
    }
}

#[cfg(test)]
#[allow(unsafe_code)] // test-only env guard restores the serialized mutation.
impl Drop for SurfaceEnvVarGuard {
    fn drop(&mut self) {
        match &self.previous {
            // SAFETY: single-threaded test context; no concurrent env readers.
            Some(previous) => unsafe { std::env::set_var(self.key, previous) },
            // SAFETY: single-threaded test context; no concurrent env readers.
            None => unsafe { std::env::remove_var(self.key) },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Daemon side, the forwarded header is authoritative: the daemon's own env
    /// list never applies to a proxied call, an absent header means no list, and
    /// an undecodable header allows nothing.
    #[tokio::test]
    async fn forwarded_connection_list_is_authoritative_and_fails_closed() {
        let _tools = SurfaceEnvVarGuard::set(TOOLS_ENV, Some("status"));
        let forwarded = connection_tools_from_header(Some(b"search_text,find_references"));
        with_connection_tools(forwarded, async {
            assert!(allow_list_refusal("find_references", None).is_none());
            assert!(allow_list_refusal("status", None).is_some());
        })
        .await;
        with_connection_tools(connection_tools_from_header(None), async {
            assert!(allow_list_refusal("get_repo_map", None).is_none());
        })
        .await;
        let garbled = connection_tools_from_header(Some(&[0xff, 0xfe]));
        with_connection_tools(garbled, async {
            assert!(allow_list_refusal("status", None).is_some());
        })
        .await;
        // Outside a proxied dispatch, this process's list applies.
        assert!(allow_list_refusal("search_text", None).is_some());
    }

    #[test]
    fn refusal_says_when_a_name_is_not_a_tool_at_all() {
        let _tools = SurfaceEnvVarGuard::set(TOOLS_ENV, Some("status"));
        let unknown = allow_list_refusal("serch_text", None).unwrap();
        assert!(
            unknown.starts_with("'serch_text' is not a tool of this server"),
            "{unknown}"
        );
        let unlisted = allow_list_refusal("search_text", None).unwrap();
        assert!(unlisted.contains("only the operator can add"), "{unlisted}");
    }

    #[test]
    fn allow_list_instructions_stay_within_the_byte_cap_for_the_longest_list() {
        let every: String = TOOL_GROUPS
            .iter()
            .flat_map(|(_, members)| members.iter().copied())
            .collect::<Vec<_>>()
            .join(",");
        let unknown = vec!["an_operator_typo_that_is_quite_long"; 200].join(",");
        let conflict = "SYMFORGE_SURFACE=compact is ignored because SYMFORGE_TOOLS is set";
        for spec in [every.clone(), format!("{every},{unknown}")] {
            let list = ToolAllowList::parse(&spec);
            assert_eq!(list.allowed.len(), 40);
            for conflict in [None, Some(conflict)] {
                let text = allow_list_instructions(&list, conflict);
                assert!(
                    text.len() <= SERVER_INSTRUCTIONS_MAX_BYTES,
                    "{} B: {text}",
                    text.len()
                );
                assert!(text.contains("40 of 40") || text.contains("symforge_retrieve"));
            }
        }
        // A short list is named in full.
        let text = allow_list_instructions(&ToolAllowList::parse("read"), None);
        assert!(text.contains("search_symbols, get_symbol"), "{text}");
        assert!(text.contains("not callable"), "{text}");
    }

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
        // The 30,000 B target is met by the opt-in core profile; this budget
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

    /// SF-TE-05: the opt-in core profile (`SYMFORGE_SURFACE=core`) stays under
    /// 30,000 B. Same measurement as the full budget test.
    #[test]
    fn core_surface_tools_list_under_byte_budget() {
        const CORE_SURFACE_BUDGET_BYTES: usize = 30_000;
        let (count, bytes) = list_tools_schema_bytes(SurfaceProfile::Core);
        eprintln!("core surface tools/list: {count} tools, {bytes} B");
        assert_eq!(count, crate::stel::surface::CORE_TOOL_NAMES.len());
        assert!(
            bytes <= CORE_SURFACE_BUDGET_BYTES,
            "core tools/list JSON must be <= {CORE_SURFACE_BUDGET_BYTES} B; got {bytes} B"
        );
    }

    /// SF-TE-05: no core tool description sends the agent to a tool core does
    /// not list. Parameter text is reported, not asserted.
    #[test]
    fn core_descriptions_name_no_unlisted_tool() {
        let off_core: Vec<String> = list_tools_for_profile(SurfaceProfile::Full)
            .into_iter()
            .map(|t| t.name.to_string())
            .filter(|n| !crate::stel::surface::CORE_TOOL_NAMES.contains(&n.as_str()))
            .collect();
        let named_in = |text: &str| -> Vec<String> {
            let words: Vec<&str> = text
                .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .collect();
            off_core
                .iter()
                .filter(|n| words.contains(&n.as_str()))
                .cloned()
                .collect()
        };
        for (name, _) in CORE_DESCRIPTIONS {
            assert!(
                crate::stel::surface::CORE_TOOL_NAMES.contains(name),
                "{name}"
            );
        }
        for tool in list_tools_for_profile(SurfaceProfile::Core) {
            let named = named_in(tool.description.as_deref().unwrap_or_default());
            assert!(
                named.is_empty(),
                "core `{}` description names {named:?}",
                tool.name
            );
            let schema = serde_json::to_string(tool.input_schema.as_ref()).unwrap();
            let named = named_in(&schema);
            if !named.is_empty() {
                eprintln!(
                    "core `{}` parameter text names off-core {named:?}",
                    tool.name
                );
            }
        }
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
        assert_eq!(
            server_instructions(SurfaceProfile::Core),
            Some(CORE_SURFACE_INSTRUCTIONS)
        );
        assert!(CORE_SURFACE_INSTRUCTIONS.contains("SYMFORGE_SURFACE=full"));

        let names = |tools: Vec<Tool>| -> Vec<String> {
            tools.into_iter().map(|t| t.name.to_string()).collect()
        };
        let full = names(list_tools_for_profile(SurfaceProfile::Full));
        let core = names(list_tools_for_profile(SurfaceProfile::Core));
        let compact = names(crate::stel::compact_surface_tools());
        let every: Vec<&String> = full.iter().chain(compact.iter()).collect();
        for (profile, served) in [
            (SurfaceProfile::Full, &full),
            (SurfaceProfile::Core, &core),
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
