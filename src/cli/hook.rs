//! Hook binary logic — reads the OS-tagged `.symforge/sidecar.<os>.port`, calls the
//! sidecar over sync HTTP, and outputs a single JSON line to stdout.
//!
//! Design constraints (HOOK-10):
//! - The ONLY thing written to stdout is the final JSON line.
//! - No tokio runtime. No tracing to stdout. No eprintln except for genuine errors.
//! - Sync I/O throughout — hooks must complete in well under 100 ms.
//! - Fail-open: if the sidecar is unreachable for any reason, output empty additionalContext
//!   JSON so Claude Code continues normally.

use std::io::{BufRead, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::cli::HookSubcommand;

// hook-adoption.log is written AND read only inside this hook binary (single OS per
// process), so it stays un-tagged. The sidecar port/session files are cross-process
// (written by the sidecar/proxy, read here) and MUST be OS-tagged in lockstep with the
// writer — both sides derive the tag from `crate::paths::os_tagged_runtime_file_name`,
// so a given OS's hook and sidecar always agree. See `sidecar_port_file_rel` below.
const ADOPTION_LOG_FILE: &str = ".symforge/hook-adoption.log";

/// Legacy (pre-OS-tag) cross-process paths, read-only fallback for one release.
const LEGACY_PORT_FILE: &str = ".symforge/sidecar.port";
const LEGACY_SESSION_FILE: &str = ".symforge/sidecar.session";

/// CWD-relative path to the OS-tagged sidecar port file, e.g. `.symforge/sidecar.windows.port`.
fn sidecar_port_file_rel() -> PathBuf {
    Path::new(crate::paths::SYMFORGE_DIR_NAME)
        .join(crate::paths::os_tagged_runtime_file_name("sidecar", "port"))
}

/// CWD-relative path to the OS-tagged sidecar session file.
fn sidecar_session_file_rel() -> PathBuf {
    Path::new(crate::paths::SYMFORGE_DIR_NAME).join(crate::paths::os_tagged_runtime_file_name(
        "sidecar", "session",
    ))
}

/// Read a CWD-relative runtime file, preferring the OS-tagged path then the legacy path.
fn read_runtime_rel(tagged: &Path, legacy: &str) -> std::io::Result<String> {
    match std::fs::read_to_string(tagged) {
        Ok(contents) => Ok(contents),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => std::fs::read_to_string(legacy),
        Err(e) => Err(e),
    }
}
/// Hard HTTP timeout — leaves margin within HOOK-03's 100 ms total budget.
const HTTP_TIMEOUT: Duration = Duration::from_millis(50);

/// Total deadline for the entire daemon fallback sequence
/// (port-file read + two HTTP round-trips + JSON parsing).
///
/// **Degraded-mode policy**: this intentionally exceeds HOOK-03's 100 ms
/// normal-path latency target.  The daemon fallback activates only when
/// the sidecar is unreachable and requires two sequential HTTP round-trips
/// that cannot reliably fit in 100 ms.  Accepting up to 500 ms of added
/// latency in this rare degraded scenario is preferable to returning
/// empty context when the daemon holds useful data.
///
/// Individual requests get whatever time remains within this budget.
const DAEMON_FALLBACK_DEADLINE: Duration = Duration::from_millis(500);

// ---------------------------------------------------------------------------
// Stdin JSON parsing structs
// ---------------------------------------------------------------------------

/// Deserialized representation of a Claude Code PostToolUse stdin payload.
///
/// The type is `pub` so integration tests can construct an empty payload via
/// `HookInput::default()` for [`run_hook_with_input`]; the fields stay
/// crate-private.
#[derive(serde::Deserialize, Default)]
pub struct HookInput {
    pub(crate) tool_name: Option<String>,
    pub(crate) tool_input: Option<HookToolInput>,
    pub(crate) cwd: Option<String>,
    pub(crate) prompt: Option<String>,
}

/// The `tool_input` field from the Claude Code hook event payload.
#[derive(serde::Deserialize, Default)]
pub(crate) struct HookToolInput {
    /// Absolute path to the file being read/edited/written.
    pub(crate) file_path: Option<String>,
    /// Search pattern for Grep events.
    pub(crate) pattern: Option<String>,
    /// Directory path for Grep events (alternative field name).
    pub(crate) path: Option<String>,
}

/// Workflow buckets used to reason about what SymForge should eventually own
/// at hook-decision time.
///
/// PR 1 only introduces the vocabulary and non-behavioral scaffolding so later
/// routing work can target stable concepts instead of raw client tool names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HookWorkflow {
    /// Repo-local source inspection such as reading a code file for orientation.
    SourceRead,
    /// Repo-local source search such as Grep over code intent.
    SourceSearch,
    /// First-contact project orientation at session start.
    RepoStart,
    /// Prompt-time narrowing when a user mentions files, symbols, or paths.
    PromptContext,
    /// Post-edit/write impact analysis on a touched file.
    PostEditImpact,
    /// Direct source-code mutation intent.
    CodeEdit,
    /// Everything intentionally left to fail-open or shell-native handling.
    PassThrough,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HookOutcome {
    Routed,
    NoSidecar,
    SidecarError,
    DaemonFallback,
}

impl HookOutcome {
    pub(crate) fn label(self) -> &'static str {
        match self {
            HookOutcome::Routed => "routed",
            HookOutcome::NoSidecar => "no-sidecar",
            HookOutcome::SidecarError => "sidecar-error",
            HookOutcome::DaemonFallback => "daemon-fallback",
        }
    }

    fn parse(raw: &str) -> Option<Self> {
        match raw {
            "routed" => Some(HookOutcome::Routed),
            "no-sidecar" => Some(HookOutcome::NoSidecar),
            "sidecar-error" => Some(HookOutcome::SidecarError),
            "daemon-fallback" => Some(HookOutcome::DaemonFallback),
            _ => None,
        }
    }
}

#[derive(Default, Clone, Debug, PartialEq, Eq)]
pub(crate) struct WorkflowAdoptionCounts {
    pub routed: usize,
    pub no_sidecar: usize,
    pub sidecar_error: usize,
    pub daemon_fallback: usize,
}

impl WorkflowAdoptionCounts {
    fn record(&mut self, outcome: HookOutcome) {
        match outcome {
            HookOutcome::Routed => self.routed += 1,
            HookOutcome::NoSidecar => self.no_sidecar += 1,
            HookOutcome::SidecarError => self.sidecar_error += 1,
            HookOutcome::DaemonFallback => self.daemon_fallback += 1,
        }
    }

    pub(crate) fn total(&self) -> usize {
        self.routed + self.no_sidecar + self.sidecar_error + self.daemon_fallback
    }

    pub(crate) fn fail_open(&self) -> usize {
        self.no_sidecar + self.sidecar_error
    }
}

#[derive(Default, Clone, Debug, PartialEq, Eq)]
pub(crate) struct HookAdoptionSnapshot {
    pub source_read: WorkflowAdoptionCounts,
    pub source_search: WorkflowAdoptionCounts,
    pub repo_start: WorkflowAdoptionCounts,
    pub prompt_context: WorkflowAdoptionCounts,
    pub post_edit_impact: WorkflowAdoptionCounts,
    pub first_repo_start: Option<HookOutcome>,
}

impl HookAdoptionSnapshot {
    fn counts_mut(&mut self, workflow: HookWorkflow) -> Option<&mut WorkflowAdoptionCounts> {
        match workflow {
            HookWorkflow::SourceRead => Some(&mut self.source_read),
            HookWorkflow::SourceSearch => Some(&mut self.source_search),
            HookWorkflow::RepoStart => Some(&mut self.repo_start),
            HookWorkflow::PromptContext => Some(&mut self.prompt_context),
            HookWorkflow::PostEditImpact => Some(&mut self.post_edit_impact),
            HookWorkflow::CodeEdit | HookWorkflow::PassThrough => None,
        }
    }

    pub(crate) fn total_attempts(&self) -> usize {
        self.source_read.total()
            + self.source_search.total()
            + self.repo_start.total()
            + self.prompt_context.total()
            + self.post_edit_impact.total()
    }

    pub(crate) fn total_routed(&self) -> usize {
        self.source_read.routed
            + self.source_read.daemon_fallback
            + self.source_search.routed
            + self.source_search.daemon_fallback
            + self.repo_start.routed
            + self.repo_start.daemon_fallback
            + self.prompt_context.routed
            + self.prompt_context.daemon_fallback
            + self.post_edit_impact.routed
            + self.post_edit_impact.daemon_fallback
    }

    pub(crate) fn total_fail_open(&self) -> usize {
        self.source_read.fail_open()
            + self.source_search.fail_open()
            + self.repo_start.fail_open()
            + self.prompt_context.fail_open()
            + self.post_edit_impact.fail_open()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.total_attempts() == 0
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Entry point called by main.rs for `symforge hook [subcommand]`.
///
/// When `subcommand` is `None`, reads stdin JSON to determine the tool_name and
/// routes to the correct sidecar endpoint (Phase 6 stdin-routing mode).
///
/// When `subcommand` is `Some`, uses the subcommand directly (backward-compat
/// for manual testing: `symforge hook read`, `symforge hook edit`, etc.).
///
/// Never returns an error — failures produce the fail-open empty JSON.
pub fn run_hook(subcommand: Option<&HookSubcommand>) -> anyhow::Result<()> {
    // Always read stdin so we have context for path/query extraction.
    // For explicit subcommands the payload may be empty or absent — that's fine.
    run_hook_with_input(parse_stdin_input(), subcommand)
}

/// `run_hook` with the stdin payload supplied by the caller instead of read
/// from the process's stdin.
///
/// This is the seam in-process callers (integration tests) must use: reading
/// the real stdin from inside a test binary blocks until the harness's stdin
/// reaches EOF, which never happens when the launching environment holds the
/// pipe open.
pub fn run_hook_with_input(
    input: HookInput,
    subcommand: Option<&HookSubcommand>,
) -> anyhow::Result<()> {
    let verbose = is_hook_verbose();

    // PreTool is a special case: no sidecar call needed, just output a
    // tool-preference suggestion based on the tool_name from stdin.
    //
    // Suppress hints when the SymForge sidecar is already running — this means
    // the agent is actively using SymForge tools and any Read/Grep/Edit calls
    // are intentional fallbacks (e.g., reading external crate source, editing
    // files where raw content is needed in context). Only show the hint when
    // there is no active sidecar, meaning the agent may not realize SymForge
    // is available.
    if matches!(subcommand, Some(HookSubcommand::PreTool)) {
        let sidecar_active = read_port_file().is_ok();
        if !sidecar_active {
            let suggestion = pre_tool_suggestion(&input);
            if !suggestion.is_empty() {
                println!("{}", success_json("PreToolUse", &json_escape(&suggestion)));
            }
        }
        return Ok(());
    }

    // Resolve the effective subcommand: explicit takes priority; otherwise
    // derive from the stdin tool_name.
    let resolved = if let Some(sub) = subcommand {
        Some(sub.clone())
    } else {
        resolve_subcommand_from_input(&input)
    };

    let event_name = resolved
        .as_ref()
        .map(event_name_for)
        .unwrap_or("PostToolUse");
    let workflow = workflow_for_subcommand(resolved.as_ref(), &input);
    let session_id = read_session_file().ok();

    // Conservatively fail open for workflows we do not want to semanticize.
    // This keeps docs/config/non-source reads and unknown tool events from
    // producing unrelated sidecar output.
    if workflow == HookWorkflow::PassThrough {
        if verbose {
            eprintln!("[symforge-hook] workflow=PassThrough — emitting fail-open");
        }
        println!("{}", fail_open_json(event_name));
        return Ok(());
    }

    // Step 1 — read port file; if missing, try daemon fallback before fail-open.
    let (port, effective_session_id, used_daemon_fallback) = match read_port_file() {
        Ok(p) => {
            if verbose {
                eprintln!("[symforge-hook] read port file: port={p}");
            }
            (p, session_id.clone(), false)
        }
        Err(e) => {
            let repo_root = std::env::current_dir().unwrap_or_default();
            let port_file_path = repo_root.join(sidecar_port_file_rel());
            if verbose {
                eprintln!(
                    "[symforge-hook] port file not readable: {e} (searched {})",
                    port_file_path.display()
                );
            }

            // --- Gap 2: Daemon fallback ---
            // Before failing open, check if the SymForge daemon is running and
            // has an active session for this repository.
            if verbose {
                eprintln!("[symforge-hook] attempting daemon fallback...");
            }
            match try_daemon_fallback(&repo_root) {
                Some(fallback) => {
                    if verbose {
                        eprintln!(
                            "[symforge-hook] daemon fallback succeeded: port={}, session={}",
                            fallback.daemon_port, fallback.session_id
                        );
                    }
                    // Daemon has an active session — use its port and session id.
                    (fallback.daemon_port, Some(fallback.session_id), true)
                }
                None => {
                    if verbose {
                        eprintln!("[symforge-hook] daemon fallback failed — no active session");
                    }
                    // --- Gap 1: Enhanced diagnostics ---
                    emit_no_sidecar_diagnostic(&repo_root, &port_file_path);
                    maybe_emit_sidecar_hint(&repo_root);
                    if verbose {
                        eprintln!("[symforge-hook] outcome=NoSidecar reason=sidecar_port_missing");
                    }
                    record_hook_outcome_with_detail(
                        workflow,
                        HookOutcome::NoSidecar,
                        session_id.as_deref(),
                        Some(NoSidecarDetail {
                            reason: "sidecar_port_missing",
                            searched_path: &port_file_path.to_string_lossy(),
                            suggestion: "start_mcp_session",
                            project_root: &repo_root.to_string_lossy(),
                        }),
                    );
                    println!("{}", fail_open_json(event_name));
                    return Ok(());
                }
            }
        }
    };

    // Step 2 — determine endpoint + query string.
    let resolved_ref = resolved.as_ref();
    let (path, query) = endpoint_for(resolved_ref, &input);
    let request_path = proxy_path(path, effective_session_id.as_deref());

    if verbose {
        eprintln!("[symforge-hook] HTTP GET 127.0.0.1:{port}{request_path}?{query}");
    }

    // Keep a copy of the query so the stale-sidecar daemon fallback below can
    // re-issue the same enrichment request — `sync_http_get` consumes `query`.
    let fallback_query = query.clone();

    // Step 3/4 — make sync HTTP GET with 50 ms timeout.
    let (body, outcome) = match sync_http_get(port, &request_path, query) {
        Ok(b) => {
            let initial_outcome = if used_daemon_fallback {
                HookOutcome::DaemonFallback
            } else {
                HookOutcome::Routed
            };
            (b, initial_outcome)
        }
        Err(_) => {
            // Port file existed but the HTTP call failed — the sidecar is dead
            // or stale. Before failing open to a NON-enriched pass-through, try
            // routing the SAME enrichment request through the daemon, which
            // holds the same index. This mirrors the missing-port branch above
            // so a dead sidecar still yields enriched results.
            //
            // Skip the fallback when we were already talking to the daemon
            // (`used_daemon_fallback`): the daemon is the thing that just
            // failed, so a second round-trip would be pointless — and this
            // guard guarantees at most one daemon attempt, never a loop.
            let repo_root = std::env::current_dir().unwrap_or_default();
            let port_file_path = repo_root.join(sidecar_port_file_rel());

            // Honest liveness diagnostics (item b): probe the actual sidecar
            // state so a dead sidecar is never a silent bypass. The probe does
            // a real TCP connect, so it is gated behind verbose to avoid adding
            // latency to the degraded path on every call. The always-on honest
            // signal is the adoption-log outcome recorded below: a successful
            // daemon fallback records `DaemonFallback` (sidecar dead/degraded,
            // served via daemon), and a total miss records `sidecar_port_stale`.
            if verbose {
                let sidecar_dir = repo_root.join(crate::sidecar::port_file::DIR_NAME);
                let liveness =
                    crate::sidecar::port_file::read_sidecar_status_at(&sidecar_dir, "127.0.0.1")
                        .liveness
                        .as_str();
                eprintln!(
                    "[symforge-hook] HTTP request failed — sidecar liveness={liveness}, \
                     attempting daemon fallback before fail-open"
                );
            }

            let daemon_enriched = if used_daemon_fallback {
                None
            } else {
                try_daemon_fallback(&repo_root).and_then(|fallback| {
                    let daemon_request_path = proxy_path(path, Some(&fallback.session_id));
                    if verbose {
                        eprintln!(
                            "[symforge-hook] daemon fallback (stale sidecar): \
                             port={}, session={}",
                            fallback.daemon_port, fallback.session_id
                        );
                    }
                    sync_http_get_with_timeout(
                        fallback.daemon_port,
                        &daemon_request_path,
                        fallback_query,
                        DAEMON_FALLBACK_DEADLINE,
                    )
                    .ok()
                })
            };

            match daemon_enriched {
                Some(b) => {
                    if verbose {
                        eprintln!(
                            "[symforge-hook] daemon fallback succeeded — \
                             sidecar dead/degraded, served enriched result via daemon"
                        );
                    }
                    (b, HookOutcome::DaemonFallback)
                }
                None => {
                    // Both sidecar and daemon are unreachable: degrade to a
                    // pass-through, never hang, never error the editor.
                    if verbose {
                        eprintln!(
                            "[symforge-hook] daemon fallback unavailable — \
                             outcome=NoSidecar reason=sidecar_port_stale"
                        );
                    }
                    maybe_emit_sidecar_hint(&repo_root);
                    record_hook_outcome_with_detail(
                        workflow,
                        HookOutcome::NoSidecar,
                        effective_session_id.as_deref(),
                        Some(NoSidecarDetail {
                            reason: "sidecar_port_stale",
                            searched_path: &port_file_path.to_string_lossy(),
                            suggestion: "restart_sidecar",
                            project_root: &repo_root.to_string_lossy(),
                        }),
                    );
                    println!("{}", fail_open_json(event_name));
                    return Ok(());
                }
            }
        }
    };

    if verbose {
        eprintln!("[symforge-hook] outcome={}", outcome.label());
    }

    // Step 5/6 — output result JSON.
    record_hook_outcome(workflow, outcome, effective_session_id.as_deref());
    println!("{}", success_json(event_name, &body));
    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers (pub for unit-testing, not part of the public module API)
// ---------------------------------------------------------------------------

/// Reads all available stdin lines and deserializes them as a Claude Code
/// PostToolUse JSON payload.
///
/// Returns `HookInput::default()` on any parse failure (fail-open).
/// Upper bound on waiting for the hook payload on stdin.
///
/// Claude Code writes the payload and closes the pipe at spawn, so the read
/// normally completes in well under a millisecond. The bound only matters when
/// stdin is held open with no writer (e.g. the hook is invoked interactively,
/// or from an environment that never closes the inherited pipe) — without it
/// the read blocks forever and the hook hangs the session instead of failing
/// open.
const STDIN_READ_TIMEOUT_MS: u64 = 250;

pub(crate) fn parse_stdin_input() -> HookInput {
    // The blocking read happens on a helper thread so the hook can enforce a
    // deadline. On timeout the thread is leaked — it stays parked on the stdin
    // read — which is acceptable because the hook is a one-shot process and
    // exits immediately after responding. In-process callers (tests) must use
    // `run_hook_with_input` and never reach this function.
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let stdin = std::io::stdin();
        let mut stdin_json = String::new();
        for line in stdin.lock().lines() {
            match line {
                Ok(l) => {
                    stdin_json.push_str(&l);
                    stdin_json.push('\n');
                }
                Err(_) => break,
            }
        }
        let _ = tx.send(stdin_json);
    });
    match rx.recv_timeout(Duration::from_millis(STDIN_READ_TIMEOUT_MS)) {
        Ok(stdin_json) => serde_json::from_str(&stdin_json).unwrap_or_default(),
        // Timeout or disconnected sender: fail open with an empty payload.
        Err(_) => HookInput::default(),
    }
}

/// Converts an absolute path to a relative path by stripping the `cwd` prefix.
///
/// Uses `std::path::Path::strip_prefix` for correct platform-aware stripping,
/// then normalises backslashes to forward slashes for the sidecar query.
/// Returns `absolute` unchanged if it does not start with `cwd`.
pub(crate) fn relative_path(absolute: &str, cwd: &str) -> String {
    let abs = std::path::Path::new(absolute);
    let base = std::path::Path::new(cwd);
    match abs.strip_prefix(base) {
        Ok(rel) => rel.to_string_lossy().replace('\\', "/"),
        Err(_) => absolute.to_string(),
    }
}

/// Maps a `tool_name` string from the stdin JSON to a `HookSubcommand`.
fn resolve_subcommand_from_input(input: &HookInput) -> Option<HookSubcommand> {
    if input.prompt.as_deref().is_some() {
        return Some(HookSubcommand::PromptSubmit);
    }

    match input.tool_name.as_deref() {
        Some("Read") => Some(HookSubcommand::Read),
        Some("Edit") => Some(HookSubcommand::Edit),
        Some("Write") => Some(HookSubcommand::Write),
        Some("Grep") => Some(HookSubcommand::Grep),
        _ => None,
    }
}

/// Returns the `hookEventName` string for a given subcommand.
pub fn event_name_for(subcommand: &HookSubcommand) -> &'static str {
    match subcommand {
        HookSubcommand::SessionStart => "SessionStart",
        HookSubcommand::PromptSubmit => "UserPromptSubmit",
        HookSubcommand::PreTool => "PreToolUse",
        _ => "PostToolUse",
    }
}

/// Returns a tool-preference suggestion for the given tool, or empty string if
/// no suggestion applies (e.g. non-source files, unknown tools).
///
/// This is the core of the PreToolUse interception: it tells the model which
/// SymForge tool to use instead of the built-in tool it's about to call.
fn pre_tool_suggestion(input: &HookInput) -> String {
    let tool = input.tool_name.as_deref().unwrap_or("");
    let cwd = input.cwd.as_deref().unwrap_or("");
    let file = extract_file_path(input, cwd);
    let pattern = input
        .tool_input
        .as_ref()
        .and_then(|ti| ti.pattern.as_deref().or(ti.path.as_deref()))
        .unwrap_or("");

    match pre_tool_workflow(input) {
        HookWorkflow::SourceSearch if !pattern.is_empty() => format!(
            "SymForge can answer this more directly. Start with search_text(query=\"{pattern}\") for symbol-aware source matches, or search_symbols(query=\"{pattern}\") if this is likely a symbol name."
        ),
        HookWorkflow::SourceSearch => "SymForge can answer this more directly. Prefer search_text for source-code search with enclosing symbol context, or search_symbols when you are searching by name/kind.".to_string(),
        HookWorkflow::SourceRead if !file.is_empty() => format!(
            "SymForge can answer this more efficiently. Start with get_file_context(path=\"{file}\") for structure and key references, or get_symbol/get_symbol_context if you only need a specific symbol."
        ),
        HookWorkflow::SourceRead => "SymForge can answer this more efficiently. Prefer get_file_context for source-file structure and get_symbol/get_symbol_context for targeted symbol reads.".to_string(),
        HookWorkflow::CodeEdit => "SymForge MCP is connected. Prefer replace_symbol_body, edit_within_symbol, or batch_edit over Edit for source code modifications — they resolve by symbol name, auto-indent, and re-index atomically.".to_string(),
        HookWorkflow::PassThrough if tool == "Glob" && !pattern.is_empty() => format!(
            "SymForge can narrow this faster. Prefer search_files(query=\"{pattern}\") for ranked path discovery, or get_repo_map if you need a project overview first."
        ),
        HookWorkflow::PassThrough if tool == "Glob" => "SymForge can narrow this faster. Prefer search_files for ranked path discovery, or get_repo_map for repository overview.".to_string(),
        _ => String::new(),
    }
}

/// Classifies the workflow intent behind a pre-tool event.
///
/// This helper intentionally preserves current PR 1 behavior:
/// - source `Read` gets a SymForge suggestion
/// - docs/config/non-source `Read` remains pass-through
/// - `Grep` is treated as source search
/// - `Edit` is treated as code-edit intent
/// - everything else remains pass-through for now
fn pre_tool_workflow(input: &HookInput) -> HookWorkflow {
    let tool = input.tool_name.as_deref().unwrap_or("");
    let file_path = input
        .tool_input
        .as_ref()
        .and_then(|ti| ti.file_path.as_deref())
        .unwrap_or("");

    match tool {
        "Read" if !should_fail_open_read(file_path) => HookWorkflow::SourceRead,
        "Grep" => HookWorkflow::SourceSearch,
        "Edit" => HookWorkflow::CodeEdit,
        _ => HookWorkflow::PassThrough,
    }
}

/// Classifies the workflow intent behind a resolved hook subcommand.
///
/// PR 1 does not change endpoint routing with this helper yet; it exists so
/// later routing work can move from raw tool-name branching to workflow-aware
/// decisions without redefining the vocabulary.
fn workflow_for_subcommand(subcommand: Option<&HookSubcommand>, input: &HookInput) -> HookWorkflow {
    match subcommand {
        Some(HookSubcommand::Read) if !should_fail_open_read(&extract_file_path(input, "")) => {
            HookWorkflow::SourceRead
        }
        Some(HookSubcommand::Read) => HookWorkflow::PassThrough,
        Some(HookSubcommand::Grep) => HookWorkflow::SourceSearch,
        Some(HookSubcommand::SessionStart) => HookWorkflow::RepoStart,
        Some(HookSubcommand::PromptSubmit) => HookWorkflow::PromptContext,
        Some(HookSubcommand::Edit | HookSubcommand::Write) => HookWorkflow::PostEditImpact,
        Some(HookSubcommand::PreTool) => pre_tool_workflow(input),
        None => HookWorkflow::PassThrough,
    }
}

/// Returns true when a read should stay conservative and fail open instead of
/// being steered into semantic code-inspection flows.
///
/// This is intentionally broader than `is_non_source_path`: SymForge may index
/// many config files, but exact raw reads of docs/configs are still often the
/// correct user intent.
fn should_fail_open_read(path: &str) -> bool {
    if is_non_source_path(path) {
        return true;
    }

    let p = path.replace('\\', "/").to_lowercase();
    let literal_read_exts = [
        ".md", ".mdx", ".txt", ".json", ".toml", ".yaml", ".yml", ".env",
    ];
    literal_read_exts.iter().any(|ext| p.ends_with(ext))
}

/// Returns true for paths that are clearly outside source-code inspection
/// flows, such as docs directories, binary-ish assets, and other non-code
/// artifacts.
///
/// This helper is intentionally coarser than `should_fail_open_read`: config
/// and doc-like extensions can still stay out of semantic hook routing even if
/// we do not classify them as broad non-source paths here.
fn is_non_source_path(path: &str) -> bool {
    let p = path.replace('\\', "/").to_lowercase();

    // Broadly non-source file extensions. Literal docs/config reads are handled
    // separately by `should_fail_open_read`.
    let non_source_exts = [
        ".txt",
        ".xml",
        ".csv",
        ".lock",
        ".gitignore",
        ".dockerignore",
        ".editorconfig",
        ".prettierrc",
        ".eslintrc",
        ".ini",
        ".cfg",
        ".conf",
        ".html",
        ".css",
        ".svg",
        ".png",
        ".jpg",
        ".jpeg",
        ".gif",
        ".ico",
    ];
    if non_source_exts.iter().any(|ext| p.ends_with(ext)) {
        return true;
    }

    // Non-source directories
    let non_source_dirs = [
        "/docs/",
        "/doc/",
        "/.github/",
        "/.planning/",
        "/.claude/",
        "/.gemini/",
        "/.codex/",
        "/node_modules/",
        "/.git/",
    ];
    if non_source_dirs.iter().any(|dir| p.contains(dir)) {
        return true;
    }

    false
}

/// Maps a resolved subcommand + stdin input to `(path, query_string)`.
///
/// The `input` carries the file path and search pattern extracted from the
/// Claude Code PostToolUse payload. When `subcommand` is `None` (unknown
/// tool_name), returns fail-open empty values.
pub(crate) fn endpoint_for(
    subcommand: Option<&HookSubcommand>,
    input: &HookInput,
) -> (&'static str, String) {
    let cwd = input.cwd.as_deref().unwrap_or("");

    match subcommand {
        Some(HookSubcommand::Read) => {
            let file = extract_file_path(input, cwd);
            let query = if file.is_empty() {
                String::new()
            } else {
                format!("path={}", url_encode(&file))
            };
            ("/outline", query)
        }
        Some(HookSubcommand::Edit) => {
            let file = extract_file_path(input, cwd);
            let query = if file.is_empty() {
                String::new()
            } else {
                format!("path={}", url_encode(&file))
            };
            ("/impact", query)
        }
        Some(HookSubcommand::Write) => {
            let file = extract_file_path(input, cwd);
            let query = if file.is_empty() {
                "new_file=true".to_string()
            } else {
                format!("path={}&new_file=true", url_encode(&file))
            };
            ("/impact", query)
        }
        Some(HookSubcommand::Grep) => {
            // Use `pattern` field first, then fall back to `path` (directory) field.
            let q = input
                .tool_input
                .as_ref()
                .and_then(|ti| ti.pattern.as_deref().or(ti.path.as_deref()))
                .unwrap_or("");
            let query = if q.is_empty() {
                String::new()
            } else {
                format!("name={}", url_encode(q))
            };
            ("/symbol-context", query)
        }
        Some(HookSubcommand::SessionStart) => ("/repo-map", String::new()),
        Some(HookSubcommand::PromptSubmit) => {
            let prompt = input.prompt.as_deref().unwrap_or("");
            let query = if prompt.is_empty() {
                String::new()
            } else {
                format!("text={}", url_encode(prompt))
            };
            ("/prompt-context", query)
        }
        // PreTool is handled before endpoint_for is called; this arm is
        // unreachable but required for exhaustiveness.
        Some(HookSubcommand::PreTool) => ("/health", String::new()),
        // Unknown tool_name → fail-open: route to a no-op that returns empty.
        None => ("/health", String::new()),
    }
}

/// Returns the fail-open JSON: empty `additionalContext`.
pub fn fail_open_json(event_name: &str) -> String {
    format!(r#"{{"hookSpecificOutput":{{"hookEventName":"{event_name}","additionalContext":""}}}}"#)
}

/// Returns the success JSON with `context` as the `additionalContext` value.
///
/// The `context` string is JSON-escaped (backslash + quote safe) so it can be
/// embedded as a JSON string value.
pub fn success_json(event_name: &str, context: &str) -> String {
    let escaped = json_escape(context);
    format!(
        r#"{{"hookSpecificOutput":{{"hookEventName":"{event_name}","additionalContext":"{escaped}"}}}}"#
    )
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Extract and relativize the file path from stdin input.
fn extract_file_path(input: &HookInput, cwd: &str) -> String {
    let abs = input
        .tool_input
        .as_ref()
        .and_then(|ti| ti.file_path.as_deref())
        .unwrap_or("");
    if abs.is_empty() || cwd.is_empty() {
        abs.to_string()
    } else {
        relative_path(abs, cwd)
    }
}

/// Read the OS-tagged `.symforge/sidecar.<os>.port` (legacy fallback) from the CWD.
fn read_port_file() -> std::io::Result<u16> {
    let contents = read_runtime_rel(&sidecar_port_file_rel(), LEGACY_PORT_FILE)?;
    contents
        .trim()
        .parse::<u16>()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

fn read_session_file() -> std::io::Result<String> {
    let contents = read_runtime_rel(&sidecar_session_file_rel(), LEGACY_SESSION_FILE)?;
    Ok(contents.trim().to_string())
}

fn proxy_path(base_path: &str, session_id: Option<&str>) -> String {
    match session_id {
        Some(session_id) if !session_id.trim().is_empty() => {
            format!("/v1/sessions/{}/sidecar{}", session_id.trim(), base_path)
        }
        _ => base_path.to_string(),
    }
}

fn tracked_workflow_name(workflow: HookWorkflow) -> Option<&'static str> {
    match workflow {
        HookWorkflow::SourceRead => Some("source-read"),
        HookWorkflow::SourceSearch => Some("source-search"),
        HookWorkflow::RepoStart => Some("repo-start"),
        HookWorkflow::PromptContext => Some("prompt-context"),
        HookWorkflow::PostEditImpact => Some("post-edit-impact"),
        HookWorkflow::CodeEdit | HookWorkflow::PassThrough => None,
    }
}

fn parse_tracked_workflow(raw: &str) -> Option<HookWorkflow> {
    match raw {
        "source-read" => Some(HookWorkflow::SourceRead),
        "source-search" => Some(HookWorkflow::SourceSearch),
        "repo-start" => Some(HookWorkflow::RepoStart),
        "prompt-context" => Some(HookWorkflow::PromptContext),
        "post-edit-impact" => Some(HookWorkflow::PostEditImpact),
        _ => None,
    }
}

fn record_hook_outcome(workflow: HookWorkflow, outcome: HookOutcome, session_id: Option<&str>) {
    let Some(workflow_name) = tracked_workflow_name(workflow) else {
        return;
    };
    let _ = append_hook_adoption_event(
        Path::new(ADOPTION_LOG_FILE),
        session_id,
        workflow_name,
        outcome.label(),
    );
}

// ---------------------------------------------------------------------------
// Daemon fallback (Gap 2)
// ---------------------------------------------------------------------------

/// Result of a successful daemon fallback lookup.
struct DaemonFallbackResult {
    daemon_port: u16,
    session_id: String,
}

/// Try to find an active daemon session for the given repo root.
///
/// Returns `Some(DaemonFallbackResult)` if the daemon is running and has a
/// session whose canonical_root matches `repo_root`. Returns `None` if the
/// daemon is unreachable, has no matching project, or any step times out.
///
/// Total budget: DAEMON_FALLBACK_DEADLINE (500ms shared across all steps).
fn try_daemon_fallback(repo_root: &Path) -> Option<DaemonFallbackResult> {
    let deadline = std::time::Instant::now() + DAEMON_FALLBACK_DEADLINE;

    // Step 1: Read the daemon port file (~/.symforge/daemon.port).
    let daemon_port = crate::daemon::read_daemon_port_file().ok()?;

    // Step 2: Query GET /v1/projects for the list of active projects.
    let remaining = deadline.checked_duration_since(std::time::Instant::now())?;
    let projects_json =
        sync_http_get_with_timeout(daemon_port, "/v1/projects", String::new(), remaining).ok()?;

    // Step 3: Parse the projects list and find one matching this repo root.
    let canon_root = std::fs::canonicalize(repo_root).unwrap_or_else(|_| repo_root.to_path_buf());
    let canon_root_str = normalize_path_for_match(&canon_root);

    // Minimal serde structs for daemon JSON responses.
    // The daemon returns a JSON array of objects with `canonical_root`,
    // `project_id`, and `session_count` fields.
    let projects: Vec<DaemonProjectEntry> = serde_json::from_str(&projects_json).ok()?;

    let matching = projects
        .iter()
        .find(|p| normalize_path_for_match(Path::new(&p.canonical_root)) == canon_root_str)?;

    if matching.session_count == 0 {
        return None;
    }

    // Step 4: Query GET /v1/projects/{project_id}/sessions to get a session id.
    let remaining = deadline.checked_duration_since(std::time::Instant::now())?;
    let sessions_path = format!("/v1/projects/{}/sessions", url_encode(&matching.project_id));
    let sessions_json =
        sync_http_get_with_timeout(daemon_port, &sessions_path, String::new(), remaining).ok()?;

    let sessions: Vec<DaemonSessionEntry> = serde_json::from_str(&sessions_json).ok()?;

    // Pick the most recently seen session.
    let session = sessions.iter().max_by_key(|s| s.last_seen_at_unix_secs)?;

    Some(DaemonFallbackResult {
        daemon_port,
        session_id: session.session_id.clone(),
    })
}

/// Minimal deserialization struct for daemon project list entries.
#[derive(serde::Deserialize)]
struct DaemonProjectEntry {
    project_id: String,
    canonical_root: String,
    session_count: usize,
}

/// Minimal deserialization struct for daemon session list entries.
#[derive(serde::Deserialize)]
struct DaemonSessionEntry {
    session_id: String,
    last_seen_at_unix_secs: u64,
}

/// Normalize a path for cross-platform comparison: lowercase on Windows,
/// forward slashes everywhere, no trailing separator.
fn normalize_path_for_match(path: &Path) -> String {
    let s = path.to_string_lossy().replace('\\', "/");
    let trimmed = s.trim_end_matches('/');
    if cfg!(windows) {
        trimmed.to_lowercase()
    } else {
        trimmed.to_string()
    }
}

/// Like `sync_http_get` but with a configurable timeout.
fn sync_http_get_with_timeout(
    port: u16,
    path: &str,
    query: String,
    timeout: Duration,
) -> anyhow::Result<String> {
    let addr = format!("127.0.0.1:{port}");
    let sock_addr: std::net::SocketAddr = addr.parse()?;

    let mut stream = TcpStream::connect_timeout(&sock_addr, timeout)?;
    stream.set_read_timeout(Some(timeout))?;
    stream.set_write_timeout(Some(timeout))?;

    let request_path = if query.is_empty() {
        path.to_string()
    } else {
        format!("{path}?{query}")
    };

    // Resolve the daemon auth token the same way the MCP proxy does — env pin
    // first, then the daemon's persisted token file — so the hook authenticates
    // against the now fail-closed daemon even when it has no env pin of its own.
    let auth_header = crate::daemon::resolve_daemon_auth_token()
        .map(|token| format!("Authorization: Bearer {token}\r\n"))
        .unwrap_or_default();
    let request = format!(
        "GET {request_path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n{auth_header}Connection: close\r\n\r\n"
    );

    stream.write_all(request.as_bytes())?;

    let mut response = String::new();
    stream.read_to_string(&mut response)?;

    let (headers, body) = response
        .split_once("\r\n\r\n")
        .ok_or_else(|| anyhow::anyhow!("malformed HTTP response: no header/body separator"))?;

    let status_line = headers
        .lines()
        .next()
        .ok_or_else(|| anyhow::anyhow!("malformed HTTP response: empty headers"))?;

    // Status line format: "HTTP/1.1 200 OK"
    let status_code: u16 = status_line
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("malformed HTTP status line: {status_line}"))?
        .parse()
        .map_err(|_| anyhow::anyhow!("non-numeric HTTP status code in: {status_line}"))?;

    if !(200..=299).contains(&status_code) {
        anyhow::bail!("HTTP {status_code} from {path}");
    }

    // Check for chunked transfer-encoding. The sidecar uses hyper which may
    // send chunked responses. Since we use Connection: close and read_to_string,
    // the raw body includes chunk framing that must be decoded.
    let is_chunked = headers.lines().any(|line| {
        let lower = line.to_lowercase();
        lower.starts_with("transfer-encoding:") && lower.contains("chunked")
    });

    if is_chunked {
        Ok(decode_chunked_body(body))
    } else {
        Ok(body.to_string())
    }
}

/// Decode a chunked transfer-encoding body into a plain string.
/// Each chunk is: `<hex-size>\r\n<data>\r\n`, terminated by `0\r\n\r\n`.
fn decode_chunked_body(raw: &str) -> String {
    let mut result = String::new();
    let mut remainder = raw;
    while let Some(size_end) = remainder.find("\r\n") {
        // Find chunk size line
        let size_str = remainder[..size_end].trim();
        let chunk_size = match usize::from_str_radix(size_str, 16) {
            Ok(0) => break, // Terminal chunk
            Ok(n) => n,
            Err(_) => break, // Malformed — return what we have
        };
        let data_start = size_end + 2; // skip \r\n
        if data_start + chunk_size > remainder.len() {
            // Incomplete chunk — append what's available
            result.push_str(&remainder[data_start..]);
            break;
        }
        result.push_str(&remainder[data_start..data_start + chunk_size]);
        // Skip past chunk data + trailing \r\n
        let next = data_start + chunk_size + 2;
        if next > remainder.len() {
            break;
        }
        remainder = &remainder[next..];
    }
    result
}

// ---------------------------------------------------------------------------
// Enhanced diagnostics (Gap 1)
// ---------------------------------------------------------------------------

/// Structured detail for no-sidecar adoption log entries.
struct NoSidecarDetail<'a> {
    reason: &'a str,
    searched_path: &'a str,
    suggestion: &'a str,
    project_root: &'a str,
}

/// Check whether verbose hook diagnostics are enabled.
///
/// Set `SYMFORGE_HOOK_VERBOSE=1` to enable detailed stderr output from the hook.
fn is_hook_verbose() -> bool {
    std::env::var("SYMFORGE_HOOK_VERBOSE").is_ok_and(|v| v == "1")
}

/// Marker file path for the one-time sidecar hint (HOOK-03).
const HOOK_HINT_MARKER: &str = ".symforge/hook-hint-shown";

/// Freshness window for the sidecar hint marker file (30 minutes).
const HOOK_HINT_FRESHNESS: Duration = Duration::from_secs(30 * 60);

/// Emit a one-time hint to stderr when the sidecar is not running (HOOK-03).
///
/// Uses a marker file (`.symforge/hook-hint-shown`) to avoid repeating the hint
/// within a 30-minute window. The hint is written to stderr regardless of
/// `SYMFORGE_HOOK_VERBOSE` — it is specifically a user-facing one-time hint.
///
/// All I/O failures are silently ignored to preserve fail-open behavior.
fn maybe_emit_sidecar_hint(repo_root: &Path) {
    let marker_path = repo_root.join(HOOK_HINT_MARKER);

    // Check if the marker file is fresh (modified within the last 30 minutes).
    if let Ok(metadata) = std::fs::metadata(&marker_path)
        && let Ok(modified) = metadata.modified()
        && let Ok(elapsed) = modified.elapsed()
        && elapsed < HOOK_HINT_FRESHNESS
    {
        // Hint was shown recently — skip.
        return;
    }

    // Write the hint to stderr.
    eprintln!("[symforge-hook] SymForge sidecar is not running. To enable rich context:");
    eprintln!(
        "[symforge-hook]   \u{2022} Configure SymForge as an MCP server in your editor settings"
    );
    eprintln!("[symforge-hook]   \u{2022} Or run: symforge --stdio");
    eprintln!("[symforge-hook] (This hint appears once per session)");

    // Touch / create the marker file so we don't repeat within 30 minutes.
    if let Some(parent) = marker_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&marker_path, "");
}

/// Emit a diagnostic message to stderr explaining why the hook is failing open.
///
/// This helps users who run hooks manually understand what's missing and how
/// to fix it. The message is written to stderr so it doesn't interfere with
/// the JSON output on stdout.
///
/// Gated behind `SYMFORGE_HOOK_VERBOSE=1` (HOOK-02).
fn emit_no_sidecar_diagnostic(repo_root: &Path, port_file_path: &Path) {
    if !is_hook_verbose() {
        return;
    }

    let daemon_status = if crate::daemon::read_daemon_port_file().is_ok() {
        "SymForge daemon is running but has no active session for this project."
    } else {
        "SymForge daemon is not running."
    };

    eprintln!(
        "[symforge-hook] sidecar not running. No {} found in {}.",
        sidecar_port_file_rel().display(),
        repo_root.display()
    );
    eprintln!("[symforge-hook]   Searched: {}", port_file_path.display());
    eprintln!("[symforge-hook]   {daemon_status}");
    eprintln!(
        "[symforge-hook]   To start: run 'symforge' as an MCP server, or start a Claude/Codex session with SymForge configured."
    );
    eprintln!("[symforge-hook]   Hook falling back to pass-through mode.");
}

/// Record a hook outcome with optional structured detail for the adoption log.
fn record_hook_outcome_with_detail(
    workflow: HookWorkflow,
    outcome: HookOutcome,
    session_id: Option<&str>,
    detail: Option<NoSidecarDetail<'_>>,
) {
    let Some(workflow_name) = tracked_workflow_name(workflow) else {
        return;
    };
    let _ = append_hook_adoption_event_with_detail(
        Path::new(ADOPTION_LOG_FILE),
        session_id,
        workflow_name,
        outcome.label(),
        detail,
    );
}

/// Append a hook adoption event with optional structured detail fields.
///
/// Extended log format (tab-separated):
///   session_id \t workflow \t outcome [\t reason=X \t searched_path=X \t suggestion=X]
fn append_hook_adoption_event_with_detail(
    log_path: &Path,
    session_id: Option<&str>,
    workflow_name: &str,
    outcome_label: &str,
    detail: Option<NoSidecarDetail<'_>>,
) -> std::io::Result<()> {
    if let Some(parent) = log_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)?;
    let session = session_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("-");

    match detail {
        Some(d) => writeln!(
            file,
            "{session}\t{workflow_name}\t{outcome_label}\treason={}\tsearched_path={}\tsuggestion={}\tproject_root={}",
            d.reason, d.searched_path, d.suggestion, d.project_root
        ),
        None => writeln!(file, "{session}\t{workflow_name}\t{outcome_label}"),
    }
}

fn append_hook_adoption_event(
    log_path: &Path,
    session_id: Option<&str>,
    workflow_name: &str,
    outcome_label: &str,
) -> std::io::Result<()> {
    if let Some(parent) = log_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)?;
    let session = session_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("-");
    writeln!(file, "{session}\t{workflow_name}\t{outcome_label}")
}

fn adoption_log_path(repo_root: Option<&Path>) -> PathBuf {
    repo_root
        .unwrap_or_else(|| Path::new("."))
        .join(ADOPTION_LOG_FILE)
}

fn session_file_path(repo_root: Option<&Path>) -> PathBuf {
    repo_root
        .unwrap_or_else(|| Path::new("."))
        .join(sidecar_session_file_rel())
}

fn read_session_id_for_repo(repo_root: Option<&Path>) -> Option<String> {
    let base = repo_root.unwrap_or_else(|| Path::new("."));
    read_runtime_rel(
        &session_file_path(repo_root),
        &base.join(LEGACY_SESSION_FILE).to_string_lossy(),
    )
    .ok()
    .map(|text| text.trim().to_string())
    .filter(|value| !value.is_empty())
}

fn load_hook_adoption_snapshot_from_path(
    log_path: &Path,
    session_filter: Option<&str>,
) -> std::io::Result<HookAdoptionSnapshot> {
    let Ok(contents) = std::fs::read_to_string(log_path) else {
        return Ok(HookAdoptionSnapshot::default());
    };

    let mut snapshot = HookAdoptionSnapshot::default();
    for line in contents.lines() {
        let mut parts = line.split('\t');
        let Some(session_id) = parts.next() else {
            continue;
        };
        let Some(workflow_raw) = parts.next() else {
            continue;
        };
        let Some(outcome_raw) = parts.next() else {
            continue;
        };

        if let Some(filter) = session_filter
            && session_id != filter
        {
            continue;
        }

        let Some(workflow) = parse_tracked_workflow(workflow_raw) else {
            continue;
        };
        let Some(outcome) = HookOutcome::parse(outcome_raw) else {
            continue;
        };

        if workflow == HookWorkflow::RepoStart && snapshot.first_repo_start.is_none() {
            snapshot.first_repo_start = Some(outcome);
        }
        if let Some(counts) = snapshot.counts_mut(workflow) {
            counts.record(outcome);
        }
    }

    Ok(snapshot)
}

pub(crate) fn load_hook_adoption_snapshot(repo_root: Option<&Path>) -> HookAdoptionSnapshot {
    let session = read_session_id_for_repo(repo_root);
    load_hook_adoption_snapshot_from_path(&adoption_log_path(repo_root), session.as_deref())
        .unwrap_or_default()
}

/// Make a synchronous HTTP/1.1 GET request to `127.0.0.1:{port}{path}?{query}`.
///
/// Uses a raw `TcpStream` (no HTTP client crate) so there is no async runtime
/// and the startup cost is near zero.  The timeout covers both connect and read.
fn sync_http_get(port: u16, path: &str, query: String) -> anyhow::Result<String> {
    sync_http_get_with_timeout(port, path, query, HTTP_TIMEOUT)
}

/// Minimal percent-encoding for query parameter values.
///
/// Only encodes characters that are unsafe in a query string: space, `&`, `=`, `+`,
/// `%`, and non-ASCII bytes.  This is sufficient for file paths and symbol names.
fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' | b':' => {
                out.push(b as char)
            }
            b => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Minimal JSON string escape — handles backslash, double-quote, and common
/// control characters.  Sufficient for embedding sidecar response bodies.
fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use once_cell::sync::Lazy;
    use serde_json::Value;
    use std::sync::Mutex;
    use tempfile::TempDir;

    static HOOK_VERBOSE_ENV_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

    // --- fail_open_json ---

    #[test]
    fn test_fail_open_json_is_valid() {
        let json = fail_open_json("PostToolUse");
        let v: Value = serde_json::from_str(&json).expect("fail_open_json must produce valid JSON");

        let output = &v["hookSpecificOutput"];
        assert_eq!(output["hookEventName"], "PostToolUse");
        assert_eq!(output["additionalContext"], "");
    }

    #[test]
    fn test_fail_open_json_session_start_event_name() {
        let json = fail_open_json("SessionStart");
        let v: Value = serde_json::from_str(&json).expect("must be valid JSON");
        assert_eq!(v["hookSpecificOutput"]["hookEventName"], "SessionStart");
    }

    // --- success_json ---

    #[test]
    fn test_success_json_is_valid() {
        let json = success_json("PostToolUse", "hello world");
        let v: Value = serde_json::from_str(&json).expect("success_json must produce valid JSON");

        let output = &v["hookSpecificOutput"];
        assert_eq!(output["hookEventName"], "PostToolUse");
        assert_eq!(output["additionalContext"], "hello world");
    }

    #[test]
    fn test_success_json_escapes_special_chars() {
        let context = r#"{"key":"value"}"#;
        let json = success_json("PostToolUse", context);
        // The outer JSON must parse correctly.
        let v: Value = serde_json::from_str(&json)
            .expect("success_json with embedded quotes must be valid JSON");
        // The additionalContext value is the escaped string, not a nested object.
        let ctx = v["hookSpecificOutput"]["additionalContext"]
            .as_str()
            .expect("additionalContext must be a string");
        assert_eq!(ctx, context);
    }

    // --- parse_stdin_input ---

    #[test]
    fn test_parse_stdin_returns_default_on_empty() {
        // We cannot pipe into stdin in a unit test, but we can verify that
        // parsing an empty string returns Default (no panics).
        let result: HookInput = serde_json::from_str("").unwrap_or_default();
        assert!(result.tool_name.is_none());
        assert!(result.tool_input.is_none());
        assert!(result.cwd.is_none());
    }

    #[test]
    fn test_parse_stdin_deserializes_read_payload() {
        let json =
            r#"{"tool_name":"Read","tool_input":{"file_path":"/abs/src/foo.rs"},"cwd":"/abs"}"#;
        let result: HookInput = serde_json::from_str(json).unwrap_or_default();
        assert_eq!(result.tool_name.as_deref(), Some("Read"));
        assert_eq!(
            result
                .tool_input
                .as_ref()
                .and_then(|ti| ti.file_path.as_deref()),
            Some("/abs/src/foo.rs")
        );
        assert_eq!(result.cwd.as_deref(), Some("/abs"));
    }

    #[test]
    fn test_parse_stdin_deserializes_grep_payload() {
        let json = r#"{"tool_name":"Grep","tool_input":{"pattern":"TODO","path":"/abs/src"},"cwd":"/abs"}"#;
        let result: HookInput = serde_json::from_str(json).unwrap_or_default();
        assert_eq!(result.tool_name.as_deref(), Some("Grep"));
        let ti = result.tool_input.as_ref().unwrap();
        assert_eq!(ti.pattern.as_deref(), Some("TODO"));
        assert_eq!(ti.path.as_deref(), Some("/abs/src"));
    }

    #[test]
    fn test_parse_stdin_returns_default_on_invalid_json() {
        let result: HookInput = serde_json::from_str("not valid json").unwrap_or_default();
        assert!(result.tool_name.is_none());
    }

    // --- relative_path ---

    #[test]
    fn test_relative_path_strips_unix_cwd_prefix() {
        let rel = relative_path("/home/user/project/src/foo.rs", "/home/user/project");
        assert_eq!(rel, "src/foo.rs");
    }

    #[test]
    fn test_relative_path_strips_windows_cwd_prefix() {
        // Test that strip_prefix works for Windows-style paths.
        // Path::strip_prefix is platform-aware, but we test the string normalization.
        // On Windows the actual separator is backslash; strip_prefix handles it.
        // We simulate by using a path that has a clear prefix relationship.
        let rel = relative_path("C:/Users/dev/project/src/foo.rs", "C:/Users/dev/project");
        // After strip_prefix the result should use forward slashes.
        assert!(
            rel.contains("src/foo.rs") || rel == "C:/Users/dev/project/src/foo.rs",
            "got: {rel}"
        );
    }

    #[test]
    fn test_relative_path_unchanged_when_no_prefix_match() {
        let rel = relative_path("/unrelated/path.rs", "/home/user/project");
        assert_eq!(rel, "/unrelated/path.rs");
    }

    #[test]
    #[cfg(windows)]
    fn test_relative_path_normalizes_backslashes() {
        // Simulate a Windows-style result from strip_prefix.
        // Since we're on MSYS/Windows the path may use backslashes.
        let rel = relative_path(
            "C:\\Users\\dev\\project\\src\\foo.rs",
            "C:\\Users\\dev\\project",
        );
        // Must not contain backslashes in result.
        assert!(
            !rel.contains('\\'),
            "backslashes must be normalized to forward slashes; got: {rel}"
        );
    }

    // --- endpoint_for (stdin-routing) ---

    #[test]
    fn test_endpoint_for_read_stdin_routes_to_outline() {
        let input = make_input("Read", Some("/abs/src/foo.rs"), None, "/abs");
        let (path, query) = endpoint_for(Some(&HookSubcommand::Read), &input);
        assert_eq!(path, "/outline");
        assert!(
            query.contains("src/foo.rs"),
            "query must include relative path; got: {query}"
        );
    }

    #[test]
    fn test_endpoint_for_edit_stdin_routes_to_impact() {
        let input = make_input("Edit", Some("/abs/src/bar.rs"), None, "/abs");
        let (path, query) = endpoint_for(Some(&HookSubcommand::Edit), &input);
        assert_eq!(path, "/impact");
        assert!(
            query.contains("src/bar.rs"),
            "query must include relative path; got: {query}"
        );
    }

    #[test]
    fn test_endpoint_for_write_routes_to_impact_with_new_file() {
        let input = make_input("Write", Some("/abs/src/new.rs"), None, "/abs");
        let (path, query) = endpoint_for(Some(&HookSubcommand::Write), &input);
        assert_eq!(path, "/impact");
        assert!(
            query.contains("new_file=true"),
            "Write must set new_file=true; got: {query}"
        );
        assert!(
            query.contains("src/new.rs"),
            "Write must include file path; got: {query}"
        );
    }

    #[test]
    fn test_endpoint_for_grep_stdin_routes_to_symbol_context() {
        let json = r#"{"tool_name":"Grep","tool_input":{"pattern":"TODO","path":"/abs/src"},"cwd":"/abs"}"#;
        let input: HookInput = serde_json::from_str(json).unwrap_or_default();
        let (path, query) = endpoint_for(Some(&HookSubcommand::Grep), &input);
        assert_eq!(path, "/symbol-context");
        assert!(
            query.contains("TODO"),
            "Grep query must include pattern; got: {query}"
        );
    }

    #[test]
    fn test_endpoint_for_session_start_routes_to_repo_map() {
        let input = HookInput::default();
        let (path, query) = endpoint_for(Some(&HookSubcommand::SessionStart), &input);
        assert_eq!(path, "/repo-map");
        assert!(query.is_empty(), "repo-map has no query params");
    }

    #[test]
    fn test_endpoint_for_prompt_submit_routes_to_prompt_context() {
        let input = HookInput {
            prompt: Some("please inspect src/foo.rs".to_string()),
            ..HookInput::default()
        };
        let (path, query) = endpoint_for(Some(&HookSubcommand::PromptSubmit), &input);
        assert_eq!(path, "/prompt-context");
        assert!(
            query.contains("please%20inspect%20src/foo.rs"),
            "prompt query must be URL-encoded; got: {query}"
        );
    }

    #[test]
    fn test_proxy_path_uses_daemon_session_namespace_when_present() {
        let path = proxy_path("/repo-map", Some("session-42"));
        assert_eq!(path, "/v1/sessions/session-42/sidecar/repo-map");
    }

    #[test]
    fn test_proxy_path_returns_base_path_without_session() {
        let path = proxy_path("/repo-map", None);
        assert_eq!(path, "/repo-map");
    }

    #[test]
    fn test_endpoint_for_unknown_tool_returns_fail_open() {
        // None subcommand with unknown/missing tool_name → fail-open /health endpoint
        let input = HookInput {
            tool_name: Some("UnknownTool".to_string()),
            ..Default::default()
        };
        let (path, _) = endpoint_for(None, &input);
        // Returns /health as the fail-open endpoint — no useful data, but graceful
        assert_eq!(path, "/health");
    }

    // --- event_name_for ---

    #[test]
    fn test_event_name_for_session_start() {
        assert_eq!(
            event_name_for(&HookSubcommand::SessionStart),
            "SessionStart"
        );
    }

    #[test]
    fn test_event_name_for_prompt_submit() {
        assert_eq!(
            event_name_for(&HookSubcommand::PromptSubmit),
            "UserPromptSubmit"
        );
    }

    #[test]
    fn test_event_name_for_post_tool_use_variants() {
        for sub in [
            HookSubcommand::Read,
            HookSubcommand::Edit,
            HookSubcommand::Write,
            HookSubcommand::Grep,
        ] {
            assert_eq!(
                event_name_for(&sub),
                "PostToolUse",
                "Read/Edit/Write/Grep must produce PostToolUse event name"
            );
        }
    }

    // --- explicit subcommand routing remains available ---

    #[test]
    fn test_hook_subcommand_to_endpoint_read_backward_compat() {
        let input = HookInput::default();
        let (path, _query) = endpoint_for(Some(&HookSubcommand::Read), &input);
        assert_eq!(path, "/outline");
    }

    #[test]
    fn test_hook_subcommand_to_endpoint_edit_backward_compat() {
        let input = HookInput::default();
        let (path, _query) = endpoint_for(Some(&HookSubcommand::Edit), &input);
        assert_eq!(path, "/impact");
    }

    #[test]
    fn test_hook_subcommand_to_endpoint_grep_backward_compat() {
        let input = HookInput::default();
        let (path, _query) = endpoint_for(Some(&HookSubcommand::Grep), &input);
        assert_eq!(path, "/symbol-context");
    }

    #[test]
    fn test_hook_subcommand_to_endpoint_session_start_backward_compat() {
        let input = HookInput::default();
        let (path, query) = endpoint_for(Some(&HookSubcommand::SessionStart), &input);
        assert_eq!(path, "/repo-map");
        assert!(query.is_empty(), "repo-map has no query params");
    }

    #[test]
    fn test_hook_subcommand_to_endpoint_prompt_submit_backward_compat() {
        let input = HookInput {
            prompt: Some("review MinioService".to_string()),
            ..HookInput::default()
        };
        let (path, query) = endpoint_for(Some(&HookSubcommand::PromptSubmit), &input);
        assert_eq!(path, "/prompt-context");
        assert!(query.contains("review%20MinioService"));
    }

    // --- resolve_subcommand_from_input ---

    #[test]
    fn test_resolve_subcommand_read() {
        let input = HookInput {
            tool_name: Some("Read".to_string()),
            ..Default::default()
        };
        assert!(matches!(
            resolve_subcommand_from_input(&input),
            Some(HookSubcommand::Read)
        ));
    }

    #[test]
    fn test_resolve_subcommand_write() {
        let input = HookInput {
            tool_name: Some("Write".to_string()),
            ..Default::default()
        };
        assert!(matches!(
            resolve_subcommand_from_input(&input),
            Some(HookSubcommand::Write)
        ));
    }

    #[test]
    fn test_resolve_subcommand_unknown_returns_none() {
        let input = HookInput {
            tool_name: Some("Bash".to_string()),
            ..Default::default()
        };
        assert!(resolve_subcommand_from_input(&input).is_none());
    }

    // --- helpers ---

    // --- pre_tool_suggestion ---

    #[test]
    fn test_pre_tool_suggestion_grep_suggests_search_text() {
        let input = HookInput {
            tool_name: Some("Grep".to_string()),
            tool_input: Some(HookToolInput {
                pattern: Some("helper".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let s = pre_tool_suggestion(&input);
        assert!(s.contains("search_text"), "should suggest search_text: {s}");
        assert!(
            s.contains("helper"),
            "should include the query in the hint: {s}"
        );
    }

    #[test]
    fn test_pre_tool_suggestion_read_source_suggests_get_file_context() {
        let input = HookInput {
            tool_name: Some("Read".to_string()),
            tool_input: Some(HookToolInput {
                file_path: Some("/repo/src/main.rs".to_string()),
                ..Default::default()
            }),
            cwd: Some("/repo".to_string()),
            ..Default::default()
        };
        let s = pre_tool_suggestion(&input);
        assert!(
            s.contains("get_file_context"),
            "should suggest get_file_context for source: {s}"
        );
        assert!(
            s.contains("src/main.rs"),
            "should include the path hint: {s}"
        );
    }

    #[test]
    fn test_pre_tool_suggestion_read_markdown_is_empty() {
        // PR 2 keeps docs/config conservative at hook time.
        let input = HookInput {
            tool_name: Some("Read".to_string()),
            tool_input: Some(HookToolInput {
                file_path: Some("docs/README.md".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let s = pre_tool_suggestion(&input);
        assert!(
            s.is_empty(),
            "should stay pass-through for markdown reads: {s}"
        );
    }

    #[test]
    fn test_pre_tool_suggestion_read_csv_is_empty() {
        // CSV is still non-source — should not suggest
        let input = HookInput {
            tool_name: Some("Read".to_string()),
            tool_input: Some(HookToolInput {
                file_path: Some("data/export.csv".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let s = pre_tool_suggestion(&input);
        assert!(s.is_empty(), "should not suggest for .csv files: {s}");
    }

    #[test]
    fn test_pre_tool_suggestion_glob_suggests_search_files() {
        let input = HookInput {
            tool_name: Some("Glob".to_string()),
            ..Default::default()
        };
        let s = pre_tool_suggestion(&input);
        assert!(
            s.contains("search_files"),
            "should suggest search_files: {s}"
        );
    }

    #[test]
    fn test_pre_tool_suggestion_edit_suggests_replace_symbol_body() {
        let input = HookInput {
            tool_name: Some("Edit".to_string()),
            ..Default::default()
        };
        let s = pre_tool_suggestion(&input);
        assert!(
            s.contains("replace_symbol_body"),
            "should suggest replace_symbol_body: {s}"
        );
    }

    #[test]
    fn test_pre_tool_workflow_classifies_source_read() {
        let input = make_input("Read", Some("/repo/src/lib.rs"), None, "/repo");
        assert_eq!(pre_tool_workflow(&input), HookWorkflow::SourceRead);
    }

    #[test]
    fn test_pre_tool_workflow_leaves_non_source_read_as_passthrough() {
        let input = make_input("Read", Some("/repo/docs/guide.md"), None, "/repo");
        assert_eq!(pre_tool_workflow(&input), HookWorkflow::PassThrough);
    }

    #[test]
    fn test_workflow_for_subcommand_leaves_non_source_read_as_passthrough() {
        let input = make_input("Read", Some("/repo/docs/guide.md"), None, "/repo");
        assert_eq!(
            workflow_for_subcommand(Some(&HookSubcommand::Read), &input),
            HookWorkflow::PassThrough
        );
    }

    #[test]
    fn test_workflow_for_subcommand_leaves_config_read_as_passthrough() {
        let input = make_input("Read", Some("/repo/Cargo.toml"), None, "/repo");
        assert_eq!(
            workflow_for_subcommand(Some(&HookSubcommand::Read), &input),
            HookWorkflow::PassThrough
        );
    }

    #[test]
    fn test_workflow_for_subcommand_classifies_repo_start() {
        let input = HookInput::default();
        assert_eq!(
            workflow_for_subcommand(Some(&HookSubcommand::SessionStart), &input),
            HookWorkflow::RepoStart
        );
    }

    #[test]
    fn test_workflow_for_subcommand_classifies_prompt_context() {
        let input = HookInput {
            prompt: Some("read src/lib.rs".to_string()),
            ..HookInput::default()
        };
        assert_eq!(
            workflow_for_subcommand(Some(&HookSubcommand::PromptSubmit), &input),
            HookWorkflow::PromptContext
        );
    }

    #[test]
    fn test_pre_tool_suggestion_unknown_tool_is_empty() {
        let input = HookInput {
            tool_name: Some("Bash".to_string()),
            ..Default::default()
        };
        let s = pre_tool_suggestion(&input);
        assert!(s.is_empty(), "should not suggest for unknown tools: {s}");
    }

    #[test]
    fn test_is_non_source_path_allows_config_files() {
        // These are not treated as broad non-source paths; literal-read routing
        // is decided separately by `should_fail_open_read`.
        assert!(!is_non_source_path("package.json"));
        assert!(!is_non_source_path("Cargo.toml"));
        assert!(!is_non_source_path("README.md"));
        assert!(!is_non_source_path(".env"));
        assert!(!is_non_source_path("config.yaml"));
        assert!(!is_non_source_path("docker-compose.yml"));
    }

    #[test]
    fn test_is_non_source_path_still_skips_non_config() {
        assert!(is_non_source_path("data.csv"));
        assert!(is_non_source_path("notes.txt"));
        assert!(is_non_source_path("icon.png"));
        assert!(is_non_source_path("Cargo.lock"));
    }

    #[test]
    fn test_is_non_source_path_allows_source_files() {
        assert!(!is_non_source_path("src/main.rs"));
        assert!(!is_non_source_path("tests/test_foo.py"));
        assert!(!is_non_source_path("lib/parser.js"));
    }

    #[test]
    fn test_load_hook_adoption_snapshot_filters_to_current_session() {
        let tmp = TempDir::new().unwrap();
        let symforge_dir = tmp.path().join(".symforge");
        std::fs::create_dir_all(&symforge_dir).unwrap();
        let log_path = symforge_dir.join("hook-adoption.log");
        let session_path = symforge_dir.join("sidecar.session");

        append_hook_adoption_event(&log_path, Some("session-a"), "source-read", "routed").unwrap();
        append_hook_adoption_event(&log_path, Some("session-a"), "repo-start", "no-sidecar")
            .unwrap();
        append_hook_adoption_event(&log_path, Some("session-b"), "source-search", "routed")
            .unwrap();
        std::fs::write(&session_path, "session-a\n").unwrap();

        let snapshot = load_hook_adoption_snapshot(Some(tmp.path()));
        assert_eq!(snapshot.source_read.routed, 1);
        assert_eq!(snapshot.source_search.routed, 0);
        assert_eq!(snapshot.repo_start.no_sidecar, 1);
        assert_eq!(snapshot.first_repo_start, Some(HookOutcome::NoSidecar));
    }

    #[test]
    fn test_load_hook_adoption_snapshot_tracks_sidecar_errors_and_totals() {
        let tmp = TempDir::new().unwrap();
        let log_path = tmp.path().join("hook-adoption.log");

        append_hook_adoption_event(
            &log_path,
            Some("session-z"),
            "prompt-context",
            "sidecar-error",
        )
        .unwrap();
        append_hook_adoption_event(&log_path, Some("session-z"), "post-edit-impact", "routed")
            .unwrap();
        append_hook_adoption_event(&log_path, Some("session-z"), "source-read", "no-sidecar")
            .unwrap();

        let snapshot = load_hook_adoption_snapshot_from_path(&log_path, Some("session-z")).unwrap();
        assert_eq!(snapshot.prompt_context.sidecar_error, 1);
        assert_eq!(snapshot.post_edit_impact.routed, 1);
        assert_eq!(snapshot.source_read.no_sidecar, 1);
        assert_eq!(snapshot.total_routed(), 1);
        assert_eq!(snapshot.total_fail_open(), 2);
        assert_eq!(snapshot.total_attempts(), 3);
    }

    // ---- Hook-adoption metric regression tests ----
    //
    // These pin the user-visible contract documented in CONTEXT.md:
    // `health` output must render `Owned workflows routed: N/M (P%)` after
    // hooks fire, and must visibly degrade (or disappear) when they don't.
    //
    // Chain under test: record_hook_outcome → ADOPTION_LOG_FILE on disk →
    // load_hook_adoption_snapshot → format_hook_adoption. A regression at
    // any link drops the "2/2 (100%)" contract, and these tests fail loudly.
    //
    // Not covered here: whether `run_hook` still calls record_hook_outcome
    // at its dispatch sites. That wire-up is guarded by code review — see
    // src/cli/hook.rs::run_hook lines 307/350/378.

    /// Serializes cwd mutation inside this test binary. Tests run with
    /// --test-threads=1 (per CLAUDE.md), so this lock only guards against
    /// future intra-binary concurrency regressions.
    static HOOK_CWD_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

    #[test]
    fn test_health_hook_adoption_metric_pins_published_contract() {
        let tmp = TempDir::new().unwrap();
        let symforge_dir = tmp.path().join(".symforge");
        std::fs::create_dir_all(&symforge_dir).unwrap();
        let log_path = symforge_dir.join("hook-adoption.log");
        let session_path = symforge_dir.join("sidecar.session");

        // Two tracked workflows, both routed — mirrors the "2/2 (100%)"
        // contract shown in CONTEXT.md §Project rules.
        append_hook_adoption_event(&log_path, Some("sess-live"), "repo-start", "routed").unwrap();
        append_hook_adoption_event(&log_path, Some("sess-live"), "prompt-context", "routed")
            .unwrap();
        std::fs::write(&session_path, "sess-live\n").unwrap();

        // adoption_log_path and load_hook_adoption_snapshot must agree on
        // where the log lives — pin that too.
        assert_eq!(adoption_log_path(Some(tmp.path())), log_path);

        let snapshot = load_hook_adoption_snapshot(Some(tmp.path()));
        assert_eq!(snapshot.total_routed(), 2);
        assert_eq!(snapshot.total_attempts(), 2);
        assert_eq!(snapshot.total_fail_open(), 0);

        let rendered = crate::protocol::format::format_hook_adoption(&snapshot);
        assert!(
            rendered.contains("── Hook Adoption (current session) ──"),
            "missing section header: {rendered}"
        );
        assert!(
            rendered.contains("Owned workflows routed: 2/2 (100%)"),
            "published contract string missing: {rendered}"
        );
        assert!(
            rendered.contains("Fail-open outcomes: 0"),
            "should show zero fail-open when all routed: {rendered}"
        );
        assert!(
            rendered.contains("Repo start: routed 1"),
            "missing per-workflow line for repo-start: {rendered}"
        );
        assert!(
            rendered.contains("Prompt context: routed 1"),
            "missing per-workflow line for prompt-context: {rendered}"
        );
        assert!(
            rendered.contains("First repo start: routed"),
            "first-repo-start outcome must render: {rendered}"
        );
    }

    #[test]
    fn test_health_hook_adoption_metric_flags_silent_failure_when_all_fail_open() {
        // Regression guard for the scenario CONTEXT.md warns about:
        // "a regression where hooks silently stop firing would drop this to
        // 0/2 or 1/2 and nothing automated would notice".
        let tmp = TempDir::new().unwrap();
        let symforge_dir = tmp.path().join(".symforge");
        std::fs::create_dir_all(&symforge_dir).unwrap();
        let log_path = symforge_dir.join("hook-adoption.log");
        let session_path = symforge_dir.join("sidecar.session");

        append_hook_adoption_event(&log_path, Some("sess-down"), "source-read", "no-sidecar")
            .unwrap();
        append_hook_adoption_event(&log_path, Some("sess-down"), "prompt-context", "no-sidecar")
            .unwrap();
        std::fs::write(&session_path, "sess-down\n").unwrap();

        let snapshot = load_hook_adoption_snapshot(Some(tmp.path()));
        let rendered = crate::protocol::format::format_hook_adoption(&snapshot);

        assert!(
            rendered.contains("Owned workflows routed: 0/2 (0%)"),
            "degraded metric must visibly read 0/2, not be absent: {rendered}"
        );
        assert!(
            rendered.contains("Fail-open outcomes: 2 (no sidecar 2"),
            "fail-open breakdown must surface the real cause: {rendered}"
        );
        assert!(
            rendered.contains("⚠ All hook attempts failed open"),
            "user-facing warning must render when no workflow routed: {rendered}"
        );
    }

    #[test]
    fn test_record_hook_outcome_writes_to_adoption_log_file_constant() {
        // Pins the wire-up between record_hook_outcome and the
        // ADOPTION_LOG_FILE path constant. A rename of either — or a
        // rewrite of record_hook_outcome that stops calling
        // append_hook_adoption_event — trips this test.
        let _guard = HOOK_CWD_LOCK.lock().unwrap();
        let tmp = TempDir::new().unwrap();
        let original = std::env::current_dir().expect("cwd readable");
        std::env::set_current_dir(tmp.path()).expect("cwd settable to tempdir");

        // Use a catch_unwind-style restore so a failing assertion doesn't
        // strand cwd in the tempdir for subsequent tests.
        let result = std::panic::catch_unwind(|| {
            record_hook_outcome(
                HookWorkflow::SourceRead,
                HookOutcome::Routed,
                Some("sess-wireup"),
            );

            let log_path = tmp.path().join(ADOPTION_LOG_FILE);
            assert!(
                log_path.exists(),
                "record_hook_outcome must create {ADOPTION_LOG_FILE} under cwd; \
                 missing at {}",
                log_path.display()
            );
            let contents = std::fs::read_to_string(&log_path).expect("log readable");
            assert!(
                contents.contains("sess-wireup\tsource-read\trouted"),
                "log must contain the tab-separated routed event; got: {contents:?}"
            );
        });

        std::env::set_current_dir(&original).expect("cwd restorable");
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }

    // ---- HOOK-02: is_hook_verbose ----

    #[allow(unsafe_code)] // test-only env mutation is serialized by HOOK_VERBOSE_ENV_LOCK.
    #[test]
    fn hook_verbose_returns_false_when_unset() {
        let _guard = HOOK_VERBOSE_ENV_LOCK.lock().unwrap();
        // SAFETY: test-only env manipulation; tests run with --test-threads=1.
        unsafe { std::env::remove_var("SYMFORGE_HOOK_VERBOSE") };
        assert!(!is_hook_verbose());
    }

    #[allow(unsafe_code)] // test-only env mutation is serialized by HOOK_VERBOSE_ENV_LOCK.
    #[test]
    fn hook_verbose_returns_true_when_set_to_1() {
        let _guard = HOOK_VERBOSE_ENV_LOCK.lock().unwrap();
        // SAFETY: test-only env manipulation; tests run with --test-threads=1.
        unsafe { std::env::set_var("SYMFORGE_HOOK_VERBOSE", "1") };
        let result = is_hook_verbose();
        unsafe { std::env::remove_var("SYMFORGE_HOOK_VERBOSE") };
        assert!(result);
    }

    #[allow(unsafe_code)] // test-only env mutation is serialized by HOOK_VERBOSE_ENV_LOCK.
    #[test]
    fn hook_verbose_returns_false_for_other_values() {
        let _guard = HOOK_VERBOSE_ENV_LOCK.lock().unwrap();
        for val in &["0", "true", "yes", "2", ""] {
            // SAFETY: test-only env manipulation; tests run with --test-threads=1.
            unsafe { std::env::set_var("SYMFORGE_HOOK_VERBOSE", val) };
            assert!(
                !is_hook_verbose(),
                "should be false for SYMFORGE_HOOK_VERBOSE={val}"
            );
        }
        unsafe { std::env::remove_var("SYMFORGE_HOOK_VERBOSE") };
    }

    // ---- HOOK-01: adoption log detail fields ----

    #[test]
    fn adoption_log_missing_port_includes_reason_and_project_root() {
        let tmp = TempDir::new().unwrap();
        let log_path = tmp.path().join("hook-adoption.log");
        let detail = NoSidecarDetail {
            reason: "sidecar_port_missing",
            searched_path: "/repo/.symforge/sidecar.port",
            suggestion: "start_mcp_session",
            project_root: "/repo",
        };
        append_hook_adoption_event_with_detail(
            &log_path,
            Some("sess-1"),
            "source-read",
            "no-sidecar",
            Some(detail),
        )
        .unwrap();
        let content = std::fs::read_to_string(&log_path).unwrap();
        assert!(
            content.contains("reason=sidecar_port_missing"),
            "missing reason field"
        );
        assert!(
            content.contains("project_root=/repo"),
            "missing project_root field"
        );
        assert!(content.contains("searched_path=/repo/.symforge/sidecar.port"));
        assert!(content.contains("suggestion=start_mcp_session"));
    }

    #[test]
    fn adoption_log_stale_port_has_distinct_reason() {
        let tmp = TempDir::new().unwrap();
        let log_path = tmp.path().join("hook-adoption.log");
        let detail = NoSidecarDetail {
            reason: "sidecar_port_stale",
            searched_path: "/repo/.symforge/sidecar.port",
            suggestion: "restart_sidecar",
            project_root: "/repo",
        };
        append_hook_adoption_event_with_detail(
            &log_path,
            Some("sess-2"),
            "source-read",
            "no-sidecar",
            Some(detail),
        )
        .unwrap();
        let content = std::fs::read_to_string(&log_path).unwrap();
        assert!(
            content.contains("reason=sidecar_port_stale"),
            "should have stale reason"
        );
        assert!(content.contains("project_root=/repo"));
    }

    #[test]
    fn adoption_log_without_detail_has_no_reason_or_project_root() {
        let tmp = TempDir::new().unwrap();
        let log_path = tmp.path().join("hook-adoption.log");
        append_hook_adoption_event_with_detail(
            &log_path,
            Some("sess-3"),
            "source-read",
            "routed",
            None,
        )
        .unwrap();
        let content = std::fs::read_to_string(&log_path).unwrap();
        assert!(
            !content.contains("reason="),
            "should have no reason without detail"
        );
        assert!(
            !content.contains("project_root="),
            "should have no project_root without detail"
        );
    }

    // ---- HOOK-03: one-time sidecar hint ----

    #[test]
    fn sidecar_hint_creates_marker_file() {
        let tmp = TempDir::new().unwrap();
        let marker = tmp.path().join(HOOK_HINT_MARKER);
        assert!(!marker.exists());
        maybe_emit_sidecar_hint(tmp.path());
        assert!(marker.exists(), "marker file should be created");
    }

    #[test]
    fn sidecar_hint_skips_when_marker_fresh() {
        let tmp = TempDir::new().unwrap();
        let marker = tmp.path().join(HOOK_HINT_MARKER);
        std::fs::create_dir_all(marker.parent().unwrap()).unwrap();
        std::fs::write(&marker, "").unwrap();
        // Marker was just created — should be fresh.
        // We can't easily capture stderr in a unit test, but we can verify
        // the marker file's mtime is NOT updated (proving the function returned early).
        let mtime_before = std::fs::metadata(&marker).unwrap().modified().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(50));
        maybe_emit_sidecar_hint(tmp.path());
        let mtime_after = std::fs::metadata(&marker).unwrap().modified().unwrap();
        assert_eq!(
            mtime_before, mtime_after,
            "marker mtime should not change when fresh"
        );
    }

    // --- helpers ---

    fn make_input(
        tool_name: &str,
        file_path: Option<&str>,
        pattern: Option<&str>,
        cwd: &str,
    ) -> HookInput {
        HookInput {
            tool_name: Some(tool_name.to_string()),
            tool_input: Some(HookToolInput {
                file_path: file_path.map(|s| s.to_string()),
                pattern: pattern.map(|s| s.to_string()),
                path: None,
            }),
            cwd: Some(cwd.to_string()),
            prompt: None,
        }
    }
}
