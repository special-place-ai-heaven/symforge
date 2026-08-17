//! Port and PID file management for the HTTP sidecar.
//!
//! All active files live under the process-global control-state `sidecar/`
//! namespace. Legacy fixed filenames remain read-only migration fallbacks.
//! The hook binary reads the sidecar port file to locate the running sidecar.
//!
//! Runtime filenames are OS-tagged (`sidecar.<os>.port`, see
//! [`crate::paths::os_tagged_runtime_file_name`]) so a Windows symforge and a
//! WSL/Linux symforge sharing one control-state root can never read
//! each other's loopback port. The writer (here) and the `symforge hook` reader both
//! derive the tag from the same compile-time `std::env::consts::OS`, so for a given
//! OS they always agree. Legacy un-tagged files are still READ as a fallback for one
//! release so an upgrade does not orphan a sidecar started by the previous binary.

use std::io::{self, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::domain::ControlStateDir;

pub const DIR_NAME: &str = crate::paths::SYMFORGE_DIR_NAME;
const SIDECAR_CONTROL_DIR: &str = "sidecar";

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

/// Ensure the process-global sidecar descriptor namespace.
pub fn ensure_symforge_dir(control_state_dir: &ControlStateDir) -> io::Result<PathBuf> {
    crate::paths::ensure_control_state_dir(control_state_dir)?;
    let dir = resolve_symforge_dir(control_state_dir);
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Resolve the isolated sidecar namespace without creating it.
fn resolve_symforge_dir(control_state_dir: &ControlStateDir) -> PathBuf {
    crate::paths::control_state_path(control_state_dir, SIDECAR_CONTROL_DIR)
}

/// Write the sidecar port to `.symforge/sidecar.port`.
///
/// The file contains ONLY the port number as ASCII digits, no trailing newline.
/// This is the convention the hook binary relies on.
pub fn write_port_file(port: u16, control_state_dir: &ControlStateDir) -> io::Result<()> {
    let dir = ensure_symforge_dir(control_state_dir)?;
    let path = dir.join(port_file_name());
    let mut file = std::fs::File::create(&path)?;
    write!(file, "{port}")?;
    Ok(())
}

/// Write the sidecar PID to `.symforge/sidecar.<os>.pid`.
///
/// The file contains ONLY the PID as ASCII digits, no trailing newline.
pub fn write_pid_file(pid: u32, control_state_dir: &ControlStateDir) -> io::Result<()> {
    let dir = ensure_symforge_dir(control_state_dir)?;
    let path = dir.join(pid_file_name());
    let mut file = std::fs::File::create(&path)?;
    write!(file, "{pid}")?;
    Ok(())
}

/// Write the daemon/session proxy identifier to `.symforge/sidecar.<os>.session`.
pub fn write_session_file(session_id: &str, control_state_dir: &ControlStateDir) -> io::Result<()> {
    let dir = ensure_symforge_dir(control_state_dir)?;
    let path = dir.join(session_file_name());
    let mut file = std::fs::File::create(&path)?;
    write!(file, "{session_id}")?;
    Ok(())
}

/// Remove only the daemon/session proxy file, preserving any live local sidecar port/pid files.
pub fn cleanup_session_file(control_state_dir: &ControlStateDir) {
    let dir = resolve_symforge_dir(control_state_dir);
    let _ = std::fs::remove_file(dir.join(session_file_name()));
    let _ = std::fs::remove_file(dir.join(LEGACY_SESSION_FILE));
}

/// Read and parse the port from `.symforge/sidecar.<os>.port` (legacy fallback).
///
/// Returns an error if the file doesn't exist or contains invalid data.
pub fn read_port(control_state_dir: &ControlStateDir) -> io::Result<u16> {
    read_port_at(&resolve_symforge_dir(control_state_dir))
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

// ── Per-adapter session descriptors (Task 8) ──────────────────────────────
//
// The fixed `sidecar.<os>.{port,pid,session}` files are a single slot: with
// two adapters on one project root, the second overwrites the first and one
// adapter's shutdown deletes the other's records. Each adapter now writes ONE
// atomic JSON descriptor keyed by its own PID under `.symforge/sessions/`;
// cleanup removes only the caller's descriptor, and readers scan the
// directory, validate identity, and select the freshest LIVE record. The
// fixed files remain as a read-compatible migration aid only — no longer
// written.

const SESSIONS_DIR: &str = "sessions";
const DESCRIPTOR_SCAN_TIMEOUT: Duration = Duration::from_millis(100);
const SIDECAR_PROBE_TIMEOUT: Duration = Duration::from_millis(200);

/// OS-tagged descriptor filename for one adapter process, e.g.
/// `sidecar.12345.windows.json`.
fn descriptor_file_name(pid: u32) -> String {
    crate::paths::os_tagged_runtime_file_name(&format!("sidecar.{pid}"), "json")
}

fn now_unix_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// One adapter/session runtime record (Task 8).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SessionDescriptor {
    /// Daemon session id when this adapter proxies a daemon session; `None`
    /// for a purely local sidecar.
    pub session_id: Option<String>,
    /// The project root this adapter serves, for identity validation.
    pub project_root: Option<String>,
    /// Native-safe identity derived from the canonical root. Old descriptors
    /// deserialize with `None` but are rejected by root-scoped selection.
    #[serde(default)]
    pub project_id: Option<String>,
    /// Boot epoch of the daemon this adapter's session belongs to (10.1+).
    /// Selection rejects a descriptor whose epoch differs from the live
    /// daemon's /health epoch — a stale record must not alias an unrelated
    /// `session-N` after a daemon restart.
    #[serde(default)]
    pub daemon_started_at: Option<u64>,
    pub pid: u32,
    pub port: u16,
    /// Heartbeat/update time; refreshed on every write.
    pub updated_at_unix_secs: u64,
}

/// Write THIS process's descriptor atomically (temp + rename) under
/// `<dir>/sessions/`. Idempotent: rewriting refreshes `updated_at`.
pub(crate) fn write_descriptor_for_pid_at(
    dir: &Path,
    pid: u32,
    port: u16,
    session_id: Option<&str>,
    project_root: Option<&Path>,
    daemon_started_at: Option<u64>,
) -> io::Result<()> {
    let sessions = dir.join(SESSIONS_DIR);
    std::fs::create_dir_all(&sessions)?;
    let project_id = project_root.map(crate::daemon::project_key);
    let project_root = project_root
        .map(|root| {
            root.to_str().map(str::to_string).ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "sidecar project root is not valid UTF-8",
                )
            })
        })
        .transpose()?;
    let descriptor = SessionDescriptor {
        session_id: session_id.map(str::to_string),
        project_root,
        project_id,
        daemon_started_at,
        pid,
        port,
        updated_at_unix_secs: now_unix_secs(),
    };
    let bytes = serde_json::to_vec_pretty(&descriptor)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let final_path = sessions.join(descriptor_file_name(pid));
    let tmp_path = sessions.join(format!("{}.tmp", descriptor_file_name(pid)));
    std::fs::write(&tmp_path, &bytes)?;
    match std::fs::rename(&tmp_path, &final_path) {
        Ok(()) => Ok(()),
        Err(error) => {
            let _ = std::fs::remove_file(&tmp_path);
            Err(error)
        }
    }
}

/// Public writer: this adapter's descriptor for `port`/`session_id` under the
/// project's runtime `.symforge/` dir.
pub fn write_session_descriptor(
    control_state_dir: &ControlStateDir,
    port: u16,
    session_id: Option<&str>,
    project_root: Option<&Path>,
    daemon_started_at: Option<u64>,
) -> io::Result<()> {
    let dir = ensure_symforge_dir(control_state_dir)?;
    write_descriptor_for_pid_at(
        &dir,
        std::process::id(),
        port,
        session_id,
        project_root,
        daemon_started_at,
    )
}

/// Remove exactly one pid's descriptor. Never touches siblings.
pub(crate) fn cleanup_descriptor_for_pid_at(dir: &Path, pid: u32) {
    let _ = std::fs::remove_file(dir.join(SESSIONS_DIR).join(descriptor_file_name(pid)));
    let _ = std::fs::remove_file(
        dir.join(SESSIONS_DIR)
            .join(format!("{}.tmp", descriptor_file_name(pid))),
    );
}

/// Remove ONLY this process's descriptor (Task 8 contract: an adapter's
/// shutdown can never delete or invalidate a sibling adapter's record).
pub fn cleanup_own_descriptor(control_state_dir: &ControlStateDir) {
    cleanup_descriptor_for_pid_at(&resolve_symforge_dir(control_state_dir), std::process::id());
}

/// Same, against an explicit dir (panic hooks cannot rely on CWD).
pub fn cleanup_own_descriptor_at(dir: &Path) {
    cleanup_descriptor_for_pid_at(dir, std::process::id());
}

/// Remove descriptors whose process is gone or whose port no longer answers —
/// the update/repair path's stale-record hygiene. Live descriptors are untouched.
pub fn cleanup_stale_descriptors_at(dir: &Path, bind_host: &str) {
    prune_dead_descriptor_files_at(dir);
    for descriptor in read_descriptors_at(dir) {
        let alive = process_may_be_alive(descriptor.pid)
            && sidecar_port_is_alive(bind_host, descriptor.port).unwrap_or(false);
        if !alive {
            cleanup_descriptor_for_pid_at(dir, descriptor.pid);
        }
    }
}

fn prune_dead_descriptor_files_at(dir: &Path) {
    let os_suffix = format!(".{}.json", std::env::consts::OS);
    let Ok(entries) = std::fs::read_dir(dir.join(SESSIONS_DIR)) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(pid) = name
            .to_str()
            .and_then(|name| name.strip_prefix("sidecar."))
            .and_then(|name| name.strip_suffix(&os_suffix))
            .and_then(|pid| pid.parse::<u32>().ok())
        else {
            continue;
        };
        if !process_may_be_alive(pid) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// All parseable descriptors for THIS OS under `<dir>/sessions/`.
fn read_descriptors_at(dir: &Path) -> Vec<SessionDescriptor> {
    let os_tag = format!(".{}.json", std::env::consts::OS);
    let mut descriptors = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir.join(SESSIONS_DIR)) else {
        return descriptors;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if !name.starts_with("sidecar.") || !name.ends_with(&os_tag) {
            continue;
        }
        if let Ok(contents) = std::fs::read_to_string(entry.path())
            && let Ok(descriptor) = serde_json::from_str::<SessionDescriptor>(&contents)
        {
            descriptors.push(descriptor);
        }
    }
    descriptors
}

/// Select the best descriptor for `dir`: identity-validated (a descriptor
/// naming a DIFFERENT project root than this dir's project is rejected, never
/// "last writer wins"), freshest first with a stable smallest-pid tie break,
/// returning the first live port found inside the fixed hook-time budget.
struct SelectedSidecar {
    status: SidecarStatus,
    session_id: Option<String>,
}

fn select_descriptor_status(
    dir: &Path,
    bind_host: &str,
    expected_project_root: Option<&Path>,
) -> Option<SelectedSidecar> {
    let scan_started = Instant::now();
    prune_dead_descriptor_files_at(dir);
    let expected_identity = match expected_project_root {
        Some(root) => match root.to_str() {
            Some(root_text) => Some((crate::daemon::project_key(root), root_text.to_string())),
            None => {
                return Some(SelectedSidecar {
                    status: SidecarStatus {
                        pid: None,
                        port: None,
                        liveness: SidecarLiveness::NoSidecar,
                        detail: Some(
                            "sidecar selection refused: project root is not valid UTF-8"
                                .to_string(),
                        ),
                    },
                    session_id: None,
                });
            }
        },
        None => None,
    };
    let mut candidates = Vec::new();
    let mut rejected = 0usize;
    for descriptor in read_descriptors_at(dir) {
        if let Some((expected_id, expected_root)) = expected_identity.as_ref() {
            let matches = descriptor.project_id.as_deref() == Some(expected_id.as_str())
                && descriptor
                    .project_root
                    .as_deref()
                    .is_some_and(|declared| same_root_identity(declared, expected_root));
            if !matches {
                rejected += 1;
                continue;
            }
        }
        if !process_may_be_alive(descriptor.pid) {
            continue;
        }
        candidates.push(descriptor);
    }
    if candidates.is_empty() {
        return (rejected > 0).then(|| SelectedSidecar {
            status: SidecarStatus {
                pid: None,
                port: None,
                liveness: SidecarLiveness::NoSidecar,
                detail: Some(format!(
                    "{rejected} descriptor(s) rejected: project-root identity mismatch"
                )),
            },
            session_id: None,
        });
    }

    candidates.sort_by(|a, b| {
        b.updated_at_unix_secs
            .cmp(&a.updated_at_unix_secs)
            .then(a.pid.cmp(&b.pid))
    });

    let selected = |best: &SessionDescriptor, liveness, epoch_rejected: usize| SelectedSidecar {
        status: SidecarStatus {
            pid: Some(best.pid),
            port: Some(best.port),
            liveness,
            detail: Some(format!(
                "descriptor sidecar.{} ({} candidate(s){}{})",
                best.pid,
                candidates.len(),
                if rejected > 0 {
                    format!(", {rejected} identity-rejected")
                } else {
                    String::new()
                },
                if epoch_rejected > 0 {
                    format!(", {epoch_rejected} epoch-rejected")
                } else {
                    String::new()
                }
            )),
        },
        session_id: best.session_id.clone(),
    };

    let mut probed = 0usize;
    let mut epoch_rejected = 0usize;
    for candidate in &candidates {
        let remaining = DESCRIPTOR_SCAN_TIMEOUT.saturating_sub(scan_started.elapsed());
        if remaining.is_zero() {
            break;
        }
        let alive = sidecar_port_is_alive_with_timeout(bind_host, candidate.port, remaining)
            .unwrap_or(false);
        probed += 1;
        if alive {
            // Daemon-backed records (session_id present) must also prove the
            // candidate port is the SAME daemon process the descriptor was
            // written against: after a daemon restart the new process re-issues
            // `session-N` ids from 1, so a stale descriptor would otherwise
            // alias an unrelated session. The boot-epoch probe is that proof.
            if candidate.session_id.is_some() {
                let remaining = DESCRIPTOR_SCAN_TIMEOUT.saturating_sub(scan_started.elapsed());
                if remaining.is_zero() {
                    break;
                }
                if !probe_daemon_epoch(
                    bind_host,
                    candidate.port,
                    candidate.daemon_started_at,
                    remaining,
                ) {
                    epoch_rejected += 1;
                    continue;
                }
            }
            return Some(selected(candidate, SidecarLiveness::Alive, epoch_rejected));
        }
    }

    if let Some(unprobed) = candidates.get(probed) {
        Some(selected(unprobed, SidecarLiveness::Unknown, epoch_rejected))
    } else {
        Some(selected(
            &candidates[0],
            SidecarLiveness::Dead,
            epoch_rejected,
        ))
    }
}

/// Semantic probe for daemon-backed descriptors: GET /health on the candidate
/// port and require the daemon's boot epoch to equal the descriptor's
/// recorded epoch (both absent is the legacy-compatible accept case: an old
/// daemon paired with an old descriptor). Any HTTP/parse failure or epoch
/// mismatch rejects the candidate — fail closed, never alias.
fn probe_daemon_epoch(
    bind_host: &str,
    port: u16,
    expected: Option<u64>,
    timeout: Duration,
) -> bool {
    use std::io::{Read, Write};
    let Ok(sock_addr) = sidecar_socket_addr(bind_host, port) else {
        return false;
    };
    let Ok(mut stream) = TcpStream::connect_timeout(&sock_addr, timeout) else {
        return false;
    };
    let _ = stream.set_read_timeout(Some(timeout));
    let _ = stream.set_write_timeout(Some(timeout));
    if stream
        .write_all(b"GET /health HTTP/1.0\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .is_err()
    {
        return false;
    }
    let mut buf = Vec::with_capacity(2048);
    let mut chunk = [0u8; 2048];
    loop {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                buf.extend_from_slice(&chunk[..n]);
                if buf.len() > 16 * 1024 {
                    return false;
                }
            }
            Err(_) => break,
        }
    }
    let Ok(response) = std::str::from_utf8(&buf) else {
        return false;
    };
    if !response.starts_with("HTTP/1.1 200") && !response.starts_with("HTTP/1.0 200") {
        return false;
    }
    let Some(body) = response.split("\r\n\r\n").nth(1) else {
        return false;
    };
    let Ok(health) = serde_json::from_str::<crate::daemon::DaemonHealth>(body) else {
        return false;
    };
    health.started_at_unix_secs == expected
}

#[cfg(windows)]
#[allow(unsafe_code)]
fn process_may_be_alive(pid: u32) -> bool {
    use windows::Win32::Foundation::{CloseHandle, ERROR_INVALID_PARAMETER, WAIT_OBJECT_0};
    use windows::Win32::System::Threading::{
        OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject,
    };

    if pid == 0 {
        return false;
    }

    // SAFETY: OpenProcess receives a PID from a parsed descriptor and requests
    // synchronization-only access. Access-denied remains conservatively alive.
    let handle = match unsafe { OpenProcess(PROCESS_SYNCHRONIZE, false, pid) } {
        Ok(handle) => handle,
        Err(error) => {
            return error.code() != windows::core::HRESULT::from_win32(ERROR_INVALID_PARAMETER.0);
        }
    };
    // SAFETY: `handle` is valid and was opened with synchronization access.
    let wait = unsafe { WaitForSingleObject(handle, 0) };
    // SAFETY: this is the single paired close for the handle opened above.
    unsafe {
        let _ = CloseHandle(handle);
    }
    wait != WAIT_OBJECT_0
}

#[cfg(unix)]
#[allow(unsafe_code)]
fn process_may_be_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    let Ok(pid) = libc::pid_t::try_from(pid) else {
        return false;
    };
    // SAFETY: signal 0 performs existence/permission checking only and does not
    // deliver a signal. ESRCH is the sole definitive "process is gone" result.
    let result = unsafe { libc::kill(pid, 0) };
    result == 0 || io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
}

#[cfg(not(any(unix, windows)))]
fn process_may_be_alive(_pid: u32) -> bool {
    true
}

/// Case-tolerant, separator-tolerant root comparison (Windows paths).
///
/// `cfg!(windows)` already folds case. That is not enough: Windows
/// `canonicalize` / `current_dir` often yield the extended-length verbatim
/// form (`\\?\C:\...`), and `Path::display` slash-unifies that to `//?/C:/...`
/// (what `health` prints as `project_root`). Sidecar descriptors typically
/// store the plain `C:\...` form. Slash+case folding still leaves
/// `//?/c:/proj` != `c:/proj`, so every descriptor is identity-rejected.
fn same_root_identity(a: &str, b: &str) -> bool {
    canon_root_identity(a) == canon_root_identity(b)
}

fn canon_root_identity_for_platform(raw: &str, windows: bool) -> String {
    if !windows {
        return raw.trim_end_matches('/').to_string();
    }

    let normalized = crate::daemon::normalized_path_text(raw, true);
    let stripped = if let Some(rest) = strip_ascii_prefix_ci(&normalized, "//?/unc/") {
        format!("//{rest}")
    } else if let Some(rest) = strip_ascii_prefix_ci(&normalized, "//?/") {
        rest.to_string()
    } else {
        normalized
    };
    let trimmed = stripped.trim_end_matches('/');
    trimmed.to_lowercase()
}

fn canon_root_identity(raw: &str) -> String {
    canon_root_identity_for_platform(raw, cfg!(windows))
}

fn strip_ascii_prefix_ci<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    let head = s.get(..prefix.len())?;
    head.eq_ignore_ascii_case(prefix)
        .then_some(&s[prefix.len()..])
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
    pub(crate) fn no_sidecar() -> Self {
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
    sidecar_port_is_alive_with_timeout(bind_host, port, SIDECAR_PROBE_TIMEOUT)
}

fn sidecar_port_is_alive_with_timeout(
    bind_host: &str,
    port: u16,
    timeout: Duration,
) -> io::Result<bool> {
    let sock_addr = sidecar_socket_addr(bind_host, port)?;
    Ok(TcpStream::connect_timeout(&sock_addr, timeout).is_ok())
}

fn read_sidecar_status_for_root_at(
    symforge_dir: &Path,
    bind_host: &str,
    expected_project_root: Option<&Path>,
) -> SidecarStatus {
    // Task 8: per-adapter descriptors are authoritative; the fixed files below
    // are only a read-compatible migration aid for records written by older
    // binaries.
    if let Some(selected) = select_descriptor_status(symforge_dir, bind_host, expected_project_root)
    {
        return selected.status;
    }
    if expected_project_root.is_some() {
        return SidecarStatus {
            pid: None,
            port: None,
            liveness: SidecarLiveness::NoSidecar,
            detail: Some(
                "root-scoped sidecar lookup refused legacy fixed files without project identity"
                    .to_string(),
            ),
        };
    }
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

pub fn read_sidecar_status_at(symforge_dir: &Path, bind_host: &str) -> SidecarStatus {
    read_sidecar_status_for_root_at(symforge_dir, bind_host, symforge_dir.parent())
}

pub fn read_sidecar_status(
    control_state_dir: &ControlStateDir,
    bind_host: &str,
    expected_project_root: Option<&Path>,
) -> SidecarStatus {
    read_sidecar_status_for_root_at(
        &resolve_symforge_dir(control_state_dir),
        bind_host,
        expected_project_root,
    )
}

/// Resolve the best project-matching sidecar descriptor for hook routing.
pub fn read_sidecar_endpoint(
    control_state_dir: &ControlStateDir,
    bind_host: &str,
    expected_project_root: Option<&Path>,
) -> io::Result<(u16, Option<String>)> {
    let dir = resolve_symforge_dir(control_state_dir);
    if let Some(selected) = select_descriptor_status(&dir, bind_host, expected_project_root) {
        if let Some(port) = selected.status.port {
            return Ok((port, selected.session_id));
        }
        // Fail closed: modern descriptors EXISTED but were all
        // identity-rejected for this root. Falling through to the legacy
        // fixed files would route around the project-root check — a
        // fail-closed mismatch must never degrade into an unscoped lookup.
        if let Some(detail) = selected.status.detail.as_deref() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                format!("sidecar descriptor identity mismatch: {detail}"),
            ));
        }
    }

    if expected_project_root.is_some() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "root-scoped sidecar lookup refused legacy fixed files without project identity",
        ));
    }

    let port = read_port_at(&dir)?;
    let session_id = read_runtime_file(&dir, &session_file_name(), LEGACY_SESSION_FILE)
        .ok()
        .map(|contents| contents.trim().to_string());
    Ok((port, session_id))
}

/// Remove both port and PID files. Ignores all errors.
///
/// Called during sidecar shutdown — it is safe to call even if files don't exist.
pub fn cleanup_files(control_state_dir: &ControlStateDir) {
    cleanup_files_at(&resolve_symforge_dir(control_state_dir));
}

pub fn cleanup_stale_descriptors(control_state_dir: &ControlStateDir, bind_host: &str) {
    cleanup_stale_descriptors_at(&resolve_symforge_dir(control_state_dir), bind_host);
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
pub fn check_stale(
    control_state_dir: &ControlStateDir,
    bind_host: &str,
    expected_project_root: Option<&Path>,
) -> bool {
    match read_sidecar_status(control_state_dir, bind_host, expected_project_root).liveness {
        SidecarLiveness::Alive | SidecarLiveness::NoSidecar => false,
        SidecarLiveness::Dead => {
            cleanup_stale_descriptors(control_state_dir, bind_host);
            cleanup_files(control_state_dir);
            true
        }
        SidecarLiveness::Unknown => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn with_temp_control(f: impl FnOnce(&ControlStateDir, &Path)) {
        let tmp = TempDir::new().unwrap();
        let control = ControlStateDir::new(tmp.path().join("control"));
        let sidecar_dir = resolve_symforge_dir(&control);
        f(&control, &sidecar_dir);
    }

    #[test]
    fn same_root_identity_strips_windows_verbatim_prefix() {
        let plain = r"C:\AI_STUFF\PROGRAMMING\symforge";
        assert!(
            canon_root_identity_for_platform(r"\\?\C:\AI_STUFF\PROGRAMMING\symforge", true,)
                == canon_root_identity_for_platform(plain, true),
            "backslash verbatim prefix must match the plain root"
        );
        assert!(
            canon_root_identity_for_platform("//?/C:/AI_STUFF/PROGRAMMING/symforge", true,)
                == canon_root_identity_for_platform("C:/AI_STUFF/PROGRAMMING/symforge", true,),
            "slash-unified verbatim prefix (health project_root form) must match"
        );
        assert!(
            canon_root_identity_for_platform(r"\\?\C:\AI_STUFF\PROGRAMMING\symforge", true,)
                == canon_root_identity_for_platform("C:/AI_STUFF/PROGRAMMING/symforge", true,),
            "mixed separators after stripping the prefix must match"
        );
        assert!(
            canon_root_identity_for_platform(r"\\?\C:\other", true)
                != canon_root_identity_for_platform(plain, true),
            "verbatim prefix must not collapse distinct roots"
        );
    }

    #[test]
    fn sidecar_root_identity_preserves_literal_backslash_on_unix_policy() {
        let literal = "/work/a\\b";
        let nested = "/work/a/b";

        assert_ne!(
            canon_root_identity_for_platform(literal, false),
            canon_root_identity_for_platform(nested, false),
            "distinct Unix roots must not admit the same sidecar descriptor"
        );
        assert_eq!(
            canon_root_identity_for_platform(literal, true),
            canon_root_identity_for_platform(nested, true),
            "Windows separator compatibility must remain intact"
        );
    }

    #[cfg(unix)]
    #[test]
    fn descriptor_authority_refuses_non_utf8_project_roots() {
        use std::os::unix::ffi::OsStringExt;

        let control = tempfile::tempdir().expect("control dir");
        let native_root = PathBuf::from(std::ffi::OsString::from_vec(vec![b'a', 0xff, b'b']));
        let error =
            write_descriptor_for_pid_at(control.path(), 42, 31337, None, Some(&native_root), None)
                .expect_err("an opaque native root cannot be serialized as authority");
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);

        let selected = select_descriptor_status(control.path(), "127.0.0.1", Some(&native_root))
            .expect("non-UTF-8 selection must block legacy fallback");
        assert_eq!(selected.status.liveness, SidecarLiveness::NoSidecar);
        assert!(
            selected
                .status
                .detail
                .as_deref()
                .is_some_and(|detail| detail.contains("not valid UTF-8"))
        );
    }

    #[test]
    fn root_scoped_lookup_rejects_legacy_descriptor_without_project_id() {
        let control = tempfile::tempdir().expect("control dir");
        let project = tempfile::tempdir().expect("project root");
        let sessions = control.path().join(SESSIONS_DIR);
        std::fs::create_dir_all(&sessions).expect("sessions dir");
        let descriptor = SessionDescriptor {
            session_id: None,
            project_root: Some(project.path().display().to_string()),
            project_id: None,
            daemon_started_at: None,
            pid: std::process::id(),
            port: 31337,
            updated_at_unix_secs: now_unix_secs(),
        };
        std::fs::write(
            sessions.join(descriptor_file_name(descriptor.pid)),
            serde_json::to_vec(&descriptor).expect("serialize legacy descriptor"),
        )
        .expect("write legacy descriptor");

        let selected = select_descriptor_status(control.path(), "127.0.0.1", Some(project.path()))
            .expect("the rejected descriptor must block legacy fallback");
        assert_eq!(selected.status.liveness, SidecarLiveness::NoSidecar);
        assert!(selected.status.port.is_none());
        assert!(
            selected
                .status
                .detail
                .as_deref()
                .is_some_and(|detail| detail.contains("identity mismatch"))
        );
    }

    #[test]
    fn root_scoped_lookup_never_uses_unscoped_legacy_fixed_files() {
        let control = tempfile::tempdir().expect("control dir");
        let project = tempfile::tempdir().expect("project root");
        std::fs::write(control.path().join(port_file_name()), "31337").expect("legacy port file");
        std::fs::write(
            control.path().join(pid_file_name()),
            std::process::id().to_string(),
        )
        .expect("legacy pid file");

        let status =
            read_sidecar_status_for_root_at(control.path(), "127.0.0.1", Some(project.path()));
        assert_eq!(status.liveness, SidecarLiveness::NoSidecar);
        assert!(status.port.is_none());
        assert!(
            status
                .detail
                .as_deref()
                .is_some_and(|detail| detail.contains("legacy fixed files"))
        );
    }

    /// Task 8 (recovered finding): closing one adapter must not delete or
    /// invalidate a sibling adapter's runtime record on the same root.
    #[test]
    fn test_per_session_descriptors_do_not_delete_siblings() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();
        {
            write_descriptor_for_pid_at(dir, 111, 40001, Some("session-a"), None, None)
                .expect("write descriptor A");
            write_descriptor_for_pid_at(dir, 222, 40002, Some("session-b"), None, None)
                .expect("write descriptor B");
            assert_eq!(read_descriptors_at(dir).len(), 2, "both descriptors exist");

            cleanup_descriptor_for_pid_at(dir, 111);

            let remaining = read_descriptors_at(dir);
            assert_eq!(
                remaining.len(),
                1,
                "only the caller's descriptor is removed"
            );
            assert_eq!(remaining[0].pid, 222, "sibling descriptor survives");
            assert_eq!(remaining[0].session_id.as_deref(), Some("session-b"));
        }
    }

    #[test]
    fn test_reader_removes_dead_pid_descriptors_before_socket_probe() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();
        let listener =
            std::net::TcpListener::bind("127.0.0.1:0").expect("bind recycled probe port");
        listener
            .set_nonblocking(true)
            .expect("make recycled probe listener nonblocking");
        let recycled_port = listener.local_addr().expect("recycled probe addr").port();

        for offset in 0..200 {
            write_descriptor_for_pid_at(
                dir,
                u32::MAX - offset,
                recycled_port,
                Some("stale-session"),
                None,
                None,
            )
            .expect("write dead-pid descriptor");
        }

        let scan_started = Instant::now();
        let selected = select_descriptor_status(dir, "127.0.0.1", None);
        let scan_elapsed = scan_started.elapsed();
        assert!(
            selected.is_none(),
            "a reachable recycled port must not revive dead-pid descriptors"
        );
        assert!(
            scan_elapsed < Duration::from_millis(300),
            "200 dead-pid descriptors must remain inside the hook budget: {scan_elapsed:?}"
        );
        assert!(
            read_descriptors_at(dir).is_empty(),
            "dead-pid descriptors must be removed opportunistically"
        );
        let accept_error = listener
            .accept()
            .expect_err("dead-pid descriptors must be rejected before any socket probe");
        assert_eq!(accept_error.kind(), io::ErrorKind::WouldBlock);
    }

    /// Task 8: the reader selects a LIVE descriptor over a fresher dead one,
    /// and rejects a descriptor whose project-root identity does not match
    /// this directory's project instead of choosing last-writer.
    #[test]
    fn test_reader_selects_live_descriptor_and_rejects_foreign_root() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();
        let project_root = dir.parent().expect("temporary control dir has a parent");
        {
            // A live port: keep the listener open for the duration.
            let listener =
                std::net::TcpListener::bind("127.0.0.1:0").expect("bind live probe port");
            let live_port = listener.local_addr().expect("live addr").port();
            // A dead port: bind then release.
            let dead_port = {
                let l = std::net::TcpListener::bind("127.0.0.1:0").expect("bind dead port");
                l.local_addr().expect("dead addr").port()
            };
            let live_pid = std::process::id();
            let dead_pid = u32::MAX;

            write_descriptor_for_pid_at(dir, live_pid, live_port, None, Some(project_root), None)
                .expect("write live descriptor");
            write_descriptor_for_pid_at(dir, dead_pid, dead_port, None, Some(project_root), None)
                .expect("write dead descriptor");
            // The dead one is FRESHER (rewrite bumps updated_at); force order by
            // rewriting it after the live one.
            std::thread::sleep(std::time::Duration::from_millis(1100));
            write_descriptor_for_pid_at(dir, dead_pid, dead_port, None, Some(project_root), None)
                .expect("refresh dead descriptor");

            let status = read_sidecar_status_at(dir, "127.0.0.1");
            assert_eq!(status.liveness, SidecarLiveness::Alive);
            assert_eq!(
                status.pid,
                Some(live_pid),
                "live beats fresher-but-dead: {status:?}"
            );
            assert_eq!(status.port, Some(live_port));

            // Identity validation: a descriptor claiming a DIFFERENT project
            // root is rejected, not last-writer-selected.
            cleanup_descriptor_for_pid_at(dir, live_pid);
            cleanup_descriptor_for_pid_at(dir, dead_pid);
            write_descriptor_for_pid_at(
                dir,
                live_pid,
                live_port,
                None,
                Some(std::path::Path::new("/somewhere/else/entirely")),
                None,
            )
            .expect("write foreign descriptor");
            let status = read_sidecar_status_at(dir, "127.0.0.1");
            assert_ne!(
                status.pid,
                Some(live_pid),
                "foreign-root descriptor must be identity-rejected: {status:?}"
            );
            drop(listener);
        }
    }

    /// Boot-epoch probe (session-aliasing incident): a daemon-backed descriptor
    /// is only selectable when the port's /health proves the SAME daemon
    /// process wrote it. After a daemon restart the new process re-issues
    /// `session-N` from 1 — epoch equality is what stops a stale descriptor
    /// from aliasing an unrelated session.
    #[test]
    fn test_daemon_backed_descriptor_requires_matching_boot_epoch() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();
        let project_root = dir.parent().expect("temporary control dir has a parent");

        // Minimal HTTP fake: fixed 200 + DaemonHealth JSON body.
        fn spawn_health_daemon(epoch: Option<u64>) -> u16 {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
            let port = listener.local_addr().expect("addr").port();
            std::thread::spawn(move || {
                while let Ok((mut stream, _)) = listener.accept() {
                    use std::io::{Read, Write};
                    let mut req = [0u8; 512];
                    let _ = stream.read(&mut req);
                    let health = crate::daemon::DaemonHealth {
                        project_count: 0,
                        session_count: 0,
                        daemon_version: "10.1.0".to_string(),
                        executable_path: "x".to_string(),
                        auth_required: true,
                        pid: Some(std::process::id()),
                        started_at_unix_secs: epoch,
                    };
                    let body = serde_json::to_string(&health).expect("health json");
                    let response = format!(
                        "HTTP/1.1 200 OK\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = stream.write_all(response.as_bytes());
                }
            });
            port
        }

        let port = spawn_health_daemon(Some(1_700_000_000));
        let pid = std::process::id();

        // Matching epoch: selected.
        write_descriptor_for_pid_at(
            dir,
            pid,
            port,
            Some("session-7"),
            Some(project_root),
            Some(1_700_000_000),
        )
        .expect("write matching descriptor");
        let status = read_sidecar_status_at(dir, "127.0.0.1");
        assert_eq!(status.liveness, SidecarLiveness::Alive);
        assert_eq!(status.port, Some(port), "matching epoch must select");

        // Stale epoch (descriptor written against the PREVIOUS daemon boot):
        // rejected even though the port is alive and serves 200.
        write_descriptor_for_pid_at(
            dir,
            pid,
            port,
            Some("session-7"),
            Some(project_root),
            Some(1_600_000_000),
        )
        .expect("write stale-epoch descriptor");
        let status = read_sidecar_status_at(dir, "127.0.0.1");
        assert_ne!(
            status.liveness,
            SidecarLiveness::Alive,
            "stale epoch must not select: {status:?}"
        );
        assert!(
            status
                .detail
                .as_deref()
                .unwrap_or("")
                .contains("epoch-rejected"),
            "rejection must be attributed to the epoch probe: {status:?}"
        );

        // Legacy descriptor (no epoch) against a NEW daemon (epoch present):
        // rejected — this is the exact aliasing case from the incident.
        write_descriptor_for_pid_at(dir, pid, port, Some("session-7"), Some(project_root), None)
            .expect("write legacy descriptor");
        let status = read_sidecar_status_at(dir, "127.0.0.1");
        assert_ne!(
            status.liveness,
            SidecarLiveness::Alive,
            "legacy descriptor must not alias a restarted daemon: {status:?}"
        );
    }

    #[test]
    fn test_write_read_port_roundtrip() {
        with_temp_control(|control, _| {
            write_port_file(12345, control).expect("write_port_file should succeed");
            let port = read_port(control).expect("read_port should succeed after write");
            assert_eq!(port, 12345, "port roundtrip must preserve value");
        });
    }

    #[test]
    fn test_write_port_file_no_trailing_newline() {
        with_temp_control(|control, dir| {
            write_port_file(8080, control).expect("write_port_file should succeed");
            let port_path = dir.join(port_file_name());
            let bytes = std::fs::read(&port_path).unwrap();
            assert_eq!(
                bytes, b"8080",
                "port file must contain ONLY the digits, no newline"
            );
        });
    }

    #[test]
    fn test_write_is_os_tagged_only() {
        with_temp_control(|control, dir| {
            write_port_file(8080, control).expect("write_port_file should succeed");
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
        with_temp_control(|control, _| {
            // Simulate a sidecar started by an OLD (pre-tag) binary.
            let dir = ensure_symforge_dir(control).expect("dir");
            std::fs::write(dir.join(LEGACY_PORT_FILE), b"7777").unwrap();
            let port = read_port(control).expect("read_port must fall back to legacy file");
            assert_eq!(port, 7777, "legacy fallback must read the un-tagged port");
        });
    }

    #[test]
    fn test_tagged_wins_over_legacy() {
        with_temp_control(|control, _| {
            let dir = ensure_symforge_dir(control).expect("dir");
            std::fs::write(dir.join(LEGACY_PORT_FILE), b"1111").unwrap();
            std::fs::write(dir.join(port_file_name()), b"2222").unwrap();
            let port = read_port(control).expect("read_port should succeed");
            assert_eq!(
                port, 2222,
                "OS-tagged file must take precedence over legacy"
            );
        });
    }

    #[test]
    fn test_cleanup_removes_files() {
        with_temp_control(|control, dir| {
            write_port_file(9000, control).expect("write should succeed");
            write_pid_file(12345, control).expect("write should succeed");
            // Also drop legacy files to prove cleanup removes BOTH.
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

            cleanup_files(control);

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
        with_temp_control(|control, _| {
            // Should not panic even if files don't exist.
            cleanup_files(control);
        });
    }

    #[test]
    fn test_read_port_missing_returns_error() {
        with_temp_control(|control, _| {
            let result = read_port(control);
            assert!(
                result.is_err(),
                "read_port should return error when file is missing"
            );
        });
    }

    #[test]
    fn test_ensure_symforge_dir_creates_directory() {
        with_temp_control(|control, _| {
            let dir = ensure_symforge_dir(control).expect("ensure_symforge_dir should succeed");
            assert!(
                dir.exists(),
                ".symforge directory should exist after ensure_symforge_dir"
            );
            assert!(dir.is_dir(), "path should be a directory");
        });
    }

    #[test]
    fn test_ensure_symforge_dir_idempotent() {
        with_temp_control(|control, _| {
            ensure_symforge_dir(control).expect("first call should succeed");
            ensure_symforge_dir(control).expect("second call should also succeed (idempotent)");
        });
    }
    #[test]
    fn test_check_stale_returns_false_when_no_port_file() {
        with_temp_control(|control, _| {
            let is_stale = check_stale(control, "127.0.0.1", None);
            assert!(!is_stale, "no port file means nothing is stale");
        });
    }

    #[test]
    fn test_check_stale_cleans_up_when_port_is_closed() {
        with_temp_control(|control, dir| {
            // Write a port that is very unlikely to have anything listening.
            write_port_file(19999, control).expect("write should succeed");
            write_pid_file(99999, control).expect("write should succeed");

            let is_stale = check_stale(control, "127.0.0.1", None);
            assert!(
                is_stale,
                "port 19999 should be detected as stale (nothing listening)"
            );

            // Cleanup should have been called.
            assert!(
                !dir.join(port_file_name()).exists(),
                "port file cleaned up after stale detection"
            );
        });
    }
}
