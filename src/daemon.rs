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

use crate::live_index::store::LiveIndex;
use crate::live_index::view::{BaseKey, CommitId, IndexBase, Targets, WorkingSet};
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
    search_symbols_options_from_input, search_text_options_from_input,
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
    /// generation from `base_generation_seq`) on a miss. Populated on project
    /// load/activate and seeded into per-session working sets; the cross-project
    /// read route reads those bases and `intern_base_refresh` FORCE-REPLACES the
    /// value for a key when a watched change has advanced the project's published
    /// index (B2/D12) — still one Arc per `BaseKey` (SC-002).
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
    /// Per-session copy-on-write working set (Feature 012). Seeded on open with a
    /// SINGLE entry — the active project + its interned shared base + an EMPTY
    /// overlay — and grown by additive opens. The CROSS-PROJECT read route reads
    /// it (Phase 3) and LAZILY REFRESHES each targeted entry's base when the
    /// project's published index has advanced (B2/D12,
    /// `refresh_working_set_bases`); the single-active path never touches it. NO
    /// code path writes into its overlay (the no-overlay-writes invariant that
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
    /// Feature 012 (Phase 3): the session's working set, shared by `Arc` so the
    /// cross-project read route (`search_symbols`/`search_text`/`find_references`
    /// with `project`/`projects`) can read the open projects directly AND lazily
    /// refresh each targeted base when the project's published index has advanced
    /// (B2/D12, `DaemonState::refresh_working_set_bases`, called from
    /// `call_tool_handler` before the read). The single active-project path NEVER
    /// reads or refreshes this (it dispatches to `server` as before), so
    /// single-project behavior is byte-identical and frecency-neutral. No overlay
    /// is written. The active project's id for the targeting contract is
    /// `project_id` above (the default target when neither param is supplied).
    working_set: Arc<RwLock<WorkingSet>>,
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

    /// FORCE-REPLACE the interned base for `candidate`'s [`BaseKey`] with a
    /// freshly-stamped base wrapping `candidate`'s (current) snapshot — the B2/D12
    /// refresh primitive. Unlike [`DaemonState::intern_base`], it does NOT return
    /// the cached Arc on a key hit: a watcher reindex keeps `BaseKey=(root,commit)`
    /// unchanged (no commit advance), so the plain intern would CACHE-HIT and hand
    /// back the STALE `Arc<LiveIndex>`. This always re-stamps a new monotonic
    /// `base_generation` and overwrites the map VALUE for the key, so SC-002 holds
    /// (exactly ONE `Arc<IndexBase>` per `BaseKey` after the replace — the value is
    /// replaced, never duplicated).
    ///
    /// LOCK ORDER: acquires ONLY `bases` (top of `bases -> projects -> sessions`),
    /// like `intern_base`. Never call while holding `projects`/`sessions`.
    fn intern_base_refresh(&self, candidate: Arc<IndexBase>) -> Arc<IndexBase> {
        let mut bases = self.bases.write();
        let generation = self.base_generation_seq.fetch_add(1, Ordering::Relaxed);
        let mut published = (*candidate).clone();
        published.base_generation = generation;
        let published = Arc::new(published);
        // Overwrite (not insert-if-absent): one Arc per BaseKey is preserved.
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

    /// B2/D12 freshness refresh: lazily re-intern + replace the working-set base
    /// for every TARGETED project whose interned snapshot has gone stale relative
    /// to the project's CURRENT published index, BEFORE a cross-project read.
    ///
    /// THE DEFECT it closes: a working-set entry holds an `Arc<IndexBase>` captured
    /// at open time. The watcher swaps a NEW `Arc<LiveIndex>` into the project's
    /// `ArcSwap` on every observed change, but a watcher edit does NOT change the
    /// git commit, so the `BaseKey=(root,commit)` is unchanged and the interned
    /// base is never refreshed — cross-project reads go stale after any watched
    /// change. This re-captures the current snapshot on the next cross-project read.
    ///
    /// FRESHNESS SIGNAL: `Arc::ptr_eq(&project.index.read(), &entry.base.index)`.
    /// Every publish does `live.store(Arc::new(..))`, so a changed pointer means
    /// the published index advanced. HONEST LIMITATION: this OVER-triggers on
    /// mtime-only touches (`touch_mtime` stores a fresh `Arc<LiveIndex>` without
    /// bumping published state), so a touch that changed no symbols can still cause
    /// one spurious re-intern. That is SAFE — it re-captures an equal-or-fresher
    /// snapshot — and merely mild churn on the rare cross-project read path; it is
    /// NOT an exact signal. (Upgrade path if churn ever matters: compare a
    /// `published_state().generation` stamped into `IndexBase`; not taken now.)
    ///
    /// MISMATCH-GATED: the warm path takes NO write lock. It snapshots the targeted
    /// entries' base pointers under `working_set.read()`, compares them under
    /// `projects.read()`, and returns early if all are fresh. Only when ≥1 entry is
    /// stale does it intern fresh bases and take `working_set.write()`.
    ///
    /// LOCK ORDER (`bases -> projects -> sessions`; `working_set` is outside the
    /// hierarchy, never held across a daemon-map lock): snapshot under
    /// `working_set.read()` -> drop; compare + build candidates under
    /// `projects.read()` -> drop; `intern_base_refresh` under `bases.write()` ->
    /// drop; finally `working_set.write()` with NO daemon-map lock held — the same
    /// intern-then-`working_set.add` sequence the retarget/additive paths use.
    ///
    /// SC-002 preserved: `intern_base_refresh` REPLACES the map value for the key
    /// (one Arc per `BaseKey`). FRECENCY-NEUTRAL: no edit-commit hook fires on this
    /// read path. `working_set.add` re-attaches a fresh empty overlay fenced to the
    /// new base, so no `StaleOverlay` skip (US1 overlays are empty).
    fn refresh_working_set_bases(&self, working_set: &Arc<RwLock<WorkingSet>>, targets: &Targets) {
        // (1) Snapshot the targeted entries' (project_id, base-index Arc) under the
        // session's own working_set read lock, then DROP it before any daemon lock.
        let targeted: Vec<(String, Arc<LiveIndex>)> = {
            let ws = working_set.read();
            ws.iter()
                .filter(|entry| targets_selects(targets, &entry.project_id))
                .map(|entry| (entry.project_id.clone(), Arc::clone(&entry.base.index)))
                .collect()
        };
        if targeted.is_empty() {
            return;
        }

        // (2) Build fresh candidate bases ONLY for entries whose interned index Arc
        // no longer matches the project's current published index (ptr_eq). Compare
        // and snapshot each `(project_id, candidate)` under one `projects.read()`,
        // then drop it. The project_id is carried through because a `BaseKey` keys
        // on `(canonical_root, commit)`, NOT the working-set project id.
        let stale: Vec<(String, Arc<IndexBase>)> = {
            let projects = self.projects.read();
            targeted
                .iter()
                .filter_map(|(project_id, entry_index)| {
                    let project = projects.get(project_id)?;
                    // `read()` yields an arc_swap Guard derefing to the current
                    // `Arc<LiveIndex>`; `&current` deref-coerces to the
                    // `&Arc<LiveIndex>` ptr_eq wants (as `base()` does for clone).
                    // Every publish stores a fresh Arc, so an equal pointer means
                    // the published index has not advanced since intern.
                    let current = project.index.read();
                    if Arc::ptr_eq(&current, entry_index) {
                        None // fresh — no re-intern
                    } else {
                        // candidate wrapping the current snapshot
                        Some((project_id.clone(), project.base()))
                    }
                })
                .collect()
        }; // projects read guard dropped here, before bases.write() below
        if stale.is_empty() {
            return; // warm path: every targeted entry already fresh, zero writes
        }

        // (3) Force-replace each stale base in the intern table (bases.write each
        // call, dropped before the next), keeping the project_id paired with the
        // fresh shared Arc for the working-set swap.
        let fresh: Vec<(String, Arc<IndexBase>)> = stale
            .into_iter()
            .map(|(project_id, candidate)| (project_id, self.intern_base_refresh(candidate)))
            .collect();

        // (4) Swap each refreshed base into the working set under its own write
        // lock — NO daemon-map lock held here. `add` replaces the entry with a
        // fresh empty overlay re-fenced to the new base (no StaleOverlay skip).
        let mut ws = working_set.write();
        for (project_id, base) in fresh {
            // ponytail: ws.add attaches a fresh EMPTY overlay — correct ONLY while
            // overlays are always empty (no production overlay writers). If cross-
            // project overlay writes ever land, switch to
            // Overlay::rebase(+uncommitted_paths) or this silently drops
            // uncommitted deltas on every freshness refresh.
            ws.add(project_id, base);
        }
        drop(ws); // release working_set before sweeping the bases table

        // (5) GC orphaned bases. A genuine commit-advance changes BaseKey.commit,
        // so step (3)/(4) interned the snapshot under a NEW key and replaced the
        // working-set entry — the OLD key's value is now referenced only by the
        // map (no session/working-set entry holds it). SC-002-safe eviction: under
        // `bases.write()` (the sole mutator/cloner of the map), `entry.base` is a
        // clone of the SAME map Arc, so the map value's `strong_count == 1` iff no
        // live consumer references it. Retain everything still referenced; drop the
        // orphans. No `Weak<IndexBase>` exists and no other Arc<IndexBase> holder
        // exists, so the count is exact. This is the ONLY path that orphans a base
        // (the no-commit watcher path force-replaces the SAME key — no orphan).
        // ponytail: full-map retain is O(bases); fine — table is tiny per daemon.
        // Add a threshold only if base count ever grows large.
        self.gc_orphaned_bases();
    }

    /// Evict orphaned interned bases — values no live consumer references anymore
    /// (the unbounded-growth defect on a long-lived multi-commit daemon).
    ///
    /// SC-002-safe eviction condition: under `bases.write()` (the SOLE mutator and
    /// cloner of the map), every `WorkingSetEntry.base` is a clone of the SAME map
    /// `Arc<IndexBase>`, so a map value's `Arc::strong_count == 1` means the map is
    /// the only owner — no session/working-set holds it. We retain everything still
    /// referenced and drop only `== 1`. No `Weak<IndexBase>` exists and no other
    /// `Arc<IndexBase>` holder exists anywhere, so the count is exact and the sweep
    /// can never evict a base a later `intern_base` would re-share (which would mint
    /// a SECOND Arc per BaseKey — the SC-002 violation this guards against).
    ///
    /// LOCK ORDER: acquires ONLY `bases` (top of `bases -> projects -> sessions`).
    /// Never call while holding `projects`/`sessions`, nor while holding a transient
    /// clone of a map value in the same scope (that would self-inflate the count).
    fn gc_orphaned_bases(&self) {
        self.bases
            .write()
            .retain(|_key, base| Arc::strong_count(base) > 1);
    }

    fn register_session_for_existing_project(
        &self,
        project_id: &str,
        request: &OpenProjectRequest,
        canonical_root: &Path,
    ) -> anyhow::Result<OpenProjectResponse> {
        let session_id = format!(
            "session-{}",
            self.next_session_id.fetch_add(1, Ordering::Relaxed)
        );
        let now = SystemTime::now();

        // D17 (atomic open, fail-never): ensure the project EXISTS and register
        // this session's id under a SINGLE `projects.write()` hold, so a
        // concurrent `close_session` cannot reap the project in a
        // check-then-register window. If a concurrent close removed it between
        // `open_project_session`'s probe and here, RECOVER by reloading +
        // activating under the held write lock (rare path; load-under-lock is
        // acceptable and is what makes open fail-never) instead of bailing
        // "was removed between check and session registration". Once our
        // `session_ids` entry is in, close will not reap the project.
        let (project_name, canonical_root_text, session_count) = {
            let mut projects = self.projects.write();
            if !projects.contains_key(project_id) {
                let mut reloaded = ProjectInstance::load(canonical_root)?;
                if reloaded.activation_state == ActivationState::Inactive {
                    reloaded.activate();
                }
                projects.insert(project_id.to_string(), reloaded);
            }
            let project = projects
                .get_mut(project_id)
                .expect("project present: inserted above under this same write lock when absent");
            project.session_ids.insert(session_id.clone());
            (
                project.project_name.clone(),
                normalized_path_string(&project.canonical_root),
                project.session_ids.len(),
            )
        };

        // Intern the base AFTER the project is guaranteed present and the session
        // is registered. `bases -> projects -> sessions` order holds: `bases` is
        // acquired (inside `intern_base_for_project`) only after the
        // `projects.write()` above is released, never while another daemon lock is
        // held. Our `session_ids` entry already pins the project, so it cannot
        // vanish during interning. Seed the session's working set with ONE entry:
        // the active project, its interned shared base, and an EMPTY overlay
        // (Phase 1, no-overlay-writes invariant). If interning returns `None` the
        // working set stays empty and `session_runtime` still resolves via
        // `active_project_id`, so behavior is unchanged.
        let base = self.intern_base_for_project(project_id);
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
        // Scan ALL projects instead of relying on session.project_id, which can be
        // stale if index_folder_for_session reassigned concurrently.
        //
        // Feature 012 (Phase 2): a session may now reference MORE THAN ONE project
        // (the active one plus any additively-opened ones in its working set), so
        // closing it must detach the session from EVERY project that lists it and
        // tear down each whose `session_ids` then empties — not just the first
        // match. Leaving an additively-opened project's `session_ids` pointing at a
        // closed session would leak the project forever. The reported `project_id`
        // / `remaining_sessions` / `project_removed` describe the session's ACTIVE
        // project (its primary association), preserving the wire contract; sibling
        // additive projects are reaped silently.
        let active_project_id = self
            .sessions
            .read()
            .get(session_id)
            .map(|s| s.active_project_id.clone());

        let mut active_remaining = 0usize;
        let mut active_removed = false;
        let mut active_pid_seen = false;
        {
            let mut projects = self.projects.write();
            // All projects that list this session (active + additive siblings).
            let owning: Vec<String> = projects
                .iter()
                .filter(|(_, project)| project.session_ids.contains(session_id))
                .map(|(id, _)| id.clone())
                .collect();

            for pid in owning {
                let is_active = active_project_id.as_deref() == Some(pid.as_str());
                let remaining = {
                    let Some(project) = projects.get_mut(&pid) else {
                        continue;
                    };
                    project.session_ids.remove(session_id);
                    project.session_ids.len()
                };
                let removed = if remaining == 0 {
                    if let Some(mut removed) = projects.remove(&pid) {
                        abort_watcher_task(&mut removed.watcher_task, &removed.stop_token);
                    }
                    true
                } else {
                    false
                };
                if is_active {
                    active_pid_seen = true;
                    active_remaining = remaining;
                    active_removed = removed;
                }
            }
        }

        let session = self.sessions.write().remove(session_id)?;

        // If the session referenced no project, or its active project was already
        // gone (e.g. concurrent reassignment), report an orphan close — the
        // session record is still cleaned up above.
        let project_id = match (active_project_id, active_pid_seen) {
            (Some(pid), true) => pid,
            _ => "orphan".to_string(),
        };

        Some(CloseSessionResponse {
            session_id: session.session_id,
            project_id,
            remaining_sessions: active_remaining,
            project_removed: active_removed,
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

        // Feature 012 (Phase 2): ADDITIVE open. A NEW code path, distinct from
        // the destructive `needs_reassign` retarget below: it adds `target_root`
        // as a SECOND project in the session's working set (intern its base, join
        // `session_ids` additively, `working_set.add(..)`) WITHOUT evicting the
        // current project or changing `active_project_id`. This is what enables
        // cross-project reads (Phase 3). No overlay is written (invariant).
        if input.add == Some(true) {
            return self.index_folder_additive(session_id, &target_project_id, &target_root);
        }

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
        if needs_reassign {
            // Feature 012 (Phase 2) — CLOSE THE RETARGET SEAM. Phase 0/1 updated
            // ONLY `active_project_id`, leaving the seeded `working_set` entry
            // pointing at the pre-retarget (now-evicted) project, so the working
            // set was inconsistent with the active project. Now that Phase 3
            // reads the working set, re-seed it: swap the OLD active entry for the
            // freshly retargeted project + its interned base + a fresh EMPTY
            // overlay. Additively-opened sibling projects (if any) are left
            // intact — retarget only swaps the ACTIVE slot. The base is interned
            // FIRST, holding no session lock (`intern_base_for_project` takes
            // `projects` then `bases` with no overlap), then applied under
            // `sessions.write()`. No overlay is written (invariant).
            let new_base = self.intern_base_for_project(&target_project_id);
            if let Some(session) = self.sessions.write().get_mut(session_id) {
                let old_active =
                    std::mem::replace(&mut session.active_project_id, target_project_id.clone());
                if let Some(base) = new_base {
                    let mut working_set = session.working_set.write();
                    // Drop the stale active entry (its project was evicted on the
                    // retarget) before adding the new active project. If `add`
                    // already holds an entry for `target_project_id` (an additive
                    // open that we are now promoting to active), it is replaced
                    // with a fresh overlay — still single-overlay, still empty.
                    if old_active != target_project_id {
                        working_set.remove(&old_active);
                    }
                    working_set.add(target_project_id.clone(), base);
                }
                session
                    .last_seen_at
                    .store(now_epoch_millis(), Ordering::Relaxed);
            }
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

    /// Feature 012 (Phase 2): the ADDITIVE `index_folder(add:true)` code path.
    ///
    /// Opens `target_root` as an ADDITIONAL project in `session_id`'s working set
    /// without retargeting: the session keeps its active project and gains this
    /// one, so a later cross-project read can target both. Concretely it
    /// (1) loads+activates the target project if not already loaded and joins the
    /// session to it additively (no evict, no `active_project_id` change), then
    /// (2) interns the target base and `working_set.add(..)`s it with a fresh
    /// EMPTY overlay. Re-adding an already-open project re-indexes it and refreshes
    /// its working-set entry (fresh empty overlay) — idempotent at the open level.
    ///
    /// LOCK ORDER (`bases -> projects -> sessions`): the `projects` write guard is
    /// fully released before `intern_base_for_project` (which takes `projects`
    /// then `bases` with no overlap) and before `sessions.write()`. No overlay is
    /// written (invariant). No idempotency-key replay here — additive opens are
    /// not part of the retarget replay ledger.
    fn index_folder_additive(
        &self,
        session_id: &str,
        target_project_id: &str,
        target_root: &Path,
    ) -> anyhow::Result<String> {
        // (1) Load/activate the target project if absent and join the session to
        // it additively, then reload to index the current tree. All project-map
        // mutation is scoped to this block so the write guard is released before
        // we touch `bases`/`sessions` below (lock order).
        let (file_count, symbol_count) = {
            let mut projects = self.projects.write();

            // Confirm the session exists (fail loud on an unknown session) under
            // the same projects guard ordering used elsewhere.
            {
                let sessions = self.sessions.read();
                if !sessions.contains_key(session_id) {
                    return Err(anyhow::anyhow!("unknown session '{session_id}'"));
                }
            }

            if !projects.contains_key(target_project_id) {
                let mut project = ProjectInstance::load(target_root)?;
                debug_assert_eq!(
                    project.activation_state,
                    ActivationState::Inactive,
                    "freshly loaded project must be Inactive"
                );
                project.activate();
                projects.insert(target_project_id.to_string(), project);
            }

            let target_project = projects
                .get_mut(target_project_id)
                .ok_or_else(|| anyhow::anyhow!("missing target project after load"))?;
            // Additive join: the session now references this project too.
            target_project.session_ids.insert(session_id.to_string());
            target_project.reload(target_root)?
        }; // projects write guard released here

        // (2) Attach the freshly-indexed project to the session's working set
        // (intern base under `bases`, join session_ids, add a fresh EMPTY overlay)
        // via the shared `add_project_to_session` helper — same lock order, one
        // implementation of the attach step.
        if !self.add_project_to_session(session_id, target_project_id) {
            return Err(anyhow::anyhow!(
                "failed to attach project to session working set after additive load"
            ));
        }

        Ok(format!(
            "Indexed {file_count} files, {symbol_count} symbols (added to working set)."
        ))
    }

    /// Feature 012 (Phase 2): add an already-loaded project to a session's working
    /// set as an additional, non-active project. Returns `false` if the session or
    /// the project is unknown. The base is interned FIRST (no session lock held)
    /// to preserve the `bases -> projects -> sessions` order; a fresh EMPTY overlay
    /// is attached (no overlay write). Idempotent: re-adding refreshes the entry.
    fn add_project_to_session(&self, session_id: &str, project_id: &str) -> bool {
        let Some(base) = self.intern_base_for_project(project_id) else {
            return false;
        };
        {
            let mut projects = self.projects.write();
            let Some(project) = projects.get_mut(project_id) else {
                return false;
            };
            project.session_ids.insert(session_id.to_string());
        }
        match self.sessions.write().get_mut(session_id) {
            Some(session) => {
                session
                    .working_set
                    .write()
                    .add(project_id.to_string(), base);
                true
            }
            None => false,
        }
    }

    /// Feature 012 (Phase 2): set the session's ACTIVE project to one already in
    /// its working set. Returns `false` if the session is unknown or `project_id`
    /// is not an open working-set entry (cannot activate a project that is not
    /// open). Only flips `active_project_id`; the working set is untouched (the
    /// target entry already exists). No overlay is written.
    //
    // ponytail: explicit working-set management seam. It has no production route
    // yet — the DEFERRED "dedicated working-set management tool" (US-follow-up)
    // owns wiring it to an MCP verb. Exercised by daemon unit tests today; allow
    // dead_code so the server build (dead-code-on) stays green until that tool
    // lands, rather than deleting a vetted, tested primitive.
    #[allow(dead_code)]
    fn set_active_project(&self, session_id: &str, project_id: &str) -> bool {
        match self.sessions.write().get_mut(session_id) {
            Some(session) => {
                let is_open = session.working_set.read().get(project_id).is_some();
                if !is_open {
                    return false;
                }
                session.active_project_id = project_id.to_string();
                session
                    .last_seen_at
                    .store(now_epoch_millis(), Ordering::Relaxed);
                true
            }
            None => false,
        }
    }

    /// Feature 012 (Phase 2): remove a project from a session's working set.
    /// Refuses (returns `false`) to remove the ACTIVE project — the active slot
    /// always resolves to a live working-set entry — or when the session/entry is
    /// unknown. Also drops the session from the project's `session_ids` (and tears
    /// the project down if that was its last session, matching retarget eviction).
    //
    // ponytail: companion seam to `set_active_project`; same DEFERRED working-set
    // management tool owns its route. Tested in the daemon module; allow dead_code
    // until the tool wires it.
    #[allow(dead_code)]
    fn remove_project_from_session(&self, session_id: &str, project_id: &str) -> bool {
        // First detach from the working set, refusing the active project.
        let removed = match self.sessions.write().get_mut(session_id) {
            Some(session) => {
                if session.active_project_id == project_id {
                    return false;
                }
                let mut working_set = session.working_set.write();
                working_set.remove(project_id).is_some()
            }
            None => false,
        };
        if !removed {
            return false;
        }
        // Detach the session from the project; tear the project down if this was
        // its last session (mirrors the retarget eviction in
        // `index_folder_for_session`).
        let mut projects = self.projects.write();
        if let Some(project) = projects.get_mut(project_id) {
            project.session_ids.remove(session_id);
            if project.session_ids.is_empty()
                && let Some(mut evicted) = projects.remove(project_id)
            {
                abort_watcher_task(&mut evicted.watcher_task, &evicted.stop_token);
            }
        }
        true
    }

    fn session_runtime(&self, session_id: &str) -> Option<SessionRuntime> {
        // Acquire projects read lock BEFORE sessions read lock so that
        // the active_project_id we read from the session is still valid while
        // we look it up in the projects map. (Lock order: projects -> sessions;
        // `bases` is not taken here.) Feature 012: resolution is via
        // `active_project_id` — the single active project — so the single-project
        // path is byte-for-byte unchanged. Phase 3 ALSO carries the session's
        // `working_set` handle (an `Arc` clone, O(1)) so the cross-project read
        // route can read it; the single-active path never touches it.
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
            // D15 overlay-WRITER (Option 3, FATAL fix): set the per-session
            // overlay handle on an OWNED clone of the shared server. `SymForgeServer`
            // is `#[derive(Clone)]` with a plain `Option<SessionOverlay>` field, so
            // this mutates only the local copy — `project.server` keeps `None`, and
            // no overlay leaks across sessions (SC-003). The lookup key is the HASH
            // `active_project_id` (CRACK fix), and the handle is the session's OWN
            // `working_set` Arc.
            server: {
                let mut s = project.server.clone();
                s.session_working_set = Some(crate::protocol::SessionOverlay {
                    project_id: session.active_project_id.clone(),
                    working_set: Arc::clone(&session.working_set),
                });
                s
            },
            working_set: Arc::clone(&session.working_set),
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
    /// STALENESS: the captured `Arc<LiveIndex>` is a SNAPSHOT frozen at this
    /// instant. Once interned, the watcher's later `ArcSwap` reloads (this project
    /// re-indexing files that changed on disk) do NOT rewrite the interned handle,
    /// so a cross-project read off the interned base goes stale after ANY
    /// watcher-picked-up change — not only after a git commit. See
    /// [`crate::live_index::view::IndexBase::index`]. Re-interning on
    /// watcher-observed change (live-freshness rebase) is the deferred Phase 4.
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
    let state_for_refresh = Arc::clone(&state);
    match state
        .governor
        .execute_non_abortable(&tool_name, async move {
            let handle = tokio::runtime::Handle::current();
            tokio::task::spawn_blocking(move || {
                // B2/D12: on the CROSS-PROJECT read path ONLY, lazily refresh each
                // targeted working-set base if the project's published index has
                // advanced since intern, BEFORE the search. The gate is shared with
                // the read route (`resolve_cross_project_targets` returns `None` for
                // the byte-identical single-active path), so single-project reads
                // never compare/re-intern and stay frecency-neutral (SC-4). Errors
                // and invalid targeting are surfaced by `execute_tool_call` itself.
                if let Some(targets) =
                    resolve_cross_project_targets(&tool_name_owned, &params, &runtime.project_id)
                {
                    state_for_refresh.refresh_working_set_bases(&runtime.working_set, &targets);
                }
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

/// Feature 012 (Phase 3): resolve the cross-project `project` / `projects`
/// params into a [`Targets`] selector, enforcing the surface contract.
///
/// CONTRACT (FR-004 / SC-001):
/// * BOTH omitted -> `One(active_project_id)` -> today's exact single-project
///   behavior (the caller dispatches it down the unchanged per-project path).
/// * `project` AND `projects` both set -> error (mutually exclusive).
/// * `projects: []` -> error (no silent "all"; `["*"]` is the explicit all).
/// * `projects: ["*"]` -> `All`.
/// * `project` containing a path separator -> corrective error (targets key on
///   project id/alias, NEVER a filesystem path).
/// * `project: id` -> `One(id)`; `projects: [ids..]` -> `Subset(ids)`.
///
/// Returns `Err(message)` for any contract violation; the message is surfaced to
/// the caller as an `InvalidRequest`-class tool response.
fn resolve_targets(
    project: Option<&str>,
    projects: Option<&[String]>,
    active_project_id: &str,
) -> Result<Targets, String> {
    match (project, projects) {
        (Some(_), Some(_)) => Err(
            "project and projects are mutually exclusive: pass exactly one (a single \
             id in `project`, or a list/`[\"*\"]` in `projects`)."
                .to_string(),
        ),
        (Some(id), None) => {
            let id = id.trim();
            if id.is_empty() {
                return Err("project must be a non-empty project id/alias.".to_string());
            }
            // Target keys on project id/alias, NEVER a filesystem path. A path
            // here is a usage mistake -> corrective error pointing at the open
            // verb. `project-<hash>` ids and aliases never contain separators.
            if id.contains('/') || id.contains('\\') {
                return Err(format!(
                    "project must be a project id/alias, not a filesystem path ('{id}'). \
                     Open the folder first with index_folder(path=\"{id}\", add=true), \
                     then target it by its returned project id."
                ));
            }
            Ok(Targets::One(id.to_string()))
        }
        (None, Some(list)) => {
            if list.is_empty() {
                return Err(
                    "projects must not be empty: pass [\"*\"] for all open projects or an \
                     explicit list of project ids."
                        .to_string(),
                );
            }
            // Trim every entry before matching (symmetry with the single `project`
            // branch, which trims): leading/trailing whitespace must not defeat
            // the `*` check, the path-separator guard, or id equality.
            let trimmed: Vec<&str> = list.iter().map(|id| id.trim()).collect();
            if trimmed.iter().any(|id| id.is_empty()) {
                return Err(
                    "projects entries must be non-empty project ids/aliases (blank entry found)."
                        .to_string(),
                );
            }
            if trimmed.contains(&"*") {
                // Any `*` in the list means "all open projects".
                return Ok(Targets::All);
            }
            for id in &trimmed {
                if id.contains('/') || id.contains('\\') {
                    return Err(format!(
                        "projects entries must be project ids/aliases, not filesystem paths \
                         ('{id}'). Open the folder first with \
                         index_folder(path=\"{id}\", add=true)."
                    ));
                }
            }
            // Dedup the subset's ids (order-preserving): a duplicate id would
            // otherwise render a second bogus `── project: <id> ──` header for the
            // same project. `Targets::selects` is membership, so dedup is purely a
            // rendering-honesty fix, not a behavior change.
            let mut seen = std::collections::HashSet::new();
            let deduped: Vec<String> = trimmed
                .into_iter()
                .filter(|id| seen.insert(id.to_string()))
                .map(|id| id.to_string())
                .collect();
            Ok(Targets::Subset(deduped))
        }
        (None, None) => Ok(Targets::One(active_project_id.to_string())),
    }
}

/// Whether `targets` resolves to exactly the single active project — the
/// no-regression fast path that dispatches down the UNCHANGED per-project route
/// (byte-identical to pre-012). Any other shape (`Subset`, `All`, or a single
/// NON-active project) is a cross-project read served from the working set.
fn targets_is_single_active(targets: &Targets, active_project_id: &str) -> bool {
    matches!(targets, Targets::One(id) if id == active_project_id)
}

/// Whether `targets` selects `project_id` — membership mirroring
/// `Targets::selects` (private to `view`) for the freshness refresh, which must
/// iterate only the targeted working-set entries.
fn targets_selects(targets: &Targets, project_id: &str) -> bool {
    match targets {
        Targets::One(id) => id == project_id,
        Targets::Subset(ids) => ids.iter().any(|id| id == project_id),
        Targets::All => true,
    }
}

/// The three cross-project READ verbs. Edits, analyze_file_impact, orient, and
/// meta verbs are NEVER cross-project (they stay single-project on the active
/// project) — kept in one place so the read route (`execute_tool_call`) and the
/// freshness-refresh gate (`call_tool_handler`) agree by construction.
fn is_cross_project_read_verb(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "search_symbols" | "search_text" | "find_references"
    )
}

/// Resolve whether a tool call is a genuine CROSS-PROJECT read, returning the
/// targeted projects iff so (B2/D12). Returns `None` for every path that stays
/// single-project — a non-read verb, an unparseable peek, a resolve error, or a
/// `Targets` that is exactly the single active project — so callers leave the
/// byte-identical single-project route untouched (SC-4).
///
/// This is the SINGLE gate shared by both the read route in `execute_tool_call`
/// (which dispatches the cross-project search) and `call_tool_handler` (which
/// lazily refreshes the targeted working-set bases just before that dispatch).
/// Identical gating here guarantees the freshness refresh fires on exactly the
/// reads that read the working set, and never on the single-active fast path.
/// A genuine resolve error is surfaced by the read route's own `resolve_targets`
/// call; this gate swallows it (returns `None`) so the refresh never errors.
fn resolve_cross_project_targets(
    tool_name: &str,
    params: &serde_json::Value,
    active_project_id: &str,
) -> Option<Targets> {
    if !is_cross_project_read_verb(tool_name) {
        return None;
    }
    #[derive(serde::Deserialize)]
    struct CrossProjectPeek {
        #[serde(default)]
        project: Option<String>,
        #[serde(default)]
        projects: Option<Vec<String>>,
    }
    let peek: CrossProjectPeek =
        serde_json::from_value(params.clone()).unwrap_or(CrossProjectPeek {
            project: None,
            projects: None,
        });
    let targets = resolve_targets(
        peek.project.as_deref(),
        peek.projects.as_deref(),
        active_project_id,
    )
    .ok()?;
    if targets_is_single_active(&targets, active_project_id) {
        None
    } else {
        Some(targets)
    }
}

/// Validate that every explicitly-named target is actually OPEN in the working
/// set, returning a clear "project not open" error otherwise (FR-004). `All`
/// needs no check (it selects whatever is open). Unknown ids are a usage error,
/// not a silent empty result.
fn ensure_targets_open(targets: &Targets, working_set: &WorkingSet) -> Result<(), String> {
    let missing: Vec<&String> = match targets {
        Targets::One(id) => {
            if working_set.get(id).is_none() {
                vec![id]
            } else {
                Vec::new()
            }
        }
        Targets::Subset(ids) => ids
            .iter()
            .filter(|id| working_set.get(id).is_none())
            .collect(),
        Targets::All => Vec::new(),
    };
    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "project not open: {}. Open it in this session first with \
             index_folder(path=..., add=true), then retarget.",
            missing
                .iter()
                .map(|id| format!("'{id}'"))
                .collect::<Vec<_>>()
                .join(", ")
        ))
    }
}

/// Default cap on the TOTAL number of rendered hits a cross-project read emits
/// across all targeted projects when the caller passes no `limit`. The
/// single-project paths bound themselves (search_symbols default 50, references
/// 20, text 50); the cross-project path fans out over N projects, so without a
/// shared cap a `projects:["*"]` query over many large projects could emit an
/// unbounded result. 200 is generous for a multi-project view yet hard-bounds the
/// output. The caller's `limit` overrides this (clamped to the hard ceiling).
const CROSS_PROJECT_DEFAULT_RESULT_CAP: usize = 200;

/// Absolute ceiling on the cross-project total hit count, even when the caller
/// passes a large `limit`. A cross-project read must never blow up the result no
/// matter what `limit` is requested.
const CROSS_PROJECT_MAX_RESULT_CAP: usize = 1000;

/// Resolve the effective total-hit cap for a cross-project read from the caller's
/// optional `limit`: `None` -> the default cap; `Some(l)` -> `l` clamped to
/// `[1, CROSS_PROJECT_MAX_RESULT_CAP]` (a zero or absurd limit cannot disable the
/// bound).
fn cross_project_result_cap(limit: Option<u32>) -> usize {
    match limit {
        Some(l) => (l as usize).clamp(1, CROSS_PROJECT_MAX_RESULT_CAP),
        None => CROSS_PROJECT_DEFAULT_RESULT_CAP,
    }
}

/// Apply the caller's optional `max_tokens` budget to an already-assembled
/// cross-project body, truncating at a line boundary and DISCLOSING the
/// truncation honestly (Principle III). Uses the project-wide ~4-bytes/token
/// approximation that the other budgeted formatters use. A `None` or zero budget
/// is a no-op. Returns the (possibly truncated) body.
fn apply_cross_project_token_budget(body: String, max_tokens: Option<u64>) -> String {
    let Some(max_tokens) = max_tokens.filter(|t| *t > 0) else {
        return body;
    };
    let max_bytes = (max_tokens as usize).saturating_mul(4);
    if max_bytes == 0 || body.len() <= max_bytes {
        return body;
    }
    // Truncate at the last newline that fits the byte budget so we never emit a
    // half-line, then disclose that the body was cut by the token budget. We scan
    // newline byte offsets directly (UTF-8 safe: `\n` is a single byte and never a
    // continuation byte, so `idx <= max_bytes` is always a valid char boundary —
    // no panic on multibyte match lines).
    let cut_end = body
        .bytes()
        .enumerate()
        .filter(|&(idx, b)| b == b'\n' && idx < max_bytes)
        .map(|(idx, _)| idx + 1)
        .next_back();
    let mut truncated = match cut_end {
        Some(end) => body[..end].to_string(),
        None => String::new(),
    };
    truncated.push_str(&format!(
        "... (truncated to fit max_tokens={max_tokens}; cross-project output is \
         token-bounded — raise max_tokens or query a single project for the full set)\n"
    ));
    truncated
}

/// Honest refusal of the cross-project params that are genuinely NOT supported
/// on the cross-project read path — as distinct from the scoping that IS now
/// honored. B1 threads `path_prefix`/`language`/noise/`limit` through the
/// engine's option-honoring `search_*_with_options` (so cross-project scoping
/// behaves identically to single-project; D11 + D14), and those are NO LONGER
/// refused here. What remains unsupported, and is therefore still refused
/// loudly rather than silently ignored:
/// * `search_text` `structural` — ast-grep runs a separate single-project
///   pipeline, not `search_text_with_options`, so it has no cross-project path;
/// * `find_references` `path`/`symbol_kind`/`direction` — single-project symbol
///   SELECTORS / implementations-mode direction, which have no cross-project
///   meaning and no option-honoring engine entry point.
///
/// Returns `Err(message)` for the FIRST still-unsupported param present, else
/// `Ok(())`. `kind`, `path_prefix`, and `language` are honored and not rejected.
fn reject_unsupported_cross_project_scoping(
    tool_name: &str,
    params: &serde_json::Value,
) -> anyhow::Result<()> {
    fn refuse(param: &str) -> anyhow::Result<()> {
        anyhow::bail!(
            "{param} scoping is not supported with cross-project targeting; \
             query a single project for scoped results."
        );
    }
    // A lenient peek of ONLY the still-unsupported fields; unrelated/invalid
    // sibling fields must not make this check fail (the real decode happens in
    // the per-tool branch). `null`/absent fields deserialize to `None`.
    #[derive(serde::Deserialize, Default)]
    struct ScopePeek {
        #[serde(default)]
        path: Option<String>,
        #[serde(default)]
        symbol_kind: Option<String>,
        #[serde(default)]
        direction: Option<String>,
        #[serde(default)]
        structural: Option<bool>,
    }
    let peek: ScopePeek = serde_json::from_value(params.clone()).unwrap_or_default();
    let present = |v: &Option<String>| v.as_deref().is_some_and(|s| !s.trim().is_empty());

    match tool_name {
        "search_text" => {
            if peek.structural == Some(true) {
                return refuse("structural");
            }
        }
        "find_references" => {
            if present(&peek.path) {
                return refuse("path");
            }
            if present(&peek.symbol_kind) {
                return refuse("symbol_kind");
            }
            if present(&peek.direction) {
                return refuse("direction");
            }
        }
        _ => {}
    }
    Ok(())
}

/// Feature 012 (Phase 3): execute one of the three cross-project READ verbs
/// against the session's working set and format the attributed [`ProjectHit`]
/// results with `── project: <id> ──` section headers (flat, no header, when a
/// single project is targeted). This is reached ONLY when `targets` is not the
/// single active project; the active-project path never enters here.
///
/// Read-only by construction: it calls the overlay-aware `WorkingSet` search
/// methods, which never mutate an overlay (the no-overlay-writes invariant).
///
/// SCOPING (B1): the cross-project path builds the SAME `SymbolSearchOptions`/
/// `TextSearchOptions` the single-project path builds (via
/// `search_symbols_options_from_input` / `search_text_options_from_input`) and
/// threads them through the engine's option-honoring `WorkingSet` search, so
/// `path_prefix`/`language`/noise/`limit` ARE honored cross-project (D11) and
/// each project's hits are `result_limit`-bounded and tier-ranked (D14). The
/// params that genuinely have no cross-project path are still REFUSED up front
/// ([`reject_unsupported_cross_project_scoping`]): `search_text` `structural`
/// and `find_references` `path`/`symbol_kind`/`direction`. The output is also
/// BOUNDED: a total-hit cap (caller `limit`, else
/// [`CROSS_PROJECT_DEFAULT_RESULT_CAP`]) and the caller's optional `max_tokens`
/// budget are applied by the formatters, which disclose any truncation.
fn execute_cross_project_read(
    tool_name: &str,
    params: serde_json::Value,
    targets: Targets,
    working_set: &WorkingSet,
) -> anyhow::Result<String> {
    ensure_targets_open(&targets, working_set).map_err(|message| anyhow::anyhow!("{message}"))?;

    // Honest refusal of the DEFERRED per-project derived-index scoping when
    // combined with cross-project targeting (rather than silently dropping it).
    reject_unsupported_cross_project_scoping(tool_name, &params)?;

    // Whether to render per-project section headers: a single targeted project
    // renders flat (one project's hits, no header); 2+ get headers.
    let multi = match &targets {
        Targets::One(_) => false,
        Targets::Subset(ids) => ids.len() > 1,
        Targets::All => working_set.len() > 1,
    };

    match tool_name {
        "search_symbols" => {
            let input: SearchSymbolsInput = decode_params(params)?;
            let query = input.query.as_deref().unwrap_or("").trim();
            if query.is_empty() {
                anyhow::bail!(
                    "cross-project search_symbols requires a non-empty query \
                     (browse mode is single-project only)."
                );
            }
            // Build the SAME options the single-project path builds, so cross-
            // project scoping (path_prefix, language, noise) + per-project
            // result_limit ranking behave identically (B1: D11 + D14). An unknown
            // language is an honest error, not a silent drop.
            let options = search_symbols_options_from_input(&input)
                .map_err(|message| anyhow::anyhow!(message))?;
            let cap = cross_project_result_cap(input.limit);
            let hits = working_set.search_symbols(&targets, query, input.kind.as_deref(), &options);
            let body = format_cross_project_symbols(&hits, query, multi, cap);
            Ok(apply_cross_project_token_budget(body, input.max_tokens))
        }
        "search_text" => {
            let input: SearchTextInput = decode_params(params)?;
            let regex = input.regex.unwrap_or(false);
            // Same options the single-project path builds (path_prefix, language,
            // noise, max_per_file, case/word, glob, ranked) so cross-project text
            // scoping is honored (B1: D11). `structural` was already refused above
            // (it runs a separate ast-grep pipeline).
            let mut options = search_text_options_from_input(&input)
                .map_err(|message| anyhow::anyhow!(message))?;
            // `context` (surrounding lines) is NOT rendered by the cross-project
            // text formatter, so drop it rather than compute-and-discard it or
            // imply it is honored. Rendering context / group_by / follow_refs
            // cross-project is a display concern tracked under A1b, not B1 scoping.
            options.context = None;
            let cap = cross_project_result_cap(input.limit);
            let results = working_set
                .search_text(
                    &targets,
                    input.query.as_deref(),
                    input.terms.as_deref(),
                    regex,
                    &options,
                )
                .map_err(|error| anyhow::anyhow!("text search failed: {error:?}"))?;
            let body = format_cross_project_text(&results, multi, cap);
            Ok(apply_cross_project_token_budget(body, input.max_tokens))
        }
        "find_references" => {
            let input: FindReferencesInput = decode_params(params)?;
            let name = input.name.trim();
            if name.is_empty() {
                anyhow::bail!("cross-project find_references requires a non-empty name.");
            }
            let kind_filter = match input.kind.as_deref() {
                Some("call") => Some(crate::domain::ReferenceKind::Call),
                Some("import") => Some(crate::domain::ReferenceKind::Import),
                Some("type_usage") => Some(crate::domain::ReferenceKind::TypeUsage),
                Some("macro_use") => Some(crate::domain::ReferenceKind::MacroUse),
                Some("value_use") => Some(crate::domain::ReferenceKind::ValueUse),
                _ => None,
            };
            let cap = cross_project_result_cap(input.limit);
            let hits = working_set.find_references(&targets, name, kind_filter, false);
            let body = format_cross_project_references(&hits, name, multi, cap);
            Ok(apply_cross_project_token_budget(body, input.max_tokens))
        }
        other => anyhow::bail!("'{other}' is not a cross-project read verb"),
    }
}

/// Format cross-project symbol hits, grouped by project (`── project: <id> ──`
/// headers when `multi`). Within a project, hits arrive already tier-sorted from
/// the view search.
///
/// BOUNDED: at most `cap` hits are rendered across all projects (the shared
/// cross-project total cap). When `hits.len() > cap` the surplus is dropped and
/// the truncation is disclosed honestly (Principle III) with the shown/total
/// counts and the corrective hint. The cap is applied in working-set/tier order,
/// so the highest-ranked hits per project survive the cut.
fn format_cross_project_symbols(
    hits: &[crate::live_index::view::ProjectHit<crate::live_index::view::ViewSymbolHit>],
    query: &str,
    multi: bool,
    cap: usize,
) -> String {
    if hits.is_empty() {
        return format!("No symbols matching '{query}' in the targeted project(s).");
    }
    let total = hits.len();
    let shown = total.min(cap);
    let mut out = String::new();
    let mut current: Option<&str> = None;
    if shown < total {
        out.push_str(&format!(
            "{shown} of {total} matches across projects (truncated; cross-project results \
             are capped — pass a larger `limit` or query a single project for the full set)\n"
        ));
    } else {
        out.push_str(&format!("{total} matches across projects\n"));
    }
    for ph in hits.iter().take(shown) {
        if multi && current != Some(ph.project_id.as_str()) {
            current = Some(ph.project_id.as_str());
            out.push_str(&format!(
                "\n\u{2500}\u{2500} project: {} \u{2500}\u{2500}\n",
                ph.project_id
            ));
        }
        let h = &ph.hit;
        out.push_str(&format!(
            "  {}: {} {}  ({})\n",
            h.line, h.kind, h.name, h.path
        ));
    }
    out
}

/// Format cross-project text-search results, grouped by project. Each project's
/// `TextSearchResult` renders its files and per-line matches.
///
/// BOUNDED: at most `cap` per-line matches are rendered across all files in all
/// projects (the unbounded element here is the cumulative match-line count, not
/// the file count). Once `cap` lines have been emitted, the rest are dropped and
/// the truncation disclosed (shown/total + corrective hint). A project/file with
/// no remaining budget is skipped, so headers are only emitted for content that
/// is actually rendered.
fn format_cross_project_text(
    results: &[crate::live_index::view::ProjectHit<crate::live_index::search::TextSearchResult>],
    multi: bool,
    cap: usize,
) -> String {
    let total: usize = results.iter().map(|ph| ph.hit.total_matches).sum();
    if total == 0 {
        return "No text matches in the targeted project(s).".to_string();
    }

    // Render into a body first while counting emitted match lines, so the header
    // can honestly state shown-vs-total once we know how many we actually wrote.
    let mut body = String::new();
    let mut shown = 0usize;
    'projects: for ph in results {
        if ph.hit.files.is_empty() {
            continue;
        }
        let mut header_written = false;
        for file in &ph.hit.files {
            if file.matches.is_empty() {
                continue;
            }
            if shown >= cap {
                break 'projects;
            }
            // Emit the project header lazily, only when this project contributes
            // at least one rendered line within budget.
            if multi && !header_written {
                body.push_str(&format!(
                    "\n\u{2500}\u{2500} project: {} \u{2500}\u{2500}\n",
                    ph.project_id
                ));
                header_written = true;
            }
            body.push_str(&format!("{}\n", file.path));
            for m in &file.matches {
                if shown >= cap {
                    break 'projects;
                }
                body.push_str(&format!("  {}: {}\n", m.line_number, m.line));
                shown += 1;
            }
        }
    }

    let mut out = String::new();
    if shown < total {
        out.push_str(&format!(
            "{shown} of {total} text matches across projects (truncated; cross-project results \
             are capped — pass a larger `limit` or query a single project for the full set)\n"
        ));
    } else {
        out.push_str(&format!("{total} text matches across projects\n"));
    }
    out.push_str(&body);
    out
}

/// Format cross-project reference hits, grouped by project.
///
/// BOUNDED: at most `cap` reference hits are rendered across all projects; the
/// surplus is dropped and the truncation disclosed (shown/total + corrective
/// hint), mirroring [`format_cross_project_symbols`].
fn format_cross_project_references(
    hits: &[crate::live_index::view::ProjectHit<(String, crate::domain::ReferenceRecord)>],
    name: &str,
    multi: bool,
    cap: usize,
) -> String {
    if hits.is_empty() {
        return format!("No references to '{name}' in the targeted project(s).");
    }
    let total = hits.len();
    let shown = total.min(cap);
    let mut out = String::new();
    let mut current: Option<&str> = None;
    if shown < total {
        out.push_str(&format!(
            "{shown} of {total} references across projects (truncated; cross-project results \
             are capped — pass a larger `limit` or query a single project for the full set)\n"
        ));
    } else {
        out.push_str(&format!("{total} references across projects\n"));
    }
    for ph in hits.iter().take(shown) {
        if multi && current != Some(ph.project_id.as_str()) {
            current = Some(ph.project_id.as_str());
            out.push_str(&format!(
                "\n\u{2500}\u{2500} project: {} \u{2500}\u{2500}\n",
                ph.project_id
            ));
        }
        let (path, reference) = &ph.hit;
        // `line_range` is zero-indexed in the engine; display 1-based.
        out.push_str(&format!(
            "  {}:{} [{:?}] {}\n",
            path,
            reference.line_range.0 + 1,
            reference.kind,
            reference.name
        ));
    }
    out
}

async fn execute_tool_call(
    runtime: SessionRuntime,
    tool_name: &str,
    params: serde_json::Value,
) -> anyhow::Result<String> {
    runtime.token_stats.record_tool_call(tool_name);

    // Feature 012 (Phase 3): cross-project READ route. For the three read verbs
    // ONLY, peek the `project`/`projects` params and resolve a `Targets`. When it
    // is the single active project (the default — both params omitted — or an
    // explicit `project=<active id>`), fall through to the UNCHANGED per-project
    // dispatch below (byte-identical no-regression). Any other target shape
    // (`Subset`, `All`, or a single non-active project) is served here from the
    // session's working set with attributed `── project: <id> ──` output. Edits,
    // analyze_file_impact, orient, and meta verbs are NEVER routed here — they
    // stay single-project on the active project (cross-project writes deferred).
    if is_cross_project_read_verb(tool_name) {
        #[derive(serde::Deserialize)]
        struct CrossProjectPeek {
            #[serde(default)]
            project: Option<String>,
            #[serde(default)]
            projects: Option<Vec<String>>,
        }
        // A lenient peek: unrelated/invalid sibling fields must NOT make the real
        // tool call fail here, so only the two targeting fields are extracted. A
        // genuine targeting CONTRACT violation (e.g. both params set) IS surfaced
        // as an error here — the freshness gate (`resolve_cross_project_targets`)
        // swallows that same error, so it never refreshes for an invalid call.
        let peek: CrossProjectPeek =
            serde_json::from_value(params.clone()).unwrap_or(CrossProjectPeek {
                project: None,
                projects: None,
            });
        let targets = resolve_targets(
            peek.project.as_deref(),
            peek.projects.as_deref(),
            &runtime.project_id,
        )
        .map_err(|message| anyhow::anyhow!("{message}"))?;
        if !targets_is_single_active(&targets, &runtime.project_id) {
            let working_set = runtime.working_set.read();
            return execute_cross_project_read(tool_name, params, targets, &working_set);
        }
        // else: single active project -> fall through to the unchanged path.
    }

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

    // ── Feature 012f — bases-table orphan GC (SC-002-safe) ──────────────────────
    //
    // A genuine commit-advance interns the snapshot under a NEW BaseKey and swaps
    // the working-set entry, leaving the OLD key referenced only by the map: an
    // orphan that grows the table unbounded on a long-lived multi-commit daemon.
    // The GC evicts orphans (strong_count == 1) and MUST NOT evict a base a live
    // working set still holds (strong_count > 1) — evicting it would let a later
    // intern mint a SECOND Arc per key (the SC-002 violation).
    #[test]
    fn test_gc_orphaned_bases_evicts_orphan_keeps_live() {
        use crate::live_index::store::LiveIndex;
        use crate::live_index::view::{CommitId, WorkingSet};

        let state = DaemonState::new();
        let root = "/synthetic/gc-root";

        // OLD-commit base: interned, then no longer referenced by any working set
        // (simulating the entry-swap a commit-advance performs). Map-only Arc.
        let orphan_key = BaseKey::new(root, CommitId::Sha("old-commit".to_string()));
        state.bases.write().insert(
            orphan_key.clone(),
            Arc::new(IndexBase::new(
                orphan_key.clone(),
                Arc::new(LiveIndex::empty_live_index()),
                1,
            )),
        );

        // NEW-commit base: interned AND held by a live working set (the post-advance
        // state). The working-set entry stores a CLONE of the SAME map Arc, so the
        // map value's strong_count is > 1 — the liveness signal the GC relies on.
        let live_key = BaseKey::new(root, CommitId::Sha("new-commit".to_string()));
        let live_base = Arc::new(IndexBase::new(
            live_key.clone(),
            Arc::new(LiveIndex::empty_live_index()),
            2,
        ));
        state
            .bases
            .write()
            .insert(live_key.clone(), Arc::clone(&live_base));
        let working_set = Arc::new(RwLock::new(WorkingSet::new()));
        working_set.write().add("proj", Arc::clone(&live_base));
        // Drop our local strong ref so ONLY the map + the working-set entry hold the
        // live base (strong_count == 2). Without this, our local clone would mask a
        // buggy GC that relies on an accidental extra ref.
        drop(live_base);

        // Non-vacuity: BEFORE the sweep the orphan IS in the table (the leak the GC
        // closes) and both keys are present.
        {
            let bases = state.bases.read();
            assert!(
                bases.contains_key(&orphan_key),
                "precondition: orphan must be present before GC (proves the leak)"
            );
            assert_eq!(bases.len(), 2, "precondition: both bases interned");
            assert_eq!(
                Arc::strong_count(bases.get(&orphan_key).unwrap()),
                1,
                "orphan must be map-only (the eviction signal)"
            );
            assert_eq!(
                Arc::strong_count(bases.get(&live_key).unwrap()),
                2,
                "live base is held by map + working-set entry"
            );
        }

        state.gc_orphaned_bases();

        let bases = state.bases.read();
        // Orphan evicted.
        assert!(
            !bases.contains_key(&orphan_key),
            "GC must evict the orphaned (map-only) base"
        );
        // SC-002 guard: the still-referenced base survives — evicting it would let a
        // later intern mint a second Arc for this key.
        assert!(
            bases.contains_key(&live_key),
            "GC must NOT evict a base a live working set still references"
        );
        assert_eq!(bases.len(), 1, "only the orphan is removed");
        // The surviving map value is STILL the SAME Arc the working-set entry holds.
        let table_live = bases.get(&live_key).unwrap();
        let ws = working_set.read();
        let entry = ws.get("proj").expect("live entry present");
        assert!(
            Arc::ptr_eq(table_live, &entry.base),
            "SC-002: survivor is the same shared Arc the working set holds"
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

    // ── Feature 012f D15 overlay-WRITER tests ───────────────────────────────────
    //
    // These drive the REAL daemon dispatch path: `DaemonState::new()` ->
    // `open_project_session` -> `session_runtime()` (the C2 wiring under test) ->
    // `execute_tool_call`. A test that hand-built a `SymForgeServer` and set
    // `session_working_set` directly would bypass C2 and be a false-green (AC1).

    /// Write a project source file under `src/` and return the relative path.
    fn write_overlay_fixture(dir: &TempDir, rel: &str, content: &str) -> String {
        let abs = dir.path().join(rel);
        if let Some(parent) = abs.parent() {
            std::fs::create_dir_all(parent).expect("fixture parent dir");
        }
        std::fs::write(&abs, content).expect("write fixture source");
        rel.to_string()
    }

    /// Read the overlay `Upsert` delta (if any) for `rel_path` under a session's
    /// own working set. Returns `true` when an `Upsert` delta is present.
    fn session_overlay_has_upsert(state: &DaemonState, session_id: &str, rel_path: &str) -> bool {
        let sessions = state.sessions.read();
        let session = sessions.get(session_id).expect("session present");
        let working_set = session.working_set.read();
        let Some(entry) = working_set.get(&session.active_project_id) else {
            return false;
        };
        matches!(
            entry.overlay.deltas.get(rel_path),
            Some(crate::live_index::view::FileDelta::Upsert(_))
        )
    }

    /// AC1 + AC2: the WRITER populates the dormant overlay seam; `get_symbol`
    /// returns the edit via the BASE. Edit a function via
    /// `execute_tool_call(replace_symbol_body)` through the real C2 wiring, then:
    /// (AC2) assert the per-session overlay carries an `Upsert` delta for the path
    /// — this proves the write-seam works — and (AC1) assert a subsequent
    /// `execute_tool_call(get_symbol)` in the SAME session returns the EDITED body.
    /// The read no longer consults the overlay (that branch was removed as
    /// redundant); it returns the edit via the shared live index (the base), which
    /// `reindex_after_write` already advanced. The runtime is obtained from
    /// `session_runtime()`, so a `None` field (C2 not wired) would leave the overlay
    /// empty and AC2 would FAIL.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_d15_overlay_writer_populates_seam_read_via_base() {
        let project = project_dir("symforge-d15-rww");
        let rel = write_overlay_fixture(&project, "src/lib.rs", "pub fn target() -> u32 { 1 }\n");

        let state = DaemonState::new();
        let opened = state
            .open_project_session(OpenProjectRequest {
                project_root: project.path().display().to_string(),
                client_name: "claude".to_string(),
                pid: Some(1),
            })
            .expect("open session");

        // Sanity: the seeded overlay starts empty (no Upsert yet).
        assert!(
            !session_overlay_has_upsert(&state, &opened.session_id, &rel),
            "overlay must be empty before the edit"
        );

        // Prove C2 wiring is LIVE: the per-session runtime carries Some, while the
        // shared instance keeps None.
        let runtime_edit = state
            .session_runtime(&opened.session_id)
            .expect("session runtime");
        assert!(
            runtime_edit.server.session_working_set.is_some(),
            "session_runtime() must set session_working_set (C2 wiring)"
        );
        {
            let projects = state.projects.read();
            let project_inst = projects.get(&opened.project_id).expect("project instance");
            assert!(
                project_inst.server.session_working_set.is_none(),
                "shared project.server must stay None (no cross-session leak, SC-003)"
            );
        }

        let edited_body = "pub fn target() -> u32 { 999 }";
        let edit_out = execute_tool_call(
            runtime_edit,
            "replace_symbol_body",
            serde_json::json!({
                "path": rel,
                "name": "target",
                "new_body": edited_body,
            }),
        )
        .await
        .expect("replace_symbol_body dispatch");
        assert!(
            !edit_out.contains("Error"),
            "edit must succeed, got: {edit_out}"
        );

        // AC2: the WRITER seam is live — an Upsert delta exists for the path.
        assert!(
            session_overlay_has_upsert(&state, &opened.session_id, &rel),
            "overlay writer must carry an Upsert delta after the edit (AC2)"
        );

        // AC1: a fresh runtime in the SAME session returns the edit via the BASE
        // (the shared live index, advanced by reindex_after_write). The overlay read
        // branch was removed as redundant; force_refresh bypasses the session symbol
        // cache so the base read is genuinely exercised.
        let runtime_read = state
            .session_runtime(&opened.session_id)
            .expect("session runtime 2");
        let read_out = execute_tool_call(
            runtime_read,
            "get_symbol",
            serde_json::json!({
                "path": rel,
                "name": "target",
                "force_refresh": true,
            }),
        )
        .await
        .expect("get_symbol dispatch");
        assert!(
            read_out.contains("999"),
            "get_symbol must return the EDITED body via the base, got: {read_out}"
        );
    }

    /// AC5: no cross-session leak. Two sessions on the SAME project; edit in A;
    /// assert B's working set carries NO Upsert delta for the path (the FATAL fix
    /// / SC-003). B sees the base content via the shared index, but the OVERLAY
    /// delta must never appear in B's per-session working set.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_d15_overlay_no_cross_session_leak() {
        let project = project_dir("symforge-d15-leak");
        let rel = write_overlay_fixture(&project, "src/lib.rs", "pub fn shared() -> u32 { 1 }\n");

        let state = DaemonState::new();
        let session_a = state
            .open_project_session(OpenProjectRequest {
                project_root: project.path().display().to_string(),
                client_name: "session-a".to_string(),
                pid: Some(1),
            })
            .expect("session a");
        let session_b = state
            .open_project_session(OpenProjectRequest {
                project_root: project.path().join(".").display().to_string(),
                client_name: "session-b".to_string(),
                pid: Some(2),
            })
            .expect("session b");
        assert_eq!(
            session_a.project_id, session_b.project_id,
            "both sessions share the same project"
        );
        assert_ne!(session_a.session_id, session_b.session_id);

        let runtime_a = state
            .session_runtime(&session_a.session_id)
            .expect("runtime a");
        execute_tool_call(
            runtime_a,
            "replace_symbol_body",
            serde_json::json!({
                "path": rel,
                "name": "shared",
                "new_body": "pub fn shared() -> u32 { 777 }",
            }),
        )
        .await
        .expect("edit in session a");

        // Session A's overlay HAS the upsert.
        assert!(
            session_overlay_has_upsert(&state, &session_a.session_id, &rel),
            "session A overlay must carry the edit"
        );
        // Session B's overlay must NOT (the leak guard).
        assert!(
            !session_overlay_has_upsert(&state, &session_b.session_id, &rel),
            "session B overlay must NOT see session A's edit (SC-003)"
        );
    }

    /// AC6: partial-parse case. Edit one function, but leave a syntactically broken
    /// region elsewhere in the file so the parse is partial. The overlay upsert
    /// must still happen (PartialParse yields a valid IndexedFile, never an error),
    /// and `get_symbol` for the cleanly-parsed edited symbol must return the edited
    /// body. The fixture is NOT clean-parsing — it ends with a broken fragment.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_d15_overlay_partial_parse_still_upserts() {
        let project = project_dir("symforge-d15-partial");
        // Two functions plus a trailing broken region. After editing `good`, the
        // file still contains the broken tail, forcing a partial parse.
        let rel = write_overlay_fixture(
            &project,
            "src/lib.rs",
            "pub fn good() -> u32 { 1 }\n\nfn broken( {\n",
        );

        let state = DaemonState::new();
        let opened = state
            .open_project_session(OpenProjectRequest {
                project_root: project.path().display().to_string(),
                client_name: "claude".to_string(),
                pid: Some(1),
            })
            .expect("open session");

        let runtime_edit = state.session_runtime(&opened.session_id).expect("runtime");
        let edit_out = execute_tool_call(
            runtime_edit,
            "replace_symbol_body",
            serde_json::json!({
                "path": rel,
                "name": "good",
                "new_body": "pub fn good() -> u32 { 424242 }",
            }),
        )
        .await
        .expect("edit dispatch");
        assert!(
            !edit_out.contains("Error writing"),
            "edit must commit despite the partial-parse tail, got: {edit_out}"
        );

        // The upsert must NOT be skipped just because the file partially failed.
        assert!(
            session_overlay_has_upsert(&state, &opened.session_id, &rel),
            "overlay upsert must happen on a partial parse (AC6)"
        );

        let runtime_read = state
            .session_runtime(&opened.session_id)
            .expect("runtime 2");
        let read_out = execute_tool_call(
            runtime_read,
            "get_symbol",
            serde_json::json!({
                "path": rel,
                "name": "good",
                "force_refresh": true,
            }),
        )
        .await
        .expect("read dispatch");
        assert!(
            read_out.contains("424242"),
            "get_symbol must return the edited body of the cleanly-parsed symbol (AC6), got: {read_out}"
        );
    }

    /// AC3 + AC4: local-stdio parity / unedited-file fall-through. With the field
    /// `None` (a bare in-process server — the local-stdio shape), `get_symbol`
    /// falls through to the base unchanged. Drives the SAME `get_symbol` method
    /// the daemon path uses, proving the `None` branch is byte-identical.
    #[tokio::test]
    async fn test_d15_overlay_none_falls_through_to_base() {
        let project = project_dir("symforge-d15-none");
        let rel =
            write_overlay_fixture(&project, "src/lib.rs", "pub fn base_only() -> u32 { 55 }\n");

        // Build a real local index over the fixture, then a server with the field
        // left at its `None` default (local-stdio shape).
        let index = crate::live_index::LiveIndex::load(project.path()).expect("load index");
        let server = crate::protocol::SymForgeServer::new(
            index,
            "d15-none".to_string(),
            Arc::new(parking_lot::Mutex::new(
                crate::watcher::WatcherInfo::default(),
            )),
            Some(project.path().to_path_buf()),
            None,
        );
        assert!(
            server.session_working_set.is_none(),
            "default server must have a None overlay (local-stdio parity)"
        );

        let input: GetSymbolInput = serde_json::from_value(serde_json::json!({
            "path": rel,
            "name": "base_only",
            "force_refresh": true,
        }))
        .expect("decode get_symbol input");
        let out = server.get_symbol(Parameters(input)).await;
        assert!(
            out.contains("55"),
            "None overlay must fall through to the base (AC3/AC4), got: {out}"
        );
    }

    // ── Feature 012 Phase 3 — resolve_targets contract (FR-004 / SC-001) ─────────
    #[test]
    fn test_resolve_targets_contract() {
        let active = "project-active";

        // Both omitted -> One(active) -> today's behavior.
        assert_eq!(
            resolve_targets(None, None, active).unwrap(),
            Targets::One(active.to_string())
        );

        // A single explicit id (the active one) -> One(active) (still the fast path).
        assert_eq!(
            resolve_targets(Some(active), None, active).unwrap(),
            Targets::One(active.to_string())
        );

        // A single explicit OTHER id -> One(other) (cross-project, single target).
        assert_eq!(
            resolve_targets(Some("project-other"), None, active).unwrap(),
            Targets::One("project-other".to_string())
        );

        // ["*"] -> All.
        assert_eq!(
            resolve_targets(None, Some(&["*".to_string()]), active).unwrap(),
            Targets::All
        );

        // An explicit subset -> Subset.
        assert_eq!(
            resolve_targets(
                None,
                Some(&["project-a".to_string(), "project-b".to_string()]),
                active
            )
            .unwrap(),
            Targets::Subset(vec!["project-a".to_string(), "project-b".to_string()])
        );

        // Mutually exclusive -> error.
        assert!(
            resolve_targets(Some("project-a"), Some(&["project-b".to_string()]), active).is_err()
        );

        // Empty projects -> error (no silent All).
        assert!(resolve_targets(None, Some(&[]), active).is_err());

        // A filesystem path in `project=` -> corrective error pointing at add:true.
        let err = resolve_targets(Some("E:/project/symforge"), None, active).unwrap_err();
        assert!(
            err.contains("index_folder") && err.contains("add"),
            "path-in-project error must point at index_folder(add:true): {err}"
        );
        assert!(resolve_targets(Some("/abs/path"), None, active).is_err());

        // A path inside `projects=` -> error too.
        assert!(resolve_targets(None, Some(&["src/lib.rs".to_string()]), active).is_err());

        // Empty/whitespace project id -> error.
        assert!(resolve_targets(Some("   "), None, active).is_err());
    }

    #[test]
    fn test_targets_is_single_active() {
        let active = "project-active";
        assert!(targets_is_single_active(
            &Targets::One(active.to_string()),
            active
        ));
        assert!(!targets_is_single_active(
            &Targets::One("project-other".to_string()),
            active
        ));
        assert!(!targets_is_single_active(&Targets::All, active));
        assert!(!targets_is_single_active(
            &Targets::Subset(vec![active.to_string()]),
            active
        ));
    }

    // ── Feature 012 Phase 2 — set_active_project / remove_project_from_session ───
    #[test]
    fn test_set_active_and_remove_project_management() {
        let project_a = project_dir("symforge-mgmt-a");
        let project_b = project_dir("symforge-mgmt-b");
        std::fs::write(project_b.path().join("src").join("b.rs"), "fn b() {}\n")
            .expect("write source b");
        let state = DaemonState::new();

        let opened = state
            .open_project_session(OpenProjectRequest {
                project_root: project_a.path().display().to_string(),
                client_name: "mgmt".to_string(),
                pid: None,
            })
            .expect("open A");
        let active_a = opened.project_id.clone();

        // Additively open B in the same session.
        state
            .index_folder_for_session(
                &opened.session_id,
                IndexFolderInput {
                    path: project_b.path().display().to_string(),
                    idempotency_key: None,
                    add: Some(true),
                },
            )
            .expect("additive open B");
        let project_b_id =
            project_key(&canonical_project_root(project_b.path()).expect("canonical b"));

        // Working set now holds both; A is still active.
        {
            let sessions = state.sessions.read();
            let session = sessions.get(&opened.session_id).expect("session");
            let ws = session.working_set.read();
            assert_eq!(ws.len(), 2, "working set holds A and B");
            assert_eq!(session.active_project_id, active_a, "A still active");
            assert!(ws.get(&active_a).is_some());
            assert!(ws.get(&project_b_id).is_some());
        }

        // Cannot activate a project that is not open.
        assert!(
            !state.set_active_project(&opened.session_id, "project-not-open"),
            "activating a non-open project must fail"
        );
        // Activate B (it is open).
        assert!(
            state.set_active_project(&opened.session_id, &project_b_id),
            "activating an open project must succeed"
        );
        assert_eq!(
            state
                .sessions
                .read()
                .get(&opened.session_id)
                .unwrap()
                .active_project_id,
            project_b_id,
            "B is now active"
        );

        // Cannot remove the ACTIVE project (B).
        assert!(
            !state.remove_project_from_session(&opened.session_id, &project_b_id),
            "removing the active project must be refused"
        );
        // Remove A (now non-active) -> succeeds and drops it from the working set.
        assert!(
            state.remove_project_from_session(&opened.session_id, &active_a),
            "removing a non-active open project must succeed"
        );
        {
            let sessions = state.sessions.read();
            let ws = sessions.get(&opened.session_id).unwrap().working_set.read();
            assert_eq!(ws.len(), 1, "only B remains");
            assert!(ws.get(&active_a).is_none(), "A removed");
            assert!(ws.get(&project_b_id).is_some(), "B remains");
        }
        // A had only this session -> it should be torn down as a project.
        assert!(
            !state
                .list_projects()
                .iter()
                .any(|p| p.project_id == active_a),
            "project A removed after its last session reference dropped"
        );

        let _ = state.close_session(&opened.session_id);
    }

    // ── Feature 012 Phase 2 — additive open keeps active project + adds to WS ────
    #[test]
    fn test_index_folder_additive_keeps_active_and_adds_to_working_set() {
        let project_a = project_dir("symforge-additive-a");
        let project_b = project_dir("symforge-additive-b");
        std::fs::write(project_b.path().join("src").join("b.rs"), "fn b() {}\n")
            .expect("write source b");
        let state = DaemonState::new();

        let opened = state
            .open_project_session(OpenProjectRequest {
                project_root: project_a.path().display().to_string(),
                client_name: "additive".to_string(),
                pid: None,
            })
            .expect("open A");
        let active_a = opened.project_id.clone();

        let output = state
            .index_folder_for_session(
                &opened.session_id,
                IndexFolderInput {
                    path: project_b.path().display().to_string(),
                    idempotency_key: None,
                    add: Some(true),
                },
            )
            .expect("additive open B");
        assert!(
            output.contains("added to working set"),
            "additive open should report working-set addition: {output}"
        );

        let sessions = state.sessions.read();
        let session = sessions.get(&opened.session_id).expect("session");
        // Active project UNCHANGED (still A) — additive does not retarget.
        assert_eq!(session.active_project_id, active_a, "A still active");
        let ws = session.working_set.read();
        assert_eq!(ws.len(), 2, "working set has A and B after additive open");
        // Both overlays remain EMPTY (no-overlay-writes invariant).
        for entry in ws.iter() {
            assert_eq!(
                entry.overlay.delta_count(),
                0,
                "no overlay write on additive open (project {})",
                entry.project_id
            );
        }
    }

    // ── Feature 012 Phase 2 — retarget re-seeds the working set's active entry ───
    #[test]
    fn test_retarget_reseeds_working_set_active_entry() {
        let project_a = project_dir("symforge-reseed-a");
        let project_b = project_dir("symforge-reseed-b");
        std::fs::write(project_b.path().join("src").join("b.rs"), "fn b() {}\n")
            .expect("write source b");
        let state = DaemonState::new();

        let opened = state
            .open_project_session(OpenProjectRequest {
                project_root: project_a.path().display().to_string(),
                client_name: "reseed".to_string(),
                pid: None,
            })
            .expect("open A");
        let active_a = opened.project_id.clone();

        // Retarget (add: None) to B — the destructive path.
        state
            .index_folder_for_session(
                &opened.session_id,
                IndexFolderInput {
                    path: project_b.path().display().to_string(),
                    idempotency_key: None,
                    add: None,
                },
            )
            .expect("retarget to B");
        let project_b_id =
            project_key(&canonical_project_root(project_b.path()).expect("canonical b"));

        let sessions = state.sessions.read();
        let session = sessions.get(&opened.session_id).expect("session");
        assert_eq!(session.active_project_id, project_b_id, "active is now B");
        let ws = session.working_set.read();
        // The working set's active entry was re-seeded to B (the retarget seam fix):
        // the stale A entry is gone and B is present and consistent with active.
        assert_eq!(ws.len(), 1, "single re-seeded active entry after retarget");
        assert!(
            ws.get(&active_a).is_none(),
            "stale pre-retarget entry (A) must be dropped"
        );
        let entry = ws
            .get(&project_b_id)
            .expect("working set re-seeded with the new active project B");
        assert_eq!(
            entry.overlay.delta_count(),
            0,
            "re-seeded overlay must be empty"
        );
        assert!(
            entry.overlay.is_valid_against(&entry.base),
            "re-seeded overlay fenced to its base"
        );
    }

    // ── Feature 012 Phase 0/1/2 — multi-threaded lock stress (critique #4) ───────
    //
    // The mandated `--test-threads=1` suite CANNOT surface a deadlock on the new
    // `bases -> projects -> sessions` hot path, so THIS test spins REAL OS threads
    // that hammer the lock acquisition order concurrently. Threads churn:
    //   * `open_project_session` (intern_base -> projects.write -> sessions.write),
    //   * `session_runtime` (projects.read -> sessions.read, the hot read path),
    //   * `index_folder_for_session` BOTH ways — the destructive `needs_reassign`
    //     RETARGET (alternating target roots: projects.write mid-function juggling
    //     + working-set re-seed + sessions.write) AND the additive `add:true` path
    //     (load/activate + additive session_ids join + intern + working_set.add) —
    //     this file's trickiest lock juggling, now under contention,
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
            // A second per-thread root to retarget/additively-open INTO under
            // contention (forces `needs_reassign` retarget + add:true paths).
            let retarget_root = project_dir(&format!("symforge-stress-retarget-{t}"));
            let retarget_path = retarget_root.path().display().to_string();
            handles.push(std::thread::spawn(move || {
                // Keep the TempDirs alive for the whole thread body.
                let _private_root = private_root;
                let _retarget_root = retarget_root;
                for i in 0..ITERS {
                    // Alternate between a shared root (contended intern of the same
                    // BaseKey) and this thread's private root (table growth/churn).
                    let root = if i % 2 == 0 {
                        shared_root_paths[(t + i) % shared_root_paths.len()].clone()
                    } else {
                        private_path.clone()
                    };

                    // D17 (atomic open, fail-never): `open_project_session` now
                    // ensures the project AND registers the session under a single
                    // `projects.write()` hold (recovering by reload if a concurrent
                    // `close_session` removed it), so the former "was removed between
                    // check and session registration" race CANNOT occur. Open MUST
                    // succeed under contention — a failure is a real regression, not
                    // a tolerated benign race. (A lock-order inversion would instead
                    // HANG at join and never reach this assert.)
                    let opened = state
                        .open_project_session(OpenProjectRequest {
                            project_root: root.clone(),
                            client_name: "stress".to_string(),
                            pid: Some((t * 1000 + i) as u32),
                        })
                        .expect(
                            "open_project_session must succeed under contention (D17 fail-never)",
                        );

                    // Hot read path: resolves the active project via the new field.
                    // (May be `None` if a concurrent close already tore the project
                    // down — also benign and not a deadlock.)
                    let _ = state.session_runtime(&opened.session_id);

                    // Independent reads exercised concurrently with writers.
                    let _ = state.health();
                    let _ = state.list_projects();
                    let _ = state.list_sessions(&opened.project_id);

                    // Feature 012 — exercise the trickiest lock juggling under
                    // contention. Alternate the two `index_folder_for_session`
                    // code paths so both run concurrently across threads:
                    //   * even i -> destructive RETARGET (add: None) into the
                    //     per-thread retarget root: projects.write mid-function
                    //     juggling + working-set re-seed + sessions.write;
                    //   * odd  i -> ADDITIVE add:true into the same root: additive
                    //     session_ids join + intern + working_set.add.
                    // Both can legitimately fail loud (e.g. the session was closed
                    // by a concurrent worker, or the open lost the benign race);
                    // those errors are tolerated — only a deadlock (hang at join)
                    // or a panic is a failure here.
                    let _ = state.index_folder_for_session(
                        &opened.session_id,
                        IndexFolderInput {
                            path: retarget_path.clone(),
                            idempotency_key: None,
                            add: Some(i % 2 == 1),
                        },
                    );

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
                    add: None,
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

    // ── Feature 012 Phase 2 — close_session reaps ALL referenced projects ────────
    //
    // The advertised multi-project leak fix: a session that additively opened a
    // SECOND project references BOTH (active A + additive B). Closing it must
    // detach from EVERY referenced project and tear down each whose session set
    // then empties — not just the active one. A leak would leave B in the project
    // map (and its watcher running) forever. This runs under a multi-thread Tokio
    // runtime so `activate()` actually spawns real watcher tasks (sync tests get
    // `None` watchers), letting us assert teardown via each project's `stop_token`.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_close_session_reaps_all_working_set_projects_and_watchers() {
        let project_a = project_dir("symforge-reap-a");
        let project_b = project_dir("symforge-reap-b");
        std::fs::write(project_b.path().join("src").join("b.rs"), "fn b() {}\n")
            .expect("write source b");
        let state = DaemonState::new();

        // Open a session bound to A, then additively open B into its working set.
        let opened = state
            .open_project_session(OpenProjectRequest {
                project_root: project_a.path().display().to_string(),
                client_name: "reap".to_string(),
                pid: None,
            })
            .expect("open A");
        let active_a = opened.project_id.clone();
        state
            .index_folder_for_session(
                &opened.session_id,
                IndexFolderInput {
                    path: project_b.path().display().to_string(),
                    idempotency_key: None,
                    add: Some(true),
                },
            )
            .expect("additive open B");
        let project_b_id =
            project_key(&canonical_project_root(project_b.path()).expect("canonical b"));

        // Both projects are open and the session references BOTH (active + additive).
        assert_eq!(
            state.list_projects().len(),
            2,
            "A and B both open before close"
        );

        // Capture each project's watcher stop_token BEFORE closing so we can prove
        // the teardown path (abort_watcher_task -> stop_token = true) ran for BOTH.
        // Under the multi-thread runtime, activate()/reload() spawned real watchers.
        let (token_a, token_b, watcher_a_live, watcher_b_live) = {
            let projects = state.projects.read();
            let a = projects.get(&active_a).expect("A loaded");
            let b = projects.get(&project_b_id).expect("B loaded");
            (
                Arc::clone(&a.stop_token),
                Arc::clone(&b.stop_token),
                a.watcher_task.is_some(),
                b.watcher_task.is_some(),
            )
        };
        assert!(
            watcher_a_live && watcher_b_live,
            "both projects must have a live watcher task before close \
             (A: {watcher_a_live}, B: {watcher_b_live})"
        );
        assert!(
            !token_a.load(Ordering::Acquire) && !token_b.load(Ordering::Acquire),
            "watcher stop tokens must be unset (false) while the projects are live"
        );

        // Close the only session.
        let closed = state
            .close_session(&opened.session_id)
            .expect("close session");
        // Wire contract: reported fields describe the ACTIVE project (A).
        assert_eq!(
            closed.project_id, active_a,
            "reported pid is the active project A"
        );
        assert_eq!(closed.remaining_sessions, 0);
        assert!(closed.project_removed, "active project A reaped");

        // BOTH projects reaped (the leak fix): the map is empty, not just A gone.
        assert!(
            state.list_projects().is_empty(),
            "close_session must reap BOTH A and B, leaving no open project: {:?}",
            state.list_projects()
        );

        // BOTH watchers torn down: each project's stop_token was flipped true by
        // abort_watcher_task during reaping (the additive sibling B included).
        assert!(
            token_a.load(Ordering::Acquire),
            "A's watcher stop_token must be set after close (watcher torn down)"
        );
        assert!(
            token_b.load(Ordering::Acquire),
            "B's watcher stop_token must be set after close (additive sibling watcher torn down)"
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

    /// Feature 012 US1 — cross-project query, end-to-end on the DAEMON path.
    ///
    /// Drives the real production data path on loopback HTTP:
    ///   1. spawn a daemon, open a session bound to project A;
    ///   2. `index_folder(B, add:true)` -> B joins the session's working set
    ///      ADDITIVELY (A stays active);
    ///   3. a `search_symbols` with `projects:["*"]` returns ATTRIBUTED hits from
    ///      BOTH A and B (each project's distinctly-named symbol, under its own
    ///      `── project: <id> ──` header);
    ///   4. a `search_symbols` with NO project params returns ONLY the active
    ///      project A's symbol (the no-regression default target).
    ///
    /// This is the US1 acceptance gate at the daemon level. Honest coverage limit:
    /// it exercises the in-process front-end-absent daemon HTTP route directly
    /// (the production topology's data path); a full MCP-client dogfood against a
    /// built binary is a separate live-verify step.
    #[tokio::test]
    async fn test_cross_project_query_returns_attributed_hits_from_both_projects() {
        let _env_lock = env_lock().await;
        let daemon_home = TempDir::new().expect("daemon home");
        let _env_guard = EnvVarGuard::set("SYMFORGE_HOME", daemon_home.path());

        let project_a = project_dir("symforge-xproj-a");
        let project_b = project_dir("symforge-xproj-b");
        // Distinctly-named symbols so attribution is unambiguous. Both share the
        // substring "xproj_marker" so one query matches across both projects.
        std::fs::write(
            project_a.path().join("src").join("a.rs"),
            "pub fn xproj_marker_alpha() -> u32 { 1 }\n",
        )
        .expect("write source a");
        std::fs::write(
            project_b.path().join("src").join("b.rs"),
            "pub fn xproj_marker_beta() -> u32 { 2 }\n",
        )
        .expect("write source b");

        let handle = spawn_daemon("127.0.0.1").await.expect("spawn daemon");
        let base_url = format!("http://127.0.0.1:{}", handle.port);
        let http = authed_client(&handle);

        // (1) Open a session bound to A.
        let opened = http
            .post(format!("{base_url}/v1/sessions/open"))
            .json(&OpenProjectRequest {
                project_root: project_a.path().display().to_string(),
                client_name: "xproj".to_string(),
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
        let project_a_id = opened.project_id.clone();
        let project_b_id =
            project_key(&canonical_project_root(project_b.path()).expect("canonical b"));

        // Index A through the session so its index is warm (open does a load; a
        // reload via the active path guarantees the file is indexed).
        let index_a = http
            .post(format!(
                "{base_url}/v1/sessions/{}/tools/index_folder",
                opened.session_id
            ))
            .json(&IndexFolderInput {
                path: project_a.path().display().to_string(),
                idempotency_key: None,
                add: None,
            })
            .send()
            .await
            .expect("index A request")
            .error_for_status()
            .expect("index A status")
            .text()
            .await
            .expect("index A body");
        assert!(index_a.contains("Indexed"), "index A: {index_a}");

        // (2) Additively open B.
        let index_b = http
            .post(format!(
                "{base_url}/v1/sessions/{}/tools/index_folder",
                opened.session_id
            ))
            .json(&IndexFolderInput {
                path: project_b.path().display().to_string(),
                idempotency_key: None,
                add: Some(true),
            })
            .send()
            .await
            .expect("index B request")
            .error_for_status()
            .expect("index B status")
            .text()
            .await
            .expect("index B body");
        assert!(
            index_b.contains("added to working set"),
            "additive index B: {index_b}"
        );

        // (3) Cross-project query with projects:["*"] -> hits from BOTH projects.
        let cross = http
            .post(format!(
                "{base_url}/v1/sessions/{}/tools/search_symbols",
                opened.session_id
            ))
            .json(&serde_json::json!({
                "query": "xproj_marker",
                "projects": ["*"]
            }))
            .send()
            .await
            .expect("cross query request")
            .error_for_status()
            .expect("cross query status")
            .text()
            .await
            .expect("cross query body");
        assert!(
            cross.contains("xproj_marker_alpha"),
            "cross-project query must surface A's symbol: {cross}"
        );
        assert!(
            cross.contains("xproj_marker_beta"),
            "cross-project query must surface B's symbol: {cross}"
        );
        // Both projects attributed with their own section headers (ProjectHit ids).
        assert!(
            cross.contains(&format!("project: {project_a_id}")),
            "A must be attributed by project id: {cross}"
        );
        assert!(
            cross.contains(&format!("project: {project_b_id}")),
            "B must be attributed by project id: {cross}"
        );

        // (4) No-params query -> ONLY the active project A (no-regression default).
        let active_only = http
            .post(format!(
                "{base_url}/v1/sessions/{}/tools/search_symbols",
                opened.session_id
            ))
            .json(&serde_json::json!({
                "query": "xproj_marker"
            }))
            .send()
            .await
            .expect("active query request")
            .error_for_status()
            .expect("active query status")
            .text()
            .await
            .expect("active query body");
        assert!(
            active_only.contains("xproj_marker_alpha"),
            "active-only query must surface A's symbol: {active_only}"
        );
        assert!(
            !active_only.contains("xproj_marker_beta"),
            "active-only query must NOT surface B's symbol (single active project): {active_only}"
        );
        // Single active project renders flat — no cross-project section header.
        assert!(
            !active_only.contains("project: "),
            "single-active query must render flat (no project header): {active_only}"
        );

        // (5) B1 — cross-project scoping is now HONORED over the wire, not
        // refused (D11). These params previously returned a 400 "scoping is not
        // supported with cross-project targeting"; now they filter the results.
        //
        // 5a — a path_prefix matching NEITHER project's files scopes BOTH out
        //      (honored as a real filter, not silently ignored, not refused).
        let scoped_out = http
            .post(format!(
                "{base_url}/v1/sessions/{}/tools/search_symbols",
                opened.session_id
            ))
            .json(&serde_json::json!({
                "query": "xproj_marker",
                "projects": ["*"],
                "path_prefix": "no_such_dir"
            }))
            .send()
            .await
            .expect("scoped query request")
            .error_for_status()
            .expect("B1: path_prefix must NOT be refused (HTTP 200, not 400)")
            .text()
            .await
            .expect("scoped query body");
        assert!(
            !scoped_out.contains("xproj_marker_alpha") && !scoped_out.contains("xproj_marker_beta"),
            "B1: a non-matching path_prefix must scope BOTH projects out \
             (honored, not ignored): {scoped_out}"
        );

        // 5b — language=Python excludes both Rust symbols (honored language filter).
        let lang_python = http
            .post(format!(
                "{base_url}/v1/sessions/{}/tools/search_symbols",
                opened.session_id
            ))
            .json(&serde_json::json!({
                "query": "xproj_marker",
                "projects": ["*"],
                "language": "Python"
            }))
            .send()
            .await
            .expect("python query request")
            .error_for_status()
            .expect("B1: language must NOT be refused (HTTP 200, not 400)")
            .text()
            .await
            .expect("python query body");
        assert!(
            !lang_python.contains("xproj_marker_alpha")
                && !lang_python.contains("xproj_marker_beta"),
            "B1: language=Python must scope out the Rust symbols cross-project: {lang_python}"
        );

        // 5c — language=Rust keeps BOTH (the filter scopes; it does not break the
        // query) — proves the scope-outs above are real filtering, not breakage.
        let lang_rust = http
            .post(format!(
                "{base_url}/v1/sessions/{}/tools/search_symbols",
                opened.session_id
            ))
            .json(&serde_json::json!({
                "query": "xproj_marker",
                "projects": ["*"],
                "language": "Rust"
            }))
            .send()
            .await
            .expect("rust query request")
            .error_for_status()
            .expect("rust query status")
            .text()
            .await
            .expect("rust query body");
        assert!(
            lang_rust.contains("xproj_marker_alpha") && lang_rust.contains("xproj_marker_beta"),
            "B1: language=Rust keeps both Rust symbols cross-project: {lang_rust}"
        );

        // (6) B1 — cross-project search_text scoping is ALSO honored over the wire
        // (closes the symbols-only coverage gap from the adversarial review). The
        // source text "xproj_marker_*" lives in both projects' .rs files.
        // 6-control: an unscoped cross-project text search finds BOTH.
        let text_all = http
            .post(format!(
                "{base_url}/v1/sessions/{}/tools/search_text",
                opened.session_id
            ))
            .json(&serde_json::json!({ "query": "xproj_marker", "projects": ["*"] }))
            .send()
            .await
            .expect("text all request")
            .error_for_status()
            .expect("text all status")
            .text()
            .await
            .expect("text all body");
        assert!(
            text_all.contains("xproj_marker_alpha") && text_all.contains("xproj_marker_beta"),
            "B1 control: unscoped cross-project text search finds both projects: {text_all}"
        );
        // 6-scoped: a non-matching path_prefix scopes BOTH out — honored as a
        // filter (HTTP 200), not refused (this was HTTP 400 before B1).
        let text_scoped = http
            .post(format!(
                "{base_url}/v1/sessions/{}/tools/search_text",
                opened.session_id
            ))
            .json(&serde_json::json!({
                "query": "xproj_marker",
                "projects": ["*"],
                "path_prefix": "no_such_dir"
            }))
            .send()
            .await
            .expect("text scoped request")
            .error_for_status()
            .expect("B1: search_text path_prefix must NOT be refused (200, not 400)")
            .text()
            .await
            .expect("text scoped body");
        assert!(
            !text_scoped.contains("xproj_marker_alpha")
                && !text_scoped.contains("xproj_marker_beta"),
            "B1: non-matching path_prefix must scope BOTH projects' text out (honored): {text_scoped}"
        );

        let _ = handle.shutdown_tx.send(());
        wait_for_path_absent(&daemon_home.path().join(LEGACY_DAEMON_PORT_FILE)).await;
    }

    /// B2/D12 regression: a cross-project read REFLECTS a project's watcher-
    /// observed change (no git commit). Two projects in one session; mutate a
    /// watched file in B (add a new symbol + delete an existing one); republish B
    /// deterministically (`SharedIndexHandle::reload`, the same swap_and_publish
    /// the watcher drives, with no debounce race); re-run the SAME cross-read.
    ///
    /// Asserts:
    ///   SC-1 (add)    — the NEW symbol `xproj_marker_gamma` now appears under B.
    ///   SC-2 (delete) — the deleted `xproj_marker_beta` is GONE (no stale ghost).
    ///   control       — B's interned base_generation ADVANCED across the refresh
    ///                   (proves the force-replace fired; the FROZEN base would
    ///                   still have surfaced beta and not gamma).
    ///   SC-002        — after the force-replace there is still exactly ONE
    ///                   `Arc<IndexBase>` per BaseKey: the session entry's base
    ///                   Arc IS the bases-table value for B's key (Arc::ptr_eq),
    ///                   and the bases-table key count is unchanged.
    #[tokio::test]
    async fn test_cross_project_read_is_fresh_after_watcher_reindex() {
        let _env_lock = env_lock().await;
        let daemon_home = TempDir::new().expect("daemon home");
        let _env_guard = EnvVarGuard::set("SYMFORGE_HOME", daemon_home.path());

        let project_a = project_dir("symforge-fresh-a");
        let project_b = project_dir("symforge-fresh-b");
        let b_source = project_b.path().join("src").join("b.rs");
        std::fs::write(
            project_a.path().join("src").join("a.rs"),
            "pub fn xproj_marker_alpha() -> u32 { 1 }\n",
        )
        .expect("write source a");
        std::fs::write(&b_source, "pub fn xproj_marker_beta() -> u32 { 2 }\n")
            .expect("write source b");

        let handle = spawn_daemon("127.0.0.1").await.expect("spawn daemon");
        let base_url = format!("http://127.0.0.1:{}", handle.port);
        let http = authed_client(&handle);

        // Open A, index A, additively open B.
        let opened = http
            .post(format!("{base_url}/v1/sessions/open"))
            .json(&OpenProjectRequest {
                project_root: project_a.path().display().to_string(),
                client_name: "fresh".to_string(),
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
        let session_id = opened.session_id.clone();
        let canonical_b = canonical_project_root(project_b.path()).expect("canonical b");
        let project_b_id = project_key(&canonical_b);

        http.post(format!(
            "{base_url}/v1/sessions/{session_id}/tools/index_folder"
        ))
        .json(&IndexFolderInput {
            path: project_a.path().display().to_string(),
            idempotency_key: None,
            add: None,
        })
        .send()
        .await
        .expect("index A request")
        .error_for_status()
        .expect("index A status");

        http.post(format!(
            "{base_url}/v1/sessions/{session_id}/tools/index_folder"
        ))
        .json(&IndexFolderInput {
            path: project_b.path().display().to_string(),
            idempotency_key: None,
            add: Some(true),
        })
        .send()
        .await
        .expect("index B request")
        .error_for_status()
        .expect("index B status");

        // Baseline cross-read: B's ORIGINAL symbol present, the new one absent.
        let baseline = http
            .post(format!(
                "{base_url}/v1/sessions/{session_id}/tools/search_symbols"
            ))
            .json(&serde_json::json!({ "query": "xproj_marker", "projects": ["*"] }))
            .send()
            .await
            .expect("baseline request")
            .error_for_status()
            .expect("baseline status")
            .text()
            .await
            .expect("baseline body");
        assert!(
            baseline.contains("xproj_marker_beta"),
            "baseline cross-read must surface B's original symbol: {baseline}"
        );
        assert!(
            !baseline.contains("xproj_marker_gamma"),
            "baseline must NOT yet contain the not-written symbol: {baseline}"
        );

        // B's interned base_generation BEFORE the watcher change (control anchor).
        let b_key = BaseKey::new(canonical_b.clone(), CommitId::Dirtyless);
        let gen_before =
            state_base_generation(&handle.state, &b_key).expect("B base interned at open");
        let key_count_before = handle.state.bases.read().len();

        // A's interned base_generation BEFORE B's refresh — A is the UNTOUCHED
        // project; the mismatch-gate must NOT re-intern it. A is a non-git TempDir
        // like B, so its key is (canonical_root, Dirtyless) — same shape as B's.
        let canonical_a = canonical_project_root(project_a.path()).expect("canonical a");
        let a_key = BaseKey::new(canonical_a, CommitId::Dirtyless);
        let a_gen_before =
            state_base_generation(&handle.state, &a_key).expect("A base interned at open");

        // Mutate a WATCHED file in B: delete beta, add gamma. Then republish B
        // DETERMINISTICALLY via the project's SharedIndex reload (same publish the
        // watcher drives) so the test does not race the real watcher debounce.
        std::fs::write(&b_source, "pub fn xproj_marker_gamma() -> u32 { 3 }\n")
            .expect("rewrite source b");
        {
            let projects = handle.state.projects.read();
            let project_b_inst = projects
                .get(&project_b_id)
                .expect("project B loaded in daemon");
            project_b_inst
                .index
                .reload(&canonical_b)
                .expect("deterministic B reload (watcher-equivalent publish)");
        }

        // The SAME cross-read now reflects B's current state.
        let fresh = http
            .post(format!(
                "{base_url}/v1/sessions/{session_id}/tools/search_symbols"
            ))
            .json(&serde_json::json!({ "query": "xproj_marker", "projects": ["*"] }))
            .send()
            .await
            .expect("fresh request")
            .error_for_status()
            .expect("fresh status")
            .text()
            .await
            .expect("fresh body");
        // SC-1 (add): the new symbol appears.
        assert!(
            fresh.contains("xproj_marker_gamma"),
            "SC-1: cross-read after watcher reindex must surface B's NEW symbol: {fresh}"
        );
        // SC-2 (delete): the deleted symbol is gone (no stale ghost).
        assert!(
            !fresh.contains("xproj_marker_beta"),
            "SC-2: cross-read must NOT surface B's DELETED symbol (stale ghost): {fresh}"
        );
        // A is untouched and still present (the refresh did not drop other projects).
        assert!(
            fresh.contains("xproj_marker_alpha"),
            "A must remain present after B's refresh: {fresh}"
        );

        // Control: B's interned base_generation advanced (force-replace fired).
        let gen_after = state_base_generation(&handle.state, &b_key)
            .expect("B base still interned after refresh");
        assert!(
            gen_after > gen_before,
            "control: force-replace must advance B's base_generation \
             (before={gen_before}, after={gen_after}); a frozen base would not"
        );

        // Mismatch-gate control: A was UNTOUCHED, so its base must NOT have been
        // re-interned by the refresh (no spurious force-replace of fresh entries).
        let a_gen_after = state_base_generation(&handle.state, &a_key)
            .expect("A base still interned after refresh");
        assert_eq!(
            a_gen_after, a_gen_before,
            "mismatch-gate: untouched project A must NOT be re-interned \
             (before={a_gen_before}, after={a_gen_after})"
        );

        // SC-002: exactly ONE Arc<IndexBase> per BaseKey after the force-replace —
        // the session entry's base IS the bases-table value, and no key was added.
        {
            let bases = handle.state.bases.read();
            let table_base = bases.get(&b_key).expect("B base in table after refresh");
            let sessions = handle.state.sessions.read();
            let session = sessions.get(&session_id).expect("session present");
            let ws = session.working_set.read();
            let entry = ws.get(&project_b_id).expect("B entry in working set");
            assert!(
                Arc::ptr_eq(table_base, &entry.base),
                "SC-002: the working-set entry must share the SAME interned base Arc"
            );
            assert_eq!(
                bases.len(),
                key_count_before,
                "SC-002: force-replace must REPLACE the value, not add a duplicate key"
            );
        }

        // Lone NON-ACTIVE target (Targets::One(non_active)): the session is active
        // on A, so targeting ONLY B exercises the single-non-active cross-project
        // path the design promised to refresh (not just Targets::All). Mutate B
        // again, republish deterministically, then read with projects:[B] only.
        std::fs::write(&b_source, "pub fn xproj_marker_delta() -> u32 { 4 }\n")
            .expect("rewrite source b (delta)");
        {
            let projects = handle.state.projects.read();
            projects
                .get(&project_b_id)
                .expect("project B loaded in daemon")
                .index
                .reload(&canonical_b)
                .expect("deterministic B reload (delta)");
        }
        let lone_b = http
            .post(format!(
                "{base_url}/v1/sessions/{session_id}/tools/search_symbols"
            ))
            .json(&serde_json::json!({ "query": "xproj_marker", "projects": [project_b_id] }))
            .send()
            .await
            .expect("lone-B request")
            .error_for_status()
            .expect("lone-B status")
            .text()
            .await
            .expect("lone-B body");
        assert!(
            lone_b.contains("xproj_marker_delta"),
            "lone non-active target: refresh must fire for Targets::One(B) too: {lone_b}"
        );
        assert!(
            !lone_b.contains("xproj_marker_gamma"),
            "lone non-active target: B's superseded symbol must be gone: {lone_b}"
        );

        let _ = handle.shutdown_tx.send(());
        wait_for_path_absent(&daemon_home.path().join(LEGACY_DAEMON_PORT_FILE)).await;
    }

    /// Read the interned `base_generation` for `key`, if present. Test helper for
    /// the B2/D12 freshness control assertion.
    fn state_base_generation(state: &SharedDaemonState, key: &BaseKey) -> Option<u64> {
        state.bases.read().get(key).map(|b| b.base_generation)
    }

    // ── Feature 012 hardening — cross-project output is BOUNDED ──────────────────
    //
    // Without a cap the cross-project formatter would render every hit from every
    // targeted project (unbounded, unlike the single-project path). Prove the
    // total-hit cap truncates the rendered hits AND discloses the truncation.
    #[test]
    fn test_cross_project_symbols_output_is_capped_and_discloses_truncation() {
        use crate::live_index::search::SymbolMatchTier;
        use crate::live_index::view::{ProjectHit, ViewSymbolHit};

        // 5 synthetic hits across two projects.
        let hits: Vec<ProjectHit<ViewSymbolHit>> = (0..5)
            .map(|i| ProjectHit {
                project_id: if i < 3 { "proj-a" } else { "proj-b" }.to_string(),
                hit: ViewSymbolHit {
                    name: format!("sym_{i}"),
                    path: format!("src/f{i}.rs"),
                    kind: "fn".to_string(),
                    line: i + 1,
                    tier: SymbolMatchTier::Exact,
                },
            })
            .collect();

        // Cap below the hit count -> truncated + disclosed.
        let capped = format_cross_project_symbols(&hits, "sym", true, 2);
        assert!(
            capped.starts_with("2 of 5 matches across projects (truncated;"),
            "header must disclose shown-of-total truncation: {capped}"
        );
        assert!(
            capped.contains("query a single project for the full set"),
            "truncation must carry the corrective hint: {capped}"
        );
        // Exactly the first 2 hit lines are rendered (sym_0, sym_1); sym_2.. dropped.
        assert!(capped.contains("sym_0") && capped.contains("sym_1"));
        assert!(
            !capped.contains("sym_2") && !capped.contains("sym_4"),
            "hits beyond the cap must NOT be rendered: {capped}"
        );

        // Cap above the hit count -> no truncation language, all hits rendered.
        let full = format_cross_project_symbols(&hits, "sym", true, 100);
        assert!(
            full.starts_with("5 matches across projects\n"),
            "uncapped header must not claim truncation: {full}"
        );
        assert!(
            full.contains("sym_4"),
            "all hits rendered when under cap: {full}"
        );
        assert!(
            !full.contains("truncated"),
            "no truncation notice when under cap: {full}"
        );
    }

    // The cross-project total-hit cap derived from the caller's `limit`:
    // None -> default cap; a value clamps into [1, MAX]; zero/absurd cannot
    // disable the bound.
    #[test]
    fn test_cross_project_result_cap_clamps_limit() {
        assert_eq!(
            cross_project_result_cap(None),
            CROSS_PROJECT_DEFAULT_RESULT_CAP,
            "no limit -> default cap"
        );
        assert_eq!(
            cross_project_result_cap(Some(10)),
            10,
            "small limit honored"
        );
        assert_eq!(
            cross_project_result_cap(Some(0)),
            1,
            "zero clamps up to 1 (cap never disabled)"
        );
        assert_eq!(
            cross_project_result_cap(Some(u32::MAX)),
            CROSS_PROJECT_MAX_RESULT_CAP,
            "absurd limit clamps to the hard ceiling"
        );
    }

    // The `max_tokens` budget truncates an assembled body at a line boundary and
    // discloses the cut.
    #[test]
    fn test_cross_project_token_budget_truncates_and_discloses() {
        let body = (0..50)
            .map(|i| format!("line {i} with some content\n"))
            .collect::<String>();
        // ~26 bytes/line * 50 = ~1300 bytes; max_tokens=10 -> 40-byte budget.
        let out = apply_cross_project_token_budget(body.clone(), Some(10));
        assert!(out.len() < body.len(), "budget must shrink the body");
        assert!(
            out.contains("truncated to fit max_tokens=10"),
            "truncation must be disclosed: {out}"
        );
        assert!(
            out.ends_with('\n'),
            "truncated body ends on a line boundary"
        );

        // No budget / zero budget -> identity.
        assert_eq!(apply_cross_project_token_budget(body.clone(), None), body);
        assert_eq!(
            apply_cross_project_token_budget(body.clone(), Some(0)),
            body
        );
    }

    // ── B1 — cross-project HONORS path_prefix/language, still REFUSES the params
    // with no cross-project path ─────────────────────────────────────────────
    //
    // `path_prefix`/`language`/noise/`limit` are now threaded through the
    // engine's option-honoring search (D11 + D14), so they are NO LONGER refused.
    // `structural` (search_text) and `path`/`symbol_kind`/`direction`
    // (find_references) have no cross-project entry point and are still refused
    // (honest, not silently dropped).
    #[test]
    fn test_reject_unsupported_cross_project_scoping_per_tool() {
        use serde_json::json;

        // search_symbols: path_prefix + language are now HONORED (not refused);
        // kind always allowed.
        assert!(
            reject_unsupported_cross_project_scoping(
                "search_symbols",
                &json!({ "query": "x", "path_prefix": "src/" }),
            )
            .is_ok(),
            "path_prefix is now honored cross-project and must NOT be refused"
        );
        assert!(
            reject_unsupported_cross_project_scoping(
                "search_symbols",
                &json!({ "query": "x", "language": "Rust" }),
            )
            .is_ok(),
            "language is now honored cross-project and must NOT be refused"
        );
        assert!(
            reject_unsupported_cross_project_scoping(
                "search_symbols",
                &json!({ "query": "x", "kind": "fn" }),
            )
            .is_ok(),
            "kind IS honored cross-project and must NOT be refused"
        );

        // search_text: path_prefix/language honored; structural still refused.
        assert!(
            reject_unsupported_cross_project_scoping(
                "search_text",
                &json!({ "query": "x", "path_prefix": "src/", "language": "Rust" }),
            )
            .is_ok(),
            "search_text path_prefix/language are now honored cross-project"
        );
        let structural_err = reject_unsupported_cross_project_scoping(
            "search_text",
            &json!({ "query": "x", "structural": true }),
        )
        .expect_err("structural must be refused");
        assert!(
            structural_err
                .to_string()
                .contains("structural scoping is not supported")
                && structural_err
                    .to_string()
                    .contains("query a single project for scoped results"),
            "structural refusal must name the param + corrective hint: {structural_err}"
        );

        // find_references: path / symbol_kind / direction still refused (no
        // cross-project meaning).
        for (field, value) in [
            ("path", json!("src/db.rs")),
            ("symbol_kind", json!("fn")),
            ("direction", json!("trait")),
        ] {
            let err = reject_unsupported_cross_project_scoping(
                "find_references",
                &json!({ "name": "x", field: value }),
            )
            .unwrap_err()
            .to_string();
            assert!(
                err.contains(&format!("{field} scoping is not supported")),
                "{field} must be refused on find_references: {err}"
            );
        }

        // A blank still-refused value is treated as not-set (no false refusal).
        assert!(
            reject_unsupported_cross_project_scoping(
                "find_references",
                &json!({ "name": "x", "path": "  " }),
            )
            .is_ok(),
            "blank path value must not trip the refusal"
        );
    }

    // End-to-end: the scoping refusal fires from `execute_cross_project_read`
    // BEFORE any query runs (the working set need not even contain the target —
    // but here it does, so we prove the refusal precedes a would-be-successful
    // query). A synthetic single-entry working set over an empty base.
    #[test]
    fn test_execute_cross_project_read_rejects_scoping_before_query() {
        use crate::live_index::store::LiveIndex;
        use crate::live_index::view::{BaseKey, CommitId, IndexBase, WorkingSet};
        use serde_json::json;

        let base = Arc::new(IndexBase::new(
            BaseKey::new("/synthetic/root", CommitId::Dirtyless),
            Arc::new(LiveIndex::empty_live_index()),
            1,
        ));
        let mut ws = WorkingSet::new();
        ws.add("proj-x", base);

        // A still-unsupported param (find_references `direction`) is refused up
        // front, before any query runs. (`path_prefix`/`language` are now honored,
        // so they no longer exercise this guard — `direction` still has no
        // cross-project path.)
        let err = execute_cross_project_read(
            "find_references",
            json!({ "name": "anything", "direction": "trait" }),
            Targets::Subset(vec!["proj-x".to_string()]),
            &ws,
        )
        .expect_err("scoping param must be refused");
        assert!(
            err.to_string()
                .contains("direction scoping is not supported with cross-project targeting"),
            "cross-project read must refuse the unsupported scoping param: {err}"
        );
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
                add: None,
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
                add: None,
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

    #[tokio::test]
    async fn test_symforge_edit_apply_commits_through_daemon_proxy() {
        let _env_lock = env_lock().await;
        let daemon_home = TempDir::new().expect("daemon home");
        let _home_guard = EnvVarGuard::set("SYMFORGE_HOME", daemon_home.path());
        let _surface_guard = EnvVarGuard::set_str("SYMFORGE_SURFACE", "compact");

        let project = project_dir("symforge-edit-proxy");
        let file_path = project.path().join("src").join("lib.rs");
        std::fs::write(&file_path, "pub fn proxy_edit() -> u32 { 1 }\n").expect("write source");

        let handle = spawn_daemon("127.0.0.1").await.expect("spawn daemon");
        let base_url = format!("http://127.0.0.1:{}", handle.port);
        let http = authed_client(&handle);

        let opened = http
            .post(format!("{base_url}/v1/sessions/open"))
            .json(&OpenProjectRequest {
                project_root: project.path().display().to_string(),
                client_name: "edit-proxy".to_string(),
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
        .with_project_root(project.path().to_path_buf());
        let server = crate::protocol::SymForgeServer::new_daemon_proxy(client);

        let indexed = server
            .index_folder(Parameters(IndexFolderInput {
                path: project.path().display().to_string(),
                idempotency_key: None,
                add: None,
            }))
            .await;
        assert!(
            indexed.starts_with("Indexed "),
            "daemon proxy index_folder must succeed, got: {indexed}"
        );
        assert_eq!(
            server.index().published_state().file_count,
            0,
            "front-end proxy index is intentionally empty"
        );

        let result = server
            .symforge_edit_facade_tool(Parameters(crate::stel::StelEditRequest {
                path: "src/lib.rs".to_string(),
                symbol: Some("proxy_edit".to_string()),
                body: Some("pub fn proxy_edit() -> u32 { 2 }".to_string()),
                apply: Some(true),
                ..Default::default()
            }))
            .await
            .expect("symforge_edit dispatch");
        let serialized = serde_json::to_value(&result).expect("serialize symforge_edit result");
        let body = serialized["content"][0]["text"]
            .as_str()
            .expect("symforge_edit result text");
        assert!(
            !body.starts_with("Index not loaded."),
            "daemon-proxy apply must not inspect the empty front-end index:\n{body}"
        );

        let on_disk = std::fs::read_to_string(&file_path).expect("read edited file");
        assert!(
            on_disk.contains("{ 2 }"),
            "symforge_edit apply must commit through the daemon proxy, got:\n{on_disk}\n\nbody:\n{body}"
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
                add: None,
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
                add: None,
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
                add: None,
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
                add: None,
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
                add: None,
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
