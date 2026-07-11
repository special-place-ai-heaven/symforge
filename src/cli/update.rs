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
use std::process::Stdio;

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
    // Task 8: purge stale per-adapter descriptors alongside the legacy files.
    crate::sidecar::port_file::cleanup_stale_descriptors_at(dir, "127.0.0.1");
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
    /// Detect whether a DIFFERENT install shadows the binary npm just installed
    /// on `$PATH`. Derives "our binary" from the global npm prefix and compares
    /// it to the PATH-first `symforge`. Returns `None` when our install wins, the
    /// prefix is unresolvable, or no shadow exists. This is ADDITIVE to the
    /// reactive stale-version bail: it also fires when the shadow is the SAME
    /// version (which the stale-version check cannot see).
    fn shadow_report(&mut self) -> Option<crate::path_shadow::ShadowReport>;
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

        // Stop every OTHER symforge process running from the SAME executable path
        // as this one — the binary npm is about to overwrite. On Windows a live
        // holder keeps an exclusive image handle and blocks the swap (EBUSY), so
        // clearing the holders BEFORE npm runs is what lets the swap proceed. The
        // set is scoped by ExecutablePath (never by image name) and excludes THIS
        // process, so unrelated installs at other paths are never touched
        // (SELF_UPDATE_PROCEDURE.md, Invariant 1).
        for line in stop_other_inscope_holders() {
            stopped.push(line);
        }

        if stopped.is_empty() {
            "no running daemon found".to_string()
        } else {
            format!("stopped {}", stopped.join(", "))
        }
    }

    fn npm_install(&mut self, program: &str, args: &[&str]) -> anyhow::Result<bool> {
        let status = crate::process_util::hidden_command(program)
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
        let output = match crate::process_util::hidden_command(symforge_launcher())
            .arg("--version")
            .output()
        {
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
        let status = crate::process_util::hidden_command(symforge_launcher())
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

    fn shadow_report(&mut self) -> Option<crate::path_shadow::ShadowReport> {
        let installed = npm_installed_launcher_path()?;
        crate::path_shadow::detect_shadow(&installed)
    }
}

/// Resolve the global npm prefix via `npm prefix -g`, then derive the path of
/// the `symforge` launcher npm installs there. On Windows the shim lives at the
/// prefix root (`<prefix>/symforge.cmd`); on Unix it lives in `<prefix>/bin/symforge`.
/// Returns `None` when `npm` is unavailable or the prefix cannot be parsed.
fn npm_installed_launcher_path() -> Option<std::path::PathBuf> {
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
    Some(launcher_path_in_prefix(
        std::path::Path::new(prefix),
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
        // Any OTHER in-scope holders were already stopped in `stop_processes`
        // (their count, if any were found, was printed above). The message stays
        // honest when enumeration found/stopped nothing: it does NOT assert "all
        // holders were stopped", and the closing hint covers an un-enumerable
        // holder. The remediation runs from a PLAIN shell — not the `symforge`
        // binary that self-locks on Windows.
        let plain_cmd = format!("npm install -g {}", specs.join(" "));
        let init_cmd = format!("{} init --client all", symforge_launcher());
        if os == "windows" {
            bail!(
                "symforge update failed: `{}` exited unsuccessfully.\n\
                 On Windows the `symforge update` process runs from the very binary npm \
                 replaces and cannot overwrite a running .exe. Any OTHER in-scope holders \
                 were stopped above (if any were found), so finish the swap from a PLAIN \
                 shell (NOT via `symforge`):\n  {}\n\
                 then re-point your MCP clients onto the new binary:\n  {}\n\
                 If it STILL fails, an MCP client (Cursor, Claude) or another symforge \
                 process is holding the binary — close them and rerun the command above.\n\
                 (The version registry was already pruned.)",
                invocation_text(program, &args),
                plain_cmd,
                init_cmd
            );
        } else {
            bail!(
                "symforge update failed: `{}` exited unsuccessfully.\n\
                 Ensure no running symforge process is holding the binary, then rerun, or \
                 install directly from a plain shell:\n  {}\n\
                 (The version registry was already pruned.)",
                invocation_text(program, &args),
                plain_cmd
            );
        }
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

    // Proactive PATH-shadow check. The version-verification above bails only when
    // the resolved binary is BEHIND the target; it cannot see a same-version
    // shadow (a stale install that happens to match) or name the exact offending
    // path and fix. Compare the binary npm just installed to the PATH-first
    // `symforge` and, when a different install wins, print the precise remediation.
    if let Some(report) = ops.shadow_report() {
        eprintln!("{}", crate::path_shadow::format_shadow_warning(&report));
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

/// Normalize a Windows executable path for identity comparison: strip the
/// extended-length verbatim prefix (`\\?\`), unify slashes, and lowercase. This
/// path is Windows-only (a case-insensitive filesystem), so always lowercasing is
/// correct AND keeps the comparison deterministic under cross-platform test
/// builds. Mirrors the slash+lowercase normalization `daemon::stable_path_identity`
/// applies, plus a verbatim-prefix strip so a `\\?\C:\..` `current_exe()` lines up
/// with WMI's plain `C:\..` `ExecutablePath`.
#[cfg(any(windows, test))]
fn normalize_exe_path(path: &str) -> String {
    let trimmed = path.trim();
    let stripped = trimmed.strip_prefix(r"\\?\").unwrap_or(trimmed);
    stripped.replace('\\', "/").to_ascii_lowercase()
}

/// Pure selection of the in-scope holder PIDs to stop before the npm swap. Given
/// a snapshot of running symforge processes as `(pid, executable_path)`, returns
/// the PIDs whose executable path identifies the SAME binary as `self_exe` (the
/// binary npm will overwrite) EXCLUDING `self_pid` (this process). Paths are
/// compared after [`normalize_exe_path`] (verbatim-strip + slash-unify +
/// lowercase) so a divergent path FORM (extended-length prefix, slash style,
/// casing) cannot silently skip a genuine holder. Scoping by executable PATH —
/// never by image name — is Invariant 1 of `SELF_UPDATE_PROCEDURE.md`: an
/// unrelated symforge install at a different path is never stopped.
#[cfg(any(windows, test))]
fn select_inscope_holder_pids(
    processes: &[(u32, String)],
    self_exe: &str,
    self_pid: u32,
) -> Vec<u32> {
    let target = normalize_exe_path(self_exe);
    processes
        .iter()
        .filter(|(pid, path)| *pid != self_pid && normalize_exe_path(path) == target)
        .map(|(pid, _)| *pid)
        .collect()
}

/// Stop every OTHER symforge process running from this process's own executable
/// path (the binary npm will overwrite), identity-gated native terminate, so
/// the Windows image lock is released before the npm swap. Excludes this
/// process. Returns a summary line when any holder was stopped.
#[cfg(windows)]
fn stop_other_inscope_holders() -> Vec<String> {
    let Ok(self_exe) = std::env::current_exe() else {
        return Vec::new();
    };
    let self_exe = self_exe.to_string_lossy().to_string();
    let self_pid = std::process::id();

    let snapshot = enumerate_symforge_processes();
    let pids = select_inscope_holder_pids(&snapshot, &self_exe, self_pid);
    if pids.is_empty() {
        return Vec::new();
    }
    for pid in &pids {
        terminate_inscope_holder(*pid, &self_exe);
    }
    vec![format!(
        "{} other in-scope symforge holder(s) (pid {})",
        pids.len(),
        pids.iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    )]
}

/// Unix can replace a running binary's file (the open binary keeps the old inode
/// while the path takes the new file), so there is no EBUSY holder lock to clear
/// before the swap — the daemon stop in `stop_processes` is sufficient.
#[cfg(not(windows))]
fn stop_other_inscope_holders() -> Vec<String> {
    Vec::new()
}

/// Enumerate every running `symforge.exe` as `(pid, full_image_path)` natively
/// via a ToolHelp snapshot + `QueryFullProcessImageNameW` — spawning NO external
/// process (pattern ported from Terminal Commander's supervisor). The full image
/// path is required to scope by install path per Invariant 1. Returns an empty
/// list when the snapshot cannot be taken — a safe no-op that degrades to the
/// staged-guidance bail rather than killing anything on uncertainty.
#[cfg(windows)]
fn enumerate_symforge_processes() -> Vec<(u32, String)> {
    windows_native::enumerate_by_image_name("symforge.exe")
}

/// Identity-gated forced terminate of one in-scope holder. Re-verifies via the
/// OS — immediately before the kill — that `pid`'s image path is THIS install's
/// binary, then terminates natively.
///
/// There is deliberately NO graceful leg on Windows: `taskkill` without `/F`
/// is REFUSED by console processes ("can only be terminated forcefully"),
/// which is what the daemon/sidecar/MCP server are — the old graceful-then-
/// forced dance either mistook that refusal for "nothing to stop" (leaving
/// holders alive and the npm swap blocked) or added a wait that protected
/// nothing. The graceful path is the IPC daemon stop in `stop_processes`;
/// whatever still holds the binary after it is terminated here.
///
/// A pid that cannot be queried or whose image path no longer matches is left
/// alone (recycled-pid defense: never kill on uncertainty).
#[cfg(windows)]
fn terminate_inscope_holder(pid: u32, expected_exe: &str) {
    let expected = normalize_exe_path(expected_exe);
    let still_ours = windows_native::pid_image_full_path(pid)
        .is_some_and(|path| normalize_exe_path(&path) == expected);
    if still_ours {
        let _ = windows_native::terminate_process(pid);
    }
}

/// Native Win32 process control for the update swap, ported from Terminal
/// Commander's supervisor: ToolHelp enumeration, image-path identity, and
/// `TerminateProcess`. Spawns NO external process — no powershell/CIM, no
/// taskkill, no tasklist (corporate EDR flags spawned kill tools, and the
/// tools themselves were the source of the refused-graceful bug).
///
/// `unsafe` here is FFI-only, each call justified by a SAFETY comment; the
/// crate-level `unsafe_code = "deny"` is opted out per-item, matching the
/// repo's test-env precedent.
#[cfg(windows)]
mod windows_native {
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
        TH32CS_SNAPPROCESS,
    };
    use windows::Win32::System::Threading::{
        OpenProcess, PROCESS_NAME_FORMAT, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE,
        QueryFullProcessImageNameW, TerminateProcess,
    };

    /// RAII guard so a process/snapshot handle is closed on every return path.
    /// `CloseHandle` failure on drop is ignored: the handle is being discarded
    /// regardless.
    struct OwnedHandle(HANDLE);

    impl Drop for OwnedHandle {
        fn drop(&mut self) {
            // SAFETY: `self.0` is a handle we opened (OpenProcess /
            // CreateToolhelp32Snapshot) and have not closed yet. Closing it
            // exactly once here is the paired release for that open.
            #[allow(unsafe_code)]
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }

    /// Read `pid`'s FULL image path via the OS, or `None` if the process
    /// cannot be opened/queried (it exited, the pid is invalid, or access was
    /// denied). Callers treat `None` as "not ours" — never kill on uncertainty.
    #[allow(unsafe_code)]
    pub(super) fn pid_image_full_path(pid: u32) -> Option<String> {
        // SAFETY: OpenProcess takes a desired-access mask, an inherit BOOL,
        // and a pid; it returns a valid handle on success or an Err we map to
        // None. PROCESS_QUERY_LIMITED_INFORMATION is the least privilege that
        // permits QueryFullProcessImageNameW.
        let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()? };
        let proc = OwnedHandle(handle);

        let mut buf = [0u16; 1024];
        let mut len = u32::try_from(buf.len()).expect("image-path buffer length fits in u32");
        // SAFETY: `proc.0` is a live handle (just opened) valid for this call.
        // PROCESS_NAME_FORMAT(0) requests the win32 path form. `buf`/`len`
        // describe a properly sized, owned u16 buffer; the call writes at most
        // `len` code units and updates `len` to the count written. We check
        // the BOOL result and a non-zero length before reading the buffer.
        let ok = unsafe {
            QueryFullProcessImageNameW(
                proc.0,
                PROCESS_NAME_FORMAT(0),
                windows::core::PWSTR(buf.as_mut_ptr()),
                &raw mut len,
            )
            .is_ok()
        };
        if !ok || len == 0 {
            return None;
        }
        Some(String::from_utf16_lossy(&buf[..len as usize]))
    }

    /// Force-terminate `pid` natively. Caller has already identity-gated the
    /// pid. Returns an IO error if the process cannot be opened for
    /// termination or the terminate call fails.
    #[allow(unsafe_code)]
    pub(super) fn terminate_process(pid: u32) -> std::io::Result<()> {
        let access = PROCESS_TERMINATE | PROCESS_QUERY_LIMITED_INFORMATION;
        // SAFETY: OpenProcess as above; PROCESS_TERMINATE is the access right
        // TerminateProcess requires. The Err arm maps the Win32 error into an
        // io::Error without dereferencing anything.
        let handle = unsafe { OpenProcess(access, false, pid) }
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        let proc = OwnedHandle(handle);
        // SAFETY: `proc.0` is a live handle opened with PROCESS_TERMINATE.
        // TerminateProcess posts the exit and returns a BOOL we propagate as
        // an io::Error on failure. Exit code 1 mirrors the prior `taskkill /F`.
        unsafe { TerminateProcess(proc.0, 1) }.map_err(|e| std::io::Error::other(e.to_string()))?;
        Ok(())
    }

    /// Enumerate all processes whose image FILE NAME matches `image_name`
    /// (case-insensitive) as `(pid, full_image_path)`. The full path comes
    /// from the authoritative per-pid query, not the snapshot's base name, so
    /// callers can scope by install path. Self-exclusion is the caller's job
    /// (`select_inscope_holder_pids` excludes `self_pid`).
    #[allow(unsafe_code)]
    pub(super) fn enumerate_by_image_name(image_name: &str) -> Vec<(u32, String)> {
        // SAFETY: CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) returns a
        // valid snapshot handle or an Err we map to an empty list. The handle
        // is owned by `OwnedHandle` and released on every return path.
        let Ok(snapshot_handle) = (unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) })
        else {
            return Vec::new();
        };
        let snapshot = OwnedHandle(snapshot_handle);

        let mut entry = PROCESSENTRY32W {
            dwSize: u32::try_from(std::mem::size_of::<PROCESSENTRY32W>())
                .expect("PROCESSENTRY32W size fits in u32"),
            ..Default::default()
        };

        let mut found = Vec::new();
        // SAFETY: `snapshot.0` is a valid snapshot handle and `entry` is a
        // properly initialized PROCESSENTRY32W with `dwSize` set, as the API
        // requires. Process32FirstW fills `entry` and returns Ok/Err.
        let mut has_entry = unsafe { Process32FirstW(snapshot.0, &raw mut entry).is_ok() };
        while has_entry {
            let pid = entry.th32ProcessID;
            if exe_name_from_entry(&entry).eq_ignore_ascii_case(image_name)
                && let Some(path) = pid_image_full_path(pid)
            {
                found.push((pid, path));
            }
            // SAFETY: same invariants as Process32FirstW; advances `entry` to
            // the next process or returns Err at the end of the snapshot.
            has_entry = unsafe { Process32NextW(snapshot.0, &raw mut entry).is_ok() };
        }
        found
    }

    /// Decode the NUL-terminated UTF-16 `szExeFile` base name from a snapshot
    /// entry into a Rust string.
    fn exe_name_from_entry(entry: &PROCESSENTRY32W) -> String {
        let end = entry
            .szExeFile
            .iter()
            .position(|c| *c == 0)
            .unwrap_or(entry.szExeFile.len());
        String::from_utf16_lossy(&entry.szExeFile[..end])
    }
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
        shadow: Option<crate::path_shadow::ShadowReport>,
        shadow_checked_after_install: Option<bool>,
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
                shadow: None,
                shadow_checked_after_install: None,
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
        fn shadow_report(&mut self) -> Option<crate::path_shadow::ShadowReport> {
            // Record that the shadow check runs only AFTER a successful install,
            // so tests can assert ordering relative to the npm swap.
            self.shadow_checked_after_install = Some(!self.install_calls.is_empty());
            self.shadow.clone()
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
        assert_eq!(
            ops.shadow_checked_after_install,
            Some(true),
            "PATH-shadow check must run, and only after a successful install"
        );
    }

    #[test]
    fn orchestrate_update_warns_but_succeeds_when_a_same_version_shadow_wins() {
        // The shadow reports the SAME version the install resolved to (7.15.4),
        // which the reactive stale-version bail cannot detect. The proactive
        // shadow check must still fire; the update must still SUCCEED (the
        // warning is advisory, not a failure).
        let mut ops = ok_ops();
        ops.shadow = Some(crate::path_shadow::ShadowReport {
            our_path: std::path::PathBuf::from("/home/you/.npm-global/bin/symforge"),
            our_version: Some("7.15.4".to_string()),
            shadow_path: std::path::PathBuf::from("/usr/local/bin/symforge"),
            shadow_version: Some("7.15.4".to_string()),
            kind: crate::path_shadow::ShadowKind::RootSystem,
        });

        orchestrate_update("linux", "x86_64", &mut ops)
            .expect("a same-version shadow is advisory, not fatal");

        assert_eq!(
            ops.shadow_checked_after_install,
            Some(true),
            "shadow check ran after the install even on the same-version case"
        );
        assert!(
            ops.reregistered_after_install,
            "the advisory shadow warning must not short-circuit re-registration"
        );
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
        // The actionable hint: clear any holder, then the exact one-step install.
        assert!(
            msg.contains("Ensure no running symforge process is holding the binary"),
            "{msg}"
        );
        assert!(
            msg.contains("npm install -g symforge@latest symforge-linux-x64@latest"),
            "{msg}"
        );
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
    fn orchestrate_update_windows_failure_gives_staged_self_lock_guidance() {
        // On Windows the remaining lock after stopping other holders is normally
        // the update process's OWN binary. The failure must name that self-lock and
        // give the exact one-step remediation for a plain shell — while staying
        // HONEST about other holders (it must not claim it stopped all of them).
        let mut ops = FakeOps {
            install_result: false,
            ..Default::default()
        };

        let err = orchestrate_update("windows", "x86_64", &mut ops)
            .expect_err("a blocked Windows swap must fail with staged guidance");

        let msg = err.to_string();
        assert!(msg.contains("exited unsuccessfully"), "{msg}");
        assert!(
            msg.contains("running .exe") && msg.contains("PLAIN shell"),
            "must name the self-lock + a plain-shell remediation: {msg}"
        );
        assert!(
            msg.contains("npm install -g symforge@latest symforge-windows-x64@latest"),
            "must print the exact install command: {msg}"
        );
        assert!(
            msg.contains("init --client all"),
            "must print the client re-registration step: {msg}"
        );
        // Honesty (M2): never claim ALL holders were stopped (enumeration may find
        // or stop none), and always cover an un-enumerable holder (an MCP client).
        assert!(
            !msg.contains("All OTHER"),
            "must not over-claim that all holders were stopped: {msg}"
        );
        assert!(
            msg.contains("close them and rerun"),
            "must cover an un-enumerated holder (MCP client) case: {msg}"
        );
        assert!(
            !ops.reregistered_after_install,
            "a blocked swap must not re-register clients"
        );
    }

    #[test]
    fn select_inscope_holder_pids_matches_same_path_excludes_self_and_other_installs() {
        let procs = vec![
            (100u32, r"C:\npm\symforge.exe".to_string()), // in scope
            (200u32, r"C:\NPM\SYMFORGE.EXE".to_string()), // same path, case-insensitive -> in scope
            (300u32, r"D:\other\symforge.exe".to_string()), // different install -> OUT of scope (Invariant 1)
            (999u32, r"C:\npm\symforge.exe".to_string()),   // self -> excluded
        ];
        let pids = select_inscope_holder_pids(&procs, r"C:\npm\symforge.exe", 999);
        assert_eq!(
            pids,
            vec![100, 200],
            "only OTHER processes at the same executable path are in scope"
        );
    }

    #[test]
    fn select_inscope_holder_pids_empty_when_only_self_runs() {
        let procs = vec![(999u32, r"C:\npm\symforge.exe".to_string())];
        assert!(
            select_inscope_holder_pids(&procs, r"C:\npm\symforge.exe", 999).is_empty(),
            "nothing to stop when this process is the only holder"
        );
    }

    #[test]
    fn select_inscope_holder_pids_normalizes_slash_and_verbatim_path_forms() {
        // M1: a genuine holder must be matched even when its path FORM differs from
        // current_exe() — forward vs back slash, or the extended-length verbatim
        // (\\?\) prefix that current_exe() can return on Windows.
        let procs = vec![
            (100u32, r"C:\npm\symforge.exe".to_string()), // back slashes
            (200u32, "C:/npm/symforge.exe".to_string()),  // forward slashes
            (300u32, r"\\?\C:\npm\symforge.exe".to_string()), // verbatim prefix
            (400u32, r"C:\other\symforge.exe".to_string()), // different install -> excluded
        ];
        // self_exe given in the verbatim + upper-case form; all three same-binary
        // rows (100/200/300) must still be selected, the different install excluded.
        let pids = select_inscope_holder_pids(&procs, r"\\?\C:\NPM\symforge.exe", 999);
        assert_eq!(
            pids,
            vec![100, 200, 300],
            "path-form variants of the same binary are all in scope (M1)"
        );
    }

    /// Copy a benign long-lived system exe to `<tmp>/symforge.exe` and spawn
    /// it so the process's image FILE NAME is the symforge binary name (the
    /// pattern Terminal Commander's supervisor tests use). The returned
    /// `TempDir` keeps the copied exe alive for the test's duration.
    #[cfg(windows)]
    fn spawn_fake_symforge() -> (tempfile::TempDir, std::process::Child) {
        let dir = tempfile::tempdir().expect("tempdir");
        let fake = dir.path().join("symforge.exe");
        let ping = std::path::Path::new(r"C:\Windows\System32\PING.EXE");
        std::fs::copy(ping, &fake).expect("copy ping.exe to symforge.exe");
        // `ping -n 30 127.0.0.1` stays alive ~30s; far longer than the test.
        let child = crate::process_util::hidden_command(&fake)
            .args(["-n", "30", "127.0.0.1"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("spawn fake symforge");
        (dir, child)
    }

    #[test]
    #[cfg(windows)]
    fn enumerate_symforge_processes_finds_spawned_symforge_image_with_full_path() {
        let (dir, mut child) = spawn_fake_symforge();
        let pid = child.id();
        let procs = enumerate_symforge_processes();
        let _ = child.kill();
        let _ = child.wait();
        let found = procs.iter().find(|(p, _)| *p == pid);
        let (_, path) = found.expect("native enumeration must list the spawned symforge.exe");
        assert_eq!(
            normalize_exe_path(path),
            normalize_exe_path(&dir.path().join("symforge.exe").to_string_lossy()),
            "the enumerated entry must carry the FULL image path for install scoping"
        );
    }

    #[test]
    #[cfg(windows)]
    fn terminate_inscope_holder_kills_matching_path_and_spares_mismatch() {
        // Mismatched expected path -> the holder must be left alive
        // (Invariant 1: never touch another install; recycled-pid defense).
        let (dir, mut child) = spawn_fake_symforge();
        let pid = child.id();
        terminate_inscope_holder(pid, r"C:\some\other\install\symforge.exe");
        assert!(
            child.try_wait().expect("try_wait").is_none(),
            "a path-mismatched pid must NOT be terminated"
        );

        // Matching expected path -> terminated (this is the bug the taskkill
        // graceful leg used to mask: console processes refused the graceful
        // close and never reached the forced kill).
        let expected = dir
            .path()
            .join("symforge.exe")
            .to_string_lossy()
            .to_string();
        terminate_inscope_holder(pid, &expected);
        let status = child.wait().expect("wait on terminated child");
        assert!(
            !status.success(),
            "TerminateProcess(exit=1) must make the holder exit non-zero"
        );

        // A second terminate of the now-dead pid must be a no-op (identity
        // gate: the pid no longer resolves to this image path).
        terminate_inscope_holder(pid, &expected);
    }

    #[test]
    #[cfg(windows)]
    fn terminate_inscope_holder_never_kills_the_test_runner() {
        // The test runner is alive but its image is the test binary, not the
        // expected symforge path -> must be refused (this failing would kill
        // the test host, exactly TC's guard).
        terminate_inscope_holder(std::process::id(), r"C:\npm\symforge.exe");
    }

    #[test]
    fn select_inscope_rejects_other_install_and_absent_rows() {
        let procs = vec![
            (100u32, r"C:\npm\symforge.exe".to_string()),
            (200u32, r"D:\other\symforge.exe".to_string()),
            (300u32, r"\\?\C:\NPM\symforge.exe".to_string()),
        ];
        assert_eq!(
            select_inscope_holder_pids(&procs, r"C:\NPM\symforge.exe", 999),
            vec![100, 300],
            "same-path rows (any form) are in scope; the other install is not"
        );
        assert_eq!(
            select_inscope_holder_pids(&procs, r"C:\npm\symforge.exe", 100),
            vec![300],
            "the self pid must never be selected, even at the same path"
        );
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
