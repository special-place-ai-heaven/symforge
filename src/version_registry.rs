//! EDR-safe version-drift detection for the SymForge binary.
//!
//! SymForge may be launched from more than one location that can drift apart in
//! version: the per-OS npm global install (refreshed by `npm install -g`), a
//! local dev `target/` build, or any other on-disk copy an MCP client points
//! at. If a client is wired to an older path while a newer install exists
//! elsewhere, that client silently serves stale code.
//!
//! NOTE: there is no longer any *durable* SymForge binary under `~/.symforge/bin`.
//! That promotion mechanism was retired — `symforge init` registers MCP clients
//! against the running native binary's own path AS-IS (the npm global install
//! for that OS; see `cli::init::binary_path_for_registration`), and nothing in
//! current code copies, promotes, or writes a binary into `~/.symforge/bin`.
//! Drift detection therefore compares only the paths that binaries have actually
//! recorded in `versions.json`; a leftover `~/.symforge/bin` entry from an old
//! install is harmless because [`detect_stale`] skips any registered path that
//! no longer exists on disk.
//!
//! This module detects drift **without ever copying, executing, or downloading
//! anything** — the one and only side effect is reading and (rarely) atomically
//! rewriting a small plain-text JSON file. That keeps it clear of antivirus /
//! EDR heuristics that flag a running process which drops or overwrites
//! executables. Any refresh is left to a user-run command surfaced in the
//! warning; the daemon never replaces its own binary.
//!
//! Mechanism: every binary, on launch, records its own canonical path and
//! version into `<home>/versions.json`. The daemon reads that registry and, if
//! a strictly newer version is registered at a *different, still-existing*
//! path, warns that it is serving stale code.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

/// File name of the version registry within the SymForge home directory.
const REGISTRY_FILE: &str = "versions.json";

/// Compile-time version of the running binary.
pub fn self_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Resolve the global SymForge home used for the version registry:
/// `$SYMFORGE_HOME` when set, else `~/.symforge`. Mirrors the daemon's home
/// resolution for the common case and does not create the directory.
pub fn resolve_home() -> Option<PathBuf> {
    if let Some(explicit) = std::env::var_os("SYMFORGE_HOME") {
        return Some(PathBuf::from(explicit));
    }
    dirs::home_dir().map(|home| home.join(".symforge"))
}

fn registry_path(home: &Path) -> PathBuf {
    home.join(REGISTRY_FILE)
}

/// Canonical path of the currently running executable, best-effort.
fn current_exe_key() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    let exe = std::fs::canonicalize(&exe).unwrap_or(exe);
    Some(exe.to_string_lossy().into_owned())
}

/// Load the registry as a `path -> version` map. Missing or malformed files
/// yield an empty map — the registry is advisory, never load-bearing.
fn load(home: &Path) -> BTreeMap<String, String> {
    std::fs::read(registry_path(home))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<BTreeMap<String, String>>(&bytes).ok())
        .unwrap_or_default()
}

fn atomic_write(home: &Path, map: &BTreeMap<String, String>) -> io::Result<()> {
    std::fs::create_dir_all(home)?;
    let data = serde_json::to_vec_pretty(map).map_err(io::Error::other)?;
    let target = registry_path(home);
    // Write to a sibling temp file then rename so a concurrent reader never
    // observes a half-written registry. `rename` replaces the target on both
    // Windows and Unix.
    let tmp = target.with_extension("json.tmp");
    std::fs::write(&tmp, &data)?;
    std::fs::rename(&tmp, &target)
}

/// Record this binary's `path -> version` in the registry. Best-effort and
/// silent: any failure is ignored so version bookkeeping can never break a
/// real command. Only writes when the recorded value is missing or changed,
/// so the hot path (e.g. repeated hook invocations) reads but does not write.
pub fn record_self(home: &Path) {
    let _ = record_self_inner(home);
}

/// [`record_self`] against the default [`resolve_home`] location — the entry
/// point every binary calls once on launch. Best-effort and silent.
pub fn record_self_default() {
    if let Some(home) = resolve_home() {
        record_self(&home);
    }
}

fn record_self_inner(home: &Path) -> io::Result<()> {
    let Some(key) = current_exe_key() else {
        return Ok(());
    };
    let mut map = load(home);
    if map.get(&key).map(String::as_str) == Some(self_version()) {
        return Ok(()); // unchanged — avoid a needless write
    }
    map.insert(key, self_version().to_string());
    atomic_write(home, &map)
}

/// A newer SymForge install discovered at a different path than the running
/// binary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaleBinary {
    pub running_version: String,
    pub running_path: String,
    pub newer_version: String,
    pub newer_path: String,
}

/// Returns `Some` when a strictly newer version is registered at a different,
/// still-existing path than the running binary — i.e. the running binary is
/// stale. Read-only; performs no writes, no process spawns, no network.
pub fn detect_stale(home: &Path) -> Option<StaleBinary> {
    let running_path = current_exe_key()?;
    let running_version = self_version();
    let map = load(home);

    let mut best: Option<(&String, &String)> = None;
    for (path, version) in &map {
        if path == &running_path {
            continue;
        }
        if !crate::cli::version::is_newer_version(version, running_version) {
            continue;
        }
        // Ignore registry entries whose binary has since been removed, so an
        // uninstalled-but-still-listed version never raises a false warning.
        if !Path::new(path).exists() {
            continue;
        }
        match best {
            Some((_, best_version))
                if !crate::cli::version::is_newer_version(version, best_version) => {}
            _ => best = Some((path, version)),
        }
    }

    best.map(|(path, version)| StaleBinary {
        running_version: running_version.to_string(),
        running_path,
        newer_version: version.clone(),
        newer_path: path.clone(),
    })
}

/// Remove registry entries whose recorded binary path no longer exists on disk,
/// while preserving entries on a transiently-offline mount (one whose whole
/// ancestor chain is unreachable — e.g. an unplugged removable or network drive).
/// Best-effort and advisory: returns the number of entries removed (`0` on any
/// I/O error or when nothing changed). `symforge update` calls this so the
/// registry does not accumulate dead entries (e.g. removed git-worktree dev
/// builds, or the retired `~/.symforge/bin` durable binary once it is removed).
pub fn prune_missing_entries(home: &Path) -> usize {
    let map = load(home);
    let before = map.len();
    let pruned: BTreeMap<String, String> = map
        .into_iter()
        // Keep an entry when its binary still exists, OR when its WHOLE mount is
        // unreachable (an unplugged removable / offline network drive). Drop a
        // vanished file only when the drive is online — e.g. a removed git
        // worktree's `target/debug/symforge.exe` whose entire subtree is gone.
        // Checking only the immediate parent (the old behavior) wrongly kept such
        // dead entries forever whenever a directory subtree was deleted.
        .filter(|(path, _)| {
            let candidate = Path::new(path);
            candidate.exists() || !mount_is_reachable(candidate)
        })
        .collect();
    let removed = before - pruned.len();
    if removed > 0 {
        let _ = atomic_write(home, &pruned);
    }
    removed
}

/// Whether the mount/drive holding `candidate` is currently reachable: true if
/// any *real* ancestor exists on disk. "Real" deliberately excludes the Windows
/// verbatim/UNC pseudo-roots `\\?` and `\\`, which `Path::exists()` reports as
/// existing for EVERY path (so a canonicalized `\\?\C:\...` entry would otherwise
/// always look reachable, defeating the offline-mount check). The genuine drive
/// root — `C:\` for a plain path, `\\?\C:` for a verbatim one — is retained, so
/// an unplugged `Z:` drive yields no reachable ancestor and its entry is kept.
fn mount_is_reachable(candidate: &Path) -> bool {
    candidate.ancestors().any(|ancestor| {
        if is_universal_pseudo_root(ancestor) {
            return false;
        }
        ancestor.exists()
    })
}

/// True for the Windows verbatim/UNC prefix pseudo-roots (`\\` and `\\?`) that
/// `Path::exists()` universally reports as existing. Everywhere else (including
/// the Unix root `/` and real drive roots like `C:\` / `\\?\C:`) returns false so
/// their genuine existence is honored.
fn is_universal_pseudo_root(ancestor: &Path) -> bool {
    matches!(ancestor.as_os_str().to_str(), Some(r"\\") | Some(r"\\?"))
}

/// Human-readable, EDR-safe drift warning. Surfaces a command the **user**
/// runs in their own shell to overwrite the stale running binary with the newer
/// install — the daemon itself never copies or replaces the executable.
pub fn stale_warning(stale: &StaleBinary) -> String {
    let refresh = if cfg!(windows) {
        format!(
            "    Get-Process symforge | Stop-Process -Force\n    \
             Copy-Item \"{}\" \"{}\" -Force",
            stale.newer_path, stale.running_path
        )
    } else {
        format!(
            "    pkill -f symforge 2>/dev/null; sleep 1\n    \
             cp -f \"{}\" \"{}\"",
            stale.newer_path, stale.running_path
        )
    };
    format!(
        "── \u{26a0} Version drift ──\n\
         This daemon is serving symforge {running_ver} from:\n  {running_path}\n\
         but a newer install ({newer_ver}) exists at:\n  {newer_path}\n\
         The MCP daemon is serving stale code. In your own shell, overwrite the\n\
         stale binary with the newer install, then reconnect your MCP client (e.g. /mcp):\n{refresh}",
        running_ver = stale.running_version,
        running_path = stale.running_path,
        newer_ver = stale.newer_version,
        newer_path = stale.newer_path,
    )
}

/// Convenience: the drift warning string, or `None` when the running binary is
/// the newest known. Callers append this to diagnostics (e.g. `health`).
pub fn drift_banner(home: &Path) -> Option<String> {
    detect_stale(home).map(|stale| stale_warning(&stale))
}

/// [`drift_banner`] against the default [`resolve_home`] location.
pub fn drift_banner_default() -> Option<String> {
    drift_banner(&resolve_home()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn write_registry(home: &Path, entries: &[(&str, &str)]) {
        let map: BTreeMap<String, String> = entries
            .iter()
            .map(|(p, v)| ((*p).to_string(), (*v).to_string()))
            .collect();
        atomic_write(home, &map).unwrap();
    }

    #[test]
    fn record_self_writes_and_is_idempotent() {
        let tmp = tempfile::TempDir::new().unwrap();
        let home = tmp.path();
        record_self(home);
        let after_first = std::fs::read_to_string(registry_path(home)).unwrap();
        let key = current_exe_key().unwrap();
        let map = load(home);
        assert_eq!(map.get(&key).map(String::as_str), Some(self_version()));

        // A second record with the same version must not rewrite the file.
        record_self(home);
        let after_second = std::fs::read_to_string(registry_path(home)).unwrap();
        assert_eq!(after_first, after_second);
    }

    #[test]
    fn prune_missing_entries_drops_dead_paths_and_keeps_live_ones() {
        let tmp = tempfile::TempDir::new().unwrap();
        let home = tmp.path();
        // One entry points at a real file, one at a path that no longer exists.
        let live = tmp.path().join("live-symforge");
        std::fs::write(&live, b"binary").unwrap();
        let live_key = live.to_string_lossy().into_owned();
        let dead_key = tmp
            .path()
            .join("gone-symforge")
            .to_string_lossy()
            .into_owned();
        write_registry(
            home,
            &[(live_key.as_str(), "7.15.4"), (dead_key.as_str(), "7.14.4")],
        );

        let removed = prune_missing_entries(home);

        assert_eq!(removed, 1, "exactly the dead entry should be pruned");
        let map = load(home);
        assert!(map.contains_key(&live_key), "live entry kept");
        assert!(!map.contains_key(&dead_key), "dead entry removed");

        // Idempotent: a second prune removes nothing.
        assert_eq!(prune_missing_entries(home), 0);
    }

    #[test]
    fn prune_missing_entries_drops_dead_entry_when_whole_subtree_was_deleted() {
        // Regression: a removed git worktree's binary lives several directories
        // deep. The immediate parent (and its parents) were all deleted along with
        // the subtree, but the temp root still exists (the drive is online). The
        // old immediate-parent-only check kept this dead entry forever; it must be
        // pruned now.
        let tmp = tempfile::TempDir::new().unwrap();
        let home = tmp.path();

        let gone_root = tmp.path().join("gone");
        let dead = gone_root.join("deep").join("symforge.exe");
        // Materialize then remove the whole `gone/` subtree, so `gone`, `gone/deep`
        // and the leaf are all absent while `tmp` (an ancestor) still exists.
        std::fs::create_dir_all(dead.parent().unwrap()).unwrap();
        std::fs::write(&dead, b"binary").unwrap();
        std::fs::remove_dir_all(&gone_root).unwrap();
        assert!(!gone_root.exists(), "subtree must be gone for the test");
        assert!(
            tmp.path().exists(),
            "an ancestor must remain (drive online)"
        );

        let dead_key = dead.to_string_lossy().into_owned();

        let live = tmp.path().join("live-symforge");
        std::fs::write(&live, b"binary").unwrap();
        let live_key = live.to_string_lossy().into_owned();

        write_registry(
            home,
            &[(live_key.as_str(), "7.15.4"), (dead_key.as_str(), "7.14.4")],
        );

        let removed = prune_missing_entries(home);

        assert_eq!(removed, 1, "the deleted-subtree entry must be pruned");
        let map = load(home);
        assert!(map.contains_key(&live_key), "live entry kept");
        assert!(
            !map.contains_key(&dead_key),
            "dead deleted-subtree entry removed"
        );
    }

    // An offline removable/network mount means EVERY ancestor up to the drive root
    // is unreachable. That can only be modelled where a drive root itself can be
    // absent — on Windows a `Z:\` root does not exist when the drive is unplugged.
    // On Unix the filesystem root `/` always exists, so a transiently-offline mount
    // there still has a reachable ancestor and, by design, a vanished file under it
    // is treated as a genuine deletion. We therefore exercise the offline-keep path
    // on Windows only; the Unix prune-on-deletion behavior is covered by the
    // deleted-subtree test above.
    //
    // The registry stores canonicalized paths, which on Windows are VERBATIM
    // (`\\?\C:\...`). Their ancestor chain ends in the pseudo-roots `\\?` and `\\`,
    // both of which `Path::exists()` always reports as existing — so we verify the
    // verbatim form specifically to prove `mount_is_reachable` ignores those
    // pseudo-roots and still keeps an offline entry. We pick an unmapped drive
    // letter at runtime so the test is independent of the host's drive layout.
    #[cfg(windows)]
    #[test]
    fn prune_missing_entries_keeps_entry_on_offline_mount() {
        let tmp = tempfile::TempDir::new().unwrap();
        let home = tmp.path();

        // Find a drive letter whose root does not exist (genuinely unmapped).
        let absent = ('D'..='Z')
            .rev()
            .find(|d| !Path::new(&format!(r"{d}:\")).exists())
            .expect("at least one drive letter D..=Z should be unmapped on the test host");

        // Verbatim canonical form (what `current_exe_key` records on Windows).
        // The genuine drive root `\\?\{absent}:` is absent; only the universal
        // pseudo-roots (`\\?`, `\\`) may report as existing — which `Path::exists()`
        // does inconsistently depending on the path's internal representation. The
        // load-bearing predicate `mount_is_reachable` must ignore those pseudo-roots
        // and classify the mount as UNREACHABLE regardless.
        let offline_key = format!(r"\\?\{absent}:\offline-mount\bin\symforge.exe");
        assert!(
            !mount_is_reachable(Path::new(&offline_key)),
            "an unmapped drive must be classified unreachable despite the pseudo-root"
        );

        write_registry(home, &[(offline_key.as_str(), "7.14.4")]);

        let removed = prune_missing_entries(home);

        assert_eq!(removed, 0, "an offline-mount entry must be kept");
        assert!(load(home).contains_key(&offline_key), "offline entry kept");
    }

    #[test]
    fn detect_stale_flags_newer_version_at_existing_path() {
        let tmp = tempfile::TempDir::new().unwrap();
        let home = tmp.path();
        // A real file stands in for the "newer" binary so the existence check passes.
        let newer = tmp.path().join("newer-symforge");
        std::fs::write(&newer, b"binary").unwrap();
        let newer_key = std::fs::canonicalize(&newer)
            .unwrap_or(newer)
            .to_string_lossy()
            .into_owned();

        let running = current_exe_key().unwrap();
        write_registry(
            home,
            &[
                (running.as_str(), self_version()),
                (newer_key.as_str(), "999.0.0"),
            ],
        );

        let stale = detect_stale(home).expect("newer version at another path is stale");
        assert_eq!(stale.newer_version, "999.0.0");
        assert_eq!(stale.newer_path, newer_key);
        assert_eq!(stale.running_version, self_version());
    }

    #[test]
    fn detect_stale_ignores_missing_newer_binary() {
        let tmp = tempfile::TempDir::new().unwrap();
        let home = tmp.path();
        let running = current_exe_key().unwrap();
        // Newer version listed, but its path does not exist on disk.
        write_registry(
            home,
            &[
                (running.as_str(), self_version()),
                ("/nonexistent/symforge-binary", "999.0.0"),
            ],
        );
        assert!(detect_stale(home).is_none());
    }

    #[test]
    fn detect_stale_returns_none_when_self_is_newest() {
        let tmp = tempfile::TempDir::new().unwrap();
        let home = tmp.path();
        let running = current_exe_key().unwrap();
        write_registry(
            home,
            &[
                (running.as_str(), self_version()),
                ("/some/old/symforge", "0.0.1"),
            ],
        );
        assert!(detect_stale(home).is_none());
    }

    #[test]
    fn detect_stale_returns_none_on_empty_registry() {
        let tmp = tempfile::TempDir::new().unwrap();
        assert!(detect_stale(tmp.path()).is_none());
    }

    #[test]
    fn stale_warning_includes_paths_and_versions() {
        let stale = StaleBinary {
            running_version: "7.14.4".to_string(),
            running_path: "/home/u/.symforge/bin/symforge".to_string(),
            newer_version: "7.14.5".to_string(),
            newer_path: "/home/u/.npm/symforge".to_string(),
        };
        let text = stale_warning(&stale);
        assert!(text.contains("Version drift"));
        assert!(text.contains("7.14.4"));
        assert!(text.contains("7.14.5"));
        assert!(text.contains("/home/u/.symforge/bin/symforge"));
        assert!(text.contains("/home/u/.npm/symforge"));
    }
}
