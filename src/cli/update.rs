//! Explicit npm-managed self-update command.
//!
//! Beyond shelling `npm install -g symforge@latest`, this orchestrates a complete
//! update so the user is never left with a half-updated mix of versions:
//!  1. stops the running daemon before the swap (a live daemon holds the old
//!     binary — a Windows file-lock — and keeps serving stale behavior) and clears
//!     a demonstrably-dead sidecar record;
//!  2. forces the OS-native platform package (`symforge-<os>-<arch>`) to the same
//!     version, because `npm install -g symforge@latest` alone can retain a stale
//!     nested platform package and leave `symforge --version` behind the wrapper;
//!  3. VERIFIES the resolved `symforge --version` reached the latest published
//!     version — and FAILS LOUDLY (stale nested package, a PATH-shadowing install,
//!     or a WSL Windows-prefix bleed) instead of a hollow success, even when the
//!     npm registry is unreachable (it floors against the running binary's version
//!     and surfaces a launcher that ran but could not resolve a binary);
//!  4. re-registers every MCP client onto the freshly-installed binary; and
//!  5. only AFTER a confirmed re-registration, clears the retired `~/.symforge/bin`
//!     durable-install leftovers and prunes dead version-registry entries.

use anyhow::{Context, bail};
use std::process::{Command, Stdio};

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

/// Build the `npm install -g` package specs. Always installs the `symforge`
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

/// Outcome of probing the freshly-installed `symforge --version`.
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

/// Clear a demonstrably-dead sidecar record for the current project (CWD
/// `.symforge`). The sidecar is NOT killed: its pid is the in-process MCP server
/// (killing it would drop the user's editor connection), and a TCP-alive probe
/// does not prove the recorded pid owns the port (recycled-pid hazard). Only a
/// `Dead` record is cleaned so the next launch starts clean.
fn clear_dead_sidecar_record() -> Option<String> {
    use crate::sidecar::port_file::{SidecarLiveness, cleanup_files_at, read_sidecar_status_at};

    let dir = std::path::Path::new(".symforge");
    let status = read_sidecar_status_at(dir, "127.0.0.1");
    if matches!(status.liveness, SidecarLiveness::Dead) {
        cleanup_files_at(dir);
        Some("cleared a stale sidecar record".to_string())
    } else {
        None
    }
}

/// Resolve the durable-install `bin` directory the same way the rest of SymForge
/// resolves its home: `$SYMFORGE_HOME/bin` when `SYMFORGE_HOME` is set (it is set
/// in the standard MCP server config and routinely points at the SAME default
/// `~/.symforge`), else `~/.symforge/bin`. Returns `None` only when neither is
/// resolvable.
fn durable_bin_dir() -> Option<std::path::PathBuf> {
    crate::version_registry::resolve_home().map(|home| home.join("bin"))
}

/// Remove the retired durable-install artifacts under the resolved durable `bin`
/// directory (`$SYMFORGE_HOME/bin` when set, else `~/.symforge/bin`) — the only
/// place the retired durable mechanism ever wrote. The real safety invariant is
/// the self-exe guard: it never deletes the binary backing the current process.
/// Best-effort; callers must only invoke this AFTER clients are re-registered off
/// the orphan.
fn remove_orphan_durable_bin() -> Vec<String> {
    let Some(bin) = durable_bin_dir() else {
        return Vec::new();
    };
    let self_exe = std::env::current_exe()
        .ok()
        .and_then(|p| std::fs::canonicalize(p).ok());

    let mut removed = Vec::new();
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
        if std::fs::remove_file(&path).is_ok() {
            removed.push(name.to_string());
        }
    }
    if removed.is_empty() {
        Vec::new()
    } else {
        vec![format!(
            "removed retired durable-install leftover(s) from {}: {}",
            bin.display(),
            removed.join(", ")
        )]
    }
}

/// Side effects of an update, injected so the orchestration is unit-testable
/// without touching npm, the network, the daemon, or the filesystem.
pub(crate) trait UpdateOps {
    /// Stop the running daemon before the binary swap + clear a dead sidecar
    /// record. Returns a human-readable summary of what was stopped.
    fn stop_processes(&mut self) -> String;
    /// Run `<program> <args...>`; return `true` on success.
    fn npm_install(&mut self, program: &str, args: &[&str]) -> anyhow::Result<bool>;
    /// Probe the resolved `symforge --version` after install.
    fn installed_version(&mut self) -> InstalledProbe;
    /// Latest version published to the npm registry, or `None` when offline.
    fn latest_version(&mut self) -> Option<String>;
    /// Prune dead version-registry entries (paths whose binary was deleted while
    /// the drive is online). Runs UNCONDITIONALLY and early — even when the npm
    /// swap is blocked (Windows `EBUSY`) — so a blocked update still cleans cruft.
    /// Returns summary lines (empty when nothing was pruned).
    fn prune_registry(&mut self) -> Vec<String>;
    /// Re-register every MCP client onto the freshly-installed binary by spawning
    /// the NEW launcher's `init`. Returns `true` on success.
    fn reregister_clients(&mut self) -> anyhow::Result<bool>;
    /// Remove the retired durable-install leftovers ONLY when `reregistered` is
    /// true (otherwise clients still point at the orphan and deleting it would
    /// break them). Registry pruning is handled separately by [`prune_registry`].
    /// Returns summary lines.
    fn reconcile_durable(&mut self, reregistered: bool) -> Vec<String>;
}

struct RealUpdateOps;

impl UpdateOps for RealUpdateOps {
    fn stop_processes(&mut self) -> String {
        let mut stopped = Vec::new();

        // Daemon (global). `main()` is synchronous, so a short-lived runtime is
        // safe (no nested-runtime panic).
        let daemon = tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .enable_time()
            .build()
            .ok()
            .and_then(|rt| {
                rt.block_on(crate::daemon::stop_running_daemon_for_update())
                    .ok()
            });
        match daemon {
            Some(crate::daemon::DaemonStopOutcome::Stopped { pid }) => {
                stopped.push(format!("daemon (pid {pid})"));
            }
            Some(crate::daemon::DaemonStopOutcome::StopTimedOut { pid }) => {
                stopped.push(format!(
                    "daemon (pid {pid}) did NOT stop in time — left discoverable; rerun update or stop it manually"
                ));
            }
            Some(crate::daemon::DaemonStopOutcome::SkippedSafety) => {
                stopped.push("daemon left running (failed ownership safety check)".to_string());
            }
            _ => {}
        }

        if let Some(sidecar) = clear_dead_sidecar_record() {
            stopped.push(sidecar);
        }

        if stopped.is_empty() {
            "no running daemon found".to_string()
        } else {
            format!("stopped {}", stopped.join(", "))
        }
    }

    fn npm_install(&mut self, program: &str, args: &[&str]) -> anyhow::Result<bool> {
        let status = Command::new(program)
            .args(args)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .with_context(|| format!("failed to start `{}`", invocation_text(program, args)))?;
        Ok(status.success())
    }

    fn installed_version(&mut self) -> InstalledProbe {
        // Spawn the freshly-resolved launcher (this update process is still the
        // OLD binary, so we ask the launcher what it now resolves to). Inspect
        // BOTH stdout and the exit status: the npm launcher prints resolve errors
        // to stderr and exits non-zero with empty stdout, which must surface as a
        // loud failure, not a silent "could not probe".
        let output = match Command::new(symforge_launcher()).arg("--version").output() {
            Ok(output) => output,
            Err(_) => return InstalledProbe::Unprobeable,
        };
        if let Some(version) = parse_symforge_version(&String::from_utf8_lossy(&output.stdout)) {
            return InstalledProbe::Version(version);
        }
        // Ran but produced no version: surface the launcher's own diagnostic.
        let stderr = String::from_utf8_lossy(&output.stderr);
        let detail = stderr
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .unwrap_or("launcher produced no version output")
            .to_string();
        InstalledProbe::LauncherFailed(detail)
    }

    fn latest_version(&mut self) -> Option<String> {
        crate::cli::version::latest_npm_version()
    }

    fn prune_registry(&mut self) -> Vec<String> {
        let Some(home) = crate::version_registry::resolve_home() else {
            return Vec::new();
        };
        let pruned = crate::version_registry::prune_missing_entries(&home);
        if pruned > 0 {
            vec![format!(
                "pruned {pruned} stale version-registry entr{}",
                if pruned == 1 { "y" } else { "ies" }
            )]
        } else {
            Vec::new()
        }
    }

    fn reregister_clients(&mut self) -> anyhow::Result<bool> {
        // Spawn the NEW launcher's init so clients are registered at the freshly
        // installed binary (this update process is still the OLD binary, so an
        // in-process `run_init` would re-register the OLD path).
        let status = Command::new(symforge_launcher())
            .args(["init", "--client", "all"])
            .stdin(Stdio::null())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status();
        Ok(matches!(status, Ok(s) if s.success()))
    }

    fn reconcile_durable(&mut self, reregistered: bool) -> Vec<String> {
        if reregistered {
            remove_orphan_durable_bin()
        } else {
            Vec::new()
        }
    }
}

pub fn run_update() -> anyhow::Result<()> {
    orchestrate_update(
        std::env::consts::OS,
        std::env::consts::ARCH,
        &mut RealUpdateOps,
    )
}

pub(crate) fn orchestrate_update(
    os: &str,
    arch: &str,
    ops: &mut impl UpdateOps,
) -> anyhow::Result<()> {
    // Stop first: a live daemon holds the old binary (Windows file-lock) and
    // would keep serving stale behavior. It respawns lazily on next use.
    let stop_summary = ops.stop_processes();
    eprintln!("symforge update: {stop_summary}.");

    // Prune dead version-registry entries UNCONDITIONALLY and BEFORE the npm swap.
    // The swap can be blocked (Windows `EBUSY`: a running MCP client still holds
    // the `.exe`) and bail below — but registry cruft (e.g. removed git-worktree
    // dev builds) should be cleaned regardless of whether the swap succeeds.
    for line in ops.prune_registry() {
        eprintln!("symforge update: {line}");
    }

    let program = npm_executable_for_os(os);
    let specs = install_specs(os, arch);
    let args: Vec<&str> = ["install", "-g"]
        .into_iter()
        .chain(specs.iter().map(String::as_str))
        .collect();

    if !ops.npm_install(program, &args)? {
        bail!(
            "symforge update failed: `{}` exited unsuccessfully.\n\
             On Windows this is usually a running symforge or MCP client (Cursor, Claude) \
             holding the binary (EBUSY). Close your MCP clients and rerun `symforge update`. \
             (The version registry was already pruned.)",
            invocation_text(program, &args)
        );
    }

    // Verify the install actually took effect. npm can report success while the
    // resolved binary stays behind, so this is the load-bearing safety net.
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
            eprintln!(
                "symforge update: WARNING — could not run `symforge --version` to verify the result \
                 (the npm prefix bin may not be on PATH). The install ran; confirm with `symforge --version`."
            );
        }
        InstalledProbe::Version(installed) => {
            let latest = ops.latest_version();
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
                    "symforge update incomplete: npm reported success but `symforge --version` still \
                     reports {installed}, behind {target}. The resolved `symforge` is not the one just \
                     installed. Likely causes:\n  \
                     - stale nested platform package — retry: npm install -g symforge@latest {pkg}@latest --force\n  \
                     - a PATH-shadowing install — run `which -a symforge`; a root /usr/local copy can win over your npm prefix\n  \
                     - on WSL, a Windows npm prefix bleeding in via /mnt — put your Linux npm prefix bin ahead of /usr/local and /mnt on PATH\n  \
                     - on Windows, a running symforge can lock its own .exe — run the npm install from a separate shell"
                );
            }
            if latest.is_none() {
                eprintln!(
                    "symforge update: `symforge --version` reports {installed} (could not reach the npm \
                     registry to confirm it is the very latest)."
                );
            } else {
                eprintln!(
                    "symforge update complete — `symforge --version` now reports {installed}."
                );
            }
        }
    }

    // Re-point all clients at the freshly-installed binary. Only AFTER a confirmed
    // re-registration do we clear the retired durable-install artifacts — deleting
    // the orphan while a client still references it would break that client.
    let reregistered = match ops.reregister_clients() {
        Ok(true) => {
            eprintln!("symforge update: re-registered all MCP clients onto the new binary.");
            true
        }
        Ok(false) => {
            eprintln!(
                "symforge update: client re-registration did not complete — run `symforge init \
                 --client all` manually. Leaving the durable-install leftovers in place until then."
            );
            false
        }
        Err(error) => {
            eprintln!(
                "symforge update: client re-registration error ({error}) — run `symforge init \
                 --client all` manually. Leaving the durable-install leftovers in place until then."
            );
            false
        }
    };
    for line in ops.reconcile_durable(reregistered) {
        eprintln!("symforge update: {line}");
    }

    Ok(())
}

fn invocation_text(program: &str, args: &[&str]) -> String {
    let mut parts = Vec::with_capacity(args.len() + 1);
    parts.push(program.to_string());
    parts.extend(args.iter().map(|arg| (*arg).to_string()));
    parts.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeOps {
        install_calls: Vec<(String, Vec<String>)>,
        install_result: bool,
        installed: InstalledProbe,
        latest: Option<String>,
        reregister_result: anyhow::Result<bool>,
        prune_lines: Vec<String>,
        stopped_before_install: bool,
        reregistered_after_install: bool,
        reconciled_with: Option<bool>,
        pruned_before_install: Option<bool>,
        prune_calls: usize,
    }

    impl Default for FakeOps {
        fn default() -> Self {
            Self {
                install_calls: Vec::new(),
                install_result: false,
                installed: InstalledProbe::Unprobeable,
                latest: None,
                reregister_result: Ok(true),
                prune_lines: Vec::new(),
                stopped_before_install: false,
                reregistered_after_install: false,
                reconciled_with: None,
                pruned_before_install: None,
                prune_calls: 0,
            }
        }
    }

    impl UpdateOps for FakeOps {
        fn stop_processes(&mut self) -> String {
            self.stopped_before_install = self.install_calls.is_empty();
            "no running daemon found".to_string()
        }
        fn npm_install(&mut self, program: &str, args: &[&str]) -> anyhow::Result<bool> {
            self.install_calls.push((
                program.to_string(),
                args.iter().map(|a| a.to_string()).collect(),
            ));
            Ok(self.install_result)
        }
        fn installed_version(&mut self) -> InstalledProbe {
            self.installed.clone()
        }
        fn latest_version(&mut self) -> Option<String> {
            self.latest.clone()
        }
        fn prune_registry(&mut self) -> Vec<String> {
            // Record that the prune ran before any npm install was attempted, so
            // tests can assert the prune is unconditional and precedes the swap.
            self.prune_calls += 1;
            self.pruned_before_install = Some(self.install_calls.is_empty());
            self.prune_lines.clone()
        }
        fn reregister_clients(&mut self) -> anyhow::Result<bool> {
            self.reregistered_after_install = !self.install_calls.is_empty();
            match &self.reregister_result {
                Ok(value) => Ok(*value),
                Err(error) => Err(anyhow::anyhow!("{error}")),
            }
        }
        fn reconcile_durable(&mut self, reregistered: bool) -> Vec<String> {
            self.reconciled_with = Some(reregistered);
            Vec::new()
        }
    }

    fn ok_ops() -> FakeOps {
        FakeOps {
            install_result: true,
            installed: InstalledProbe::Version("7.15.4".to_string()),
            latest: Some("7.15.4".to_string()),
            ..Default::default()
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
        let mut ops = ok_ops();
        orchestrate_update("linux", "x86_64", &mut ops).expect("update should succeed");

        assert!(ops.stopped_before_install, "stop must precede install");
        assert_eq!(
            ops.install_calls,
            vec![(
                "npm".to_string(),
                vec![
                    "install".to_string(),
                    "-g".to_string(),
                    "symforge@latest".to_string(),
                    "symforge-linux-x64@latest".to_string(),
                ]
            )]
        );
        assert!(
            ops.reregistered_after_install,
            "clients re-registered after install"
        );
        assert_eq!(
            ops.reconciled_with,
            Some(true),
            "orphan reconcile must run with reregistered=true on success"
        );
    }

    #[test]
    fn orchestrate_update_uses_npm_cmd_and_windows_package_on_windows() {
        let mut ops = ok_ops();
        orchestrate_update("windows", "x86_64", &mut ops).expect("update should succeed");

        let (program, args) = &ops.install_calls[0];
        assert_eq!(program, "npm.cmd");
        assert!(args.contains(&"symforge-windows-x64@latest".to_string()));
    }

    #[test]
    fn orchestrate_update_reports_failed_npm_without_reregistering() {
        let mut ops = FakeOps {
            install_result: false,
            prune_lines: vec!["pruned 1 stale version-registry entry".to_string()],
            ..Default::default()
        };

        let err = orchestrate_update("linux", "x86_64", &mut ops)
            .expect_err("failed npm install should be reported");

        let msg = err.to_string();
        assert!(msg.contains("exited unsuccessfully"), "{msg}");
        // The new actionable hint for the Windows EBUSY (locked .exe) case.
        assert!(msg.contains("Close your MCP clients"), "{msg}");
        // The registry prune must run UNCONDITIONALLY and BEFORE the (blocked) swap,
        // so a blocked update still cleans cruft.
        assert_eq!(
            ops.prune_calls, 1,
            "prune must run even on a blocked update"
        );
        assert_eq!(
            ops.pruned_before_install,
            Some(true),
            "prune must run before the npm swap is attempted"
        );
        assert!(!ops.reregistered_after_install);
        assert_eq!(ops.reconciled_with, None, "no reconcile on failed install");
    }

    #[test]
    fn orchestrate_update_prunes_registry_before_install_on_success() {
        let mut ops = ok_ops();
        ops.prune_lines = vec!["pruned 2 stale version-registry entries".to_string()];

        orchestrate_update("linux", "x86_64", &mut ops).expect("update should succeed");

        assert_eq!(ops.prune_calls, 1, "prune runs exactly once");
        assert_eq!(
            ops.pruned_before_install,
            Some(true),
            "prune precedes the npm swap"
        );
    }

    #[test]
    fn orchestrate_update_fails_loudly_when_resolved_version_stays_stale() {
        let mut ops = FakeOps {
            install_result: true,
            installed: InstalledProbe::Version("7.15.2".to_string()),
            latest: Some("7.15.4".to_string()),
            ..Default::default()
        };

        let err = orchestrate_update("linux", "x86_64", &mut ops)
            .expect_err("stale resolved version must fail loudly");

        let msg = err.to_string();
        assert!(msg.contains("incomplete"), "{msg}");
        assert!(msg.contains("7.15.2") && msg.contains("7.15.4"), "{msg}");
        assert!(msg.contains("PATH-shadowing"), "{msg}");
        assert!(
            !ops.reregistered_after_install,
            "no re-register on a drifted install"
        );
    }

    #[test]
    fn orchestrate_update_bails_on_launcher_failure_surfacing_stderr() {
        // The marquee WSL/launcher case: install "succeeds" but the launcher
        // cannot resolve a binary. Must bail loudly, not report "skipped".
        let mut ops = FakeOps {
            install_result: true,
            installed: InstalledProbe::LauncherFailed(
                "symforge: platform package symforge-linux-x64 not found".to_string(),
            ),
            latest: Some("7.15.4".to_string()),
            ..Default::default()
        };

        let err = orchestrate_update("linux", "x86_64", &mut ops)
            .expect_err("a launcher that cannot resolve a binary must fail loudly");

        let msg = err.to_string();
        assert!(msg.contains("could not resolve a native binary"), "{msg}");
        assert!(msg.contains("symforge-linux-x64 not found"), "{msg}");
        assert!(!ops.reregistered_after_install);
    }

    #[test]
    fn orchestrate_update_floors_against_running_version_when_registry_offline() {
        // Registry unreachable (latest=None) AND the resolved binary is older than
        // the binary running the update — the swap demonstrably failed, so bail
        // even without a registry answer.
        let mut ops = FakeOps {
            install_result: true,
            installed: InstalledProbe::Version("0.0.1".to_string()),
            latest: None,
            ..Default::default()
        };

        let err = orchestrate_update("linux", "x86_64", &mut ops)
            .expect_err("a binary older than the running update binary must fail even offline");

        assert!(err.to_string().contains("incomplete"), "{err:?}");
        assert!(!ops.reregistered_after_install);
    }

    #[test]
    fn orchestrate_update_warns_but_succeeds_when_probe_is_unavailable() {
        // Launcher genuinely not spawnable: install ran, so don't fail; still
        // re-register + reconcile.
        let mut ops = FakeOps {
            install_result: true,
            installed: InstalledProbe::Unprobeable,
            latest: Some("7.15.4".to_string()),
            ..Default::default()
        };
        orchestrate_update("linux", "x86_64", &mut ops)
            .expect("unprobeable launcher must not fail an otherwise-successful update");
        assert!(ops.reregistered_after_install);
        assert_eq!(ops.reconciled_with, Some(true));
    }

    #[test]
    fn orchestrate_update_does_not_remove_orphan_when_reregistration_fails() {
        // reconcile must be told reregistered=false so the orphan (still
        // referenced by clients) is NOT deleted.
        let mut ops = FakeOps {
            install_result: true,
            installed: InstalledProbe::Version("7.15.4".to_string()),
            latest: Some("7.15.4".to_string()),
            reregister_result: Ok(false),
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
        let mut ops = ok_ops();
        orchestrate_update("windows", "x86_64", &mut ops).unwrap();

        let (program, args) = &ops.install_calls[0];
        let text = invocation_text(
            program,
            &args.iter().map(String::as_str).collect::<Vec<_>>(),
        )
        .to_ascii_lowercase();
        assert!(!text.contains("powershell"));
        assert!(!text.contains("cmd /c"));
        assert!(!text.contains("executionpolicy"));
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

        let summary = remove_orphan_durable_bin();

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
