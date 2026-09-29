//! Explicit npm-managed self-update command.
//!
//! `symforge update` must be safe to run while MCP harness sessions are open,
//! so it never terminates a stdio server and never lets npm write the path the
//! harnesses spawn. It orchestrates:
//!  1. a short circuit: when the binary at the install path already reports the
//!     latest published version the swap is skipped (verification still runs);
//!  2. a STAGED install: npm installs the new wrapper + OS-native platform
//!     package (`symforge-<os>-<arch>`) into a staging prefix beside the live
//!     install, and the staged binary must report the latest version before the
//!     live tree is touched;
//!  3. the swap: each staged file is renamed over its live counterpart. A
//!     running Windows image refuses replacement but allows a rename, so there
//!     the old binary is moved aside and the new one follows immediately. Live
//!     sessions keep executing from the moved file (Unix keeps the old inode);
//!  4. VERIFIES the resolved `symforge --version` reached the latest published
//!     version — and FAILS LOUDLY (stale nested package, a PATH-shadowing install,
//!     or a WSL Windows-prefix bleed) instead of a hollow success, even when the
//!     npm registry is unreachable (it floors against the running binary's version
//!     and surfaces a launcher that ran but could not resolve a binary);
//!  5. stops ONLY the daemon, right before the swap, and starts it again after it
//!     through the stdio client's own spawn path so the next session finds it warm,
//!     then replays a harness `initialize` against the new binary and fails loudly
//!     if it does not answer;
//!  6. re-registers ONLY the harnesses that already carry a SymForge entry,
//!     running the new binary's `init` from the home directory, never the
//!     caller's cwd; and
//!  7. only AFTER a confirmed re-registration, clears the retired `~/.symforge/bin`
//!     durable-install leftovers. Dead version-registry entries are pruned first.

use anyhow::{Context, bail};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

use crate::cli::harness::{AttachEntry, HarnessId, HarnessRegistry, HarnessState, HarnessStatus};
use crate::domain::ControlStateDir;

/// Hard ceiling on the staging `npm install` so a hung registry fetch can NEVER
/// hang the terminal. The live install is untouched while it runs. A plain
/// const, not config: it only needs to be "longer than a healthy install,
/// shorter than human patience".
const NPM_INSTALL_TIMEOUT: Duration = Duration::from_secs(180);
/// Poll cadence while waiting on the npm child.
const NPM_INSTALL_POLL: Duration = Duration::from_millis(200);
/// How long the new binary gets to answer a replayed harness `initialize`.
const INITIALIZE_VERIFY_TIMEOUT: Duration = Duration::from_secs(10);
/// The first request every MCP harness sends. It names a protocol revision
/// every SymForge release still serves, so the replay does not depend on the
/// newest one.
const INITIALIZE_REQUEST: &str = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"symforge-update-verify","version":"1"}}}"#;

/// Map `(os, arch)` to the npm platform package that ships the native binary.
/// Mirrors `SUPPORTED_TARGETS` in `npm/lib/resolve-binary.js`. `os` is
/// `std::env::consts::OS`, `arch` is `std::env::consts::ARCH`.
fn platform_package_for(os: &str, arch: &str) -> Option<&'static str> {
    match (os, arch) {
        ("windows", "x86_64") => Some("symforge-windows-x64"),
        ("linux", "x86_64") => Some("symforge-linux-x64"),
        ("macos", "aarch64") => Some("symforge-macos-arm64"),
        ("macos", "x86_64") => Some("symforge-macos-x64"),
        _ => None,
    }
}

fn npm_executable_for_os(os: &str) -> &'static str {
    if os == "windows" { "npm.cmd" } else { "npm" }
}

/// The resolved `symforge` launcher name for spawning the freshly-installed
/// binary (`.cmd` shim on Windows, bare name elsewhere).
fn symforge_launcher() -> &'static str {
    if std::env::consts::OS == "windows" {
        "symforge.cmd"
    } else {
        "symforge"
    }
}

/// Build the `npm install` package specs. Always installs the `symforge`
/// wrapper at `@latest`; when the OS/arch is known, also names the platform
/// package explicitly so npm materializes the new nested binary instead of
/// silently reusing a stale one.
fn install_specs(os: &str, arch: &str) -> Vec<String> {
    let mut specs = vec!["symforge@latest".to_string()];
    if let Some(pkg) = platform_package_for(os, arch) {
        specs.push(format!("{pkg}@latest"));
    }
    specs
}

/// Arguments for the staging install. It is a LOCAL install rooted at
/// `staging` (not `-g`), so the layout is `<staging>/node_modules/<pkg>` on
/// every OS and the live global tree is never written by npm.
fn staging_install_args(staging: &Path, specs: &[String]) -> Vec<String> {
    let mut args = vec![
        "install".to_string(),
        "--prefix".to_string(),
        staging.display().to_string(),
        "--no-save".to_string(),
    ];
    args.extend(specs.iter().cloned());
    args
}

/// File name of the native binary inside a platform package.
fn native_binary_name(os: &str) -> &'static str {
    if os == "windows" {
        "symforge.exe"
    } else {
        "symforge"
    }
}

/// Where npm keeps GLOBAL packages under `prefix`: `<prefix>/node_modules` on
/// Windows, `<prefix>/lib/node_modules` elsewhere.
fn global_modules_dir(prefix: &Path, os: &str) -> PathBuf {
    if os == "windows" {
        prefix.join("node_modules")
    } else {
        prefix.join("lib").join("node_modules")
    }
}

/// The native binary inside `<modules>/<platform package>` — for the live
/// tree, the exact path `symforge init` registers with every harness.
fn native_binary_in(modules: &Path, platform_package: &str, os: &str) -> PathBuf {
    modules
        .join(platform_package)
        .join("bin")
        .join(native_binary_name(os))
}

/// Scratch prefix the new packages install into before the swap: a sibling of
/// the global `node_modules`, so it is on the live install's volume (the swap is
/// a rename, never a copy) and outside every package dir npm manages.
fn update_staging_dir(npm_prefix: &Path) -> PathBuf {
    npm_prefix.join(".symforge-update-staging")
}

/// Parse a `symforge --version` semver out of arbitrary launcher output. Scans
/// EVERY line (not just the first) so a leading banner/notice on stdout does not
/// hide the version, and accepts the first digit-leading dotted token.
fn parse_symforge_version(text: &str) -> Option<String> {
    text.lines().find_map(|line| {
        line.split_whitespace()
            .map(str::trim)
            .find(|tok| tok.contains('.') && tok.chars().next().is_some_and(|c| c.is_ascii_digit()))
            .map(str::to_string)
    })
}

/// Outcome of probing a `symforge --version`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum InstalledProbe {
    /// The launcher ran and reported this version.
    Version(String),
    /// The launcher RAN but exited non-zero / printed no version — e.g. it could
    /// not resolve a native binary (a stale/missing platform package or the WSL
    /// Windows-prefix trap). Carries a trimmed diagnostic line from stderr.
    LauncherFailed(String),
    /// The launcher could not be executed at all (not on PATH / spawn error).
    Unprobeable,
}

/// Run `<program> --version` and classify the result. Inspects BOTH stdout and
/// the exit status: the npm launcher prints resolve errors to stderr and exits
/// non-zero with empty stdout, which must surface as a loud failure, not a
/// silent "could not probe".
fn probe_version(program: impl AsRef<std::ffi::OsStr>) -> InstalledProbe {
    let output = match crate::process_util::hidden_command(program)
        .arg("--version")
        .stdin(Stdio::null())
        .output()
    {
        Ok(output) => output,
        Err(_) => return InstalledProbe::Unprobeable,
    };
    if let Some(version) = parse_symforge_version(&String::from_utf8_lossy(&output.stdout)) {
        return InstalledProbe::Version(version);
    }
    // Ran but produced no version: surface its own diagnostic.
    let stderr = String::from_utf8_lossy(&output.stderr);
    let detail = stderr
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("launcher produced no version output")
        .to_string();
    InstalledProbe::LauncherFailed(detail)
}

/// Clear a demonstrably-dead sidecar record for the current project (CWD
/// `.symforge`). The sidecar is NOT killed: its pid is the in-process MCP server
/// (killing it would drop the user's editor connection), and a TCP-alive probe
/// does not prove the recorded pid owns the port (recycled-pid hazard). Only a
/// `Dead` record is cleaned, and "cleared" is reported only when a re-read
/// confirms it.
fn clear_dead_sidecar_record() -> Option<String> {
    use crate::sidecar::port_file::{
        SidecarLiveness, cleanup_files, cleanup_stale_descriptors, read_sidecar_status,
    };

    let control_state_dir: ControlStateDir = crate::version_registry::resolve_home()?;
    let project_root = std::env::current_dir().ok();
    let status = read_sidecar_status(&control_state_dir, "127.0.0.1", project_root.as_deref());
    // Task 8: purge stale per-adapter descriptors alongside the legacy files.
    cleanup_stale_descriptors(&control_state_dir, "127.0.0.1");
    if !matches!(status.liveness, SidecarLiveness::Dead) {
        return None;
    }
    cleanup_files(&control_state_dir);
    let after = read_sidecar_status(&control_state_dir, "127.0.0.1", project_root.as_deref());
    if matches!(after.liveness, SidecarLiveness::NoSidecar) {
        Some("cleared a stale sidecar record".to_string())
    } else {
        Some(format!(
            "skipped: clearing a stale sidecar record (it still reads as {})",
            after.liveness.as_str()
        ))
    }
}

/// Resolve the durable-install `bin` directory the same way the rest of SymForge
/// resolves its home: `$SYMFORGE_HOME/bin` when `SYMFORGE_HOME` is set (it is set
/// in the standard MCP server config and routinely points at the SAME default
/// `~/.symforge`), else `~/.symforge/bin`. Returns `None` only when neither is
/// resolvable.
/// Remove the retired durable-install artifacts under the resolved durable `bin`
/// directory (`$SYMFORGE_HOME/bin` when set, else `~/.symforge/bin`) — the only
/// place the retired durable mechanism ever wrote. The real safety invariant is
/// the self-exe guard: it never deletes the binary backing the current process.
/// Callers must only invoke this AFTER clients are re-registered off the orphan.
fn remove_orphan_durable_bin() -> Vec<String> {
    let Some(control_state_dir) = crate::version_registry::resolve_home() else {
        return Vec::new();
    };
    remove_orphan_durable_bin_at(&control_state_dir)
}

fn remove_orphan_durable_bin_at(control_state_dir: &crate::domain::ControlStateDir) -> Vec<String> {
    let bin = control_state_dir.as_path().join("bin");
    let self_exe = std::env::current_exe()
        .ok()
        .and_then(|p| std::fs::canonicalize(p).ok());

    let mut removed = Vec::new();
    let mut failed = Vec::new();
    for name in [
        "symforge.exe",
        "symforge",
        "symforge.version",
        "symforge-desktop.cmd",
    ] {
        let path = bin.join(name);
        if !path.exists() {
            continue;
        }
        // Never delete the binary backing the running update process.
        if let Some(self_exe) = &self_exe
            && std::fs::canonicalize(&path).ok().as_ref() == Some(self_exe)
        {
            continue;
        }
        match std::fs::remove_file(&path) {
            Ok(()) => removed.push(name.to_string()),
            Err(error) => failed.push(format!(
                "skipped: removing retired durable-install leftover {}: {error}",
                path.display()
            )),
        }
    }
    let mut lines = Vec::new();
    if !removed.is_empty() {
        lines.push(format!(
            "removed retired durable-install leftover(s) from {}: {}",
            bin.display(),
            removed.join(", ")
        ));
    }
    lines.extend(failed);
    lines
}

/// What the harness scan found: every registry target's state, plus whether a
/// Grok config exists (Grok is registrable by `init` but not covered by the
/// harness registry).
#[derive(Debug, Clone)]
pub(crate) struct HarnessScan {
    statuses: Vec<HarnessStatus>,
    grok_config_present: bool,
}

/// Decide which harnesses `update` re-registers. Only a harness that ALREADY
/// carries a `symforge` entry is touched, so update never creates config for a
/// client the user does not run. Kilo Code is excluded: its config is
/// project-local, and update never writes project-local files. Returns the
/// targets plus the `skipped:` lines to print.
fn plan_reregistration(scan: &HarnessScan) -> (Vec<HarnessId>, Vec<String>) {
    let mut targets = Vec::new();
    let mut skipped = Vec::new();
    for status in &scan.statuses {
        match &status.state {
            HarnessState::PresentCurrent | HarnessState::PresentStale(_)
                if status.id == HarnessId::KiloCode =>
            {
                skipped.push(format!(
                    "skipped: {} (its config is project-local; run `symforge init --client kilo-code` inside that project)",
                    status.id.display_name()
                ));
            }
            HarnessState::PresentCurrent | HarnessState::PresentStale(_) => {
                targets.push(status.id);
            }
            // The parse error text is deliberately not echoed: a TOML error
            // quotes the offending line, which can carry a bearer token.
            HarnessState::Malformed(_) => skipped.push(format!(
                "skipped: {} (its config {} does not parse; left untouched)",
                status.id.display_name(),
                status.config_path.display()
            )),
            HarnessState::NotInstalled | HarnessState::Absent => {}
        }
    }
    if scan.grok_config_present {
        skipped.push(
            "skipped: Grok (the harness scan does not cover it; run `symforge init --client grok` if Grok uses SymForge)"
                .to_string(),
        );
    }
    (targets, skipped)
}

/// What `symforge update` did, printed at the end (and before a post-swap
/// failure) so every step's outcome is visible — including every step it
/// skipped or could not complete.
#[derive(Debug, Default)]
struct UpdateSummary {
    old_version: String,
    new_version: String,
    /// The live binary already reported the latest version.
    up_to_date: bool,
    swapped: bool,
    reregistered: Vec<&'static str>,
    daemon: Option<Result<u16, String>>,
    initialize: Option<Result<(), String>>,
    notes: Vec<String>,
}

impl UpdateSummary {
    fn render(&self) -> String {
        let version = if self.swapped {
            format!("{} -> {}", self.old_version, self.new_version)
        } else if self.up_to_date {
            format!("{} (already the latest; swap skipped)", self.old_version)
        } else {
            format!("{} (unchanged)", self.old_version)
        };
        let reregistered = if self.reregistered.is_empty() {
            "none".to_string()
        } else {
            self.reregistered.join(", ")
        };
        let daemon = match (&self.daemon, self.swapped) {
            (Some(Ok(port)), true) => format!("yes (port {port})"),
            (Some(Ok(port)), false) => format!("no, not needed (running on port {port})"),
            (Some(Err(error)), _) => format!("no ({error})"),
            (None, _) => "no (not attempted)".to_string(),
        };
        let initialize = match &self.initialize {
            Some(Ok(())) => "yes".to_string(),
            Some(Err(error)) => format!("no ({error})"),
            None => "no (not attempted)".to_string(),
        };
        let mut out = format!(
            "symforge update summary:\n  version: {version}\n  re-registered: {reregistered}\n  daemon restarted: {daemon}\n  initialize verified: {initialize}"
        );
        for note in &self.notes {
            out.push_str("\n  ");
            out.push_str(note);
        }
        out
    }
}

fn probe_label(probe: &InstalledProbe) -> String {
    match probe {
        InstalledProbe::Version(version) => version.clone(),
        InstalledProbe::LauncherFailed(_) | InstalledProbe::Unprobeable => "unknown".to_string(),
    }
}

/// Side effects of an update, injected so the orchestration is unit-testable
/// without touching npm, the network, the daemon, or the filesystem.
pub(crate) trait UpdateOps {
    /// Sweep leftover `.old-*` files a PRIOR swap moved aside. One still held
    /// by a live session is reported as skipped and cleaned on a later run.
    /// Runs at the START of update. Returns summary lines.
    fn sweep_stale_staging(&mut self) -> Vec<String>;
    /// Stop the recorded daemon right before the swap: its records are what the
    /// new binary replaces, and the ownership gate that protects the stop can
    /// still identify it while its binary is in place. Returns a summary line.
    fn stop_daemon(&mut self) -> anyhow::Result<String>;
    /// Install `specs` into the staging prefix with `program` (npm). The live
    /// install is not touched. Returns `true` on success.
    fn stage_install(&mut self, program: &str, specs: &[String]) -> anyhow::Result<bool>;
    /// `--version` of the staged binary.
    fn staged_version(&mut self) -> InstalledProbe;
    /// Rename the staged packages' files over the live ones. Returns summary
    /// lines.
    fn swap_staged_into_place(&mut self) -> anyhow::Result<Vec<String>>;
    /// Probe the resolved `symforge --version` after install.
    fn installed_version(&mut self) -> InstalledProbe;
    /// Latest version published to the npm registry, or `None` when offline.
    fn latest_version(&mut self) -> Option<String>;
    /// Prune dead version-registry entries (paths whose binary was deleted while
    /// the drive is online) and clear a demonstrably-dead sidecar record. Runs
    /// UNCONDITIONALLY and early, so a failed update still cleans cruft.
    /// Returns summary lines (empty when nothing was pruned).
    fn prune_registry(&mut self) -> Vec<String>;
    /// Scan the known harness configs.
    fn harness_scan(&mut self) -> HarnessScan;
    /// Re-register one harness onto the freshly-installed binary by spawning
    /// the NEW binary's `init`.
    fn reregister(&mut self, harness: HarnessId) -> anyhow::Result<()>;
    /// Remove the retired durable-install leftovers ONLY when `reregistered` is
    /// true (otherwise clients still point at the orphan and deleting it would
    /// break them). Registry pruning is handled separately by [`UpdateOps::prune_registry`].
    /// Returns summary lines.
    fn reconcile_durable(&mut self, reregistered: bool) -> Vec<String>;
    /// Detect whether a DIFFERENT install shadows the binary npm just installed
    /// on `$PATH`. Derives "our binary" from the global npm prefix and compares
    /// it to the PATH-first `symforge`. Returns `None` when our install wins, the
    /// prefix is unresolvable, or no shadow exists. This is ADDITIVE to the
    /// reactive stale-version bail: it also fires when the shadow is the SAME
    /// version (which the stale-version check cannot see).
    fn shadow_report(&mut self) -> Option<crate::path_shadow::ShadowReport>;
    /// `--version` of the binary at the live install path (the one harnesses
    /// spawn), probed directly rather than through the PATH launcher.
    fn live_version(&mut self) -> InstalledProbe;
    /// Make sure a daemon of `version`, running from the live install path, owns
    /// the daemon records; replaces an older recorded daemon. Returns its port.
    fn restart_daemon(&mut self, version: &str) -> anyhow::Result<u16>;
    /// Replay a harness `initialize` against the live binary over stdio.
    fn verify_initialize(&mut self) -> anyhow::Result<()>;
}

struct RealUpdateOps {
    npm_prefix: PathBuf,
    live_modules: PathBuf,
    staging: PathBuf,
    platform_package: &'static str,
    /// The binary every harness registration points at.
    live_binary: PathBuf,
    staged_binary: PathBuf,
    home: PathBuf,
}

impl RealUpdateOps {
    fn new(os: &str, platform_package: &'static str, npm_prefix: PathBuf, home: PathBuf) -> Self {
        let live_modules = global_modules_dir(&npm_prefix, os);
        let staging = update_staging_dir(&npm_prefix);
        Self {
            live_binary: native_binary_in(&live_modules, platform_package, os),
            staged_binary: native_binary_in(&staging.join("node_modules"), platform_package, os),
            npm_prefix,
            live_modules,
            staging,
            platform_package,
            home,
        }
    }
}

impl UpdateOps for RealUpdateOps {
    fn sweep_stale_staging(&mut self) -> Vec<String> {
        sweep_stale_dir(&stale_staging_dir(&self.npm_prefix))
    }

    fn stop_daemon(&mut self) -> anyhow::Result<String> {
        use crate::daemon::DaemonStopOutcome;
        // `main()` is synchronous, so a short-lived runtime is safe (no
        // nested-runtime panic).
        let outcome = tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .enable_time()
            .build()
            .context("building a runtime for the daemon stop")?
            .block_on(crate::daemon::stop_running_daemon_for_update())?;
        Ok(match outcome {
            DaemonStopOutcome::NotRunning => "no running daemon to stop".to_string(),
            DaemonStopOutcome::Stopped { pid } => format!("stopped the daemon (pid {pid})"),
            DaemonStopOutcome::StopTimedOut { pid } => format!(
                "skipped: stopping the daemon (pid {pid} did not exit in time; the restart replaces it)"
            ),
            DaemonStopOutcome::SkippedSafety => "skipped: stopping the daemon (its record failed \
                 the ownership check and it was left running)"
                .to_string(),
        })
    }

    fn stage_install(&mut self, program: &str, specs: &[String]) -> anyhow::Result<bool> {
        // A leftover from an interrupted run would be overlaid onto the live
        // install along with the fresh files, so start from an empty staging dir.
        match std::fs::remove_dir_all(&self.staging) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("clearing the staging directory {}", self.staging.display())
                });
            }
        }
        let args = staging_install_args(&self.staging, specs);
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        // Bounded wait: spawn + poll `try_wait` against a deadline and kill the
        // child on timeout, returning Ok(false) so the caller bails instead of
        // hanging.
        let mut child = crate::process_util::hidden_command(program)
            .args(&args)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()
            .with_context(|| format!("failed to start `{}`", invocation_text(program, &args)))?;
        wait_or_kill(
            &mut child,
            Instant::now() + NPM_INSTALL_TIMEOUT,
            NPM_INSTALL_POLL,
        )
    }

    fn staged_version(&mut self) -> InstalledProbe {
        probe_version(&self.staged_binary)
    }

    fn swap_staged_into_place(&mut self) -> anyhow::Result<Vec<String>> {
        let staged_modules = self.staging.join("node_modules");
        let aside = stale_staging_dir(&self.npm_prefix);
        // The platform package first: it holds the binary harnesses spawn.
        for package in [self.platform_package, "symforge"] {
            overlay_package(
                &staged_modules.join(package),
                &self.live_modules.join(package),
                &aside,
            )
            .with_context(|| format!("swapping the staged `{package}` package into place"))?;
        }
        Ok(match std::fs::remove_dir_all(&self.staging) {
            Ok(()) => Vec::new(),
            Err(error) => vec![format!(
                "skipped: removing the staging directory {}: {error}",
                self.staging.display()
            )],
        })
    }

    fn installed_version(&mut self) -> InstalledProbe {
        // This update process is still the OLD binary, so ask the launcher what
        // it now resolves to.
        probe_version(symforge_launcher())
    }

    fn latest_version(&mut self) -> Option<String> {
        crate::cli::version::latest_npm_version()
    }

    fn prune_registry(&mut self) -> Vec<String> {
        let mut lines: Vec<String> = clear_dead_sidecar_record().into_iter().collect();
        let Some(home) = crate::version_registry::resolve_home() else {
            return lines;
        };
        let pruned = crate::version_registry::prune_missing_entries(&home);
        if pruned > 0 {
            lines.push(format!(
                "pruned {pruned} stale version-registry entr{}",
                if pruned == 1 { "y" } else { "ies" }
            ));
        }
        lines
    }

    fn harness_scan(&mut self) -> HarnessScan {
        // The working dir only feeds the project-local Kilo Code target, which
        // `plan_reregistration` never re-registers; home keeps the caller's cwd
        // out of it. Only presence matters here, so the desired attach entry is
        // a placeholder: a stdio entry reads as present-stale.
        let registry = HarnessRegistry::known_with(&self.home, &self.home);
        HarnessScan {
            statuses: registry.scan(&AttachEntry::new("", None)),
            grok_config_present: self.home.join(".grok").join("config.toml").exists(),
        }
    }

    fn reregister(&mut self, harness: HarnessId) -> anyhow::Result<()> {
        // The NEW binary writes the registration (this process is still the old
        // one). It runs from the home directory with no workspace override, so
        // no project-local file is written and the caller's cwd is never
        // captured as a workspace root.
        let status = crate::process_util::hidden_command(&self.live_binary)
            .args(["init", "--client", harness.slug()])
            .current_dir(&self.home)
            .env_remove(crate::discovery::WORKSPACE_ROOT_ENV)
            .stdin(Stdio::null())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .with_context(|| {
                format!(
                    "starting `{} init --client {}`",
                    self.live_binary.display(),
                    harness.slug()
                )
            })?;
        anyhow::ensure!(
            status.success(),
            "`symforge init --client {}` exited with {status}",
            harness.slug()
        );
        Ok(())
    }

    fn reconcile_durable(&mut self, reregistered: bool) -> Vec<String> {
        if reregistered {
            remove_orphan_durable_bin()
        } else {
            Vec::new()
        }
    }

    fn shadow_report(&mut self) -> Option<crate::path_shadow::ShadowReport> {
        let installed = npm_installed_launcher_path()?;
        crate::path_shadow::detect_shadow(&installed)
    }

    fn live_version(&mut self) -> InstalledProbe {
        probe_version(&self.live_binary)
    }

    fn restart_daemon(&mut self, version: &str) -> anyhow::Result<u16> {
        // `main()` is synchronous, so a short-lived runtime is safe (no
        // nested-runtime panic).
        tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .enable_time()
            .build()
            .context("building a runtime for the daemon restart")?
            .block_on(crate::daemon::ensure_installed_daemon_running(
                &self.live_binary,
                version,
            ))
    }

    fn verify_initialize(&mut self) -> anyhow::Result<()> {
        verify_initialize_at(&self.live_binary, &self.home, INITIALIZE_VERIFY_TIMEOUT)
    }
}

/// Replay a harness's first exchange against `binary` over stdio: send
/// `initialize` and require a JSON-RPC result for it within `timeout`. Runs from
/// `cwd` with no workspace override, so the probe binds no project.
fn verify_initialize_at(binary: &Path, cwd: &Path, timeout: Duration) -> anyhow::Result<()> {
    let mut child = crate::process_util::hidden_command(binary)
        .current_dir(cwd)
        .env_remove(crate::discovery::WORKSPACE_ROOT_ENV)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("starting {} as a stdio MCP server", binary.display()))?;
    let (Some(mut stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
        child
            .kill()
            .context("stopping the `initialize` probe server")?;
        bail!("stdio pipes were not attached to the probe server");
    };
    let (lines_tx, lines_rx) = std::sync::mpsc::channel();
    // The reader ends when the child's stdout closes (the kill below).
    std::thread::spawn(move || {
        for line in std::io::BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            if lines_tx.send(line).is_err() {
                break;
            }
        }
    });

    let answered = writeln!(stdin, "{INITIALIZE_REQUEST}")
        .and_then(|()| stdin.flush())
        .context("writing `initialize` to the probe server")
        .and_then(|()| {
            let deadline = Instant::now() + timeout;
            loop {
                let remaining = deadline.saturating_duration_since(Instant::now());
                match lines_rx.recv_timeout(remaining) {
                    Ok(line) if is_initialize_result(&line) => break Ok(()),
                    Ok(_) => {}
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                        break Err(anyhow::anyhow!(
                            "no `initialize` result within {}s",
                            timeout.as_secs()
                        ));
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                        break Err(anyhow::anyhow!(
                            "the server closed stdout before answering `initialize`"
                        ));
                    }
                }
            }
        });
    drop(stdin);
    child
        .kill()
        .context("stopping the `initialize` probe server")?;
    child
        .wait()
        .context("reaping the `initialize` probe server")?;
    answered
}

/// A JSON-RPC success response to the replayed `initialize` (id 1).
fn is_initialize_result(line: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(line)
        .is_ok_and(|message| message["id"] == 1 && message["result"]["protocolVersion"].is_string())
}

/// A child process we can poll for completion and force-kill. The seam exists so
/// [`wait_or_kill`]'s timeout logic is unit-testable with a fake that never
/// exits, without spawning a real slow process.
trait Waitable {
    fn try_wait(&mut self) -> std::io::Result<Option<std::process::ExitStatus>>;
    fn kill(&mut self) -> std::io::Result<()>;
}

impl Waitable for std::process::Child {
    fn try_wait(&mut self) -> std::io::Result<Option<std::process::ExitStatus>> {
        std::process::Child::try_wait(self)
    }
    fn kill(&mut self) -> std::io::Result<()> {
        std::process::Child::kill(self)
    }
}

/// Poll `child` until it exits or `deadline` passes. On exit, return whether it
/// succeeded; on timeout, kill it and return `Ok(false)` so the caller takes the
/// existing graceful-bail path. The npm swap must NEVER block indefinitely — a
/// locked-file retry loop was the original Windows hang.
fn wait_or_kill<C: Waitable>(
    child: &mut C,
    deadline: Instant,
    poll: Duration,
) -> anyhow::Result<bool> {
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status.success());
        }
        if Instant::now() >= deadline {
            child.kill().context("killing the timed-out npm install")?;
            child.try_wait().context("reaping the killed npm install")?;
            return Ok(false);
        }
        std::thread::sleep(poll);
    }
}

/// The dedicated stale-staging directory for moved-aside binaries:
/// `<npm-global-prefix>/.symforge-update-stale`, a sibling of the GLOBAL
/// `node_modules`. Derived from the npm global prefix (`npm prefix -g`), NEVER by
/// walking `current_exe()`'s `node_modules` ancestors: a nested layout
/// (`.../node_modules/symforge/node_modules/symforge-windows-x64/bin/...`) would
/// resolve INSIDE the wrapper package npm rimrafs, and a divergent prefix (WSL
/// bleed) would free the wrong tree. It sits outside every package dir (npm may
/// rimraf `node_modules/*` on reinstall, so a still-locked `.old` must not live
/// inside it) and, in the normal install, on the same volume as the binary (so
/// the move is a rename, not a cross-volume copy).
fn stale_staging_dir(npm_prefix: &std::path::Path) -> std::path::PathBuf {
    npm_prefix.join(".symforge-update-stale")
}

/// Best-effort delete of every staged leftover in `dir` (files a prior swap
/// moved aside). Uses `remove_file` only: a file still held by a live old
/// session errors and is reported as skipped (cleaned on a later run once that
/// session exits). A nonexistent `dir` is a no-op.
fn sweep_stale_dir(dir: &std::path::Path) -> Vec<String> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
        Err(error) => return vec![format!("skipped: sweeping {}: {error}", dir.display())],
    };
    let mut removed = 0usize;
    let mut kept = 0usize;
    for entry in entries {
        match entry.map(|entry| std::fs::remove_file(entry.path())) {
            Ok(Ok(())) => removed += 1,
            Ok(Err(_)) | Err(_) => kept += 1,
        }
    }
    let mut lines = Vec::new();
    if removed > 0 {
        lines.push(format!(
            "swept {removed} stale staged binar{} from {}",
            if removed == 1 { "y" } else { "ies" },
            dir.display()
        ));
    }
    if kept > 0 {
        lines.push(format!(
            "skipped: {kept} leftover(s) in {} could not be removed (likely still running from an open session; a later update removes them)",
            dir.display()
        ));
    }
    lines
}

/// Rename every file under `staged` over its counterpart under `live`, creating
/// directories as needed. Files present only in `live` are left in place.
// ponytail: overlay, not a mirror; delete live-only files if a release ever
// drops a file that must not linger.
fn overlay_package(staged: &Path, live: &Path, aside_dir: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(live).with_context(|| format!("creating {}", live.display()))?;
    for entry in
        std::fs::read_dir(staged).with_context(|| format!("reading {}", staged.display()))?
    {
        let entry = entry.with_context(|| format!("reading {}", staged.display()))?;
        let from = entry.path();
        let to = live.join(entry.file_name());
        if entry
            .file_type()
            .with_context(|| format!("inspecting {}", from.display()))?
            .is_dir()
        {
            overlay_package(&from, &to, aside_dir)?;
        } else {
            replace_file(&from, &to, aside_dir, |src, dst| std::fs::rename(src, dst))
                .with_context(|| format!("moving {} to {}", from.display(), to.display()))?;
        }
    }
    Ok(())
}

/// Move `src` over `dst` so that `dst` holds either the old or the new file at
/// every step except, on Windows, between two back-to-back renames. A plain
/// rename replaces atomically everywhere except over a running Windows image,
/// which refuses replacement (`PermissionDenied`) yet allows being renamed:
/// there `dst` goes aside into `aside_dir` and `src` follows immediately. If
/// that second rename fails, the old file is moved back. `rename` is injected
/// so the move-back path is testable.
fn replace_file(
    src: &Path,
    dst: &Path,
    aside_dir: &Path,
    rename: impl Fn(&Path, &Path) -> std::io::Result<()>,
) -> std::io::Result<()> {
    let refused = match rename(src, dst) {
        Ok(()) => return Ok(()),
        Err(error) => error,
    };
    if refused.kind() != std::io::ErrorKind::PermissionDenied || !dst.exists() {
        return Err(refused);
    }
    std::fs::create_dir_all(aside_dir)?;
    let name = dst
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let aside = aside_dir.join(format!("{name}.old-{}", stage_suffix()));
    move_path_aside(dst, &aside)?;
    if let Err(error) = rename(src, dst) {
        return match move_path_aside(&aside, dst) {
            Ok(()) => Err(error),
            Err(restore) => Err(std::io::Error::other(format!(
                "{error}; moving the previous file back also failed ({restore}), it is at {}",
                aside.display()
            ))),
        };
    }
    Ok(())
}

/// A random-enough suffix for a staged `.old-*` filename: pid + wall-clock nanos
/// (no `rand` dependency needed — collisions only need avoiding across a rare
/// concurrent or repeated update, and the sweep tolerates leftovers anyway).
fn stage_suffix() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{}-{nanos}", std::process::id())
}

/// Move `src` to `dst`, freeing `src` for npm. `std::fs::rename` (which maps to
/// `MoveFileExW`) handles the common case; fall back to an explicit
/// `MoveFileExW(MOVEFILE_REPLACE_EXISTING)` on error. A running `.exe` can be
/// renamed aside (DELETE access) even though it cannot be overwritten.
#[cfg(windows)]
fn move_path_aside(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    match std::fs::rename(src, dst) {
        Ok(()) => Ok(()),
        Err(_) => windows_native::move_file_replace_existing(src, dst),
    }
}

/// Unix renames over a running binary directly, so [`replace_file`] only moves
/// a file aside on the Windows refusal path; a plain rename suffices here.
#[cfg(not(windows))]
fn move_path_aside(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    std::fs::rename(src, dst)
}

/// Resolve the npm global prefix via `npm prefix -g`. This is the authoritative
/// install root (the parent of the GLOBAL `node_modules`); staging and sweep
/// derive their directory from it, and shadow detection derives the launcher
/// path from it. Returns `None` when `npm` is unavailable or the prefix cannot
/// be parsed, or when it names no existing directory (npm masks UUID-shaped
/// path segments as `***` in its output).
fn npm_global_prefix() -> Option<std::path::PathBuf> {
    let program = npm_executable_for_os(std::env::consts::OS);
    let output = crate::process_util::hidden_command(program)
        .args(["prefix", "-g"])
        .stdin(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let prefix = String::from_utf8_lossy(&output.stdout);
    let prefix = prefix.trim();
    if prefix.is_empty() {
        return None;
    }
    let prefix = std::path::PathBuf::from(prefix);
    prefix.is_dir().then_some(prefix)
}

/// Derive the path of the `symforge` launcher npm installs at the global prefix.
/// On Windows the shim lives at the prefix root (`<prefix>/symforge.cmd`); on
/// Unix it lives in `<prefix>/bin/symforge`. Returns `None` when the prefix
/// cannot be resolved.
fn npm_installed_launcher_path() -> Option<std::path::PathBuf> {
    Some(launcher_path_in_prefix(
        &npm_global_prefix()?,
        std::env::consts::OS,
    ))
}

/// Pure mapping from an npm global prefix to the `symforge` launcher path it
/// installs, given the target OS. Windows places the shim at the prefix root;
/// every other platform uses the conventional `<prefix>/bin/<name>` layout.
fn launcher_path_in_prefix(prefix: &std::path::Path, os: &str) -> std::path::PathBuf {
    if os == "windows" {
        prefix.join("symforge.cmd")
    } else {
        prefix.join("bin").join("symforge")
    }
}

pub fn run_update() -> anyhow::Result<()> {
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    let platform_package = platform_package_for(os, arch).with_context(|| {
        format!("symforge update: no npm platform package ships a binary for {os}-{arch}")
    })?;
    let npm_prefix = npm_global_prefix().context(
        "symforge update: could not resolve the npm global prefix (`npm prefix -g` failed or \
         named no existing directory); is npm on PATH?",
    )?;
    let home = dirs::home_dir().context("cannot determine home directory")?;
    let mut ops = RealUpdateOps::new(os, platform_package, npm_prefix, home);
    orchestrate_update(os, arch, &mut ops)
}

pub(crate) fn orchestrate_update(
    os: &str,
    arch: &str,
    ops: &mut impl UpdateOps,
) -> anyhow::Result<()> {
    let mut summary = UpdateSummary::default();
    let result = update_steps(os, arch, ops, &mut summary);
    // Printed on success AND failure, so no skipped step goes unreported.
    eprintln!("{}", summary.render());
    result
}

fn update_steps(
    os: &str,
    arch: &str,
    ops: &mut impl UpdateOps,
    summary: &mut UpdateSummary,
) -> anyhow::Result<()> {
    // Clean cruft from prior runs first, whatever happens to the swap below.
    summary.notes.extend(ops.sweep_stale_staging());
    summary.notes.extend(ops.prune_registry());

    let latest = ops.latest_version();
    if latest.is_none() {
        summary.notes.push(
            "skipped: confirming the latest published version (the npm registry was unreachable)"
                .to_string(),
        );
    }
    let before = ops.live_version();
    summary.old_version = probe_label(&before);
    let already_latest =
        matches!((&before, &latest), (InstalledProbe::Version(v), Some(l)) if v == l);

    summary.up_to_date = already_latest;
    if already_latest {
        eprintln!(
            "symforge update: {} is already the latest published version; skipping the swap.",
            summary.old_version
        );
    } else {
        let program = npm_executable_for_os(os);
        let specs = install_specs(os, arch);
        let plain_cmd = format!("npm install -g {}", specs.join(" "));
        eprintln!(
            "symforge update: installing {} into a staging directory; running sessions are not touched.",
            specs.join(" ")
        );
        if !ops.stage_install(program, &specs)? {
            bail!(
                "symforge update failed: the staging install (`{program} install ... {}`) exited \
                 unsuccessfully or timed out. The live install was NOT touched and running sessions \
                 are unaffected. Retry, or install from a plain shell:\n  {plain_cmd}\n\
                 (The version registry was already pruned.)",
                specs.join(" ")
            );
        }
        let untouched = "The live install was NOT touched.";
        match ops.staged_version() {
            InstalledProbe::Version(staged) => {
                if let Some(latest) = &latest
                    && &staged != latest
                {
                    bail!(
                        "symforge update failed: the staged binary reports {staged}, not the latest \
                         published {latest}. {untouched}"
                    );
                }
            }
            InstalledProbe::LauncherFailed(detail) => bail!(
                "symforge update failed: the staged binary did not report a version:\n  {detail}\n{untouched}"
            ),
            InstalledProbe::Unprobeable => {
                bail!("symforge update failed: the staged binary could not be run. {untouched}")
            }
        }
        // Only the daemon is stopped; stdio sessions keep running from the old
        // binary. A failed stop is not fatal: the restart below replaces an
        // older daemon it finds.
        summary.notes.push(match ops.stop_daemon() {
            Ok(line) => line,
            Err(error) => format!("skipped: stopping the daemon: {error:#}"),
        });
        summary.notes.extend(ops.swap_staged_into_place().context(
            "symforge update failed while swapping the staged files into place; the binary path \
             holds either the old or the new binary, rerun `symforge update` to finish",
        )?);
        summary.swapped = true;
    }

    // What harnesses will spawn from now on.
    let new_version = match ops.live_version() {
        InstalledProbe::Version(version) => version,
        InstalledProbe::LauncherFailed(detail) => bail!(
            "symforge update incomplete: the binary at the install path did not report a version:\n  {detail}"
        ),
        InstalledProbe::Unprobeable => {
            bail!("symforge update incomplete: the binary at the install path could not be run")
        }
    };
    summary.new_version = new_version.clone();

    // Verify the install actually took effect for the PATH launcher too. npm can
    // leave the resolved binary behind, so this is the load-bearing safety net.
    let running = env!("CARGO_PKG_VERSION");
    let pkg = platform_package_for(os, arch).unwrap_or("symforge-<os>-<arch>");
    match ops.installed_version() {
        InstalledProbe::LauncherFailed(detail) => {
            bail!(
                "symforge update incomplete: the freshly-installed `symforge` launcher could not \
                 resolve a native binary. Launcher reported:\n  {detail}\n\
                 This is usually a stale/missing platform package or a WSL Windows-prefix bleed. Try: \
                 npm install -g symforge@latest {pkg}@latest --force; on WSL, ensure your Linux npm \
                 prefix bin is on PATH ahead of /usr/local and /mnt."
            );
        }
        InstalledProbe::Unprobeable => {
            summary.notes.push(
                "skipped: verifying `symforge --version` through the launcher on PATH (it could not \
                 be run; the npm prefix bin may not be on PATH)"
                    .to_string(),
            );
        }
        InstalledProbe::Version(installed) => {
            // When the registry is reachable, require the resolved binary to be
            // at the latest; otherwise floor against the version of the binary
            // running this update — the swap must produce something at least as
            // new, or it demonstrably did not take effect.
            let (stale, target) = match &latest {
                Some(l) => (
                    crate::cli::version::is_newer_version(l, &installed),
                    l.clone(),
                ),
                None => (
                    crate::cli::version::is_newer_version(running, &installed),
                    running.to_string(),
                ),
            };
            if stale {
                bail!(
                    "symforge update incomplete: `symforge --version` still reports {installed}, \
                     behind {target}. The resolved `symforge` is not the one just installed. Likely causes:\n  \
                     - stale nested platform package — retry: npm install -g symforge@latest {pkg}@latest --force\n  \
                     - a PATH-shadowing install — run `which -a symforge`; a root /usr/local copy can win over your npm prefix\n  \
                     - on WSL, a Windows npm prefix bleeding in via /mnt — put your Linux npm prefix bin ahead of /usr/local and /mnt on PATH"
                );
            }
        }
    }

    // Proactive PATH-shadow check: the version check above cannot see a
    // same-version shadow or name the exact offending path and fix.
    if let Some(report) = ops.shadow_report() {
        eprintln!("{}", crate::path_shadow::format_shadow_warning(&report));
    }

    summary.daemon = Some(
        ops.restart_daemon(&new_version)
            .map_err(|error| format!("{error:#}")),
    );
    if let Err(error) = ops.verify_initialize() {
        summary.initialize = Some(Err(format!("{error:#}")));
        summary.notes.push(
            "skipped: harness re-registration (the new binary failed verification)".to_string(),
        );
        return Err(error.context(
            "symforge update failed: the binary at the install path did not answer a harness \
             `initialize`; new sessions will fail until this is fixed",
        ));
    }
    summary.initialize = Some(Ok(()));

    // Re-point the harnesses that already use SymForge. Only after every one of
    // them succeeded are the retired durable-install artifacts cleared —
    // deleting the orphan while a client still references it would break it.
    let (targets, skipped) = plan_reregistration(&ops.harness_scan());
    summary.notes.extend(skipped);
    let mut all_reregistered = true;
    for harness in targets {
        match ops.reregister(harness) {
            Ok(()) => summary.reregistered.push(harness.display_name()),
            Err(error) => {
                all_reregistered = false;
                summary.notes.push(format!(
                    "skipped: re-registering {}: {error:#}",
                    harness.display_name()
                ));
            }
        }
    }
    if !all_reregistered {
        summary.notes.push(
            "skipped: removing retired durable-install leftovers (a re-registration failed)"
                .to_string(),
        );
    }
    summary
        .notes
        .extend(ops.reconcile_durable(all_reregistered));
    Ok(())
}

fn invocation_text(program: &str, args: &[&str]) -> String {
    let mut parts = Vec::with_capacity(args.len() + 1);
    parts.push(program.to_string());
    parts.extend(args.iter().map(|arg| (*arg).to_string()));
    parts.join(" ")
}

/// Native Win32 fallback for moving a file aside. `unsafe` here is FFI-only,
/// justified by a SAFETY comment; the crate-level `unsafe_code = "deny"` is
/// opted out per-item, matching the repo's test-env precedent.
#[cfg(windows)]
mod windows_native {
    /// Build a NUL-terminated UTF-16 buffer for a Win32 wide-string path arg.
    fn to_wide(path: &std::path::Path) -> Vec<u16> {
        use std::os::windows::ffi::OsStrExt;
        path.as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    /// Move `src` to `dst` via `MoveFileExW(MOVEFILE_REPLACE_EXISTING)` — the
    /// fallback when `std::fs::rename` fails. Renaming a running `.exe` aside
    /// needs only DELETE access, so it succeeds where an overwrite-in-place is
    /// refused. Same-volume only (no `COPY_ALLOWED`): a locked image can be
    /// renamed within its volume but never block-copied across one.
    #[allow(unsafe_code)]
    pub(super) fn move_file_replace_existing(
        src: &std::path::Path,
        dst: &std::path::Path,
    ) -> std::io::Result<()> {
        use windows::Win32::Storage::FileSystem::{MOVEFILE_REPLACE_EXISTING, MoveFileExW};
        let src_w = to_wide(src);
        let dst_w = to_wide(dst);
        // SAFETY: `src_w`/`dst_w` are NUL-terminated UTF-16 buffers that outlive
        // this call; the PCWSTR wrappers only borrow their pointers for the
        // duration of MoveFileExW. MOVEFILE_REPLACE_EXISTING is a valid flag; the
        // call returns a Result we map into an io::Error.
        unsafe {
            MoveFileExW(
                windows::core::PCWSTR(src_w.as_ptr()),
                windows::core::PCWSTR(dst_w.as_ptr()),
                MOVEFILE_REPLACE_EXISTING,
            )
        }
        .map_err(|e| std::io::Error::other(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Records every side effect in order so tests can assert sequencing.
    struct FakeOps {
        events: Vec<String>,
        latest: Option<String>,
        /// Successive `live_version` answers: before the swap, then after.
        live: Vec<InstalledProbe>,
        stage_result: bool,
        staged: InstalledProbe,
        stop: Result<String, String>,
        installed: InstalledProbe,
        daemon: Result<u16, String>,
        initialize: Result<(), String>,
        scan: Vec<(HarnessId, HarnessState)>,
        reregister_fails: Vec<HarnessId>,
        reconciled_with: Option<bool>,
        shadow: Option<crate::path_shadow::ShadowReport>,
    }

    impl Default for FakeOps {
        fn default() -> Self {
            Self {
                events: Vec::new(),
                latest: Some("7.15.4".to_string()),
                live: vec![
                    InstalledProbe::Version("7.15.3".to_string()),
                    InstalledProbe::Version("7.15.4".to_string()),
                ],
                stage_result: true,
                staged: InstalledProbe::Version("7.15.4".to_string()),
                stop: Ok("stopped the daemon (pid 7)".to_string()),
                installed: InstalledProbe::Version("7.15.4".to_string()),
                daemon: Ok(4242),
                initialize: Ok(()),
                scan: vec![
                    (HarnessId::ClaudeCode, HarnessState::PresentCurrent),
                    (HarnessId::Cursor, HarnessState::NotInstalled),
                ],
                reregister_fails: Vec::new(),
                reconciled_with: None,
                shadow: None,
            }
        }
    }

    impl FakeOps {
        fn happened(&self, event: &str) -> bool {
            self.events.iter().any(|e| e == event)
        }
        fn position(&self, event: &str) -> usize {
            self.events
                .iter()
                .position(|e| e == event)
                .unwrap_or_else(|| panic!("{event} never happened: {:?}", self.events))
        }
    }

    impl UpdateOps for FakeOps {
        fn sweep_stale_staging(&mut self) -> Vec<String> {
            self.events.push("sweep".into());
            Vec::new()
        }
        fn stop_daemon(&mut self) -> anyhow::Result<String> {
            self.events.push("stop-daemon".into());
            self.stop.clone().map_err(|e| anyhow::anyhow!(e))
        }
        fn stage_install(&mut self, program: &str, specs: &[String]) -> anyhow::Result<bool> {
            self.events
                .push(format!("stage {program} {}", specs.join(" ")));
            Ok(self.stage_result)
        }
        fn staged_version(&mut self) -> InstalledProbe {
            self.staged.clone()
        }
        fn swap_staged_into_place(&mut self) -> anyhow::Result<Vec<String>> {
            self.events.push("swap".into());
            Ok(Vec::new())
        }
        fn installed_version(&mut self) -> InstalledProbe {
            self.installed.clone()
        }
        fn latest_version(&mut self) -> Option<String> {
            self.latest.clone()
        }
        fn prune_registry(&mut self) -> Vec<String> {
            self.events.push("prune".into());
            Vec::new()
        }
        fn harness_scan(&mut self) -> HarnessScan {
            scan_of(&self.scan)
        }
        fn reregister(&mut self, harness: HarnessId) -> anyhow::Result<()> {
            self.events.push(format!("reregister {}", harness.slug()));
            if self.reregister_fails.contains(&harness) {
                bail!("init exited with 1");
            }
            Ok(())
        }
        fn reconcile_durable(&mut self, reregistered: bool) -> Vec<String> {
            self.reconciled_with = Some(reregistered);
            Vec::new()
        }
        fn shadow_report(&mut self) -> Option<crate::path_shadow::ShadowReport> {
            self.events.push("shadow".into());
            self.shadow.clone()
        }
        fn live_version(&mut self) -> InstalledProbe {
            self.events.push("live-probe".into());
            if self.live.len() > 1 {
                self.live.remove(0)
            } else {
                self.live[0].clone()
            }
        }
        fn restart_daemon(&mut self, version: &str) -> anyhow::Result<u16> {
            self.events.push(format!("daemon {version}"));
            self.daemon.clone().map_err(|e| anyhow::anyhow!(e))
        }
        fn verify_initialize(&mut self) -> anyhow::Result<()> {
            self.events.push("initialize".into());
            self.initialize.clone().map_err(|e| anyhow::anyhow!(e))
        }
    }

    fn scan_of(states: &[(HarnessId, HarnessState)]) -> HarnessScan {
        HarnessScan {
            statuses: states
                .iter()
                .map(|(id, state)| HarnessStatus {
                    id: *id,
                    config_path: PathBuf::from(format!("/home/you/{}.json", id.slug())),
                    format: crate::cli::harness::HarnessFormat::Json,
                    state: state.clone(),
                })
                .collect(),
            grok_config_present: false,
        }
    }

    #[test]
    fn npm_executable_is_npm_cmd_on_windows_else_npm() {
        assert_eq!(npm_executable_for_os("windows"), "npm.cmd");
        assert_eq!(npm_executable_for_os("linux"), "npm");
        assert_eq!(npm_executable_for_os("macos"), "npm");
    }

    #[test]
    fn install_specs_force_the_os_native_platform_package() {
        assert_eq!(
            install_specs("windows", "x86_64"),
            vec!["symforge@latest", "symforge-windows-x64@latest"]
        );
        assert_eq!(
            install_specs("linux", "x86_64"),
            vec!["symforge@latest", "symforge-linux-x64@latest"]
        );
        assert_eq!(
            install_specs("macos", "aarch64"),
            vec!["symforge@latest", "symforge-macos-arm64@latest"]
        );
    }

    #[test]
    fn install_specs_falls_back_to_wrapper_only_for_unknown_target() {
        assert_eq!(install_specs("linux", "riscv64"), vec!["symforge@latest"]);
        assert_eq!(install_specs("plan9", "x86_64"), vec!["symforge@latest"]);
    }

    #[test]
    fn parse_symforge_version_scans_all_lines_past_leading_banner() {
        assert_eq!(
            parse_symforge_version("symforge 7.15.4\nUpdate available: 7.16.0"),
            Some("7.15.4".to_string())
        );
        // A leading banner/notice line must not hide the version.
        assert_eq!(
            parse_symforge_version("(node:1) ExperimentalWarning: blah\nsymforge 7.15.4"),
            Some("7.15.4".to_string())
        );
        assert_eq!(parse_symforge_version("no version here"), None);
        assert_eq!(parse_symforge_version(""), None);
    }

    #[test]
    fn orchestrate_update_runs_full_sequence_on_success() {
        let mut ops = FakeOps::default();
        orchestrate_update("linux", "x86_64", &mut ops).expect("update should succeed");

        let stage = ops.position("stage npm symforge@latest symforge-linux-x64@latest");
        let stop = ops.position("stop-daemon");
        let swap = ops.position("swap");
        let daemon = ops.position("daemon 7.15.4");
        let initialize = ops.position("initialize");
        let reregister = ops.position("reregister claude");
        assert!(ops.position("prune") < stage, "cruft is pruned first");
        assert!(
            stage < stop && stop + 1 == swap,
            "the daemon stops only once the staged binary is verified, right before the swap: {:?}",
            ops.events
        );
        assert!(swap < daemon && daemon < initialize && initialize < reregister);
        assert_eq!(ops.reconciled_with, Some(true));
        // The daemon is the only process update stops.
        let stops: Vec<_> = ops
            .events
            .iter()
            .filter(|e| e.contains("stop") || e.contains("kill"))
            .collect();
        assert_eq!(stops, vec!["stop-daemon"], "{:?}", ops.events);
    }

    #[test]
    fn orchestrate_update_warns_but_succeeds_when_a_same_version_shadow_wins() {
        let mut ops = FakeOps {
            shadow: Some(crate::path_shadow::ShadowReport {
                our_path: PathBuf::from("/home/you/.npm-global/bin/symforge"),
                our_version: Some("7.15.4".to_string()),
                shadow_path: PathBuf::from("/usr/local/bin/symforge"),
                shadow_version: Some("7.15.4".to_string()),
                kind: crate::path_shadow::ShadowKind::RootSystem,
            }),
            ..Default::default()
        };
        orchestrate_update("linux", "x86_64", &mut ops)
            .expect("a same-version shadow is advisory, not fatal");
        assert!(ops.position("swap") < ops.position("shadow"));
        assert!(ops.happened("reregister claude"));
    }

    #[test]
    fn launcher_path_in_prefix_maps_per_os_layout() {
        // Windows: npm places the shim at the prefix root.
        assert_eq!(
            launcher_path_in_prefix(std::path::Path::new("C:/npm-prefix"), "windows"),
            std::path::Path::new("C:/npm-prefix").join("symforge.cmd")
        );
        // Unix: conventional <prefix>/bin/<name>.
        assert_eq!(
            launcher_path_in_prefix(std::path::Path::new("/home/you/.npm-global"), "linux"),
            std::path::Path::new("/home/you/.npm-global/bin/symforge")
        );
    }

    #[test]
    fn orchestrate_update_uses_npm_cmd_and_windows_package_on_windows() {
        let mut ops = FakeOps::default();
        orchestrate_update("windows", "x86_64", &mut ops).expect("update should succeed");

        assert!(ops.happened("stage npm.cmd symforge@latest symforge-windows-x64@latest"));
    }

    #[test]
    fn orchestrate_update_reports_failed_npm_without_reregistering() {
        let mut ops = FakeOps {
            stage_result: false,
            ..Default::default()
        };
        let err = orchestrate_update("windows", "x86_64", &mut ops)
            .expect_err("a failed staging install is reported");
        let msg = err.to_string();
        assert!(msg.contains("NOT touched"), "{msg}");
        assert!(
            msg.contains("npm install -g symforge@latest symforge-windows-x64@latest"),
            "{msg}"
        );
        assert!(ops.happened("prune"), "prune runs even when staging fails");
        assert!(!ops.happened("swap") && !ops.happened("stop-daemon"));
        assert!(!ops.events.iter().any(|e| e.starts_with("daemon")));
        assert!(!ops.events.iter().any(|e| e.starts_with("reregister")));
        assert_eq!(ops.reconciled_with, None);
    }

    #[test]
    fn orchestrate_update_prunes_registry_before_install_on_success() {
        let mut ops = FakeOps::default();

        orchestrate_update("linux", "x86_64", &mut ops).expect("update should succeed");

        let prunes = ops.events.iter().filter(|e| *e == "prune").count();
        assert_eq!(prunes, 1, "prune runs exactly once");
        assert!(
            ops.position("prune")
                < ops.position("stage npm symforge@latest symforge-linux-x64@latest"),
            "prune precedes the npm swap"
        );
    }

    #[test]
    fn orchestrate_update_fails_loudly_when_resolved_version_stays_stale() {
        let mut ops = FakeOps {
            installed: InstalledProbe::Version("7.15.2".to_string()),
            ..Default::default()
        };
        let err = orchestrate_update("linux", "x86_64", &mut ops)
            .expect_err("stale resolved version must fail loudly");
        let msg = err.to_string();
        assert!(msg.contains("incomplete"), "{msg}");
        assert!(msg.contains("7.15.2") && msg.contains("7.15.4"), "{msg}");
        assert!(msg.contains("PATH-shadowing"), "{msg}");
        assert!(!ops.events.iter().any(|e| e.starts_with("reregister")));
    }

    #[test]
    fn orchestrate_update_bails_on_launcher_failure_surfacing_stderr() {
        let mut ops = FakeOps {
            installed: InstalledProbe::LauncherFailed(
                "symforge: platform package symforge-linux-x64 not found".to_string(),
            ),
            ..Default::default()
        };
        let err = orchestrate_update("linux", "x86_64", &mut ops)
            .expect_err("a launcher that cannot resolve a binary must fail loudly");
        let msg = err.to_string();
        assert!(msg.contains("could not resolve a native binary"), "{msg}");
        assert!(msg.contains("symforge-linux-x64 not found"), "{msg}");
        assert!(!ops.events.iter().any(|e| e.starts_with("reregister")));
    }

    #[test]
    fn orchestrate_update_floors_against_running_version_when_registry_offline() {
        let mut ops = FakeOps {
            latest: None,
            staged: InstalledProbe::Version("0.0.1".to_string()),
            installed: InstalledProbe::Version("0.0.1".to_string()),
            ..Default::default()
        };
        let err = orchestrate_update("linux", "x86_64", &mut ops)
            .expect_err("a binary older than the running update binary must fail even offline");
        assert!(err.to_string().contains("incomplete"), "{err:?}");
        assert!(!ops.events.iter().any(|e| e.starts_with("reregister")));
    }

    #[test]
    fn orchestrate_update_warns_but_succeeds_when_probe_is_unavailable() {
        let mut ops = FakeOps {
            installed: InstalledProbe::Unprobeable,
            ..Default::default()
        };
        orchestrate_update("linux", "x86_64", &mut ops)
            .expect("an unprobeable launcher must not fail an otherwise-verified update");
        assert!(ops.happened("reregister claude"));
        assert_eq!(ops.reconciled_with, Some(true));
    }

    #[test]
    fn orchestrate_update_does_not_remove_orphan_when_reregistration_fails() {
        let mut ops = FakeOps {
            reregister_fails: vec![HarnessId::ClaudeCode],
            ..Default::default()
        };
        orchestrate_update("linux", "x86_64", &mut ops).expect("update should still succeed");
        assert_eq!(
            ops.reconciled_with,
            Some(false),
            "failed re-registration must prevent orphan removal"
        );
    }

    #[test]
    fn install_invocation_uses_no_shell_wrappers() {
        let args = staging_install_args(
            Path::new("C:/npm/.symforge-update-staging"),
            &install_specs("windows", "x86_64"),
        );
        let text = invocation_text(
            npm_executable_for_os("windows"),
            &args.iter().map(String::as_str).collect::<Vec<_>>(),
        )
        .to_ascii_lowercase();
        assert!(!text.contains("powershell"));
        assert!(!text.contains("cmd /c"));
        assert!(!text.contains("executionpolicy"));
    }

    #[test]
    fn a_failed_daemon_stop_is_reported_and_the_restart_still_runs() {
        let mut ops = FakeOps {
            stop: Err("reading daemon port file: access denied".to_string()),
            ..Default::default()
        };
        orchestrate_update("linux", "x86_64", &mut ops).expect("update should succeed");
        assert!(ops.position("swap") < ops.position("daemon 7.15.4"));
    }

    #[test]
    fn orchestrate_update_skips_the_swap_when_the_live_binary_is_latest_but_still_verifies() {
        let mut ops = FakeOps {
            live: vec![InstalledProbe::Version("7.15.4".to_string())],
            ..Default::default()
        };
        orchestrate_update("windows", "x86_64", &mut ops).expect("an up-to-date update succeeds");

        assert!(
            !ops.events.iter().any(|e| e.starts_with("stage")),
            "{:?}",
            ops.events
        );
        assert!(!ops.happened("swap"));
        assert!(
            !ops.happened("stop-daemon"),
            "nothing is stopped when nothing is swapped"
        );
        assert!(
            ops.happened("daemon 7.15.4"),
            "post-verify still restarts/ensures the daemon"
        );
        assert!(
            ops.happened("initialize"),
            "post-verify still replays initialize"
        );
    }

    #[test]
    fn orchestrate_update_never_short_circuits_without_a_registry_answer() {
        let mut ops = FakeOps {
            latest: None,
            live: vec![InstalledProbe::Version(
                env!("CARGO_PKG_VERSION").to_string(),
            )],
            installed: InstalledProbe::Version(env!("CARGO_PKG_VERSION").to_string()),
            ..Default::default()
        };
        orchestrate_update("linux", "x86_64", &mut ops).expect("update should succeed");
        assert!(ops.happened("swap"), "equal-to-unknown is not up to date");
    }

    #[test]
    fn a_staged_binary_behind_the_registry_aborts_before_the_swap() {
        let mut ops = FakeOps {
            staged: InstalledProbe::Version("7.15.2".to_string()),
            ..Default::default()
        };
        let err = orchestrate_update("linux", "x86_64", &mut ops)
            .expect_err("a stale staged binary must not be swapped in");
        assert!(err.to_string().contains("7.15.2"), "{err}");
        assert!(!ops.happened("swap") && !ops.happened("stop-daemon"));
    }

    #[test]
    fn a_silent_new_binary_fails_the_update_loudly_and_skips_reregistration() {
        let mut ops = FakeOps {
            initialize: Err("no `initialize` result within 10s".to_string()),
            ..Default::default()
        };
        let err = orchestrate_update("linux", "x86_64", &mut ops)
            .expect_err("an unanswered initialize must fail the update");
        assert!(format!("{err:#}").contains("initialize"), "{err:#}");
        assert!(!ops.events.iter().any(|e| e.starts_with("reregister")));
        assert_eq!(ops.reconciled_with, None);
    }

    #[test]
    fn a_daemon_restart_failure_is_reported_not_swallowed() {
        let mut ops = FakeOps {
            daemon: Err("auto-spawn disabled".to_string()),
            ..Default::default()
        };
        // The machine still works (clients spawn the daemon on demand), so the
        // update succeeds; the summary carries the failure.
        orchestrate_update("linux", "x86_64", &mut ops).expect("update should succeed");
        assert!(ops.happened("initialize"));
    }

    #[test]
    fn plan_reregistration_touches_only_harnesses_that_already_use_symforge() {
        let mut scan = scan_of(&[
            (HarnessId::ClaudeCode, HarnessState::PresentCurrent),
            (
                HarnessId::Codex,
                HarnessState::PresentStale(crate::cli::harness::StaleFields::Url),
            ),
            (HarnessId::Gemini, HarnessState::NotInstalled),
            (HarnessId::Cursor, HarnessState::Absent),
            (
                HarnessId::ClaudeDesktop,
                HarnessState::Malformed("expected `=`, found `sk-secret`".to_string()),
            ),
            (HarnessId::KiloCode, HarnessState::PresentCurrent),
        ]);
        scan.grok_config_present = true;

        let (targets, skipped) = plan_reregistration(&scan);

        assert_eq!(targets, vec![HarnessId::ClaudeCode, HarnessId::Codex]);
        let text = skipped.join("\n");
        assert!(
            text.contains("Claude Desktop") && text.contains("does not parse"),
            "{text}"
        );
        assert!(
            !text.contains("sk-secret"),
            "a parse error can quote a credential and must not be echoed: {text}"
        );
        assert!(
            text.contains("Kilo Code") && text.contains("project-local"),
            "{text}"
        );
        assert!(text.contains("Grok"), "{text}");
        assert!(
            !text.contains("Gemini") && !text.contains("Cursor"),
            "{text}"
        );
    }

    #[test]
    fn summary_renders_versions_harnesses_daemon_initialize_and_skips() {
        let summary = UpdateSummary {
            old_version: "11.2.0".to_string(),
            new_version: "11.3.0".to_string(),
            up_to_date: false,
            swapped: true,
            reregistered: vec!["Claude Code", "Codex"],
            daemon: Some(Ok(51234)),
            initialize: Some(Ok(())),
            notes: vec!["skipped: re-registering Cursor: init exited with 1".to_string()],
        };
        assert_eq!(
            summary.render(),
            "symforge update summary:\n  version: 11.2.0 -> 11.3.0\n  re-registered: Claude Code, Codex\n  \
             daemon restarted: yes (port 51234)\n  initialize verified: yes\n  \
             skipped: re-registering Cursor: init exited with 1"
        );

        let up_to_date = UpdateSummary {
            old_version: "11.3.0".to_string(),
            new_version: "11.3.0".to_string(),
            up_to_date: true,
            daemon: Some(Err("auto-spawn disabled".to_string())),
            ..Default::default()
        };
        let text = up_to_date.render();
        assert!(
            text.contains("11.3.0 (already the latest; swap skipped)"),
            "{text}"
        );
        assert!(text.contains("re-registered: none"), "{text}");
        assert!(
            text.contains("daemon restarted: no (auto-spawn disabled)"),
            "{text}"
        );
        assert!(
            text.contains("initialize verified: no (not attempted)"),
            "{text}"
        );

        // A failure before the swap: the version did not move and nothing
        // after the failure ran.
        let failed = UpdateSummary {
            old_version: "11.2.0".to_string(),
            notes: vec!["skipped: 1 leftover(s) in C:/npm/.symforge-update-stale".to_string()],
            ..Default::default()
        };
        let text = failed.render();
        assert!(text.contains("version: 11.2.0 (unchanged)"), "{text}");
        assert!(
            text.contains("daemon restarted: no (not attempted)"),
            "{text}"
        );
        assert!(text.contains("skipped: 1 leftover(s)"), "{text}");
    }

    #[test]
    fn initialize_result_requires_the_replayed_id_and_a_protocol_version() {
        assert!(is_initialize_result(
            r#"{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2025-06-18","capabilities":{}}}"#
        ));
        assert!(!is_initialize_result(
            r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32600,"message":"bad"}}"#
        ));
        assert!(!is_initialize_result(
            r#"{"jsonrpc":"2.0","method":"notifications/message"}"#
        ));
        assert!(!is_initialize_result("not json"));
    }

    #[test]
    fn verify_initialize_fails_fast_when_the_binary_cannot_start() {
        let tmp = tempfile::tempdir().unwrap();
        let err = verify_initialize_at(
            &tmp.path().join("missing-symforge"),
            tmp.path(),
            Duration::from_secs(1),
        )
        .expect_err("a missing binary cannot answer");
        assert!(format!("{err:#}").contains("stdio MCP server"), "{err:#}");
    }

    #[test]
    fn staging_install_is_local_to_the_staging_prefix_and_never_global() {
        let staging = Path::new("/npm/.symforge-update-staging");
        let args = staging_install_args(staging, &install_specs("linux", "x86_64"));
        assert_eq!(
            args,
            vec![
                "install",
                "--prefix",
                &staging.display().to_string(),
                "--no-save",
                "symforge@latest",
                "symforge-linux-x64@latest",
            ]
        );
        assert!(
            !args.iter().any(|a| a == "-g" || a == "--global"),
            "{args:?}"
        );
    }

    #[test]
    fn install_layout_maps_the_live_and_staged_binary_per_os() {
        let prefix = Path::new("/p");
        assert_eq!(
            native_binary_in(
                &global_modules_dir(prefix, "windows"),
                "symforge-windows-x64",
                "windows"
            ),
            prefix.join("node_modules/symforge-windows-x64/bin/symforge.exe")
        );
        assert_eq!(
            native_binary_in(
                &global_modules_dir(prefix, "linux"),
                "symforge-linux-x64",
                "linux"
            ),
            prefix.join("lib/node_modules/symforge-linux-x64/bin/symforge")
        );
        let staging = update_staging_dir(prefix);
        assert_eq!(staging, prefix.join(".symforge-update-staging"));
        assert!(!staging.starts_with(global_modules_dir(prefix, "windows")));
    }

    // ── Windows self-update robustness: timeout floor, move-aside staging, sweep ──

    struct NeverExits {
        kills: usize,
    }
    impl Waitable for NeverExits {
        fn try_wait(&mut self) -> std::io::Result<Option<std::process::ExitStatus>> {
            Ok(None)
        }
        fn kill(&mut self) -> std::io::Result<()> {
            self.kills += 1;
            Ok(())
        }
    }

    #[test]
    fn wait_or_kill_times_out_kills_the_child_and_returns_false() {
        // The safety floor: a child that never exits must be killed at the
        // deadline and reported as Ok(false) so the graceful bail fires — never a
        // hang. A deadline already in the past trips on the first poll (no sleep).
        let mut child = NeverExits { kills: 0 };
        let result = wait_or_kill(
            &mut child,
            std::time::Instant::now(),
            std::time::Duration::from_millis(1),
        )
        .expect("a timeout is Ok(false), not an error");
        assert!(!result, "a timed-out swap must return Ok(false)");
        assert_eq!(child.kills, 1, "the hung child must be killed exactly once");
    }

    #[test]
    fn stale_staging_dir_is_the_global_prefix_sibling_not_inside_a_wrapper() {
        // Derived from the npm GLOBAL prefix (npm prefix -g), so even a deeply
        // nested install
        // (.../node_modules/symforge/node_modules/symforge-windows-x64/bin/...)
        // stages beside the GLOBAL node_modules — never inside the wrapper package
        // npm rimrafs. The dir depends ONLY on the prefix, not on exe nesting, so
        // the old "walk to innermost node_modules ancestor" brick is gone.
        // Forward slashes so components parse identically on Windows and Unix.
        let prefix = std::path::Path::new("C:/Users/me/AppData/Roaming/npm");
        let dir = stale_staging_dir(prefix);
        assert_eq!(
            dir,
            std::path::Path::new("C:/Users/me/AppData/Roaming/npm/.symforge-update-stale"),
            "staging dir is the global prefix's .symforge-update-stale"
        );
        assert!(
            !dir.starts_with("C:/Users/me/AppData/Roaming/npm/node_modules"),
            "staging dir must be a global-prefix sibling, outside node_modules: {dir:?}"
        );
    }

    #[test]
    fn sweep_stale_dir_removes_leftovers_skips_unremovable_and_tolerates_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        std::fs::write(dir.join("symforge.exe.old-1"), b"a").unwrap();
        std::fs::write(dir.join("symforge.exe.old-2"), b"b").unwrap();
        // A subdirectory stands in for a still-locked entry: remove_file refuses a
        // directory on every platform.
        std::fs::create_dir(dir.join("locked-like-subdir")).unwrap();

        let lines = sweep_stale_dir(dir);
        assert!(!dir.join("symforge.exe.old-1").exists());
        assert!(!dir.join("symforge.exe.old-2").exists());
        assert!(dir.join("locked-like-subdir").exists());
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert!(lines[0].contains("swept 2"), "{lines:?}");
        assert!(lines[1].starts_with("skipped: 1 leftover"), "{lines:?}");

        assert!(sweep_stale_dir(&dir.join("nope")).is_empty());
    }

    /// Copy a benign long-lived system binary to `path` and run it, standing in
    /// for a live harness session executing the installed symforge binary.
    fn spawn_holder_at(path: &Path) -> std::process::Child {
        #[cfg(windows)]
        let (source, args): (&str, &[&str]) =
            (r"C:\Windows\System32\PING.EXE", &["-n", "60", "127.0.0.1"]);
        #[cfg(not(windows))]
        let (source, args): (&str, &[&str]) = ("/bin/sleep", &["60"]);
        std::fs::copy(source, path).expect("copy holder binary");
        crate::process_util::hidden_command(path)
            .args(args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn holder")
    }

    #[test]
    fn the_swap_replaces_a_running_binary_without_stopping_its_process() {
        let tmp = tempfile::tempdir().unwrap();
        let staged = tmp.path().join("staging/node_modules/pkg");
        let live = tmp.path().join("node_modules/pkg");
        let aside = tmp.path().join(".symforge-update-stale");
        std::fs::create_dir_all(staged.join("bin")).unwrap();
        std::fs::create_dir_all(live.join("bin")).unwrap();
        let live_binary = live
            .join("bin")
            .join(native_binary_name(std::env::consts::OS));
        std::fs::write(
            staged
                .join("bin")
                .join(native_binary_name(std::env::consts::OS)),
            b"new-binary",
        )
        .unwrap();
        std::fs::write(staged.join("package.json"), b"{\"version\":\"2\"}").unwrap();
        std::fs::write(live.join("package.json"), b"{\"version\":\"1\"}").unwrap();
        std::fs::write(live.join("only-in-old.txt"), b"kept").unwrap();
        let mut holder = spawn_holder_at(&live_binary);
        let old_bytes = std::fs::read(&live_binary).unwrap();

        // Probe the path the whole time the swap runs.
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let prober = {
            let stop = std::sync::Arc::clone(&stop);
            let path = live_binary.clone();
            std::thread::spawn(move || {
                let (mut checks, mut misses) = (0u64, 0u64);
                while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                    checks += 1;
                    if std::fs::metadata(&path).is_err() {
                        misses += 1;
                    }
                }
                (checks, misses)
            })
        };
        let swapped = overlay_package(&staged, &live, &aside);
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        let (checks, misses) = prober.join().unwrap();
        let still_running = holder.try_wait().unwrap().is_none();
        holder.kill().unwrap();
        holder.wait().unwrap();

        swapped.expect("the overlay succeeds over a running binary");
        assert!(still_running, "the running session must survive the swap");
        assert_eq!(std::fs::read(&live_binary).unwrap(), b"new-binary");
        assert_eq!(
            std::fs::read(live.join("package.json")).unwrap(),
            b"{\"version\":\"2\"}"
        );
        assert!(live.join("only-in-old.txt").exists());
        assert!(checks > 0);
        if cfg!(windows) {
            // The running image was refused as a rename target, so it went aside.
            let moved: Vec<_> = std::fs::read_dir(&aside).unwrap().flatten().collect();
            assert_eq!(moved.len(), 1, "{moved:?}");
            assert_eq!(std::fs::read(moved[0].path()).unwrap(), old_bytes);
        } else {
            // rename(2) replaces a running binary atomically: never absent.
            assert_eq!(misses, 0, "the path was absent in {misses}/{checks} probes");
        }
    }

    #[test]
    fn replace_file_moves_the_old_file_back_when_the_new_one_cannot_follow() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("staged.bin");
        let dst = tmp.path().join("symforge.bin");
        let aside = tmp.path().join("aside");
        std::fs::write(&src, b"new").unwrap();
        std::fs::write(&dst, b"old").unwrap();

        // Every rename of the staged file is refused the way Windows refuses a
        // running image; the aside and move-back renames are real.
        let err = replace_file(&src, &dst, &aside, |from, to| {
            if from == src.as_path() {
                Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied))
            } else {
                std::fs::rename(from, to)
            }
        })
        .expect_err("the staged file never landed");

        assert_eq!(err.kind(), std::io::ErrorKind::PermissionDenied);
        assert_eq!(
            std::fs::read(&dst).unwrap(),
            b"old",
            "the old file is back in place"
        );
        assert_eq!(std::fs::read_dir(&aside).unwrap().count(), 0);
        assert_eq!(std::fs::read(&src).unwrap(), b"new");
    }

    #[test]
    fn replace_file_does_not_touch_the_target_for_other_errors() {
        let tmp = tempfile::tempdir().unwrap();
        let dst = tmp.path().join("symforge.bin");
        std::fs::write(&dst, b"old").unwrap();
        let err = replace_file(
            &tmp.path().join("missing"),
            &dst,
            &tmp.path().join("aside"),
            |a, b| std::fs::rename(a, b),
        )
        .expect_err("a missing staged file is an error");
        assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
        assert_eq!(std::fs::read(&dst).unwrap(), b"old");
        assert!(!tmp.path().join("aside").exists());
    }

    #[test]
    #[cfg(windows)]
    fn move_file_replace_existing_ffi_moves_a_plain_file() {
        // Exercises the load-bearing unsafe on a NON-running file (a live-binary
        // swap can't be unit-tested; see the module SAFETY notes).
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.bin");
        let dst = tmp.path().join("staged.old-42");
        std::fs::write(&src, b"payload").unwrap();
        windows_native::move_file_replace_existing(&src, &dst).expect("FFI move succeeds");
        assert!(!src.exists(), "the source path is freed after the move");
        assert_eq!(std::fs::read(&dst).unwrap(), b"payload");
    }

    #[test]
    #[cfg(windows)]
    fn move_path_aside_frees_the_source_path() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("symforge.exe");
        let staged = tmp
            .path()
            .join(".symforge-update-stale")
            .join("symforge.exe.old-1");
        std::fs::create_dir_all(staged.parent().unwrap()).unwrap();
        std::fs::write(&src, b"binary").unwrap();
        move_path_aside(&src, &staged).expect("move aside succeeds");
        assert!(!src.exists(), "the install path is freed for npm");
        assert!(staged.exists());
    }

    // Tests that mutate `SYMFORGE_HOME` serialize on this lock and rely on
    // `--test-threads=1`, so no concurrent env reader observes the transition.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// RAII guard: set `SYMFORGE_HOME` for the body's duration and restore the
    /// prior value (set or unset) on drop, so the env mutation never leaks to
    /// other tests even on panic.
    struct SymforgeHomeGuard {
        prev: Option<std::ffi::OsString>,
    }

    impl SymforgeHomeGuard {
        #[allow(unsafe_code)] // test-only env mutation under ENV_LOCK + --test-threads=1.
        fn set(value: &std::path::Path) -> Self {
            let prev = std::env::var_os("SYMFORGE_HOME");
            // SAFETY: the caller holds ENV_LOCK and the suite runs single-threaded,
            // so no other thread can read or write the environment concurrently.
            unsafe { std::env::set_var("SYMFORGE_HOME", value) };
            Self { prev }
        }
    }

    impl Drop for SymforgeHomeGuard {
        #[allow(unsafe_code)] // test-only env restore under ENV_LOCK + --test-threads=1.
        fn drop(&mut self) {
            // SAFETY: see `SymforgeHomeGuard::set`.
            match &self.prev {
                Some(prev) => unsafe { std::env::set_var("SYMFORGE_HOME", prev) },
                None => unsafe { std::env::remove_var("SYMFORGE_HOME") },
            }
        }
    }

    #[test]
    fn remove_orphan_durable_bin_cleans_under_symforge_home_and_spares_self_exe() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let home = tempfile::TempDir::new().unwrap();
        let bin = home.path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();

        // A retired durable leftover that is NOT the running process binary.
        let leftover = bin.join("symforge.exe");
        std::fs::write(&leftover, b"old-7.14.4-binary").unwrap();
        // An unrelated file that is not in the watched set must be left untouched.
        let bystander = bin.join("notes.txt");
        std::fs::write(&bystander, b"keep me").unwrap();

        // Point SYMFORGE_HOME at this temp home: Bug 2 is that the cleanup used to
        // bail entirely whenever SYMFORGE_HOME was set. It must now run.
        let _home_guard = SymforgeHomeGuard::set(home.path());

        // Sanity: the running test binary is a real, canonicalizable path and is
        // NOT inside this temp bin, so the self-exe guard must never delete it.
        let self_exe = std::env::current_exe().unwrap();
        assert!(self_exe.exists(), "precondition: running exe exists");

        let summary = remove_orphan_durable_bin_at(&crate::domain::ControlStateDir::new(
            home.path().to_path_buf(),
        ));

        assert!(
            !leftover.exists(),
            "the retired durable leftover under $SYMFORGE_HOME/bin must be removed"
        );
        assert_eq!(summary.len(), 1, "exactly one summary line: {summary:?}");
        assert!(
            summary[0].contains("symforge.exe"),
            "summary names what was removed: {summary:?}"
        );
        assert!(bystander.exists(), "an unwatched file must be left alone");
        // The self-exe guard's invariant: the running process binary still exists.
        assert!(
            self_exe.exists(),
            "the binary backing the running process must never be deleted"
        );
    }

    #[test]
    fn remove_orphan_durable_bin_skips_the_running_binary_when_it_lives_in_the_bin_dir() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        // Directly exercise the self-exe guard's `continue` branch: place the
        // running test binary INTO `$SYMFORGE_HOME/bin` under a watched name via a
        // symlink (so canonicalization resolves it back to the same file, matching
        // the guard's canonical-path identity check). A plain copy would have a
        // distinct canonical path and would not be a fair test of the guard.
        let self_exe = match std::env::current_exe()
            .ok()
            .and_then(|p| std::fs::canonicalize(p).ok())
        {
            Some(p) => p,
            None => return, // no canonicalizable current exe — nothing to assert
        };

        let home = tempfile::TempDir::new().unwrap();
        let bin = home.path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let watched = bin.join("symforge.exe");

        if make_symlink(&self_exe, &watched).is_err() {
            // Symlink creation may require privilege (Windows) — skip rather than
            // fail; the other durable test still covers the spare-self invariant.
            return;
        }
        // Confirm the link resolves to the running binary, so the guard must skip it.
        let resolves_to_self =
            std::fs::canonicalize(&watched).ok().as_deref() == Some(self_exe.as_path());
        if !resolves_to_self {
            return;
        }

        let _home_guard = SymforgeHomeGuard::set(home.path());
        let summary = remove_orphan_durable_bin();

        assert!(
            watched.exists() || std::fs::symlink_metadata(&watched).is_ok(),
            "the entry resolving to the running binary must NOT be removed"
        );
        assert!(self_exe.exists(), "the running binary itself must survive");
        assert!(
            summary.is_empty(),
            "nothing should be reported removed: {summary:?}"
        );
    }

    #[cfg(unix)]
    fn make_symlink(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
        std::os::unix::fs::symlink(src, dst)
    }

    #[cfg(windows)]
    fn make_symlink(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
        std::os::windows::fs::symlink_file(src, dst)
    }
}
