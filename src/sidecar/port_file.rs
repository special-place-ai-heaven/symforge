//! Port and PID file management for the HTTP sidecar.
//!
//! All files live under `.symforge/` beside the project when one is known, or
//! under the user-level SymForge home when the launch cwd is unsafe/unwritable
//! (see [`crate::paths::ensure_runtime_symforge_dir`]).
//! The hook binary reads the sidecar port file to locate the running sidecar.
//!
//! Runtime filenames are OS-tagged (`sidecar.<os>.port`, see
//! [`crate::paths::os_tagged_runtime_file_name`]) so a Windows symforge and a
//! WSL/Linux symforge sharing one physical project `.symforge/` dir can never read
//! each other's loopback port. The writer (here) and the `symforge hook` reader both
//! derive the tag from the same compile-time `std::env::consts::OS`, so for a given
//! OS they always agree. Legacy un-tagged files are still READ as a fallback for one
//! release so an upgrade does not orphan a sidecar started by the previous binary.

use std::io::{self, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const DIR_NAME: &str = crate::paths::SYMFORGE_DIR_NAME;

// Legacy (pre-OS-tag) names. Read-only fallback + cleanup for one release window.
const LEGACY_PORT_FILE: &str = "sidecar.port";
const LEGACY_PID_FILE: &str = "sidecar.pid";
const LEGACY_SESSION_FILE: &str = "sidecar.session";

/// OS-tagged sidecar port filename, e.g. `sidecar.windows.port`.
fn port_file_name() -> String {
    crate::paths::os_tagged_runtime_file_name("sidecar", "port")
}
/// OS-tagged sidecar pid filename, e.g. `sidecar.linux.pid`.
fn pid_file_name() -> String {
    crate::paths::os_tagged_runtime_file_name("sidecar", "pid")
}
/// OS-tagged sidecar session filename, e.g. `sidecar.macos.session`.
fn session_file_name() -> String {
    crate::paths::os_tagged_runtime_file_name("sidecar", "session")
}

/// Read a runtime file under `dir`, preferring the OS-tagged name and falling back
/// to the legacy un-tagged name. Returns the first that exists/parses.
fn read_runtime_file(dir: &Path, tagged: &str, legacy: &str) -> io::Result<String> {
    match std::fs::read_to_string(dir.join(tagged)) {
        Ok(contents) => Ok(contents),
        Err(e) if e.kind() == io::ErrorKind::NotFound => std::fs::read_to_string(dir.join(legacy)),
        Err(e) => Err(e),
    }
}

/// Ensure a usable `.symforge/` runtime directory for sidecar port/pid files.
pub fn ensure_symforge_dir(project_root: Option<&Path>) -> io::Result<PathBuf> {
    crate::paths::ensure_runtime_symforge_dir(project_root)
}

/// Resolve the runtime `.symforge/` directory without creating it.
fn resolve_symforge_dir(project_root: Option<&Path>) -> PathBuf {
    let cwd = std::env::current_dir().ok();
    crate::paths::select_runtime_data_base(project_root, cwd.as_deref())
}

/// Write the sidecar port to `.symforge/sidecar.port`.
///
/// The file contains ONLY the port number as ASCII digits, no trailing newline.
/// This is the convention the hook binary relies on.
pub fn write_port_file(port: u16, project_root: Option<&Path>) -> io::Result<()> {
    let dir = ensure_symforge_dir(project_root)?;
    let path = dir.join(port_file_name());
    let mut file = std::fs::File::create(&path)?;
    write!(file, "{port}")?;
    Ok(())
}

/// Write the sidecar PID to `.symforge/sidecar.<os>.pid`.
///
/// The file contains ONLY the PID as ASCII digits, no trailing newline.
pub fn write_pid_file(pid: u32, project_root: Option<&Path>) -> io::Result<()> {
    let dir = ensure_symforge_dir(project_root)?;
    let path = dir.join(pid_file_name());
    let mut file = std::fs::File::create(&path)?;
    write!(file, "{pid}")?;
    Ok(())
}

/// Write the daemon/session proxy identifier to `.symforge/sidecar.<os>.session`.
pub fn write_session_file(session_id: &str, project_root: Option<&Path>) -> io::Result<()> {
    let dir = ensure_symforge_dir(project_root)?;
    let path = dir.join(session_file_name());
    let mut file = std::fs::File::create(&path)?;
    write!(file, "{session_id}")?;
    Ok(())
}

/// Remove only the daemon/session proxy file, preserving any live local sidecar port/pid files.
pub fn cleanup_session_file() {
    let dir = PathBuf::from(DIR_NAME);
    let _ = std::fs::remove_file(dir.join(session_file_name()));
    let _ = std::fs::remove_file(dir.join(LEGACY_SESSION_FILE));
}

/// Read and parse the port from `.symforge/sidecar.<os>.port` (legacy fallback).
///
/// Returns an error if the file doesn't exist or contains invalid data.
pub fn read_port() -> io::Result<u16> {
    read_port_at(&resolve_symforge_dir(None))
}

fn read_port_at(dir: &Path) -> io::Result<u16> {
    let contents = read_runtime_file(dir, &port_file_name(), LEGACY_PORT_FILE)?;
    contents
        .trim()
        .parse::<u16>()
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

fn read_pid_at(dir: &Path) -> io::Result<u32> {
    let contents = read_runtime_file(dir, &pid_file_name(), LEGACY_PID_FILE)?;
    contents
        .trim()
        .parse::<u32>()
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidecarLiveness {
    Alive,
    Dead,
    Unknown,
    NoSidecar,
}

impl SidecarLiveness {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Alive => "alive",
            Self::Dead => "dead",
            Self::Unknown => "unknown",
            Self::NoSidecar => "none",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SidecarStatus {
    pub pid: Option<u32>,
    pub port: Option<u16>,
    pub liveness: SidecarLiveness,
    pub detail: Option<String>,
}

impl SidecarStatus {
    fn no_sidecar() -> Self {
        Self {
            pid: None,
            port: None,
            liveness: SidecarLiveness::NoSidecar,
            detail: None,
        }
    }
}

fn sidecar_files_exist(dir: &Path) -> bool {
    dir.join(port_file_name()).exists()
        || dir.join(pid_file_name()).exists()
        || dir.join(session_file_name()).exists()
        || dir.join(LEGACY_PORT_FILE).exists()
        || dir.join(LEGACY_PID_FILE).exists()
        || dir.join(LEGACY_SESSION_FILE).exists()
}

fn sidecar_socket_addr(bind_host: &str, port: u16) -> io::Result<std::net::SocketAddr> {
    let addr = if bind_host.contains(':') {
        format!("[{bind_host}]:{port}")
    } else {
        format!("{bind_host}:{port}")
    };
    addr.parse()
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))
}

fn sidecar_port_is_alive(bind_host: &str, port: u16) -> io::Result<bool> {
    let sock_addr = sidecar_socket_addr(bind_host, port)?;
    Ok(TcpStream::connect_timeout(&sock_addr, Duration::from_millis(200)).is_ok())
}

pub fn read_sidecar_status_at(symforge_dir: &Path, bind_host: &str) -> SidecarStatus {
    if !sidecar_files_exist(symforge_dir) {
        return SidecarStatus::no_sidecar();
    }

    let mut details = Vec::new();
    let pid = match read_pid_at(symforge_dir) {
        Ok(pid) => Some(pid),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            details.push("sidecar.pid missing".to_string());
            None
        }
        Err(error) => {
            details.push(format!("sidecar.pid invalid: {error}"));
            None
        }
    };
    let port = match read_port_at(symforge_dir) {
        Ok(port) => Some(port),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            details.push("sidecar.port missing".to_string());
            None
        }
        Err(error) => {
            details.push(format!("sidecar.port invalid: {error}"));
            None
        }
    };

    let liveness = match port {
        Some(port) => match sidecar_port_is_alive(bind_host, port) {
            Ok(true) => SidecarLiveness::Alive,
            Ok(false) => SidecarLiveness::Dead,
            Err(error) => {
                details.push(format!("sidecar port probe unavailable: {error}"));
                SidecarLiveness::Unknown
            }
        },
        None => SidecarLiveness::Unknown,
    };

    SidecarStatus {
        pid,
        port,
        liveness,
        detail: (!details.is_empty()).then(|| details.join("; ")),
    }
}

pub fn read_sidecar_status(bind_host: &str) -> SidecarStatus {
    read_sidecar_status_at(&resolve_symforge_dir(None), bind_host)
}

/// Remove both port and PID files. Ignores all errors.
///
/// Called during sidecar shutdown — it is safe to call even if files don't exist.
pub fn cleanup_files() {
    cleanup_files_at(&resolve_symforge_dir(None));
}

/// Remove port/PID/session files from a specific directory (both the OS-tagged names
/// and the legacy un-tagged names, so a dead old-binary file cannot shadow a fresh one).
/// Used by the panic hook which cannot rely on CWD.
pub fn cleanup_files_at(dir: &std::path::Path) {
    let _ = std::fs::remove_file(dir.join(port_file_name()));
    let _ = std::fs::remove_file(dir.join(pid_file_name()));
    let _ = std::fs::remove_file(dir.join(session_file_name()));
    let _ = std::fs::remove_file(dir.join(LEGACY_PORT_FILE));
    let _ = std::fs::remove_file(dir.join(LEGACY_PID_FILE));
    let _ = std::fs::remove_file(dir.join(LEGACY_SESSION_FILE));
}

/// Check whether the port/PID files are stale (i.e., the old sidecar is no longer running).
///
/// If no port file exists, there is nothing stale — returns `false`.
/// If a port file exists, attempts a blocking TCP connect to `{bind_host}:{port}` with a
/// 200 ms timeout. If the connection succeeds the sidecar is alive and returns `false`.
/// If the connection is refused or times out, the files are stale: calls `cleanup_files()`
/// and returns `true`.
pub fn check_stale(bind_host: &str) -> bool {
    let port = match read_port() {
        Ok(p) => p,
        Err(_) => return false, // No port file — nothing to clean up.
    };

    match sidecar_port_is_alive(bind_host, port) {
        Ok(true) => false, // Connection succeeded — sidecar is still alive.
        Ok(false) => {
            // Connection refused or timed out — files are stale.
            cleanup_files();
            true
        }
        Err(_) => {
            // Cannot determine staleness when the address is unparseable —
            // default to "not stale" to avoid deleting a live sidecar's files.
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use tempfile::TempDir;

    /// Serialize all cwd-manipulating tests so they don't interfere with each other.
    /// cwd is process-global — parallel manipulation causes flaky failures.
    static CWD_LOCK: Mutex<()> = Mutex::new(());

    fn stable_cwd() -> PathBuf {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")))
    }

    fn restore_cwd(path: &std::path::Path) {
        if std::env::set_current_dir(path).is_err() {
            std::env::set_current_dir(env!("CARGO_MANIFEST_DIR"))
                .expect("manifest dir must be a valid cwd fallback");
        }
    }

    /// Run a test with cwd set to a temp directory so file operations are isolated.
    /// Holds `CWD_LOCK` for the duration, restores cwd on exit (even on panic).
    fn with_temp_dir<F: FnOnce() + std::panic::UnwindSafe>(f: F) {
        let _guard = CWD_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let tmp = TempDir::new().unwrap();
        let original = stable_cwd();
        std::env::set_current_dir(tmp.path()).unwrap();
        let result = std::panic::catch_unwind(f);
        restore_cwd(&original);
        drop(tmp);
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }

    #[test]
    fn test_write_read_port_roundtrip() {
        with_temp_dir(|| {
            write_port_file(12345, None).expect("write_port_file should succeed");
            let port = read_port().expect("read_port should succeed after write");
            assert_eq!(port, 12345, "port roundtrip must preserve value");
        });
    }

    #[test]
    fn test_write_port_file_no_trailing_newline() {
        with_temp_dir(|| {
            write_port_file(8080, None).expect("write_port_file should succeed");
            // Read while still inside the temp cwd so the relative path resolves correctly.
            let port_path = PathBuf::from(DIR_NAME).join(port_file_name());
            let bytes = std::fs::read(&port_path).unwrap();
            assert_eq!(
                bytes, b"8080",
                "port file must contain ONLY the digits, no newline"
            );
        });
    }

    #[test]
    fn test_write_is_os_tagged_only() {
        with_temp_dir(|| {
            write_port_file(8080, None).expect("write_port_file should succeed");
            let dir = PathBuf::from(DIR_NAME);
            // Writer is tag-pure: the OS-tagged file exists, the legacy name does NOT.
            assert!(
                dir.join(port_file_name()).exists(),
                "OS-tagged port file must exist after write"
            );
            assert!(
                !dir.join(LEGACY_PORT_FILE).exists(),
                "writer must NOT create a legacy un-tagged port file (would re-open cross-OS collision)"
            );
            assert!(
                port_file_name().contains(std::env::consts::OS),
                "tagged name must carry this OS"
            );
        });
    }

    #[test]
    fn test_read_falls_back_to_legacy_untagged() {
        with_temp_dir(|| {
            // Simulate a sidecar started by an OLD (pre-tag) binary.
            let dir = ensure_symforge_dir(None).expect("dir");
            std::fs::write(dir.join(LEGACY_PORT_FILE), b"7777").unwrap();
            let port = read_port().expect("read_port must fall back to legacy file");
            assert_eq!(port, 7777, "legacy fallback must read the un-tagged port");
        });
    }

    #[test]
    fn test_tagged_wins_over_legacy() {
        with_temp_dir(|| {
            let dir = ensure_symforge_dir(None).expect("dir");
            std::fs::write(dir.join(LEGACY_PORT_FILE), b"1111").unwrap();
            std::fs::write(dir.join(port_file_name()), b"2222").unwrap();
            let port = read_port().expect("read_port should succeed");
            assert_eq!(
                port, 2222,
                "OS-tagged file must take precedence over legacy"
            );
        });
    }

    #[test]
    fn test_cleanup_removes_files() {
        with_temp_dir(|| {
            write_port_file(9000, None).expect("write should succeed");
            write_pid_file(12345, None).expect("write should succeed");
            // Also drop legacy files to prove cleanup removes BOTH.
            let dir = PathBuf::from(DIR_NAME);
            std::fs::write(dir.join(LEGACY_PORT_FILE), b"9000").unwrap();
            std::fs::write(dir.join(LEGACY_PID_FILE), b"12345").unwrap();

            assert!(
                dir.join(port_file_name()).exists(),
                "tagged port file should exist before cleanup"
            );
            assert!(
                dir.join(pid_file_name()).exists(),
                "tagged pid file should exist before cleanup"
            );

            cleanup_files();

            for name in [
                port_file_name(),
                pid_file_name(),
                LEGACY_PORT_FILE.to_string(),
                LEGACY_PID_FILE.to_string(),
            ] {
                assert!(
                    !dir.join(&name).exists(),
                    "{name} should be gone after cleanup (tagged + legacy)"
                );
            }
        });
    }

    #[test]
    fn test_cleanup_is_noop_when_no_files() {
        with_temp_dir(|| {
            // Should not panic even if files don't exist.
            cleanup_files();
        });
    }

    #[test]
    fn test_read_port_missing_returns_error() {
        with_temp_dir(|| {
            let result = read_port();
            assert!(
                result.is_err(),
                "read_port should return error when file is missing"
            );
        });
    }

    #[test]
    fn test_ensure_symforge_dir_creates_directory() {
        with_temp_dir(|| {
            let dir = ensure_symforge_dir(None).expect("ensure_symforge_dir should succeed");
            assert!(
                dir.exists(),
                ".symforge directory should exist after ensure_symforge_dir"
            );
            assert!(dir.is_dir(), "path should be a directory");
        });
    }

    #[test]
    fn test_ensure_symforge_dir_idempotent() {
        with_temp_dir(|| {
            ensure_symforge_dir(None).expect("first call should succeed");
            ensure_symforge_dir(None).expect("second call should also succeed (idempotent)");
        });
    }
    #[test]
    fn test_check_stale_returns_false_when_no_port_file() {
        with_temp_dir(|| {
            let is_stale = check_stale("127.0.0.1");
            assert!(!is_stale, "no port file means nothing is stale");
        });
    }

    #[test]
    fn test_check_stale_cleans_up_when_port_is_closed() {
        with_temp_dir(|| {
            // Write a port that is very unlikely to have anything listening.
            write_port_file(19999, None).expect("write should succeed");
            write_pid_file(99999, None).expect("write should succeed");

            let is_stale = check_stale("127.0.0.1");
            assert!(
                is_stale,
                "port 19999 should be detected as stale (nothing listening)"
            );

            // Cleanup should have been called.
            let dir = PathBuf::from(DIR_NAME);
            assert!(
                !dir.join(port_file_name()).exists(),
                "port file cleaned up after stale detection"
            );
        });
    }
}
