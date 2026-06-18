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

use clap::{Args, ValueEnum};
use serde::{Deserialize, Serialize};

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
        crate::discovery::find_project_root()
            .unwrap_or_else(|| self.working_dir.clone())
    }

    fn registry(&self) -> HarnessRegistry {
        HarnessRegistry::known_with(&self.home, &self.working_dir)
    }
}

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

use crate::cli::admin::{
    operator_server_reachable, start_operator_server, ServerSessionDescriptor,
};
use crate::cli::browser::{BrowserOpener, OsBrowserOpener};
use crate::cli::harness::{AttachEntry, HarnessId, HarnessRegistry, HarnessState};
use crate::cli::harness_apply::{self, ApplyOutcome, PlannedAction};
use crate::cli::operator_profile::{AuthPosture, OperatorSetupProfile};
use crate::server::serve::DEFAULT_LISTEN;

use super::operator_profile;

/// Entry point for `symforge setup`.
pub fn run(args: SetupCliArgs) -> anyhow::Result<()> {
    let ctx = SetupContext::from_env()?;
    if args.non_interactive {
        let mut sink = ScriptedSetupSink::new([], args.yes);
        let browser = crate::cli::browser::NoopBrowserOpener::default();
        run_with(args, &ctx, &mut sink, &browser)
    } else {
        let mut sink = StderrSetupSink;
        let browser = OsBrowserOpener;
        run_with(args, &ctx, &mut sink, &browser)
    }
}

/// Testable wizard core: all terminal and browser I/O via seams (FR-017).
pub fn run_with<S: SetupSink + ?Sized, B: BrowserOpener + ?Sized>(
    args: SetupCliArgs,
    ctx: &SetupContext,
    sink: &mut S,
    browser: &B,
) -> anyhow::Result<()> {
    let registry = ctx.registry();
    let project_base = ctx.project_base();
    let existing_profile = OperatorSetupProfile::load(&project_base);

    let suggested_port = suggest_free_port(args.port)?;
    sink.status(&format!(
        "SymForge setup — OS: {} — suggested port: {suggested_port}",
        std::env::consts::OS
    ));

    for status in scan_harness_summary(&registry, None) {
        sink.status(&status);
    }

    let remembered = existing_profile.as_ref().map(|p| p.port);
    if let Some(port) = remembered {
        let addr = loopback_addr(port);
        if operator_server_reachable(addr, Duration::from_millis(500)) {
            sink.status(&format!(
                "Operator server already reachable on {addr} (profile port {port})"
            ));
        }
    }

    let installation_type = resolve_installation_type(&args, sink)?;
    let selected_ids = resolve_harness_ids(&args, &registry);
    let bind_port = args.port.unwrap_or(suggested_port);
    let bind_addr = loopback_addr(bind_port);

    let needs_server = matches!(
        installation_type,
        InstallationType::Server | InstallationType::Both
    );
    let needs_harness_http = installation_type == InstallationType::Both;

    let action_plan = build_action_plan(
        installation_type,
        &selected_ids,
        bind_addr,
        needs_server,
        needs_harness_http,
    );
    if !args.yes && !args.non_interactive {
        if !sink.confirm(&action_plan) {
            sink.status("Setup cancelled — no changes made.");
            return Ok(());
        }
    } else if !args.yes && args.non_interactive {
        anyhow::bail!("non-interactive setup requires `--yes` to confirm the action plan");
    } else if !args.non_interactive {
        sink.status(&action_plan);
    }

    if installation_type == InstallationType::InHarness {
        sink.status(
            "In-harness (stdio) mode: configure local MCP clients with `symforge init` per client.",
        );
    }

    let mut session: Option<ServerSessionDescriptor> = None;
    if needs_server {
        if let Some(port) = remembered {
            let addr = loopback_addr(port);
            if operator_server_reachable(addr, Duration::from_millis(500)) {
                session = Some(ServerSessionDescriptor::for_addr(addr, true));
            }
        }
        if session.is_none() {
            sink.status(&format!("Starting operator server on {bind_addr}…"));
            session = Some(start_operator_server(
                Some(bind_addr),
                None,
                None,
                Duration::from_secs(15),
            )?);
        }
        let desc = session.as_ref().expect("session started or reused");
        sink.status(&format!("Dashboard: {}", desc.dashboard_url));
        sink.status(&format!("Attach:    {}", desc.attach_url));
    }

    if needs_harness_http {
        let desc = session.as_ref().context(
            "internal error: harness HTTP attach requires a running operator server",
        )?;
        let attach_entry = AttachEntry::new(desc.attach_url.clone(), None);
        let subset = registry_subset(&registry, &selected_ids);
        let plan = harness_apply::plan(&subset, &attach_entry);
        sink.status("Applying harness attach entries (restorable backups)…");
        for change in &plan.changes {
            let label = match &change.action {
                PlannedAction::Add => "add",
                PlannedAction::Refresh => "refresh",
                PlannedAction::Skip(reason) => {
                    sink.status(&format!(
                        "  [skip] {} ({reason})",
                        change.id.display_name()
                    ));
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
        for outcome in harness_apply::apply(&plan) {
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

    if let Some(desc) = &session {
        let outcome = browser.open_url(&desc.dashboard_url);
        sink.status(&format!(
            "Browser: {:?} — open {}",
            outcome, desc.dashboard_url
        ));
    }

    sink.status("Setup complete.");
    Ok(())
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
    let addr = crate::cli::admin::select_free_addr_std_for_setup(preferred_addr)
        .map_err(|e| anyhow::anyhow!("could not find a free operator-server port: {e}"))?;
    Ok(addr.port())
}

fn scan_harness_summary(registry: &HarnessRegistry, attach: Option<&AttachEntry>) -> Vec<String> {
    let entry = attach.cloned().unwrap_or_else(|| AttachEntry::new("", None));
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

fn resolve_installation_type(args: &SetupCliArgs, sink: &mut dyn SetupSink) -> anyhow::Result<InstallationType> {
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
    let id_set: std::collections::HashSet<_> = ids.iter().copied().collect();
    let targets = registry
        .targets()
        .iter()
        .filter(|t| id_set.contains(&t.id))
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
    let mut lines = vec![format!("Planned actions ({installation_type:?}):")];
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
}
