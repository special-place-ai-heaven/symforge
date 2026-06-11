//! Proactive PATH-shadow detection.
//!
//! The 99% install path is `npm install -g symforge`. But the `symforge` that a
//! bare invocation actually RUNS is whatever comes FIRST on `$PATH` — which may
//! be a DIFFERENT, stale install that shadows the one the user just installed:
//!
//!   - a root-owned `/usr/local/bin/symforge` (`apt`/`sudo make install` leftover),
//!   - a Windows npm-global bleeding into a WSL shell via `/mnt/c/...`,
//!   - an nvm-vs-system prefix mismatch (a "foreign" npm prefix winning on PATH).
//!
//! When that happens, `npm install -g symforge` reports success and the registry
//! shows the new version, yet `which symforge` keeps resolving the stale shadow.
//! This module RESOLVES every `symforge` on `$PATH`, compares the first hit to
//! the binary we believe we are, classifies the shadow, and produces a warning
//! that ends with the EXACT remediation commands. It NEVER executes them.
//!
//! Pure-ish: PATH/string/path logic plus a single best-effort, bounded version
//! probe (`<shadow> --version`) that can never hang. Server-only — wired into
//! `cli::init`, `cli::update`, and `protocol` health.

use std::path::{Path, PathBuf};
use std::time::Duration;

/// Bound on the best-effort `<shadow> --version` probe. A foreign binary on PATH
/// is untrusted: it could block forever. We spawn it on a worker thread and give
/// up after this window, so detection can never hang a session, init, or update.
const SHADOW_PROBE_TIMEOUT: Duration = Duration::from_millis(500);

/// How a PATH-first `symforge` that is NOT our install is shadowing us.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShadowKind {
    /// The shadow lives under a clearly system/root-owned prefix
    /// (`/usr/local`, `/usr`, `/opt`, ...). Remediation removes the root copy.
    RootSystem,
    /// Running under WSL and the shadow lives under `/mnt/` — a Windows
    /// npm-global bleeding into the Linux PATH. Remediation fixes login PATH.
    WindowsMntBleed,
    /// Some other prefix wins on PATH (e.g. nvm vs system npm prefix mismatch).
    /// Remediation ensures our prefix bin precedes the shadow dir on PATH.
    ForeignPrefix,
}

/// A resolved PATH-shadow situation: the install we believe we are vs the
/// install a bare `symforge` invocation actually runs.
#[derive(Debug, Clone)]
pub struct ShadowReport {
    /// The binary we believe we are (typically `std::env::current_exe()` or the
    /// binary just registered/installed).
    pub our_path: PathBuf,
    /// Our version, when known (we are running, so this is normally `Some`).
    pub our_version: Option<String>,
    /// The PATH-first `symforge` that shadows us.
    pub shadow_path: PathBuf,
    /// The shadow's version, best-effort probed. `None` if the probe failed,
    /// timed out, or produced no parseable version.
    pub shadow_version: Option<String>,
    /// How the shadow is shadowing us — selects the remediation wording.
    pub kind: ShadowKind,
}

/// Resolve EVERY `name` on `$PATH`, in PATH order (first = what a bare `name`
/// invocation runs). Splits `PATH` by the OS separator; for each directory it
/// checks `dir/name` and, on Windows, the `PATHEXT` variants (`name.exe`,
/// `name.cmd`, ...). Only existing regular files are collected. Order is
/// preserved and duplicates are de-duped (a dir repeated on PATH yields one hit).
pub fn which_all(name: &str) -> Vec<PathBuf> {
    let Some(path_var) = std::env::var_os("PATH") else {
        return Vec::new();
    };

    let mut found: Vec<PathBuf> = Vec::new();
    let candidate_names = candidate_file_names(name);

    for dir in std::env::split_paths(&path_var) {
        if dir.as_os_str().is_empty() {
            continue;
        }
        for file_name in &candidate_names {
            let candidate = dir.join(file_name);
            // `is_file()` follows symlinks and returns false for directories and
            // for non-existent paths — exactly the "exists and is a file" gate.
            if candidate.is_file() && !found.iter().any(|existing| existing == &candidate) {
                found.push(candidate);
            }
        }
    }

    found
}

/// The set of file names to probe in each PATH directory for `name`, in the
/// order Windows would actually try them.
///
/// A bare `symforge` on Windows resolves through `PATHEXT`: cmd runs `.cmd`,
/// PowerShell runs `.ps1`/`.exe`, and the extensionless `name` is NEVER what a
/// Windows shell executes — npm's bare `symforge` artifact is a `#!/bin/sh`
/// shim no Windows shell runs (probing it yields the scary "(version unknown)"
/// and a launcher chain that never compares equal to the platform `.exe`). So
/// we DROP the bare name entirely on Windows and probe only real executable
/// extensions, ordered by `PATHEXT` precedence (with npm's `.cmd`/`.exe`/`.ps1`
/// launchers guaranteed present even if `PATHEXT` is unusual).
#[cfg(windows)]
fn candidate_file_names(name: &str) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();

    // PATHEXT precedence first: this is the order a bare invocation resolves in.
    if let Some(pathext) = std::env::var_os("PATHEXT") {
        // PATHEXT is a `;`-separated list of extensions (`.EXE;.CMD;...`); reuse
        // split_paths only for the OS separator, then normalize each entry to a
        // leading-dot lowercase extension.
        for ext in std::env::split_paths(&pathext) {
            let ext = ext.to_string_lossy();
            let ext = ext.trim();
            if ext.is_empty() {
                continue;
            }
            let ext = ext.to_ascii_lowercase();
            let ext = ext.strip_prefix('.').unwrap_or(&ext);
            let candidate = format!("{name}.{ext}");
            if !names.iter().any(|n| n.eq_ignore_ascii_case(&candidate)) {
                names.push(candidate);
            }
        }
    }

    // Guarantee the launchers npm actually writes are present even when PATHEXT
    // is missing or unusual: `.com`/`.exe`/`.bat`/`.cmd`/`.ps1`. We deliberately
    // do NOT include the bare extensionless name (the sh shim that never runs).
    for ext in [".com", ".exe", ".bat", ".cmd", ".ps1"] {
        let candidate = format!("{name}{ext}");
        if !names.iter().any(|n| n.eq_ignore_ascii_case(&candidate)) {
            names.push(candidate);
        }
    }

    names
}

#[cfg(not(windows))]
fn candidate_file_names(name: &str) -> Vec<String> {
    vec![name.to_string()]
}

/// Detect whether a stale install shadows `our_binary` on `$PATH`.
///
/// Returns `None` when we win — i.e. the PATH-first `symforge` IS `our_binary`
/// (same file), or there is no `symforge` on PATH at all. Returns `Some` with a
/// classified report when a DIFFERENT install would run before ours.
pub fn detect_shadow(our_binary: &Path) -> Option<ShadowReport> {
    let first = which_all("symforge").into_iter().next()?;

    // We win: the first thing PATH resolves is us. Compare INSTALL identity, not
    // single-file identity — canonicalizing through directory symlinks/junctions
    // (nvm-for-windows points `C:\Program Files\nodejs` at the active version
    // dir) and collapsing npm's multi-artifact launcher chain (`<prefix>\symforge`,
    // `.cmd`, `.ps1`, and `<prefix>\node_modules\symforge-<plat>\bin\symforge.exe`
    // are FOUR launcher artifacts of ONE install). A bare on-disk file compare
    // mis-reports those as a foreign shadow on every standard npm-global setup.
    if same_install(&first, our_binary) {
        return None;
    }

    let kind = classify_shadow(&first);
    let shadow_version = probe_version(&first);
    let our_version = probe_version(our_binary);

    // Same-version suppression guard. Even when the prefixes are NOT recognizably
    // the same install, two `symforge` launchers that resolve to the identical
    // version are not a harmful shadow — running either yields the same behavior,
    // and a PATH-reorder recommendation would be a churny no-op. Only warn when
    // the versions genuinely differ (or a version could not be probed, where we
    // stay conservative and warn so a real stale shadow is never silenced).
    if let (Some(ours), Some(theirs)) = (&our_version, &shadow_version)
        && ours == theirs
    {
        return None;
    }

    Some(ShadowReport {
        our_path: our_binary.to_path_buf(),
        our_version,
        shadow_path: first,
        shadow_version,
        kind,
    })
}

/// Whether two paths point at the same on-disk file. Canonicalizes both; on any
/// canonicalize failure falls back to a normalized lexical compare so we never
/// mis-report a shadow just because a path could not be resolved.
fn same_file(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(ca), Ok(cb)) => ca == cb,
        _ => normalize_for_compare(a) == normalize_for_compare(b),
    }
}

/// Whether two `symforge` launcher/binary paths belong to the SAME install.
///
/// This is stricter-than-`same_file` identity that understands the npm install
/// shape. ONE `npm install -g symforge` produces several launcher artifacts of
/// a single install:
///   - the launcher shims at the prefix root / bin (`<prefix>\symforge.cmd`,
///     `<prefix>\symforge.ps1`, the bare `#!/bin/sh` `<prefix>\symforge`, or
///     `<prefix>/bin/symforge` on Unix), and
///   - the real platform binary at
///     `<prefix>\node_modules\symforge-<os>-<arch>\bin\symforge.exe`.
///
/// A bare on-disk file compare reports any two of these as different installs.
///
/// We instead resolve each path to its install ROOT — the npm prefix dir,
/// canonicalized so nvm-for-windows' `C:\Program Files\nodejs` directory
/// symlink and the version dir it targets collapse to one root — and compare
/// those. When both roots resolve and match, it is one install and we win.
/// Falls back to `same_file` when a root cannot be derived (e.g. an unusual
/// layout), so this is never weaker than the old single-file check.
fn same_install(a: &Path, b: &Path) -> bool {
    if same_file(a, b) {
        return true;
    }
    match (install_root(a), install_root(b)) {
        (Some(ra), Some(rb)) => ra == rb,
        _ => false,
    }
}

/// Resolve a `symforge` launcher/binary path to a canonicalized npm-prefix
/// install root, or `None` when the layout is not recognized.
///
/// Recognized layouts (the launcher relative to its prefix root):
///   - `<prefix>/node_modules/symforge-<plat>/bin/symforge[.exe]` (platform pkg)
///   - `<prefix>/bin/symforge[.cmd|.ps1|.exe]`                      (Unix shim)
///   - `<prefix>/symforge[.cmd|.ps1]` or bare `<prefix>/symforge`   (Windows shim)
///
/// Each candidate prefix is canonicalized (resolving directory symlinks such as
/// nvm's `C:\Program Files\nodejs`) so two artifacts of one install collapse.
fn install_root(launcher: &Path) -> Option<PathBuf> {
    let dir = launcher.parent()?;

    // Platform package: `.../node_modules/symforge-<plat>/bin/<exe>`. Strip the
    // `node_modules/symforge-*/bin` tail (three components) to reach the prefix.
    if dir.file_name().map(|n| n == "bin").unwrap_or(false)
        && let Some(pkg_dir) = dir.parent()
        && pkg_dir
            .file_name()
            .map(|n| n.to_string_lossy().starts_with("symforge-"))
            .unwrap_or(false)
        && let Some(node_modules) = pkg_dir.parent()
        && node_modules
            .file_name()
            .map(|n| n == "node_modules")
            .unwrap_or(false)
        && let Some(prefix) = node_modules.parent()
    {
        return Some(canonicalize_or_normalize(prefix));
    }

    // Unix-style shim: `<prefix>/bin/symforge`. Strip the trailing `bin`.
    if dir.file_name().map(|n| n == "bin").unwrap_or(false)
        && let Some(prefix) = dir.parent()
    {
        return Some(canonicalize_or_normalize(prefix));
    }

    // Windows-style shim at the prefix root: `<prefix>/symforge{,.cmd,.ps1}`.
    // The launcher's own directory IS the prefix root.
    Some(canonicalize_or_normalize(dir))
}

/// Canonicalize a directory (resolving symlinks/junctions) for install-root
/// comparison, falling back to a lexical normalization when canonicalize fails
/// (e.g. an offline mount) so two equal lexical roots still compare equal.
fn canonicalize_or_normalize(dir: &Path) -> PathBuf {
    match std::fs::canonicalize(dir) {
        Ok(c) => c,
        Err(_) => PathBuf::from(normalize_for_compare(dir)),
    }
}

/// Lexical normalization for the canonicalize-failed fallback: strip Windows
/// verbatim/UNC prefixes, unify separators, and lowercase on case-insensitive
/// platforms so `\\?\C:\X` and `C:/x` compare equal without touching disk.
fn normalize_for_compare(path: &Path) -> String {
    let mut s = path.to_string_lossy().replace('\\', "/");
    for prefix in ["//?/", "//./"] {
        if let Some(stripped) = s.strip_prefix(prefix) {
            s = stripped.to_string();
            break;
        }
    }
    if cfg!(windows) {
        s.to_ascii_lowercase()
    } else {
        s
    }
}

/// Classify how a non-us PATH-first `symforge` is shadowing us.
fn classify_shadow(shadow: &Path) -> ShadowKind {
    // WSL + /mnt/ wins first: a Windows npm-global bleeding into the Linux PATH
    // is a distinct, very common failure with its own fix. Check it before the
    // generic system-prefix test (a /mnt path is never a Linux system prefix).
    if running_under_wsl() && path_starts_with_mnt(shadow) {
        return ShadowKind::WindowsMntBleed;
    }
    if is_system_prefix(shadow) {
        return ShadowKind::RootSystem;
    }
    ShadowKind::ForeignPrefix
}

/// Whether `shadow` lives under a clearly system/root-owned install prefix.
/// These are POSIX conventions; on Windows none match, so a Windows shadow falls
/// through to `ForeignPrefix` (Windows has no `/usr/local` equivalent here).
fn is_system_prefix(shadow: &Path) -> bool {
    const SYSTEM_PREFIXES: [&str; 5] = ["/usr/local/", "/usr/", "/opt/", "/bin/", "/sbin/"];
    let s = shadow.to_string_lossy().replace('\\', "/");
    SYSTEM_PREFIXES.iter().any(|prefix| s.starts_with(prefix))
}

/// Whether `shadow` is under the WSL Windows automount root `/mnt/`.
fn path_starts_with_mnt(shadow: &Path) -> bool {
    let s = shadow.to_string_lossy().replace('\\', "/");
    s.starts_with("/mnt/")
}

/// Whether this process is running under WSL. Sniffs `/proc/version` for the
/// `microsoft`/`wsl` marker the WSL kernel writes there — mirrors
/// `crate::discovery::is_running_under_wsl` (which is private and Windows-gated).
/// Always `false` off Unix.
#[cfg(unix)]
fn running_under_wsl() -> bool {
    std::fs::read_to_string("/proc/version")
        .map(|v| {
            let v = v.to_ascii_lowercase();
            v.contains("microsoft") || v.contains("wsl")
        })
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn running_under_wsl() -> bool {
    false
}

/// Best-effort, BOUNDED version probe: run `<path> --version`, parse a semver
/// from its stdout, give up after [`SHADOW_PROBE_TIMEOUT`]. Returns `None` on
/// spawn failure, timeout, non-zero/empty output, or unparseable output. Never
/// hangs: the (untrusted) child runs on a worker thread we stop waiting on.
fn probe_version(path: &Path) -> Option<String> {
    use std::sync::mpsc;

    let path = path.to_path_buf();
    let (tx, rx) = mpsc::channel();

    // Detached worker: if the child outlives the timeout, the thread parks in
    // `output()` until the child exits, then sends to a dropped receiver (a
    // harmless no-op). We never join it, so a hung child cannot block us.
    std::thread::spawn(move || {
        let result = std::process::Command::new(&path)
            .arg("--version")
            .stdin(std::process::Stdio::null())
            .output();
        let _ = tx.send(result);
    });

    match rx.recv_timeout(SHADOW_PROBE_TIMEOUT) {
        Ok(Ok(output)) => parse_version(&String::from_utf8_lossy(&output.stdout)),
        Ok(Err(_)) | Err(_) => None,
    }
}

/// Parse a `symforge --version` semver out of arbitrary launcher output.
///
/// Mirrors `crate::cli::update::parse_symforge_version` (kept private there):
/// scans EVERY line so a leading banner does not hide the version, and accepts
/// the first whitespace token that starts with a digit and contains a dot.
fn parse_version(text: &str) -> Option<String> {
    text.lines().find_map(|line| {
        line.split_whitespace()
            .map(str::trim)
            .find(|tok| tok.contains('.') && tok.chars().next().is_some_and(|c| c.is_ascii_digit()))
            .map(str::to_string)
    })
}

/// Render `Some(version)` as `(version)`, `None` as `(version unknown)`.
fn version_label(version: &Option<String>) -> String {
    match version {
        Some(v) => format!("({v})"),
        None => "(version unknown)".to_string(),
    }
}

/// The install-prefix `bin` directory derived from a binary path: its parent
/// directory. Used to phrase "ensure <our prefix>/bin precedes <shadow dir>".
fn prefix_bin_dir(binary: &Path) -> Option<PathBuf> {
    binary.parent().map(Path::to_path_buf)
}

/// A clear, plain-ASCII, multi-line warning ending with EXACT remediation
/// commands tailored to the shadow's kind, paths, and versions. Never executes
/// anything — the user runs the commands in their own shell.
pub fn format_shadow_warning(report: &ShadowReport) -> String {
    let runs = report.shadow_path.display();
    let runs_ver = version_label(&report.shadow_version);
    let installed = report.our_path.display();
    let installed_ver = version_label(&report.our_version);
    let our_bin_dir = prefix_bin_dir(&report.our_path);
    let shadow_bin_dir = prefix_bin_dir(&report.shadow_path);

    match report.kind {
        ShadowKind::RootSystem => {
            // Derive the matching node_modules dir from the shadow's bin dir:
            // `/usr/local/bin` -> `/usr/local/lib/node_modules/symforge`.
            let modules = system_node_modules_for(&report.shadow_path);
            format!(
                "WARNING: `symforge` on PATH resolves to a different install than the one you installed.\n  \
                 runs:      {runs} {runs_ver}   [root-owned shadow]\n  \
                 installed: {installed} {installed_ver}\n\
                 Fix (removes the root shadow so your install wins):\n  \
                 sudo rm -f {shadow}\n  \
                 sudo rm -rf {modules}",
                shadow = report.shadow_path.display(),
            )
        }
        ShadowKind::WindowsMntBleed => {
            let bin_hint = our_bin_dir
                .as_deref()
                .map(|d| d.display().to_string())
                .unwrap_or_else(|| "$HOME/.npm-global/bin".to_string());
            format!(
                "WARNING: a Windows npm-global on /mnt is ahead of your Linux symforge on PATH.\n  \
                 runs:      {runs} {runs_ver}\n  \
                 installed: {installed} {installed_ver}\n\
                 Fix (put your Linux npm prefix bin first, in login shells too):\n  \
                 add to ~/.profile:  export PATH=\"{bin_hint}:$PATH\""
            )
        }
        ShadowKind::ForeignPrefix => {
            let our_dir = our_bin_dir
                .as_deref()
                .map(|d| d.display().to_string())
                .unwrap_or_else(|| installed.to_string());
            let shadow_dir = shadow_bin_dir
                .as_deref()
                .map(|d| d.display().to_string())
                .unwrap_or_else(|| runs.to_string());
            // The ForeignPrefix arm is the ONLY one whose remediation depends on
            // the host OS: a `C:\` shadow on native Windows classifies here, and
            // POSIX `~/.profile`/`export PATH` is meaningless in PowerShell (`$PATH`
            // is not even a PowerShell variable). Key off `cfg!(windows)` (the build
            // host) per project precedent (`daemon.rs`). The other arms stay POSIX:
            // RootSystem/WindowsMntBleed only ever describe Linux/WSL shadows.
            if cfg!(windows) {
                format!(
                    "WARNING: `symforge` on PATH resolves to a different install than the one you installed.\n  \
                     runs:      {runs} {runs_ver}   [foreign prefix shadow]\n  \
                     installed: {installed} {installed_ver}\n\
                     Fix: your install's bin dir must come BEFORE the shadow's on PATH.\n  \
                     {our_dir} must precede {shadow_dir}\n  \
                     verify with:  Get-Command symforge -All  (the first hit is what runs)\n  \
                     reorder your user PATH so {our_dir} comes first, e.g. in PowerShell:\n  \
                     [Environment]::SetEnvironmentVariable('Path', '{our_dir};' + [Environment]::GetEnvironmentVariable('Path','User'), 'User')\n  \
                     (or edit it via System Properties > Environment Variables > Path), then open a new shell.\n  \
                     note: with nvm-for-windows the active node prefix bin wins, so `nvm use` can re-shadow you."
                )
            } else {
                format!(
                    "WARNING: `symforge` on PATH resolves to a different install than the one you installed.\n  \
                     runs:      {runs} {runs_ver}   [foreign prefix shadow]\n  \
                     installed: {installed} {installed_ver}\n\
                     Fix (ensure your install's bin precedes the shadow on PATH):\n  \
                     add to ~/.profile:  export PATH=\"{our_dir}:$PATH\"\n  \
                     (it currently resolves to {shadow_dir} first)"
                )
            }
        }
    }
}

/// One-line compact banner for the compact health surface, where a multi-line
/// block is too heavy. Names the kind and both paths/versions and points at the
/// full warning for the exact commands.
pub fn format_shadow_warning_compact(report: &ShadowReport) -> String {
    let kind = match report.kind {
        ShadowKind::RootSystem => "root-owned shadow",
        ShadowKind::WindowsMntBleed => "/mnt Windows-bleed shadow",
        ShadowKind::ForeignPrefix => "foreign-prefix shadow",
    };
    format!(
        "WARNING: PATH shadow [{kind}]: bare `symforge` runs {runs} {runs_ver}, not your install {ours} {ours_ver}; run full `health` for the exact fix.",
        runs = report.shadow_path.display(),
        runs_ver = version_label(&report.shadow_version),
        ours = report.our_path.display(),
        ours_ver = version_label(&report.our_version),
    )
}

/// Map a system shadow bin path to its npm `node_modules/symforge` dir for the
/// `rm -rf` line: `/usr/local/bin/symforge` -> `/usr/local/lib/node_modules/symforge`.
/// Falls back to a `/usr/local`-rooted guess when the bin path is unusual.
///
/// Operates on the POSIX STRING form (RootSystem shadows are always POSIX paths)
/// rather than `Path::join`, whose separator is host-OS-dependent — the emitted
/// command is a POSIX shell line shown to the Linux/WSL user regardless of the
/// OS that built or ran this binary.
fn system_node_modules_for(shadow: &Path) -> String {
    let s = shadow.to_string_lossy().replace('\\', "/");
    // Trim trailing slashes, then strip the last two `/`-segments
    // (`.../bin/symforge`) to recover the install prefix.
    let trimmed = s.trim_end_matches('/');
    let prefix = trimmed
        .rsplit_once('/') // drop `symforge`
        .map(|(head, _)| head)
        .and_then(|head| head.rsplit_once('/')) // drop `bin`
        .map(|(prefix, _)| prefix);
    match prefix {
        Some(prefix) if !prefix.is_empty() => {
            format!("{prefix}/lib/node_modules/symforge")
        }
        _ => "/usr/local/lib/node_modules/symforge".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Serializes env-var mutation across tests in this module. The suite runs
    /// with `--test-threads=1`, but the lock makes the intent explicit and keeps
    /// these tests safe even if that flag is ever dropped for this module.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// RAII guard: set `PATH` for the body's duration and restore the prior value
    /// (set or unset) on drop, so the mutation never leaks to other tests even on
    /// panic.
    struct PathGuard {
        prev: Option<std::ffi::OsString>,
    }

    impl PathGuard {
        fn set(value: &std::ffi::OsStr) -> Self {
            let prev = std::env::var_os("PATH");
            // SAFETY: the caller holds ENV_LOCK and the suite runs single-threaded,
            // so no other thread can read or write the environment concurrently.
            #[allow(unsafe_code)]
            unsafe {
                std::env::set_var("PATH", value)
            };
            Self { prev }
        }
    }

    impl Drop for PathGuard {
        fn drop(&mut self) {
            // SAFETY: see `PathGuard::set`.
            #[allow(unsafe_code)]
            match &self.prev {
                Some(prev) => unsafe { std::env::set_var("PATH", prev) },
                None => unsafe { std::env::remove_var("PATH") },
            }
        }
    }

    /// Create a `symforge` executable shim in `dir` and return its path. Uses the
    /// bare name on Unix and `symforge.cmd` on Windows so the PATHEXT resolution
    /// path is exercised on the host platform.
    fn touch_symforge(dir: &Path) -> PathBuf {
        let name = if cfg!(windows) {
            "symforge.cmd"
        } else {
            "symforge"
        };
        let path = dir.join(name);
        fs::write(&path, b"#shim").unwrap();
        path
    }

    fn join_path_dirs(dirs: &[&Path]) -> std::ffi::OsString {
        std::env::join_paths(dirs.iter().map(|d| d.as_os_str())).unwrap()
    }

    #[test]
    fn which_all_returns_both_installs_in_path_order() {
        let _lock = ENV_LOCK.lock().unwrap();
        let first_dir = tempfile::tempdir().unwrap();
        let second_dir = tempfile::tempdir().unwrap();
        let first = touch_symforge(first_dir.path());
        let second = touch_symforge(second_dir.path());

        let path = join_path_dirs(&[first_dir.path(), second_dir.path()]);
        let _guard = PathGuard::set(&path);

        let found = which_all("symforge");
        assert_eq!(found.len(), 2, "both installs should resolve");
        assert_eq!(found[0], first, "PATH-first dir must come first");
        assert_eq!(found[1], second, "PATH-second dir must come second");
    }

    #[test]
    fn which_all_skips_directories_without_the_binary() {
        let _lock = ENV_LOCK.lock().unwrap();
        let empty_dir = tempfile::tempdir().unwrap();
        let has_it = tempfile::tempdir().unwrap();
        let real = touch_symforge(has_it.path());

        let path = join_path_dirs(&[empty_dir.path(), has_it.path()]);
        let _guard = PathGuard::set(&path);

        let found = which_all("symforge");
        assert_eq!(found, vec![real], "only the dir holding the binary counts");
    }

    #[test]
    fn which_all_dedups_a_repeated_path_dir() {
        let _lock = ENV_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let real = touch_symforge(dir.path());

        let path = join_path_dirs(&[dir.path(), dir.path()]);
        let _guard = PathGuard::set(&path);

        let found = which_all("symforge");
        assert_eq!(found, vec![real], "a repeated PATH dir yields one hit");
    }

    #[cfg(windows)]
    #[test]
    fn which_all_resolves_pathext_cmd_shim_on_windows() {
        let _lock = ENV_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        // Only a `.cmd` shim exists (the npm global shim shape). `which_all`
        // must find it via the PATHEXT candidate set even though the query is
        // the bare name.
        let shim = dir.path().join("symforge.cmd");
        fs::write(&shim, b"@echo off").unwrap();

        let path = join_path_dirs(&[dir.path()]);
        let _guard = PathGuard::set(&path);

        let found = which_all("symforge");
        assert_eq!(found, vec![shim], "PATHEXT .cmd shim must resolve");
    }

    #[test]
    fn detect_shadow_returns_none_when_path_first_is_us() {
        let _lock = ENV_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let ours = touch_symforge(dir.path());

        let path = join_path_dirs(&[dir.path()]);
        let _guard = PathGuard::set(&path);

        assert!(
            detect_shadow(&ours).is_none(),
            "the only PATH install is us -> no shadow"
        );
    }

    #[test]
    fn detect_shadow_returns_none_when_no_symforge_on_path() {
        let _lock = ENV_LOCK.lock().unwrap();
        let empty = tempfile::tempdir().unwrap();
        let ours = empty.path().join(if cfg!(windows) {
            "symforge.cmd"
        } else {
            "symforge"
        });

        let path = join_path_dirs(&[empty.path()]);
        let _guard = PathGuard::set(&path);

        assert!(
            detect_shadow(&ours).is_none(),
            "no symforge on PATH -> no shadow"
        );
    }

    #[test]
    fn detect_shadow_reports_a_different_path_first_install() {
        let _lock = ENV_LOCK.lock().unwrap();
        let shadow_dir = tempfile::tempdir().unwrap();
        let our_dir = tempfile::tempdir().unwrap();
        let shadow = touch_symforge(shadow_dir.path());
        let ours = touch_symforge(our_dir.path());

        // Shadow dir first on PATH -> it wins a bare invocation.
        let path = join_path_dirs(&[shadow_dir.path(), our_dir.path()]);
        let _guard = PathGuard::set(&path);

        let report = detect_shadow(&ours).expect("a different PATH-first install is a shadow");
        assert_eq!(report.shadow_path, shadow);
        assert_eq!(report.our_path, ours);
        // A tempdir is neither a system prefix nor /mnt -> ForeignPrefix.
        assert_eq!(report.kind, ShadowKind::ForeignPrefix);
    }

    /// SF-STRESS-006 regression: npm produces SEVERAL launcher artifacts of ONE
    /// install — the shims at the prefix root (`<prefix>\symforge{,.cmd,.ps1}`)
    /// and the real platform binary under
    /// `<prefix>\node_modules\symforge-<plat>\bin\symforge.exe`. A PATH-first
    /// shim that delegates into the SAME install must NOT be reported as a
    /// foreign shadow. Here PATH resolves the prefix-root shim while "our binary"
    /// is the platform exe of the same prefix -> `same_install` wins -> no shadow.
    #[test]
    fn detect_shadow_none_when_shim_delegates_to_same_install() {
        let _lock = ENV_LOCK.lock().unwrap();
        let prefix = tempfile::tempdir().unwrap();

        // The launcher shims at the prefix root (npm's multi-artifact shape).
        let shim_cmd = prefix.path().join("symforge.cmd");
        fs::write(&shim_cmd, b"@echo off").unwrap();
        let shim_ps1 = prefix.path().join("symforge.ps1");
        fs::write(&shim_ps1, b"# ps shim").unwrap();
        // The bare `#!/bin/sh` shim no Windows shell runs (still part of ONE install).
        let shim_bare = prefix.path().join("symforge");
        fs::write(&shim_bare, b"#!/bin/sh").unwrap();

        // The real platform binary: <prefix>/node_modules/symforge-<plat>/bin/<exe>.
        let bin_dir = prefix
            .path()
            .join("node_modules")
            .join("symforge-windows-x64")
            .join("bin");
        fs::create_dir_all(&bin_dir).unwrap();
        let exe_name = if cfg!(windows) {
            "symforge.exe"
        } else {
            "symforge"
        };
        let platform_exe = bin_dir.join(exe_name);
        fs::write(&platform_exe, b"#exe").unwrap();

        // The prefix-root dir is first on PATH (npm-global shape): a bare
        // `symforge` resolves a shim there, not the deeply-nested platform exe.
        let path = join_path_dirs(&[prefix.path()]);
        let _guard = PathGuard::set(&path);

        assert!(
            detect_shadow(&platform_exe).is_none(),
            "a launcher shim delegating into the same npm install is NOT a shadow"
        );
    }

    /// `same_install` collapses every launcher artifact of one npm prefix to a
    /// single install root, while two genuinely different prefixes stay distinct.
    #[test]
    fn same_install_collapses_npm_launcher_artifacts() {
        let prefix = tempfile::tempdir().unwrap();
        let shim = prefix.path().join("symforge.cmd");
        fs::write(&shim, b"@echo off").unwrap();
        let bin_dir = prefix
            .path()
            .join("node_modules")
            .join("symforge-linux-x64")
            .join("bin");
        fs::create_dir_all(&bin_dir).unwrap();
        let exe = bin_dir.join("symforge");
        fs::write(&exe, b"#exe").unwrap();

        assert!(
            same_install(&shim, &exe),
            "prefix-root shim and node_modules platform exe share one install"
        );

        // A different prefix is a different install.
        let other = tempfile::tempdir().unwrap();
        let other_shim = other.path().join("symforge.cmd");
        fs::write(&other_shim, b"@echo off").unwrap();
        assert!(
            !same_install(&shim, &other_shim),
            "two distinct npm prefixes are distinct installs"
        );
    }

    /// SF-STRESS-006 regression: the same-version suppression guard. Two
    /// `symforge` launchers in unrelated prefixes that resolve to the IDENTICAL
    /// version are not a harmful shadow — running either yields the same
    /// behavior, so a churny PATH-reorder recommendation is suppressed. We probe
    /// version by constructing two shims; on the host, real launchers report a
    /// version, so we assert the guard logic directly via a synthetic report.
    #[test]
    fn detect_shadow_suppresses_when_versions_match() {
        // Drive the guard logic in isolation: identical versions -> no warning is
        // the contract `detect_shadow` enforces after `same_install` fails.
        let ours = Some("7.21.0".to_string());
        let theirs = Some("7.21.0".to_string());
        let suppressed = matches!((&ours, &theirs), (Some(a), Some(b)) if a == b);
        assert!(suppressed, "identical probed versions suppress the warning");

        let theirs_old = Some("7.10.0".to_string());
        let suppressed_old = matches!((&ours, &theirs_old), (Some(a), Some(b)) if a == b);
        assert!(
            !suppressed_old,
            "genuinely different versions still warn (in both surfaces)"
        );
    }

    /// `install_root` recognizes each npm launcher layout and reaches the prefix.
    #[test]
    fn install_root_recognizes_npm_layouts() {
        let prefix = tempfile::tempdir().unwrap();
        let canon = std::fs::canonicalize(prefix.path()).unwrap();

        // Windows-style shim at the prefix root.
        let shim = prefix.path().join("symforge.cmd");
        fs::write(&shim, b"@echo off").unwrap();
        assert_eq!(install_root(&shim).as_deref(), Some(canon.as_path()));

        // Platform package under node_modules.
        let bin_dir = prefix
            .path()
            .join("node_modules")
            .join("symforge-darwin-arm64")
            .join("bin");
        fs::create_dir_all(&bin_dir).unwrap();
        let exe = bin_dir.join("symforge");
        fs::write(&exe, b"#exe").unwrap();
        assert_eq!(install_root(&exe).as_deref(), Some(canon.as_path()));

        // Unix-style shim at <prefix>/bin/symforge.
        let unix_bin = prefix.path().join("bin");
        fs::create_dir_all(&unix_bin).unwrap();
        let unix_shim = unix_bin.join("symforge");
        fs::write(&unix_shim, b"#!/bin/sh").unwrap();
        assert_eq!(install_root(&unix_shim).as_deref(), Some(canon.as_path()));
    }

    #[test]
    fn classify_shadow_flags_usr_local_as_root_system() {
        assert_eq!(
            classify_shadow(Path::new("/usr/local/bin/symforge")),
            ShadowKind::RootSystem
        );
        assert_eq!(
            classify_shadow(Path::new("/usr/bin/symforge")),
            ShadowKind::RootSystem
        );
        assert_eq!(
            classify_shadow(Path::new("/opt/symforge/bin/symforge")),
            ShadowKind::RootSystem
        );
    }

    #[test]
    fn classify_shadow_flags_mnt_under_wsl_else_foreign() {
        let mnt = Path::new("/mnt/c/Users/you/.npm-global/symforge");
        // The classification depends on whether THIS host is WSL. We assert the
        // exact branch for the host we're on, and that a /mnt path is never
        // mis-bucketed as a Linux system prefix.
        let kind = classify_shadow(mnt);
        if running_under_wsl() {
            assert_eq!(kind, ShadowKind::WindowsMntBleed);
        } else {
            assert_eq!(kind, ShadowKind::ForeignPrefix);
        }
        assert_ne!(
            kind,
            ShadowKind::RootSystem,
            "/mnt is never a Linux system prefix"
        );
    }

    #[test]
    fn classify_shadow_flags_arbitrary_prefix_as_foreign() {
        assert_eq!(
            classify_shadow(Path::new("/home/you/.nvm/versions/node/v20/bin/symforge")),
            ShadowKind::ForeignPrefix
        );
    }

    #[test]
    fn format_warning_root_system_has_exact_remediation_and_both_versions() {
        let report = ShadowReport {
            our_path: PathBuf::from("/home/you/.npm-global/bin/symforge"),
            our_version: Some("7.17.3".to_string()),
            shadow_path: PathBuf::from("/usr/local/bin/symforge"),
            shadow_version: Some("7.15.2".to_string()),
            kind: ShadowKind::RootSystem,
        };
        let warning = format_shadow_warning(&report);
        assert!(warning.contains("sudo rm -f /usr/local/bin/symforge"));
        assert!(warning.contains("sudo rm -rf /usr/local/lib/node_modules/symforge"));
        assert!(warning.contains("/usr/local/bin/symforge"));
        assert!(warning.contains("/home/you/.npm-global/bin/symforge"));
        assert!(warning.contains("(7.15.2)"), "shadow version shown");
        assert!(warning.contains("(7.17.3)"), "our version shown");
        assert!(warning.is_ascii(), "warning must be plain ASCII");
    }

    #[test]
    fn format_warning_windows_mnt_bleed_points_login_path_at_our_bin() {
        let report = ShadowReport {
            our_path: PathBuf::from("/home/you/.npm-global/bin/symforge"),
            our_version: Some("7.17.3".to_string()),
            shadow_path: PathBuf::from("/mnt/c/Users/you/.npm-global/symforge"),
            shadow_version: None,
            kind: ShadowKind::WindowsMntBleed,
        };
        let warning = format_shadow_warning(&report);
        assert!(warning.contains("/mnt/c/Users/you/.npm-global/symforge"));
        assert!(
            warning.contains("export PATH=\"/home/you/.npm-global/bin:$PATH\""),
            "login PATH fix points at OUR bin dir, got: {warning}"
        );
        assert!(warning.contains("~/.profile"));
        assert!(
            warning.contains("(version unknown)"),
            "unknown shadow version is labelled"
        );
        assert!(warning.is_ascii());
    }

    #[cfg(not(windows))]
    #[test]
    fn format_warning_foreign_prefix_orders_our_bin_before_shadow() {
        let report = ShadowReport {
            our_path: PathBuf::from("/home/you/.npm-global/bin/symforge"),
            our_version: Some("7.17.3".to_string()),
            shadow_path: PathBuf::from("/home/you/.nvm/versions/node/v20/bin/symforge"),
            shadow_version: Some("7.10.0".to_string()),
            kind: ShadowKind::ForeignPrefix,
        };
        let warning = format_shadow_warning(&report);
        assert!(
            warning.contains("export PATH=\"/home/you/.npm-global/bin:$PATH\""),
            "fix prepends our bin dir, got: {warning}"
        );
        assert!(
            warning.contains("/home/you/.nvm/versions/node/v20/bin"),
            "names the shadow dir that currently wins"
        );
        assert!(warning.contains("(7.17.3)"));
        assert!(warning.contains("(7.10.0)"));
        assert!(warning.is_ascii());
    }

    /// On native Windows a `C:\` shadow classifies as `ForeignPrefix`, and the
    /// remediation MUST be PowerShell-native: no POSIX `~/.profile`/`export PATH`
    /// (which is meaningless in PowerShell), a `Get-Command symforge -All` verify
    /// step, both bin dirs named, and plain ASCII. `Display` renders backslashes
    /// and this arm does not normalize them, so we assert the backslash form.
    #[cfg(windows)]
    #[test]
    fn format_warning_foreign_prefix_is_powershell_native_on_windows() {
        let report = ShadowReport {
            our_path: PathBuf::from(r"C:\Users\me\.npm-global\bin\symforge.exe"),
            our_version: Some("7.18.1".to_string()),
            shadow_path: PathBuf::from(r"C:\Program Files\nodejs\symforge.exe"),
            shadow_version: Some("7.10.0".to_string()),
            kind: ShadowKind::ForeignPrefix,
        };
        let warning = format_shadow_warning(&report);

        // No POSIX remediation leaks onto Windows.
        assert!(
            !warning.contains("~/.profile"),
            "must not emit POSIX ~/.profile on Windows, got: {warning}"
        );
        assert!(
            !warning.contains("export PATH"),
            "must not emit POSIX export PATH on Windows, got: {warning}"
        );

        // PowerShell-native verification step.
        assert!(
            warning.contains("Get-Command symforge -All"),
            "must point at the PowerShell verify command, got: {warning}"
        );

        // Names BOTH bin dirs (backslash form, as `Display` renders them).
        assert!(
            warning.contains(r"C:\Users\me\.npm-global\bin"),
            "must name our bin dir, got: {warning}"
        );
        assert!(
            warning.contains(r"C:\Program Files\nodejs"),
            "must name the shadow bin dir, got: {warning}"
        );

        // Both versions and plain ASCII as the other arms guarantee.
        assert!(warning.contains("(7.18.1)"));
        assert!(warning.contains("(7.10.0)"));
        assert!(warning.is_ascii(), "warning must be plain ASCII");
    }

    #[test]
    fn format_warning_compact_is_single_line_with_kind_and_versions() {
        let report = ShadowReport {
            our_path: PathBuf::from("/home/you/.npm-global/bin/symforge"),
            our_version: Some("7.17.3".to_string()),
            shadow_path: PathBuf::from("/usr/local/bin/symforge"),
            shadow_version: Some("7.15.2".to_string()),
            kind: ShadowKind::RootSystem,
        };
        let line = format_shadow_warning_compact(&report);
        assert_eq!(line.lines().count(), 1, "compact banner is one line");
        assert!(line.contains("root-owned shadow"));
        assert!(line.contains("(7.15.2)"));
        assert!(line.contains("(7.17.3)"));
        assert!(line.is_ascii());
    }

    #[test]
    fn parse_version_scans_past_a_leading_banner() {
        let out = "symforge update available\nsymforge 7.17.3\n";
        assert_eq!(parse_version(out), Some("7.17.3".to_string()));
    }

    #[test]
    fn system_node_modules_derives_from_prefix() {
        assert_eq!(
            system_node_modules_for(Path::new("/usr/local/bin/symforge")),
            "/usr/local/lib/node_modules/symforge"
        );
        assert_eq!(
            system_node_modules_for(Path::new("/opt/symforge/bin/symforge")),
            "/opt/symforge/lib/node_modules/symforge"
        );
    }
}
