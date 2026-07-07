//! `symforge init` command — client-aware Claude/Claude Desktop/Codex/Gemini/Kilo Code configuration.
//!
//! Strategy:
//! 1. Discover the absolute path of the running symforge binary.
//! 2. Configure Claude, Claude Desktop, Codex, Gemini, Kilo Code, or all based on the selected client target.
//! 3. For Claude (Code), merge symforge hook entries into `~/.claude/settings.json`
//!    and register the MCP server in `~/.claude.json`.
//! 4. For Claude Desktop, register the MCP server in `claude_desktop_config.json`.
//!    On Windows, a `.cmd` wrapper is generated to fix the System32 CWD issue.
//! 5. For Codex, register the MCP server in `~/.codex/config.toml`.
//! 6. For Kilo Code, register the MCP server in `.kilocode/mcp.json` (workspace-local).
//! 7. Ensure runtime `.symforge/` state exists (global home when cwd is unsafe).
//!
//! Identification: any hook entry whose `hooks[].command` contains the substring
//! `"symforge hook"` is considered a symforge-owned entry and will be replaced.

use std::path::PathBuf;

use anyhow::Context;
use serde_json::{Value, json};
use toml_edit::{Array, DocumentMut, Item, Table, value};

use crate::paths;

use crate::cli::InitClient;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct InitPaths {
    claude_settings: PathBuf,
    claude_config: PathBuf,
    claude_memory: PathBuf,
    claude_desktop_config: PathBuf,
    codex_config: PathBuf,
    codex_agents: PathBuf,
    gemini_settings: PathBuf,
    gemini_memory: PathBuf,
    gemini_trusted_folders: PathBuf,
    kilo_vscode_config: PathBuf,
    kilo_rules_guidance: PathBuf,
    cursor_config: PathBuf,
}

impl InitPaths {
    fn from_current_environment(home: &std::path::Path, working_dir: &std::path::Path) -> Self {
        let windows_appdata = std::env::var_os("APPDATA").map(PathBuf::from);
        Self::from_home_working_dir_and_desktop_config(
            home,
            working_dir,
            claude_desktop_config_path(home, windows_appdata),
        )
    }

    fn from_home_working_dir_and_desktop_config(
        home: &std::path::Path,
        working_dir: &std::path::Path,
        claude_desktop_config: PathBuf,
    ) -> Self {
        Self {
            claude_settings: home.join(".claude").join("settings.json"),
            claude_config: home.join(".claude.json"),
            claude_memory: home.join(".claude").join("CLAUDE.md"),
            claude_desktop_config,
            codex_config: home.join(".codex").join("config.toml"),
            codex_agents: home.join(".codex").join("AGENTS.md"),
            gemini_settings: home.join(".gemini").join("settings.json"),
            gemini_memory: home.join(".gemini").join("GEMINI.md"),
            gemini_trusted_folders: home.join(".gemini").join("trustedFolders.json"),
            kilo_vscode_config: working_dir.join(".kilocode").join("mcp.json"),
            kilo_rules_guidance: working_dir
                .join(".kilocode")
                .join("rules")
                .join("symforge.md"),
            cursor_config: home.join(".cursor").join("mcp.json"),
        }
    }
}

pub(crate) fn claude_desktop_config_path(
    home: &std::path::Path,
    windows_appdata: Option<PathBuf>,
) -> PathBuf {
    // Claude Desktop config path varies by platform:
    // - Windows: %APPDATA%\Claude\claude_desktop_config.json
    // - macOS:   ~/Library/Application Support/Claude/claude_desktop_config.json
    // - Linux:   ~/.config/Claude/claude_desktop_config.json
    if cfg!(windows) {
        windows_appdata
            .unwrap_or_else(|| home.join("AppData").join("Roaming"))
            .join("Claude")
            .join("claude_desktop_config.json")
    } else if cfg!(target_os = "macos") {
        home.join("Library")
            .join("Application Support")
            .join("Claude")
            .join("claude_desktop_config.json")
    } else {
        home.join(".config")
            .join("Claude")
            .join("claude_desktop_config.json")
    }
}

const CODEX_STARTUP_TIMEOUT_SEC: i64 = 30;
const CODEX_TOOL_TIMEOUT_SEC: i64 = 120;
const SYMFORGE_GUIDANCE_START: &str = "<!-- SYMFORGE START -->";
const SYMFORGE_GUIDANCE_END: &str = "<!-- SYMFORGE END -->";

/// Entry point called by main.rs for `symforge init`.
pub fn run_init(client: InitClient) -> anyhow::Result<()> {
    let home = dirs::home_dir().context("cannot determine home directory")?;
    let working_dir =
        std::env::current_dir().context("cannot determine current working directory")?;
    let binary_path = discover_binary_path();
    let paths = InitPaths::from_current_environment(&home, &working_dir);

    run_init_with_paths(client, paths, &home, &working_dir, &binary_path)
}

/// Entry point for `symforge init --scan [--apply --serve-url <url> [--serve-key <key>]]`.
///
/// Default (`--scan` alone) is non-destructive: it scans the known MCP harness
/// configs and reports each client's SymForge attach status (and, when a serve
/// URL is supplied, the dry-run plan). With `--apply` it backs up and writes the
/// HTTP attach entry into each config. Reuses the client-path knowledge in this
/// module via [`crate::cli::harness::HarnessRegistry`].
pub fn run_scan(
    apply: bool,
    serve_url: Option<String>,
    serve_key: Option<String>,
) -> anyhow::Result<()> {
    use crate::cli::harness::{AttachEntry, HarnessRegistry, HarnessState};
    use crate::cli::harness_apply::{self, ApplyOutcome, PlannedAction};

    let registry = HarnessRegistry::known()?;

    // The attach entry is only meaningful when a serve URL is supplied. For a
    // bare `--scan` with no URL, report installed-status only (the comparison
    // target is an empty URL, which simply distinguishes present-vs-absent).
    let entry = AttachEntry::new(serve_url.clone().unwrap_or_default(), serve_key.clone());

    if apply {
        let url = serve_url.as_deref().filter(|u| !u.is_empty());
        if url.is_none() {
            anyhow::bail!(
                "`init --scan --apply` requires `--serve-url <url>` (the running `symforge serve` attach URL)"
            );
        }

        let plan = harness_apply::plan(&registry, &entry);
        eprintln!("Applying SymForge attach entry to discovered harnesses:");
        let outcomes = harness_apply::apply(&plan);
        for outcome in &outcomes {
            match outcome {
                ApplyOutcome::Wrote {
                    id,
                    config_path,
                    backup,
                } => {
                    let where_backup = backup
                        .as_ref()
                        .map(|b| format!(" (backup: {})", b.backup.display()))
                        .unwrap_or_default();
                    eprintln!(
                        "  [written] {} -> {}{}",
                        id.display_name(),
                        config_path.display(),
                        where_backup
                    );
                }
                ApplyOutcome::Skipped { id, reason } => {
                    eprintln!("  [skipped] {} ({reason})", id.display_name());
                }
                ApplyOutcome::Failed { id, reason } => {
                    eprintln!("  [error]   {} ({reason})", id.display_name());
                }
            }
        }
        return Ok(());
    }

    // Report-only (scan / dry-run preview).
    if serve_url.is_some() {
        let plan = harness_apply::plan(&registry, &entry);
        eprintln!("Dry-run plan (no files modified). Re-run with --apply to write:");
        for change in &plan.changes {
            let action = match &change.action {
                PlannedAction::Add => "would add".to_string(),
                PlannedAction::Refresh => "would refresh".to_string(),
                PlannedAction::Skip(reason) => format!("skip ({reason})"),
                PlannedAction::Error(reason) => format!("error ({reason})"),
            };
            eprintln!(
                "  {} - {action} - {}",
                change.id.display_name(),
                change.config_path.display()
            );
        }
    } else {
        eprintln!("SymForge harness scan (report only):");
        for status in registry.scan(&entry) {
            let label = match &status.state {
                HarnessState::NotInstalled => "not installed".to_string(),
                HarnessState::Absent => "no SymForge entry".to_string(),
                HarnessState::PresentCurrent => "SymForge entry present".to_string(),
                HarnessState::PresentStale => {
                    "SymForge entry present (different URL/key)".to_string()
                }
                HarnessState::Malformed(why) => format!("config does not parse: {why}"),
            };
            eprintln!(
                "  {} - {label} - {}",
                status.id.display_name(),
                status.config_path.display()
            );
        }
        eprintln!(
            "Pass --serve-url <url> [--serve-key <key>] to preview, then add --apply to write."
        );
    }

    Ok(())
}

/// Testable core for `symforge init` with injected paths.
pub fn run_init_with_context(
    client: InitClient,
    home_dir: &std::path::Path,
    working_dir: &std::path::Path,
    binary_path: &std::path::Path,
) -> anyhow::Result<()> {
    // Injected-path construction (tests + non-prod callers): use the home-relative
    // Claude Desktop config path, never the host's real APPDATA, so paths stay
    // deterministic. Production wiring uses `from_current_environment`.
    let paths = InitPaths::from_home_working_dir_and_desktop_config(
        home_dir,
        working_dir,
        claude_desktop_config_path(home_dir, None),
    );

    run_init_with_paths(client, paths, home_dir, working_dir, binary_path)
}

fn run_init_with_paths(
    client: InitClient,
    paths: InitPaths,
    home_dir: &std::path::Path,
    working_dir: &std::path::Path,
    binary_path: &std::path::Path,
) -> anyhow::Result<()> {
    let registration_binary_path = binary_path_for_registration(binary_path, home_dir)?;
    let binary_path_str = registration_binary_path.display().to_string();

    if matches!(client, InitClient::Claude | InitClient::All) {
        merge_hooks_into_settings(&paths.claude_settings, &registration_binary_path)?;
        eprintln!(
            "Claude hooks installed in {}",
            paths.claude_settings.display()
        );

        register_mcp_server(&paths.claude_config, &binary_path_str)?;
        eprintln!(
            "Claude MCP server registered in {}",
            paths.claude_config.display()
        );

        upsert_guidance_markdown(&paths.claude_memory, &claude_guidance_block())?;
        eprintln!(
            "Claude guidance written to {}",
            paths.claude_memory.display()
        );
    }

    if matches!(client, InitClient::ClaudeDesktop | InitClient::All) {
        register_claude_desktop_mcp_server_with_home(
            &paths.claude_desktop_config,
            &binary_path_str,
            home_dir,
        )?;
        eprintln!(
            "Claude Desktop MCP server registered in {}",
            paths.claude_desktop_config.display()
        );
    }

    if matches!(client, InitClient::Codex | InitClient::All) {
        register_codex_mcp_server(&paths.codex_config, &binary_path_str)?;
        eprintln!(
            "Codex MCP server registered in {}",
            paths.codex_config.display()
        );

        upsert_guidance_markdown(&paths.codex_agents, &codex_guidance_block())?;
        eprintln!("Codex guidance written to {}", paths.codex_agents.display());
        eprintln!(
            "note: Codex gets MCP tools only. No documented Codex hook/session-start enrichment interface was found, so transparent enrichment remains Claude-only."
        );
    }

    if matches!(client, InitClient::Gemini | InitClient::All) {
        register_gemini_mcp_server(&paths.gemini_settings, &binary_path_str)?;
        eprintln!(
            "Gemini MCP server registered in {}",
            paths.gemini_settings.display()
        );

        upsert_guidance_markdown(&paths.gemini_memory, &gemini_guidance_block())?;
        eprintln!(
            "Gemini guidance written to {}",
            paths.gemini_memory.display()
        );

        match gemini_workspace_trust_warning(
            &paths.gemini_settings,
            &paths.gemini_trusted_folders,
            working_dir,
        ) {
            Ok(Some(warning)) => eprintln!("{warning}"),
            Ok(None) => {}
            Err(error) => eprintln!(
                "warning: could not evaluate Gemini folder trust from {}: {error}",
                paths.gemini_trusted_folders.display()
            ),
        }
    }

    if matches!(client, InitClient::KiloCode | InitClient::All) {
        register_kilo_mcp_server(&paths.kilo_vscode_config, &binary_path_str)?;
        eprintln!(
            "Kilo Code MCP server registered in {}",
            paths.kilo_vscode_config.display()
        );

        upsert_guidance_markdown(&paths.kilo_rules_guidance, &kilo_guidance_block())?;
        eprintln!(
            "Kilo Code guidance written to {}",
            paths.kilo_rules_guidance.display()
        );
    }

    if matches!(client, InitClient::Cursor | InitClient::All) {
        register_cursor_mcp_server(&paths.cursor_config, &binary_path_str)?;
        eprintln!(
            "Cursor MCP server registered in {}",
            paths.cursor_config.display()
        );
    }

    paths::ensure_runtime_symforge_dir(Some(working_dir))
        .context("ensuring symforge runtime data directory")?;

    // Registration writes ABSOLUTE binary paths, so the MCP clients we just wired
    // always launch this exact binary. But the user's own bare `symforge` CLI
    // invocations resolve through `$PATH` and may hit a DIFFERENT (stale) install
    // that shadows the one being registered. Warn loudly with the exact fix; the
    // detector never executes anything.
    if let Some(report) = crate::path_shadow::detect_shadow(&registration_binary_path) {
        eprintln!("{}", crate::path_shadow::format_shadow_warning(&report));
    }

    eprintln!("symforge init complete");

    Ok(())
}

/// Read a client config file as UTF-8 text, stripping a leading byte-order mark.
///
/// Windows tools (Notepad, PowerShell `Set-Content -Encoding UTF8`) prepend a
/// UTF-8 BOM; `serde_json` and `toml_edit` reject it with "expected value at
/// line 1 column 1", which aborts `symforge init --client all` against real
/// user configs. Stripping at the read boundary keeps every parser working;
/// the merged file is rewritten without the BOM.
pub(crate) fn read_config_text(path: &std::path::Path) -> anyhow::Result<String> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    Ok(match text.strip_prefix('\u{feff}') {
        Some(stripped) => stripped.to_owned(),
        None => text,
    })
}

/// Merge symforge hook entries into `settings_path`, creating it if necessary.
///
/// This is the testable core of `run_init`. Integration tests can pass a temp-dir path
/// instead of the real `~/.claude/settings.json`.
///
/// `binary_path` is the absolute path of the symforge binary.
pub fn merge_hooks_into_settings(
    settings_path: &std::path::Path,
    binary_path: &std::path::Path,
) -> anyhow::Result<()> {
    // Ensure parent dir exists.
    if let Some(parent) = settings_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }

    // Read existing settings or start with empty object.
    let mut settings: Value = if settings_path.exists() {
        let settings_json = read_config_text(settings_path)?;
        serde_json::from_str(&settings_json)
            .with_context(|| format!("parsing {}", settings_path.display()))?
    } else {
        json!({})
    };

    // Normalise binary path to forward slashes for JSON command strings.
    let binary_str = binary_path.display().to_string().replace('\\', "/");

    // Merge hooks in-place.
    merge_symforge_hooks(&mut settings, &binary_str);

    // Write back.
    let pretty = serde_json::to_string_pretty(&settings)?;
    std::fs::write(settings_path, pretty)
        .with_context(|| format!("writing {}", settings_path.display()))?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Tool name constants
// ---------------------------------------------------------------------------

const SYMFORGE_TOOL_NAMES: &[&str] = &[
    "mcp__symforge__health",
    "mcp__symforge__health_compact",
    "mcp__symforge__checkpoint_now",
    "mcp__symforge__index_folder",
    "mcp__symforge__validate_file_syntax",
    "mcp__symforge__get_file_content",
    "mcp__symforge__get_symbol",
    "mcp__symforge__get_repo_map",
    "mcp__symforge__get_file_context",
    "mcp__symforge__get_symbol_context",
    "mcp__symforge__search_symbols",
    "mcp__symforge__search_text",
    "mcp__symforge__search_files",
    "mcp__symforge__find_references",
    "mcp__symforge__find_dependents",
    "mcp__symforge__inspect_match",
    "mcp__symforge__analyze_file_impact",
    "mcp__symforge__what_changed",
    "mcp__symforge__diff_symbols",
    "mcp__symforge__detect_impact",
    "mcp__symforge__explore",
    "mcp__symforge__replace_symbol_body",
    "mcp__symforge__edit_within_symbol",
    "mcp__symforge__insert_symbol",
    "mcp__symforge__delete_symbol",
    "mcp__symforge__batch_edit",
    "mcp__symforge__batch_insert",
    "mcp__symforge__batch_rename",
    "mcp__symforge__ask",
    "mcp__symforge__conventions",
    "mcp__symforge__edit_plan",
    "mcp__symforge__context_inventory",
    "mcp__symforge__investigation_suggest",
    "mcp__symforge__symforge_retrieve",
    "mcp__symforge__status",
    "mcp__symforge__symforge",
    "mcp__symforge__symforge_edit",
];

const CLAUDE_ALWAYS_ALLOW: &[&str] = &[
    "health",
    "health_compact",
    "checkpoint_now",
    "get_repo_map",
    "explore",
    "validate_file_syntax",
    "get_file_content",
    "get_file_context",
    "get_symbol",
    "get_symbol_context",
    "search_symbols",
    "search_text",
    "search_files",
    "find_references",
    "find_dependents",
    "inspect_match",
    "what_changed",
    "analyze_file_impact",
    "diff_symbols",
    "detect_impact",
    "index_folder",
    "replace_symbol_body",
    "edit_within_symbol",
    "insert_symbol",
    "delete_symbol",
    "batch_edit",
    "batch_rename",
    "batch_insert",
    "ask",
    "conventions",
    "edit_plan",
    "context_inventory",
    "investigation_suggest",
    "symforge_retrieve",
    "status",
    "symforge",
    "symforge_edit",
];

fn merge_allowed_tools(settings: &mut Value) {
    if !settings["allowedTools"].is_array() {
        settings["allowedTools"] = json!([]);
    }
    let allowed = settings["allowedTools"].as_array_mut().expect("is array");
    for tool_name in SYMFORGE_TOOL_NAMES {
        let val = Value::String(tool_name.to_string());
        if !allowed.contains(&val) {
            allowed.push(val);
        }
    }
}

// ---------------------------------------------------------------------------
// Core merge logic (pub for unit testing)
// ---------------------------------------------------------------------------

/// Merge symforge hook entries into an existing `settings` Value in-place.
///
/// `binary_path` is the absolute path of the symforge binary (already
/// normalised to forward-slash on Windows).
pub fn merge_symforge_hooks(settings: &mut Value, binary_path: &str) {
    // Ensure `hooks` key is an object.
    if !settings["hooks"].is_object() {
        settings["hooks"] = json!({});
    }

    // Build fresh symforge entries.
    let post_tool_use_entries = build_post_tool_use_entries(binary_path);
    let pre_tool_use_entries = build_pre_tool_use_entries(binary_path);
    let session_start_entries = build_session_start_entries(binary_path);
    let user_prompt_submit_entries = build_user_prompt_submit_entries(binary_path);

    {
        let hooks = settings["hooks"]
            .as_object_mut()
            .expect("hooks is an object");
        merge_event_entries(hooks, "PostToolUse", post_tool_use_entries);
        merge_event_entries(hooks, "PreToolUse", pre_tool_use_entries);
        merge_event_entries(hooks, "SessionStart", session_start_entries);
        merge_event_entries(hooks, "UserPromptSubmit", user_prompt_submit_entries);
    }

    merge_allowed_tools(settings);
}

// ---------------------------------------------------------------------------
// Entry builders
// ---------------------------------------------------------------------------

fn build_post_tool_use_entries(binary_path: &str) -> Vec<Value> {
    vec![json!({
        "matcher": "Read|Edit|Write|Grep",
        "hooks": [{"type": "command", "command": format!("{binary_path} hook"), "timeout": 5}]
    })]
}

fn build_pre_tool_use_entries(binary_path: &str) -> Vec<Value> {
    // Single entry matching all tools where SymForge can help.
    // The pre-tool handler reads tool_name from stdin and outputs a suggestion.
    // When the SymForge sidecar is already running (agent actively using MCP
    // tools), the hint is suppressed to avoid noise — see run_hook() in hook.rs.
    vec![json!({
        "matcher": "Grep|Read|Glob|Edit",
        "hooks": [{"type": "command", "command": format!("{binary_path} hook pre-tool"), "timeout": 2}]
    })]
}

fn build_session_start_entries(binary_path: &str) -> Vec<Value> {
    vec![json!({
        "matcher": "startup|resume",
        "hooks": [{"type": "command", "command": format!("{binary_path} hook session-start"), "timeout": 5}]
    })]
}

fn build_user_prompt_submit_entries(binary_path: &str) -> Vec<Value> {
    vec![json!({
        "hooks": [{"type": "command", "command": format!("{binary_path} hook prompt-submit"), "timeout": 5}]
    })]
}

// ---------------------------------------------------------------------------
// Merge helpers
// ---------------------------------------------------------------------------

/// Returns `true` if a hook entry array contains a symforge hook command.
fn is_symforge_entry(entry: &Value) -> bool {
    if let Some(hooks) = entry["hooks"].as_array() {
        hooks.iter().any(|h| {
            h["command"]
                .as_str()
                .map(|cmd| cmd.contains("symforge") && cmd.contains(" hook"))
                .unwrap_or(false)
        })
    } else {
        false
    }
}

/// Merge `new_entries` into the `event_key` array of the hooks object.
///
/// Existing symforge entries (identified by `is_symforge_entry`) are filtered
/// out before appending the fresh entries, which achieves idempotency.
fn merge_event_entries(
    hooks: &mut serde_json::Map<String, Value>,
    event_key: &str,
    new_entries: Vec<Value>,
) {
    let existing: Vec<Value> = hooks
        .get(event_key)
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    // Keep only non-symforge entries.
    let mut retained: Vec<Value> = existing
        .into_iter()
        .filter(|e| !is_symforge_entry(e))
        .collect();

    // Append fresh symforge entries at the end.
    retained.extend(new_entries);

    hooks.insert(event_key.to_string(), Value::Array(retained));
}

// ---------------------------------------------------------------------------
// Shared JSON MCP-entry merge helpers (G-036 init/update coherence)
// ---------------------------------------------------------------------------
//
// Every JSON harness (Claude Code, Claude Desktop, Cursor, Gemini, Kilo) writes
// its `mcpServers.symforge` entry through these. Invariant: re-registration
// (e.g. `symforge update` -> `symforge init --client all`) REFRESHES the managed
// launcher path but never clobbers a user-set env value, allowlist entry, or
// unknown field. The 8.10.1 update wiped `SYMFORGE_SURFACE=full`; merging into
// the existing object in place instead of replacing it kills that defect class.

/// Get (or create) the `mcpServers.symforge` entry as a mutable object so
/// callers merge INTO the existing entry rather than replacing it wholesale.
fn symforge_json_entry_mut(config: &mut Value) -> &mut serde_json::Map<String, Value> {
    if !config["mcpServers"].is_object() {
        config["mcpServers"] = json!({});
    }
    let servers = config["mcpServers"]
        .as_object_mut()
        .expect("mcpServers is an object");
    let slot = servers.entry("symforge").or_insert_with(|| json!({}));
    if !slot.is_object() {
        *slot = json!({});
    }
    slot.as_object_mut().expect("symforge entry is an object")
}

/// Insert each `(key, value)` env default into the entry's `env` object ONLY
/// when the key is absent — a present env key's value is preserved verbatim.
fn insert_env_defaults(entry: &mut serde_json::Map<String, Value>, defaults: &[(&str, &str)]) {
    if defaults.is_empty() {
        return;
    }
    if !entry.get("env").map(Value::is_object).unwrap_or(false) {
        entry.insert("env".to_string(), json!({}));
    }
    let env = entry
        .get_mut("env")
        .and_then(Value::as_object_mut)
        .expect("env is an object");
    for (key, val) in defaults {
        env.entry((*key).to_string())
            .or_insert_with(|| Value::String((*val).to_string()));
    }
}

/// Union `names` into the entry's `key` array (e.g. `alwaysAllow`), preserving
/// order and every user-added entry — names already present are not duplicated.
fn union_allow_names(entry: &mut serde_json::Map<String, Value>, key: &str, names: &[&str]) {
    if !entry.get(key).map(Value::is_array).unwrap_or(false) {
        entry.insert(key.to_string(), json!([]));
    }
    let arr = entry
        .get_mut(key)
        .and_then(Value::as_array_mut)
        .expect("allow list is an array");
    for name in names {
        let val = Value::String((*name).to_string());
        if !arr.contains(&val) {
            arr.push(val);
        }
    }
}

/// Register symforge as an MCP server in `~/.claude.json` using the absolute binary path.
///
/// This ensures Claude Code launches the native binary directly — no shell, no .cmd wrapper,
/// no Node.js intermediary. Works on all platforms.
pub fn register_mcp_server(
    claude_json_path: &std::path::Path,
    binary_path: &str,
) -> anyhow::Result<()> {
    let mut config: Value = if claude_json_path.exists() {
        let config_json = read_config_text(claude_json_path)?;
        serde_json::from_str(&config_json)
            .with_context(|| format!("parsing {}", claude_json_path.display()))?
    } else {
        json!({})
    };

    // Use backslashes on Windows for the command path (Claude Code spawns natively, not via shell).
    let command_path = native_command_path(binary_path);

    let entry = symforge_json_entry_mut(&mut config);
    // Refresh the managed launcher path; preserve every other user-set field.
    entry.insert("command".to_string(), Value::String(command_path));
    entry.entry("args".to_string()).or_insert_with(|| json!([]));
    entry
        .entry("disabled".to_string())
        .or_insert_with(|| Value::Bool(false));
    // Claude Code has native deferred tool loading, so pin the full surface —
    // this makes the 37-name allowlist coherent (spec §4). Never clobber a
    // user-set value (the 8.10.1 SYMFORGE_SURFACE=full wipe).
    insert_env_defaults(entry, &[("SYMFORGE_SURFACE", "full")]);
    union_allow_names(entry, "alwaysAllow", CLAUDE_ALWAYS_ALLOW);

    let pretty = serde_json::to_string_pretty(&config)?;
    std::fs::write(claude_json_path, pretty)
        .with_context(|| format!("writing {}", claude_json_path.display()))?;

    Ok(())
}

/// Register symforge as an MCP server in Claude Desktop's `claude_desktop_config.json`.
///
/// On Windows, Claude Desktop launches MCP servers with CWD = `C:\WINDOWS\System32`,
/// which causes symforge to crash with "Access is denied (os error 5)" when it tries
/// to write files. We work around this by generating a thin `.cmd` wrapper that changes
/// directory to `%USERPROFILE%` before launching the real binary.
///
/// On other platforms the binary is registered directly (no CWD issue).
pub fn register_claude_desktop_mcp_server(
    desktop_config_path: &std::path::Path,
    binary_path: &str,
) -> anyhow::Result<()> {
    let home = dirs::home_dir().context("cannot determine home directory")?;
    register_claude_desktop_mcp_server_with_home(desktop_config_path, binary_path, &home)
}

fn register_claude_desktop_mcp_server_with_home(
    desktop_config_path: &std::path::Path,
    binary_path: &str,
    home_dir: &std::path::Path,
) -> anyhow::Result<()> {
    if let Some(parent) = desktop_config_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }

    let mut config: Value = if desktop_config_path.exists() {
        let config_json = read_config_text(desktop_config_path)?;
        serde_json::from_str(&config_json)
            .with_context(|| format!("parsing {}", desktop_config_path.display()))?
    } else {
        json!({})
    };

    let desktop_binary_path =
        desktop_binary_for_registration(std::path::Path::new(binary_path), home_dir)?;
    let desktop_binary_path_str = desktop_binary_path.display().to_string();

    // Discover the operator's workspace at install time (TR-03 / FR-013). The
    // `symforge init` process CWD is the project the operator is in, so
    // `find_project_root` resolves the real workspace here — unlike cold start
    // under Claude Desktop, whose CWD is `System32` (Windows) and is useless.
    // We thread the discovered root into both the launcher CWD and the
    // registered `env`, so the server indexes a populated workspace instead of
    // binding an empty index and emitting the TR-02 dead-end.
    let workspace_root = crate::discovery::find_project_root();
    let workspace_root_str = workspace_root.as_ref().map(|r| r.display().to_string());

    let command_path = if cfg!(windows) {
        // Generate a wrapper script that sets CWD before launching symforge.
        // It lives in the Claude Desktop config dir (%APPDATA%\Claude) — a
        // stable, symforge-managed injection point — NEVER next to the npm
        // platform binary: npm wipes that bin dir on every package swap,
        // which deleted the wrapper and left Desktop pointing at nothing.
        let wrapper_dir = desktop_config_path
            .parent()
            .context("cannot determine Claude Desktop config directory")?;
        let wrapper_path = create_desktop_wrapper_windows(
            &desktop_binary_path_str,
            workspace_root_str.as_deref(),
            wrapper_dir,
        )?;
        native_command_path(&wrapper_path)
    } else {
        native_command_path(&desktop_binary_path_str)
    };

    // Write a proven env instead of `{}` (TR-03 / FR-013):
    //  - SYMFORGE_SURFACE=compact pins the compact surface explicitly in the
    //    registered config (the token-sensitive escape hatch; the server default
    //    is now full), keeping the opt-in visible to operators.
    //  - SYMFORGE_WORKSPACE_ROOT carries the discovered workspace so cold start
    //    populates the index even when the launcher CWD is unusable. Only set
    //    when a real root was discovered (the server validates it again at
    //    startup via the same trust-boundary guard).
    let mut env_defaults: Vec<(&str, &str)> = vec![("SYMFORGE_SURFACE", "compact")];
    if let Some(root) = workspace_root_str.as_deref() {
        env_defaults.push((crate::discovery::WORKSPACE_ROOT_ENV, root));
    }

    let entry = symforge_json_entry_mut(&mut config);
    // Refresh the managed launcher path; preserve every other user-set field,
    // including a user-set env value or an existing SYMFORGE_WORKSPACE_ROOT.
    entry.insert("command".to_string(), Value::String(command_path));
    entry.entry("args".to_string()).or_insert_with(|| json!([]));
    insert_env_defaults(entry, &env_defaults);

    let pretty = serde_json::to_string_pretty(&config)?;
    std::fs::write(desktop_config_path, pretty)
        .with_context(|| format!("writing {}", desktop_config_path.display()))?;

    Ok(())
}

fn binary_path_for_registration(
    binary_path: &std::path::Path,
    _home_dir: &std::path::Path,
) -> anyhow::Result<PathBuf> {
    if !path_is_inside(&std::env::temp_dir(), binary_path) {
        return Ok(binary_path.to_path_buf());
    }

    anyhow::bail!(
        "refusing to register MCP harnesses with temporary SymForge binary {}; \
         run `npm install -g symforge` first, then run `symforge init --client all` from the global install",
        binary_path.display()
    )
}

fn desktop_binary_for_registration(
    binary_path: &std::path::Path,
    home_dir: &std::path::Path,
) -> anyhow::Result<PathBuf> {
    binary_path_for_registration(binary_path, home_dir)
}

fn path_is_inside(parent: &std::path::Path, child: &std::path::Path) -> bool {
    let parent = comparable_path(parent);
    let child = comparable_path(child);
    let parent = parent.trim_end_matches(['\\', '/']);

    child == parent || child.starts_with(&format!("{parent}\\"))
}

fn comparable_path(path: &std::path::Path) -> String {
    let absolute = std::fs::canonicalize(path).unwrap_or_else(|_| {
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(path)
        }
    });
    let normalized = absolute.to_string_lossy().replace('/', "\\");
    if cfg!(windows) {
        normalized.to_ascii_lowercase()
    } else {
        normalized
    }
}

/// Create a `.cmd` wrapper in `wrapper_dir` (the Claude Desktop config dir)
/// that sets CWD before launching the symforge binary by ABSOLUTE path.
///
/// The wrapper must NOT live next to the npm platform binary: npm replaces
/// that `bin/` directory wholesale on every package swap, deleting the
/// generated wrapper and leaving the registered Desktop command pointing at
/// a nonexistent file. The Desktop config dir survives updates and is the
/// same place the registration itself is injected.
///
/// Claude Desktop on Windows launches MCP servers with CWD = `C:\WINDOWS\System32`
/// (a forbidden directory that crashes symforge with "Access is denied" on first
/// write), so the wrapper MUST `cd` somewhere writable before launch. The CWD it
/// chooses also drives cold-start root discovery (`find_project_root`):
///
///  - When init discovered a workspace root, the wrapper `cd`s into it. That
///    directory is both writable (no crash) AND discoverable (cold start indexes
///    the real workspace instead of the home dir — TR-03/FR-013).
///  - When no workspace was discovered, the wrapper falls back to `%USERPROFILE%`
///    — still writable (preserves the original System32-crash fix) but a
///    forbidden root for indexing, so the index stays empty unless the registered
///    `SYMFORGE_WORKSPACE_ROOT` env carries a root. This matches the pre-fix
///    behavior for the no-workspace case (no regression) while the common
///    "operator ran init from their project" case is now populated.
///
/// Returns the absolute path to the wrapper script.
#[cfg(windows)]
fn create_desktop_wrapper_windows(
    binary_path: &str,
    workspace_root: Option<&str>,
    wrapper_dir: &std::path::Path,
) -> anyhow::Result<String> {
    std::fs::create_dir_all(wrapper_dir)
        .with_context(|| format!("creating {}", wrapper_dir.display()))?;
    let wrapper_path = wrapper_dir.join("symforge-desktop.cmd");

    // `cd /d` into the discovered workspace (writable + indexable) when known,
    // else fall back to the always-writable home dir to keep the System32-crash
    // fix. Both are double-quoted; `cd /d` accepts a literal path verbatim.
    // The binary is invoked by ABSOLUTE path (not `%~dp0`): the wrapper no
    // longer lives next to the binary.
    let cd_target = workspace_root.unwrap_or("%USERPROFILE%");
    let script = format!("@echo off\r\ncd /d \"{cd_target}\"\r\n\"{binary_path}\" %*\r\n");

    std::fs::write(&wrapper_path, script)
        .with_context(|| format!("writing {}", wrapper_path.display()))?;

    Ok(wrapper_path.display().to_string())
}

#[cfg(not(windows))]
fn create_desktop_wrapper_windows(
    _binary_path: &str,
    _workspace_root: Option<&str>,
    _wrapper_dir: &std::path::Path,
) -> anyhow::Result<String> {
    unreachable!("desktop wrapper is only created on Windows")
}

/// Register symforge as an MCP server in `~/.codex/config.toml`.
///
/// Codex stores MCP servers under `[mcp_servers.<name>]` tables in TOML.
/// We update only the `symforge` entry and preserve the rest of the file.
pub fn register_codex_mcp_server(
    codex_config_path: &std::path::Path,
    binary_path: &str,
) -> anyhow::Result<()> {
    if let Some(parent) = codex_config_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }

    let config_toml = if codex_config_path.exists() {
        read_config_text(codex_config_path)?
    } else {
        String::new()
    };

    let mut config = if config_toml.trim().is_empty() {
        DocumentMut::new()
    } else {
        config_toml
            .parse::<DocumentMut>()
            .with_context(|| format!("parsing {}", codex_config_path.display()))?
    };

    merge_symforge_codex_server(&mut config, binary_path);

    std::fs::write(codex_config_path, config.to_string())
        .with_context(|| format!("writing {}", codex_config_path.display()))?;

    Ok(())
}

fn merge_symforge_codex_server(config: &mut DocumentMut, binary_path: &str) {
    merge_symforge_codex_server_for_target_os(config, binary_path, std::env::consts::OS);
}

fn merge_symforge_codex_server_for_target_os(
    config: &mut DocumentMut,
    binary_path: &str,
    target_os: &str,
) {
    if !config.as_table().contains_key("mcp_servers") || !config["mcp_servers"].is_table() {
        config["mcp_servers"] = Item::Table(Table::new());
    }

    let mcp_servers = config["mcp_servers"]
        .as_table_mut()
        .expect("mcp_servers must be a table");

    if !mcp_servers.contains_key("symforge") || !mcp_servers["symforge"].is_table() {
        mcp_servers.insert("symforge", Item::Table(Table::new()));
    }

    let symforge = mcp_servers["symforge"]
        .as_table_mut()
        .expect("symforge server entry must be a table");

    symforge["command"] = value(native_command_path(binary_path));
    // Preserve user-tuned timeouts on re-registration (same wipe class as the
    // G-036 env wipe); only seed defaults when absent.
    symforge
        .entry("startup_timeout_sec")
        .or_insert(value(CODEX_STARTUP_TIMEOUT_SEC));
    symforge
        .entry("tool_timeout_sec")
        .or_insert(value(CODEX_TOOL_TIMEOUT_SEC));
    merge_codex_mcp_env_overrides(symforge, target_os);

    merge_codex_allowed_tools(symforge);

    merge_codex_project_doc_fallbacks(config);
}

/// Union the full-surface tool names into Codex's `allowed_tools`,
/// preserving any user-added entries. Codex strips the `mcp__symforge__` prefix,
/// so the unprefixed short names are what the served surface actually exposes —
/// the allowlist matches the surface (spec §4; full-by-init per the 2026-07-03
/// spike gate + 2026-07-06 operator flip).
fn merge_codex_allowed_tools(symforge: &mut Table) {
    let mut names: Vec<String> = symforge
        .get("allowed_tools")
        .and_then(|item| item.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();

    for name in CLAUDE_ALWAYS_ALLOW {
        if !names.iter().any(|existing| existing == name) {
            names.push(name.to_string());
        }
    }

    let mut allow_array = Array::new();
    for name in &names {
        allow_array.push(name.as_str());
    }
    symforge["allowed_tools"] = value(allow_array);
}

fn merge_codex_mcp_env_overrides(symforge: &mut Table, target_os: &str) {
    // Ensure an env table exists — SYMFORGE_SURFACE is written on every OS, not
    // only where an OS override applies.
    if !symforge.contains_key("env") || !symforge["env"].is_table_like() {
        symforge["env"] = Item::Table(Table::new());
    }

    let env = symforge["env"]
        .as_table_like_mut()
        .expect("symforge env must be a table or inline table");

    // Codex defers schema loading (undocumented `tool_search` layer, measured
    // 2026-07-03), so serve the full surface. Never clobber a user-set value
    // (G-036: the 8.10.1 update wiped SYMFORGE_SURFACE=full).
    if env.get("SYMFORGE_SURFACE").is_none() {
        env.insert("SYMFORGE_SURFACE", value("full"));
    }

    for (name, env_value) in codex_mcp_env_overrides_for_target_os(target_os) {
        env.insert(name, value(*env_value));
    }
}

fn codex_mcp_env_overrides_for_target_os(
    target_os: &str,
) -> &'static [(&'static str, &'static str)] {
    match target_os {
        "linux" => &[("SYMFORGE_NO_DAEMON", "1")],
        _ => &[],
    }
}

fn merge_codex_project_doc_fallbacks(config: &mut DocumentMut) {
    let key = "project_doc_fallback_filenames";
    if !config.as_table().contains_key(key) || !config[key].is_array() {
        let mut fallbacks = Array::default();
        fallbacks.push("AGENTS.md");
        fallbacks.push("CLAUDE.md");
        config[key] = value(fallbacks);
        return;
    }

    let fallbacks = config[key]
        .as_array_mut()
        .expect("project_doc_fallback_filenames must be an array");
    for doc_name in ["AGENTS.md", "CLAUDE.md"] {
        let has_doc = fallbacks
            .iter()
            .any(|entry| entry.as_str() == Some(doc_name));
        if !has_doc {
            fallbacks.push(doc_name);
        }
    }
}

/// Register symforge as an MCP server in `~/.gemini/settings.json`.
///
/// Gemini CLI stores MCP servers under `mcpServers` in a JSON settings file.
/// We update only the `symforge` entry and preserve the rest of the file.
pub fn register_gemini_mcp_server(
    gemini_settings_path: &std::path::Path,
    binary_path: &str,
) -> anyhow::Result<()> {
    if let Some(parent) = gemini_settings_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }

    let mut config: Value = if gemini_settings_path.exists() {
        let config_json = read_config_text(gemini_settings_path)?;
        serde_json::from_str(&config_json)
            .with_context(|| format!("parsing {}", gemini_settings_path.display()))?
    } else {
        json!({})
    };

    let command_path = native_command_path(binary_path);

    let entry = symforge_json_entry_mut(&mut config);
    // Refresh the managed launcher path; preserve every other user-set field.
    entry.insert("command".to_string(), Value::String(command_path));
    entry.entry("args".to_string()).or_insert_with(|| json!([]));
    entry
        .entry("timeout".to_string())
        .or_insert_with(|| json!(120000));
    entry
        .entry("trust".to_string())
        .or_insert_with(|| Value::Bool(true));
    // Gemini CLI full-injects (~16k tokens/turn) but accepts all 36 tools
    // (measured 2026-07-03); operator flipped init to full 2026-07-06.
    // `SYMFORGE_SURFACE=compact` stays the documented escape hatch.
    insert_env_defaults(entry, &[("SYMFORGE_SURFACE", "full")]);

    let pretty = serde_json::to_string_pretty(&config)?;
    std::fs::write(gemini_settings_path, pretty)
        .with_context(|| format!("writing {}", gemini_settings_path.display()))?;
    Ok(())
}

/// Register symforge as an MCP server in `.kilocode/mcp.json` (workspace-local).
///
/// Kilo Code (VS Code extension) stores MCP servers under `mcpServers` in a JSON
/// config file. Unlike Claude/Codex/Gemini, this file lives in the project directory
/// rather than the user's home directory.
pub fn register_kilo_mcp_server(
    kilo_config_path: &std::path::Path,
    binary_path: &str,
) -> anyhow::Result<()> {
    if let Some(parent) = kilo_config_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }

    let mut config: Value = if kilo_config_path.exists() {
        let config_json = read_config_text(kilo_config_path)?;
        serde_json::from_str(&config_json)
            .with_context(|| format!("parsing {}", kilo_config_path.display()))?
    } else {
        json!({})
    };

    let command_path = native_command_path(binary_path);

    let entry = symforge_json_entry_mut(&mut config);
    // Refresh the managed launcher path; preserve every other user-set field.
    entry.insert("command".to_string(), Value::String(command_path));
    entry.entry("args".to_string()).or_insert_with(|| json!([]));
    entry
        .entry("disabled".to_string())
        .or_insert_with(|| Value::Bool(false));
    // Kilo full-injects (~16k tokens/turn) but accepts all 36 tools (measured
    // 2026-07-03); operator flipped init to full 2026-07-06. Grant the full
    // names so the allowlist matches the served surface (spec §4);
    // `SYMFORGE_SURFACE=compact` stays the documented escape hatch.
    insert_env_defaults(entry, &[("SYMFORGE_SURFACE", "full")]);
    union_allow_names(entry, "alwaysAllow", CLAUDE_ALWAYS_ALLOW);

    let pretty = serde_json::to_string_pretty(&config)?;
    std::fs::write(kilo_config_path, pretty)
        .with_context(|| format!("writing {}", kilo_config_path.display()))?;
    Ok(())
}

/// Register symforge as an MCP server in Cursor's global `~/.cursor/mcp.json`.
///
/// Cursor launches MCP servers from a fixed `cwd` (commonly the user's home),
/// so a cold `find_project_root` resolves nothing and the server binds an empty
/// index — the home-cwd "Index not loaded" trap. Like Claude Desktop, we
/// discover the operator's workspace at install time (the `symforge init` CWD is
/// the project the operator is in) and thread it into BOTH the per-server `cwd`
/// (Cursor honors a `cwd` field) and `SYMFORGE_WORKSPACE_ROOT`, so cold start
/// indexes a real workspace regardless of how Cursor launches the process. Only
/// the `symforge` entry is touched; the rest of the file is preserved.
pub fn register_cursor_mcp_server(
    cursor_config_path: &std::path::Path,
    binary_path: &str,
) -> anyhow::Result<()> {
    if let Some(parent) = cursor_config_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }

    let mut config: Value = if cursor_config_path.exists() {
        let config_json = read_config_text(cursor_config_path)?;
        serde_json::from_str(&config_json)
            .with_context(|| format!("parsing {}", cursor_config_path.display()))?
    } else {
        json!({})
    };

    let command_path = native_command_path(binary_path);

    // Discover the operator's workspace at install time (same rationale as
    // Claude Desktop registration); thread it into env + cwd below.
    let workspace_root = crate::discovery::find_project_root();
    let workspace_root_str = workspace_root.as_ref().map(|r| r.display().to_string());

    let mut env_defaults: Vec<(&str, &str)> = vec![("SYMFORGE_SURFACE", "full")];
    if let Some(root) = workspace_root_str.as_deref() {
        env_defaults.push((crate::discovery::WORKSPACE_ROOT_ENV, root));
    }

    let entry = symforge_json_entry_mut(&mut config);
    // Refresh the managed launcher path; preserve every other user-set field,
    // including a user-set env value, workspace root, or `cwd`.
    entry.insert("command".to_string(), Value::String(command_path));
    entry.entry("args".to_string()).or_insert_with(|| json!([]));
    insert_env_defaults(entry, &env_defaults);
    // Cursor honors a per-server `cwd`; point it at the discovered workspace so
    // the launch CWD is the project, not the home directory. Preserve an
    // existing user-set `cwd`.
    if let Some(root) = workspace_root_str.as_deref() {
        entry
            .entry("cwd".to_string())
            .or_insert_with(|| Value::String(root.to_string()));
    }

    let pretty = serde_json::to_string_pretty(&config)?;
    std::fs::write(cursor_config_path, pretty)
        .with_context(|| format!("writing {}", cursor_config_path.display()))?;
    Ok(())
}

fn upsert_guidance_markdown(path: &std::path::Path, guidance_block: &str) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }

    let existing = if path.exists() {
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?
    } else {
        String::new()
    };

    let merged = upsert_markdown_block(&existing, guidance_block);
    // Idempotent: an unchanged block must not churn the file's mtime.
    if merged == existing {
        return Ok(());
    }
    std::fs::write(path, merged).with_context(|| format!("writing {}", path.display()))?;

    Ok(())
}

fn gemini_workspace_trust_warning(
    gemini_settings_path: &std::path::Path,
    gemini_trusted_folders_path: &std::path::Path,
    working_dir: &std::path::Path,
) -> anyhow::Result<Option<String>> {
    if !gemini_folder_trust_enabled(gemini_settings_path)? {
        return Ok(None);
    }

    if gemini_workspace_is_trusted(gemini_trusted_folders_path, working_dir)? {
        return Ok(None);
    }

    Ok(Some(format!(
        "warning: Gemini folder trust is enabled, but this workspace is not trusted in {}. \
stdio MCP servers like SymForge will not connect here until the workspace is trusted. \
Trust {} in Gemini (for example via `gemini trust` or Gemini `/permissions`) and restart Gemini.",
        gemini_trusted_folders_path.display(),
        working_dir.display()
    )))
}

fn gemini_folder_trust_enabled(gemini_settings_path: &std::path::Path) -> anyhow::Result<bool> {
    if !gemini_settings_path.exists() {
        return Ok(false);
    }

    let config_json = read_config_text(gemini_settings_path)?;
    let config: Value = serde_json::from_str(&config_json)
        .with_context(|| format!("parsing {}", gemini_settings_path.display()))?;

    Ok(config["security"]["folderTrust"]["enabled"]
        .as_bool()
        .unwrap_or(false))
}

fn gemini_workspace_is_trusted(
    gemini_trusted_folders_path: &std::path::Path,
    working_dir: &std::path::Path,
) -> anyhow::Result<bool> {
    if !gemini_trusted_folders_path.exists() {
        return Ok(false);
    }

    let trust_rules_json = read_config_text(gemini_trusted_folders_path)?;
    let trust_rules: Value = serde_json::from_str(&trust_rules_json)
        .with_context(|| format!("parsing {}", gemini_trusted_folders_path.display()))?;
    let Some(entries) = trust_rules.as_object() else {
        return Ok(false);
    };

    let normalized_working_dir = gemini_real_trust_path(working_dir);
    let mut longest_match_len = 0usize;
    let mut longest_match_status: Option<String> = None;

    for (rule_path, status) in entries {
        let Some(status) = status.as_str() else {
            continue;
        };

        let status = status.to_ascii_uppercase();
        let rule_path = std::path::Path::new(rule_path);
        let effective_path = if status == "TRUST_PARENT" {
            rule_path.parent().unwrap_or(rule_path)
        } else {
            rule_path
        };
        let normalized_effective_path = gemini_real_trust_path(effective_path);
        if !gemini_is_within_root(&normalized_working_dir, &normalized_effective_path) {
            continue;
        }

        let normalized_rule_path = gemini_real_trust_path(rule_path);
        if normalized_rule_path.len() > longest_match_len {
            longest_match_len = normalized_rule_path.len();
            longest_match_status = Some(status);
        }
    }

    Ok(matches!(
        longest_match_status.as_deref(),
        Some("TRUST_FOLDER" | "TRUST_PARENT")
    ))
}

fn normalize_gemini_trust_path(path: &std::path::Path) -> String {
    let mut normalized = path.to_string_lossy().into_owned();
    if cfg!(windows) {
        normalized = normalized.replace('/', "\\");
        if normalized.len() > 3 {
            normalized = normalized.trim_end_matches('\\').to_string();
        }
        normalized = normalized.to_ascii_lowercase();
    } else {
        normalized = normalized.replace('\\', "/");
        if normalized.len() > 1 {
            normalized = normalized.trim_end_matches('/').to_string();
        }
    }
    normalized
}

fn gemini_real_trust_path(path: &std::path::Path) -> String {
    let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    normalize_gemini_trust_path(&canonical)
}

fn gemini_is_within_root(location: &str, root: &str) -> bool {
    if location == root {
        return true;
    }

    let Some(suffix) = location.strip_prefix(root) else {
        return false;
    };

    suffix.starts_with('\\') || suffix.starts_with('/')
}

fn upsert_markdown_block(existing: &str, guidance_block: &str) -> String {
    if let Some(start) = existing.find(SYMFORGE_GUIDANCE_START)
        && let Some(end_marker_start) = existing[start..].find(SYMFORGE_GUIDANCE_END)
    {
        let end = start + end_marker_start + SYMFORGE_GUIDANCE_END.len();
        let mut merged = String::new();
        merged.push_str(&existing[..start]);
        merged.push_str(guidance_block);
        merged.push_str(&existing[end..]);
        return merged;
    }

    if existing.trim().is_empty() {
        return format!("{guidance_block}\n");
    }

    let mut merged = existing.trim_end_matches(['\r', '\n']).to_string();
    merged.push_str("\n\n");
    merged.push_str(guidance_block);
    merged.push('\n');
    merged
}

/// Body of the managed guidance block, exactly as injected between the
/// markers. Standalone by design: it must read correctly in any user's
/// memory file, so it references no host-file section numbers.
const SHARED_GUIDANCE_BODY: &str = r#"## SymForge MCP — task-to-tool map

The granular tools and the 3-tool facade are both live — use whichever answers in one call.

| Task | Call |
|---|---|
| Repo overview | `get_repo_map` (low detail first on large repos) |
| Before reading a file | `get_file_context` — outline/imports/consumers; full read only when exact text is needed |
| Before grepping | `search_text` (`group_by='symbol'`, `follow_refs=true`) |
| Find a function/class/type | `search_symbols`; exact source via `get_symbol` |
| Callers / callees / usage | `find_references`; deep dive via `get_symbol_context` |
| Before writing new code | `conventions` — match the project's patterns |
| Before editing a symbol | `edit_plan`, then `replace_symbol_body` / `edit_within_symbol` / `batch_edit` over text-based edits |
| After editing a file | `analyze_file_impact` |
| Resuming work | `what_changed` |
| Config file looks malformed | `validate_file_syntax` |
| Unsure which tool | `ask` (natural-language routing) |

Raw reads (`get_file_content`/Read) remain correct for docs/configs where literal wording matters, unindexed files, and SymForge errors — say so when falling back."#;

fn shared_guidance_block() -> String {
    format!("{SYMFORGE_GUIDANCE_START}\n{SHARED_GUIDANCE_BODY}\n{SYMFORGE_GUIDANCE_END}")
}

fn claude_guidance_block() -> String {
    shared_guidance_block()
}

fn codex_guidance_block() -> String {
    shared_guidance_block()
}

fn gemini_guidance_block() -> String {
    shared_guidance_block()
}

fn kilo_guidance_block() -> String {
    shared_guidance_block()
}

/// Returns the binary path of the currently running symforge executable.
fn discover_binary_path() -> PathBuf {
    match std::env::current_exe() {
        Ok(path) => {
            let s = path.display().to_string();
            // Warn if the binary is running from an unstable location.
            let is_npx_cache = s.contains("_npx") || s.contains("npx-cache");
            if is_npx_cache || s.ends_with(".cmd") {
                eprintln!(
                    "warning: binary is a temporary npm shim or npx cache entry ({s}); \
                     run: npm install -g symforge && symforge init --client all"
                );
            }
            path
        }
        Err(e) => {
            eprintln!("warning: could not determine symforge binary path: {e}");
            PathBuf::from("symforge")
        }
    }
}

fn native_command_path(binary_path: &str) -> String {
    if cfg!(windows) {
        binary_path.replace('/', "\\")
    } else {
        binary_path.to_string()
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const FAKE_BINARY: &str = "/usr/local/bin/symforge";

    fn run_merge(initial: Value) -> Value {
        let mut settings = initial;
        merge_symforge_hooks(&mut settings, FAKE_BINARY);
        settings
    }

    // --- test_init_creates_hooks_in_empty_settings ---

    #[test]
    fn test_init_creates_hooks_in_empty_settings() {
        let result = run_merge(json!({}));

        let post = result["hooks"]["PostToolUse"]
            .as_array()
            .expect("PostToolUse must be an array");
        let session = result["hooks"]["SessionStart"]
            .as_array()
            .expect("SessionStart must be an array");
        let prompt = result["hooks"]["UserPromptSubmit"]
            .as_array()
            .expect("UserPromptSubmit must be an array");

        assert_eq!(
            post.len(),
            1,
            "PostToolUse must have 1 entry (single stdin-routed entry)"
        );
        assert_eq!(session.len(), 1, "SessionStart must have 1 entry");
        assert_eq!(prompt.len(), 1, "UserPromptSubmit must have 1 entry");
    }

    /// Windows editors and PowerShell write settings.json with a UTF-8 BOM;
    /// init must parse it instead of dying with "expected value at line 1
    /// column 1", and the rewrite must not carry the BOM forward.
    #[test]
    fn test_init_parses_settings_with_utf8_bom() {
        let dir = tempfile::tempdir().unwrap();
        let settings_path = dir.path().join("settings.json");
        std::fs::write(
            &settings_path,
            "\u{feff}{\r\n    \"cleanupPeriodDays\": 99999\r\n}",
        )
        .unwrap();

        merge_hooks_into_settings(&settings_path, std::path::Path::new(FAKE_BINARY))
            .expect("BOM-prefixed settings.json must parse");

        let written = std::fs::read_to_string(&settings_path).unwrap();
        assert!(
            !written.starts_with('\u{feff}'),
            "BOM must not survive the rewrite"
        );
        let settings: Value = serde_json::from_str(&written).unwrap();
        assert_eq!(settings["cleanupPeriodDays"], 99999);
        assert!(
            settings["hooks"]["PostToolUse"].is_array(),
            "hooks must be merged into the BOM'd settings"
        );
    }

    #[test]
    fn test_init_entries_have_correct_commands() {
        let result = run_merge(json!({}));

        let post = &result["hooks"]["PostToolUse"];
        let entry = &post[0];
        let cmd = entry["hooks"][0]["command"].as_str().unwrap();
        assert_eq!(
            cmd, "/usr/local/bin/symforge hook",
            "Single PostToolUse hook command must have no subcommand suffix"
        );

        let session = &result["hooks"]["SessionStart"][0];
        let session_cmd = session["hooks"][0]["command"].as_str().unwrap();
        assert_eq!(session_cmd, "/usr/local/bin/symforge hook session-start");

        let prompt = &result["hooks"]["UserPromptSubmit"][0];
        let prompt_cmd = prompt["hooks"][0]["command"].as_str().unwrap();
        assert_eq!(prompt_cmd, "/usr/local/bin/symforge hook prompt-submit");
    }

    #[test]
    fn test_init_new_entry_matcher_includes_write() {
        let result = run_merge(json!({}));
        let matcher = result["hooks"]["PostToolUse"][0]["matcher"]
            .as_str()
            .unwrap();
        assert_eq!(
            matcher, "Read|Edit|Write|Grep",
            "matcher must include Write"
        );
    }

    // --- test_init_preserves_existing_hooks ---

    #[test]
    fn test_init_preserves_existing_hooks() {
        let initial = json!({
            "hooks": {
                "PostToolUse": [
                    {
                        "matcher": "Bash",
                        "hooks": [{"type": "command", "command": "/some/other/hook bash", "timeout": 10}]
                    }
                ]
            }
        });

        let result = run_merge(initial);
        let post = result["hooks"]["PostToolUse"]
            .as_array()
            .expect("PostToolUse must be an array");

        // 1 existing + 1 symforge = 2 total.
        assert_eq!(
            post.len(),
            2,
            "existing hook + 1 symforge hook = 2 entries; got {post:?}"
        );

        // The first entry is the preserved non-symforge hook.
        let first_cmd = post[0]["hooks"][0]["command"].as_str().unwrap();
        assert_eq!(
            first_cmd, "/some/other/hook bash",
            "non-symforge hook must be preserved"
        );
    }

    // --- test_init_migrates_old_three_entry_format ---

    #[test]
    fn test_init_migrates_old_three_entry_format() {
        let old_binary = "/usr/local/bin/symforge";
        let initial = json!({
            "hooks": {
                "PostToolUse": [
                    {
                        "matcher": "Read",
                        "hooks": [{"type": "command", "command": format!("{old_binary} hook read"), "timeout": 5}]
                    },
                    {
                        "matcher": "Edit|Write",
                        "hooks": [{"type": "command", "command": format!("{old_binary} hook edit"), "timeout": 5}]
                    },
                    {
                        "matcher": "Grep",
                        "hooks": [{"type": "command", "command": format!("{old_binary} hook grep"), "timeout": 5}]
                    }
                ]
            }
        });

        let result = run_merge(initial);
        let post = result["hooks"]["PostToolUse"].as_array().unwrap();

        // All 3 old entries must be replaced by exactly 1 new entry.
        assert_eq!(
            post.len(),
            1,
            "migration must replace 3 old entries with 1 new entry; got {post:?}"
        );

        let cmd = post[0]["hooks"][0]["command"].as_str().unwrap();
        assert_eq!(
            cmd, "/usr/local/bin/symforge hook",
            "migrated entry must use new no-subcommand command format"
        );

        let matcher = post[0]["matcher"].as_str().unwrap();
        assert_eq!(
            matcher, "Read|Edit|Write|Grep",
            "migrated entry must use full matcher"
        );
    }

    // --- test_init_idempotent ---

    #[test]
    fn test_init_idempotent() {
        let mut settings = json!({});
        merge_symforge_hooks(&mut settings, FAKE_BINARY);
        let after_first = settings.clone();

        merge_symforge_hooks(&mut settings, FAKE_BINARY);
        let after_second = settings.clone();

        assert_eq!(
            after_first, after_second,
            "running merge twice must produce identical output (idempotent)"
        );
    }

    #[test]
    fn test_init_idempotent_entry_count() {
        let mut settings = json!({});
        merge_symforge_hooks(&mut settings, FAKE_BINARY);
        let count_first = settings["hooks"]["PostToolUse"].as_array().unwrap().len();

        merge_symforge_hooks(&mut settings, FAKE_BINARY);
        let count_second = settings["hooks"]["PostToolUse"].as_array().unwrap().len();

        assert_eq!(
            count_first, count_second,
            "second merge must not add duplicate symforge entries"
        );
    }

    // --- test_init_replaces_stale_symforge_entries ---

    #[test]
    fn test_init_replaces_stale_symforge_entries() {
        let old_binary = "/old/path/to/symforge";
        let new_binary = "/new/path/to/symforge";

        // Set up settings with the old binary path.
        let initial = json!({
            "hooks": {
                "PostToolUse": [
                    {
                        "matcher": "Read",
                        "hooks": [{"type": "command", "command": format!("{old_binary} hook read"), "timeout": 5}]
                    }
                ]
            }
        });

        let mut settings = initial;
        merge_symforge_hooks(&mut settings, new_binary);

        let post = settings["hooks"]["PostToolUse"].as_array().unwrap();

        // Old entry must be gone.
        let has_old = post.iter().any(|e| {
            e["hooks"][0]["command"]
                .as_str()
                .map(|c| c.contains(old_binary))
                .unwrap_or(false)
        });
        assert!(
            !has_old,
            "stale symforge entry with old binary path must be removed"
        );

        // New entry must be present.
        let has_new = post.iter().any(|e| {
            e["hooks"][0]["command"]
                .as_str()
                .map(|c| c.contains(new_binary))
                .unwrap_or(false)
        });
        assert!(
            has_new,
            "new symforge entry with new binary path must be present"
        );
    }

    // --- is_symforge_entry ---

    #[test]
    fn test_is_symforge_entry_detects_symforge_command() {
        let entry = json!({
            "matcher": "Read",
            "hooks": [{"type": "command", "command": "/path/symforge hook read"}]
        });
        assert!(is_symforge_entry(&entry));
    }

    #[test]
    fn test_is_symforge_entry_ignores_non_symforge() {
        let entry = json!({
            "matcher": "Bash",
            "hooks": [{"type": "command", "command": "/some/other/script bash"}]
        });
        assert!(!is_symforge_entry(&entry));
    }

    #[test]
    fn test_merge_adds_allowed_tools() {
        let mut settings = json!({});
        merge_symforge_hooks(&mut settings, "/usr/bin/symforge");
        let allowed = settings["allowedTools"]
            .as_array()
            .expect("allowedTools should be array");
        assert!(
            allowed
                .iter()
                .any(|v| v.as_str() == Some("mcp__symforge__search_symbols")),
            "should include search_symbols, got: {allowed:?}"
        );
        assert!(
            allowed
                .iter()
                .any(|v| v.as_str() == Some("mcp__symforge__get_file_content")),
            "should include get_file_content"
        );
        assert!(
            allowed
                .iter()
                .any(|v| v.as_str() == Some("mcp__symforge__get_symbol")),
            "should include get_symbol"
        );
        assert!(
            allowed
                .iter()
                .any(|v| v.as_str() == Some("mcp__symforge__get_file_context")),
            "should include get_file_context"
        );
        assert!(
            allowed
                .iter()
                .any(|v| v.as_str() == Some("mcp__symforge__health_compact")),
            "should include health_compact"
        );
        assert!(
            !allowed
                .iter()
                .any(|v| v.as_str() == Some("mcp__symforge__trace_symbol")),
            "retired trace_symbol alias must not be granted by default: {allowed:?}"
        );
        let first_len = allowed.len();
        // Should not duplicate on re-run
        merge_symforge_hooks(&mut settings, "/usr/bin/symforge");
        let allowed2 = settings["allowedTools"].as_array().unwrap();
        assert_eq!(first_len, allowed2.len(), "should not duplicate entries");
    }

    #[test]
    fn test_default_client_allow_lists_exclude_retired_trace_symbol() {
        assert!(
            !SYMFORGE_TOOL_NAMES.contains(&"mcp__symforge__trace_symbol"),
            "Codex/client allow list should not grant retired trace_symbol alias"
        );
        assert!(
            !CLAUDE_ALWAYS_ALLOW.contains(&"trace_symbol"),
            "Claude allow list should not grant retired trace_symbol alias"
        );
    }

    #[test]
    fn test_default_client_allow_lists_include_health_compact() {
        assert!(
            SYMFORGE_TOOL_NAMES.contains(&"mcp__symforge__health_compact"),
            "Codex/client allow list should grant health_compact when conformance exposes it"
        );
        assert!(
            CLAUDE_ALWAYS_ALLOW.contains(&"health_compact"),
            "Claude allow list should grant health_compact when conformance exposes it"
        );
    }

    #[test]
    fn test_client_allow_lists_match_registered_tool_surface() {
        use std::collections::BTreeSet;

        // The registered MCP tool surface is the single source of truth.
        let registered: BTreeSet<String> = crate::protocol::SymForgeServer::tool_definitions()
            .iter()
            .map(|t| t.name.as_ref().to_string())
            .collect();

        // SYMFORGE_TOOL_NAMES carries the `mcp__symforge__` prefix; strip it to compare.
        // It backs the Claude Code settings.json `allowedTools` union (the full
        // surface). Kilo/Codex allowlists union CLAUDE_ALWAYS_ALLOW (pinned to
        // the registered surface below), so they are covered transitively.
        let full: BTreeSet<String> = SYMFORGE_TOOL_NAMES
            .iter()
            .map(|n| n.trim_start_matches("mcp__symforge__").to_string())
            .collect();
        assert_eq!(
            full, registered,
            "SYMFORGE_TOOL_NAMES (Claude settings.json allowedTools union) must match the registered \
             MCP tool surface exactly — a registered tool missing here means clients prompt for \
             permission on every call; a stale entry grants a retired tool. Update the allow list \
             when the tool surface changes."
        );

        let claude: BTreeSet<String> = CLAUDE_ALWAYS_ALLOW.iter().map(|n| n.to_string()).collect();
        assert_eq!(
            claude, registered,
            "CLAUDE_ALWAYS_ALLOW must match the registered MCP tool surface exactly"
        );
    }

    /// SF-010 drift guard: every tool referenced by the `ask` ToolHelp catalog
    /// (`tool_catalog_groups`) must be a real, registered tool. We compare against
    /// the `SYMFORGE_TOOL_NAMES` allow list (which the test above pins to the
    /// registered surface), so the catalog can never advertise a tool that does
    /// not exist.
    #[test]
    fn test_tool_catalog_names_exist_in_tool_surface() {
        use std::collections::BTreeSet;

        let known: BTreeSet<&str> = SYMFORGE_TOOL_NAMES
            .iter()
            .map(|n| n.trim_start_matches("mcp__symforge__"))
            .collect();

        for group in crate::protocol::smart_query::tool_catalog_groups() {
            for tool in group.tools {
                assert!(
                    known.contains(tool),
                    "tool_catalog_groups() advertises `{tool}` (group `{}`) which is not in \
                     SYMFORGE_TOOL_NAMES — the catalog drifted from the real tool surface",
                    group.key
                );
            }
        }
    }

    #[test]
    fn test_codex_registration_includes_allow_list() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.toml");
        register_codex_mcp_server(&config_path, "/usr/bin/symforge").unwrap();
        let content = std::fs::read_to_string(&config_path).unwrap();
        // G-036 coherence: Codex serves the full surface, so its allowlist
        // grants every full-surface short name.
        for name in [
            "\"symforge\"",
            "\"symforge_edit\"",
            "\"status\"",
            "\"search_symbols\"",
        ] {
            assert!(
                content.contains(name),
                "Codex allow list must grant full-surface name {name}: {content}"
            );
        }
        assert!(
            !content.contains("trace_symbol"),
            "Codex allow list must not include retired trace_symbol alias: {content}"
        );
        assert!(
            content.contains("SYMFORGE_SURFACE = \"full\""),
            "Codex env must make the full surface explicit: {content}"
        );
        assert!(
            content.contains("project_doc_fallback_filenames = [\"AGENTS.md\", \"CLAUDE.md\"]"),
            "should register both AGENTS.md and CLAUDE.md as project doc fallbacks: {content}"
        );
    }

    // -- G-036 init/update coherence regression tests -----------------------

    #[test]
    fn test_claude_code_fresh_registration_is_coherent_full_surface() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".claude.json");
        register_mcp_server(&path, "/usr/bin/symforge").unwrap();
        let config: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let entry = &config["mcpServers"]["symforge"];
        assert_eq!(
            entry["env"]["SYMFORGE_SURFACE"].as_str(),
            Some("full"),
            "fresh Claude Code registration must pin the full surface: {entry}"
        );
        let allow = entry["alwaysAllow"].as_array().unwrap();
        assert_eq!(
            allow.len(),
            CLAUDE_ALWAYS_ALLOW.len(),
            "the allowlist must match the full surface it serves: {entry}"
        );
    }

    #[test]
    fn test_reregistration_preserves_user_set_surface_both_directions() {
        // Direction 1: user pinned compact on Claude Code (fresh default full).
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".claude.json");
        std::fs::write(
            &path,
            serde_json::to_string_pretty(&json!({
                "mcpServers": {
                    "symforge": {
                        "command": "/old/symforge",
                        "env": {"SYMFORGE_SURFACE": "compact"}
                    }
                }
            }))
            .unwrap(),
        )
        .unwrap();
        register_mcp_server(&path, "/new/symforge").unwrap();
        let config: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(
            config["mcpServers"]["symforge"]["env"]["SYMFORGE_SURFACE"].as_str(),
            Some("compact"),
            "re-registration must NOT downgrade a user-set surface (the 8.10.1 wipe)"
        );

        // Direction 2: user pinned compact (escape hatch) on Kilo (fresh default full).
        let dir2 = tempfile::tempdir().unwrap();
        let path2 = dir2.path().join("mcp.json");
        std::fs::write(
            &path2,
            serde_json::to_string_pretty(&json!({
                "mcpServers": {
                    "symforge": {
                        "command": "/old/symforge",
                        "env": {"SYMFORGE_SURFACE": "compact"}
                    }
                }
            }))
            .unwrap(),
        )
        .unwrap();
        register_kilo_mcp_server(&path2, "/new/symforge").unwrap();
        let config2: Value =
            serde_json::from_str(&std::fs::read_to_string(&path2).unwrap()).unwrap();
        assert_eq!(
            config2["mcpServers"]["symforge"]["env"]["SYMFORGE_SURFACE"].as_str(),
            Some("compact"),
            "re-registration must preserve a user-set compact escape hatch on Kilo"
        );
    }

    #[test]
    fn test_reregistration_preserves_unknown_fields_and_updates_command() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".claude.json");
        std::fs::write(
            &path,
            serde_json::to_string_pretty(&json!({
                "mcpServers": {
                    "symforge": {
                        "command": "/old/symforge",
                        "userField": "keep-me",
                        "env": {"MY_CUSTOM_ENV": "keep-me-too"},
                        "alwaysAllow": ["my_custom_tool"]
                    }
                }
            }))
            .unwrap(),
        )
        .unwrap();
        register_mcp_server(&path, "/new/symforge").unwrap();
        let config: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let entry = &config["mcpServers"]["symforge"];
        assert_eq!(
            entry["userField"].as_str(),
            Some("keep-me"),
            "unknown user field must survive re-registration: {entry}"
        );
        assert_eq!(
            entry["env"]["MY_CUSTOM_ENV"].as_str(),
            Some("keep-me-too"),
            "unknown user env key must survive re-registration: {entry}"
        );
        let allow: Vec<&str> = entry["alwaysAllow"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .collect();
        assert!(
            allow.contains(&"my_custom_tool"),
            "user-added allowlist entries must never be dropped (union): {entry}"
        );
        let expected_command = if cfg!(windows) {
            "\\new\\symforge"
        } else {
            "/new/symforge"
        };
        assert_eq!(
            entry["command"].as_str(),
            Some(expected_command),
            "re-registration must refresh the launcher command path: {entry}"
        );
    }

    #[test]
    fn test_cursor_reregistration_preserves_existing_workspace_root() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mcp.json");
        std::fs::write(
            &path,
            serde_json::to_string_pretty(&json!({
                "mcpServers": {
                    "symforge": {
                        "command": "/old/symforge",
                        "env": {
                            "SYMFORGE_SURFACE": "compact",
                            "SYMFORGE_WORKSPACE_ROOT": "/user/pinned/workspace"
                        },
                        "cwd": "/user/pinned/workspace"
                    }
                }
            }))
            .unwrap(),
        )
        .unwrap();
        register_cursor_mcp_server(&path, "/new/symforge").unwrap();
        let config: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let entry = &config["mcpServers"]["symforge"];
        assert_eq!(
            entry["env"][crate::discovery::WORKSPACE_ROOT_ENV].as_str(),
            Some("/user/pinned/workspace"),
            "existing workspace root must NOT be overwritten by rediscovery: {entry}"
        );
        assert_eq!(
            entry["cwd"].as_str(),
            Some("/user/pinned/workspace"),
            "existing cwd must be preserved: {entry}"
        );
    }

    #[test]
    fn test_gemini_fresh_registration_pins_full_surface() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        register_gemini_mcp_server(&path, "/usr/bin/symforge").unwrap();
        let config: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(
            config["mcpServers"]["symforge"]["env"]["SYMFORGE_SURFACE"].as_str(),
            Some("full"),
            "fresh Gemini registration must make the full surface explicit"
        );
    }

    #[test]
    fn test_kilo_fresh_registration_allowlist_is_exactly_full() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mcp.json");
        register_kilo_mcp_server(&path, "/usr/bin/symforge").unwrap();
        let config: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let allow: Vec<&str> = config["mcpServers"]["symforge"]["alwaysAllow"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .collect();
        assert_eq!(allow, CLAUDE_ALWAYS_ALLOW.to_vec());
        assert_eq!(
            config["mcpServers"]["symforge"]["env"]["SYMFORGE_SURFACE"].as_str(),
            Some("full"),
            "fresh Kilo registration must make the full surface explicit"
        );
    }

    #[test]
    fn test_kilo_reregistration_unions_existing_allowlist() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mcp.json");
        std::fs::write(
            &path,
            serde_json::to_string_pretty(&json!({
                "mcpServers": {
                    "symforge": {
                        "command": "/old/symforge",
                        "alwaysAllow": ["my_custom_tool"]
                    }
                }
            }))
            .unwrap(),
        )
        .unwrap();
        register_kilo_mcp_server(&path, "/new/symforge").unwrap();
        let config: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let allow: Vec<&str> = config["mcpServers"]["symforge"]["alwaysAllow"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .collect();
        assert!(
            allow.contains(&"my_custom_tool"),
            "user entry dropped: {allow:?}"
        );
        for name in ["symforge", "symforge_edit", "status"] {
            assert!(
                allow.contains(&name),
                "missing compact name {name}: {allow:?}"
            );
        }
    }

    #[test]
    fn test_codex_fresh_registration_allowlist_is_exactly_full() {
        let mut config = DocumentMut::new();
        merge_symforge_codex_server_for_target_os(&mut config, "/usr/bin/symforge", "macos");
        let content = config.to_string();
        for name in CLAUDE_ALWAYS_ALLOW {
            assert!(
                content.contains(&format!("\"{name}\"")),
                "fresh Codex allowlist must grant full-surface name {name}: {content}"
            );
        }
        assert!(
            content.contains("SYMFORGE_SURFACE = \"full\""),
            "fresh Codex env must pin the full surface on every OS: {content}"
        );
    }

    #[test]
    fn test_codex_reregistration_unions_allowlist_and_preserves_surface() {
        let mut config = r#"
    [mcp_servers.symforge]
    command = "/old/symforge"
    startup_timeout_sec = 90
    tool_timeout_sec = 900
    allowed_tools = ["my_custom_tool"]

    [mcp_servers.symforge.env]
    SYMFORGE_SURFACE = "compact"
    "#
        .parse::<DocumentMut>()
        .unwrap();
        merge_symforge_codex_server_for_target_os(&mut config, "/new/symforge", "macos");
        let content = config.to_string();
        assert!(
            content.contains("my_custom_tool"),
            "user-added Codex allow entry must never be dropped: {content}"
        );
        for name in ["\"symforge\"", "\"symforge_edit\"", "\"status\""] {
            assert!(
                content.contains(name),
                "surface name {name} must be unioned in: {content}"
            );
        }
        assert!(
            content.contains("SYMFORGE_SURFACE = \"compact\""),
            "re-registration must preserve a user-set Codex compact escape hatch: {content}"
        );
        assert!(
            content.contains("startup_timeout_sec = 90")
                && content.contains("tool_timeout_sec = 900"),
            "re-registration must preserve user-tuned Codex timeouts: {content}"
        );
        assert!(
            !content.contains("/old/symforge") && content.contains("symforge"),
            "re-registration must still refresh the binary path: {content}"
        );
    }

    #[test]
    fn test_codex_linux_registration_disables_daemon() {
        let mut config = DocumentMut::new();
        merge_symforge_codex_server_for_target_os(&mut config, "/usr/bin/symforge", "linux");
        let content = config.to_string();

        assert!(
            content.contains("[mcp_servers.symforge.env]"),
            "linux Codex config should include an env table: {content}"
        );
        assert!(
            content.contains("SYMFORGE_NO_DAEMON = \"1\""),
            "linux Codex config should force reliable local stdio mode: {content}"
        );
        assert!(
            content.contains("allowed_tools ="),
            "existing Codex allow-list behavior should remain intact: {content}"
        );
    }

    #[test]
    fn test_codex_linux_registration_preserves_existing_env() {
        let mut config = r#"
[mcp_servers.symforge]
command = "/old/symforge"

[mcp_servers.symforge.env]
EXISTING_FLAG = "keep"
"#
        .parse::<DocumentMut>()
        .unwrap();

        merge_symforge_codex_server_for_target_os(&mut config, "/usr/bin/symforge", "linux");
        let content = config.to_string();

        assert!(
            content.contains("EXISTING_FLAG = \"keep\""),
            "should preserve user-managed env entries: {content}"
        );
        assert!(
            content.contains("SYMFORGE_NO_DAEMON = \"1\""),
            "should add the Linux daemon bypass alongside existing env entries: {content}"
        );
    }

    #[test]
    fn test_codex_linux_registration_preserves_inline_env() {
        let mut config = r#"
[mcp_servers.symforge]
command = "/old/symforge"
env = { EXISTING_FLAG = "keep" }
"#
        .parse::<DocumentMut>()
        .unwrap();

        merge_symforge_codex_server_for_target_os(&mut config, "/usr/bin/symforge", "linux");
        let content = config.to_string();

        assert!(
            content.contains("EXISTING_FLAG = \"keep\""),
            "should preserve user-managed inline env entries: {content}"
        );
        assert!(
            content.contains("SYMFORGE_NO_DAEMON = \"1\""),
            "should add the Linux daemon bypass alongside inline env entries: {content}"
        );
    }

    #[test]
    fn test_codex_non_linux_registration_does_not_add_daemon_override() {
        let mut config = DocumentMut::new();
        merge_symforge_codex_server_for_target_os(&mut config, "/usr/bin/symforge", "windows");
        let content = config.to_string();

        assert!(
            !content.contains("SYMFORGE_NO_DAEMON"),
            "non-Linux Codex config should keep daemon behavior unchanged: {content}"
        );
    }

    #[test]
    fn test_codex_guidance_is_task_map_block() {
        let block = codex_guidance_block();
        assert!(
            block.contains("| Task | Call |"),
            "codex guidance should carry the task-to-tool map: {block}"
        );
        assert!(
            block.contains("Raw reads (`get_file_content`/Read) remain correct"),
            "codex guidance should keep the raw-read fallback rule: {block}"
        );
        assert!(
            !block.contains("## Agent Directives: Mechanical Overrides")
                && !block.contains("## Tooling Preference"),
            "removed sections must not ship again: {block}"
        );
    }

    #[test]
    fn test_guidance_block_markers_are_bare_lines() {
        let block = shared_guidance_block();
        assert!(
            block.starts_with("<!-- SYMFORGE START -->\n"),
            "start marker must be alone on the first line: {block}"
        );
        assert!(
            block.ends_with("\n<!-- SYMFORGE END -->"),
            "end marker must be alone on the last line: {block}"
        );
        assert!(
            !block.contains("____"),
            "no stray text may fuse onto the markers: {block}"
        );
    }

    #[test]
    fn test_upsert_markdown_block_is_idempotent() {
        let block = shared_guidance_block();
        let existing = format!("# Host file\n\nHost sections stay untouched.\n\n{block}\n");
        assert_eq!(
            upsert_markdown_block(&existing, &block),
            existing,
            "re-running the upsert on an up-to-date file must be a no-op"
        );
    }

    #[test]
    fn test_desktop_registration_rejects_temporary_binary_even_with_legacy_home_binary() {
        let home = tempfile::tempdir().unwrap();
        let home_bin_dir = home.path().join(".symforge").join("bin");
        std::fs::create_dir_all(&home_bin_dir).unwrap();
        let legacy_home_binary = home_bin_dir.join(if cfg!(windows) {
            "symforge.exe"
        } else {
            "symforge"
        });
        std::fs::write(&legacy_home_binary, "stable").unwrap();

        let temp = tempfile::tempdir().unwrap();
        let temp_binary = temp.path().join(if cfg!(windows) {
            "symforge.exe"
        } else {
            "symforge"
        });
        std::fs::write(&temp_binary, "transient").unwrap();

        let error = desktop_binary_for_registration(&temp_binary, home.path()).unwrap_err();
        assert!(
            error.to_string().contains("npm install -g symforge"),
            "temporary binary error should point to the passive global npm install: {error}"
        );
    }

    #[test]
    fn test_desktop_registration_rejects_temporary_binary_without_home_install() {
        let home = tempfile::tempdir().unwrap();
        let temp = tempfile::tempdir().unwrap();
        let temp_binary = temp.path().join(if cfg!(windows) {
            "symforge.exe"
        } else {
            "symforge"
        });
        std::fs::write(&temp_binary, "transient").unwrap();

        let error = desktop_binary_for_registration(&temp_binary, home.path()).unwrap_err();
        assert!(
            error.to_string().contains("temporary"),
            "error should explain that desktop config cannot point at temp paths: {error}"
        );
    }

    #[test]
    fn test_codex_registration_rejects_temporary_binary() {
        let home = tempfile::tempdir().unwrap();
        let working_dir = tempfile::tempdir().unwrap();
        let temp = tempfile::tempdir().unwrap();
        let temp_binary = temp.path().join(if cfg!(windows) {
            "symforge.exe"
        } else {
            "symforge"
        });
        std::fs::write(&temp_binary, "transient").unwrap();

        let error = run_init_with_context(
            InitClient::Codex,
            home.path(),
            working_dir.path(),
            &temp_binary,
        )
        .unwrap_err();

        assert!(
            error.to_string().contains("temporary"),
            "Codex init should reject temporary binaries: {error}"
        );
    }

    #[cfg(windows)]
    #[test]
    fn test_claude_desktop_registration_writes_wrapper_into_config_dir() {
        let home = tempfile::tempdir().unwrap();
        let stable_bin_dir = std::env::current_dir()
            .unwrap()
            .join("target")
            .join(format!("symforge-init-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&stable_bin_dir);
        std::fs::create_dir_all(&stable_bin_dir).unwrap();
        let home_binary = stable_bin_dir.join("symforge.exe");
        std::fs::write(&home_binary, "stable").unwrap();

        let config_dir = tempfile::tempdir().unwrap();
        let config_path = config_dir.path().join("claude_desktop_config.json");
        register_claude_desktop_mcp_server_with_home(
            &config_path,
            &home_binary.display().to_string(),
            home.path(),
        )
        .unwrap();

        let content = std::fs::read_to_string(&config_path).unwrap();
        let config: Value = serde_json::from_str(&content).unwrap();
        let command = config["mcpServers"]["symforge"]["command"]
            .as_str()
            .unwrap();

        // The wrapper lives in the DESKTOP CONFIG DIR (survives npm package
        // swaps), never next to the npm binary (npm wipes that dir on every
        // update, which orphaned the registered command).
        let wrapper = config_dir.path().join("symforge-desktop.cmd");
        assert_eq!(
            normalize_exe_path_for_test(command),
            normalize_exe_path_for_test(&wrapper.display().to_string()),
            "Claude Desktop command must point at the wrapper in the config dir"
        );
        assert!(
            !stable_bin_dir.join("symforge-desktop.cmd").exists(),
            "no wrapper may be written next to the binary (npm wipes that dir)"
        );
        let script = std::fs::read_to_string(&wrapper).unwrap();
        assert!(
            script.contains(&home_binary.display().to_string()),
            "wrapper must launch the binary by absolute path: {script}"
        );
        let _ = std::fs::remove_dir_all(&stable_bin_dir);
    }

    #[cfg(windows)]
    fn normalize_exe_path_for_test(path: &str) -> String {
        path.replace('/', "\\").to_ascii_lowercase()
    }

    // Plan 001 (home-cwd disease): `symforge init --client cursor` registers
    // symforge in Cursor's global mcp.json with a proven env pinning the full
    // surface (spike-gate default); and when a workspace is discoverable it threads both
    // SYMFORGE_WORKSPACE_ROOT and a per-server `cwd`, so Cursor never launches
    // into the empty-index home-cwd trap. Only the `symforge` server is touched.
    #[test]
    fn test_cursor_registration_writes_proven_env_and_cwd() {
        let config_dir = tempfile::tempdir().unwrap();
        let config_path = config_dir.path().join("mcp.json");
        let binary = if cfg!(windows) {
            "C:\\bin\\symforge.exe"
        } else {
            "/usr/bin/symforge"
        };
        register_cursor_mcp_server(&config_path, binary).unwrap();

        let config: Value =
            serde_json::from_str(&std::fs::read_to_string(&config_path).unwrap()).unwrap();
        let server = &config["mcpServers"]["symforge"];
        assert!(
            server["command"].as_str().is_some_and(|c| !c.is_empty()),
            "cursor server must carry a command: {server}"
        );
        assert!(server["args"].is_array(), "cursor server must carry args");
        let env = server["env"]
            .as_object()
            .expect("cursor env must be a proven object, not absent");
        assert_eq!(
            env.get("SYMFORGE_SURFACE").and_then(Value::as_str),
            Some("full"),
            "cursor env must pin the full surface: {server}"
        );
        // The init test process runs inside the symforge git repo, so
        // find_project_root resolves a workspace; when it does, the per-server
        // `cwd` must equal the SYMFORGE_WORKSPACE_ROOT env (the home-cwd fix —
        // both point at the discovered workspace).
        if let Some(root) = env
            .get(crate::discovery::WORKSPACE_ROOT_ENV)
            .and_then(Value::as_str)
        {
            assert_eq!(
                server["cwd"].as_str(),
                Some(root),
                "cursor cwd must match the workspace-root env when discovered: {server}"
            );
        }
    }

    // T029 (TR-03 / FR-013): the generated wrapper `cd`s into the discovered
    // workspace (writable + indexable), NOT the home dir, so cold-start
    // `find_project_root` resolves the project and the index is populated.
    #[cfg(windows)]
    #[test]
    fn test_desktop_wrapper_cds_to_discovered_workspace_not_home() {
        let bin_dir = tempfile::tempdir().unwrap();
        let binary = bin_dir.path().join("symforge.exe");
        std::fs::write(&binary, "x").unwrap();
        let workspace = tempfile::tempdir().unwrap();
        let workspace_str = workspace.path().display().to_string();

        let config_dir = tempfile::tempdir().unwrap();
        let wrapper_path = create_desktop_wrapper_windows(
            &binary.display().to_string(),
            Some(workspace_str.as_str()),
            config_dir.path(),
        )
        .unwrap();
        let script = std::fs::read_to_string(&wrapper_path).unwrap();

        assert!(
            script.contains(&format!("cd /d \"{workspace_str}\"")),
            "wrapper must cd into the discovered workspace: {script}"
        );
        assert!(
            !script.contains("%USERPROFILE%"),
            "wrapper must NOT cd to %USERPROFILE% when a workspace was discovered \
             (the TR-03 empty-index trap): {script}"
        );
    }

    // T029: with NO discoverable workspace, the wrapper falls back to the
    // always-writable home dir — preserving the original System32-crash fix and
    // not regressing the no-workspace case.
    #[cfg(windows)]
    #[test]
    fn test_desktop_wrapper_falls_back_to_home_without_workspace() {
        let bin_dir = tempfile::tempdir().unwrap();
        let binary = bin_dir.path().join("symforge.exe");
        std::fs::write(&binary, "x").unwrap();

        let config_dir = tempfile::tempdir().unwrap();
        let wrapper_path =
            create_desktop_wrapper_windows(&binary.display().to_string(), None, config_dir.path())
                .unwrap();
        let script = std::fs::read_to_string(&wrapper_path).unwrap();

        assert!(
            script.contains("cd /d \"%USERPROFILE%\""),
            "wrapper must fall back to %USERPROFILE% (writable) without a workspace: {script}"
        );
    }

    // T029: init writes a proven `env` (not `{}`) — the surface is made explicit
    // and the discovered workspace is threaded through so cold start indexes it.
    #[test]
    fn test_desktop_registration_writes_proven_env_with_surface_and_workspace() {
        let home = tempfile::tempdir().unwrap();
        let stable_bin_dir = std::env::current_dir()
            .unwrap()
            .join("target")
            .join(format!("symforge-env-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&stable_bin_dir);
        std::fs::create_dir_all(&stable_bin_dir).unwrap();
        let home_binary = stable_bin_dir.join(if cfg!(windows) {
            "symforge.exe"
        } else {
            "symforge"
        });
        std::fs::write(&home_binary, "stable").unwrap();

        let config_dir = tempfile::tempdir().unwrap();
        let config_path = config_dir.path().join("claude_desktop_config.json");
        register_claude_desktop_mcp_server_with_home(
            &config_path,
            &home_binary.display().to_string(),
            home.path(),
        )
        .unwrap();

        let config: Value =
            serde_json::from_str(&std::fs::read_to_string(&config_path).unwrap()).unwrap();
        let env = config["mcpServers"]["symforge"]["env"]
            .as_object()
            .expect("env must be a proven object, not empty `{}`");

        assert_eq!(
            env.get("SYMFORGE_SURFACE").and_then(Value::as_str),
            Some("compact"),
            "registered env must pin the compact surface explicitly: {env:?}"
        );

        // The test binary runs from the symforge repo (a git repo), so init
        // discovers a real workspace root and threads it through.
        let root = env
            .get(crate::discovery::WORKSPACE_ROOT_ENV)
            .and_then(Value::as_str)
            .expect("registered env must carry the discovered workspace root");
        assert!(
            std::path::Path::new(root).is_dir(),
            "the threaded workspace root must be an existing directory: {root}"
        );

        let _ = std::fs::remove_dir_all(&stable_bin_dir);
    }

    #[test]
    fn test_gemini_guidance_is_task_map_block() {
        let block = gemini_guidance_block();
        assert!(
            block.contains("| Task | Call |"),
            "gemini guidance should carry the task-to-tool map: {block}"
        );
        assert!(
            block.contains("validate_file_syntax"),
            "gemini guidance should mention config validation inside SymForge: {block}"
        );
    }

    #[test]
    fn test_kilo_guidance_is_task_map_block() {
        let block = kilo_guidance_block();
        assert!(
            block.contains("| Task | Call |"),
            "kilo guidance should carry the task-to-tool map: {block}"
        );
        assert!(
            block.contains("`edit_plan`, then `replace_symbol_body`"),
            "kilo guidance should route symbol edits through edit_plan first: {block}"
        );
    }

    #[test]
    fn test_gemini_registration_creates_config() {
        let dir = tempfile::tempdir().unwrap();
        let settings_path = dir.path().join("settings.json");
        register_gemini_mcp_server(&settings_path, "/usr/bin/symforge").unwrap();
        let content = std::fs::read_to_string(&settings_path).unwrap();
        let config: Value = serde_json::from_str(&content).unwrap();
        assert!(config["mcpServers"]["symforge"]["command"].is_string());
    }

    #[test]
    fn test_gemini_registration_includes_trust() {
        let dir = tempfile::tempdir().unwrap();
        let settings_path = dir.path().join("settings.json");
        register_gemini_mcp_server(&settings_path, "/usr/bin/symforge").unwrap();
        let content = std::fs::read_to_string(&settings_path).unwrap();
        let config: Value = serde_json::from_str(&content).unwrap();
        assert_eq!(
            config["mcpServers"]["symforge"]["trust"],
            json!(true),
            "symforge server must have trust: true"
        );
    }

    #[test]
    fn test_gemini_registration_timeout_in_milliseconds() {
        let dir = tempfile::tempdir().unwrap();
        let settings_path = dir.path().join("settings.json");
        register_gemini_mcp_server(&settings_path, "/usr/bin/symforge").unwrap();
        let content = std::fs::read_to_string(&settings_path).unwrap();
        let config: Value = serde_json::from_str(&content).unwrap();
        assert_eq!(
            config["mcpServers"]["symforge"]["timeout"],
            json!(120000),
            "timeout must be in milliseconds (120000ms = 2 minutes)"
        );
    }

    #[test]
    fn test_gemini_registration_no_allowed_tools_key() {
        let dir = tempfile::tempdir().unwrap();
        let settings_path = dir.path().join("settings.json");
        register_gemini_mcp_server(&settings_path, "/usr/bin/symforge").unwrap();
        let content = std::fs::read_to_string(&settings_path).unwrap();
        let config: Value = serde_json::from_str(&content).unwrap();
        assert!(
            config.get("allowedTools").is_none(),
            "Gemini config must not include allowedTools (Claude-only concept)"
        );
    }

    #[test]
    fn test_gemini_registration_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let settings_path = dir.path().join("settings.json");
        register_gemini_mcp_server(&settings_path, "/usr/bin/symforge").unwrap();
        let first = std::fs::read_to_string(&settings_path).unwrap();
        register_gemini_mcp_server(&settings_path, "/usr/bin/symforge").unwrap();
        let second = std::fs::read_to_string(&settings_path).unwrap();
        assert_eq!(
            first, second,
            "running registration twice must produce identical output"
        );
    }

    #[test]
    fn test_gemini_folder_trust_enabled_defaults_false_when_missing() {
        let dir = tempfile::tempdir().unwrap();
        let settings_path = dir.path().join("settings.json");
        assert!(
            !gemini_folder_trust_enabled(&settings_path).unwrap(),
            "missing settings should not claim Gemini folder trust is enabled"
        );
    }

    #[test]
    fn test_gemini_folder_trust_enabled_reads_true_flag() {
        let dir = tempfile::tempdir().unwrap();
        let settings_path = dir.path().join("settings.json");
        std::fs::write(
            &settings_path,
            serde_json::to_string_pretty(&json!({
                "security": { "folderTrust": { "enabled": true } }
            }))
            .unwrap(),
        )
        .unwrap();

        assert!(
            gemini_folder_trust_enabled(&settings_path).unwrap(),
            "explicit folder trust flag should be honored"
        );
    }

    #[test]
    fn test_gemini_workspace_is_trusted_for_exact_folder_rule() {
        let dir = tempfile::tempdir().unwrap();
        let working_dir = dir.path().join("workspace");
        std::fs::create_dir_all(&working_dir).unwrap();
        let trusted_folders_path = dir.path().join("trustedFolders.json");
        std::fs::write(
            &trusted_folders_path,
            serde_json::to_string_pretty(&json!({
                working_dir.to_string_lossy().to_string(): "TRUST_FOLDER"
            }))
            .unwrap(),
        )
        .unwrap();

        assert!(
            gemini_workspace_is_trusted(&trusted_folders_path, &working_dir).unwrap(),
            "exact TRUST_FOLDER rule should trust the current workspace"
        );
    }

    #[test]
    fn test_gemini_workspace_is_trusted_for_trust_parent_rule() {
        let dir = tempfile::tempdir().unwrap();
        let projects_dir = dir.path().join("projects");
        let trusted_rule_dir = projects_dir.join("seed-project");
        let working_dir = projects_dir.join("active-project");
        std::fs::create_dir_all(&trusted_rule_dir).unwrap();
        std::fs::create_dir_all(&working_dir).unwrap();
        let trusted_folders_path = dir.path().join("trustedFolders.json");
        std::fs::write(
            &trusted_folders_path,
            serde_json::to_string_pretty(&json!({
                trusted_rule_dir.to_string_lossy().to_string(): "TRUST_PARENT"
            }))
            .unwrap(),
        )
        .unwrap();

        assert!(
            gemini_workspace_is_trusted(&trusted_folders_path, &working_dir).unwrap(),
            "TRUST_PARENT should trust sibling workspaces under the same parent"
        );
    }

    #[test]
    fn test_gemini_workspace_is_trusted_prefers_more_specific_do_not_trust_rule() {
        let dir = tempfile::tempdir().unwrap();
        let projects_dir = dir.path().join("projects");
        let trusted_rule_dir = projects_dir.join("seed-project");
        let working_dir = projects_dir.join("active-project");
        std::fs::create_dir_all(&trusted_rule_dir).unwrap();
        std::fs::create_dir_all(&working_dir).unwrap();
        let trusted_folders_path = dir.path().join("trustedFolders.json");
        std::fs::write(
            &trusted_folders_path,
            serde_json::to_string_pretty(&json!({
                trusted_rule_dir.to_string_lossy().to_string(): "TRUST_PARENT",
                working_dir.to_string_lossy().to_string(): "DO_NOT_TRUST"
            }))
            .unwrap(),
        )
        .unwrap();

        assert!(
            !gemini_workspace_is_trusted(&trusted_folders_path, &working_dir).unwrap(),
            "a more specific DO_NOT_TRUST rule should override inherited parent trust"
        );
    }

    #[test]
    fn test_gemini_workspace_trust_warning_reports_untrusted_workspace() {
        let dir = tempfile::tempdir().unwrap();
        let working_dir = dir.path().join("workspace");
        std::fs::create_dir_all(&working_dir).unwrap();
        let settings_path = dir.path().join("settings.json");
        let trusted_folders_path = dir.path().join("trustedFolders.json");
        std::fs::write(
            &settings_path,
            serde_json::to_string_pretty(&json!({
                "security": { "folderTrust": { "enabled": true } }
            }))
            .unwrap(),
        )
        .unwrap();
        std::fs::write(
            &trusted_folders_path,
            serde_json::to_string_pretty(&json!({
                dir.path().join("elsewhere").to_string_lossy().to_string(): "TRUST_FOLDER"
            }))
            .unwrap(),
        )
        .unwrap();

        let warning =
            gemini_workspace_trust_warning(&settings_path, &trusted_folders_path, &working_dir)
                .unwrap()
                .expect("warning should be emitted for an untrusted workspace");
        assert!(
            warning.contains("Gemini folder trust is enabled"),
            "warning should explain why Gemini MCP will not connect: {warning}"
        );
        assert!(
            warning.contains(&working_dir.display().to_string()),
            "warning should point to the exact workspace path: {warning}"
        );
    }

    #[test]
    fn test_gemini_workspace_trust_warning_suppresses_trusted_workspace() {
        let dir = tempfile::tempdir().unwrap();
        let working_dir = dir.path().join("workspace");
        std::fs::create_dir_all(&working_dir).unwrap();
        let settings_path = dir.path().join("settings.json");
        let trusted_folders_path = dir.path().join("trustedFolders.json");
        std::fs::write(
            &settings_path,
            serde_json::to_string_pretty(&json!({
                "security": { "folderTrust": { "enabled": true } }
            }))
            .unwrap(),
        )
        .unwrap();
        std::fs::write(
            &trusted_folders_path,
            serde_json::to_string_pretty(&json!({
                working_dir.to_string_lossy().to_string(): "TRUST_FOLDER"
            }))
            .unwrap(),
        )
        .unwrap();

        assert!(
            gemini_workspace_trust_warning(&settings_path, &trusted_folders_path, &working_dir)
                .unwrap()
                .is_none(),
            "trusted workspaces should not produce a Gemini trust warning"
        );
    }
}
