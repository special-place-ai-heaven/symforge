//! EDR-safe version-drift detection for the SymForge binary.
//!
//! SymForge is launched two ways that can drift apart: a global npm package
//! (refreshed by `npm install -g`) and a *durable* per-user binary under
//! `~/.symforge/bin` that MCP clients launch directly. `npm install` updates
//! the former but not the latter, so a stale durable binary can silently serve
//! old code to every harness.
//!
//! This module detects that drift **without ever copying, executing, or
//! downloading anything** — the one and only side effect is reading and
//! (rarely) atomically rewriting a small plain-text JSON file. That keeps it
//! clear of antivirus / EDR heuristics that flag a running process which drops
//! or overwrites executables. The actual refresh is left to a user-run command
//! surfaced in the warning; the daemon never replaces its own binary.
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

/// Human-readable, EDR-safe drift warning. Surfaces a command the **user**
/// runs in their own shell to refresh the durable binary — the daemon itself
/// never copies or replaces the executable.
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
         The MCP daemon is serving stale code. In your own shell, refresh the\n\
         durable binary, then reconnect your MCP client (e.g. /mcp):\n{refresh}",
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
