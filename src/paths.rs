use std::io;
use std::path::{Path, PathBuf};

pub const SYMFORGE_DIR_NAME: &str = ".symforge";

/// Bare db filenames (no `.symforge/` prefix). These are the `name` argument to
/// [`symforge_db_path`] — the ONLY sanctioned way to build a `.symforge/<name>.db`
/// on-disk path. Keeping them bare is the whole point: the prefix lives in exactly
/// one place (the helper), so no store can hand-roll a path and double the prefix
/// (D1-ROOT — the defect that shipped a doubled `root/.symforge/.symforge/...` path).
pub const FRECENCY_DB_NAME: &str = "frecency.db";
pub const COUPLING_DB_NAME: &str = "coupling.db";
pub const ANALYTICS_DB_NAME: &str = "analytics.db";
pub const API_KEYS_DB_NAME: &str = "api-keys.db";
pub const STEL_LEDGER_DB_NAME: &str = "stel-ledger.db";

pub const SYMFORGE_IDEMPOTENCY_DIR_PATH: &str = ".symforge/idempotency";
pub const SYMFORGE_IDEMPOTENCY_RECORDS_DIR_PATH: &str = ".symforge/idempotency/records";
pub const SYMFORGE_IDEMPOTENCY_QUARANTINE_DIR_PATH: &str = ".symforge/idempotency/quarantine";
pub const SYMFORGE_INDEX_SNAPSHOT_QUARANTINE_DIR_PATH: &str =
    ".symforge/quarantine/index-snapshots";
/// contracts/team-artifact.md § Integrity failure.
pub const SYMFORGE_ARTIFACT_QUARANTINE_DIR_PATH: &str = ".symforge/quarantine/artifacts";

/// OS isolation tag for per-process runtime files (sidecar/daemon port/pid/session).
///
/// This is a pure compile-time constant baked into the binary from its build
/// target (`std::env::consts::OS`): `"windows"`, `"linux"`, `"macos"`, etc. It is
/// NOT a runtime probe, so any two binaries built for the same OS — notably the
/// sidecar/daemon writer and the `symforge hook` reader, which are the SAME crate
/// — always compute the IDENTICAL tag and therefore always agree on filenames.
///
/// Rationale: a Windows symforge and a WSL/Linux symforge can share one physical
/// project-local `.symforge/` directory (a project on a Windows drive opened from
/// both `C:\proj` and `/mnt/c/proj`). Each writes a port that is only valid in its
/// own loopback namespace. Tagging the runtime filenames by OS guarantees neither
/// side ever reads the other's port file. WSL2 reports `"linux"`, which is correct:
/// two Linux processes sharing a dir share the same namespace semantics, so no
/// WSL-vs-native discriminator is needed (adding a `/proc` sniff would make the tag
/// a runtime probe that the Windows side could not reproduce — defeating agreement).
#[must_use]
pub fn os_runtime_tag() -> &'static str {
    std::env::consts::OS
}

/// Build an OS-tagged runtime filename: `sidecar_runtime_file_name("sidecar", "port")`
/// yields e.g. `"sidecar.linux.port"`. The extension is preserved so docs/tools that
/// key on `.port`/`.pid`/`.session` continue to match, and the file stays a sibling
/// in the same `.symforge/` directory.
#[must_use]
pub fn os_tagged_runtime_file_name(stem: &str, ext: &str) -> String {
    format!("{stem}.{tag}.{ext}", tag = os_runtime_tag())
}

/// Resolve the canonical symforge data directory under `base`.
pub fn resolve_symforge_dir(base: &Path) -> PathBuf {
    base.join(SYMFORGE_DIR_NAME)
}

/// User-level SymForge home for runtime files when no safe project root exists.
///
/// Honors `SYMFORGE_HOME` when set (the directory itself, no extra nesting).
/// Otherwise uses `~/.symforge`, matching the daemon's global state layout.
pub fn global_symforge_home() -> io::Result<PathBuf> {
    if let Some(explicit_home) = std::env::var_os("SYMFORGE_HOME") {
        let dir = PathBuf::from(explicit_home);
        std::fs::create_dir_all(&dir).map_err(|e| {
            io::Error::new(
                e.kind(),
                format!("ensuring symforge global home at {}: {}", dir.display(), e),
            )
        })?;
        return Ok(dir);
    }

    let home = dirs::home_dir()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "home directory not found"))?;
    ensure_symforge_dir(&home)
}

/// True when `path` must not host a project-local `.symforge` (sensitive /
/// credential-bearing / too-broad roots). Mirrors the launcher's
/// `discovery::find_project_root` refusal without importing discovery.
fn is_unsafe_data_dir_base(path: &Path) -> bool {
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    is_sensitive_path(&canonical)
}

/// Choose the base directory for runtime `.symforge` state (sidecar port/pid
/// files, init bootstrap, etc.) without creating it.
///
/// Preference order:
/// 1. `project_root` when supplied and safe
/// 2. Launch cwd when safe (not a sensitive/forbidden root)
/// 3. [`global_symforge_home`]
pub fn select_runtime_data_base(project_root: Option<&Path>, cwd: Option<&Path>) -> PathBuf {
    if let Some(root) = project_root.filter(|p| !is_unsafe_data_dir_base(p)) {
        return resolve_symforge_dir(root);
    }
    if let Some(cwd) = cwd.filter(|p| !is_unsafe_data_dir_base(p)) {
        return resolve_symforge_dir(cwd);
    }
    global_symforge_home().unwrap_or_else(|error| {
        tracing::warn!(
            "falling back to cwd-relative .symforge after global home resolution failed: {error}"
        );
        cwd.as_ref()
            .map(|path| resolve_symforge_dir(path))
            .unwrap_or_else(|| PathBuf::from(SYMFORGE_DIR_NAME))
    })
}

/// Ensure runtime `.symforge` state exists, falling back to [`global_symforge_home`]
/// when the preferred project/cwd base is unsafe or not writable.
pub fn ensure_runtime_symforge_dir(project_root: Option<&Path>) -> io::Result<PathBuf> {
    let cwd = std::env::current_dir().ok();
    let preferred_base = project_root
        .filter(|p| !is_unsafe_data_dir_base(p))
        .map(|p| p.to_path_buf())
        .or_else(|| {
            cwd.as_ref()
                .filter(|p| !is_unsafe_data_dir_base(p))
                .cloned()
        });

    if let Some(base) = preferred_base {
        match ensure_symforge_dir(&base) {
            Ok(dir) => return Ok(dir),
            Err(error) if error.kind() == io::ErrorKind::PermissionDenied => {
                tracing::warn!(
                    path = %base.display(),
                    "cannot create project-local .symforge (permission denied); using global SymForge home"
                );
            }
            Err(error) => return Err(error),
        }
    } else if let Some(ref launch_cwd) = cwd {
        tracing::info!(
            path = %launch_cwd.display(),
            "launch cwd is not a safe project root for .symforge; using global SymForge home"
        );
    }

    global_symforge_home()
}

/// Ensure the canonical symforge data directory exists under `base`.
pub fn ensure_symforge_dir(base: &Path) -> io::Result<PathBuf> {
    let dir = resolve_symforge_dir(base);
    std::fs::create_dir_all(&dir).map_err(|e| {
        io::Error::new(
            e.kind(),
            format!("ensuring symforge data dir at {}: {}", dir.display(), e),
        )
    })?;
    Ok(dir)
}

/// Build the canonical on-disk path for a symforge db: `root/.symforge/<name>`.
///
/// `root` is the project ROOT, NOT the `.symforge` data dir. `name` MUST be a
/// BARE filename (e.g. `"api-keys.db"`) with NO `.symforge/` prefix — this helper
/// owns the single `.symforge` segment. Passing a `.symforge/`-prefixed name, or
/// passing the already-`.symforge` data dir as `root`, would double the prefix to
/// `root/.symforge/.symforge/<name>` — exactly the D1-ROOT defect this helper
/// exists to make impossible. Every db store builds its path through here so the
/// prefix lives in exactly one place.
///
/// This does NOT create the parent directory (SQLite will not create it either);
/// callers that open for write must first run [`ensure_symforge_dir`] (or use
/// [`ensure_symforge_db_path`], which does both).
#[must_use]
pub fn symforge_db_path(root: &Path, name: &str) -> PathBuf {
    debug_assert!(
        !name.contains('/') && !name.contains('\\'),
        "symforge_db_path `name` must be a BARE filename (got {name:?}); \
         the `.symforge/` prefix is owned by this helper"
    );
    root.join(SYMFORGE_DIR_NAME).join(name)
}

/// Ensure `root/.symforge` exists, then return the db path `root/.symforge/<name>`.
///
/// The write-side companion of [`symforge_db_path`]: SQLite will NOT create the
/// parent `.symforge` directory, so a store that opens a db for write calls this
/// to guarantee the parent exists before `Connection::open`. `name` MUST be a
/// BARE filename (same rule as [`symforge_db_path`]).
pub fn ensure_symforge_db_path(root: &Path, name: &str) -> io::Result<PathBuf> {
    ensure_symforge_dir(root)?;
    Ok(symforge_db_path(root, name))
}

/// Resolve the canonical idempotency replay directory under `base`.
pub fn resolve_idempotency_dir(base: &Path) -> PathBuf {
    base.join(SYMFORGE_IDEMPOTENCY_DIR_PATH)
}

/// Ensure the canonical idempotency replay directory exists under `base`.
pub fn ensure_idempotency_dir(base: &Path) -> io::Result<PathBuf> {
    let dir = resolve_idempotency_dir(base);
    std::fs::create_dir_all(&dir).map_err(|e| {
        io::Error::new(
            e.kind(),
            format!("ensuring idempotency dir at {}: {}", dir.display(), e),
        )
    })?;
    Ok(dir)
}

/// Resolve the canonical index-snapshot quarantine directory under `base`.
pub fn resolve_index_snapshot_quarantine_dir(base: &Path) -> PathBuf {
    base.join(SYMFORGE_INDEX_SNAPSHOT_QUARANTINE_DIR_PATH)
}

/// Ensure the canonical index-snapshot quarantine directory exists under `base`.
pub fn ensure_index_snapshot_quarantine_dir(base: &Path) -> io::Result<PathBuf> {
    let dir = resolve_index_snapshot_quarantine_dir(base);
    std::fs::create_dir_all(&dir).map_err(|e| {
        io::Error::new(
            e.kind(),
            format!(
                "ensuring index snapshot quarantine dir at {}: {}",
                dir.display(),
                e
            ),
        )
    })?;
    Ok(dir)
}

/// Resolve the canonical team-artifact quarantine directory under `base`
/// (contracts/team-artifact.md § Integrity failure).
pub fn resolve_artifact_quarantine_dir(base: &Path) -> PathBuf {
    base.join(SYMFORGE_ARTIFACT_QUARANTINE_DIR_PATH)
}

/// Ensure the canonical team-artifact quarantine directory exists under `base`.
pub fn ensure_artifact_quarantine_dir(base: &Path) -> io::Result<PathBuf> {
    let dir = resolve_artifact_quarantine_dir(base);
    std::fs::create_dir_all(&dir).map_err(|e| {
        io::Error::new(
            e.kind(),
            format!(
                "ensuring team artifact quarantine dir at {}: {}",
                dir.display(),
                e
            ),
        )
    })?;
    Ok(dir)
}

/// Strip the Windows extended-length / verbatim path prefix (`\\?\` and
/// `\\?\UNC\`) from a path string, returning a plain form suitable for
/// boundary matching. On non-prefixed input the string is returned
/// unchanged. This is intentionally string-level (not `Path::components`),
/// because `std::path` parses verbatim prefixes into opaque `Prefix`
/// components on Windows that do not normalize separators or case the way
/// sensitive-root matching requires.
fn strip_verbatim_prefix(s: &str) -> &str {
    if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        // `\\?\UNC\server\share\...` denotes `\\server\share\...`. The
        // leading `\\` is irrelevant for sensitive-root matching (it is a
        // network share, never a local system root), so the share-relative
        // remainder is sufficient.
        rest
    } else if let Some(rest) = s.strip_prefix(r"\\?\") {
        rest
    } else {
        s
    }
}

/// Sensitive top-level Windows system directories (lowercased). Matching the
/// first non-drive component of a path on a boundary (not a substring) blocks
/// the root *and* every subdirectory (`...\Windows\System32\...`). `system32`
/// is listed defensively even though it normally lives under `windows`.
/// `programdata` is credential/state-bearing (machine-wide secrets, package
/// caches) and is blocked together with its descendants.
const WINDOWS_SENSITIVE_SEGMENTS: &[&str] = &[
    "windows",
    "program files",
    "program files (x86)",
    "programdata",
    "system32",
];

/// Returns true if the first non-drive component of an already-lowercased,
/// `/`-separated component slice names a sensitive Windows system directory.
/// `comps[0]` is expected to be the drive (e.g. `c:`); the system directory is
/// `comps[1]`. Used by both the native Windows arm and the WSL mount check.
fn is_windows_sensitive_under_drive(comps: &[&str]) -> bool {
    comps
        .get(1)
        .is_some_and(|first| WINDOWS_SENSITIVE_SEGMENTS.contains(first))
}

/// Returns true if an already-lowercased, `/`-separated component slice points
/// at the Windows user-profile container or a *bare* profile root — both of
/// which are credential-bearing (each `C:\Users\<name>` holds `AppData` OAuth
/// tokens, `.ssh`, `.aws`, `.npmrc`) and must never be indexed wholesale.
///
/// `comps[0]` is the drive (e.g. `c:`). The rule, mirrored on Unix `/home` and
/// `/Users`, blocks exactly two shapes and *allows* anything deeper so genuine
/// projects stay indexable:
///
/// - `[drive, "users"]`           → the profile container (`C:\Users`)         BLOCK
/// - `[drive, "users", <name>]`   → a bare profile root (`C:\Users\alice`)     BLOCK
/// - `[drive, "users", <name>, …]`→ a project under a profile                  ALLOW
///
/// The deeper-than-profile allowance is load-bearing: every normal user keeps
/// repos at `C:\Users\<name>\...`, and the launcher already relies on indexing
/// them, so blocking the bare root must not block its descendants.
fn is_windows_user_container(comps: &[&str]) -> bool {
    matches!(comps.get(1), Some(&"users")) && comps.len() <= 3
}

/// Returns true if `canonical` is a sensitive system or credential-bearing
/// directory (or a descendant of one) that must never be indexed.
///
/// This is the single, canonical trust-boundary guard for *both* attacker-
/// facing index entrypoints (`tools::index_folder`, `daemon::
/// index_folder_for_session`, `daemon::open_project_session`) *and* the trusted
/// launcher (`discovery::is_forbidden_root` delegates here). Keeping one guard
/// means the tool surface can never drift weaker than the launcher again — the
/// exact drift that caused the original daemon bypass.
///
/// Two block classes:
///
/// 1. **System roots — root and every descendant blocked.** `/`, `/etc`,
///    `/proc`, `/usr`, `C:\Windows`, `System32`, `Program Files`,
///    `C:\ProgramData`, the bare drive root, etc. Indexing these reads system
///    files and, on some hosts, drives a reload into a denial-of-service.
///
/// 2. **User-profile containers — container and *bare* profile root blocked,
///    descendants allowed.** `C:\Users`, `C:\Users\<name>`, `/home`,
///    `/home/<name>`, `/Users`, `/Users/<name>`. Each profile root holds
///    credentials (`AppData` OAuth tokens, `.ssh`, `.aws`, `.npmrc`), so
///    indexing it wholesale is credential exfiltration. But real projects live
///    *under* a profile (`C:\Users\<name>\projects\repo`), so anything deeper
///    than the bare profile root stays indexable. Blocking those would be a DoS
///    for every user.
///
/// Matching is component-boundary aware: a project path that merely *contains*
/// a sensitive segment as a substring (e.g. `C:\Users\me\my-windows-project`,
/// `C:\Users\me\system32-emulator`, or `/home/me/etcd-client`) is allowed. On
/// Windows the `\\?\` extended-length prefix is stripped before matching so
/// canonicalized paths (which carry it) are not silently waved through. On Unix
/// the WSL Windows mount (`/mnt/<drive>`, `/mnt/<drive>/Users`,
/// `/mnt/<drive>/Windows`, …) is blocked with the same container semantics.
#[must_use]
pub fn is_sensitive_path(canonical: &Path) -> bool {
    let raw = canonical.to_string_lossy();
    // On Unix the canonical form has no verbatim prefix, but strip defensively
    // in case a Windows-style path is ever routed through this code path.
    let stripped = strip_verbatim_prefix(&raw);

    #[cfg(windows)]
    {
        // Normalize separators so component splitting is uniform regardless
        // of whether the source used `\` (canonicalize) or `/` (user input).
        let lower = stripped.replace('\\', "/").to_ascii_lowercase();

        // Split into non-empty path components. The first component of an
        // absolute Windows path is the drive (e.g. `c:`).
        let comps: Vec<&str> = lower.split('/').filter(|c| !c.is_empty()).collect();

        // Bare drive root: `c:` / `c:\` / `c:/` → exactly one component that
        // looks like a drive letter.
        if let [drive] = comps.as_slice()
            && drive.len() == 2
            && drive.ends_with(':')
            && drive.as_bytes()[0].is_ascii_alphabetic()
        {
            return true;
        }

        is_windows_sensitive_under_drive(&comps) || is_windows_user_container(&comps)
    }

    #[cfg(unix)]
    {
        let path = Path::new(stripped);

        // Class 1 — system roots: blocked together with every descendant,
        // matched on path components (not raw substring), so `/etc/x` and
        // `/usr/lib/y` are caught while `/home/etc-notes` is not. `/root` is a
        // privileged home holding `.ssh`/`.aws` and is treated as a system root
        // (no legitimate project container nests directly under it).
        const BLOCKED_RECURSIVE: &[&str] = &[
            "/bin", "/boot", "/dev", "/etc", "/lib", "/lib64", "/proc", "/run", "/sbin", "/sys",
            "/usr", "/var", "/root", "/Library", "/System", "/private",
        ];

        // Class 2 — block the bare root only; a real project legitimately nests
        // one level under these (`/opt/app`, `/srv/site`, `/tmp/build`), so only
        // the container itself is refused. `/tmp` in particular MUST stay
        // root-only: TempDir-based tests create project dirs under `/tmp`.
        const BLOCKED_ROOT_ONLY: &[&str] = &["/opt", "/srv", "/media", "/tmp", "/snap"];

        // Root `/` itself: a path with no normal components.
        let has_normal_component = path
            .components()
            .any(|c| matches!(c, std::path::Component::Normal(_)));
        if path.is_absolute() && !has_normal_component {
            return true;
        }

        for blocked in BLOCKED_RECURSIVE {
            if path == Path::new(blocked) || path.starts_with(blocked) {
                return true;
            }
        }

        for blocked in BLOCKED_ROOT_ONLY {
            if path == Path::new(blocked) {
                return true;
            }
        }

        let lower = stripped.to_ascii_lowercase();
        let comps: Vec<&str> = lower.split('/').filter(|c| !c.is_empty()).collect();

        // Class 2 — user-profile containers (`/home`, `/Users`): block the
        // container and a *bare* profile root, allow anything deeper so genuine
        // projects (`/home/<name>/repo`) stay indexable. Matched on lowercased
        // components for consistency with the Windows `Users` rule.
        if let [container, rest @ ..] = comps.as_slice()
            && (*container == "home" || *container == "users")
            && rest.len() <= 1
        {
            return true;
        }

        // WSL Windows mount under `/mnt/<drive>`. The bare drive mount and the
        // Windows profile container/root surface a huge, credential-bearing tree
        // over a slow DrvFs mount; the Windows system dirs map onto the host's.
        // Block:
        //   /mnt                              (bare automount root)
        //   /mnt/<drive>                      (bare Windows drive root)
        //   /mnt/<drive>/Users               (profile container)
        //   /mnt/<drive>/Users/<name>        (bare profile root)
        //   /mnt/<drive>/Windows|System32|…  (host system dirs)
        // Allow `/mnt/<drive>/Users/<name>/…` and `/mnt/<drive>/<non-Users>/…`.
        if comps.first() == Some(&"mnt") {
            match comps.as_slice() {
                // Bare `/mnt`.
                [_mnt] => return true,
                [_mnt, drive, rest @ ..]
                    if drive.len() == 1 && drive.as_bytes()[0].is_ascii_alphabetic() =>
                {
                    // Bare drive root `/mnt/<drive>`.
                    if rest.is_empty() {
                        return true;
                    }
                    // Windows profile container/root under the drive mount,
                    // reusing the shared `[drive, users, …]` shape semantics.
                    let mut drive_view = Vec::with_capacity(rest.len() + 1);
                    drive_view.push(*drive);
                    drive_view.extend_from_slice(rest);
                    if is_windows_user_container(&drive_view) {
                        return true;
                    }
                    // Windows system dirs mapped onto host (`/mnt/c/Windows`, …).
                    if is_windows_sensitive_under_drive(&drive_view) {
                        return true;
                    }
                }
                _ => {}
            }
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use tempfile::TempDir;

    #[test]
    fn select_runtime_data_base_prefers_project_root_over_launch_cwd() {
        let tmp = TempDir::new().unwrap();
        let project = tmp.path().join("repo");
        std::fs::create_dir_all(&project).unwrap();
        let chosen = select_runtime_data_base(Some(&project), Some(Path::new("/tmp")));
        assert_eq!(chosen, resolve_symforge_dir(&project));
    }

    #[cfg(windows)]
    #[test]
    fn select_runtime_data_base_skips_system32_launch_cwd() {
        let chosen = select_runtime_data_base(None, Some(Path::new(r"C:\Windows\System32")));
        assert!(
            !chosen
                .to_string_lossy()
                .to_ascii_lowercase()
                .contains("system32"),
            "System32 launch cwd must not host .symforge; got {}",
            chosen.display()
        );
    }

    #[test]
    fn test_resolve_symforge_dir_prefers_existing_canonical_dir() {
        let tmp = TempDir::new().unwrap();
        let symforge_dir = tmp.path().join(SYMFORGE_DIR_NAME);
        std::fs::create_dir_all(&symforge_dir).unwrap();

        let resolved = resolve_symforge_dir(tmp.path());

        assert_eq!(resolved, symforge_dir);
    }

    #[test]
    fn test_ensure_symforge_dir_creates_canonical_dir_when_missing() {
        let tmp = TempDir::new().unwrap();

        let dir = ensure_symforge_dir(tmp.path()).unwrap();

        assert_eq!(dir, tmp.path().join(SYMFORGE_DIR_NAME));
        assert!(dir.exists(), "canonical directory should be created");
    }

    /// D1-ROOT regression: every db store now builds its on-disk path through
    /// [`symforge_db_path`] with a BARE filename. Under the PRODUCTION calling
    /// convention (pass the project ROOT, not the `.symforge` data dir) the path
    /// is the SINGLE-prefixed `root/.symforge/<name>.db`, and the DOUBLED
    /// `root/.symforge/.symforge/<name>.db` is never produced. This is the check
    /// the old per-store tests skipped — they passed a different arg than the
    /// production caller, which is exactly how the doubled-path bug hid (D1/D7).
    #[test]
    fn symforge_db_path_is_single_prefixed_never_doubled() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        for name in [
            FRECENCY_DB_NAME,
            COUPLING_DB_NAME,
            ANALYTICS_DB_NAME,
            API_KEYS_DB_NAME,
            STEL_LEDGER_DB_NAME,
        ] {
            let got = symforge_db_path(root, name);
            assert_eq!(
                got,
                root.join(SYMFORGE_DIR_NAME).join(name),
                "{name}: production path must be single-prefixed root/.symforge/<name>"
            );
            let doubled = root
                .join(SYMFORGE_DIR_NAME)
                .join(SYMFORGE_DIR_NAME)
                .join(name);
            assert_ne!(
                got, doubled,
                "{name}: helper must never produce the doubled root/.symforge/.symforge/<name> path"
            );
        }
    }

    /// `ensure_symforge_db_path` creates the parent `.symforge` dir (SQLite will
    /// not) and returns the same single-prefixed path as `symforge_db_path`.
    #[test]
    fn ensure_symforge_db_path_creates_parent_and_returns_single_prefixed() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let db = ensure_symforge_db_path(root, ANALYTICS_DB_NAME).unwrap();
        assert_eq!(db, root.join(SYMFORGE_DIR_NAME).join(ANALYTICS_DB_NAME));
        assert!(
            db.parent().unwrap().is_dir(),
            "parent .symforge dir must exist after ensure_symforge_db_path"
        );
    }

    #[test]
    fn test_idempotency_paths_stay_under_canonical_symforge_dir() {
        let tmp = TempDir::new().unwrap();

        assert_eq!(
            resolve_idempotency_dir(tmp.path()),
            tmp.path().join(SYMFORGE_DIR_NAME).join("idempotency")
        );
        assert_eq!(
            tmp.path().join(SYMFORGE_IDEMPOTENCY_RECORDS_DIR_PATH),
            resolve_idempotency_dir(tmp.path()).join("records")
        );
        assert_eq!(
            tmp.path().join(SYMFORGE_IDEMPOTENCY_QUARANTINE_DIR_PATH),
            resolve_idempotency_dir(tmp.path()).join("quarantine")
        );
    }

    #[test]
    fn test_index_snapshot_quarantine_path_stays_under_canonical_symforge_dir() {
        let tmp = TempDir::new().unwrap();

        assert_eq!(
            resolve_index_snapshot_quarantine_dir(tmp.path()),
            tmp.path()
                .join(SYMFORGE_DIR_NAME)
                .join("quarantine")
                .join("index-snapshots")
        );
    }

    #[test]
    fn test_artifact_quarantine_path_stays_under_canonical_symforge_dir() {
        let tmp = TempDir::new().unwrap();

        assert_eq!(
            resolve_artifact_quarantine_dir(tmp.path()),
            tmp.path()
                .join(SYMFORGE_DIR_NAME)
                .join("quarantine")
                .join("artifacts")
        );
        let dir = ensure_artifact_quarantine_dir(tmp.path()).unwrap();
        assert!(dir.is_dir(), "artifact quarantine dir should be created");
    }

    #[test]
    fn strip_verbatim_prefix_handles_plain_unc_and_verbatim() {
        assert_eq!(strip_verbatim_prefix(r"\\?\C:\Windows"), r"C:\Windows");
        assert_eq!(
            strip_verbatim_prefix(r"\\?\UNC\server\share\proj"),
            r"server\share\proj"
        );
        assert_eq!(
            strip_verbatim_prefix(r"C:\Users\me\proj"),
            r"C:\Users\me\proj"
        );
        assert_eq!(
            strip_verbatim_prefix("/home/user/project"),
            "/home/user/project"
        );
    }

    #[cfg(windows)]
    #[test]
    fn is_sensitive_path_windows_blocks_system_roots_and_subdirs() {
        let blocked = [
            r"\\?\C:\Windows\System32",
            r"\\?\C:\Windows",
            r"C:\Windows",
            "C:/Windows",
            r"C:\Windows\System32\drivers",
            r"C:\Program Files\X",
            r"C:\Program Files (x86)\Y\bin",
            // ProgramData is machine-wide credential/state storage.
            r"C:\ProgramData",
            r"C:\ProgramData\some\creds",
            r"C:\",
            "C:/",
            r"D:\",
        ];
        for path in blocked {
            assert!(
                is_sensitive_path(Path::new(path)),
                "expected `{path}` to be refused as sensitive"
            );
        }
    }

    #[cfg(windows)]
    #[test]
    fn is_sensitive_path_windows_blocks_user_profile_container_and_root() {
        // The profile container and a bare profile root are credential-bearing
        // (each holds AppData OAuth tokens, .ssh, .aws, .npmrc) and must be
        // refused — but a project *under* a profile must stay indexable (asserted
        // separately in the allow test). Verbatim-prefixed forms must also block.
        let blocked = [
            r"C:\Users",
            "C:/Users",
            r"\\?\C:\Users",
            r"C:\Users\victim",
            "C:/Users/victim",
            r"\\?\C:\Users\victim",
        ];
        for path in blocked {
            assert!(
                is_sensitive_path(Path::new(path)),
                "expected `{path}` to be refused as sensitive (profile container/root)"
            );
        }
    }

    #[cfg(windows)]
    #[test]
    fn is_sensitive_path_windows_allows_normal_project_paths() {
        let allowed = [
            // A project under a profile — the no-false-positive case. Blocking
            // these would be a DoS for every user who keeps repos under Users.
            r"\\?\C:\Users\me\project",
            r"C:\Users\me\project",
            r"C:\Users\me\projects\repo",
            // Substring of a blocked segment must NOT false-positive.
            r"C:\Users\me\my-windows-project",
            // Leaf that merely *contains* a blocked segment name, one level under
            // the profile, must be allowed (component-boundary, not substring).
            r"C:\Users\me\system32-emulator",
            r"C:\dev\windows-tools\src",
            r"\\?\UNC\server\share\proj",
        ];
        for path in allowed {
            assert!(
                !is_sensitive_path(Path::new(path)),
                "expected `{path}` to be allowed (not sensitive)"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn is_sensitive_path_unix_blocks_roots_and_subdirs() {
        let blocked = [
            "/",
            "/etc",
            "/etc/passwd",
            // Task matrix: an arbitrary subdir of a system root stays blocked.
            "/etc/sub",
            "/usr",
            "/usr/lib/x",
            // Privileged / credential-bearing homes and macOS system roots.
            "/root",
            "/Library",
            "/System",
            "/private",
            // User-profile containers and bare profile roots (credential-bearing).
            "/home",
            "/home/victim",
            "/Users",
            "/Users/victim",
            // Mount/temp/optional containers — bare root only.
            "/mnt",
            "/opt",
            "/srv",
            "/media",
            "/tmp",
            "/snap",
            // WSL drive mount roots and Windows system/profile dirs mapped onto host.
            "/mnt/c",
            "/mnt/c/Users",
            "/mnt/c/Users/victim",
            "/mnt/c/Windows",
            "/mnt/c/Windows/System32",
            "/mnt/c/Program Files/X",
            "/mnt/c/ProgramData",
        ];
        for path in blocked {
            assert!(
                is_sensitive_path(Path::new(path)),
                "expected `{path}` to be refused as sensitive"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn is_sensitive_path_unix_allows_normal_project_paths() {
        let allowed = [
            // The no-false-positive cases: real projects under a profile / home.
            "/home/user/project",
            "/home/me/repo",
            // Substring of a blocked root must NOT false-positive.
            "/home/user/etc-notes",
            "/home/user/usr-local-clone",
            // Leaf merely *contains* a blocked name, one level under the home.
            "/home/me/etcd-client",
            // Project nested one level under a root-only container.
            "/opt/app",
            "/srv/site",
            "/tmp/build/proj",
            // A legit WSL project under a drive mount must be allowed.
            "/mnt/c/Users/me/project",
            "/mnt/c/Users/me/repo",
            "/mnt/d/dev/windows-tools",
            // Non-Users subtree of a drive mount stays indexable.
            "/mnt/c/code/proj",
        ];
        for path in allowed {
            assert!(
                !is_sensitive_path(Path::new(path)),
                "expected `{path}` to be allowed (not sensitive)"
            );
        }
    }
}
