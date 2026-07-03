//! `symforge setup` — guided operator setup wizard (009 US2).
//!
//! Orchestrates the shipped 004/005/006 capabilities (harness scan/apply,
//! serve, admin dashboard) into one guided command: scan -> choose -> restate
//! -> apply (with restorable backups) -> serve-start -> open -> persist. This
//! module is the thin wizard layer; it does not reimplement scan/apply/serve.
//!
//! Phase 1 (T003) lands the clap arg surface + a `run` skeleton that returns a
//! clear "not yet implemented" notice. The flow steps (scan/choose/apply/serve)
//! and the `SetupSink` seam land in later phases (T009/T014-T019).

use std::io::{BufRead, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

use anyhow::Context;
use clap::{Args, ValueEnum};
use serde::{Deserialize, Serialize};

use crate::cli::admin::{
    ADMIN_SERVE_START_DEADLINE, FOREGROUND_SERVE_NOTICE, ServerSessionDescriptor,
    operator_server_reachable, start_operator_server,
};
use crate::cli::browser::{BrowserOpener, OsBrowserOpener};
use crate::cli::harness::{AttachEntry, HarnessId, HarnessRegistry, HarnessState};
use crate::cli::harness_apply::{self, ApplyOutcome, PlannedAction};
use crate::cli::operator_profile::{AuthPosture, OperatorSetupProfile};
use crate::server::serve::DEFAULT_LISTEN;

/// Which parts of SymForge a setup run configures.
///
/// Mirrors the `--installation-type` contract values (contracts/setup-cli.md):
/// `in-harness` configures harness MCP attach only; `server` starts the
/// operator server/dashboard; `both` does both. The serde rename pair matches the
/// clap `value(name = ...)` so the CLI flag, the profile JSON, and the wire shape
/// all use the same `in-harness | server | both` strings.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum, Serialize, Deserialize)]
pub enum InstallationType {
    /// Configure harness MCP attach entries only (no operator server).
    #[value(name = "in-harness")]
    #[serde(rename = "in-harness")]
    InHarness,
    /// Start the operator server / dashboard.
    #[serde(rename = "server")]
    Server,
    /// Configure harnesses and start the operator server.
    #[serde(rename = "both")]
    Both,
}

/// Flags for `symforge setup` (see `contracts/setup-cli.md`).
#[derive(Args, Debug, Clone)]
pub struct SetupCliArgs {
    /// Drive with pre-supplied answers: no terminal read, no browser, no network
    /// probe beyond the bind (FR-014).
    #[arg(long)]
    pub non_interactive: bool,

    /// Pre-answer the install type (`in-harness` | `server` | `both`).
    #[arg(long, value_enum)]
    pub installation_type: Option<InstallationType>,

    /// Preferred bind port (else the verified-free suggestion).
    #[arg(long)]
    pub port: Option<u16>,

    /// Comma-separated harness ids to configure (else all detected).
    #[arg(long, value_delimiter = ',')]
    pub harnesses: Vec<String>,

    /// Auto-confirm the restated action plan (for scripts).
    #[arg(long)]
    pub yes: bool,
}

/// Operator-facing I/O seam for the setup wizard (E6, contracts/seams.md).
///
/// Every terminal side effect goes through this trait so the whole flow runs in
/// tests with scripted answers and zero real terminal reads (FR-017/FR-014).
/// Mirrors the shipped `OnboardingSink` seam (`cli::onboarding`).
pub trait SetupSink {
    /// Emit a progress / summary line.
    fn status(&mut self, line: &str);
    /// Ask the operator to pick one of `opts`; returns the chosen index.
    fn ask_choice(&mut self, q: &str, opts: &[&str]) -> usize;
    /// Restate the exact action plan and ask for confirmation (FR-008).
    fn confirm(&mut self, action_plan: &str) -> bool;
}

/// Real terminal sink: prints to stderr and reads a line from stdin.
///
/// `ask_choice` numbers the options and parses a 1-based index (out-of-range or
/// unparseable input falls back to the first option — a safe default that never
/// panics). `confirm` parses a `y`/`yes` (case-insensitive) as `true`, anything
/// else as `false`.
pub struct StderrSetupSink;

impl SetupSink for StderrSetupSink {
    fn status(&mut self, line: &str) {
        eprintln!("{line}");
    }

    fn ask_choice(&mut self, q: &str, opts: &[&str]) -> usize {
        eprintln!("{q}");
        for (i, opt) in opts.iter().enumerate() {
            eprintln!("  {}) {opt}", i + 1);
        }
        eprint!("> ");
        let _ = std::io::stderr().flush();
        let line = read_stdin_line();
        match line.trim().parse::<usize>() {
            Ok(n) if (1..=opts.len()).contains(&n) => n - 1,
            // Unparseable / out of range: default to the first option rather than
            // looping or panicking — the caller restates before any change.
            _ => 0,
        }
    }

    fn confirm(&mut self, action_plan: &str) -> bool {
        eprintln!("{action_plan}");
        eprint!("Proceed? [y/N] ");
        let _ = std::io::stderr().flush();
        let line = read_stdin_line();
        matches!(line.trim().to_ascii_lowercase().as_str(), "y" | "yes")
    }
}

/// Read one line from stdin, returning an empty string on EOF / read error so the
/// caller's parsing applies its safe default rather than the sink panicking.
fn read_stdin_line() -> String {
    let mut line = String::new();
    let stdin = std::io::stdin();
    let _ = stdin.lock().read_line(&mut line);
    line
}

/// Test sink: returns pre-scripted answers and records every prompt; never reads
/// stdin (FR-014). Construct with a queue of choice indices and a single confirm
/// answer.
#[derive(Debug, Default)]
pub struct ScriptedSetupSink {
    /// Pre-supplied `ask_choice` answers, consumed front-to-back.
    choices: std::collections::VecDeque<usize>,
    /// The answer every `confirm` returns.
    confirm_answer: bool,
    /// Every `status` line emitted, in order.
    pub statuses: Vec<String>,
    /// Every `ask_choice` question + its options, in order.
    pub questions: Vec<String>,
    /// Every `confirm` action plan presented, in order.
    pub confirmations: Vec<String>,
}

impl ScriptedSetupSink {
    /// Build a scripted sink with the given choice answers and confirm result.
    pub fn new(choices: impl IntoIterator<Item = usize>, confirm_answer: bool) -> Self {
        Self {
            choices: choices.into_iter().collect(),
            confirm_answer,
            statuses: Vec::new(),
            questions: Vec::new(),
            confirmations: Vec::new(),
        }
    }
}

impl SetupSink for ScriptedSetupSink {
    fn status(&mut self, line: &str) {
        self.statuses.push(line.to_string());
    }

    fn ask_choice(&mut self, q: &str, opts: &[&str]) -> usize {
        self.questions.push(format!("{q} [{}]", opts.join(", ")));
        // Pop the next scripted answer; if the script is exhausted, default to the
        // first option (deterministic, never reads stdin, never panics).
        self.choices.pop_front().unwrap_or(0)
    }

    fn confirm(&mut self, action_plan: &str) -> bool {
        self.confirmations.push(action_plan.to_string());
        self.confirm_answer
    }
}

/// Injectable paths for the wizard (FR-017/018). Tests pass a temp home + project.
#[derive(Debug, Clone)]
pub struct SetupContext {
    pub home: std::path::PathBuf,
    pub working_dir: std::path::PathBuf,
}

impl SetupContext {
    /// Build from the live process environment.
    pub fn from_env() -> anyhow::Result<Self> {
        let home = dirs::home_dir().context("cannot determine home directory")?;
        let working_dir =
            std::env::current_dir().context("cannot determine current working directory")?;
        Ok(Self { home, working_dir })
    }

    /// Where [`OperatorSetupProfile`] is stored (`<project>/.symforge/`).
    pub fn project_base(&self) -> std::path::PathBuf {
        self.working_dir.clone()
    }

    /// The harness registry resolved against this context's home + working dir.
    /// Fixture seam: tests build the context over a TempDir so the wizard scans
    /// fixture configs, never the real operator's (FR-018).
    pub fn registry(&self) -> HarnessRegistry {
        HarnessRegistry::known_with(&self.home, &self.working_dir)
    }
}

/// The result of one wizard run, returned for caller messaging and test
/// assertions (FR-017: every effect is observable without scraping stderr).
///
/// Production [`run`] discards it after the sink has already narrated each step;
/// tests inspect it to assert outcomes deterministically — the apply results,
/// the reachable server descriptor, the persisted profile — over fixtures.
#[derive(Debug)]
pub struct WizardOutcome {
    /// The installation type the wizard actually ran with.
    pub installation_type: InstallationType,
    /// The running operator server (started or reused) for a server mode; always
    /// reachable when present (FR-020). `None` for in-harness-only or a decline.
    pub session: Option<ServerSessionDescriptor>,
    /// `true` when a server mode reused an already-reachable server instead of
    /// starting a second one (FR-013 idempotency / SC-004).
    pub reused_server: bool,
    /// Per-harness apply outcomes (empty unless `installation_type == Both`).
    pub harness_outcomes: Vec<ApplyOutcome>,
    /// The browser-open outcome for the dashboard, if a server was reported.
    pub browser_outcome: Option<crate::cli::browser::BrowserOpenOutcome>,
    /// The profile persisted by this run (FR-012). On a decline this carries the
    /// would-be choices but was NOT written to disk (`cancelled == true`).
    pub profile: OperatorSetupProfile,
    /// `true` when the operator declined the restated plan; nothing was changed.
    pub cancelled: bool,
}

/// Entry point for `symforge setup`. Wires the real seams (stderr sink, OS
/// browser, live home/cwd) into [`run_wizard`] and discards the outcome — the
/// sink has already narrated every step to the operator.
pub fn run(args: SetupCliArgs) -> anyhow::Result<()> {
    let ctx = SetupContext::from_env()?;
    if args.non_interactive {
        // Non-interactive answers are pre-supplied (FR-014); `--yes` is the
        // scripted confirmation, so no terminal read occurs.
        let mut sink = ScriptedSetupSink::new([], args.yes);
        let browser = crate::cli::browser::NoopBrowserOpener::default();
        run_wizard(args, &ctx, &mut sink, &browser)?;
    } else {
        let mut sink = StderrSetupSink;
        let browser = OsBrowserOpener;
        let outcome = run_wizard(args, &ctx, &mut sink, &browser)?;
        // D21 sibling: an interactive fresh-start server runs on a background
        // thread that dies when this process exits — so the dashboard URL the
        // wizard just printed would be dead the instant setup returns. Hold the
        // foreground and keep serving until Ctrl-C; a reused server (owned by
        // another process) returns immediately. The non-interactive branch above
        // never blocks (scripting / CI must return).
        // Manual-verification contract: the seam's reuse-vs-block behavior is unit-
        // tested directly (setup_holds_foreground_*/setup_does_not_hold_*), but
        // `run`'s own wiring — INTERACTIVE branch calls the seam, non-interactive
        // branch never does — rides on `from_env` (un-seamed real home/cwd) and is
        // pinned by an interactive TTY run, not an in-lib test.
        serve_foreground_if_wizard_started(&outcome, block_until_ctrl_c)?;
    }
    Ok(())
}

/// Keep an interactive `symforge setup` in the foreground when the wizard STARTED
/// the operator server, so the "Dashboard: <url>" it just printed actually keeps
/// serving (D21 sibling of the admin-verb fix). A reused server is owned by
/// another process (return immediately); a run that started nothing (in-harness
/// mode, or a declined plan) also returns. Only a server we started ourselves
/// must be held alive here, blocking in `wait_for_shutdown` until the operator
/// stops the process (Ctrl-C in production).
///
/// The non-interactive / scripted path (FR-014) never reaches this — [`run`]
/// returns there so scripts and CI don't hang, and its `ScriptedSetupSink` prints
/// nothing to a human, so a transient verify-server dying with the process makes
/// no false promise. Split from [`run`] and generic over the waiter so the
/// reuse-vs-block decision is unit-testable without an unstoppable Ctrl-C wait.
/// Mirrors `admin::serve_foreground_if_started`.
fn serve_foreground_if_wizard_started(
    outcome: &WizardOutcome,
    wait_for_shutdown: impl FnOnce(&ServerSessionDescriptor) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    if outcome.reused_server {
        return Ok(());
    }
    let Some(session) = outcome.session.as_ref() else {
        return Ok(());
    };
    eprintln!("{FOREGROUND_SERVE_NOTICE}");
    wait_for_shutdown(session)
}

/// Block until the operator asks the process to stop (Ctrl-C on all platforms) on
/// a sibling current-thread runtime — [`run`] is synchronous and the operator
/// server owns its own thread + runtime, so this is never a nested reactor.
///
/// ponytail: a 5-line twin of the private `admin::block_until_ctrl_c`; reusing it
/// would mean widening that helper's visibility in admin.rs, out of scope for this
/// fix. Collapse the two into one shared helper if a third caller ever appears.
fn block_until_ctrl_c(_session: &ServerSessionDescriptor) -> anyhow::Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(tokio::signal::ctrl_c())?;
    Ok(())
}

/// Testable wizard core: scan -> choose -> restate/confirm -> apply -> serve ->
/// open -> persist, with every terminal / browser / process effect behind a
/// seam (FR-017). Tests call this directly with a [`ScriptedSetupSink`], a
/// [`crate::cli::browser::NoopBrowserOpener`], and a TempDir-backed
/// [`SetupContext`], then assert on the returned [`WizardOutcome`] — no real
/// terminal, no real browser, and (apart from a deliberate loopback bind) no
/// network (FR-014/FR-018).
pub fn run_wizard<S: SetupSink + ?Sized, B: BrowserOpener + ?Sized>(
    args: SetupCliArgs,
    ctx: &SetupContext,
    sink: &mut S,
    browser: &B,
) -> anyhow::Result<WizardOutcome> {
    let registry = ctx.registry();
    let project_base = ctx.project_base();
    let existing_profile = OperatorSetupProfile::load(&project_base);

    // --- Step 1: scan (read-only, FR-004) -----------------------------------
    let suggested_port = suggest_free_port(args.port)?;
    sink.status(&format!(
        "SymForge setup — OS: {} — suggested port: {suggested_port}",
        std::env::consts::OS
    ));
    for status in scan_harness_summary(&registry, None) {
        sink.status(&status);
    }

    // Report (read-only) whether a remembered server is already up (FR-004/013).
    let remembered = existing_profile.as_ref().map(|p| p.port);
    let mut remembered_reachable: Option<SocketAddr> = None;
    if let Some(port) = remembered {
        let addr = loopback_addr(port);
        if operator_server_reachable(addr, Duration::from_millis(500)) {
            remembered_reachable = Some(addr);
            sink.status(&format!(
                "Operator server already reachable on {addr} (profile port {port}) — will reuse"
            ));
        }
    }

    // --- Step 2: choose (FR-005/006) ----------------------------------------
    let installation_type = resolve_installation_type(&args, sink)?;
    let selected_ids = resolve_harness_ids(&args, &registry);
    let bind_port = args.port.unwrap_or(suggested_port);
    let bind_addr = loopback_addr(bind_port);

    let needs_server = matches!(
        installation_type,
        InstallationType::Server | InstallationType::Both
    );
    let needs_harness_http = installation_type == InstallationType::Both;

    // --- Step 3: restate + confirm (FR-008) ---------------------------------
    // The confirm always routes through the sink so a non-interactive run uses
    // the pre-supplied (scripted) answer and the restate is recorded for
    // assertion. `--yes` short-circuits the prompt for scripts; an interactive
    // decline exits having changed nothing.
    let action_plan = build_action_plan(
        installation_type,
        &selected_ids,
        bind_addr,
        needs_server,
        needs_harness_http,
    );
    if !args.yes && !sink.confirm(&action_plan) {
        sink.status("Setup cancelled — no changes made.");
        let profile = OperatorSetupProfile::new(
            installation_type,
            bind_port,
            AuthPosture::LoopbackNoKey,
            &selected_ids,
            now_epoch_ms(),
        );
        return Ok(WizardOutcome {
            installation_type,
            session: None,
            reused_server: false,
            harness_outcomes: Vec::new(),
            browser_outcome: None,
            profile,
            cancelled: true,
        });
    }
    if args.yes {
        // Still surface the plan even when auto-confirming (FR-008 visibility).
        sink.status(&action_plan);
    }

    if installation_type == InstallationType::InHarness {
        sink.status(
            "In-harness (stdio) mode: configure local MCP clients with `symforge init` per client.",
        );
    }

    // --- Step 5: server mode (FR-007/010/011/013) ---------------------------
    let mut session: Option<ServerSessionDescriptor> = None;
    let mut reused_server = false;
    if needs_server {
        if let Some(addr) = remembered_reachable {
            // FR-013: reuse the already-running server; never start a second one.
            session = Some(ServerSessionDescriptor::for_addr(addr, true));
            reused_server = true;
            sink.status(&format!("Reusing running operator server on {addr}"));
        }
        if session.is_none() {
            sink.status(&format!("Starting operator server on {bind_addr}…"));
            // FR-007: a loopback bind requires no key — `AuthConfig::refuse_to_start`
            // permits a keyless loopback serve. A routable bind would require a key
            // sourced from the env (api_key_env), never an inline key; serve::run
            // enforces refuse-to-start for any non-loopback address regardless.
            // This slice binds loopback only, so no key is passed.
            session = Some(start_operator_server(
                Some(bind_addr),
                None,
                None,
                ADMIN_SERVE_START_DEADLINE,
            )?);
        }
        let desc = session.as_ref().expect("session started or reused");
        sink.status(&format!("Dashboard: {}", desc.dashboard_url));
        sink.status(&format!("Attach:    {}", desc.attach_url));
    }

    // --- Step 4: apply harness attach entries (FR-009) ----------------------
    let mut harness_outcomes = Vec::new();
    if needs_harness_http {
        let desc = session
            .as_ref()
            .context("internal error: harness HTTP attach requires a running operator server")?;
        let attach_entry = AttachEntry::new(desc.attach_url.clone(), None);
        let subset = registry_subset(&registry, &selected_ids);
        let plan = harness_apply::plan(&subset, &attach_entry);
        sink.status("Applying harness attach entries (restorable backups)…");
        for change in &plan.changes {
            let label = match &change.action {
                PlannedAction::Add => "add",
                PlannedAction::Refresh => "refresh",
                PlannedAction::Skip(reason) => {
                    sink.status(&format!("  [skip] {} ({reason})", change.id.display_name()));
                    continue;
                }
                PlannedAction::Error(reason) => {
                    sink.status(&format!(
                        "  [error] {} ({reason})",
                        change.id.display_name()
                    ));
                    continue;
                }
            };
            sink.status(&format!(
                "  [{label}] {} -> {}",
                change.id.display_name(),
                change.config_path.display()
            ));
        }
        harness_outcomes = harness_apply::apply(&plan);
        for outcome in &harness_outcomes {
            match outcome {
                ApplyOutcome::Wrote {
                    id,
                    config_path,
                    backup,
                } => {
                    let backup_note = backup
                        .as_ref()
                        .map(|b| format!(" (backup: {})", b.backup.display()))
                        .unwrap_or_default();
                    sink.status(&format!(
                        "  [written] {} -> {}{backup_note}",
                        id.display_name(),
                        config_path.display()
                    ));
                }
                ApplyOutcome::Skipped { id, reason } => {
                    sink.status(&format!("  [skipped] {} ({reason})", id.display_name()));
                }
                ApplyOutcome::Failed { id, reason } => {
                    sink.status(&format!("  [failed] {} ({reason})", id.display_name()));
                }
            }
        }
    }

    // --- Step 6: persist (FR-012) -------------------------------------------
    let bound_port = session
        .as_ref()
        .map(|s| s.bound_addr.port())
        .unwrap_or(bind_port);
    let profile = OperatorSetupProfile::new(
        installation_type,
        bound_port,
        AuthPosture::LoopbackNoKey,
        &selected_ids,
        now_epoch_ms(),
    );
    profile
        .save(&project_base)
        .context("could not persist operator setup profile")?;

    // --- Step 5b: offer to open the dashboard (FR-011) ----------------------
    let mut browser_outcome = None;
    if let Some(desc) = &session {
        let outcome = browser.open_url(&desc.dashboard_url);
        sink.status(&format!(
            "Browser: {outcome:?} — open {}",
            desc.dashboard_url
        ));
        browser_outcome = Some(outcome);
    }

    sink.status("Setup complete.");
    Ok(WizardOutcome {
        installation_type,
        session,
        reused_server,
        harness_outcomes,
        browser_outcome,
        profile,
        cancelled: false,
    })
}

fn now_epoch_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn loopback_addr(port: u16) -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port)
}

fn suggest_free_port(preferred: Option<u16>) -> anyhow::Result<u16> {
    let preferred_addr = preferred
        .map(loopback_addr)
        .or_else(|| DEFAULT_LISTEN.parse().ok());
    let addr = crate::cli::admin::select_free_addr_std(preferred_addr)
        .map_err(|e| anyhow::anyhow!("could not find a free operator-server port: {e}"))?;
    Ok(addr.port())
}

fn scan_harness_summary(registry: &HarnessRegistry, attach: Option<&AttachEntry>) -> Vec<String> {
    let entry = attach
        .cloned()
        .unwrap_or_else(|| AttachEntry::new("", None));
    registry
        .scan(&entry)
        .into_iter()
        .map(|s| {
            format!(
                "  {} — {} ({})",
                s.id.display_name(),
                format_harness_state(&s.state),
                s.config_path.display()
            )
        })
        .collect()
}

fn format_harness_state(state: &HarnessState) -> &'static str {
    match state {
        HarnessState::NotInstalled => "not installed",
        HarnessState::Absent => "not configured",
        HarnessState::PresentCurrent => "configured (current)",
        HarnessState::PresentStale => "configured (stale)",
        HarnessState::Malformed(_) => "unreadable",
    }
}

fn resolve_installation_type<S: SetupSink + ?Sized>(
    args: &SetupCliArgs,
    sink: &mut S,
) -> anyhow::Result<InstallationType> {
    if let Some(t) = args.installation_type {
        return Ok(t);
    }
    if args.non_interactive {
        return Ok(InstallationType::Both);
    }
    let idx = sink.ask_choice(
        "Installation type?",
        &[
            "in-harness only (stdio via symforge init)",
            "operator server + dashboard",
            "both (server + harness HTTP attach)",
        ],
    );
    Ok(match idx {
        0 => InstallationType::InHarness,
        1 => InstallationType::Server,
        _ => InstallationType::Both,
    })
}

fn resolve_harness_ids(args: &SetupCliArgs, registry: &HarnessRegistry) -> Vec<HarnessId> {
    if !args.harnesses.is_empty() {
        return args
            .harnesses
            .iter()
            .filter_map(|slug| harness_id_from_slug(slug))
            .collect();
    }
    let entry = AttachEntry::new("", None);
    registry
        .scan(&entry)
        .into_iter()
        .filter(|s| !matches!(s.state, HarnessState::NotInstalled))
        .map(|s| s.id)
        .collect()
}

fn harness_id_from_slug(slug: &str) -> Option<HarnessId> {
    let slug = slug.trim().to_ascii_lowercase();
    [
        HarnessId::ClaudeCode,
        HarnessId::ClaudeDesktop,
        HarnessId::Codex,
        HarnessId::Gemini,
        HarnessId::KiloCode,
        HarnessId::Cursor,
    ]
    .into_iter()
    .find(|id| id.slug() == slug)
}

fn registry_subset(registry: &HarnessRegistry, ids: &[HarnessId]) -> HarnessRegistry {
    let targets = registry
        .targets()
        .iter()
        .filter(|t| ids.contains(&t.id))
        .cloned()
        .collect();
    HarnessRegistry::from_targets(targets)
}

fn build_action_plan(
    installation_type: InstallationType,
    harnesses: &[HarnessId],
    bind_addr: SocketAddr,
    needs_server: bool,
    needs_harness_http: bool,
) -> String {
    let mut lines = vec![format!(
        "Planned actions ({}):",
        installation_type_label(installation_type)
    )];
    if needs_server {
        lines.push(format!("  • start operator server on {bind_addr}"));
    }
    if needs_harness_http {
        for id in harnesses {
            lines.push(format!(
                "  • configure {} HTTP attach (backup + write)",
                id.display_name()
            ));
        }
    }
    if installation_type == InstallationType::InHarness {
        lines.push("  • no server; use `symforge init` for stdio MCP clients".to_string());
    }
    lines.join("\n")
}

fn installation_type_label(t: InstallationType) -> &'static str {
    match t {
        InstallationType::InHarness => "in-harness",
        InstallationType::Server => "server",
        InstallationType::Both => "both",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scripted_sink_returns_scripted_answers_and_records_prompts() {
        let mut sink = ScriptedSetupSink::new([1, 0], true);

        sink.status("scanning harnesses");
        let first = sink.ask_choice("install type?", &["in-harness", "server", "both"]);
        let second = sink.ask_choice("which harness?", &["claude", "codex"]);
        let proceed = sink.confirm("will configure: claude");

        // Scripted answers are returned front-to-back.
        assert_eq!(first, 1, "first ask_choice pops the first scripted answer");
        assert_eq!(second, 0, "second ask_choice pops the next scripted answer");
        assert!(proceed, "confirm returns the scripted bool");

        // Every prompt is recorded for assertion.
        assert_eq!(sink.statuses, vec!["scanning harnesses"]);
        assert_eq!(sink.questions.len(), 2);
        assert!(sink.questions[0].contains("install type?"));
        assert!(sink.questions[0].contains("in-harness, server, both"));
        assert_eq!(sink.confirmations, vec!["will configure: claude"]);
    }

    #[test]
    fn scripted_sink_defaults_to_first_when_script_exhausted() {
        // An empty script must never read stdin / panic: it defaults to index 0.
        let mut sink = ScriptedSetupSink::new([], false);
        assert_eq!(sink.ask_choice("pick", &["a", "b"]), 0);
        assert!(!sink.confirm("plan"));
    }

    #[test]
    fn installation_type_serde_uses_contract_strings() {
        // The profile + wire shape must use the documented strings, matching the
        // clap `value(name = ...)`.
        assert_eq!(
            serde_json::to_string(&InstallationType::InHarness).unwrap(),
            "\"in-harness\""
        );
        assert_eq!(
            serde_json::to_string(&InstallationType::Both).unwrap(),
            "\"both\""
        );
        let parsed: InstallationType = serde_json::from_str("\"server\"").unwrap();
        assert_eq!(parsed, InstallationType::Server);
    }

    #[test]
    fn non_interactive_setup_configures_fixture_and_persists_profile() {
        use crate::cli::admin::operator_server_reachable;
        use crate::cli::browser::NoopBrowserOpener;
        use crate::cli::harness::{HarnessFormat, HarnessTarget, SYMFORGE_SERVER_NAME};
        use crate::cli::harness_apply::{PlannedAction, plan};

        let home = tempfile::tempdir().expect("temp home");
        let project = tempfile::tempdir().expect("temp project");
        let cfg = home.path().join(".claude.json");
        std::fs::write(&cfg, "{}\n").expect("fixture harness config");

        let ctx = SetupContext {
            home: home.path().to_path_buf(),
            working_dir: project.path().to_path_buf(),
        };

        let mut sink = ScriptedSetupSink::new([], true);
        let browser = NoopBrowserOpener::default();

        let outcome = run_wizard(
            SetupCliArgs {
                non_interactive: true,
                installation_type: Some(InstallationType::Both),
                port: None,
                harnesses: vec!["claude".into()],
                yes: true,
            },
            &ctx,
            &mut sink,
            &browser,
        )
        .expect("setup should succeed");

        assert!(!outcome.cancelled);
        assert_eq!(outcome.installation_type, InstallationType::Both);

        assert!(
            sink.statuses.iter().any(|l| l.contains("Claude Code")),
            "scan must list harnesses: {:?}",
            sink.statuses
        );

        let profile = OperatorSetupProfile::load(project.path()).expect("profile persisted");
        assert_eq!(profile.installation_type, InstallationType::Both);
        assert!(profile.port > 0);
        assert_eq!(profile.harnesses, vec!["claude"]);

        let after = std::fs::read_to_string(&cfg).expect("config exists");
        assert!(after.contains(SYMFORGE_SERVER_NAME));

        // The reported attach URL comes straight off the reachable descriptor.
        let session = outcome.session.as_ref().expect("server started");
        assert!(session.reachable);
        let attach_url = session.attach_url.clone();
        assert!(attach_url.contains("/mcp"));

        assert!(
            operator_server_reachable(session.bound_addr, Duration::from_millis(500)),
            "server must be reachable on reported port"
        );
        assert_eq!(browser.opened_urls().len(), 1);

        let entry = AttachEntry::new(attach_url, None);
        let reg = HarnessRegistry::from_targets(vec![HarnessTarget {
            id: HarnessId::ClaudeCode,
            config_path: cfg,
            format: HarnessFormat::Json,
        }]);
        let p2 = plan(&reg, &entry);
        assert!(matches!(p2.changes[0].action, PlannedAction::Skip(_)));
    }

    #[test]
    fn non_interactive_decline_writes_nothing_and_is_not_an_error() {
        use crate::cli::browser::NoopBrowserOpener;

        let home = tempfile::tempdir().expect("temp home");
        let project = tempfile::tempdir().expect("temp project");
        let cfg = home.path().join(".claude.json");
        let original = "{\n  \"mcpServers\": {}\n}\n";
        std::fs::write(&cfg, original).expect("fixture");

        let ctx = SetupContext {
            home: home.path().to_path_buf(),
            working_dir: project.path().to_path_buf(),
        };
        // No `--yes`: the scripted confirm answer (false) declines the plan.
        let mut sink = ScriptedSetupSink::new([], false);
        let browser = NoopBrowserOpener::default();

        let outcome = run_wizard(
            SetupCliArgs {
                non_interactive: true,
                installation_type: Some(InstallationType::Both),
                port: None,
                harnesses: vec!["claude".into()],
                yes: false,
            },
            &ctx,
            &mut sink,
            &browser,
        )
        .expect("a decline is a clean no-op, not an error");

        // Declined: nothing applied, no server, no profile, config untouched.
        assert!(outcome.cancelled);
        assert!(outcome.session.is_none());
        assert!(outcome.harness_outcomes.is_empty());
        assert_eq!(std::fs::read_to_string(&cfg).unwrap(), original);
        assert!(OperatorSetupProfile::load(project.path()).is_none());
        // The restate was still presented before the decline (FR-008).
        assert_eq!(sink.confirmations.len(), 1);
    }

    #[test]
    fn setup_holds_foreground_after_fresh_start_until_shutdown() {
        // No server running -> the wizard's Server mode starts one on a background
        // thread. serve_foreground_if_wizard_started must then KEEP setup in the
        // foreground (invoke the waiter), and while "blocked" the freshly-started
        // server must be reachable AND STAY reachable. D21 sibling: pre-fix
        // `setup::run` returned immediately, so the serve thread died with the process
        // and the "Dashboard: <url>" the interactive wizard printed was refused.
        use crate::cli::admin::operator_server_reachable;
        use crate::cli::browser::NoopBrowserOpener;

        let home = tempfile::tempdir().expect("temp home");
        let project = tempfile::tempdir().expect("temp project");
        let ctx = SetupContext {
            home: home.path().to_path_buf(),
            working_dir: project.path().to_path_buf(),
        };
        let mut sink = ScriptedSetupSink::new([], true);
        let browser = NoopBrowserOpener::default();

        let outcome = run_wizard(
            SetupCliArgs {
                non_interactive: true,
                installation_type: Some(InstallationType::Server),
                port: None,
                harnesses: vec![],
                yes: true,
            },
            &ctx,
            &mut sink,
            &browser,
        )
        .expect("server-mode setup should start a server when none runs");
        assert!(
            !outcome.reused_server,
            "no server ran; the wizard must start one"
        );
        assert!(
            outcome.session.is_some(),
            "a fresh start yields a live session"
        );

        let waited = std::cell::Cell::new(false);
        serve_foreground_if_wizard_started(&outcome, |session| {
            waited.set(true);
            // Probe twice with a gap: the started server must be serving now AND still
            // serving a moment later — not fire-and-return (D21).
            assert!(
                operator_server_reachable(session.bound_addr, Duration::from_millis(500)),
                "freshly-started server must be reachable while setup holds the foreground"
            );
            std::thread::sleep(Duration::from_millis(300));
            assert!(
                operator_server_reachable(session.bound_addr, Duration::from_millis(500)),
                "the started server must STAY reachable (D21: it must not die after start)"
            );
            Ok(())
        })
        .expect("foreground wait returns cleanly on shutdown");

        assert!(
            waited.get(),
            "a freshly-started server must hold the foreground (D21 fix); pre-fix `run` returned immediately"
        );
        // The fresh branch prints FOREGROUND_SERVE_NOTICE right before the waiter, so
        // an interactive fresh start is not a silent block; pin the wording (stderr
        // capture is impractical in-lib, and `waited.get()` proves the branch ran).
        assert!(
            FOREGROUND_SERVE_NOTICE.contains("Ctrl-C"),
            "the foreground notice must tell the operator how to stop it"
        );
    }

    #[test]
    fn setup_does_not_hold_foreground_when_reusing() {
        // A server already runs and the profile points at it -> the wizard reuses it.
        // serve_foreground_if_wizard_started must NOT block: another process owns the
        // server, so setup returns without invoking the waiter (reuse path unchanged).
        use crate::cli::browser::NoopBrowserOpener;

        let preferred = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
        let running =
            start_operator_server(Some(preferred), None, None, ADMIN_SERVE_START_DEADLINE)
                .expect("operator server should come up");

        let home = tempfile::tempdir().expect("temp home");
        let project = tempfile::tempdir().expect("temp project");
        OperatorSetupProfile::new(
            InstallationType::Server,
            running.bound_addr.port(),
            AuthPosture::LoopbackNoKey,
            &[],
            1,
        )
        .save(project.path())
        .expect("persist profile");

        let ctx = SetupContext {
            home: home.path().to_path_buf(),
            working_dir: project.path().to_path_buf(),
        };
        let mut sink = ScriptedSetupSink::new([], true);
        let browser = NoopBrowserOpener::default();

        let outcome = run_wizard(
            SetupCliArgs {
                non_interactive: true,
                installation_type: Some(InstallationType::Server),
                port: None,
                harnesses: vec![],
                yes: true,
            },
            &ctx,
            &mut sink,
            &browser,
        )
        .expect("server-mode setup should reuse the running server");
        assert!(
            outcome.reused_server,
            "must reuse the already-running server"
        );

        let waited = std::cell::Cell::new(false);
        serve_foreground_if_wizard_started(&outcome, |_| {
            waited.set(true);
            Ok(())
        })
        .expect("reuse path returns cleanly");
        assert!(
            !waited.get(),
            "the reuse path must not hold the foreground (another process owns the server)"
        );
    }
}
