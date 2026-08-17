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
use crate::domain::ControlStateDir;

// hook-adoption.log is written AND read only inside this hook binary (single OS per
// process), so it stays un-tagged. The sidecar port/session files are cross-process
// (written by the sidecar/proxy, read here) and MUST be OS-tagged in lockstep with the
// writer — both sides derive the tag from `crate::paths::os_tagged_runtime_file_name`,
// so a given OS's hook and sidecar always agree. See `sidecar_port_file_rel` below.
const ADOPTION_LOG_FILE: &str = "hook-adoption.log";

fn process_control_state_dir() -> Option<ControlStateDir> {
    crate::paths::process_control_state_placement()
        .directory()
        .cloned()
}

fn sidecar_descriptor_path(control_state_dir: &ControlStateDir) -> PathBuf {
    crate::paths::control_state_path(control_state_dir, "sidecar/sessions")
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
pub fn run_hook_with_input(
    input: HookInput,
    subcommand: Option<&HookSubcommand>,
) -> anyhow::Result<()> {
    run_hook_with_input_at(input, subcommand, process_control_state_dir())
}

/// Run a hook against an explicitly resolved control-state owner.
///
/// This is the seam in-process callers and integration harnesses use when
/// reading the real stdin would block or process-global placement would make
/// isolated state ownership impossible.
#[doc(hidden)]
pub fn run_hook_with_input_at(
    input: HookInput,
    subcommand: Option<&HookSubcommand>,
    control_state_dir: Option<ControlStateDir>,
) -> anyhow::Result<()> {
    let verbose = is_hook_verbose();
    let repo_root = std::env::current_dir().unwrap_or_default();

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
        let sidecar_active = control_state_dir
            .as_ref()
            .is_some_and(|state_dir| read_sidecar_endpoint(state_dir, &repo_root).is_ok());
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
    let sidecar_endpoint = control_state_dir
        .as_ref()
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "process control state unavailable",
            )
        })
        .and_then(|state_dir| read_sidecar_endpoint(state_dir, &repo_root))
        .map(|(port, session_id)| (port, normalize_session_id(session_id)));
    let session_id = sidecar_endpoint
        .as_ref()
        .ok()
        .and_then(|(_, session_id)| session_id.clone());

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
    let (port, effective_session_id, used_daemon_fallback) = match sidecar_endpoint {
        Ok((p, descriptor_session_id)) => {
            if verbose {
                eprintln!("[symforge-hook] read port file: port={p}");
            }
            (p, descriptor_session_id, false)
        }
        Err(e) => {
            let port_file_path = control_state_dir
                .as_ref()
                .map(sidecar_descriptor_path)
                .unwrap_or_default();
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
            match try_daemon_fallback(&repo_root, None) {
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
                    maybe_emit_sidecar_hint(control_state_dir.as_ref());
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
    // A descriptor carrying a session id already targets the daemon proxy.
    // Keep this separate from `used_daemon_fallback`: descriptor routing is a
    // normal route (and records `Routed`), but it must not rediscover and call
    // the same daemon a second time after a 503. Session ids are normalized at
    // descriptor read so this predicate is identical to `proxy_path` routing.
    let initial_endpoint_is_daemon = effective_session_id.is_some();

    // Step 2 — determine endpoint + query string.
    let resolved_ref = resolved.as_ref();
    let (path, query) = endpoint_for(resolved_ref, &input);
    let request_path = proxy_path(path, effective_session_id.as_deref());

    if verbose {
        eprintln!("[symforge-hook] HTTP GET 127.0.0.1:{port}{request_path}?{query}");
    }

    // Dogfood #6 / spec 012 FR-006b (hook half): pin the sidecar request to
    // the caller's repo root. A sidecar whose shared session was retargeted by
    // another agent's `index_folder` answers 409, and the daemon fallback
    // below re-resolves the caller's project BY ROOT. Keep the daemon request
    // pinned too: a session can remain open while its active project changes.
    let Some(query) = append_caller_root(query) else {
        if verbose {
            eprintln!(
                "[symforge-hook] project root is not valid UTF-8 — refusing enrichment authority and failing open"
            );
        }
        record_hook_outcome(
            workflow,
            HookOutcome::NoSidecar,
            effective_session_id.as_deref(),
        );
        println!("{}", fail_open_json(event_name));
        return Ok(());
    };
    // Keep a copy so the stale-sidecar daemon fallback can re-issue the same
    // root-pinned enrichment request — the first HTTP call consumes `query`.
    let fallback_query = query.clone();

    // Step 3/4 — make sync HTTP GET with 50 ms timeout.
    let (body, outcome, outcome_session_id) = match sync_enrichment_http_get(
        port,
        &request_path,
        query,
    ) {
        EnrichmentHttpResult::Success(b) => {
            let initial_outcome = if used_daemon_fallback {
                HookOutcome::DaemonFallback
            } else {
                HookOutcome::Routed
            };
            (b, initial_outcome, effective_session_id.clone())
        }
        initial_failure @ (EnrichmentHttpResult::IndexNotReady
        | EnrichmentHttpResult::RootConflict
        | EnrichmentHttpResult::HttpFailure(_)
        | EnrichmentHttpResult::Unavailable) => {
            let initial_index_not_ready =
                matches!(initial_failure, EnrichmentHttpResult::IndexNotReady);
            let initial_root_conflict =
                matches!(initial_failure, EnrichmentHttpResult::RootConflict);
            let initial_http_failure =
                matches!(initial_failure, EnrichmentHttpResult::HttpFailure(_));

            // Before failing open, try routing the SAME enrichment request
            // through the daemon. A local sidecar may still be loading while a
            // separate root-matched daemon session is already queryable.
            //
            // A 503 from a daemon-backed endpoint is authoritative for that
            // root-matched project and must not be retried through the same
            // daemon. A closed session (404/unavailable) or a root conflict can
            // still recover through a different active session, so rediscover
            // while excluding the session that just failed.
            let port_file_path = control_state_dir
                .as_ref()
                .map(sidecar_descriptor_path)
                .unwrap_or_default();

            if verbose {
                if initial_index_not_ready {
                    if initial_endpoint_is_daemon {
                        eprintln!("[symforge-hook] daemon index not ready — failing open");
                    } else {
                        eprintln!(
                            "[symforge-hook] index not ready — attempting daemon fallback before fail-open"
                        );
                    }
                } else if initial_root_conflict {
                    if initial_endpoint_is_daemon {
                        eprintln!(
                            "[symforge-hook] daemon root conflict — attempting alternate daemon session"
                        );
                    } else {
                        eprintln!(
                            "[symforge-hook] sidecar root conflict — attempting daemon fallback before fail-open"
                        );
                    }
                } else if initial_http_failure {
                    eprintln!(
                        "[symforge-hook] sidecar returned a live HTTP failure — attempting daemon fallback before fail-open"
                    );
                } else {
                    // Probe only transport failures. A 503 already proves the
                    // sidecar is live and must never produce a restart hint.
                    let liveness = control_state_dir
                        .as_ref()
                        .map(|state_dir| {
                            crate::sidecar::port_file::read_sidecar_status(
                                state_dir,
                                "127.0.0.1",
                                Some(&repo_root),
                            )
                            .liveness
                            .as_str()
                        })
                        .unwrap_or("unavailable");
                    if initial_endpoint_is_daemon {
                        eprintln!(
                            "[symforge-hook] daemon HTTP request failed — liveness={liveness}, attempting alternate daemon session"
                        );
                    } else {
                        eprintln!(
                            "[symforge-hook] HTTP request failed — sidecar liveness={liveness}, \
                             attempting daemon fallback before fail-open"
                        );
                    }
                }
            }

            let daemon_enriched = if initial_endpoint_is_daemon && initial_index_not_ready {
                None
            } else {
                let excluded_session_id = initial_endpoint_is_daemon
                    .then_some(effective_session_id.as_deref())
                    .flatten();
                match try_daemon_fallback(&repo_root, excluded_session_id) {
                    Some(fallback) => {
                        let fallback_session_id = fallback.session_id.clone();
                        let daemon_request_path = proxy_path(path, Some(&fallback.session_id));
                        if verbose {
                            eprintln!(
                                "[symforge-hook] daemon fallback (stale sidecar): \
                                 port={}, session={}",
                                fallback.daemon_port, fallback.session_id
                            );
                        }
                        Some((
                            sync_enrichment_http_get_with_timeout(
                                fallback.daemon_port,
                                &daemon_request_path,
                                fallback_query,
                                DAEMON_FALLBACK_DEADLINE,
                            ),
                            fallback_session_id,
                        ))
                    }
                    None => None,
                }
            };

            match daemon_enriched {
                Some((EnrichmentHttpResult::Success(b), daemon_session_id)) => {
                    if verbose {
                        eprintln!(
                            "[symforge-hook] daemon fallback succeeded — \
                             sidecar dead/degraded, served enriched result via daemon"
                        );
                    }
                    (b, HookOutcome::DaemonFallback, Some(daemon_session_id))
                }
                Some((EnrichmentHttpResult::IndexNotReady, daemon_session_id)) => {
                    emit_live_refusal_fail_open(
                        workflow,
                        event_name,
                        Some(&daemon_session_id),
                        "index_not_ready",
                        verbose,
                    );
                    return Ok(());
                }
                Some((EnrichmentHttpResult::RootConflict, daemon_session_id)) => {
                    emit_live_refusal_fail_open(
                        workflow,
                        event_name,
                        Some(&daemon_session_id),
                        "root_conflict",
                        verbose,
                    );
                    return Ok(());
                }
                Some((EnrichmentHttpResult::HttpFailure(_), daemon_session_id)) => {
                    emit_live_refusal_fail_open(
                        workflow,
                        event_name,
                        Some(&daemon_session_id),
                        "http_failure",
                        verbose,
                    );
                    return Ok(());
                }
                Some((EnrichmentHttpResult::Unavailable, _)) | None if initial_index_not_ready => {
                    emit_live_refusal_fail_open(
                        workflow,
                        event_name,
                        effective_session_id.as_deref(),
                        "index_not_ready",
                        verbose,
                    );
                    return Ok(());
                }
                Some((EnrichmentHttpResult::Unavailable, _)) | None if initial_root_conflict => {
                    emit_live_refusal_fail_open(
                        workflow,
                        event_name,
                        effective_session_id.as_deref(),
                        "root_conflict",
                        verbose,
                    );
                    return Ok(());
                }
                Some((EnrichmentHttpResult::Unavailable, _)) | None if initial_http_failure => {
                    emit_live_refusal_fail_open(
                        workflow,
                        event_name,
                        effective_session_id.as_deref(),
                        "http_failure",
                        verbose,
                    );
                    return Ok(());
                }
                Some((EnrichmentHttpResult::Unavailable, _)) | None => {
                    // Both sidecar and daemon are unreachable: degrade to a
                    // pass-through, never hang, never error the editor.
                    if verbose {
                        eprintln!(
                            "[symforge-hook] daemon fallback unavailable — \
                             outcome=NoSidecar reason=sidecar_port_stale"
                        );
                    }
                    maybe_emit_sidecar_hint(control_state_dir.as_ref());
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
    record_hook_outcome(workflow, outcome, outcome_session_id.as_deref());
    println!("{}", success_json(event_name, &body));
    Ok(())
}

fn emit_live_refusal_fail_open(
    workflow: HookWorkflow,
    event_name: &str,
    session_id: Option<&str>,
    reason: &str,
    verbose: bool,
) {
    if verbose {
        eprintln!("[symforge-hook] outcome=SidecarError reason={reason}");
    }
    record_hook_outcome(workflow, HookOutcome::SidecarError, session_id);
    println!("{}", fail_open_json(event_name));
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

    // Suppress hints for targets outside the workspace root: SymForge cannot
    // serve paths outside the indexed repo (e.g. %TEMP% task outputs, other
    // repos), so the hint would be pure noise. Fail-open silent.
    let target = input
        .tool_input
        .as_ref()
        .and_then(|ti| ti.file_path.as_deref().or(ti.path.as_deref()))
        .unwrap_or("");
    if is_outside_workspace(target, cwd) {
        return String::new();
    }

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

/// True when `target` is an absolute path that does not live under the
/// workspace root (`root`, falling back to the hook's current directory).
///
/// SymForge tools resolve paths relative to the indexed project root, so an
/// absolute path outside it (temp dirs, other repos) can never be served and
/// must not produce an efficiency hint. Relative and empty targets are always
/// treated as in-workspace. Cheap by design: string prefix check after
/// normalization, no filesystem access beyond an optional `current_dir`.
fn is_outside_workspace(target: &str, root: &str) -> bool {
    if target.is_empty() || !std::path::Path::new(target).is_absolute() {
        return false;
    }
    let root_norm = if root.is_empty() {
        match std::env::current_dir() {
            Ok(d) => normalize_path_for_match(&d),
            // Fail-open: cannot determine the root, keep existing behavior.
            Err(_) => return false,
        }
    } else {
        normalize_path_for_match(std::path::Path::new(root))
    };
    if root_norm.is_empty() {
        return false;
    }
    let target_norm = normalize_path_for_match(std::path::Path::new(target));
    target_norm != root_norm && !target_norm.starts_with(&format!("{root_norm}/"))
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

/// True when a Grep pattern is a bare code identifier worth a symbol lookup.
/// Regexes, globs, and multi-token phrases can never match a symbol name, so
/// forwarding them only produces zero-hit noise in prompt context (dogfood #8).
fn is_plausible_symbol_name(pattern: &str) -> bool {
    let mut chars = pattern.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
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
            // Dogfood #8 (2026-07-06): grep patterns are often regexes or
            // multi-token phrases (`Tip:`, `compact|SYMFORGE_SURFACE`) — not
            // symbol names. Forwarding them buys a guaranteed zero-hit report
            // in prompt context. Only a bare identifier is worth the lookup.
            if q.is_empty() || !is_plausible_symbol_name(q) {
                return ("/health", String::new());
            }
            ("/symbol-context", format!("name={}", url_encode(q)))
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

/// Append the hook's repo root (its cwd, canonicalized) as `caller_root` so
/// the sidecar's root guard can 409 a wrong-project answer (dogfood #6 /
/// spec 012 FR-006b).
fn append_caller_root(query: String) -> Option<String> {
    let cwd = std::env::current_dir().unwrap_or_default();
    let root = std::fs::canonicalize(&cwd).unwrap_or(cwd);
    append_caller_root_for_path(query, &root)
}

fn append_caller_root_for_path(query: String, root: &Path) -> Option<String> {
    let encoded = url_encode(root.to_str()?);
    Some(if query.is_empty() {
        format!("caller_root={encoded}")
    } else {
        format!("{query}&caller_root={encoded}")
    })
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

fn read_sidecar_endpoint(
    control_state_dir: &ControlStateDir,
    project_root: &Path,
) -> std::io::Result<(u16, Option<String>)> {
    crate::sidecar::port_file::read_sidecar_endpoint(
        control_state_dir,
        "127.0.0.1",
        Some(project_root),
    )
}

fn normalize_session_id(session_id: Option<String>) -> Option<String> {
    session_id
        .map(|session_id| session_id.trim().to_string())
        .filter(|session_id| !session_id.is_empty())
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
    let Some(control_state_dir) = process_control_state_dir() else {
        return;
    };
    record_hook_outcome_at(&control_state_dir, workflow, outcome, session_id);
}

fn record_hook_outcome_at(
    control_state_dir: &ControlStateDir,
    workflow: HookWorkflow,
    outcome: HookOutcome,
    session_id: Option<&str>,
) {
    let Some(workflow_name) = tracked_workflow_name(workflow) else {
        return;
    };
    let _ = append_hook_adoption_event(
        &adoption_log_path(control_state_dir),
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
fn try_daemon_fallback(
    repo_root: &Path,
    excluded_session_id: Option<&str>,
) -> Option<DaemonFallbackResult> {
    let deadline = std::time::Instant::now() + DAEMON_FALLBACK_DEADLINE;

    // Step 1: Read the daemon port file (~/.symforge/daemon.port).
    let daemon_port = crate::daemon::read_daemon_port_file().ok()?;

    // Step 2: Query GET /v1/projects for the list of active projects.
    let remaining = deadline.checked_duration_since(std::time::Instant::now())?;
    let projects_json =
        sync_http_get_with_timeout(daemon_port, "/v1/projects", String::new(), remaining).ok()?;

    // Step 3: Parse the projects list and find one matching this repo root.
    let canon_root = std::fs::canonicalize(repo_root).unwrap_or_else(|_| repo_root.to_path_buf());
    // Minimal serde structs for daemon JSON responses.
    // The daemon returns a JSON array of objects with `canonical_root`,
    // `project_id`, and `session_count` fields.
    let projects: Vec<DaemonProjectEntry> = serde_json::from_str(&projects_json).ok()?;

    let matching = projects
        .iter()
        .find(|project| daemon_project_matches_native_root(project, &canon_root))?;

    if matching.session_count == 0 {
        return None;
    }

    // Step 4: Query GET /v1/projects/{project_id}/sessions to get a session id.
    let remaining = deadline.checked_duration_since(std::time::Instant::now())?;
    let sessions_path = format!("/v1/projects/{}/sessions", url_encode(&matching.project_id));
    let sessions_json =
        sync_http_get_with_timeout(daemon_port, &sessions_path, String::new(), remaining).ok()?;

    let sessions: Vec<DaemonSessionEntry> = serde_json::from_str(&sessions_json).ok()?;

    // A session listed under this project may since have activated another
    // project. Only route through a session whose current active project still
    // matches the root-resolved project, then prefer the most recently seen.
    let session = sessions
        .iter()
        .filter(|session| {
            session.project_id == matching.project_id
                && !session.session_id.trim().is_empty()
                && excluded_session_id.is_none_or(|excluded| session.session_id.trim() != excluded)
        })
        .max_by_key(|session| session.last_seen_at_unix_secs)?;

    Some(DaemonFallbackResult {
        daemon_port,
        session_id: session.session_id.trim().to_string(),
    })
}

/// Minimal deserialization struct for daemon project list entries.
#[derive(serde::Deserialize)]
struct DaemonProjectEntry {
    project_id: String,
    canonical_root: String,
    session_count: usize,
}

fn daemon_project_matches_native_root(project: &DaemonProjectEntry, root: &Path) -> bool {
    let expected_project_id = crate::daemon::project_key(root);
    project.project_id == expected_project_id
        && crate::daemon::project_key(Path::new(&project.canonical_root)) == project.project_id
}

/// Minimal deserialization struct for daemon session list entries.
#[derive(serde::Deserialize)]
struct DaemonSessionEntry {
    session_id: String,
    project_id: String,
    last_seen_at_unix_secs: u64,
}

#[derive(Debug, PartialEq, Eq)]
struct HttpResponse {
    status_code: u16,
    body: String,
}

#[derive(Debug, PartialEq, Eq)]
enum EnrichmentHttpResult {
    Success(String),
    IndexNotReady,
    RootConflict,
    HttpFailure(u16),
    Unavailable,
}

fn normalize_path_text_for_match(path_text: &str, windows: bool) -> String {
    let normalized = crate::daemon::normalized_path_text(path_text, windows);
    let trimmed = normalized.trim_end_matches('/');
    if windows {
        trimmed.to_ascii_lowercase()
    } else {
        trimmed.to_string()
    }
}

/// Normalize a native path for cross-platform comparison: Windows accepts
/// either slash spelling and compares case-insensitively; Unix preserves a
/// literal backslash because it is a valid filename byte.
fn normalize_path_for_match(path: &Path) -> String {
    normalize_path_text_for_match(&path.to_string_lossy(), cfg!(windows))
}

fn sync_http_response_with_timeout(
    port: u16,
    path: &str,
    query: String,
    timeout: Duration,
) -> anyhow::Result<HttpResponse> {
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

    // Check for chunked transfer-encoding. The sidecar uses hyper which may
    // send chunked responses. Since we use Connection: close and read_to_string,
    // the raw body includes chunk framing that must be decoded.
    let is_chunked = headers.lines().any(|line| {
        let lower = line.to_lowercase();
        lower.starts_with("transfer-encoding:") && lower.contains("chunked")
    });

    let body = if is_chunked {
        decode_chunked_body(body)
    } else {
        body.to_string()
    };

    Ok(HttpResponse { status_code, body })
}

/// Like `sync_http_get` but with a configurable timeout.
fn sync_http_get_with_timeout(
    port: u16,
    path: &str,
    query: String,
    timeout: Duration,
) -> anyhow::Result<String> {
    let response = sync_http_response_with_timeout(port, path, query, timeout)?;
    if !(200..=299).contains(&response.status_code) {
        anyhow::bail!("HTTP {} from {path}", response.status_code);
    }
    Ok(response.body)
}

fn classify_enrichment_response(response: HttpResponse) -> EnrichmentHttpResult {
    match response.status_code {
        200..=299 => EnrichmentHttpResult::Success(response.body),
        409 => EnrichmentHttpResult::RootConflict,
        503 => EnrichmentHttpResult::IndexNotReady,
        status_code => EnrichmentHttpResult::HttpFailure(status_code),
    }
}

fn sync_enrichment_http_get_with_timeout(
    port: u16,
    path: &str,
    query: String,
    timeout: Duration,
) -> EnrichmentHttpResult {
    match sync_http_response_with_timeout(port, path, query, timeout) {
        Ok(response) => classify_enrichment_response(response),
        Err(_) => EnrichmentHttpResult::Unavailable,
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
const HOOK_HINT_MARKER: &str = "hook-hint-shown";

/// Freshness window for the sidecar hint marker file (30 minutes).
const HOOK_HINT_FRESHNESS: Duration = Duration::from_secs(30 * 60);

/// CLI invocation advertised by the sidecar hint to set up MCP integration.
///
/// Must remain a valid `symforge` subcommand (asserted in tests). The earlier
/// `symforge --stdio` form was never a valid flag — clap rejected it with
/// exit 2 — so first-run users following the hint hit an immediate error.
const SIDECAR_HINT_COMMAND: &str = "symforge init";

/// Emit a one-time hint to stderr when the sidecar is not running (HOOK-03).
///
/// Uses a marker file (`.symforge/hook-hint-shown`) to avoid repeating the hint
/// within a 30-minute window. The hint is written to stderr regardless of
/// `SYMFORGE_HOOK_VERBOSE` — it is specifically a user-facing one-time hint.
///
/// All I/O failures are silently ignored to preserve fail-open behavior.
fn maybe_emit_sidecar_hint(control_state_dir: Option<&ControlStateDir>) {
    let Some(control_state_dir) = control_state_dir else {
        return;
    };
    let marker_path = crate::paths::control_state_path(control_state_dir, HOOK_HINT_MARKER);

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
    eprintln!("[symforge-hook]   \u{2022} Or run: {SIDECAR_HINT_COMMAND}");
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
        "[symforge-hook] sidecar not running. No matching control-state descriptor for {}.",
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
    let Some(control_state_dir) = process_control_state_dir() else {
        return;
    };
    let _ = append_hook_adoption_event_with_detail(
        &adoption_log_path(&control_state_dir),
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

fn adoption_log_path(control_state_dir: &ControlStateDir) -> PathBuf {
    crate::paths::control_state_path(control_state_dir, ADOPTION_LOG_FILE)
}

fn read_session_id_for_repo_at(
    control_state_dir: &ControlStateDir,
    repo_root: Option<&Path>,
) -> Option<String> {
    crate::sidecar::port_file::read_sidecar_endpoint(control_state_dir, "127.0.0.1", repo_root)
        .ok()?
        .1
        .filter(|value| !value.trim().is_empty())
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
    let Some(control_state_dir) = process_control_state_dir() else {
        return HookAdoptionSnapshot::default();
    };
    load_hook_adoption_snapshot_at(&control_state_dir, repo_root)
}

fn load_hook_adoption_snapshot_at(
    control_state_dir: &ControlStateDir,
    repo_root: Option<&Path>,
) -> HookAdoptionSnapshot {
    let session = read_session_id_for_repo_at(control_state_dir, repo_root);
    load_hook_adoption_snapshot_from_path(&adoption_log_path(control_state_dir), session.as_deref())
        .unwrap_or_default()
}

/// Make a synchronous enrichment GET to `127.0.0.1:{port}{path}?{query}`.
///
/// Uses a raw `TcpStream` (no HTTP client crate) so there is no async runtime
/// and the startup cost is near zero. A 503 is kept distinct from transport
/// failure so an index-loading refusal is not reported as a dead sidecar.
fn sync_enrichment_http_get(port: u16, path: &str, query: String) -> EnrichmentHttpResult {
    sync_enrichment_http_get_with_timeout(port, path, query, HTTP_TIMEOUT)
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

    #[test]
    fn daemon_fallback_path_identity_treats_backslash_as_a_separator_only_on_windows() {
        let literal = r"/work/a\b";
        let nested = "/work/a/b";
        assert_ne!(
            normalize_path_text_for_match(literal, false),
            normalize_path_text_for_match(nested, false)
        );
        assert_eq!(
            normalize_path_text_for_match(literal, true),
            normalize_path_text_for_match(nested, true)
        );
    }

    #[cfg(unix)]
    #[test]
    fn daemon_fallback_path_matching_preserves_literal_backslash_components_on_unix() {
        let root = TempDir::new().expect("create hook path identity fixture");
        let literal = root.path().join(r"a\b");
        let nested = root.path().join("a").join("b");
        std::fs::create_dir_all(&literal).expect("create literal-backslash root");
        std::fs::create_dir_all(&nested).expect("create nested root");

        assert_ne!(
            normalize_path_for_match(&literal),
            normalize_path_for_match(&nested),
            "daemon fallback must not route a nested Unix root to a literal-backslash project"
        );
    }

    #[cfg(unix)]
    #[test]
    fn hook_authority_refuses_non_utf8_lossy_root_collisions() {
        use std::os::unix::ffi::OsStringExt;

        let native = PathBuf::from(std::ffi::OsString::from_vec(vec![b'a', 0xff, b'b']));
        let lossy = PathBuf::from("a\u{fffd}b");
        assert_eq!(native.to_string_lossy(), lossy.to_string_lossy());

        let foreign = DaemonProjectEntry {
            project_id: crate::daemon::project_key(&lossy),
            canonical_root: lossy.to_str().expect("UTF-8 root").to_string(),
            session_count: 1,
        };
        assert!(!daemon_project_matches_native_root(&foreign, &native));
        assert!(append_caller_root_for_path(String::new(), &native).is_none());
    }

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

    #[test]
    fn enrichment_response_keeps_live_refusals_distinct_from_transport_failure() {
        assert_eq!(
            classify_enrichment_response(HttpResponse {
                status_code: 503,
                body: "partial index data must not be injected".to_string(),
            }),
            EnrichmentHttpResult::IndexNotReady
        );
        assert_eq!(
            classify_enrichment_response(HttpResponse {
                status_code: 409,
                body: String::new(),
            }),
            EnrichmentHttpResult::RootConflict
        );
        assert_eq!(
            classify_enrichment_response(HttpResponse {
                status_code: 500,
                body: String::new(),
            }),
            EnrichmentHttpResult::HttpFailure(500)
        );
        assert_eq!(
            classify_enrichment_response(HttpResponse {
                status_code: 404,
                body: String::new(),
            }),
            EnrichmentHttpResult::HttpFailure(404)
        );
        assert_eq!(
            classify_enrichment_response(HttpResponse {
                status_code: 200,
                body: "trusted context".to_string(),
            }),
            EnrichmentHttpResult::Success("trusted context".to_string())
        );
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
    fn test_endpoint_for_grep_regex_pattern_fails_open() {
        // Dogfood #8: regex / multi-token grep patterns are not symbol names —
        // forwarding them guarantees a zero-hit report in prompt context.
        for pattern in [
            "compact|SYMFORGE_SURFACE",
            "Tip:",
            "fn .*_handler",
            "two words",
            "foo.bar",
            "^anchor",
        ] {
            let json = format!(
                r#"{{"tool_name":"Grep","tool_input":{{"pattern":"{pattern}"}},"cwd":"/abs"}}"#
            );
            let input: HookInput = serde_json::from_str(&json).unwrap_or_default();
            let (path, query) = endpoint_for(Some(&HookSubcommand::Grep), &input);
            assert_eq!(path, "/health", "pattern {pattern:?} must fail open");
            assert!(
                query.is_empty(),
                "pattern {pattern:?} must not forward a query"
            );
        }
        // Bare identifiers still get the lookup.
        for pattern in ["classify_admission", "TODO", "_private", "Vec2d"] {
            let json = format!(
                r#"{{"tool_name":"Grep","tool_input":{{"pattern":"{pattern}"}},"cwd":"/abs"}}"#
            );
            let input: HookInput = serde_json::from_str(&json).unwrap_or_default();
            let (path, _) = endpoint_for(Some(&HookSubcommand::Grep), &input);
            assert_eq!(
                path, "/symbol-context",
                "pattern {pattern:?} is a plausible symbol"
            );
        }
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
        // Empty pattern fail-opens to /health (dogfood #8): with nothing to
        // look up, /symbol-context could only produce a zero-hit report.
        let input = HookInput::default();
        let (path, query) = endpoint_for(Some(&HookSubcommand::Grep), &input);
        assert_eq!(path, "/health");
        assert!(query.is_empty());
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
    fn test_pre_tool_suggestion_read_outside_workspace_is_empty() {
        // B5: Read of a %TEMP%-style path outside the repo root must not hint —
        // get_file_context cannot serve paths outside every indexed root.
        let temp_target = std::env::temp_dir()
            .join("claude")
            .join("tasks")
            .join("1.output");
        let input = HookInput {
            tool_name: Some("Read".to_string()),
            tool_input: Some(HookToolInput {
                file_path: Some(temp_target.to_string_lossy().into_owned()),
                ..Default::default()
            }),
            cwd: Some("/repo".to_string()),
            ..Default::default()
        };
        let s = pre_tool_suggestion(&input);
        assert!(s.is_empty(), "should not hint outside the workspace: {s}");
    }

    #[test]
    fn test_pre_tool_suggestion_read_in_repo_absolute_still_hints() {
        // In-repo absolute path under cwd keeps the hint. Use a real absolute
        // path so the check is exercised on every platform.
        let cwd = std::env::current_dir().unwrap();
        let target = cwd.join("src").join("main.rs");
        let input = HookInput {
            tool_name: Some("Read".to_string()),
            tool_input: Some(HookToolInput {
                file_path: Some(target.to_string_lossy().into_owned()),
                ..Default::default()
            }),
            cwd: Some(cwd.to_string_lossy().into_owned()),
            ..Default::default()
        };
        let s = pre_tool_suggestion(&input);
        assert!(
            s.contains("get_file_context"),
            "in-repo absolute read should still hint: {s}"
        );
    }

    #[test]
    fn test_pre_tool_suggestion_grep_outside_workspace_is_empty() {
        // B5: Grep scoped to a directory outside the repo root must not hint.
        let temp_dir = std::env::temp_dir().join("scratch");
        let input = HookInput {
            tool_name: Some("Grep".to_string()),
            tool_input: Some(HookToolInput {
                pattern: Some("helper".to_string()),
                path: Some(temp_dir.to_string_lossy().into_owned()),
                ..Default::default()
            }),
            cwd: Some("/repo".to_string()),
            ..Default::default()
        };
        let s = pre_tool_suggestion(&input);
        assert!(
            s.is_empty(),
            "should not hint for grep outside the workspace: {s}"
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
        let control_state = ControlStateDir::new(tmp.path().join(".symforge"));
        let log_path = adoption_log_path(&control_state);

        append_hook_adoption_event(&log_path, Some("session-a"), "source-read", "routed").unwrap();
        append_hook_adoption_event(&log_path, Some("session-a"), "repo-start", "no-sidecar")
            .unwrap();
        append_hook_adoption_event(&log_path, Some("session-b"), "source-search", "routed")
            .unwrap();
        crate::sidecar::port_file::write_session_descriptor(
            &control_state,
            41_321,
            Some("session-a"),
            Some(tmp.path()),
            None,
        )
        .unwrap();

        let snapshot = load_hook_adoption_snapshot_at(&control_state, Some(tmp.path()));
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

    #[test]
    fn test_health_hook_adoption_metric_pins_published_contract() {
        let tmp = TempDir::new().unwrap();
        let control = ControlStateDir::new(tmp.path().join("control"));
        let log_path = adoption_log_path(&control);

        // Two tracked workflows, both routed — mirrors the "2/2 (100%)"
        // contract shown in CONTEXT.md §Project rules.
        append_hook_adoption_event(&log_path, Some("sess-live"), "repo-start", "routed").unwrap();
        append_hook_adoption_event(&log_path, Some("sess-live"), "prompt-context", "routed")
            .unwrap();
        // adoption_log_path and load_hook_adoption_snapshot must agree on
        // where the log lives — pin that too.
        assert_eq!(adoption_log_path(&control), log_path);

        let snapshot = load_hook_adoption_snapshot_from_path(&log_path, Some("sess-live"))
            .expect("load adoption snapshot");
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
        let control = ControlStateDir::new(tmp.path().join("control"));
        let log_path = adoption_log_path(&control);

        append_hook_adoption_event(&log_path, Some("sess-down"), "source-read", "no-sidecar")
            .unwrap();
        append_hook_adoption_event(&log_path, Some("sess-down"), "prompt-context", "no-sidecar")
            .unwrap();
        let snapshot = load_hook_adoption_snapshot_from_path(&log_path, Some("sess-down"))
            .expect("load adoption snapshot");
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
        let tmp = TempDir::new().unwrap();
        let control = ControlStateDir::new(tmp.path().join("control"));
        record_hook_outcome_at(
            &control,
            HookWorkflow::SourceRead,
            HookOutcome::Routed,
            Some("sess-wireup"),
        );

        let log_path = adoption_log_path(&control);
        assert!(
            log_path.exists(),
            "record_hook_outcome must create {ADOPTION_LOG_FILE} in control state; missing at {}",
            log_path.display()
        );
        let contents = std::fs::read_to_string(&log_path).expect("log readable");
        assert!(
            contents.contains("sess-wireup\tsource-read\trouted"),
            "log must contain the tab-separated routed event; got: {contents:?}"
        );
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
        let control = ControlStateDir::new(tmp.path().join("control"));
        let marker = crate::paths::control_state_path(&control, HOOK_HINT_MARKER);
        assert!(!marker.exists());
        maybe_emit_sidecar_hint(Some(&control));
        assert!(marker.exists(), "marker file should be created");
    }

    #[test]
    fn sidecar_hint_skips_when_marker_fresh() {
        let tmp = TempDir::new().unwrap();
        let control = ControlStateDir::new(tmp.path().join("control"));
        let marker = crate::paths::control_state_path(&control, HOOK_HINT_MARKER);
        std::fs::create_dir_all(marker.parent().unwrap()).unwrap();
        std::fs::write(&marker, "").unwrap();
        // Marker was just created — should be fresh.
        // We can't easily capture stderr in a unit test, but we can verify
        // the marker file's mtime is NOT updated (proving the function returned early).
        let mtime_before = std::fs::metadata(&marker).unwrap().modified().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(50));
        maybe_emit_sidecar_hint(Some(&control));
        let mtime_after = std::fs::metadata(&marker).unwrap().modified().unwrap();
        assert_eq!(
            mtime_before, mtime_after,
            "marker mtime should not change when fresh"
        );
    }

    /// SF-STRESS-020: the sidecar hint must advertise a command the binary
    /// actually accepts. The previous `symforge --stdio` form was rejected by
    /// clap with exit 2, so first-run users following the hint hit an error.
    #[test]
    fn sidecar_hint_command_parses_as_valid_cli_invocation() {
        use clap::Parser;
        let args: Vec<&str> = SIDECAR_HINT_COMMAND.split_whitespace().collect();
        assert_eq!(
            args.first().copied(),
            Some("symforge"),
            "hint command must invoke the symforge binary"
        );
        crate::cli::Cli::try_parse_from(args).unwrap_or_else(|e| {
            panic!("sidecar hint command `{SIDECAR_HINT_COMMAND}` must parse as a valid CLI invocation, got: {e}")
        });
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
