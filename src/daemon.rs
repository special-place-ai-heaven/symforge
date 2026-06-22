//! Shared local daemon for project-aware and session-aware backend state.

use parking_lot::{Mutex, RwLock};
use std::collections::{HashMap, HashSet};
use std::io;
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

use anyhow::Context;
use axum::extract::{Path as AxumPath, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use rmcp::handler::server::wrapper::Parameters;
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;

use crate::live_index::view::{BaseKey, CommitId, IndexBase, WorkingSet};
use crate::live_index::{self, SharedIndex};
use crate::paths;
use crate::protocol::SymForgeServer;
use crate::protocol::edit::{
    BatchEditInput, BatchInsertInput, BatchRenameInput, DeleteSymbolInput, EditWithinSymbolInput,
    InsertSymbolInput, ReplaceSymbolBodyInput,
};
use crate::protocol::read_tools::SymforgeRetrieveInput;
use crate::protocol::tools::{
    AnalyzeFileImpactInput, CheckpointNowInput, DiffSymbolsInput, EditPlanInput, ExploreInput,
    FindDependentsInput, FindReferencesInput, GetFileContentInput, GetFileContextInput,
    GetRepoMapInput, GetSymbolContextInput, GetSymbolInput, HealthInput, IndexFolderInput,
    InspectMatchInput, InvestigationInput, SearchFilesInput, SearchSymbolsInput, SearchTextInput,
    SmartQueryInput, TraceSymbolInput, ValidateFileSyntaxInput, WhatChangedInput,
};
use crate::sidecar::{SidecarState, SymbolSnapshot, TokenStats};
use crate::watcher::{self, WatcherInfo};

// Daemon runtime files are OS-tagged (daemon.<os>.port) so a Windows daemon and a
// WSL/Linux daemon never collide — even when SYMFORGE_HOME is deliberately pointed at
// a shared mount. Without the tag, isolation would rely only on dirs::home_dir()
// landing on different filesystems per OS, which a shared SYMFORGE_HOME defeats.
// The tag is the same compile-time std::env::consts::OS the sidecar uses, so all
// in-crate readers/writers agree by construction. Legacy un-tagged files are read as
// a fallback for one release so an upgrade does not strand a running daemon.
const LEGACY_DAEMON_PORT_FILE: &str = "daemon.port";
const LEGACY_DAEMON_PID_FILE: &str = "daemon.pid";
// Lock files are transient and never read across the upgrade boundary, so the legacy
// name is only needed by the test-only cleanup helper and the test-only assertions
// that write/read the legacy un-tagged names.
#[cfg(test)]
const LEGACY_DAEMON_START_LOCK_FILE: &str = "daemon.starting";

fn daemon_port_file_name() -> String {
    crate::paths::os_tagged_runtime_file_name("daemon", "port")
}
fn daemon_pid_file_name() -> String {
    crate::paths::os_tagged_runtime_file_name("daemon", "pid")
}
fn daemon_start_lock_file_name() -> String {
    crate::paths::os_tagged_runtime_file_name("daemon", "starting")
}
/// OS-tagged auth-token file. Sits alongside the port/pid files in `daemon_dir()`
/// and carries the bearer token the daemon requires on every authenticated
/// route. It is the connection-info channel the legit MCP front-end and hook
/// read when the operator has NOT pinned a token via env — closing the prior
/// "no token ⇒ open daemon" ambient-authority hole without breaking the
/// handshake. On Unix it is created `0o600` (owner read/write only).
fn daemon_token_file_name() -> String {
    crate::paths::os_tagged_runtime_file_name("daemon", "token")
}

/// Read a daemon runtime file under `dir`, OS-tagged name first then legacy.
fn read_daemon_runtime(dir: &std::path::Path, tagged: &str, legacy: &str) -> io::Result<String> {
    match std::fs::read_to_string(dir.join(tagged)) {
        Ok(c) => Ok(c),
        Err(e) if e.kind() == io::ErrorKind::NotFound => std::fs::read_to_string(dir.join(legacy)),
        Err(e) => Err(e),
    }
}
const DAEMON_BIND_ENV: &str = "SYMFORGE_DAEMON_BIND";
const DAEMON_ALLOW_NON_LOOPBACK_ENV: &str = "SYMFORGE_DAEMON_ALLOW_NON_LOOPBACK";
const DAEMON_AUTH_TOKEN_ENV: &str = "SYMFORGE_DAEMON_AUTH_TOKEN";
const TRACE_SYMBOL_ALIAS_DEPRECATION: &str = concat!(
    "Deprecation warning: `trace_symbol` is retired; ",
    "use `get_symbol_context` with `sections=[...]` or `find_references` instead. ",
    "Compatibility policy: keep daemon alias through v7.x; planned removal in v8.0."
);

pub type SharedDaemonState = Arc<DaemonState>;

pub struct DaemonHandle {
    pub port: u16,
    pub shutdown_tx: tokio::sync::oneshot::Sender<()>,
    pub state: SharedDaemonState,
    server_task: tokio::task::JoinHandle<()>,
}

#[derive(Clone)]
pub struct DaemonSessionClient {
    http_client: reqwest::Client,
    base_url: String,
    project_id: String,
    session_id: String,
    project_name: String,
    auth_token: Option<String>,
    /// Stored so reconnection can re-open a session at the same project root.
    project_root: Option<PathBuf>,
}

pub struct DaemonState {
    next_session_id: AtomicU64,
    // LOCK ORDER (Feature 012): `bases` -> `projects` -> `sessions`.
    // Acquire only DOWNWARD; never acquire an earlier lock while holding a later
    // one. The pre-012 rule was `projects` -> `sessions` (see `close_session`,
    // `index_folder_for_session`, `session_runtime`); 012 prepends `bases` at the
    // top because base interning (`intern_base`) may run just before a project
    // insert. `intern_base` itself acquires ONLY `bases` and touches no other
    // lock, so call sites stay free to order the rest. Per-session `working_set`
    // is an `Arc<RwLock<WorkingSet>>` OUTSIDE this hierarchy (its own lock, never
    // held across a daemon-map lock), so it cannot participate in an inversion.
    /// Per-daemon base intern table (Feature 012). Equal `(canonical_root,
    /// commit)` keys MUST share ONE `Arc<IndexBase>` (SC-002): `intern_base`
    /// returns the existing Arc on a key hit and only mints a new base (drawing a
    /// generation from `base_generation_seq`) on a miss. Phase 0/1: populated on
    /// project load/activate and seeded into per-session working sets, but no
    /// query route reads it yet (cross-project routing is Phase 2/3).
    bases: RwLock<HashMap<BaseKey, Arc<IndexBase>>>,
    /// Monotonic source of `base_generation` fence tokens, minted only when
    /// `intern_base` publishes a NEW base. Starts at 1; a shared (interned) base
    /// keeps its original generation, so this is strictly increasing across
    /// distinct published bases (the D2 fence the engine asserts on).
    base_generation_seq: AtomicU64,
    projects: RwLock<HashMap<String, ProjectInstance>>,
    sessions: RwLock<HashMap<String, SessionRecord>>,
    identity: DaemonIdentity,
    /// The bearer token this daemon requires on every authenticated route.
    /// Non-optional by design: the daemon ALWAYS establishes a token at startup
    /// (env pin or freshly generated), so `authorize_daemon_request` is strictly
    /// fail-closed — there is no "no token ⇒ allow" path. Making this a `String`
    /// (not `Option`) encodes the fail-closed invariant in the type system.
    auth_token: String,
    /// Concurrency governor — limits parallel tool calls and enforces timeouts.
    governor: crate::sidecar::governor::RequestGovernor,
}

/// Tracks whether a ProjectInstance has been fully activated (watcher + git temporal started).
/// Prevents two racing opens from both activating the same project.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ActivationState {
    /// Freshly constructed, no background tasks started. Safe to discard.
    Inactive,
    /// Activation in progress (watcher + git temporal being started).
    Activating,
    /// Fully active with watcher and git temporal running.
    Active,
}

struct ProjectInstance {
    project_id: String,
    canonical_root: PathBuf,
    project_name: String,
    index: SharedIndex,
    watcher_info: Arc<Mutex<WatcherInfo>>,
    watcher_task: Option<tokio::task::JoinHandle<()>>,
    stop_token: Arc<AtomicBool>,
    token_stats: Arc<TokenStats>,
    symbol_cache: Arc<RwLock<HashMap<String, Vec<SymbolSnapshot>>>>,
    session_ids: HashSet<String>,
    opened_at: SystemTime,
    activation_state: ActivationState,
    /// Cached `SymForgeServer` instance — constructed once per project and
    /// reused for all tool calls, avoiding per-call router/prompt table allocation.
    server: SymForgeServer,
}

struct SessionRecord {
    session_id: String,
    /// The single active project this session resolves to (Feature 012: renamed
    /// from `project_id`). `session_runtime` resolves the one active project via
    /// this id, so single-project behavior is unchanged. Phases 2/3 will let a
    /// session hold MULTIPLE projects in `working_set` with this field naming the
    /// active/bound one; for Phase 0/1 it is the sole project, byte-for-byte the
    /// prior `project_id`.
    active_project_id: String,
    /// Per-session copy-on-write working set (Feature 012, Phase 1). Seeded on
    /// open with a SINGLE entry — the active project + its interned shared base +
    /// an EMPTY overlay — and INERT: no query route reads it yet (Phase 2/3) and
    /// NO code path writes into its overlay (the no-overlay-writes invariant that
    /// keeps Principle I airtight). `Arc<RwLock>` is mandatory: `SessionRecord` is
    /// cloned on every `session_runtime` call and `WorkingSet: Clone` deep-clones
    /// overlays, so the `Arc` keeps the clone O(1) and the overlay state singular.
    working_set: Arc<RwLock<WorkingSet>>,
    client_name: String,
    pid: Option<u32>,
    opened_at: SystemTime,
    /// Epoch millis stored atomically so heartbeats only need a read lock.
    last_seen_at: AtomicU64,
}

impl Clone for SessionRecord {
    fn clone(&self) -> Self {
        Self {
            session_id: self.session_id.clone(),
            active_project_id: self.active_project_id.clone(),
            // Share the SAME working-set lock across clones (O(1), singular state).
            working_set: Arc::clone(&self.working_set),
            client_name: self.client_name.clone(),
            pid: self.pid,
            opened_at: self.opened_at,
            last_seen_at: AtomicU64::new(self.last_seen_at.load(Ordering::Relaxed)),
        }
    }
}

impl SessionRecord {
    /// Convert the atomic epoch-millis value back to a [`SystemTime`].
    fn last_seen_at_time(&self) -> SystemTime {
        let millis = self.last_seen_at.load(Ordering::Relaxed);
        SystemTime::UNIX_EPOCH + Duration::from_millis(millis)
    }
}

/// Current time as milliseconds since the Unix epoch.
fn now_epoch_millis() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn daemon_auth_token_from_env() -> Option<String> {
    std::env::var(DAEMON_AUTH_TOKEN_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

/// Read the persisted daemon auth token from the OS-tagged token file in
/// `daemon_dir()`. Returns `None` if the file is absent, empty, or unreadable.
/// This is how a front-end / hook process that did NOT spawn the daemon — and
/// therefore has no `SYMFORGE_DAEMON_AUTH_TOKEN` in its env — still learns the
/// running daemon's token and authenticates.
fn daemon_auth_token_from_file() -> Option<String> {
    let dir = daemon_dir().ok()?;
    let contents = std::fs::read_to_string(dir.join(daemon_token_file_name())).ok()?;
    let trimmed = contents.trim().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

/// Resolve the daemon auth token a CLIENT (front-end proxy, hook) must present.
///
/// Resolution order:
/// 1. `SYMFORGE_DAEMON_AUTH_TOKEN` env var — explicit operator pin / test
///    override, and what a parent that spawned the daemon shares with it.
/// 2. The persisted token file written by the daemon at startup.
///
/// Returns `None` only when neither source is available; with the fail-closed
/// daemon this means the request will be rejected, which is the safe outcome.
pub(crate) fn resolve_daemon_auth_token() -> Option<String> {
    daemon_auth_token_from_env().or_else(daemon_auth_token_from_file)
}

/// Generate an unpredictable 64-hex-character daemon auth token.
///
/// Prefers the OS CSPRNG via `/dev/urandom` on Unix (a plain file read — no
/// `unsafe`, no extra dependency). When that is unavailable (Windows, or a
/// sandbox without `/dev/urandom`), it falls back to a SHA-256 digest over
/// several process-local entropy sources that a local peer cannot observe or
/// reproduce: high-resolution timestamps, the process id, a heap-allocation
/// address (ASLR), the current thread id, and a per-call atomic counter.
///
/// The token's real confidentiality barrier is the `0o600` token FILE; the
/// token only needs to be non-guessable by a local process that cannot read
/// that file. Both paths satisfy that.
fn generate_daemon_auth_token() -> String {
    #[cfg(unix)]
    {
        use std::io::Read;
        if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
            let mut buf = [0u8; 32];
            if f.read_exact(&mut buf).is_ok() {
                return crate::hash::digest_hex(&buf);
            }
        }
    }

    // Portable fallback: hash a mix of process-local, non-deterministic sources.
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);

    let mut seed: Vec<u8> = Vec::with_capacity(64);
    let now_nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    seed.extend_from_slice(&now_nanos.to_le_bytes());
    let mono = std::time::Instant::now();
    seed.extend_from_slice(&format!("{mono:?}").into_bytes());
    seed.extend_from_slice(&std::process::id().to_le_bytes());
    seed.extend_from_slice(&COUNTER.fetch_add(1, Ordering::Relaxed).to_le_bytes());
    // Heap-allocation address carries ASLR entropy that is not observable to a
    // peer process without the token file.
    let boxed = Box::new(0u8);
    let addr = (&*boxed as *const u8) as usize;
    seed.extend_from_slice(&addr.to_le_bytes());
    seed.extend_from_slice(format!("{:?}", std::thread::current().id()).as_bytes());

    crate::hash::digest_hex(&seed)
}

/// Establish the token the DAEMON will require: an explicit env pin if present,
/// otherwise a freshly generated random token. Always returns `Some` — the
/// daemon never starts without a token, so `authorize_daemon_request` can be
/// strictly fail-closed.
fn establish_daemon_auth_token() -> String {
    daemon_auth_token_from_env().unwrap_or_else(generate_daemon_auth_token)
}

/// Persist the daemon auth token to its OS-tagged file with owner-only
/// permissions on Unix. The token is written before the daemon starts serving
/// so a client that observes the port file can also read a valid token.
fn write_daemon_token_file(token: &str) -> io::Result<()> {
    let path = daemon_dir()?.join(daemon_token_file_name());

    // On Unix, create the file with 0o600 from the start so the secret is never
    // briefly world-readable between create and chmod.
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        use std::os::unix::fs::PermissionsExt;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&path)
            .map_err(|e| {
                io::Error::new(
                    e.kind(),
                    format!("writing daemon token file at {}: {}", path.display(), e),
                )
            })?;
        file.write_all(token.as_bytes()).map_err(|e| {
            io::Error::new(
                e.kind(),
                format!("writing daemon token file at {}: {}", path.display(), e),
            )
        })?;
        // Best-effort re-assert mode in case a restrictive umask still widened it
        // (OpenOptions mode is ANDed with the complement of umask).
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
        Ok(())
    }

    #[cfg(not(unix))]
    {
        std::fs::write(&path, token.as_bytes()).map_err(|e| {
            io::Error::new(
                e.kind(),
                format!("writing daemon token file at {}: {}", path.display(), e),
            )
        })
    }
}

fn env_var_truthy(name: &str) -> bool {
    std::env::var(name)
        .ok()
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

fn is_loopback_bind_host(host: &str) -> bool {
    let trimmed = host.trim().trim_start_matches('[').trim_end_matches(']');
    trimmed.eq_ignore_ascii_case("localhost")
        || trimmed
            .parse::<IpAddr>()
            .map(|ip| ip.is_loopback())
            .unwrap_or(false)
}

fn resolve_daemon_bind_host(default_host: &str) -> anyhow::Result<String> {
    let configured = std::env::var(DAEMON_BIND_ENV).unwrap_or_else(|_| default_host.to_string());
    let host = configured.trim();
    if host.is_empty() {
        anyhow::bail!("{DAEMON_BIND_ENV} must not be empty");
    }

    if is_loopback_bind_host(host) {
        return Ok(host.to_string());
    }

    if env_var_truthy(DAEMON_ALLOW_NON_LOOPBACK_ENV) {
        tracing::warn!(
            bind_host = %host,
            allow_env = DAEMON_ALLOW_NON_LOOPBACK_ENV,
            "daemon binding to a non-loopback interface after explicit operator opt-in"
        );
        Ok(host.to_string())
    } else {
        anyhow::bail!(
            "refusing non-loopback daemon bind '{host}'; set {DAEMON_ALLOW_NON_LOOPBACK_ENV}=1 to allow it explicitly"
        );
    }
}

fn daemon_socket_bind_address(host: &str) -> String {
    let host = host.trim();
    if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]:0")
    } else {
        format!("{host}:0")
    }
}

fn apply_daemon_auth_header(
    request: reqwest::RequestBuilder,
    auth_token: Option<&str>,
) -> reqwest::RequestBuilder {
    if let Some(token) = auth_token {
        request.bearer_auth(token)
    } else {
        request
    }
}

/// Constant-time byte-slice equality.
///
/// Returns `false` on length mismatch and otherwise XOR-folds every byte pair
/// into an accumulator, returning `acc == 0`. There is no early return on the
/// first differing byte, so the running time depends only on the input length,
/// not on the position of the first mismatch. This denies the byte-by-byte
/// timing oracle that `==` on `&[u8]`/`&str` exposes (it short-circuits).
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut acc: u8 = 0;
    for (lhs, rhs) in a.iter().zip(b.iter()) {
        acc |= lhs ^ rhs;
    }
    acc == 0
}

fn authorize_daemon_request(state: &DaemonState, headers: &HeaderMap) -> Result<(), StatusCode> {
    // Fail-closed: the daemon always holds a token (see `DaemonState.auth_token`),
    // so there is no unauthenticated-allow path. A local process that cannot read
    // the 0o600 token file (and has no env pin) can no longer drive the daemon.
    let expected_token = state.auth_token.as_str();

    let Some(header_value) = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
    else {
        return Err(StatusCode::UNAUTHORIZED);
    };

    let Some((scheme, provided_token)) = header_value.split_once(' ') else {
        return Err(StatusCode::UNAUTHORIZED);
    };

    if !scheme.eq_ignore_ascii_case("bearer") {
        return Err(StatusCode::UNAUTHORIZED);
    }

    // Compare in constant time. We hash BOTH sides to a fixed 32-byte SHA-256
    // digest first so the comparison width is constant regardless of the
    // presented token's length: this leaks neither the real token's length nor
    // the byte position of the first mismatch. A wrong/empty token still 401s,
    // preserving the fail-closed contract.
    let expected_digest = crate::hash::digest(expected_token.as_bytes());
    let provided_digest = crate::hash::digest(provided_token.as_bytes());
    if constant_time_eq(&expected_digest, &provided_digest) {
        Ok(())
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}

fn daemon_auth_error(status: StatusCode) -> (StatusCode, String) {
    (status, "daemon authentication required".to_string())
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OpenProjectRequest {
    pub project_root: String,
    pub client_name: String,
    pub pid: Option<u32>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct OpenProjectResponse {
    pub project_id: String,
    pub session_id: String,
    pub project_name: String,
    pub canonical_root: String,
    pub session_count: usize,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct CloseSessionResponse {
    pub session_id: String,
    pub project_id: String,
    pub remaining_sessions: usize,
    pub project_removed: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct HeartbeatResponse {
    pub session_id: String,
    pub known_session: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct ProjectSummary {
    pub project_id: String,
    pub project_name: String,
    pub canonical_root: String,
    pub session_count: usize,
    pub opened_at_unix_secs: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct SessionSummary {
    pub session_id: String,
    pub project_id: String,
    pub client_name: String,
    pub pid: Option<u32>,
    pub opened_at_unix_secs: u64,
    pub last_seen_at_unix_secs: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct ProjectHealth {
    pub project_id: String,
    pub project_name: String,
    pub canonical_root: String,
    pub session_count: usize,
    pub file_count: usize,
    pub symbol_count: usize,
    pub index_state: String,
    pub opened_at_unix_secs: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct DaemonHealth {
    pub project_count: usize,
    pub session_count: usize,
    pub daemon_version: String,
    pub executable_path: String,
    #[serde(default)]
    pub auth_required: bool,
    #[serde(default)]
    pub pid: Option<u32>,
}

#[derive(Clone)]
struct SessionRuntime {
    canonical_root: PathBuf,
    project_id: String,
    session_id: String,
    index: SharedIndex,
    token_stats: Arc<TokenStats>,
    symbol_cache: Arc<RwLock<HashMap<String, Vec<SymbolSnapshot>>>>,
    /// Cached `SymForgeServer` clone — cheap because all inner state is `Arc`-wrapped.
    server: SymForgeServer,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DaemonIdentity {
    version: String,
    executable_path: String,
}

impl DaemonState {
    /// Construct daemon state with an always-present auth token: the env pin if
    /// set, otherwise a freshly generated random token. Used by tests and as the
    /// in-process default. The serving path (`spawn_daemon`) instead constructs
    /// via [`DaemonState::with_token`] so the SAME token it persists to the token
    /// file is the one the state enforces.
    pub fn new() -> Self {
        Self::with_token(establish_daemon_auth_token())
    }

    /// Construct daemon state bound to a specific, already-established token.
    fn with_token(auth_token: String) -> Self {
        Self {
            next_session_id: AtomicU64::new(1),
            bases: RwLock::new(HashMap::new()),
            base_generation_seq: AtomicU64::new(1),
            projects: RwLock::new(HashMap::new()),
            sessions: RwLock::new(HashMap::new()),
            identity: current_daemon_identity(),
            auth_token,
            governor: crate::sidecar::governor::RequestGovernor::new(),
        }
    }

    /// Intern `candidate` into the per-daemon base table, enforcing SC-002 (Feature
    /// 012, Phase 0). Returns the SHARED `Arc<IndexBase>` for the candidate's
    /// [`BaseKey`]: the existing allocation when an equal key is already present
    /// (so two consumers at the same `(root, commit)` share ONE base, verified by
    /// `Arc::ptr_eq`), otherwise the candidate re-stamped with a freshly minted
    /// monotonic `base_generation` and published.
    ///
    /// LOCK ORDER: acquires ONLY `bases` (the top of the `bases -> projects ->
    /// sessions` hierarchy) and touches no other daemon lock, so callers remain
    /// free to acquire `projects`/`sessions` afterwards without risking an
    /// inversion. Never call this while holding `projects` or `sessions`.
    fn intern_base(&self, candidate: Arc<IndexBase>) -> Arc<IndexBase> {
        let mut bases = self.bases.write();
        if let Some(existing) = bases.get(&candidate.key) {
            return Arc::clone(existing);
        }
        // Miss: re-stamp with the authoritative monotonic generation, then publish.
        // `IndexBase: Clone` is cheap (it clones the `Arc<LiveIndex>` handle, never
        // the file map), and the candidate is the sole owner here.
        let generation = self.base_generation_seq.fetch_add(1, Ordering::Relaxed);
        let mut published = (*candidate).clone();
        published.base_generation = generation;
        let published = Arc::new(published);
        bases.insert(published.key.clone(), Arc::clone(&published));
        published
    }

    /// Intern the base for `project_id` if the project is currently loaded
    /// (Feature 012, Phase 0). Reads the project under `projects.read()` to build
    /// the candidate base, releases that lock, then interns under `bases.write()`.
    ///
    /// LOCK ORDER NOTE: this takes `projects` THEN `bases`, which is the REVERSE
    /// of the declared `bases -> projects` order. It is safe ONLY because the two
    /// guards never overlap — the `projects` read guard is dropped at the end of
    /// the inner block BEFORE `intern_base` acquires `bases`. The hierarchy
    /// forbids HOLDING `projects` while acquiring `bases`, not acquiring them in
    /// sequence with no overlap. Returns `None` if the project is not loaded.
    fn intern_base_for_project(&self, project_id: &str) -> Option<Arc<IndexBase>> {
        let candidate = {
            let projects = self.projects.read();
            projects.get(project_id).map(ProjectInstance::base)
        }; // projects read guard dropped here, before bases.write() below
        candidate.map(|candidate| self.intern_base(candidate))
    }

    fn register_session_for_existing_project(
        &self,
        project_id: &str,
        request: &OpenProjectRequest,
        _canonical_root: &Path,
    ) -> anyhow::Result<OpenProjectResponse> {
        let session_id = format!(
            "session-{}",
            self.next_session_id.fetch_add(1, Ordering::Relaxed)
        );
        let now = SystemTime::now();

        // Feature 012, Phase 0+1: intern the project's base FIRST, holding no
        // other daemon lock (`intern_base_for_project` takes `projects` then
        // `bases` with no overlap, both released before we proceed). This seeds
        // the per-session working set below and keeps the `bases -> projects ->
        // sessions` order: every subsequent lock here is acquired AFTER `bases`
        // is released.
        let base = self.intern_base_for_project(project_id);

        let (project_name, canonical_root_text, session_count) = {
            let mut projects = self.projects.write();
            let project = projects.get_mut(project_id).ok_or_else(|| {
                anyhow::anyhow!(
                    "project {} was removed between check and session registration",
                    project_id
                )
            })?;
            project.session_ids.insert(session_id.clone());
            (
                project.project_name.clone(),
                normalized_path_string(&project.canonical_root),
                project.session_ids.len(),
            )
        };

        // Seed the session's working set with ONE entry: the active project, its
        // interned shared base, and an EMPTY overlay (Phase 1). INERT: no route
        // reads it yet, and no path writes the overlay (no-overlay-writes
        // invariant). If interning returned `None` (project vanished mid-open) the
        // working set stays empty; `session_runtime` still resolves via
        // `active_project_id`, so behavior is unchanged.
        let mut working_set = WorkingSet::new();
        if let Some(base) = base {
            working_set.add(project_id.to_string(), base);
        }

        let session = SessionRecord {
            session_id: session_id.clone(),
            active_project_id: project_id.to_string(),
            working_set: Arc::new(RwLock::new(working_set)),
            client_name: request.client_name.clone(),
            pid: request.pid,
            opened_at: now,
            last_seen_at: AtomicU64::new(now_epoch_millis()),
        };
        self.sessions.write().insert(session_id.clone(), session);

        Ok(OpenProjectResponse {
            project_id: project_id.to_string(),
            session_id,
            project_name,
            canonical_root: canonical_root_text,
            session_count,
        })
    }

    pub fn open_project_session(
        &self,
        request: OpenProjectRequest,
    ) -> anyhow::Result<OpenProjectResponse> {
        let canonical_root = canonical_project_root(Path::new(&request.project_root))?;
        // Trust boundary: `open_project_session` performs a full `LiveIndex::load`
        // with no guard of its own, so a session-open on a sensitive system or
        // credential-bearing root would read system/credential files. Apply the
        // same unified guard used by `index_folder_for_session`, immediately
        // after canonicalization and before any load, refusing cleanly (no panic).
        if crate::paths::is_sensitive_path(&canonical_root) {
            anyhow::bail!(
                "Refused to open session for sensitive system path: {}. Use a project directory instead.",
                canonical_root.display()
            );
        }
        let project_id = project_key(&canonical_root);

        // Fast path: project already loaded — just add session.
        {
            let projects = self.projects.read();
            if projects.contains_key(&project_id) {
                drop(projects);
                return self.register_session_for_existing_project(
                    &project_id,
                    &request,
                    &canonical_root,
                );
            }
        }

        // Slow path: project not loaded — load unlocked, then re-check under write lock.
        let new_project = ProjectInstance::load(&canonical_root)?;

        {
            let mut projects = self.projects.write();
            if projects.contains_key(&project_id) {
                // Another thread won the race — discard our loaded instance.
                // Instance is Inactive with no spawned tasks, safe to drop.
            } else {
                // We won — insert as Inactive, then activate under the same lock
                // to avoid a second write-lock acquisition.
                projects.insert(project_id.clone(), new_project);
                if let Some(project) = projects.get_mut(&project_id)
                    && project.activation_state == ActivationState::Inactive
                {
                    project.activate();
                }
            }
        }

        // Register session (works whether we inserted or another thread did).
        self.register_session_for_existing_project(&project_id, &request, &canonical_root)
    }

    pub fn heartbeat(&self, session_id: &str) -> HeartbeatResponse {
        let known_session = self
            .sessions
            .read()
            .get(session_id)
            .map(|session| {
                session
                    .last_seen_at
                    .store(now_epoch_millis(), Ordering::Relaxed);
                true
            })
            .unwrap_or(false);

        HeartbeatResponse {
            session_id: session_id.to_string(),
            known_session,
        }
    }

    pub fn close_session(&self, session_id: &str) -> Option<CloseSessionResponse> {
        // Lock ordering: projects write first, then sessions write.
        // Scan all projects instead of relying on session.project_id,
        // which can be stale if index_folder_for_session reassigned concurrently.
        let mut project_removed = false;
        let (project_id, remaining_sessions) = {
            let mut projects = self.projects.write();
            let owning_project_id = projects
                .iter()
                .find(|(_, project)| project.session_ids.contains(session_id))
                .map(|(id, _)| id.clone());

            match owning_project_id {
                Some(pid) => {
                    let Some(project) = projects.get_mut(&pid) else {
                        tracing::warn!(
                            project_id = %pid,
                            session_id,
                            "daemon close observed stale owning project; returning orphan close response"
                        );
                        let session = self.sessions.write().remove(session_id)?;
                        return Some(CloseSessionResponse {
                            session_id: session.session_id,
                            project_id: "orphan".to_string(),
                            remaining_sessions: 0,
                            project_removed: false,
                        });
                    };
                    project.session_ids.remove(session_id);
                    let remaining = project.session_ids.len();
                    if remaining == 0 {
                        if let Some(mut removed) = projects.remove(&pid) {
                            abort_watcher_task(&mut removed.watcher_task, &removed.stop_token);
                        }
                        project_removed = true;
                    }
                    (pid, remaining)
                }
                None => {
                    // Session not found in any project -- it was never registered under
                    // a project or its project was already removed. Clean up the session
                    // record only. We return "orphan" as the project_id because
                    // session.project_id may be stale (set at open time and never
                    // updated if index_folder_for_session later reassigned it).
                    let session = self.sessions.write().remove(session_id)?;
                    return Some(CloseSessionResponse {
                        session_id: session.session_id,
                        project_id: "orphan".to_string(),
                        remaining_sessions: 0,
                        project_removed: false,
                    });
                }
            }
        };

        let session = self.sessions.write().remove(session_id)?;

        Some(CloseSessionResponse {
            session_id: session.session_id,
            project_id,
            remaining_sessions,
            project_removed,
        })
    }

    pub fn list_projects(&self) -> Vec<ProjectSummary> {
        let projects = self.projects.read();
        let mut summaries: Vec<ProjectSummary> = projects
            .values()
            .map(|project| ProjectSummary {
                project_id: project.project_id.clone(),
                project_name: project.project_name.clone(),
                canonical_root: normalized_path_string(&project.canonical_root),
                session_count: project.session_ids.len(),
                opened_at_unix_secs: unix_seconds(project.opened_at),
            })
            .collect();
        summaries.sort_by(|a, b| a.canonical_root.cmp(&b.canonical_root));
        summaries
    }

    pub fn project_health(&self, project_id: &str) -> Option<ProjectHealth> {
        let projects = self.projects.read();
        let project = projects.get(project_id)?;
        let published = project.index.published_state();

        Some(ProjectHealth {
            project_id: project.project_id.clone(),
            project_name: project.project_name.clone(),
            canonical_root: normalized_path_string(&project.canonical_root),
            session_count: project.session_ids.len(),
            file_count: published.file_count,
            symbol_count: published.symbol_count,
            index_state: published.status_label().to_string(),
            opened_at_unix_secs: unix_seconds(project.opened_at),
        })
    }

    pub fn list_sessions(&self, project_id: &str) -> Option<Vec<SessionSummary>> {
        let session_ids: Vec<String> = {
            let projects = self.projects.read();
            let project = projects.get(project_id)?;
            project.session_ids.iter().cloned().collect()
        };

        let sessions = self.sessions.read();
        let mut summaries: Vec<SessionSummary> = session_ids
            .iter()
            .filter_map(|session_id| sessions.get(session_id))
            .map(|session| SessionSummary {
                session_id: session.session_id.clone(),
                project_id: session.active_project_id.clone(),
                client_name: session.client_name.clone(),
                pid: session.pid,
                opened_at_unix_secs: unix_seconds(session.opened_at),
                last_seen_at_unix_secs: unix_seconds(session.last_seen_at_time()),
            })
            .collect();
        summaries.sort_by(|a, b| a.session_id.cmp(&b.session_id));
        Some(summaries)
    }

    fn index_folder_for_session(
        &self,
        session_id: &str,
        input: IndexFolderInput,
    ) -> anyhow::Result<String> {
        let target_root = canonical_project_root(Path::new(&input.path))?;
        // Fail-closed daemon auth is now in force: the daemon ALWAYS establishes
        // a token at startup and `authorize_daemon_request` rejects any caller
        // that does not present it, so this route (like every authenticated
        // route) is unreachable by an unauthenticated local process. The
        // ambient-authority surface is closed. See `establish_daemon_auth_token`
        // / `write_daemon_token_file` / `authorize_daemon_request`.
        //
        // Trust boundary: refuse sensitive system paths before any reload/IO.
        // The local `tools::index_folder` already guards this; the daemon path
        // canonicalizes via `canonical_project_root` (which yields the `\\?\`
        // extended-length form on Windows) and must apply the same hardened,
        // prefix-aware guard so a daemon-routed call cannot index system files
        // or drive a reload into a denial-of-service.
        if crate::paths::is_sensitive_path(&target_root) {
            return Ok(format!(
                "Refused to index sensitive system path: {}.                  Use a project directory instead.",
                target_root.display()
            ));
        }
        let target_project_id = project_key(&target_root);
        let current_session_root = {
            let projects = self.projects.read();
            let sessions = self.sessions.read();
            let session = sessions
                .get(session_id)
                .ok_or_else(|| anyhow::anyhow!("unknown session '{session_id}'"))?;
            projects
                .get(&session.active_project_id)
                .map(|project| project.canonical_root.clone())
        };
        let reset_requested = crate::protocol::tools::index_folder_reset_requested();
        let idempotency = match input.idempotency_key.as_deref() {
            Some(raw_key) => match crate::idempotency::begin_index_folder_replay(
                &target_root,
                current_session_root.as_deref(),
                &target_root,
                raw_key,
                reset_requested,
            ) {
                Ok(crate::idempotency::ReplayStart::FirstExecution(active)) => Some(active),
                Ok(crate::idempotency::ReplayStart::Replay(response)) => return Ok(response),
                Err(error) => return Ok(crate::idempotency::format_tool_error(&error)),
            },
            None => None,
        };

        // All project-map mutations happen inside this block so the write guard
        // is fully released before we touch sessions.write() below — preventing
        // the lock-order inversion with close_session (sessions → projects).
        let reload_result = (|| -> anyhow::Result<(usize, usize, bool)> {
            let mut projects = self.projects.write();

            // Re-read current_project_id under projects write lock to handle
            // concurrent reassignment by another index_folder_for_session call.
            let current_project_id = {
                let sessions = self.sessions.read();
                sessions
                    .get(session_id)
                    .map(|s| s.active_project_id.clone())
                    .ok_or_else(|| anyhow::anyhow!("unknown session '{session_id}'"))?
            };
            let needs_reassign = current_project_id != target_project_id;

            if needs_reassign {
                if !projects.contains_key(&target_project_id) {
                    let mut project = ProjectInstance::load(&target_root)?;
                    debug_assert_eq!(
                        project.activation_state,
                        ActivationState::Inactive,
                        "freshly loaded project must be Inactive"
                    );
                    project.activate();
                    projects.insert(target_project_id.clone(), project);
                }

                if let Some(current_project) = projects.get_mut(&current_project_id) {
                    current_project.session_ids.remove(session_id);
                }

                if let Some(target_project) = projects.get_mut(&target_project_id) {
                    target_project.session_ids.insert(session_id.to_string());
                }

                let should_remove_old = projects
                    .get(&current_project_id)
                    .map(|project| project.session_ids.is_empty())
                    .unwrap_or(false);
                if should_remove_old && let Some(mut removed) = projects.remove(&current_project_id)
                {
                    abort_watcher_task(&mut removed.watcher_task, &removed.stop_token);
                }
            }

            let target_project = projects
                .get_mut(&target_project_id)
                .ok_or_else(|| anyhow::anyhow!("missing target project after reload"))?;
            let counts = target_project.reload(&target_root)?;
            Ok((counts.0, counts.1, needs_reassign))
        })(); // projects write lock released here

        let (file_count, symbol_count, needs_reassign) = match reload_result {
            Ok(counts) => counts,
            Err(error) => {
                if let Some(idempotency) = &idempotency {
                    let _ = idempotency.fail(format!("Index failed: {error}"));
                }
                return Err(error);
            }
        };

        // Update the session's project association *after* the projects lock is
        // released to maintain lock order (projects before sessions everywhere).
        // ponytail: Phase 1 retarget updates ONLY `active_project_id` (the field
        // `session_runtime` resolves), leaving the seeded `working_set` entry
        // pointing at the pre-retarget project. This is invisible in Phase 0/1
        // because NO route reads the working set yet; Phase 2's additive
        // `index_folder(add:true)` / `set_active_project` path owns re-seeding the
        // working set on a project change. No overlay is written here (invariant).
        if needs_reassign && let Some(session) = self.sessions.write().get_mut(session_id) {
            session.active_project_id = target_project_id;
            session
                .last_seen_at
                .store(now_epoch_millis(), Ordering::Relaxed);
        }

        let mut output = format!("Indexed {} files, {} symbols.", file_count, symbol_count);
        if let Some(idempotency) = &idempotency
            && let Err(error) = idempotency.complete(output.clone())
        {
            output.push_str(&format!(
                "\nIdempotency warning: failed to store replay result: {error}"
            ));
        }
        Ok(output)
    }

    fn session_runtime(&self, session_id: &str) -> Option<SessionRuntime> {
        // Acquire projects read lock BEFORE sessions read lock so that
        // the active_project_id we read from the session is still valid while
        // we look it up in the projects map. (Lock order: projects -> sessions;
        // `bases` is not taken here.) Feature 012, Phase 1: resolution is via
        // `active_project_id` — the single active project — so the single-project
        // path is byte-for-byte unchanged; the session's `working_set` is NOT read
        // here (cross-project routing is Phase 2/3).
        let projects = self.projects.read();
        let session = {
            let sessions = self.sessions.read();
            sessions.get(session_id)?.clone()
        };
        let project = projects.get(&session.active_project_id)?;
        Some(SessionRuntime {
            canonical_root: project.canonical_root.clone(),
            project_id: session.active_project_id.clone(),
            session_id: session.session_id.clone(),
            index: Arc::clone(&project.index),
            token_stats: Arc::clone(&project.token_stats),
            symbol_cache: Arc::clone(&project.symbol_cache),
            server: project.server.clone(),
        })
    }

    pub fn health(&self) -> DaemonHealth {
        DaemonHealth {
            project_count: self.projects.read().len(),
            session_count: self.sessions.read().len(),
            daemon_version: self.identity.version.clone(),
            executable_path: self.identity.executable_path.clone(),
            // Always true now: the daemon is fail-closed and always requires a
            // token. The field is retained for wire compatibility and so clients
            // can surface that authentication is in force.
            auth_required: true,
            pid: Some(std::process::id()),
        }
    }
}

impl Default for DaemonState {
    fn default() -> Self {
        Self::new()
    }
}

impl DaemonSessionClient {
    fn new_with_auth_token(
        base_url: String,
        project_id: String,
        session_id: String,
        project_name: String,
        auth_token: Option<String>,
    ) -> Self {
        // CRITICAL: reqwest::Client::new() has NO timeout by default.
        // Under concurrent load, if the daemon is slow, HTTP requests hang
        // indefinitely while holding read locks on daemon_lock in the proxy
        // layer. This creates a deadlock-like scenario when a reconnect
        // attempt needs a write lock but reads never release.
        //
        // 30s connect + 60s total covers heavy ops (batch_edit on large repos)
        // while ensuring stuck requests eventually fail and release locks.
        let http_client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(5))
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .expect("failed to build reqwest client with timeout — this is a critical invariant");

        Self {
            http_client,
            base_url,
            project_id,
            session_id,
            project_name,
            auth_token,
            project_root: None,
        }
    }

    fn with_project_root(mut self, root: PathBuf) -> Self {
        self.project_root = Some(root);
        self
    }

    #[cfg(test)]
    pub(crate) fn new_for_test(
        base_url: String,
        project_id: String,
        session_id: String,
        project_name: String,
    ) -> Self {
        // The daemon is fail-closed; a proxy client must carry the token. Resolve
        // it the same way production does (env pin, then the persisted token
        // file) so test proxies authenticate against a spawned daemon.
        Self::new_with_auth_token(
            base_url,
            project_id,
            session_id,
            project_name,
            resolve_daemon_auth_token(),
        )
    }

    pub fn project_name(&self) -> &str {
        &self.project_name
    }

    pub fn project_id(&self) -> &str {
        &self.project_id
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn project_root(&self) -> Option<&Path> {
        self.project_root.as_deref()
    }

    pub fn port(&self) -> Option<u16> {
        self.base_url
            .rsplit(':')
            .next()
            .and_then(|value| value.parse::<u16>().ok())
    }

    /// Attempt to reconnect to the daemon after a connection failure.
    ///
    /// Calls `ensure_daemon_running` (which will spawn a new daemon if needed),
    /// opens a fresh session, and returns the new client. The caller should
    /// replace their stored client with the returned one.
    pub async fn reconnect(&self) -> anyhow::Result<DaemonSessionClient> {
        let project_root = self
            .project_root
            .as_deref()
            .context("cannot reconnect: no project root stored")?;
        tracing::info!(
            "attempting daemon reconnection for project {}",
            self.project_name
        );
        let new_client =
            connect_or_spawn_session(project_root, "mcp-stdio", Some(std::process::id())).await?;
        Ok(new_client.with_project_root(project_root.to_path_buf()))
    }

    pub async fn call_tool_value(
        &self,
        tool_name: &str,
        params: serde_json::Value,
    ) -> anyhow::Result<String> {
        let request = self
            .http_client
            .post(format!(
                "{}/v1/sessions/{}/tools/{}",
                self.base_url, self.session_id, tool_name
            ))
            .json(&params);
        let response = apply_daemon_auth_header(request, self.auth_token.as_deref())
            .send()
            .await
            .with_context(|| format!("calling daemon tool '{tool_name}'"))?
            .error_for_status()
            .with_context(|| format!("daemon rejected tool '{tool_name}'"))?;

        response
            .text()
            .await
            .with_context(|| format!("reading daemon tool response for '{tool_name}'"))
    }

    pub async fn heartbeat(&self) -> anyhow::Result<HeartbeatResponse> {
        let request = self.http_client.post(format!(
            "{}/v1/sessions/{}/heartbeat",
            self.base_url, self.session_id
        ));
        apply_daemon_auth_header(request, self.auth_token.as_deref())
            .send()
            .await
            .context("sending daemon heartbeat")?
            .error_for_status()
            .context("daemon heartbeat status")?
            .json::<HeartbeatResponse>()
            .await
            .context("daemon heartbeat body")
    }

    pub async fn close(&self) -> anyhow::Result<CloseSessionResponse> {
        let request = self
            .http_client
            .delete(format!("{}/v1/sessions/{}", self.base_url, self.session_id));
        apply_daemon_auth_header(request, self.auth_token.as_deref())
            .send()
            .await
            .context("closing daemon session")?
            .error_for_status()
            .context("daemon close status")?
            .json::<CloseSessionResponse>()
            .await
            .context("daemon close body")
    }
}

struct DaemonStartLock {
    path: PathBuf,
}

impl Drop for DaemonStartLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

pub async fn connect_or_spawn_session(
    project_root: &Path,
    client_name: &str,
    pid: Option<u32>,
) -> anyhow::Result<DaemonSessionClient> {
    let port = ensure_daemon_running().await?;
    let base_url = format!("http://127.0.0.1:{port}");
    let http_client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(60))
        .build()
        .context("building reqwest client for connect_or_spawn_session")?;
    // Resolve the token AFTER `ensure_daemon_running` so a freshly spawned
    // daemon's persisted token file is already present (it is written before the
    // port file). Env pin takes precedence; otherwise we read the token file.
    let auth_token = resolve_daemon_auth_token();
    let open_request = http_client
        .post(format!("{base_url}/v1/sessions/open"))
        .json(&OpenProjectRequest {
            project_root: project_root.display().to_string(),
            client_name: client_name.to_string(),
            pid,
        });
    let opened = apply_daemon_auth_header(open_request, auth_token.as_deref())
        .send()
        .await
        .context("opening daemon session")?
        .error_for_status()
        .context("daemon session open status")?
        .json::<OpenProjectResponse>()
        .await
        .context("daemon session open body")?;

    Ok(DaemonSessionClient::new_with_auth_token(
        base_url,
        opened.project_id,
        opened.session_id,
        opened.project_name,
        auth_token,
    )
    .with_project_root(project_root.to_path_buf()))
}

async fn ensure_daemon_running() -> anyhow::Result<u16> {
    let identity = current_daemon_identity();
    if let Some(port) = daemon_port_if_compatible(&identity).await? {
        tracing::debug!("daemon already running on port {port}");
        return Ok(port);
    }

    if let Some(_lock) = try_acquire_start_lock()? {
        if let Some(port) = daemon_port_if_compatible(&identity).await? {
            tracing::debug!("daemon became ready while acquiring lock, port {port}");
            return Ok(port);
        }
        tracing::info!("acquired start lock, spawning new daemon");
        stop_incompatible_recorded_daemon(&identity).await?;
        spawn_daemon_process()?;
        wait_for_daemon_ready(&identity).await
    } else {
        tracing::info!("start lock held by another process, waiting for daemon");
        wait_for_daemon_ready(&identity).await
    }
}

async fn daemon_port_if_compatible(identity: &DaemonIdentity) -> anyhow::Result<Option<u16>> {
    let port = match read_daemon_port_file() {
        Ok(port) => port,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) if error.kind() == io::ErrorKind::InvalidData => {
            tracing::warn!("ignoring corrupt symforge daemon port file: {error}");
            return Ok(None);
        }
        Err(error) => return Err(error).context("reading daemon port file"),
    };

    match daemon_health(port).await {
        Some(health) if daemon_health_matches(&health, identity) => Ok(Some(port)),
        Some(health) => {
            tracing::warn!(
                recorded_port = port,
                recorded_version = %health.daemon_version,
                expected_version = %identity.version,
                recorded_executable = %health.executable_path,
                expected_executable = %identity.executable_path,
                "recorded symforge daemon is incompatible with the current executable"
            );
            Ok(None)
        }
        None => Ok(None),
    }
}

async fn wait_for_daemon_ready(identity: &DaemonIdentity) -> anyhow::Result<u16> {
    let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(5);
    loop {
        if let Some(port) = daemon_port_if_compatible(identity).await? {
            return Ok(port);
        }

        if tokio::time::Instant::now() >= deadline {
            anyhow::bail!("timed out waiting for symforge daemon to become ready");
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }
}

async fn daemon_health(port: u16) -> Option<DaemonHealth> {
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(10))
        .build()
        .ok()?;
    client
        .get(format!("http://127.0.0.1:{port}/health"))
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?
        .json::<DaemonHealth>()
        .await
        .ok()
}

async fn daemon_health_ok(port: u16) -> bool {
    daemon_health(port).await.is_some()
}

async fn stop_incompatible_recorded_daemon(identity: &DaemonIdentity) -> anyhow::Result<()> {
    let port = match read_daemon_port_file() {
        Ok(port) => port,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) if error.kind() == io::ErrorKind::InvalidData => {
            tracing::warn!("removing corrupt symforge daemon port file: {error}");
            cleanup_daemon_runtime_files();
            return Ok(());
        }
        Err(error) => return Err(error).context("reading daemon port file"),
    };

    let Some(health) = daemon_health(port).await else {
        cleanup_daemon_runtime_files();
        return Ok(());
    };

    if daemon_health_matches(&health, identity) {
        return Ok(());
    }

    if let Ok(pid) = read_daemon_pid_file() {
        if should_terminate_recorded_daemon(&health, identity, pid) {
            if let Err(error) = terminate_process(pid) {
                tracing::warn!(
                    pid,
                    "failed to terminate incompatible symforge daemon automatically: {error}"
                );
            }
            wait_for_daemon_unhealthy(port).await;
        } else if !daemon_health_matches_recorded_pid(&health, pid) {
            match health.pid {
                Some(health_pid) => {
                    tracing::warn!(
                        recorded_pid = pid,
                        daemon_pid = health_pid,
                        "not terminating incompatible symforge daemon because the pid file does not match health"
                    );
                }
                None => {
                    tracing::warn!(
                        recorded_pid = pid,
                        "not terminating incompatible symforge daemon because health did not report a pid"
                    );
                }
            }
        } else {
            tracing::warn!(
                recorded_pid = pid,
                daemon_pid = ?health.pid,
                recorded_executable = %health.executable_path,
                expected_executable = %identity.executable_path,
                "not terminating incompatible symforge daemon because pid ownership/executable safety checks failed"
            );
        }
    }

    cleanup_daemon_runtime_files();
    Ok(())
}

/// Outcome of stopping the global daemon during `symforge update`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DaemonStopOutcome {
    /// No live, recorded daemon was found.
    NotRunning,
    /// The daemon was terminated and confirmed gone.
    Stopped { pid: u32 },
    /// The daemon was signaled (incl. SIGKILL escalation) but had not exited
    /// within the wait window; its runtime files are LEFT IN PLACE so it stays
    /// discoverable and the next launch reuses it instead of spawning a duplicate.
    StopTimedOut { pid: u32 },
    /// A daemon was recorded but the ownership/executable safety gate refused to
    /// terminate it (e.g. the recorded pid was recycled by an unrelated process).
    SkippedSafety,
}

/// Stop the currently-recorded global daemon regardless of its version, for the
/// update flow: the binary is about to be replaced, so even a same-version
/// daemon must exit and let the next launch respawn the new one. Unlike
/// [`stop_incompatible_recorded_daemon`], this does not skip a same-identity
/// daemon — but it preserves the exact ownership/executable safety gate
/// (`should_terminate_recorded_daemon`) so an unrelated or pid-recycled process
/// is never terminated.
#[allow(unsafe_code)] // SAFETY: SIGKILL targets a pid that already passed the ownership safety gate; a dead/invalid pid only errors, no memory is touched.
pub(crate) async fn stop_running_daemon_for_update() -> anyhow::Result<DaemonStopOutcome> {
    let port = match read_daemon_port_file() {
        Ok(port) => port,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(DaemonStopOutcome::NotRunning);
        }
        Err(error) if error.kind() == io::ErrorKind::InvalidData => {
            tracing::warn!("removing corrupt symforge daemon port file during update: {error}");
            cleanup_daemon_runtime_files();
            return Ok(DaemonStopOutcome::NotRunning);
        }
        Err(error) => return Err(error).context("reading daemon port file"),
    };

    let Some(health) = daemon_health(port).await else {
        // Recorded but unreachable — clear the stale files and report not-running.
        cleanup_daemon_runtime_files();
        return Ok(DaemonStopOutcome::NotRunning);
    };

    let identity = current_daemon_identity();
    match read_daemon_pid_file() {
        Ok(pid) if should_terminate_recorded_daemon(&health, &identity, pid) => {
            if let Err(error) = terminate_process(pid) {
                tracing::warn!(
                    pid,
                    "failed to terminate symforge daemon during update: {error}"
                );
            }
            wait_for_daemon_unhealthy(port).await;

            // Confirm the process is actually gone before declaring success and
            // removing its discovery files. The daemon's graceful-shutdown drain
            // (up to ~5s on Unix) can outlive terminate_process's wait window, so
            // escalate to SIGKILL and re-poll rather than orphaning a live daemon.
            if process_is_alive(pid) {
                #[cfg(unix)]
                // SAFETY: sending a signal to a pid is always memory-safe; an
                // already-dead/invalid pid simply returns an error we ignore, and
                // liveness is re-confirmed by the loop below.
                unsafe {
                    libc::kill(pid as i32, libc::SIGKILL);
                }
                for _ in 0..20 {
                    if !process_is_alive(pid) {
                        break;
                    }
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                }
            }

            if process_is_alive(pid) {
                // Still alive: LEAVE its runtime files so it stays discoverable and
                // the next launch reuses it instead of spawning a duplicate.
                tracing::warn!(
                    pid,
                    "symforge daemon still alive after terminate during update"
                );
                Ok(DaemonStopOutcome::StopTimedOut { pid })
            } else {
                cleanup_daemon_runtime_files();
                Ok(DaemonStopOutcome::Stopped { pid })
            }
        }
        // Safety gate refused: the daemon is alive and we will not touch it, so
        // LEAVE its runtime files in place — removing them would orphan a live,
        // discoverable daemon and cause a duplicate spawn on the next launch.
        Ok(_) => Ok(DaemonStopOutcome::SkippedSafety),
        // No readable pid file: nothing to stop; clear any stale leftovers.
        Err(_) => {
            cleanup_daemon_runtime_files();
            Ok(DaemonStopOutcome::NotRunning)
        }
    }
}

/// Best-effort liveness check: does a process with this pid still exist?
#[allow(unsafe_code)] // SAFETY: kill(pid, 0) only probes for existence; it sends no signal and touches no memory.
fn process_is_alive(pid: u32) -> bool {
    #[cfg(windows)]
    {
        // `tasklist` lists the pid only when it exists; "No tasks" otherwise.
        // hidden_command: the daemon has no console, so a plain spawn would
        // flash a new conhost window on every periodic liveness check.
        crate::process_util::hidden_command("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH", "/FO", "CSV"])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .map(|output| String::from_utf8_lossy(&output.stdout).contains(&pid.to_string()))
            .unwrap_or(false)
    }
    #[cfg(not(windows))]
    {
        // kill(pid, 0): Ok(0) => the process exists (or is a zombie),
        // ESRCH => it is gone.
        unsafe { libc::kill(pid as i32, 0) == 0 }
    }
}

async fn wait_for_daemon_unhealthy(port: u16) {
    let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(3);
    while tokio::time::Instant::now() < deadline {
        if !daemon_health_ok(port).await {
            break;
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }
}

fn try_acquire_start_lock() -> anyhow::Result<Option<DaemonStartLock>> {
    let path = daemon_dir()?.join(daemon_start_lock_file_name());
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
    {
        Ok(_) => Ok(Some(DaemonStartLock { path })),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            // Check if the lock is stale (older than 30 seconds).
            // Daemon startup takes <5s normally, so a 30s-old lock is certainly stale.
            if let Ok(metadata) = std::fs::metadata(&path)
                && let Ok(modified) = metadata.modified()
                && modified.elapsed().unwrap_or_default() > std::time::Duration::from_secs(30)
            {
                tracing::warn!("removing stale daemon start lock (age > 30s)");
                let _ = std::fs::remove_file(&path);
                // Retry creation — another process may grab it first.
                if std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&path)
                    .is_ok()
                {
                    return Ok(Some(DaemonStartLock { path }));
                }
            }
            Ok(None)
        }
        Err(error) => Err(error).context("creating daemon start lock"),
    }
}

fn spawn_daemon_process() -> anyhow::Result<()> {
    let current_exe = std::env::current_exe().context("locating current symforge executable")?;
    let mut command = std::process::Command::new(current_exe);
    command
        .arg("daemon")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;

        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(DETACHED_PROCESS | CREATE_NO_WINDOW);
    }

    command
        .spawn()
        .context("spawning detached symforge daemon")?;
    Ok(())
}

fn current_daemon_identity() -> DaemonIdentity {
    let executable_path = std::env::current_exe()
        .ok()
        .map(|path| normalized_path_string(&path))
        .unwrap_or_else(|| "unknown".to_string());
    DaemonIdentity {
        version: env!("CARGO_PKG_VERSION").to_string(),
        executable_path,
    }
}

fn daemon_health_matches(health: &DaemonHealth, identity: &DaemonIdentity) -> bool {
    if health.daemon_version != identity.version {
        return false;
    }

    if health.executable_path == "unknown" || identity.executable_path == "unknown" {
        return true;
    }

    stable_path_identity(&health.executable_path) == stable_path_identity(&identity.executable_path)
}

fn daemon_health_matches_recorded_pid(health: &DaemonHealth, recorded_pid: u32) -> bool {
    health.pid == Some(recorded_pid)
}

fn should_terminate_recorded_daemon(
    health: &DaemonHealth,
    identity: &DaemonIdentity,
    recorded_pid: u32,
) -> bool {
    if !daemon_health_matches_recorded_pid(health, recorded_pid) {
        return false;
    }

    if !daemon_executable_name_matches_current(health, identity) {
        return false;
    }

    #[cfg(target_os = "linux")]
    {
        if !recorded_pid_owner_matches_current_user(recorded_pid).unwrap_or(false) {
            return false;
        }

        if !recorded_pid_executable_matches_health(recorded_pid, health).unwrap_or(false) {
            return false;
        }
    }

    true
}

fn daemon_executable_name_matches_current(
    health: &DaemonHealth,
    identity: &DaemonIdentity,
) -> bool {
    let Some(health_name) = daemon_executable_file_name(&health.executable_path) else {
        return false;
    };
    let Some(identity_name) = daemon_executable_file_name(&identity.executable_path) else {
        return false;
    };
    health_name == identity_name
}

fn daemon_executable_file_name(path: &str) -> Option<String> {
    if path == "unknown" {
        return None;
    }

    let normalized = path.replace('\\', "/");
    let name = normalized.rsplit('/').find(|part| !part.is_empty())?;
    if cfg!(windows) {
        Some(name.to_lowercase())
    } else {
        Some(name.to_string())
    }
}

#[cfg(target_os = "linux")]
#[allow(unsafe_code)] // Reading /proc ownership needs the current effective uid from libc.
fn recorded_pid_owner_matches_current_user(pid: u32) -> Option<bool> {
    let status = std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    let uid_line = status.lines().find(|line| line.starts_with("Uid:"))?;
    let effective_uid = uid_line.split_whitespace().nth(2)?.parse::<u32>().ok()?;
    let current_uid = unsafe { libc::geteuid() as u32 };
    Some(effective_uid == current_uid)
}

#[cfg(target_os = "linux")]
fn recorded_pid_executable_matches_health(pid: u32, health: &DaemonHealth) -> Option<bool> {
    if health.executable_path == "unknown" {
        return None;
    }

    let actual = std::fs::read_link(format!("/proc/{pid}/exe")).ok()?;
    Some(
        stable_path_identity(&normalized_path_string(&actual))
            == stable_path_identity(&health.executable_path),
    )
}

fn stable_path_identity(path: &str) -> String {
    let normalized = path.replace('\\', "/");
    if cfg!(windows) {
        normalized.to_lowercase()
    } else {
        normalized
    }
}

impl ProjectInstance {
    fn load(canonical_root: &Path) -> anyhow::Result<Self> {
        let project_name = canonical_root
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("project")
            .to_string();

        let index = live_index::LiveIndex::load(canonical_root).with_context(|| {
            format!(
                "failed to load project index for {}",
                canonical_root.display()
            )
        })?;
        let watcher_info = Arc::new(Mutex::new(WatcherInfo::default()));
        let token_stats = TokenStats::new();

        let server = SymForgeServer::new(
            Arc::clone(&index),
            project_name.clone(),
            Arc::clone(&watcher_info),
            Some(canonical_root.to_path_buf()),
            Some(Arc::clone(&token_stats)),
        );

        Ok(Self {
            project_id: project_key(canonical_root),
            canonical_root: canonical_root.to_path_buf(),
            project_name,
            index,
            watcher_info,
            watcher_task: None,
            stop_token: Arc::new(AtomicBool::new(false)),
            token_stats,
            symbol_cache: Arc::new(RwLock::new(HashMap::new())),
            session_ids: HashSet::new(),
            opened_at: SystemTime::now(),
            activation_state: ActivationState::Inactive,
            server,
        })
    }

    /// Build a candidate [`IndexBase`] for this project (Feature 012, Phase 0).
    ///
    /// Wraps the project's CURRENT published snapshot (`index.read()` ->
    /// `Arc<LiveIndex>`, the SAME shared handle the project already serves, so no
    /// second store and no `LiveIndex` change — Principle I) with a sharable
    /// [`BaseKey`] = `(canonical_root, commit)`. `commit` is the git HEAD sha for
    /// the root, or [`CommitId::Dirtyless`] when the root is not a git repository
    /// (so a non-git tree still has a stable, shareable identity).
    ///
    /// The returned base carries `base_generation = 1` (the start value). This is
    /// a CANDIDATE only: [`DaemonState::intern_base`] is the authority on
    /// generation and identity. On an intern cache hit it returns the EXISTING
    /// shared `Arc<IndexBase>` (SC-002) and discards this candidate; on a miss it
    /// re-stamps the generation from the daemon's monotonic sequence before
    /// publishing. Callers should therefore route every base through
    /// `intern_base` rather than using this value directly.
    fn base(&self) -> Arc<IndexBase> {
        let index = Arc::clone(&self.index.read());
        let commit = match crate::git::head_sha(&self.canonical_root) {
            Ok(sha) => CommitId::Sha(sha),
            Err(_) => CommitId::Dirtyless,
        };
        let key = BaseKey::new(self.canonical_root.clone(), commit);
        Arc::new(IndexBase::new(key, index, 1))
    }

    /// Activate background tasks (watcher + git temporal) for this project.
    /// Must only be called once, on an `Inactive` instance, after the caller
    /// has committed the instance into the project map under write-lock.
    fn activate(&mut self) {
        if self.activation_state != ActivationState::Inactive {
            tracing::error!(
                project_id = %self.project_id,
                state = ?self.activation_state,
                "activate() called on a project that is not Inactive — skipping"
            );
            return;
        }
        self.activation_state = ActivationState::Activating;

        self.stop_token = Arc::new(AtomicBool::new(false));
        self.watcher_task = start_project_watcher(
            self.canonical_root.clone(),
            Arc::clone(&self.index),
            Arc::clone(&self.watcher_info),
            Arc::clone(&self.stop_token),
        );

        // Kick off background git temporal analysis (non-blocking).
        let expected_gen = self.index.current_project_generation();
        live_index::git_temporal::spawn_git_temporal_computation(
            Arc::clone(&self.index),
            self.canonical_root.clone(),
            expected_gen,
        );

        self.activation_state = ActivationState::Active;
    }

    fn reload(&mut self, canonical_root: &Path) -> anyhow::Result<(usize, usize)> {
        abort_watcher_task(&mut self.watcher_task, &self.stop_token);

        self.index.reload(canonical_root)?;
        let published = self.index.published_state();
        let file_count = published.file_count;
        let symbol_count = published.symbol_count;

        self.stop_token = Arc::new(AtomicBool::new(false));
        self.watcher_task = start_project_watcher(
            canonical_root.to_path_buf(),
            Arc::clone(&self.index),
            Arc::clone(&self.watcher_info),
            Arc::clone(&self.stop_token),
        );
        self.canonical_root = canonical_root.to_path_buf();
        self.project_name = canonical_root
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("project")
            .to_string();
        self.project_id = project_key(canonical_root);

        // Rebuild cached server so it picks up the new project name / root.
        // The index Arc is the same handle, so the server sees updated data.
        self.server = SymForgeServer::new(
            Arc::clone(&self.index),
            self.project_name.clone(),
            Arc::clone(&self.watcher_info),
            Some(canonical_root.to_path_buf()),
            Some(Arc::clone(&self.token_stats)),
        );

        // Refresh git temporal data after reload.
        let expected_gen = self.index.current_project_generation();
        live_index::git_temporal::spawn_git_temporal_computation(
            Arc::clone(&self.index),
            canonical_root.to_path_buf(),
            expected_gen,
        );

        Ok((file_count, symbol_count))
    }
}

fn start_project_watcher(
    repo_root: PathBuf,
    index: SharedIndex,
    watcher_info: Arc<Mutex<WatcherInfo>>,
    stop_token: Arc<AtomicBool>,
) -> Option<tokio::task::JoinHandle<()>> {
    tokio::runtime::Handle::try_current().ok().map(|handle| {
        handle.spawn(watcher::run_watcher_with_stop(
            repo_root,
            index,
            watcher_info,
            stop_token,
        ))
    })
}

fn abort_watcher_task(
    task: &mut Option<tokio::task::JoinHandle<()>>,
    stop_token: &Arc<AtomicBool>,
) {
    stop_token.store(true, Ordering::Release);
    if let Some(task) = task.take() {
        task.abort();
    }
}

pub fn build_router(state: SharedDaemonState) -> Router {
    Router::new()
        .route("/health", get(daemon_health_handler))
        .route("/v1/projects", get(list_projects_handler))
        .route(
            "/v1/projects/{project_id}/health",
            get(project_health_handler),
        )
        .route(
            "/v1/projects/{project_id}/sessions",
            get(list_sessions_handler),
        )
        .route("/v1/sessions/open", post(open_project_session_handler))
        .route(
            "/v1/sessions/{session_id}/tools/{tool_name}",
            post(call_tool_handler),
        )
        .route(
            "/v1/sessions/{session_id}/sidecar/health",
            get(sidecar_health_handler),
        )
        .route(
            "/v1/sessions/{session_id}/sidecar/outline",
            get(sidecar_outline_handler),
        )
        .route(
            "/v1/sessions/{session_id}/sidecar/workflows/source-read",
            get(sidecar_outline_handler),
        )
        .route(
            "/v1/sessions/{session_id}/sidecar/impact",
            get(sidecar_impact_handler),
        )
        .route(
            "/v1/sessions/{session_id}/sidecar/workflows/post-edit-impact",
            get(sidecar_impact_handler),
        )
        .route(
            "/v1/sessions/{session_id}/sidecar/symbol-context",
            get(sidecar_symbol_context_handler),
        )
        .route(
            "/v1/sessions/{session_id}/sidecar/workflows/search-hit-expansion",
            get(sidecar_symbol_context_handler),
        )
        .route(
            "/v1/sessions/{session_id}/sidecar/repo-map",
            get(sidecar_repo_map_handler),
        )
        .route(
            "/v1/sessions/{session_id}/sidecar/workflows/repo-start",
            get(sidecar_repo_map_handler),
        )
        .route(
            "/v1/sessions/{session_id}/sidecar/prompt-context",
            get(sidecar_prompt_context_handler),
        )
        .route(
            "/v1/sessions/{session_id}/sidecar/workflows/prompt-context",
            get(sidecar_prompt_context_handler),
        )
        .route(
            "/v1/sessions/{session_id}/sidecar/stats",
            get(sidecar_stats_handler),
        )
        .route(
            "/v1/sessions/{session_id}/heartbeat",
            post(heartbeat_handler),
        )
        .route("/v1/sessions/{session_id}", delete(close_session_handler))
        .with_state(state)
}

pub async fn spawn_daemon(bind_host: &str) -> anyhow::Result<DaemonHandle> {
    let resolved_host = resolve_daemon_bind_host(bind_host)?;
    cleanup_daemon_runtime_files();

    let listener = TcpListener::bind(daemon_socket_bind_address(&resolved_host)).await?;
    let port = listener.local_addr()?.port();

    // Establish the fail-closed auth token and persist it BEFORE the port file.
    // Clients discover the daemon via the port file and then read the token file;
    // writing the token first guarantees a client that observes the port can
    // always read a valid token (no open-then-tokenless window).
    let auth_token = establish_daemon_auth_token();
    write_daemon_token_file(&auth_token)?;
    write_daemon_port_file(port)?;
    write_daemon_pid_file(std::process::id())?;

    let state = Arc::new(DaemonState::with_token(auth_token));
    let app = build_router(Arc::clone(&state));
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    let server_task = tokio::spawn(async move {
        let shutdown_signal = async move {
            let _ = shutdown_rx.await;
        };

        if let Err(error) = axum::serve(listener, app)
            .with_graceful_shutdown(shutdown_signal)
            .await
        {
            tracing::error!("daemon server error: {error}");
        }

        cleanup_daemon_runtime_files();
    });

    Ok(DaemonHandle {
        port,
        shutdown_tx,
        state,
        server_task,
    })
}

pub async fn run_daemon_until_shutdown(bind_host: &str) -> anyhow::Result<()> {
    let handle = spawn_daemon(bind_host).await?;
    tracing::info!(port = handle.port, "shared daemon started");
    // Wait for either SIGINT (Ctrl+C) or SIGTERM (kill, systemd, containers).
    // Both trigger the same graceful shutdown path.
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        let mut sigterm = signal(SignalKind::terminate())?;
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                tracing::info!("received SIGINT, shutting down");
            },
            _ = sigterm.recv() => {
                tracing::info!("received SIGTERM, shutting down");
            },
        }
    }
    #[cfg(not(unix))]
    {
        tokio::signal::ctrl_c().await?;
        tracing::info!("received Ctrl+C, shutting down");
    }
    let DaemonHandle {
        shutdown_tx,
        server_task,
        ..
    } = handle;
    let _ = shutdown_tx.send(());
    match tokio::time::timeout(tokio::time::Duration::from_secs(5), server_task).await {
        Ok(Ok(())) => {}
        Ok(Err(join_err)) => tracing::warn!("daemon server task ended with join error: {join_err}"),
        Err(_) => tracing::warn!("timed out waiting for daemon server task to shut down"),
    }
    Ok(())
}

async fn daemon_health_handler(State(state): State<SharedDaemonState>) -> Json<DaemonHealth> {
    Json(state.health())
}

async fn list_projects_handler(
    State(state): State<SharedDaemonState>,
    headers: HeaderMap,
) -> Result<Json<Vec<ProjectSummary>>, StatusCode> {
    authorize_daemon_request(&state, &headers)?;
    Ok(Json(state.list_projects()))
}

async fn project_health_handler(
    State(state): State<SharedDaemonState>,
    headers: HeaderMap,
    AxumPath(project_id): AxumPath<String>,
) -> Result<Json<ProjectHealth>, StatusCode> {
    authorize_daemon_request(&state, &headers)?;
    state
        .project_health(&project_id)
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

async fn list_sessions_handler(
    State(state): State<SharedDaemonState>,
    headers: HeaderMap,
    AxumPath(project_id): AxumPath<String>,
) -> Result<Json<Vec<SessionSummary>>, StatusCode> {
    authorize_daemon_request(&state, &headers)?;
    state
        .list_sessions(&project_id)
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

async fn open_project_session_handler(
    State(state): State<SharedDaemonState>,
    headers: HeaderMap,
    Json(request): Json<OpenProjectRequest>,
) -> Result<Json<OpenProjectResponse>, (StatusCode, String)> {
    authorize_daemon_request(&state, &headers).map_err(daemon_auth_error)?;
    let state_for_load = Arc::clone(&state);
    let response =
        tokio::task::spawn_blocking(move || state_for_load.open_project_session(request))
            .await
            .map_err(internal_error)?
            .map_err(bad_request)?;
    Ok(Json(response))
}

async fn call_tool_handler(
    State(state): State<SharedDaemonState>,
    headers: HeaderMap,
    AxumPath((session_id, tool_name)): AxumPath<(String, String)>,
    Json(params): Json<serde_json::Value>,
) -> Result<String, (StatusCode, String)> {
    authorize_daemon_request(&state, &headers).map_err(daemon_auth_error)?;
    if tool_name == "index_folder" {
        let input = decode_params::<IndexFolderInput>(params).map_err(bad_request)?;
        let state_for_index = state.clone();
        let session_id_owned = session_id.clone();
        return state
            .governor
            .execute_non_abortable("index_folder", async move {
                tokio::task::spawn_blocking(move || {
                    state_for_index.index_folder_for_session(&session_id_owned, input)
                })
                .await
                .map_err(|join_err| anyhow::anyhow!("index_folder task panicked: {join_err}"))?
            })
            .await
            .map_err(|gov_err| bad_request(gov_err.into()))?
            .map_err(bad_request);
    }

    let runtime = state.session_runtime(&session_id).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            format!("unknown session '{session_id}'"),
        )
    })?;

    // Tool handlers acquire parking_lot::RwLock on the shared index, which
    // blocks the OS thread. Running them directly on the async runtime starves
    // tokio worker threads under concurrent load (10+ subagents).
    //
    // spawn_blocking moves execution to tokio's blocking thread pool (default
    // 512 threads), keeping async worker threads free for I/O, MCP transport,
    // and new request acceptance.
    let tool_name_owned = tool_name.clone();
    let tool_name_for_panic = tool_name.clone();
    match state
        .governor
        .execute_non_abortable(&tool_name, async move {
            let handle = tokio::runtime::Handle::current();
            tokio::task::spawn_blocking(move || {
                handle.block_on(execute_tool_call(runtime, &tool_name_owned, params))
            })
            .await
            .map_err(|join_err| anyhow::anyhow!("tool task panicked: {join_err}"))?
        })
        .await
    {
        Ok(Ok(result)) => Ok(result),
        Ok(Err(tool_err)) => {
            // Tool returned an error — surface it as HTTP 200 so the MCP client
            // gets the message immediately instead of entering reconnect/timeout.
            Ok(format!("Error in {}: {}", tool_name_for_panic, tool_err))
        }
        Err(gov_err) => {
            // Governor error (timeout, queue full, panic) — return as HTTP 200
            // with a clear error prefix so the model knows to stop waiting.
            let msg = format!(
                "Error: tool '{}' failed — {}. The tool did not complete. Do not retry immediately.",
                tool_name_for_panic, gov_err
            );
            tracing::error!(tool = %tool_name_for_panic, "tool execution failed: {gov_err}");
            Ok(msg)
        }
    }
}

async fn sidecar_health_handler(
    State(state): State<SharedDaemonState>,
    headers: HeaderMap,
    AxumPath(session_id): AxumPath<String>,
) -> Result<Json<crate::sidecar::handlers::HealthResponse>, StatusCode> {
    authorize_daemon_request(&state, &headers)?;
    let runtime = state
        .session_runtime(&session_id)
        .ok_or(StatusCode::NOT_FOUND)?;
    let sidecar = sidecar_state_for_runtime(&runtime);
    state
        .governor
        .execute("sidecar/health", async move {
            let handle = tokio::runtime::Handle::current();
            tokio::task::spawn_blocking(move || {
                handle.block_on(crate::sidecar::handlers::health_handler(State(sidecar)))
            })
            .await
            .unwrap_or(Err(StatusCode::INTERNAL_SERVER_ERROR))
        })
        .await
        .map_err(|_| StatusCode::GATEWAY_TIMEOUT)?
}

async fn sidecar_outline_handler(
    State(state): State<SharedDaemonState>,
    headers: HeaderMap,
    AxumPath(session_id): AxumPath<String>,
    Query(params): Query<crate::sidecar::handlers::OutlineParams>,
) -> Result<String, StatusCode> {
    authorize_daemon_request(&state, &headers)?;
    let runtime = state
        .session_runtime(&session_id)
        .ok_or(StatusCode::NOT_FOUND)?;
    let sidecar = sidecar_state_for_runtime(&runtime);
    state
        .governor
        .execute("sidecar/outline", async move {
            let handle = tokio::runtime::Handle::current();
            tokio::task::spawn_blocking(move || {
                handle.block_on(crate::sidecar::handlers::outline_handler(
                    State(sidecar),
                    Query(params),
                ))
            })
            .await
            .unwrap_or(Err(StatusCode::INTERNAL_SERVER_ERROR))
        })
        .await
        .map_err(|_| StatusCode::GATEWAY_TIMEOUT)?
}

async fn sidecar_impact_handler(
    State(state): State<SharedDaemonState>,
    headers: HeaderMap,
    AxumPath(session_id): AxumPath<String>,
    Query(params): Query<crate::sidecar::handlers::ImpactParams>,
) -> Result<String, StatusCode> {
    authorize_daemon_request(&state, &headers)?;
    let runtime = state
        .session_runtime(&session_id)
        .ok_or(StatusCode::NOT_FOUND)?;
    let sidecar = sidecar_state_for_runtime(&runtime);
    state
        .governor
        .execute("sidecar/impact", async move {
            let handle = tokio::runtime::Handle::current();
            tokio::task::spawn_blocking(move || {
                handle.block_on(crate::sidecar::handlers::impact_handler(
                    State(sidecar),
                    Query(params),
                ))
            })
            .await
            .unwrap_or(Err(StatusCode::INTERNAL_SERVER_ERROR))
        })
        .await
        .map_err(|_| StatusCode::GATEWAY_TIMEOUT)?
}

async fn sidecar_symbol_context_handler(
    State(state): State<SharedDaemonState>,
    headers: HeaderMap,
    AxumPath(session_id): AxumPath<String>,
    Query(params): Query<crate::sidecar::handlers::SymbolContextParams>,
) -> Result<String, StatusCode> {
    authorize_daemon_request(&state, &headers)?;
    let runtime = state
        .session_runtime(&session_id)
        .ok_or(StatusCode::NOT_FOUND)?;
    let sidecar = sidecar_state_for_runtime(&runtime);
    state
        .governor
        .execute("sidecar/symbol-context", async move {
            let handle = tokio::runtime::Handle::current();
            tokio::task::spawn_blocking(move || {
                handle.block_on(crate::sidecar::handlers::symbol_context_handler(
                    State(sidecar),
                    Query(params),
                ))
            })
            .await
            .unwrap_or(Err(StatusCode::INTERNAL_SERVER_ERROR))
        })
        .await
        .map_err(|_| StatusCode::GATEWAY_TIMEOUT)?
}

async fn sidecar_repo_map_handler(
    State(state): State<SharedDaemonState>,
    headers: HeaderMap,
    AxumPath(session_id): AxumPath<String>,
) -> Result<String, StatusCode> {
    authorize_daemon_request(&state, &headers)?;
    let runtime = state
        .session_runtime(&session_id)
        .ok_or(StatusCode::NOT_FOUND)?;
    let sidecar = sidecar_state_for_runtime(&runtime);
    state
        .governor
        .execute("sidecar/repo-map", async move {
            let handle = tokio::runtime::Handle::current();
            tokio::task::spawn_blocking(move || {
                handle.block_on(crate::sidecar::handlers::repo_map_handler(State(sidecar)))
            })
            .await
            .unwrap_or(Err(StatusCode::INTERNAL_SERVER_ERROR))
        })
        .await
        .map_err(|_| StatusCode::GATEWAY_TIMEOUT)?
}

async fn sidecar_prompt_context_handler(
    State(state): State<SharedDaemonState>,
    headers: HeaderMap,
    AxumPath(session_id): AxumPath<String>,
    Query(params): Query<crate::sidecar::handlers::PromptContextParams>,
) -> Result<String, StatusCode> {
    authorize_daemon_request(&state, &headers)?;
    let runtime = state
        .session_runtime(&session_id)
        .ok_or(StatusCode::NOT_FOUND)?;
    let sidecar = sidecar_state_for_runtime(&runtime);
    state
        .governor
        .execute("sidecar/prompt-context", async move {
            let handle = tokio::runtime::Handle::current();
            tokio::task::spawn_blocking(move || {
                handle.block_on(crate::sidecar::handlers::prompt_context_handler(
                    State(sidecar),
                    Query(params),
                ))
            })
            .await
            .unwrap_or(Err(StatusCode::INTERNAL_SERVER_ERROR))
        })
        .await
        .map_err(|_| StatusCode::GATEWAY_TIMEOUT)?
}

async fn sidecar_stats_handler(
    State(state): State<SharedDaemonState>,
    headers: HeaderMap,
    AxumPath(session_id): AxumPath<String>,
) -> Result<Json<crate::sidecar::StatsSnapshot>, StatusCode> {
    authorize_daemon_request(&state, &headers)?;
    let runtime = state
        .session_runtime(&session_id)
        .ok_or(StatusCode::NOT_FOUND)?;
    let sidecar = sidecar_state_for_runtime(&runtime);
    state
        .governor
        .execute("sidecar/stats", async move {
            let handle = tokio::runtime::Handle::current();
            tokio::task::spawn_blocking(move || {
                handle.block_on(async {
                    Ok(crate::sidecar::handlers::stats_handler(State(sidecar)).await)
                })
            })
            .await
            .unwrap_or(Err(StatusCode::INTERNAL_SERVER_ERROR))
        })
        .await
        .map_err(|_| StatusCode::GATEWAY_TIMEOUT)?
}

async fn heartbeat_handler(
    State(state): State<SharedDaemonState>,
    headers: HeaderMap,
    AxumPath(session_id): AxumPath<String>,
) -> Result<Json<HeartbeatResponse>, StatusCode> {
    authorize_daemon_request(&state, &headers)?;
    Ok(Json(state.heartbeat(&session_id)))
}

async fn close_session_handler(
    State(state): State<SharedDaemonState>,
    headers: HeaderMap,
    AxumPath(session_id): AxumPath<String>,
) -> Result<Json<CloseSessionResponse>, StatusCode> {
    authorize_daemon_request(&state, &headers)?;
    state
        .close_session(&session_id)
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

async fn execute_tool_call(
    runtime: SessionRuntime,
    tool_name: &str,
    params: serde_json::Value,
) -> anyhow::Result<String> {
    runtime.token_stats.record_tool_call(tool_name);

    // Use the cached server from ProjectInstance (cloned cheaply via Arc internals)
    // instead of constructing a new SymForgeServer per tool call.
    let server = runtime.server;

    match tool_name {
        "get_symbol" => Ok(server
            .get_symbol(Parameters(decode_params::<GetSymbolInput>(params)?))
            .await),
        "get_repo_map" => Ok(server
            .get_repo_map(Parameters(decode_params::<GetRepoMapInput>(params)?))
            .await),
        "get_file_context" => Ok(server
            .get_file_context(Parameters(decode_params::<GetFileContextInput>(params)?))
            .await),
        "get_symbol_context" => Ok(server
            .get_symbol_context(Parameters(decode_params::<GetSymbolContextInput>(params)?))
            .await),
        "analyze_file_impact" => Ok(server
            .analyze_file_impact(Parameters(decode_params::<AnalyzeFileImpactInput>(params)?))
            .await),
        "search_symbols" => Ok(server
            .search_symbols(Parameters(decode_params::<SearchSymbolsInput>(params)?))
            .await),
        "search_text" => Ok(server
            .search_text(Parameters(decode_params::<SearchTextInput>(params)?))
            .await),
        "trace_symbol" => {
            let tp: TraceSymbolInput = decode_params(params)?;
            // Convert: trace_symbol's None sections = "all" = empty vec in get_symbol_context
            let sections = tp.sections.unwrap_or_default();
            let output = server
                .get_symbol_context(Parameters(GetSymbolContextInput {
                    name: tp.name,
                    file: None,
                    path: Some(tp.path),
                    symbol_kind: tp.kind,
                    symbol_line: tp.symbol_line,
                    verbosity: tp.verbosity,
                    bundle: None,
                    sections: Some(sections),
                    max_tokens: None,
                    estimate: None,
                }))
                .await;
            Ok(format!("{TRACE_SYMBOL_ALIAS_DEPRECATION}\n\n{output}"))
        }
        "inspect_match" => Ok(server
            .inspect_match(Parameters(decode_params::<InspectMatchInput>(params)?))
            .await),
        "search_files" => Ok(server
            .search_files(Parameters(decode_params::<SearchFilesInput>(params)?))
            .await),
        "health" => {
            // SF-STRESS-010: forward the quarantine paging window so the daemon
            // (which owns the authoritative index in production) renders the
            // requested registry slice instead of silently ignoring args.
            let input: HealthInput = decode_params(params)?;
            Ok(server.health_for_daemon_session(
                runtime.project_id.clone(),
                runtime.session_id.clone(),
                runtime.canonical_root.clone(),
                crate::protocol::format::QuarantineWindow::from_args(
                    input.quarantine_offset.map(|n| n as usize),
                    input.quarantine_limit.map(|n| n as usize),
                ),
            ))
        }
        "health_compact" => Ok(server.health_compact_for_daemon_session(
            runtime.project_id.clone(),
            runtime.session_id.clone(),
            runtime.canonical_root.clone(),
        )),
        "index_folder" => Ok(server
            .index_folder(Parameters(decode_params::<IndexFolderInput>(params)?))
            .await),
        "what_changed" => Ok(server
            .what_changed(Parameters(decode_params::<WhatChangedInput>(params)?))
            .await),
        "get_file_content" => Ok(server
            .get_file_content(Parameters(decode_params::<GetFileContentInput>(params)?))
            .await),
        "find_references" => Ok(server
            .find_references(Parameters(decode_params::<FindReferencesInput>(params)?))
            .await),
        "find_dependents" => Ok(server
            .find_dependents(Parameters(decode_params::<FindDependentsInput>(params)?))
            .await),
        "explore" => Ok(server
            .explore(Parameters(decode_params::<ExploreInput>(params)?))
            .await),
        "diff_symbols" => Ok(server
            .diff_symbols(Parameters(decode_params::<DiffSymbolsInput>(params)?))
            .await),
        "replace_symbol_body" => Ok(server
            .replace_symbol_body(Parameters(decode_params::<ReplaceSymbolBodyInput>(params)?))
            .await),
        "insert_symbol" => Ok(server
            .insert_symbol(Parameters(decode_params::<InsertSymbolInput>(params)?))
            .await),
        "delete_symbol" => Ok(server
            .delete_symbol(Parameters(decode_params::<DeleteSymbolInput>(params)?))
            .await),
        "edit_within_symbol" => Ok(server
            .edit_within_symbol(Parameters(decode_params::<EditWithinSymbolInput>(params)?))
            .await),
        "batch_edit" => Ok(server
            .batch_edit(Parameters(decode_params::<BatchEditInput>(params)?))
            .await),
        "batch_rename" => Ok(server
            .batch_rename(Parameters(decode_params::<BatchRenameInput>(params)?))
            .await),
        "batch_insert" => Ok(server
            .batch_insert(Parameters(decode_params::<BatchInsertInput>(params)?))
            .await),
        "validate_file_syntax" => Ok(server
            .validate_file_syntax(Parameters(decode_params::<ValidateFileSyntaxInput>(
                params,
            )?))
            .await),
        "conventions" => Ok(server.conventions().await),
        "edit_plan" => Ok(server
            .edit_plan(Parameters(decode_params::<EditPlanInput>(params)?))
            .await),
        "investigation_suggest" => Ok(server
            .investigation_suggest(Parameters(decode_params::<InvestigationInput>(params)?))
            .await),
        "context_inventory" => Ok(server.context_inventory().await),
        "symforge_retrieve" => Ok(server
            .symforge_retrieve(Parameters(decode_params::<SymforgeRetrieveInput>(params)?))
            .await),
        "ask" => Ok(server
            .ask(Parameters(decode_params::<SmartQueryInput>(params)?))
            .await),
        "checkpoint_now" => Ok(server
            .checkpoint_now(Parameters(decode_params::<CheckpointNowInput>(params)?))
            .await),
        // TR-01 / FR-006: the front-end `status` tool proxies here so the readout
        // reflects the DAEMON's populated index (the one that actually serves
        // queries), not the empty front-end index. `status_for_daemon_session`
        // renders from this daemon server's own index/ledger/durable store and
        // does not re-apply the surface-env gate (the front-end owns that).
        "status" => Ok(server
            .status_for_daemon_session(&decode_params::<crate::stel::StelStatusRequest>(params)?)),
        other => anyhow::bail!("unknown tool '{other}'"),
    }
}

fn sidecar_state_for_runtime(runtime: &SessionRuntime) -> SidecarState {
    SidecarState {
        index: Arc::clone(&runtime.index),
        token_stats: Arc::clone(&runtime.token_stats),
        repo_root: Some(runtime.canonical_root.clone()),
        symbol_cache: Arc::clone(&runtime.symbol_cache),
    }
}

fn decode_params<T>(params: serde_json::Value) -> anyhow::Result<T>
where
    T: serde::de::DeserializeOwned,
{
    serde_json::from_value(params).context("invalid tool parameters")
}

fn bad_request(error: anyhow::Error) -> (axum::http::StatusCode, String) {
    (axum::http::StatusCode::BAD_REQUEST, error.to_string())
}

fn internal_error(error: tokio::task::JoinError) -> (axum::http::StatusCode, String) {
    (
        axum::http::StatusCode::INTERNAL_SERVER_ERROR,
        error.to_string(),
    )
}

fn canonical_project_root(root: &Path) -> anyhow::Result<PathBuf> {
    root.canonicalize()
        .with_context(|| format!("failed to canonicalize project root {}", root.display()))
}

fn project_key(root: &Path) -> String {
    let normalized = normalized_path_string(root);
    let stable_path = if cfg!(windows) {
        normalized.to_lowercase()
    } else {
        normalized
    };
    format!(
        "project-{}",
        crate::hash::digest_hex(stable_path.as_bytes())
    )
}

fn normalized_path_string(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

pub(crate) fn daemon_dir() -> io::Result<PathBuf> {
    if let Some(explicit_home) = std::env::var_os("SYMFORGE_HOME") {
        let dir = PathBuf::from(explicit_home);
        std::fs::create_dir_all(&dir)?;
        return Ok(dir);
    }

    let home = dirs::home_dir()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "home directory not found"))?;
    paths::ensure_symforge_dir(&home)
}

fn write_daemon_port_file(port: u16) -> io::Result<()> {
    let path = daemon_dir()?.join(daemon_port_file_name());
    std::fs::write(&path, port.to_string()).map_err(|e| {
        io::Error::new(
            e.kind(),
            format!("writing daemon port file at {}: {}", path.display(), e),
        )
    })
}

fn write_daemon_pid_file(pid: u32) -> io::Result<()> {
    let path = daemon_dir()?.join(daemon_pid_file_name());
    std::fs::write(&path, pid.to_string()).map_err(|e| {
        io::Error::new(
            e.kind(),
            format!("writing daemon pid file at {}: {}", path.display(), e),
        )
    })
}

fn read_daemon_pid_file() -> io::Result<u32> {
    let contents = read_daemon_runtime(
        &daemon_dir()?,
        &daemon_pid_file_name(),
        LEGACY_DAEMON_PID_FILE,
    )?;
    contents
        .trim()
        .parse::<u32>()
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

pub(crate) fn read_daemon_port_file() -> io::Result<u16> {
    let contents = read_daemon_runtime(
        &daemon_dir()?,
        &daemon_port_file_name(),
        LEGACY_DAEMON_PORT_FILE,
    )?;
    contents
        .trim()
        .parse::<u16>()
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

#[cfg(test)]
fn cleanup_daemon_files() {
    cleanup_daemon_runtime_files();
    if let Ok(dir) = daemon_dir() {
        let _ = std::fs::remove_file(dir.join(daemon_start_lock_file_name()));
        let _ = std::fs::remove_file(dir.join(LEGACY_DAEMON_START_LOCK_FILE));
    }
}

fn cleanup_daemon_runtime_files() {
    if let Ok(dir) = daemon_dir() {
        let _ = std::fs::remove_file(dir.join(daemon_port_file_name()));
        let _ = std::fs::remove_file(dir.join(daemon_pid_file_name()));
        // Remove the auth-token file too so a stale token from a previous daemon
        // can never be presented to (or mistaken for) a newly spawned daemon.
        let _ = std::fs::remove_file(dir.join(daemon_token_file_name()));
        let _ = std::fs::remove_file(dir.join(LEGACY_DAEMON_PORT_FILE));
        let _ = std::fs::remove_file(dir.join(LEGACY_DAEMON_PID_FILE));
    }
}

#[allow(unsafe_code)] // Unix process signaling requires libc::kill; Windows uses taskkill.
pub(crate) fn terminate_process(pid: u32) -> io::Result<()> {
    #[cfg(windows)]
    {
        let status = crate::process_util::hidden_command("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?;
        // Treat "process not found" (exit code 128) as success (idempotent)
        if status.success() || status.code() == Some(128) {
            Ok(())
        } else {
            Err(io::Error::other(format!(
                "taskkill exited with status {status}"
            )))
        }
    }

    #[cfg(not(windows))]
    {
        let result = unsafe { libc::kill(pid as i32, libc::SIGTERM) };
        if result == 0 {
            // Signal sent — poll briefly for exit
            for _ in 0..10 {
                std::thread::sleep(std::time::Duration::from_millis(100));
                if unsafe { libc::kill(pid as i32, 0) } != 0 {
                    return Ok(());
                }
            }
            Ok(()) // Sent signal, process may still be shutting down
        } else {
            let errno = std::io::Error::last_os_error();
            if errno.raw_os_error() == Some(libc::ESRCH) {
                Ok(()) // Already dead — idempotent success
            } else {
                Err(errno)
            }
        }
    }
}

fn unix_seconds(time: SystemTime) -> u64 {
    time.duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    use once_cell::sync::Lazy;
    use tempfile::TempDir;
    use tokio::sync::{Mutex, MutexGuard};

    static ENV_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

    fn project_dir(name: &str) -> TempDir {
        let dir = TempDir::with_prefix(name).expect("temp dir");
        std::fs::create_dir_all(dir.path().join("src")).expect("src dir");
        dir
    }

    async fn env_lock() -> MutexGuard<'static, ()> {
        ENV_LOCK.lock().await
    }

    /// Build a reqwest client that authenticates every request against the now
    /// fail-closed daemon by attaching the daemon's established token as a
    /// default `Authorization: Bearer` header. The daemon ALWAYS has a token
    /// (generated when no env pin is set), so functional tests that exercise the
    /// real handshake must present it — `authorize_daemon_request` is strict.
    fn authed_client(handle: &DaemonHandle) -> reqwest::Client {
        let token = handle.state.auth_token.clone();
        let mut headers = header::HeaderMap::new();
        let mut value =
            header::HeaderValue::from_str(&format!("Bearer {token}")).expect("valid bearer header");
        value.set_sensitive(true);
        headers.insert(header::AUTHORIZATION, value);
        reqwest::Client::builder()
            .default_headers(headers)
            .build()
            .expect("build authed reqwest client")
    }

    #[test]
    fn constant_time_eq_accepts_equal_and_rejects_unequal() {
        // Equal slices accept.
        assert!(constant_time_eq(b"sekret-token", b"sekret-token"));
        assert!(constant_time_eq(b"", b""));

        // Length mismatch rejects (and does not panic on zip of unequal lens).
        assert!(!constant_time_eq(b"short", b"longer-token"));
        assert!(!constant_time_eq(b"longer-token", b"short"));

        // Same length, differing content rejects — including a mismatch only in
        // the very last byte, which a short-circuiting `==` would reveal late.
        assert!(!constant_time_eq(b"sekret-token", b"sekret-tokeX"));
        assert!(!constant_time_eq(b"sekret-token", b"Xekret-token"));
    }

    #[test]
    fn constant_time_eq_has_no_early_out_on_first_mismatch() {
        // A correct constant-time compare must inspect the WHOLE slice even when
        // the first byte already differs. We verify there is no early return by
        // constructing two inputs that differ in byte 0 but are otherwise equal:
        // the XOR-fold accumulator must still incorporate every later byte. If
        // the implementation returned early on byte 0, flipping a later byte
        // would be unobservable; here we assert it is correctly still unequal.
        let base = vec![0u8; 64];
        let mut differ_first = base.clone();
        differ_first[0] = 1;
        assert!(!constant_time_eq(&base, &differ_first));

        // Identical except the final byte — must also reject, proving the loop
        // reaches the end rather than stopping at the first equal prefix.
        let mut differ_last = base.clone();
        differ_last[63] = 1;
        assert!(!constant_time_eq(&base, &differ_last));

        // Fully equal long slice still accepts after folding all bytes.
        assert!(constant_time_eq(&base, &base.clone()));
    }

    #[test]
    fn authorize_daemon_request_constant_time_token_check() {
        fn state_with_token(token: &str) -> DaemonState {
            DaemonState::with_token(token.to_string())
        }

        fn bearer_headers(value: &str) -> HeaderMap {
            let mut headers = HeaderMap::new();
            headers.insert(
                header::AUTHORIZATION,
                header::HeaderValue::from_str(value).expect("valid header value"),
            );
            headers
        }

        let state = state_with_token("the-real-token-value");

        // Correct token accepts.
        assert!(
            authorize_daemon_request(&state, &bearer_headers("Bearer the-real-token-value"))
                .is_ok()
        );

        // Wrong token (same length) still 401s — fail-closed preserved.
        assert_eq!(
            authorize_daemon_request(&state, &bearer_headers("Bearer the-real-token-WRONG")),
            Err(StatusCode::UNAUTHORIZED)
        );

        // Empty token still 401s.
        assert_eq!(
            authorize_daemon_request(&state, &bearer_headers("Bearer ")),
            Err(StatusCode::UNAUTHORIZED)
        );

        // Missing Authorization header still 401s.
        assert_eq!(
            authorize_daemon_request(&state, &HeaderMap::new()),
            Err(StatusCode::UNAUTHORIZED)
        );

        // Wrong scheme still 401s.
        assert_eq!(
            authorize_daemon_request(&state, &bearer_headers("Basic the-real-token-value")),
            Err(StatusCode::UNAUTHORIZED)
        );
    }

    async fn wait_for_path_absent(path: &Path) {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        while path.exists() {
            if tokio::time::Instant::now() >= deadline {
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }

    async fn spawn_fake_health_server(
        health: DaemonHealth,
    ) -> (u16, tokio::sync::oneshot::Sender<()>) {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind fake daemon");
        let port = listener.local_addr().expect("listener addr").port();
        let app = Router::new().route(
            "/health",
            get({
                let health = health.clone();
                move || {
                    let health = health.clone();
                    async move { Json(health) }
                }
            }),
        );
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            let shutdown = async move {
                let _ = shutdown_rx.await;
            };
            let _ = axum::serve(listener, app)
                .with_graceful_shutdown(shutdown)
                .await;
        });
        (port, shutdown_tx)
    }

    struct EnvVarGuard {
        key: &'static str,
        previous: Option<OsString>,
    }

    #[allow(unsafe_code)] // test-only env guard serializes process env mutation.
    impl EnvVarGuard {
        fn set(key: &'static str, value: &std::path::Path) -> Self {
            let previous = std::env::var_os(key);
            // SAFETY: called only in single-threaded test context; no concurrent env readers.
            unsafe {
                std::env::set_var(key, value);
            }
            Self { key, previous }
        }

        fn set_str(key: &'static str, value: &str) -> Self {
            let previous = std::env::var_os(key);
            // SAFETY: called only in single-threaded test context; no concurrent env readers.
            unsafe {
                std::env::set_var(key, value);
            }
            Self { key, previous }
        }

        fn unset(key: &'static str) -> Self {
            let previous = std::env::var_os(key);
            // SAFETY: called only in single-threaded test context; no concurrent env readers.
            unsafe {
                std::env::remove_var(key);
            }
            Self { key, previous }
        }
    }

    #[allow(unsafe_code)] // test-only env guard restores serialized process env mutation.
    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            match &self.previous {
                // SAFETY: called only in single-threaded test context; no concurrent env readers.
                Some(previous) => unsafe {
                    std::env::set_var(self.key, previous);
                },
                // SAFETY: called only in single-threaded test context; no concurrent env readers.
                None => unsafe {
                    std::env::remove_var(self.key);
                },
            }
        }
    }

    struct CwdGuard {
        previous: PathBuf,
    }

    impl CwdGuard {
        fn set(path: &Path) -> Self {
            let previous = std::env::current_dir().expect("current dir");
            std::env::set_current_dir(path).expect("set current dir");
            Self { previous }
        }
    }

    impl Drop for CwdGuard {
        fn drop(&mut self) {
            if std::env::set_current_dir(&self.previous).is_err() {
                std::env::set_current_dir(env!("CARGO_MANIFEST_DIR"))
                    .expect("manifest dir must be a valid cwd fallback");
            }
        }
    }

    #[tokio::test]
    async fn test_daemon_bind_policy_rejects_non_loopback_without_opt_in() {
        let _env_lock = env_lock().await;
        let _bind_guard = EnvVarGuard::set_str(DAEMON_BIND_ENV, "0.0.0.0");
        let _allow_guard = EnvVarGuard::unset(DAEMON_ALLOW_NON_LOOPBACK_ENV);

        let error = resolve_daemon_bind_host("127.0.0.1")
            .expect_err("non-loopback bind must require explicit opt-in");
        let message = error.to_string();
        assert!(
            message.contains("refusing non-loopback daemon bind"),
            "unexpected bind rejection: {message}"
        );
        assert!(
            message.contains(DAEMON_ALLOW_NON_LOOPBACK_ENV),
            "rejection should name the explicit opt-in env var: {message}"
        );
    }

    #[tokio::test]
    async fn test_daemon_bind_policy_allows_non_loopback_with_explicit_opt_in() {
        let _env_lock = env_lock().await;
        let _bind_guard = EnvVarGuard::set_str(DAEMON_BIND_ENV, "0.0.0.0");
        let _allow_guard = EnvVarGuard::set_str(DAEMON_ALLOW_NON_LOOPBACK_ENV, "1");

        let resolved = resolve_daemon_bind_host("127.0.0.1")
            .expect("explicit opt-in should allow non-loopback bind");
        assert_eq!(resolved, "0.0.0.0");
    }

    #[test]
    fn test_open_same_root_reuses_project_instance() {
        let project = project_dir("symforge-daemon-a");
        let state = DaemonState::new();

        let first = state
            .open_project_session(OpenProjectRequest {
                project_root: project.path().display().to_string(),
                client_name: "claude".to_string(),
                pid: Some(100),
            })
            .expect("first session");
        let second = state
            .open_project_session(OpenProjectRequest {
                project_root: project.path().join(".").display().to_string(),
                client_name: "codex".to_string(),
                pid: Some(200),
            })
            .expect("second session");

        assert_eq!(first.project_id, second.project_id);
        assert_ne!(first.session_id, second.session_id);

        let projects = state.list_projects();
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].session_count, 2);
    }

    #[test]
    fn test_open_distinct_roots_creates_distinct_projects() {
        let project_a = project_dir("symforge-daemon-b");
        let project_b = project_dir("symforge-daemon-c");
        let state = DaemonState::new();

        let first = state
            .open_project_session(OpenProjectRequest {
                project_root: project_a.path().display().to_string(),
                client_name: "claude".to_string(),
                pid: None,
            })
            .expect("first project");
        let second = state
            .open_project_session(OpenProjectRequest {
                project_root: project_b.path().display().to_string(),
                client_name: "codex".to_string(),
                pid: None,
            })
            .expect("second project");

        assert_ne!(first.project_id, second.project_id);
        assert_eq!(state.list_projects().len(), 2);
        assert_eq!(state.health().session_count, 2);
    }

    /// Test-only: pull the single interned `Arc<IndexBase>` a session's working
    /// set was seeded with (Feature 012, Phase 0/1). Panics if the session or its
    /// seeded entry is missing — both are invariants the open path guarantees.
    fn session_seeded_base(state: &DaemonState, session_id: &str) -> Arc<IndexBase> {
        let sessions = state.sessions.read();
        let session = sessions.get(session_id).expect("session present");
        let working_set = session.working_set.read();
        assert_eq!(
            working_set.len(),
            1,
            "Phase 1 seeds exactly one working-set entry"
        );
        let entry = working_set
            .get(&session.active_project_id)
            .expect("seeded entry for the active project");
        Arc::clone(&entry.base)
    }

    // ── Feature 012 Phase 0 — base interning shares one Arc<IndexBase> (SC-002) ──
    #[test]
    fn test_open_same_root_twice_shares_interned_base() {
        let project = project_dir("symforge-daemon-base-intern");
        let state = DaemonState::new();

        let first = state
            .open_project_session(OpenProjectRequest {
                project_root: project.path().display().to_string(),
                client_name: "claude".to_string(),
                pid: Some(11),
            })
            .expect("first session");
        let second = state
            .open_project_session(OpenProjectRequest {
                // Same logical root via a `.` component -> same canonical root,
                // same BaseKey -> MUST intern to the same Arc<IndexBase>.
                project_root: project.path().join(".").display().to_string(),
                client_name: "codex".to_string(),
                pid: Some(22),
            })
            .expect("second session");

        assert_eq!(
            first.project_id, second.project_id,
            "same canonical root -> same project"
        );

        let base_first = session_seeded_base(&state, &first.session_id);
        let base_second = session_seeded_base(&state, &second.session_id);

        // SC-002: two opens on the same (root, commit) share ONE base allocation.
        assert!(
            Arc::ptr_eq(&base_first, &base_second),
            "two sessions on the same (root, commit) must share one Arc<IndexBase>"
        );
        // The intern table holds exactly one base for the single key.
        assert_eq!(
            state.bases.read().len(),
            1,
            "one canonical root -> one interned base"
        );
    }

    // ── Feature 012 Phase 0 — distinct roots intern distinct bases ──────────────
    #[test]
    fn test_open_distinct_roots_intern_distinct_bases() {
        let project_a = project_dir("symforge-daemon-base-distinct-a");
        let project_b = project_dir("symforge-daemon-base-distinct-b");
        let state = DaemonState::new();

        let first = state
            .open_project_session(OpenProjectRequest {
                project_root: project_a.path().display().to_string(),
                client_name: "claude".to_string(),
                pid: None,
            })
            .expect("project a session");
        let second = state
            .open_project_session(OpenProjectRequest {
                project_root: project_b.path().display().to_string(),
                client_name: "codex".to_string(),
                pid: None,
            })
            .expect("project b session");

        let base_a = session_seeded_base(&state, &first.session_id);
        let base_b = session_seeded_base(&state, &second.session_id);

        assert!(
            !Arc::ptr_eq(&base_a, &base_b),
            "distinct roots must NOT share a base"
        );
        assert_ne!(
            base_a.key, base_b.key,
            "distinct roots -> distinct BaseKeys"
        );
        assert_eq!(
            state.bases.read().len(),
            2,
            "two roots -> two interned bases"
        );
    }

    // ── Feature 012 Phase 1 — seeded working set is single-entry with an EMPTY
    //    overlay (the no-overlay-writes invariant) ───────────────────────────────
    #[test]
    fn test_session_working_set_seeded_single_entry_empty_overlay() {
        let project = project_dir("symforge-daemon-ws-seed");
        let state = DaemonState::new();
        let opened = state
            .open_project_session(OpenProjectRequest {
                project_root: project.path().display().to_string(),
                client_name: "claude".to_string(),
                pid: None,
            })
            .expect("session");

        let sessions = state.sessions.read();
        let session = sessions.get(&opened.session_id).expect("session present");
        assert_eq!(
            session.active_project_id, opened.project_id,
            "active project id is the opened project"
        );
        let working_set = session.working_set.read();
        assert_eq!(working_set.len(), 1, "exactly one seeded entry");
        let entry = working_set
            .get(&opened.project_id)
            .expect("seeded entry keyed by the active project");
        // INVARIANT: the seeded overlay is EMPTY (no US1 code path writes one).
        assert_eq!(
            entry.overlay.delta_count(),
            0,
            "seeded overlay must be empty (no-overlay-writes invariant)"
        );
        // The seeded overlay is fenced to the seeded base.
        assert!(
            entry.overlay.is_valid_against(&entry.base),
            "seeded overlay must be fenced to its base"
        );
    }

    // ── Feature 012 Phase 0/1 — multi-threaded lock stress (critique #4) ─────────
    //
    // The mandated `--test-threads=1` suite CANNOT surface a deadlock on the new
    // `bases -> projects -> sessions` hot path, so THIS test spins REAL OS threads
    // that hammer the lock acquisition order concurrently. Threads churn:
    //   * `open_project_session` (intern_base -> projects.write -> sessions.write),
    //   * `session_runtime` (projects.read -> sessions.read, the hot read path),
    //   * `close_session` (projects.write -> sessions.write),
    //   * `health` / `list_projects` (independent reads),
    // over a SHARED pool of roots so the SAME BaseKey is interned from many threads
    // (the contended path) AND distinct roots are interned (table growth). A
    // lock-order inversion would deadlock; a panic in any thread is propagated by
    // `join().expect(..)`. The test passing == no deadlock and no panic under load.
    #[test]
    fn test_concurrent_session_lifecycle_no_deadlock_or_panic() {
        use std::sync::Arc as StdArc;

        // A shared pool of roots: same roots reused across threads force repeated
        // interning of identical BaseKeys; the per-thread index adds fresh roots.
        let shared_roots: Vec<TempDir> = (0..4)
            .map(|i| project_dir(&format!("symforge-stress-shared-{i}")))
            .collect();
        let shared_root_paths: StdArc<Vec<String>> = StdArc::new(
            shared_roots
                .iter()
                .map(|d| d.path().display().to_string())
                .collect(),
        );

        let state = StdArc::new(DaemonState::new());

        const THREADS: usize = 8;
        const ITERS: usize = 40;

        let mut handles = Vec::with_capacity(THREADS);
        for t in 0..THREADS {
            let state = StdArc::clone(&state);
            let shared_root_paths = StdArc::clone(&shared_root_paths);
            // Each thread also owns a private root so distinct-root interning and
            // project churn (load -> activate -> remove on last close) is exercised.
            let private_root = project_dir(&format!("symforge-stress-private-{t}"));
            let private_path = private_root.path().display().to_string();
            handles.push(std::thread::spawn(move || {
                // Keep the TempDir alive for the whole thread body.
                let _private_root = private_root;
                for i in 0..ITERS {
                    // Alternate between a shared root (contended intern of the same
                    // BaseKey) and this thread's private root (table growth/churn).
                    let root = if i % 2 == 0 {
                        shared_root_paths[(t + i) % shared_root_paths.len()].clone()
                    } else {
                        private_path.clone()
                    };

                    // PRE-EXISTING benign race (NOT a Feature 012 regression): the
                    // fast path of `open_project_session` checks `projects` under a
                    // read lock, drops it, then re-acquires write to register the
                    // session; a concurrent `close_session` can remove the project
                    // (its last session closed) in that window, so the open fails
                    // loud with "was removed between check and session registration".
                    // This `Err` (confirmed present in `HEAD` before this branch)
                    // is fail-LOUD, not a deadlock or corruption, so it is OUTSIDE
                    // this test's mandate (no deadlock / no panic on the new
                    // `bases -> projects -> sessions` locks). We tolerate it with a
                    // bounded retry; if every attempt loses the race we skip the
                    // iteration. A genuine lock-order inversion would instead HANG
                    // (caught at join) and never reach this branch.
                    let mut opened = None;
                    for _ in 0..16 {
                        match state.open_project_session(OpenProjectRequest {
                            project_root: root.clone(),
                            client_name: "stress".to_string(),
                            pid: Some((t * 1000 + i) as u32),
                        }) {
                            Ok(resp) => {
                                opened = Some(resp);
                                break;
                            }
                            // Only the known open-vs-close race is tolerated; any
                            // other error is a real failure and must surface.
                            Err(err) => {
                                let msg = err.to_string();
                                assert!(
                                    msg.contains(
                                        "was removed between check and session registration"
                                    ),
                                    "unexpected open failure under contention: {msg}"
                                );
                                std::thread::yield_now();
                            }
                        }
                    }
                    let Some(opened) = opened else {
                        // Lost the benign race on every attempt this iteration.
                        continue;
                    };

                    // Hot read path: resolves the active project via the new field.
                    // (May be `None` if a concurrent close already tore the project
                    // down — also benign and not a deadlock.)
                    let _ = state.session_runtime(&opened.session_id);

                    // Independent reads exercised concurrently with writers.
                    let _ = state.health();
                    let _ = state.list_projects();
                    let _ = state.list_sessions(&opened.project_id);

                    // Close: projects.write -> sessions.write (the order under test).
                    let _ = state.close_session(&opened.session_id);
                }
            }));
        }

        for handle in handles {
            // A deadlock would hang here (caught by the harness/CI timeout); a panic
            // in any worker is surfaced as a test failure via expect.
            handle.join().expect("stress worker thread panicked");
        }

        // Keep the shared TempDirs alive until all threads have joined.
        drop(shared_roots);

        // After every session is closed, no projects or sessions should remain.
        assert_eq!(
            state.health().session_count,
            0,
            "all sessions closed after the stress loop"
        );
        assert!(
            state.list_projects().is_empty(),
            "all projects removed after their last session closed"
        );
    }

    #[test]
    fn test_index_folder_for_session_refuses_sensitive_root() {
        // Regression: the daemon path historically never called the sensitive
        // -path guard, so a daemon-routed `index_folder` on a system root could
        // index system files and drive a reload into a denial-of-service.
        let project = project_dir("symforge-daemon-sensitive");
        let state = DaemonState::new();
        let opened = state
            .open_project_session(OpenProjectRequest {
                project_root: project.path().display().to_string(),
                client_name: "claude".to_string(),
                pid: None,
            })
            .expect("session");

        // A sensitive system root that exists and canonicalizes on this host.
        #[cfg(windows)]
        let sensitive = r"C:\Windows";
        #[cfg(unix)]
        let sensitive = "/etc";

        let result = state
            .index_folder_for_session(
                &opened.session_id,
                IndexFolderInput {
                    path: sensitive.to_string(),
                    idempotency_key: None,
                },
            )
            .expect("refusal must be a clean Ok response, never an Err or panic");
        assert!(
            result.contains("Refused to index sensitive system path"),
            "daemon path must refuse sensitive root `{sensitive}`, got: {result}"
        );

        // The session must remain bound to the original project (no reassign).
        let projects = state.list_projects();
        assert_eq!(
            projects.len(),
            1,
            "refusal must not create a project for the sensitive root"
        );
    }

    #[test]
    fn test_open_project_session_refuses_sensitive_root() {
        // Regression: `open_project_session` performs a full `LiveIndex::load`
        // with no guard of its own. A session-open on a sensitive root must be
        // refused cleanly (an `Err`, never a panic, never a load) before any IO.
        let state = DaemonState::new();

        // A sensitive system root that exists and canonicalizes on this host.
        #[cfg(windows)]
        let sensitive = r"C:\Windows";
        #[cfg(unix)]
        let sensitive = "/etc";

        let result = state.open_project_session(OpenProjectRequest {
            project_root: sensitive.to_string(),
            client_name: "claude".to_string(),
            pid: None,
        });

        let err = result.expect_err("sensitive root must be refused, not opened");
        let msg = err.to_string();
        assert!(
            msg.contains("Refused to open session for sensitive system path"),
            "open_project_session must refuse sensitive root `{sensitive}`, got: {msg}"
        );

        // The refusal must not have created any project.
        assert!(
            state.list_projects().is_empty(),
            "refusal must not create a project for the sensitive root"
        );
    }

    #[test]
    fn test_close_session_removes_project_when_last_session_leaves() {
        let project = project_dir("symforge-daemon-d");
        let state = DaemonState::new();

        let first = state
            .open_project_session(OpenProjectRequest {
                project_root: project.path().display().to_string(),
                client_name: "claude".to_string(),
                pid: None,
            })
            .expect("first session");
        let second = state
            .open_project_session(OpenProjectRequest {
                project_root: project.path().display().to_string(),
                client_name: "codex".to_string(),
                pid: None,
            })
            .expect("second session");

        let close_first = state
            .close_session(&first.session_id)
            .expect("close first session");
        assert_eq!(close_first.remaining_sessions, 1);
        assert!(!close_first.project_removed);
        assert_eq!(state.list_projects().len(), 1);

        let close_second = state
            .close_session(&second.session_id)
            .expect("close second session");
        assert_eq!(close_second.remaining_sessions, 0);
        assert!(close_second.project_removed);
        assert!(state.list_projects().is_empty());
    }

    #[test]
    fn test_close_session_handles_orphan_session_record_without_panic() {
        let state = DaemonState::new();
        let session_id = "stale-session".to_string();
        state.sessions.write().insert(
            session_id.clone(),
            SessionRecord {
                session_id: session_id.clone(),
                active_project_id: "missing-project".to_string(),
                working_set: Arc::new(RwLock::new(WorkingSet::new())),
                client_name: "codex".to_string(),
                pid: Some(777),
                opened_at: SystemTime::now(),
                last_seen_at: AtomicU64::new(now_epoch_millis()),
            },
        );

        let closed = state
            .close_session(&session_id)
            .expect("orphan session should still close");
        assert_eq!(closed.session_id, session_id);
        assert_eq!(closed.project_id, "orphan");
        assert_eq!(closed.remaining_sessions, 0);
        assert!(!closed.project_removed);
        assert!(state.close_session(&closed.session_id).is_none());
    }

    #[test]
    fn test_heartbeat_updates_known_session() {
        let project = project_dir("symforge-daemon-e");
        let state = DaemonState::new();

        let opened = state
            .open_project_session(OpenProjectRequest {
                project_root: project.path().display().to_string(),
                client_name: "claude".to_string(),
                pid: Some(123),
            })
            .expect("session");

        let known = state.heartbeat(&opened.session_id);
        let unknown = state.heartbeat("missing-session");

        assert!(known.known_session);
        assert!(!unknown.known_session);
    }

    #[test]
    fn test_project_health_and_sessions_expose_instance_metadata() {
        let project = project_dir("symforge-daemon-f");
        let state = DaemonState::new();

        let first = state
            .open_project_session(OpenProjectRequest {
                project_root: project.path().display().to_string(),
                client_name: "claude".to_string(),
                pid: Some(111),
            })
            .expect("first session");
        let second = state
            .open_project_session(OpenProjectRequest {
                project_root: project.path().display().to_string(),
                client_name: "codex".to_string(),
                pid: Some(222),
            })
            .expect("second session");

        let health = state
            .project_health(&first.project_id)
            .expect("project health should exist");
        assert_eq!(health.project_id, first.project_id);
        assert_eq!(health.session_count, 2);
        assert_eq!(health.index_state, "Ready");

        let sessions = state
            .list_sessions(&first.project_id)
            .expect("session list should exist");
        assert_eq!(sessions.len(), 2);
        assert_eq!(sessions[0].project_id, first.project_id);
        assert!(
            sessions
                .iter()
                .any(|session| session.client_name == "claude")
        );
        assert!(
            sessions
                .iter()
                .any(|session| session.client_name == "codex")
        );
        assert!(sessions.iter().any(|session| session.pid == Some(111)));
        assert!(sessions.iter().any(|session| session.pid == Some(222)));
        assert_ne!(first.session_id, second.session_id);
    }

    #[tokio::test]
    async fn test_spawn_daemon_serves_project_and_session_endpoints() {
        let _env_lock = env_lock().await;
        let daemon_home = TempDir::new().expect("daemon home");
        let _env_guard = EnvVarGuard::set("SYMFORGE_HOME", daemon_home.path());
        let project = project_dir("symforge-daemon-http");

        let handle = spawn_daemon("127.0.0.1").await.expect("spawn daemon");
        let client = authed_client(&handle);
        let base_url = format!("http://127.0.0.1:{}", handle.port);

        let daemon_health = client
            .get(format!("{base_url}/health"))
            .send()
            .await
            .expect("health request")
            .error_for_status()
            .expect("health status")
            .json::<DaemonHealth>()
            .await
            .expect("health body");
        assert_eq!(daemon_health.project_count, 0);
        assert_eq!(daemon_health.session_count, 0);
        assert_eq!(daemon_health.daemon_version, env!("CARGO_PKG_VERSION"));
        // Fail-closed: the daemon always establishes a token, so auth is always
        // required even when no env pin was set (this test sets none).
        assert!(daemon_health.auth_required);
        assert!(!daemon_health.executable_path.is_empty());

        let opened = client
            .post(format!("{base_url}/v1/sessions/open"))
            .json(&OpenProjectRequest {
                project_root: project.path().display().to_string(),
                client_name: "codex".to_string(),
                pid: Some(4242),
            })
            .send()
            .await
            .expect("open request")
            .error_for_status()
            .expect("open status")
            .json::<OpenProjectResponse>()
            .await
            .expect("open body");

        let project_health = client
            .get(format!(
                "{base_url}/v1/projects/{}/health",
                opened.project_id
            ))
            .send()
            .await
            .expect("project health request")
            .error_for_status()
            .expect("project health status")
            .json::<ProjectHealth>()
            .await
            .expect("project health body");
        assert_eq!(project_health.project_id, opened.project_id);
        assert_eq!(project_health.session_count, 1);
        assert_eq!(project_health.index_state, "Ready");

        let sessions = client
            .get(format!(
                "{base_url}/v1/projects/{}/sessions",
                opened.project_id
            ))
            .send()
            .await
            .expect("sessions request")
            .error_for_status()
            .expect("sessions status")
            .json::<Vec<SessionSummary>>()
            .await
            .expect("sessions body");
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].session_id, opened.session_id);
        assert_eq!(sessions[0].client_name, "codex");
        assert_eq!(sessions[0].pid, Some(4242));

        let heartbeat = client
            .post(format!(
                "{base_url}/v1/sessions/{}/heartbeat",
                opened.session_id
            ))
            .send()
            .await
            .expect("heartbeat request")
            .error_for_status()
            .expect("heartbeat status")
            .json::<HeartbeatResponse>()
            .await
            .expect("heartbeat body");
        assert!(heartbeat.known_session);

        let closed = client
            .delete(format!("{base_url}/v1/sessions/{}", opened.session_id))
            .send()
            .await
            .expect("close request")
            .error_for_status()
            .expect("close status")
            .json::<CloseSessionResponse>()
            .await
            .expect("close body");
        assert!(closed.project_removed);
        assert_eq!(closed.remaining_sessions, 0);

        let final_health = client
            .get(format!("{base_url}/health"))
            .send()
            .await
            .expect("final health request")
            .error_for_status()
            .expect("final health status")
            .json::<DaemonHealth>()
            .await
            .expect("final health body");
        assert_eq!(final_health.project_count, 0);
        assert_eq!(final_health.session_count, 0);
        assert_eq!(final_health.daemon_version, env!("CARGO_PKG_VERSION"));
        assert!(!final_health.executable_path.is_empty());

        let _ = handle.shutdown_tx.send(());
        wait_for_path_absent(&daemon_home.path().join(LEGACY_DAEMON_PORT_FILE)).await;
        wait_for_path_absent(&daemon_home.path().join(LEGACY_DAEMON_PID_FILE)).await;
        assert!(
            !daemon_home.path().join(LEGACY_DAEMON_PORT_FILE).exists(),
            "daemon port file should be removed on shutdown"
        );
        assert!(
            !daemon_home.path().join(LEGACY_DAEMON_PID_FILE).exists(),
            "daemon pid file should be removed on shutdown"
        );
    }

    #[tokio::test]
    async fn test_daemon_auth_token_protects_session_tool_and_sidecar_routes() {
        let _env_lock = env_lock().await;
        let daemon_home = TempDir::new().expect("daemon home");
        let _home_guard = EnvVarGuard::set("SYMFORGE_HOME", daemon_home.path());
        let _bind_guard = EnvVarGuard::unset(DAEMON_BIND_ENV);
        let _allow_guard = EnvVarGuard::unset(DAEMON_ALLOW_NON_LOOPBACK_ENV);
        let token = "sfr03-test-token";
        let _auth_guard = EnvVarGuard::set_str(DAEMON_AUTH_TOKEN_ENV, token);
        let project = project_dir("symforge-daemon-auth");

        let handle = spawn_daemon("127.0.0.1").await.expect("spawn daemon");
        // This test drives auth explicitly (missing / wrong / correct token), so
        // it uses a bare client and attaches `.bearer_auth` per request rather
        // than the always-authed `authed_client` helper.
        let client = reqwest::Client::new();
        let base_url = format!("http://127.0.0.1:{}", handle.port);

        let health_body = client
            .get(format!("{base_url}/health"))
            .send()
            .await
            .expect("health request")
            .error_for_status()
            .expect("health status")
            .text()
            .await
            .expect("health body");
        assert!(
            !health_body.contains(token),
            "health body must not leak auth token"
        );
        let daemon_health: DaemonHealth = serde_json::from_str(&health_body).expect("health json");
        assert!(daemon_health.auth_required);

        let missing_projects = client
            .get(format!("{base_url}/v1/projects"))
            .send()
            .await
            .expect("projects request");
        assert_eq!(missing_projects.status(), StatusCode::UNAUTHORIZED);
        let missing_body = missing_projects.text().await.expect("missing auth body");
        assert!(
            !missing_body.contains(token),
            "401 body must not leak auth token"
        );

        let wrong_open = client
            .post(format!("{base_url}/v1/sessions/open"))
            .bearer_auth("wrong-token")
            .json(&OpenProjectRequest {
                project_root: project.path().display().to_string(),
                client_name: "codex".to_string(),
                pid: Some(5150),
            })
            .send()
            .await
            .expect("wrong auth open request");
        assert_eq!(wrong_open.status(), StatusCode::UNAUTHORIZED);
        let wrong_body = wrong_open.text().await.expect("wrong auth body");
        assert!(
            !wrong_body.contains(token),
            "wrong-token body must not leak auth token"
        );

        let opened = client
            .post(format!("{base_url}/v1/sessions/open"))
            .bearer_auth(token)
            .json(&OpenProjectRequest {
                project_root: project.path().display().to_string(),
                client_name: "codex".to_string(),
                pid: Some(5150),
            })
            .send()
            .await
            .expect("open request")
            .error_for_status()
            .expect("open status")
            .json::<OpenProjectResponse>()
            .await
            .expect("open body");

        client
            .get(format!("{base_url}/v1/projects"))
            .bearer_auth(token)
            .send()
            .await
            .expect("authorized projects request")
            .error_for_status()
            .expect("authorized projects status");

        let missing_tool = client
            .post(format!(
                "{base_url}/v1/sessions/{}/tools/health",
                opened.session_id
            ))
            .json(&serde_json::json!({}))
            .send()
            .await
            .expect("missing tool auth request");
        assert_eq!(missing_tool.status(), StatusCode::UNAUTHORIZED);

        let tool_body = client
            .post(format!(
                "{base_url}/v1/sessions/{}/tools/health",
                opened.session_id
            ))
            .bearer_auth(token)
            .json(&serde_json::json!({}))
            .send()
            .await
            .expect("authorized tool request")
            .error_for_status()
            .expect("authorized tool status")
            .text()
            .await
            .expect("authorized tool body");
        assert!(
            tool_body.contains("Status:"),
            "health tool response should succeed: {tool_body}"
        );
        assert!(
            !tool_body.contains(token),
            "tool response must not leak auth token"
        );

        let missing_sidecar = client
            .get(format!(
                "{base_url}/v1/sessions/{}/sidecar/health",
                opened.session_id
            ))
            .send()
            .await
            .expect("missing sidecar auth request");
        assert_eq!(missing_sidecar.status(), StatusCode::UNAUTHORIZED);

        client
            .get(format!(
                "{base_url}/v1/sessions/{}/sidecar/health",
                opened.session_id
            ))
            .bearer_auth(token)
            .send()
            .await
            .expect("authorized sidecar request")
            .error_for_status()
            .expect("authorized sidecar status");

        client
            .post(format!(
                "{base_url}/v1/sessions/{}/heartbeat",
                opened.session_id
            ))
            .bearer_auth(token)
            .send()
            .await
            .expect("authorized heartbeat request")
            .error_for_status()
            .expect("authorized heartbeat status");

        client
            .delete(format!("{base_url}/v1/sessions/{}", opened.session_id))
            .bearer_auth(token)
            .send()
            .await
            .expect("authorized close request")
            .error_for_status()
            .expect("authorized close status");

        let _ = handle.shutdown_tx.send(());
        wait_for_path_absent(&daemon_home.path().join(LEGACY_DAEMON_PORT_FILE)).await;
    }

    #[tokio::test]
    async fn test_daemon_is_fail_closed_without_env_token() {
        // Item-2 acceptance: with NO env pin, the daemon must STILL establish a
        // token, persist it to the token file, reject unauthenticated requests,
        // and accept requests bearing the file-resolved token.
        let _env_lock = env_lock().await;
        let daemon_home = TempDir::new().expect("daemon home");
        let _home_guard = EnvVarGuard::set("SYMFORGE_HOME", daemon_home.path());
        let _bind_guard = EnvVarGuard::unset(DAEMON_BIND_ENV);
        let _allow_guard = EnvVarGuard::unset(DAEMON_ALLOW_NON_LOOPBACK_ENV);
        // Critical: no env token. Previously this meant "auth disabled".
        let _auth_guard = EnvVarGuard::unset(DAEMON_AUTH_TOKEN_ENV);
        let project = project_dir("symforge-daemon-failclosed");

        let handle = spawn_daemon("127.0.0.1").await.expect("spawn daemon");
        let base_url = format!("http://127.0.0.1:{}", handle.port);

        // 1. A token file is always present after start.
        let token_path = daemon_home.path().join(daemon_token_file_name());
        assert!(
            token_path.exists(),
            "daemon must persist an auth token file even without an env pin"
        );
        let file_token = std::fs::read_to_string(&token_path)
            .expect("read token file")
            .trim()
            .to_string();
        assert!(!file_token.is_empty(), "persisted token must be non-empty");
        assert_eq!(
            file_token, handle.state.auth_token,
            "the persisted token must match the token the daemon enforces"
        );

        // The token must not leak via the unauthenticated health endpoint.
        let bare = reqwest::Client::new();
        let health_body = bare
            .get(format!("{base_url}/health"))
            .send()
            .await
            .expect("health request")
            .error_for_status()
            .expect("health status")
            .text()
            .await
            .expect("health body");
        assert!(
            !health_body.contains(&file_token),
            "health body must not leak the auth token"
        );
        let health: DaemonHealth = serde_json::from_str(&health_body).expect("health json");
        assert!(health.auth_required, "auth must always be required");

        // 2. An unauthenticated session-open is rejected.
        let unauth = bare
            .post(format!("{base_url}/v1/sessions/open"))
            .json(&OpenProjectRequest {
                project_root: project.path().display().to_string(),
                client_name: "codex".to_string(),
                pid: Some(7000),
            })
            .send()
            .await
            .expect("unauth open request");
        assert_eq!(
            unauth.status(),
            StatusCode::UNAUTHORIZED,
            "a request without the token must be rejected"
        );

        // 3. The file-resolved token authenticates successfully.
        let opened = bare
            .post(format!("{base_url}/v1/sessions/open"))
            .bearer_auth(&file_token)
            .json(&OpenProjectRequest {
                project_root: project.path().display().to_string(),
                client_name: "codex".to_string(),
                pid: Some(7000),
            })
            .send()
            .await
            .expect("authed open request")
            .error_for_status()
            .expect("authed open status")
            .json::<OpenProjectResponse>()
            .await
            .expect("authed open body");
        assert!(!opened.session_id.is_empty());

        // 4. `resolve_daemon_auth_token` (the client/hook resolution path) reads
        //    the same token from the file when no env pin is set.
        assert_eq!(
            resolve_daemon_auth_token().as_deref(),
            Some(file_token.as_str()),
            "client token resolution must read the persisted token file"
        );

        let _ = handle.shutdown_tx.send(());
        wait_for_path_absent(&daemon_home.path().join(LEGACY_DAEMON_PORT_FILE)).await;
    }

    #[tokio::test]
    async fn test_daemon_executes_session_scoped_tool_calls() {
        let _env_lock = env_lock().await;
        let daemon_home = TempDir::new().expect("daemon home");
        let _env_guard = EnvVarGuard::set("SYMFORGE_HOME", daemon_home.path());
        let project = project_dir("symforge-daemon-tool");
        std::fs::write(project.path().join("src").join("main.rs"), "fn main() {}\n")
            .expect("write source");

        let handle = spawn_daemon("127.0.0.1").await.expect("spawn daemon");
        let client = authed_client(&handle);
        let base_url = format!("http://127.0.0.1:{}", handle.port);

        let opened = client
            .post(format!("{base_url}/v1/sessions/open"))
            .json(&OpenProjectRequest {
                project_root: project.path().display().to_string(),
                client_name: "codex".to_string(),
                pid: Some(9001),
            })
            .send()
            .await
            .expect("open request")
            .error_for_status()
            .expect("open status")
            .json::<OpenProjectResponse>()
            .await
            .expect("open body");

        let response = client
            .post(format!(
                "{base_url}/v1/sessions/{}/tools/get_repo_map",
                opened.session_id
            ))
            .json(&serde_json::json!({
                "detail": "full"
            }))
            .send()
            .await
            .expect("tool request");

        assert!(
            response.status().is_success(),
            "tool endpoint should succeed, got {}",
            response.status()
        );

        let body = response.text().await.expect("tool body");
        assert!(
            body.contains("main.rs"),
            "repo outline should include the indexed file, got: {body}"
        );

        let search_files = client
            .post(format!(
                "{base_url}/v1/sessions/{}/tools/search_files",
                opened.session_id
            ))
            .json(&serde_json::json!({
                "query": "main.rs",
                "limit": 5
            }))
            .send()
            .await
            .expect("search_files request");

        assert!(
            search_files.status().is_success(),
            "search_files endpoint should succeed, got {}",
            search_files.status()
        );

        let search_files_body = search_files.text().await.expect("search_files body");
        assert!(
            search_files_body.contains("src/main.rs"),
            "search_files should return the indexed file, got: {search_files_body}"
        );

        let resolved_path = client
            .post(format!(
                "{base_url}/v1/sessions/{}/tools/search_files",
                opened.session_id
            ))
            .json(&serde_json::json!({
                "query": "main.rs",
                "resolve": true
            }))
            .send()
            .await
            .expect("search_files resolve request");

        assert!(
            resolved_path.status().is_success(),
            "search_files resolve mode should succeed, got {}",
            resolved_path.status()
        );

        let resolved_path_body = resolved_path
            .text()
            .await
            .expect("search_files resolve body");
        assert!(
            resolved_path_body.contains("src/main.rs"),
            "search_files resolve mode should return the indexed file, got: {resolved_path_body}"
        );

        let reused = client
            .post(format!("{base_url}/v1/sessions/open"))
            .json(&OpenProjectRequest {
                project_root: project.path().display().to_string(),
                client_name: "codex-reused".to_string(),
                pid: Some(9002),
            })
            .send()
            .await
            .expect("reused open request")
            .error_for_status()
            .expect("reused open status")
            .json::<OpenProjectResponse>()
            .await
            .expect("reused open body");
        assert_eq!(reused.project_id, opened.project_id);
        assert_ne!(reused.session_id, opened.session_id);

        let health = client
            .post(format!(
                "{base_url}/v1/sessions/{}/tools/health_compact",
                reused.session_id
            ))
            .json(&serde_json::json!({}))
            .send()
            .await
            .expect("health request")
            .error_for_status()
            .expect("health status")
            .text()
            .await
            .expect("health body");
        assert!(
            health.contains("Runtime: mode=daemon_reused_session"),
            "daemon health should distinguish reused daemon sessions, got: {health}"
        );
        assert!(
            health.contains(&format!("project_root={}", reused.canonical_root)),
            "daemon health should surface canonical project root, got: {health}"
        );
        assert!(
            health.contains(&format!("project_id={}", reused.project_id)),
            "daemon health should surface daemon project id, got: {health}"
        );
        assert!(
            health.contains(&format!("session_id={}", reused.session_id)),
            "daemon health should surface daemon session id, got: {health}"
        );
        assert!(
            health.contains("index_id=index-"),
            "daemon health should surface index identity, got: {health}"
        );

        let _ = handle.shutdown_tx.send(());
        wait_for_path_absent(&daemon_home.path().join(LEGACY_DAEMON_PORT_FILE)).await;
    }

    /// SF-007 regression: `checkpoint_now` must forward to the daemon in
    /// daemon-proxy mode and checkpoint the daemon's authoritative live index,
    /// instead of hard-failing with "unavailable in daemon-proxy mode" (the old
    /// proxy-side guard) or "unknown tool" (the missing daemon dispatch arm).
    #[tokio::test]
    async fn test_daemon_executes_checkpoint_now_tool_call() {
        let _env_lock = env_lock().await;
        let daemon_home = TempDir::new().expect("daemon home");
        let _env_guard = EnvVarGuard::set("SYMFORGE_HOME", daemon_home.path());
        let project = project_dir("symforge-daemon-checkpoint");
        std::fs::write(project.path().join("src").join("main.rs"), "fn main() {}\n")
            .expect("write source");

        let handle = spawn_daemon("127.0.0.1").await.expect("spawn daemon");
        let client = authed_client(&handle);
        let base_url = format!("http://127.0.0.1:{}", handle.port);

        let opened = client
            .post(format!("{base_url}/v1/sessions/open"))
            .json(&OpenProjectRequest {
                project_root: project.path().display().to_string(),
                client_name: "codex".to_string(),
                pid: Some(9101),
            })
            .send()
            .await
            .expect("open request")
            .error_for_status()
            .expect("open status")
            .json::<OpenProjectResponse>()
            .await
            .expect("open body");

        let response = client
            .post(format!(
                "{base_url}/v1/sessions/{}/tools/checkpoint_now",
                opened.session_id
            ))
            .json(&serde_json::json!({
                "verify_after_write": true
            }))
            .send()
            .await
            .expect("checkpoint_now request");

        assert!(
            response.status().is_success(),
            "checkpoint_now endpoint should succeed, got {}",
            response.status()
        );

        let body = response.text().await.expect("checkpoint_now body");
        assert!(
            body.contains("Checkpoint complete"),
            "checkpoint_now should report completion, got: {body}"
        );
        assert!(
            !body.contains("unavailable in daemon-proxy mode"),
            "checkpoint_now must not hard-fail in daemon-proxy mode, got: {body}"
        );
        assert!(
            !body.contains("unknown tool"),
            "daemon must dispatch checkpoint_now, got: {body}"
        );

        // The snapshot was written to the daemon's authoritative project root
        // (the canonical root the session is bound to) and must deserialize.
        let canonical_root = PathBuf::from(&opened.canonical_root);
        assert!(
            crate::live_index::persist::load_snapshot(&canonical_root).is_some(),
            "checkpoint should write a loadable snapshot under {}/.symforge/index.bin",
            canonical_root.display()
        );

        let _ = handle.shutdown_tx.send(());
        wait_for_path_absent(&daemon_home.path().join(LEGACY_DAEMON_PORT_FILE)).await;
    }

    #[tokio::test]
    async fn test_daemon_port_if_compatible_accepts_matching_identity() {
        let _env_lock = env_lock().await;
        let daemon_home = TempDir::new().expect("daemon home");
        let _env_guard = EnvVarGuard::set("SYMFORGE_HOME", daemon_home.path());

        let health = DaemonState::new().health();
        let (port, shutdown_tx) = spawn_fake_health_server(health).await;
        std::fs::write(
            daemon_home.path().join(LEGACY_DAEMON_PORT_FILE),
            port.to_string(),
        )
        .expect("write daemon port");

        let identity = current_daemon_identity();
        let selected = daemon_port_if_compatible(&identity)
            .await
            .expect("compatible health lookup");

        assert_eq!(selected, Some(port));

        let _ = shutdown_tx.send(());
    }

    #[tokio::test]
    async fn test_daemon_port_if_compatible_rejects_version_mismatch() {
        let _env_lock = env_lock().await;
        let daemon_home = TempDir::new().expect("daemon home");
        let _env_guard = EnvVarGuard::set("SYMFORGE_HOME", daemon_home.path());

        let health = DaemonHealth {
            project_count: 0,
            session_count: 0,
            daemon_version: "0.0.0".to_string(),
            executable_path: current_daemon_identity().executable_path,
            auth_required: false,
            pid: None,
        };
        let (port, shutdown_tx) = spawn_fake_health_server(health).await;
        std::fs::write(
            daemon_home.path().join(LEGACY_DAEMON_PORT_FILE),
            port.to_string(),
        )
        .expect("write daemon port");

        let identity = current_daemon_identity();
        let selected = daemon_port_if_compatible(&identity)
            .await
            .expect("mismatch health lookup");

        assert_eq!(selected, None);

        let _ = shutdown_tx.send(());
    }

    #[test]
    fn test_daemon_health_matches_recorded_pid_requires_exact_pid() {
        let mut health = DaemonHealth {
            project_count: 0,
            session_count: 0,
            daemon_version: current_daemon_identity().version,
            executable_path: current_daemon_identity().executable_path,
            auth_required: false,
            pid: Some(42),
        };

        assert!(daemon_health_matches_recorded_pid(&health, 42));
        assert!(!daemon_health_matches_recorded_pid(&health, 43));

        health.pid = None;
        assert!(!daemon_health_matches_recorded_pid(&health, 42));
    }

    #[tokio::test]
    async fn test_stop_incompatible_recorded_daemon_does_not_kill_unrelated_pid() {
        let _env_lock = env_lock().await;
        let daemon_home = TempDir::new().expect("daemon home");
        let _env_guard = EnvVarGuard::set("SYMFORGE_HOME", daemon_home.path());

        #[cfg(windows)]
        let mut child = std::process::Command::new("powershell")
            .args(["-NoProfile", "-Command", "Start-Sleep -Seconds 30"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn long-running child");

        #[cfg(not(windows))]
        let mut child = std::process::Command::new("sleep")
            .arg("30")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn long-running child");

        let child_pid = child.id();
        let health = DaemonHealth {
            project_count: 0,
            session_count: 0,
            daemon_version: current_daemon_identity().version,
            executable_path: "C:/unrelated/not-symforge-daemon.exe".to_string(),
            auth_required: false,
            pid: Some(child_pid),
        };
        let (port, shutdown_tx) = spawn_fake_health_server(health).await;
        std::fs::write(
            daemon_home.path().join(LEGACY_DAEMON_PORT_FILE),
            port.to_string(),
        )
        .expect("write daemon port");
        std::fs::write(
            daemon_home.path().join(LEGACY_DAEMON_PID_FILE),
            child_pid.to_string(),
        )
        .expect("write daemon pid");

        stop_incompatible_recorded_daemon(&current_daemon_identity())
            .await
            .expect("stop incompatible daemon");

        assert!(
            child.try_wait().expect("poll child").is_none(),
            "daemon pid safety checks must not terminate unrelated processes"
        );

        let _ = child.kill();
        let _ = child.wait();
        let _ = shutdown_tx.send(());
    }

    #[tokio::test]
    async fn test_stop_incompatible_recorded_daemon_cleans_port_file_without_pid() {
        let _env_lock = env_lock().await;
        let daemon_home = TempDir::new().expect("daemon home");
        let _env_guard = EnvVarGuard::set("SYMFORGE_HOME", daemon_home.path());

        let health = DaemonHealth {
            project_count: 0,
            session_count: 0,
            daemon_version: "0.0.0".to_string(),
            executable_path: current_daemon_identity().executable_path,
            auth_required: false,
            pid: None,
        };
        let (port, shutdown_tx) = spawn_fake_health_server(health).await;
        std::fs::write(
            daemon_home.path().join(LEGACY_DAEMON_PORT_FILE),
            port.to_string(),
        )
        .expect("write daemon port");

        stop_incompatible_recorded_daemon(&current_daemon_identity())
            .await
            .expect("stop incompatible daemon");

        assert!(
            !daemon_home.path().join(LEGACY_DAEMON_PORT_FILE).exists(),
            "incompatible daemon port file should be cleared"
        );

        let _ = shutdown_tx.send(());
    }

    #[tokio::test]
    async fn test_daemon_serves_session_scoped_repo_map_hook_endpoint() {
        let _env_lock = env_lock().await;
        let daemon_home = TempDir::new().expect("daemon home");
        let _env_guard = EnvVarGuard::set("SYMFORGE_HOME", daemon_home.path());
        let project = project_dir("symforge-daemon-hook");
        std::fs::write(project.path().join("src").join("main.rs"), "fn main() {}\n")
            .expect("write source");

        let handle = spawn_daemon("127.0.0.1").await.expect("spawn daemon");
        let client = authed_client(&handle);
        let base_url = format!("http://127.0.0.1:{}", handle.port);

        let opened = client
            .post(format!("{base_url}/v1/sessions/open"))
            .json(&OpenProjectRequest {
                project_root: project.path().display().to_string(),
                client_name: "claude".to_string(),
                pid: Some(77),
            })
            .send()
            .await
            .expect("open request")
            .error_for_status()
            .expect("open status")
            .json::<OpenProjectResponse>()
            .await
            .expect("open body");

        let response = client
            .get(format!(
                "{base_url}/v1/sessions/{}/sidecar/repo-map",
                opened.session_id
            ))
            .send()
            .await
            .expect("hook request");

        assert!(
            response.status().is_success(),
            "repo-map hook endpoint should succeed, got {}",
            response.status()
        );

        let body = response.text().await.expect("hook body");
        assert!(
            body.contains("Index: 1 files, 1 symbols"),
            "repo-map hook output should come from daemon project instance, got: {body}"
        );

        let _ = handle.shutdown_tx.send(());
        wait_for_path_absent(&daemon_home.path().join(LEGACY_DAEMON_PORT_FILE)).await;
    }

    #[tokio::test]
    async fn test_daemon_workflow_repo_start_endpoint_matches_repo_map_route() {
        let _env_lock = env_lock().await;
        let daemon_home = TempDir::new().expect("daemon home");
        let _env_guard = EnvVarGuard::set("SYMFORGE_HOME", daemon_home.path());
        let project = project_dir("symforge-daemon-workflow-repo-start");
        std::fs::write(project.path().join("src").join("main.rs"), "fn main() {}\n")
            .expect("write source");

        let handle = spawn_daemon("127.0.0.1").await.expect("spawn daemon");
        let client = authed_client(&handle);
        let base_url = format!("http://127.0.0.1:{}", handle.port);

        let opened = client
            .post(format!("{base_url}/v1/sessions/open"))
            .json(&OpenProjectRequest {
                project_root: project.path().display().to_string(),
                client_name: "claude".to_string(),
                pid: Some(78),
            })
            .send()
            .await
            .expect("open request")
            .error_for_status()
            .expect("open status")
            .json::<OpenProjectResponse>()
            .await
            .expect("open body");

        let canonical = client
            .get(format!(
                "{base_url}/v1/sessions/{}/sidecar/repo-map",
                opened.session_id
            ))
            .send()
            .await
            .expect("canonical request")
            .error_for_status()
            .expect("canonical status")
            .text()
            .await
            .expect("canonical body");

        let workflow = client
            .get(format!(
                "{base_url}/v1/sessions/{}/sidecar/workflows/repo-start",
                opened.session_id
            ))
            .send()
            .await
            .expect("workflow request")
            .error_for_status()
            .expect("workflow status")
            .text()
            .await
            .expect("workflow body");

        assert_eq!(
            workflow, canonical,
            "workflow repo-start route should stay identical to the canonical repo-map route"
        );

        let _ = handle.shutdown_tx.send(());
        wait_for_path_absent(&daemon_home.path().join(LEGACY_DAEMON_PORT_FILE)).await;
    }

    #[tokio::test]
    async fn test_daemon_serves_session_scoped_prompt_context_hook_endpoint() {
        let _env_lock = env_lock().await;
        let daemon_home = TempDir::new().expect("daemon home");
        let _env_guard = EnvVarGuard::set("SYMFORGE_HOME", daemon_home.path());
        let project = project_dir("symforge-daemon-prompt-hook");
        std::fs::write(project.path().join("src").join("main.rs"), "fn main() {}\n")
            .expect("write source");

        let handle = spawn_daemon("127.0.0.1").await.expect("spawn daemon");
        let client = authed_client(&handle);
        let base_url = format!("http://127.0.0.1:{}", handle.port);

        let opened = client
            .post(format!("{base_url}/v1/sessions/open"))
            .json(&OpenProjectRequest {
                project_root: project.path().display().to_string(),
                client_name: "claude".to_string(),
                pid: Some(88),
            })
            .send()
            .await
            .expect("open request")
            .error_for_status()
            .expect("open status")
            .json::<OpenProjectResponse>()
            .await
            .expect("open body");

        let response = client
            .get(format!(
                "{base_url}/v1/sessions/{}/sidecar/prompt-context",
                opened.session_id
            ))
            .query(&[("text", "please inspect src/main.rs")])
            .send()
            .await
            .expect("hook request");

        assert!(
            response.status().is_success(),
            "prompt-context hook endpoint should succeed, got {}",
            response.status()
        );

        let body = response.text().await.expect("hook body");
        assert!(
            body.contains("src/main.rs") && body.contains("main"),
            "prompt-context hook output should come from daemon project instance, got: {body}"
        );

        let _ = handle.shutdown_tx.send(());
        wait_for_path_absent(&daemon_home.path().join(LEGACY_DAEMON_PORT_FILE)).await;
    }

    #[tokio::test]
    async fn test_daemon_workflow_prompt_context_endpoint_matches_canonical_route() {
        let _env_lock = env_lock().await;
        let daemon_home = TempDir::new().expect("daemon home");
        let _env_guard = EnvVarGuard::set("SYMFORGE_HOME", daemon_home.path());
        let project = project_dir("symforge-daemon-workflow-prompt-context");
        std::fs::write(project.path().join("src").join("main.rs"), "fn main() {}\n")
            .expect("write source");

        let handle = spawn_daemon("127.0.0.1").await.expect("spawn daemon");
        let client = authed_client(&handle);
        let base_url = format!("http://127.0.0.1:{}", handle.port);

        let opened = client
            .post(format!("{base_url}/v1/sessions/open"))
            .json(&OpenProjectRequest {
                project_root: project.path().display().to_string(),
                client_name: "claude".to_string(),
                pid: Some(89),
            })
            .send()
            .await
            .expect("open request")
            .error_for_status()
            .expect("open status")
            .json::<OpenProjectResponse>()
            .await
            .expect("open body");

        let canonical = client
            .get(format!(
                "{base_url}/v1/sessions/{}/sidecar/prompt-context",
                opened.session_id
            ))
            .query(&[("text", "please inspect src/main.rs")])
            .send()
            .await
            .expect("canonical request")
            .error_for_status()
            .expect("canonical status")
            .text()
            .await
            .expect("canonical body");

        let workflow = client
            .get(format!(
                "{base_url}/v1/sessions/{}/sidecar/workflows/prompt-context",
                opened.session_id
            ))
            .query(&[("text", "please inspect src/main.rs")])
            .send()
            .await
            .expect("workflow request")
            .error_for_status()
            .expect("workflow status")
            .text()
            .await
            .expect("workflow body");

        assert_eq!(
            workflow, canonical,
            "workflow prompt-context route should stay identical to the canonical prompt-context route"
        );

        let _ = handle.shutdown_tx.send(());
        wait_for_path_absent(&daemon_home.path().join(LEGACY_DAEMON_PORT_FILE)).await;
    }

    #[tokio::test]
    async fn test_index_folder_rebinds_session_to_new_project_root() {
        let _env_lock = env_lock().await;
        let daemon_home = TempDir::new().expect("daemon home");
        let _env_guard = EnvVarGuard::set("SYMFORGE_HOME", daemon_home.path());
        let project_a = project_dir("symforge-daemon-a");
        let project_b = project_dir("symforge-daemon-b");
        std::fs::write(
            project_a.path().join("src").join("old.rs"),
            "fn old_fn() {}\n",
        )
        .expect("write source a");
        std::fs::write(
            project_b.path().join("src").join("new.rs"),
            "fn new_fn() {}\n",
        )
        .expect("write source b");

        let handle = spawn_daemon("127.0.0.1").await.expect("spawn daemon");
        let client = authed_client(&handle);
        let base_url = format!("http://127.0.0.1:{}", handle.port);

        let opened = client
            .post(format!("{base_url}/v1/sessions/open"))
            .json(&OpenProjectRequest {
                project_root: project_a.path().display().to_string(),
                client_name: "codex".to_string(),
                pid: Some(55),
            })
            .send()
            .await
            .expect("open request")
            .error_for_status()
            .expect("open status")
            .json::<OpenProjectResponse>()
            .await
            .expect("open body");

        let reload = client
            .post(format!(
                "{base_url}/v1/sessions/{}/tools/index_folder",
                opened.session_id
            ))
            .json(&IndexFolderInput {
                path: project_b.path().display().to_string(),
                idempotency_key: None,
            })
            .send()
            .await
            .expect("index request")
            .error_for_status()
            .expect("index status")
            .text()
            .await
            .expect("index body");
        assert!(
            reload.contains("Indexed"),
            "index_folder should report success, got: {reload}"
        );

        let sessions = client
            .get(format!(
                "{base_url}/v1/projects/{}/sessions",
                project_key(&canonical_project_root(project_b.path()).expect("canonical root"))
            ))
            .send()
            .await
            .expect("session list request")
            .error_for_status()
            .expect("session list status")
            .json::<Vec<SessionSummary>>()
            .await
            .expect("session list body");
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].session_id, opened.session_id);

        let outline = client
            .post(format!(
                "{base_url}/v1/sessions/{}/tools/get_repo_map",
                opened.session_id
            ))
            .json(&serde_json::json!({
                "detail": "full"
            }))
            .send()
            .await
            .expect("outline request")
            .error_for_status()
            .expect("outline status")
            .text()
            .await
            .expect("outline body");
        assert!(
            outline.contains("new.rs"),
            "rebound session should see new root: {outline}"
        );
        assert!(
            !outline.contains("old.rs"),
            "rebound session should no longer point at old root: {outline}"
        );

        let _ = handle.shutdown_tx.send(());
        wait_for_path_absent(&daemon_home.path().join(LEGACY_DAEMON_PORT_FILE)).await;
    }

    /// T016 / TR-01 / FR-006 / FR-007 (SC-002): in the default daemon-proxy
    /// topology the front-end `status` readout MUST reflect the DAEMON's
    /// populated index (the one that actually serves queries) — never the empty
    /// front-end `self.index`.
    ///
    /// This drives the REAL proxy path end-to-end:
    ///   1. spawn a daemon, open a session, build a `new_daemon_proxy`
    ///      front-end server (the exact shape `run_remote_mcp_server_async`
    ///      constructs for an MCP client);
    ///   2. index the project through the proxy so the DAEMON's index is warm;
    ///   3. run a real query (`search_symbols`) through the proxy and confirm it
    ///      serves the indexed symbol from the daemon;
    ///   4. call `status` through the front-end and assert the readout reports
    ///      `index_ready: true` with a non-zero `index_files` count.
    ///
    /// The test also asserts the front-end's OWN `self.index` stays EMPTY,
    /// proving `status` did not read it. This is the air-tight form of the bug:
    /// pre-fix, step 4 failed two ways — the daemon had no `status` dispatch arm
    /// (`unknown tool 'status'`) and the front-end read its empty index
    /// (`index_ready: false`, `index_files: 0`). Post-fix it reports the served
    /// index.
    ///
    /// Coverage limits (honest): this exercises the in-process front-end →
    /// daemon HTTP proxy on loopback, which is the production topology's data
    /// path. It does NOT exercise the OS desktop launcher / CWD discovery
    /// (TR-03, a separate phase) nor a cross-binary version skew; those remain
    /// for live-verify against a built 8.0.0 binary.
    #[tokio::test]
    async fn test_status_index_matches_daemon_proxy_after_symforge_serve() {
        let _env_lock = env_lock().await;
        let daemon_home = TempDir::new().expect("daemon home");
        let _home_guard = EnvVarGuard::set("SYMFORGE_HOME", daemon_home.path());
        // `status_stel_tool`'s front-end entry gate requires the compact surface.
        let _surface_guard = EnvVarGuard::set_str("SYMFORGE_SURFACE", "compact");

        // A project with one real Rust symbol so the daemon index is non-empty
        // and a query can demonstrably serve from it.
        let project = project_dir("symforge-status-proxy");
        std::fs::write(
            project.path().join("src").join("lib.rs"),
            "pub fn served_symbol() -> u32 { 42 }\n",
        )
        .expect("write source");

        let handle = spawn_daemon("127.0.0.1").await.expect("spawn daemon");
        let base_url = format!("http://127.0.0.1:{}", handle.port);
        let http = authed_client(&handle);

        let opened = http
            .post(format!("{base_url}/v1/sessions/open"))
            .json(&OpenProjectRequest {
                project_root: project.path().display().to_string(),
                client_name: "status-proxy".to_string(),
                pid: Some(std::process::id()),
            })
            .send()
            .await
            .expect("open request")
            .error_for_status()
            .expect("open status")
            .json::<OpenProjectResponse>()
            .await
            .expect("open body");

        // The exact front-end an MCP client gets: empty self.index, proxying to
        // the warm daemon.
        let client = DaemonSessionClient::new_for_test(
            base_url.clone(),
            opened.project_id.clone(),
            opened.session_id.clone(),
            opened.project_name.clone(),
        )
        .with_project_root(project.path().to_path_buf());
        let server = crate::protocol::SymForgeServer::new_daemon_proxy(client);

        // Index the project through the proxy → populates the DAEMON's index.
        let indexed = server
            .index_folder(Parameters(IndexFolderInput {
                path: project.path().display().to_string(),
                idempotency_key: None,
            }))
            .await;
        assert!(
            indexed.starts_with("Indexed "),
            "daemon proxy index_folder must succeed, got: {indexed}"
        );

        // Run a real query through the proxy: it must serve the indexed symbol
        // from the warm daemon index (so a successful query is established).
        // Build the input from JSON so this stays robust to optional-field
        // additions on `SearchSymbolsInput`.
        let search_input = serde_json::from_value(serde_json::json!({
            "query": "served_symbol"
        }))
        .expect("search_symbols input");
        let query = server.search_symbols(Parameters(search_input)).await;
        assert!(
            query.contains("served_symbol"),
            "proxied query must serve the indexed symbol from the warm daemon, got: {query}"
        );

        // Now read `status` through the front-end. With the TR-01 fix it proxies
        // to the daemon and reports the SERVED index.
        let status_result = server
            .status_stel_tool(Parameters(crate::stel::StelStatusRequest::default()))
            .await
            .expect("status dispatch");
        let serialized = serde_json::to_value(&status_result).expect("serialize status result");
        let status_body = serialized["content"][0]["text"]
            .as_str()
            .expect("status result text")
            .to_string();

        assert!(
            status_body.contains("index_ready: true"),
            "status must report the served daemon index as ready (FR-006), got:\n{status_body}"
        );
        // A successful query implies non-zero counts — the index_files line must
        // not be zero (SC-002).
        assert!(
            !status_body.contains("index_files: 0"),
            "status must report non-zero index_files for the served index (SC-002), got:\n{status_body}"
        );
        assert!(
            status_body.contains("index_files: 1\n"),
            "status must report exactly the single indexed file from the daemon, got:\n{status_body}"
        );

        // Air-tight: the front-end's OWN index stayed empty; `status` reported
        // the daemon's index, not this one. This is what was broken pre-fix.
        assert_eq!(
            server.index().published_state().file_count,
            0,
            "front-end self.index must remain empty — status must source from the daemon"
        );

        let _ = handle.shutdown_tx.send(());
        wait_for_path_absent(&daemon_home.path().join(LEGACY_DAEMON_PORT_FILE)).await;
    }

    /// Regression: a daemon-proxy `index_folder` switch must invalidate any
    /// stale in-process index that a prior local fallback populated for the OLD
    /// project. Without the fix, the server keeps serving the old project from
    /// every tool that falls back to local execution (search_symbols,
    /// search_text, get_file_context, conventions, explore), silently mixing
    /// two projects in one session while health/get_repo_map/index_folder
    /// follow the switch.
    #[tokio::test]
    async fn test_index_folder_proxy_switch_invalidates_stale_local_index() {
        let _env_lock = env_lock().await;
        let daemon_home = TempDir::new().expect("daemon home");
        let _env_guard = EnvVarGuard::set("SYMFORGE_HOME", daemon_home.path());
        let project_a = project_dir("symforge-proxy-stale-a");
        let project_b = project_dir("symforge-proxy-stale-b");
        std::fs::write(
            project_a.path().join("src").join("old.rs"),
            "fn old_fn() {}\n",
        )
        .expect("write source a");
        std::fs::write(
            project_b.path().join("src").join("new.rs"),
            "fn new_fn() {}\n",
        )
        .expect("write source b");

        let handle = spawn_daemon("127.0.0.1").await.expect("spawn daemon");
        let base_url = format!("http://127.0.0.1:{}", handle.port);
        let http = authed_client(&handle);

        // Open a daemon session bound to project A.
        let opened = http
            .post(format!("{base_url}/v1/sessions/open"))
            .json(&OpenProjectRequest {
                project_root: project_a.path().display().to_string(),
                client_name: "regression".to_string(),
                pid: Some(std::process::id()),
            })
            .send()
            .await
            .expect("open request")
            .error_for_status()
            .expect("open status")
            .json::<OpenProjectResponse>()
            .await
            .expect("open body");

        // Build a daemon-proxy SymForgeServer for that session — the exact shape
        // `run_remote_mcp_server_async` constructs for a real MCP client.
        let client = DaemonSessionClient::new_for_test(
            base_url.clone(),
            opened.project_id.clone(),
            opened.session_id.clone(),
            opened.project_name.clone(),
        )
        .with_project_root(project_a.path().to_path_buf());
        let server = crate::protocol::SymForgeServer::new_daemon_proxy(client);

        // Simulate a prior local-fallback load: the in-process index already
        // holds OLD-project (project A) state.
        server.index.add_file(
            "src/old.rs".to_string(),
            crate::live_index::store::IndexedFile {
                relative_path: "src/old.rs".to_string(),
                language: crate::domain::index::LanguageId::Rust,
                classification: crate::domain::FileClassification::for_code_path("src/old.rs"),
                content: b"fn old_fn() {}".to_vec(),
                symbols: vec![],
                parse_status: crate::live_index::store::ParseStatus::Parsed,
                parse_diagnostic: None,
                byte_len: 14,
                content_hash: "old-hash".to_string(),
                references: vec![],
                alias_map: std::collections::HashMap::new(),
                mtime_secs: 0,
            },
        );
        assert_eq!(
            server.index.published_state().file_count,
            1,
            "precondition: stale local index holds the OLD project"
        );

        // Switch projects via the proxy path. The daemon answers "Indexed ..."
        // and rebinds the session to project B.
        let result = server
            .index_folder(Parameters(IndexFolderInput {
                path: project_b.path().display().to_string(),
                idempotency_key: None,
            }))
            .await;
        assert!(
            result.starts_with("Indexed "),
            "daemon proxy index_folder should report success, got: {result}"
        );

        // The fix: the stale local index must be invalidated so no local
        // fallback can serve the OLD project after the switch.
        assert_eq!(
            server.index.published_state().file_count,
            0,
            "stale local index must be reset to empty after a proxy project switch, got: {result}"
        );
        assert!(
            server.index.read().get_file("src/old.rs").is_none(),
            "OLD-project file must be unreachable from the local index after switch"
        );

        // And repo_root must follow the switch so the next local fallback
        // reloads project B (not project A).
        let switched_root = server.capture_repo_root().expect("repo root after switch");
        assert_eq!(
            switched_root,
            project_b.path().to_path_buf(),
            "repo_root must point at the new project after a proxy switch"
        );

        let _ = handle.shutdown_tx.send(());
        wait_for_path_absent(&daemon_home.path().join(LEGACY_DAEMON_PORT_FILE)).await;
    }

    /// 012 C4 (D4 retarget + D6-a bound-root visibility): after a daemon-proxy
    /// session is retargeted from project A to project B at runtime via
    /// `index_folder`, the `status` and `symforge` surfaces must report the NEW
    /// project's bound root — never the stale one. This is the field wrong-repo
    /// bug made observable: a stale binding can no longer read as a working one.
    #[tokio::test]
    async fn test_retarget_updates_bound_root_in_status_and_symforge_surfaces() {
        let _env_lock = env_lock().await;
        let daemon_home = TempDir::new().expect("daemon home");
        let _env_guard = EnvVarGuard::set("SYMFORGE_HOME", daemon_home.path());
        // The facade surfaces require the compact profile.
        let _surface_guard = EnvVarGuard::set_str("SYMFORGE_SURFACE", "compact");

        let project_a = project_dir("symforge-retarget-root-a");
        let project_b = project_dir("symforge-retarget-root-b");
        std::fs::write(
            project_a.path().join("src").join("lib.rs"),
            "pub fn alpha_symbol() -> u32 { 1 }\n",
        )
        .expect("write source a");
        std::fs::write(
            project_b.path().join("src").join("lib.rs"),
            "pub fn beta_symbol() -> u32 { 2 }\n",
        )
        .expect("write source b");

        let handle = spawn_daemon("127.0.0.1").await.expect("spawn daemon");
        let base_url = format!("http://127.0.0.1:{}", handle.port);
        let http = authed_client(&handle);

        // Open a daemon session bound to project A.
        let opened = http
            .post(format!("{base_url}/v1/sessions/open"))
            .json(&OpenProjectRequest {
                project_root: project_a.path().display().to_string(),
                client_name: "retarget-root".to_string(),
                pid: Some(std::process::id()),
            })
            .send()
            .await
            .expect("open request")
            .error_for_status()
            .expect("open status")
            .json::<OpenProjectResponse>()
            .await
            .expect("open body");

        let client = DaemonSessionClient::new_for_test(
            base_url.clone(),
            opened.project_id.clone(),
            opened.session_id.clone(),
            opened.project_name.clone(),
        )
        .with_project_root(project_a.path().to_path_buf());
        let server = crate::protocol::SymForgeServer::new_daemon_proxy(client);

        // Sanity: before retarget the bound root is project A and the local
        // status render surfaces it.
        let root_a_norm = project_a.path().display().to_string().replace('\\', "/");
        let status_before =
            server.render_stel_status_body(&crate::stel::StelStatusRequest::default());
        assert!(
            status_before.contains(&format!("project_root: {root_a_norm}")),
            "pre-retarget status must surface project A root, got:\n{status_before}"
        );

        // Retarget to project B via the explicit `index_folder` verb (D4-C).
        let indexed = server
            .index_folder(Parameters(IndexFolderInput {
                path: project_b.path().display().to_string(),
                idempotency_key: None,
            }))
            .await;
        assert!(
            indexed.starts_with("Indexed "),
            "daemon proxy retarget must succeed, got: {indexed}"
        );

        // (a) The bound root followed the switch.
        let switched = server
            .capture_repo_root()
            .expect("repo root after retarget");
        assert_eq!(
            switched,
            project_b.path().to_path_buf(),
            "repo_root must point at project B after retarget"
        );

        // (b) `status` (local render path) now reflects project B's root, not A.
        let root_b_norm = project_b.path().display().to_string().replace('\\', "/");
        let status_after =
            server.render_stel_status_body(&crate::stel::StelStatusRequest::default());
        assert!(
            status_after.contains(&format!("project_root: {root_b_norm}")),
            "post-retarget status must surface project B root, got:\n{status_after}"
        );
        assert!(
            !status_after.contains(&format!("project_root: {root_a_norm}")),
            "stale project A root must NOT survive in status after retarget, got:\n{status_after}"
        );

        // (c) the `symforge` facade envelope also carries the bound root line so a
        // wrong-repo binding is loud on the primary read surface too.
        // `SymforgeCallInput` flattens the `StelRequest`, so `query` is top-level.
        let symforge_input = serde_json::from_value(serde_json::json!({
            "query": "find beta_symbol"
        }))
        .expect("symforge facade input");
        let symforge_result = server
            .symforge_facade_tool(Parameters(symforge_input))
            .await
            .expect("symforge facade dispatch");
        let serialized = serde_json::to_value(&symforge_result).expect("serialize symforge result");
        let symforge_body = serialized["content"][0]["text"]
            .as_str()
            .expect("symforge result text")
            .to_string();
        assert!(
            symforge_body.contains(&format!("project_root: {root_b_norm}")),
            "symforge envelope must surface the bound project B root, got:\n{symforge_body}"
        );

        let _ = handle.shutdown_tx.send(());
        wait_for_path_absent(&daemon_home.path().join(LEGACY_DAEMON_PORT_FILE)).await;
    }

    #[tokio::test]
    async fn test_index_folder_idempotency_replays_same_key_same_request_in_daemon_route() {
        let _env_lock = env_lock().await;
        let daemon_home = TempDir::new().expect("daemon home");
        let _env_guard = EnvVarGuard::set("SYMFORGE_HOME", daemon_home.path());
        let project_a = project_dir("symforge-daemon-idempotency-a");
        let project_b = project_dir("symforge-daemon-idempotency-b");
        std::fs::write(
            project_a.path().join("src").join("old.rs"),
            "fn old_fn() {}\n",
        )
        .expect("write source a");
        std::fs::write(
            project_b.path().join("src").join("new.rs"),
            "fn new_fn() {}\n",
        )
        .expect("write source b");

        let handle = spawn_daemon("127.0.0.1").await.expect("spawn daemon");
        let client = authed_client(&handle);
        let base_url = format!("http://127.0.0.1:{}", handle.port);

        let opened = client
            .post(format!("{base_url}/v1/sessions/open"))
            .json(&OpenProjectRequest {
                project_root: project_a.path().display().to_string(),
                client_name: "codex".to_string(),
                pid: Some(56),
            })
            .send()
            .await
            .expect("open request")
            .error_for_status()
            .expect("open status")
            .json::<OpenProjectResponse>()
            .await
            .expect("open body");

        let tool_url = format!(
            "{base_url}/v1/sessions/{}/tools/index_folder",
            opened.session_id
        );
        let first = client
            .post(&tool_url)
            .json(&IndexFolderInput {
                path: project_b.path().display().to_string(),
                idempotency_key: Some("daemon-replay-key".to_string()),
            })
            .send()
            .await
            .expect("first index request")
            .error_for_status()
            .expect("first index status")
            .text()
            .await
            .expect("first index body");
        assert!(
            first.starts_with("Indexed 1 files"),
            "first daemon idempotent index_folder should index once, got: {first}"
        );

        std::fs::write(
            project_b.path().join("src").join("second.rs"),
            "fn second_fn() {}\n",
        )
        .expect("write second source b");

        let replay = client
            .post(&tool_url)
            .json(&IndexFolderInput {
                path: project_b.path().display().to_string(),
                idempotency_key: Some("daemon-replay-key".to_string()),
            })
            .send()
            .await
            .expect("replay index request")
            .error_for_status()
            .expect("replay index status")
            .text()
            .await
            .expect("replay index body");
        assert_eq!(
            replay, first,
            "daemon special route must replay stored output for same key and canonical request"
        );

        let conflict = client
            .post(&tool_url)
            .json(&IndexFolderInput {
                path: project_a.path().display().to_string(),
                idempotency_key: Some("daemon-replay-key".to_string()),
            })
            .send()
            .await
            .expect("conflict index request")
            .error_for_status()
            .expect("conflict index status")
            .text()
            .await
            .expect("conflict index body");
        assert!(
            conflict.contains("Idempotency conflict"),
            "daemon special route must reject same key with different request, got: {conflict}"
        );
        assert!(
            !conflict.starts_with("Indexed "),
            "daemon conflict must not report synthetic indexing success: {conflict}"
        );

        let _ = handle.shutdown_tx.send(());
        wait_for_path_absent(&daemon_home.path().join(LEGACY_DAEMON_PORT_FILE)).await;
    }

    #[tokio::test]
    async fn test_analyze_file_impact_uses_session_project_root_not_process_cwd() {
        let _env_lock = env_lock().await;
        let daemon_home = TempDir::new().expect("daemon home");
        let _env_guard = EnvVarGuard::set("SYMFORGE_HOME", daemon_home.path());
        let project = project_dir("symforge-daemon-impact-root");
        let outside = TempDir::new().expect("outside cwd");
        let source_path = project.path().join("src").join("lib.rs");
        std::fs::write(&source_path, "pub fn old_name() {}\n").expect("write initial source");

        let handle = spawn_daemon("127.0.0.1").await.expect("spawn daemon");
        let client = authed_client(&handle);
        let base_url = format!("http://127.0.0.1:{}", handle.port);

        let opened = client
            .post(format!("{base_url}/v1/sessions/open"))
            .json(&OpenProjectRequest {
                project_root: project.path().display().to_string(),
                client_name: "codex".to_string(),
                pid: Some(4242),
            })
            .send()
            .await
            .expect("open request")
            .error_for_status()
            .expect("open status")
            .json::<OpenProjectResponse>()
            .await
            .expect("open body");

        std::fs::write(&source_path, "pub fn new_name() {}\n").expect("write updated source");

        let _cwd_guard = CwdGuard::set(outside.path());

        let impact = client
            .post(format!(
                "{base_url}/v1/sessions/{}/tools/analyze_file_impact",
                opened.session_id
            ))
            .json(&serde_json::json!({
                "path": "src/lib.rs"
            }))
            .send()
            .await
            .expect("impact request")
            .error_for_status()
            .expect("impact status")
            .text()
            .await
            .expect("impact body");

        assert!(
            impact.contains("new_name"),
            "impact analysis should read from the session project root, got: {impact}"
        );

        let _ = handle.shutdown_tx.send(());
        wait_for_path_absent(&daemon_home.path().join(LEGACY_DAEMON_PORT_FILE)).await;
    }

    // -----------------------------------------------------------------
    // C6: concurrent open_project_session tests
    // -----------------------------------------------------------------

    #[test]
    fn test_concurrent_open_same_project_no_panic() {
        use std::sync::{Arc, Barrier};

        let project = project_dir("symforge-conc-same");
        std::fs::write(project.path().join("src").join("main.rs"), "fn main() {}\n")
            .expect("write source");
        let project_root = project.path().display().to_string();

        let state = Arc::new(DaemonState::new());
        let barrier = Arc::new(Barrier::new(2));

        let handles: Vec<_> = (0..2)
            .map(|i| {
                let state = Arc::clone(&state);
                let barrier = Arc::clone(&barrier);
                let root = project_root.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    state.open_project_session(OpenProjectRequest {
                        project_root: root,
                        client_name: format!("client-{i}"),
                        pid: None,
                    })
                })
            })
            .collect();

        for handle in handles {
            handle
                .join()
                .expect("thread panicked")
                .expect("open_project_session failed");
        }

        let projects = state.list_projects();
        assert_eq!(projects.len(), 1, "exactly 1 project instance expected");
        assert_eq!(
            state.health().session_count,
            2,
            "exactly 2 sessions expected"
        );

        let project_id = &projects[0].project_id;
        let projects_lock = state.projects.read();
        let instance = projects_lock.get(project_id).expect("project must exist");
        assert_eq!(
            instance.activation_state,
            ActivationState::Active,
            "project must be Active after concurrent opens"
        );
    }

    #[test]
    fn test_concurrent_open_different_projects() {
        use std::sync::{Arc, Barrier};

        let project_a = project_dir("symforge-conc-diff-a");
        std::fs::write(
            project_a.path().join("src").join("main.rs"),
            "fn main() {}\n",
        )
        .expect("write source a");
        let project_b = project_dir("symforge-conc-diff-b");
        std::fs::write(
            project_b.path().join("src").join("main.rs"),
            "fn main() {}\n",
        )
        .expect("write source b");

        let root_a = project_a.path().display().to_string();
        let root_b = project_b.path().display().to_string();

        let state = Arc::new(DaemonState::new());
        let barrier = Arc::new(Barrier::new(2));

        let roots = [root_a, root_b];
        let handles: Vec<_> = roots
            .iter()
            .enumerate()
            .map(|(i, root)| {
                let state = Arc::clone(&state);
                let barrier = Arc::clone(&barrier);
                let root = root.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    state.open_project_session(OpenProjectRequest {
                        project_root: root,
                        client_name: format!("client-{i}"),
                        pid: None,
                    })
                })
            })
            .collect();

        let responses: Vec<_> = handles
            .into_iter()
            .map(|h| h.join().expect("thread panicked").expect("open failed"))
            .collect();

        assert_ne!(
            responses[0].project_id, responses[1].project_id,
            "two distinct projects expected"
        );
        assert_eq!(
            state.list_projects().len(),
            2,
            "exactly 2 project instances expected"
        );
    }

    #[test]
    fn test_open_close_race_no_panic() {
        use std::sync::{Arc, Barrier};

        let project = project_dir("symforge-conc-race");
        std::fs::write(project.path().join("src").join("main.rs"), "fn main() {}\n")
            .expect("write source");
        let project_root = project.path().display().to_string();

        let state = Arc::new(DaemonState::new());

        // Open a first session before the race.
        let first = state
            .open_project_session(OpenProjectRequest {
                project_root: project_root.clone(),
                client_name: "racer-first".to_string(),
                pid: None,
            })
            .expect("first session");
        let first_session_id = first.session_id.clone();

        let barrier = Arc::new(Barrier::new(2));

        // Thread A: opens another session concurrently.
        let state_a = Arc::clone(&state);
        let barrier_a = Arc::clone(&barrier);
        let root_a = project_root.clone();
        let open_handle = std::thread::spawn(move || {
            barrier_a.wait();
            state_a.open_project_session(OpenProjectRequest {
                project_root: root_a,
                client_name: "racer-open".to_string(),
                pid: None,
            })
        });

        // Thread B: closes the first session concurrently.
        let state_b = Arc::clone(&state);
        let barrier_b = Arc::clone(&barrier);
        let close_handle = std::thread::spawn(move || {
            barrier_b.wait();
            state_b.close_session(&first_session_id)
        });

        // Both must return Ok / Some — not panic.
        open_handle
            .join()
            .expect("open thread panicked")
            .expect("open failed");
        close_handle.join().expect("close thread panicked");
        // (close_session returns Option — None is acceptable if session was already gone)
    }

    #[test]
    fn test_discarded_instance_no_leaked_tasks() {
        use std::sync::{Arc, Barrier};

        let project = project_dir("symforge-conc-3way");
        std::fs::write(project.path().join("src").join("main.rs"), "fn main() {}\n")
            .expect("write source");
        let project_root = project.path().display().to_string();

        let state = Arc::new(DaemonState::new());
        let barrier = Arc::new(Barrier::new(3));

        let handles: Vec<_> = (0..3)
            .map(|i| {
                let state = Arc::clone(&state);
                let barrier = Arc::clone(&barrier);
                let root = project_root.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    state.open_project_session(OpenProjectRequest {
                        project_root: root,
                        client_name: format!("racer-{i}"),
                        pid: None,
                    })
                })
            })
            .collect();

        for handle in handles {
            handle
                .join()
                .expect("thread panicked")
                .expect("open_project_session failed");
        }

        let projects = state.list_projects();
        assert_eq!(projects.len(), 1, "exactly 1 project instance expected");
        assert_eq!(
            state.health().session_count,
            3,
            "exactly 3 sessions expected"
        );

        let project_id = &projects[0].project_id;
        let projects_lock = state.projects.read();
        let instance = projects_lock.get(project_id).expect("project must exist");

        assert_eq!(
            instance.activation_state,
            ActivationState::Active,
            "project must be Active"
        );
        // watcher_task is None in sync tests (no Tokio runtime for try_current()),
        // but activation_state == Active proves activate() ran exactly once and
        // no extra ProjectInstance was left alive from discarded racers.
        // The session_count == 3 proves all 3 callers got a valid session back.
    }

    #[tokio::test]
    async fn test_stale_start_lock_is_removed() {
        let _env_lock = env_lock().await;
        let daemon_home = TempDir::new().expect("daemon home");
        let _env_guard = EnvVarGuard::set("SYMFORGE_HOME", daemon_home.path());

        let lock_path = daemon_home.path().join(daemon_start_lock_file_name());

        // Create a lock file and backdate it to 60 seconds ago.
        let file = std::fs::File::create(&lock_path).expect("create lock");
        let old_time = std::time::SystemTime::now() - std::time::Duration::from_secs(60);
        let times = std::fs::FileTimes::new().set_modified(old_time);
        file.set_times(times).expect("set file times");
        drop(file);

        // try_acquire_start_lock should detect the stale lock and succeed.
        let result = try_acquire_start_lock().expect("should not error");
        assert!(
            result.is_some(),
            "stale lock (>30s old) should be removed and lock acquired"
        );

        // Clean up — the DaemonStartLock Drop impl removes the file.
        drop(result);
    }

    #[tokio::test]
    async fn test_fresh_start_lock_blocks_acquisition() {
        let _env_lock = env_lock().await;
        let daemon_home = TempDir::new().expect("daemon home");
        let _env_guard = EnvVarGuard::set("SYMFORGE_HOME", daemon_home.path());

        let lock_path = daemon_home.path().join(daemon_start_lock_file_name());

        // Create a fresh lock file (just now — well within 30s threshold).
        std::fs::write(&lock_path, "").expect("create lock");

        // try_acquire_start_lock should see the fresh lock and return None.
        let result = try_acquire_start_lock().expect("should not error");
        assert!(
            result.is_none(),
            "fresh lock (<30s old) should block acquisition"
        );

        // Clean up.
        let _ = std::fs::remove_file(&lock_path);
    }

    #[tokio::test]
    async fn test_cleanup_daemon_files_removes_start_lock() {
        let _env_lock = env_lock().await;
        // Verify that cleanup_daemon_files removes the start lock file
        // alongside port and pid files. We test the function signature
        // and that it doesn't panic — actual file removal is best-effort.
        let dir = TempDir::new().expect("temp dir");
        let _env_guard = EnvVarGuard::set("SYMFORGE_HOME", dir.path());
        let port_file = dir.path().join(LEGACY_DAEMON_PORT_FILE);
        let pid_file = dir.path().join(LEGACY_DAEMON_PID_FILE);
        let lock_file = dir.path().join(LEGACY_DAEMON_START_LOCK_FILE);

        std::fs::write(&port_file, "12345").expect("write port");
        std::fs::write(&pid_file, "99999").expect("write pid");
        std::fs::write(&lock_file, "").expect("write lock");

        cleanup_daemon_files();

        assert!(!port_file.exists(), "cleanup removes port file");
        assert!(!pid_file.exists(), "cleanup removes pid file");
        assert!(!lock_file.exists(), "cleanup removes start lock file");
    }

    #[tokio::test]
    async fn test_runtime_cleanup_preserves_start_lock() {
        let _env_lock = env_lock().await;
        let daemon_home = TempDir::new().expect("daemon home");
        let _env_guard = EnvVarGuard::set("SYMFORGE_HOME", daemon_home.path());

        let port_file = daemon_home.path().join(LEGACY_DAEMON_PORT_FILE);
        let pid_file = daemon_home.path().join(LEGACY_DAEMON_PID_FILE);
        let lock_file = daemon_home.path().join(LEGACY_DAEMON_START_LOCK_FILE);

        std::fs::write(&port_file, "12345").expect("write port");
        std::fs::write(&pid_file, "99999").expect("write pid");
        std::fs::write(&lock_file, "").expect("write lock");

        cleanup_daemon_runtime_files();

        assert!(!port_file.exists(), "runtime cleanup removes port file");
        assert!(!pid_file.exists(), "runtime cleanup removes pid file");
        assert!(
            lock_file.exists(),
            "runtime cleanup must not release another process' start lock"
        );
    }

    #[tokio::test]
    async fn test_corrupt_port_file_does_not_block_daemon_selection() {
        let _env_lock = env_lock().await;
        let daemon_home = TempDir::new().expect("daemon home");
        let _env_guard = EnvVarGuard::set("SYMFORGE_HOME", daemon_home.path());

        std::fs::write(
            daemon_home.path().join(LEGACY_DAEMON_PORT_FILE),
            "not-a-port",
        )
        .expect("write corrupt port");

        let identity = current_daemon_identity();
        let selected = daemon_port_if_compatible(&identity)
            .await
            .expect("corrupt port should be ignored, not fatal");

        assert_eq!(selected, None);
    }
}
