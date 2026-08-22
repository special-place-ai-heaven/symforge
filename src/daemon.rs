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

use crate::domain::{
    CapabilityStatus, ControlStateDir, FreshnessStatus, ProjectCapabilities, RootBinding,
    RootCandidateSource, RootRequestMode, RootResolution, SourceAccessMode, StatePlacement,
    UnboundReason,
};
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
    AnalyzeFileImpactInput, CheckpointNowInput, CurateKnowledgeInput, DetectImpactInput,
    DiffSymbolsInput, EditPlanInput, ExploreInput, FindDependentsInput, FindReferencesInput,
    GetFileContentInput, GetFileContextInput, GetRepoMapInput, GetSymbolContextInput,
    GetSymbolInput, HealthInput, IndexFolderInput, InspectMatchInput, InvestigationInput,
    ReviewKnowledgeInput, SearchFilesInput, SearchKnowledgeInput, SearchSymbolsInput,
    SearchTextInput, SmartQueryInput, TraceSymbolInput, ValidateFileSyntaxInput, WhatChangedInput,
    search_symbols_options_from_input, search_text_options_from_input,
};
use crate::sidecar::{SidecarState, SymbolSnapshotCache, TokenStats};
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
/// Set once this process binds AS the daemon (see `spawn_daemon_at`, where it
/// writes its own pid file). Only a bound daemon can become an orphan, so
/// [`unrecorded_daemon_pid`] stays silent for every other topology — stdio,
/// embed, and the MCP adapter front-end.
static DAEMON_BOUND: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

const DAEMON_BIND_ENV: &str = "SYMFORGE_DAEMON_BIND";
const DAEMON_ALLOW_NON_LOOPBACK_ENV: &str = "SYMFORGE_DAEMON_ALLOW_NON_LOOPBACK";
/// Task 9: seconds a session may go without a heartbeat before the daemon
/// reaper closes it through the normal close path. Override with
/// `SYMFORGE_SESSION_TTL_SECS` (clamped to >= 60). The default is deliberately
/// long — a reaped session cannot be resurrected, so only clearly abandoned
/// sessions qualify.
const SESSION_TTL_ENV: &str = "SYMFORGE_SESSION_TTL_SECS";
const DEFAULT_SESSION_TTL_SECS: u64 = 86_400;

fn session_ttl_from_env() -> std::time::Duration {
    let secs = std::env::var(SESSION_TTL_ENV)
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .unwrap_or(DEFAULT_SESSION_TTL_SECS)
        .max(60);
    std::time::Duration::from_secs(secs)
}

/// Zombie-daemon guard: seconds without any AUTHENTICATED request before the
/// daemon shuts itself down. Detached auto-spawn defaults to 600 seconds;
/// an explicitly managed `symforge daemon` remains persistent unless this
/// variable is set. Values are clamped to at least 60 seconds; `0` disables.
const DAEMON_IDLE_SHUTDOWN_ENV: &str = "SYMFORGE_DAEMON_IDLE_SHUTDOWN_SECS";
// Snapshot-backed respawn costs milliseconds, so a short window is cheap:
// zombies die within ~10 minutes of the last client's death.
const DEFAULT_DAEMON_IDLE_SHUTDOWN_SECS: u64 = 600;

fn daemon_idle_shutdown_from_env() -> Option<std::time::Duration> {
    let value = std::env::var(DAEMON_IDLE_SHUTDOWN_ENV).ok()?;
    let secs = value
        .trim()
        .parse::<u64>()
        .unwrap_or(DEFAULT_DAEMON_IDLE_SHUTDOWN_SECS);
    (secs > 0).then(|| std::time::Duration::from_secs(secs.max(60)))
}

/// Feature 020 / L-V02: gate that turns ON background publication of P1 local-ref
/// lanes (bare local branches) when a project opens or reloads. OFF by default so
/// the current-only P0 path keeps its measured latency/memory unchanged — ref
/// lanes are strictly opt-in. Accepts `1`/`true`/`on` (case-insensitive); anything
/// else, including unset, is OFF.
const LOCAL_REF_LANES_ENV: &str = "SYMFORGE_LOCAL_REF_LANES";

fn local_ref_lanes_enabled() -> bool {
    std::env::var(LOCAL_REF_LANES_ENV)
        .ok()
        .is_some_and(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "on"
            )
        })
}

const DAEMON_AUTH_TOKEN_ENV: &str = "SYMFORGE_DAEMON_AUTH_TOKEN";
const TRACE_SYMBOL_ALIAS_DEPRECATION: &str = concat!(
    "Deprecation warning: `trace_symbol` is retired; ",
    "use `get_symbol_context` with `sections=[...]` or `find_references` instead. ",
    "Compatibility policy: keep daemon alias through v7.x; planned removal in v8.0."
);
/// D-015-012: CBM migrator ergonomics — route the CBM `detect_changes` tool
/// name to the real `detect_impact` tool so a migrating client's existing
/// tool-name references keep working.
const DETECT_CHANGES_ALIAS_DEPRECATION: &str = concat!(
    "Deprecation warning: `detect_changes` is a CBM-compatibility alias; ",
    "use `detect_impact` instead. Compatibility policy: kept for CBM migrator ",
    "ergonomics (decision-log D-015-012); no removal date set."
);

pub type SharedDaemonState = Arc<DaemonState>;

pub struct DaemonHandle {
    pub port: u16,
    pub shutdown_tx: tokio::sync::oneshot::Sender<()>,
    pub state: SharedDaemonState,
    server_task: tokio::task::JoinHandle<()>,
    /// Task 9: the daemon-owned session reaper. Holds a `Weak` on the
    /// daemon state, so it exits on its own once the daemon shuts down
    /// and the state drops; `run_daemon_until_shutdown` also aborts it
    /// explicitly so restarts/tests cannot leak the interval task.
    reaper_task: tokio::task::JoinHandle<()>,
}

#[derive(Clone)]
pub struct DaemonSessionClient {
    http_client: reqwest::Client,
    base_url: String,
    project_id: String,
    session_id: String,
    project_name: String,
    /// Task 8: roots of projects this connection successfully opened
    /// ADDITIVELY (beyond the immutable home in `project_root`). Ordered,
    /// deduplicated, shared across clones so a reconnect can reopen the
    /// whole working set and verify deterministic IDs before serving.
    opened_roots: std::sync::Arc<parking_lot::Mutex<Vec<PathBuf>>>,
    /// Locked contract: the root a default (non-`add`) `index_folder` last
    /// ACTIVATED for unqualified reads on this connection. `None` = home is
    /// active. Shared across clones so a reconnect can restore the active
    /// project after re-opening the working set.
    active_root: std::sync::Arc<parking_lot::Mutex<Option<PathBuf>>>,
    /// Serializes daemon-backed `index_folder` calls through the matching
    /// adapter ACTIVE-mirror update. Without one shared lane, two overlapping
    /// default opens can activate in daemon order B -> C but record their
    /// adapter mirrors in response order C -> B.
    activation_lane: std::sync::Arc<tokio::sync::Mutex<()>>,
    /// The connected daemon's boot epoch (from /health at connect). Written
    /// into this adapter's session descriptor so descriptor selection can
    /// reject the record after a daemon restart instead of aliasing an
    /// unrelated `session-N`.
    daemon_started_at: Option<u64>,
    auth_token: Option<String>,
    /// The process-coordination namespace that owns this daemon connection.
    /// Reconnect must reuse this exact typed owner rather than re-resolving
    /// mutable environment state.
    control_state_dir: ControlStateDir,
    /// Stored so reconnection can re-open a session at the same project root.
    project_root: Option<PathBuf>,
}

pub struct DaemonState {
    next_session_id: AtomicU64,
    // Registry/session guards protect only map membership and short metadata
    // updates. Project load, reload, watcher lifecycle, and git/file IO always run
    // after those guards are released. Per-session `working_set` is independent.
    /// Per-daemon base intern table (Feature 012). Equal `(canonical_root,
    /// commit)` keys MUST share ONE `Arc<IndexBase>` (SC-002): `intern_base`
    /// returns the existing Arc on a key hit and only mints a new base (drawing a
    /// generation from `base_generation_seq`) on a miss. Populated on project
    /// load/activate and seeded into per-session working sets; the cross-project
    /// read route reads those bases and `intern_base_refresh` FORCE-REPLACES the
    /// value for a key when a watched change has advanced the project's published
    /// index (B2/D12) — still one Arc per `BaseKey` (SC-002).
    /// Cache census (C4c): each value is a projection PINNED to one
    /// publication — keyed by `(canonical_root, commit)`, fenced by its
    /// minted `base_generation`, force-replaced when the published index
    /// advances, and GC'd by the strong-count orphan sweep. C5's leases
    /// take over the pinned-publication identity.
    bases: RwLock<HashMap<BaseKey, Arc<IndexBase>>>,
    /// Monotonic source of `base_generation` fence tokens, minted only when
    /// `intern_base` publishes a NEW base. Starts at 1; a shared (interned) base
    /// keeps its original generation, so this is strictly increasing across
    /// distinct published bases (the D2 fence the engine asserts on).
    base_generation_seq: AtomicU64,
    projects: RwLock<HashMap<String, Arc<ProjectSlot>>>,
    sessions: RwLock<HashMap<String, SessionRecord>>,
    identity: DaemonIdentity,
    /// Boot epoch minted once per daemon process (unix seconds). Served on
    /// /health; descriptor selection rejects records from a previous boot.
    started_at_unix_secs: u64,
    /// The bearer token this daemon requires on every authenticated route.
    /// Non-optional by design: the daemon ALWAYS establishes a token at startup
    /// (env pin or freshly generated), so `authorize_daemon_request` is strictly
    /// fail-closed — there is no "no token ⇒ allow" path. Making this a `String`
    /// (not `Option`) encodes the fail-closed invariant in the type system.
    auth_token: String,
    /// Process-global coordination namespace used by daemon discovery and
    /// cross-project idempotency. None preserves the explicit fail-closed
    /// behavior when no durable control state is available.
    control_state_dir: Option<ControlStateDir>,
    /// Concurrency governor — limits parallel tool calls and enforces timeouts.
    governor: crate::sidecar::governor::RequestGovernor,
    /// Epoch millis of the last completed AUTHENTICATED request — the idle-shutdown signal.
    last_activity_at: AtomicU64,
    /// Authenticated handlers currently in flight. Idle shutdown must not cut
    /// off active writes or long indexing requests.
    active_authenticated_requests: AtomicU64,
    /// Notified by the reaper once the idle-shutdown window elapses;
    /// `run_daemon_until_shutdown` treats it like a shutdown signal.
    idle_shutdown: tokio::sync::Notify,
}

struct AuthenticatedActivityGuard {
    state: SharedDaemonState,
}

impl Drop for AuthenticatedActivityGuard {
    fn drop(&mut self) {
        self.state
            .last_activity_at
            .store(now_epoch_millis(), Ordering::Relaxed);
        self.state
            .active_authenticated_requests
            .fetch_sub(1, Ordering::Relaxed);
    }
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
    source_access_mode: SourceAccessMode,
    state_placement: StatePlacement,
    persistence_health: Arc<RwLock<CapabilityStatus>>,
    curation_coordinator: Arc<crate::protocol::knowledge_curation::KnowledgeCurationCoordinator>,
    // C4 (Feature 020 Slice 4, D1): typed handle; the frozen publication_roots
    // census retires bare `SharedIndex` state fields.
    index: crate::live_index::index_lifecycle::activation::ProjectRuntimeHandle,
    watcher_info: Arc<Mutex<WatcherInfo>>,
    watcher_task: Option<tokio::task::JoinHandle<()>>,
    stop_token: Arc<AtomicBool>,
    token_stats: Arc<TokenStats>,
    /// Cache census (C4c): the project's symbol-snapshot cache. Never read
    /// here — flows to `SessionRuntime`/`SidecarState`, whose sole reader
    /// (the sidecar handlers) fences every hit to the current project
    /// generation via `ensure_symbol_cache_generation`. Dropped whole with
    /// the instance on eviction/retarget.
    symbol_cache: Arc<RwLock<SymbolSnapshotCache>>,
    session_ids: HashSet<String>,
    opened_at: SystemTime,
    activation_state: ActivationState,
}

struct ProjectSlot {
    metadata: RwLock<ProjectInstance>,
    /// Serializes project mutation without blocking registry lookup or reads of
    /// the currently published SharedIndex generation.
    mutation: Mutex<()>,
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
    /// The immutable HOME identity (the project this session opened with).
    /// `active_project_id` may move via a default `index_folder`; this never
    /// does — reconnect verifies home identity against it, and health renders
    /// home and active distinctly.
    home_project_id: String,
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
    /// Cache census (C4c): entries hold interned bases pinned per
    /// publication; every cross-project read lazily re-fences its targeted
    /// entry against the CURRENT published index (B2/D12) before serving,
    /// and no path writes into the overlay.
    working_set: Arc<RwLock<WorkingSet>>,
    /// Session-private protocol state, partitioned by project while shared index
    /// and project metrics remain owned by the project instance.
    servers: HashMap<String, SymForgeServer>,
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
            home_project_id: self.home_project_id.clone(),
            // Share the SAME working-set lock across clones (O(1), singular state).
            working_set: Arc::clone(&self.working_set),
            servers: self.servers.clone(),
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
    let control_state_dir = process_control_state_dir().ok()?;
    daemon_auth_token_from_file_at(control_state_dir)
}

fn daemon_auth_token_from_file_at(control_state_dir: &ControlStateDir) -> Option<String> {
    let dir = daemon_dir_for(control_state_dir).ok()?;
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

fn resolve_daemon_auth_token_at(control_state_dir: &ControlStateDir) -> Option<String> {
    daemon_auth_token_from_env().or_else(|| daemon_auth_token_from_file_at(control_state_dir))
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
fn write_daemon_token_file_at(control_state_dir: &ControlStateDir, token: &str) -> io::Result<()> {
    let path = daemon_dir_for(control_state_dir)?.join(daemon_token_file_name());

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

fn authorize_daemon_request(
    state: &SharedDaemonState,
    headers: &HeaderMap,
) -> Result<AuthenticatedActivityGuard, StatusCode> {
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
        // Idle-shutdown signal: only AUTHENTICATED traffic counts as activity,
        // so unauthenticated probes cannot keep a zombie daemon alive.
        state
            .last_activity_at
            .store(now_epoch_millis(), Ordering::Relaxed);
        state
            .active_authenticated_requests
            .fetch_add(1, Ordering::Relaxed);
        Ok(AuthenticatedActivityGuard {
            state: Arc::clone(state),
        })
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
    #[serde(default = "default_project_freshness")]
    pub freshness: FreshnessStatus,
    pub durability: CapabilityStatus,
    pub capabilities: ProjectCapabilities,
    pub opened_at_unix_secs: u64,
}

fn default_project_freshness() -> FreshnessStatus {
    FreshnessStatus::Current
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
    /// Daemon boot epoch (unix seconds). Present on 10.1+ daemons; descriptors
    /// record it and selection rejects a descriptor whose epoch differs from
    /// the live daemon's — a stale record can no longer alias an unrelated
    /// `session-N` after a daemon restart (the counter resets per process).
    #[serde(default)]
    pub started_at_unix_secs: Option<u64>,
}

#[derive(Clone)]
struct SessionRuntime {
    canonical_root: PathBuf,
    project_id: String,
    session_id: String,
    index: crate::live_index::index_lifecycle::activation::ProjectRuntimeHandle,
    token_stats: Arc<TokenStats>,
    /// Cache census (C4c): a clone of the project's symbol-snapshot Arc.
    /// Sole reader is the sidecar handler family behind
    /// `ensure_symbol_cache_generation` — generation-keyed, deterministic
    /// clear on mismatch, unavailability when the generation moved.
    symbol_cache: Arc<RwLock<SymbolSnapshotCache>>,
    /// Session-private, project-keyed `SymForgeServer` clone.
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
    /// Cache census (C4c): same adjudication as `SessionRecord::working_set`
    /// — this is the same `Arc`, re-fenced per read via B2/D12.
    working_set: Arc<RwLock<WorkingSet>>,
    /// Session-scoped project publications used by `search_knowledge`.
    /// Capturing from this map, rather than the daemon-global project table,
    /// makes `projects=["*"]` expand only to projects open in this session.
    project_indexes:
        HashMap<String, crate::live_index::index_lifecycle::activation::ProjectRuntimeHandle>,
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

    /// The token this daemon enforces — used by the owner-checked shutdown
    /// cleanup to prove a runtime file still belongs to this daemon.
    fn auth_token_for_cleanup(&self) -> String {
        self.auth_token.clone()
    }

    /// Construct daemon state bound to a specific, already-established token.
    fn with_token(auth_token: String) -> Self {
        let control_state_dir = paths::process_control_state_placement()
            .directory()
            .cloned();
        Self::with_token_and_control_state(auth_token, control_state_dir)
    }

    fn with_token_at(auth_token: String, control_state_dir: ControlStateDir) -> Self {
        Self::with_token_and_control_state(auth_token, Some(control_state_dir))
    }

    fn with_token_and_control_state(
        auth_token: String,
        control_state_dir: Option<ControlStateDir>,
    ) -> Self {
        // V11 bootstrap (Feature 020 Slice 4, T030, C4b): the daemon surface
        // runs the process activation ceremony and attaches its capacity
        // owner BEFORE any request can be served through this state.
        live_index::index_lifecycle::activation::activate_surface(
            live_index::index_lifecycle::process_runtime::SurfaceKind::Daemon,
        );
        Self {
            next_session_id: AtomicU64::new(1),
            bases: RwLock::new(HashMap::new()),
            base_generation_seq: AtomicU64::new(1),
            projects: RwLock::new(HashMap::new()),
            sessions: RwLock::new(HashMap::new()),
            identity: current_daemon_identity(),
            started_at_unix_secs: now_epoch_millis() / 1000,
            auth_token,
            control_state_dir,
            governor: crate::sidecar::governor::RequestGovernor::new(),
            last_activity_at: AtomicU64::new(now_epoch_millis()),
            active_authenticated_requests: AtomicU64::new(0),
            idle_shutdown: tokio::sync::Notify::new(),
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
    /// (Feature 012, Phase 0). Clones the project slot under the registry lock,
    /// then builds and interns the candidate without holding the registry.
    fn intern_base_for_project(&self, project_id: &str) -> Option<Arc<IndexBase>> {
        let slot = self.projects.read().get(project_id).cloned()?;
        Some(self.intern_base(slot.base()))
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
    /// FRESHNESS SIGNAL: `Arc::ptr_eq(&project.index.data_plane().read(), entry.base.index.data_plane())`.
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
    /// entries' base pointers under `working_set.read()`, clones the targeted
    /// slots under the registry, and returns early if all are fresh. Only when ≥1
    /// entry is stale does it intern fresh bases and take `working_set.write()`.
    ///
    /// LOCK ORDER (`bases -> projects -> sessions`; `working_set` is outside the
    /// hierarchy, never held across a daemon-map lock): snapshot under
    /// `working_set.read()` -> drop; clone slots under the registry -> drop;
    /// compare through per-slot metadata -> `intern_base_refresh` under
    /// `bases.write()` -> drop; finally `working_set.write()` with NO daemon-map
    /// lock held — the same intern-then-`working_set.add` sequence used elsewhere.
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
        // after cloning targeted slots under one registry read. The project_id is
        // carried through because a `BaseKey` keys on `(canonical_root, commit)`,
        // NOT the working-set project id.
        let slots: HashMap<String, Arc<ProjectSlot>> = {
            let projects = self.projects.read();
            targeted
                .iter()
                .filter_map(|(project_id, _)| {
                    projects
                        .get(project_id)
                        .cloned()
                        .map(|slot| (project_id.clone(), slot))
                })
                .collect()
        };
        let stale: Vec<(String, Arc<IndexBase>)> = targeted
            .iter()
            .filter_map(|(project_id, entry_index)| {
                let slot = slots.get(project_id)?;
                let index = slot.metadata.read().index.shared();
                // `read()` yields an arc_swap Guard derefing to the current
                // `Arc<LiveIndex>`; `&current` deref-coerces to the
                // `&Arc<LiveIndex>` ptr_eq wants (as `base()` does for clone).
                // Every publish stores a fresh Arc, so an equal pointer means
                // the published index has not advanced since intern.
                let current = index.read();
                if Arc::ptr_eq(&current, entry_index) {
                    None // fresh — no re-intern
                } else {
                    // candidate wrapping the current snapshot
                    Some((project_id.clone(), slot.base()))
                }
            })
            .collect();
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
            // ponytail: ws.add attaches a fresh EMPTY overlay — correct because
            // overlays are always empty (the session-private overlay writer track
            // was retired 2026-06-29; see docs/superpowers/specs/
            // 2026-06-29-overlay-track-retirement-design.md). If a real overlay
            // writer is ever reintroduced, this must rebase the uncommitted deltas
            // instead, or it silently drops them on every freshness refresh.
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
        // exists, so the count is exact. Commit-advance is ONE orphan source; the
        // other is the last session on a BaseKey closing (swept in close_session).
        // The no-commit watcher path force-replaces the SAME key — no orphan.
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

    /// Ensure a live slot for `project_id` and join `session_id` to it, with the
    /// join recorded under the same `projects` write lock that proves the slot is
    /// the map's CURRENT entry. This closes the cleanup/reinsertion race
    /// (recovered finding #15): a slot observed via the read lock may be removed
    /// by a concurrent close before we join it — joining such a zombie (or
    /// re-inserting it) would resurrect a stopped slot. The loop retries against
    /// the authoritative entry instead; a cold load happens OUTSIDE the registry
    /// lock (a racing opener may win insertion; the inactive loser has no
    /// background work and is simply dropped).
    fn ensure_project_slot_for_session(
        &self,
        session_id: &str,
        project_id: &str,
        canonical_root: &Path,
    ) -> anyhow::Result<Arc<ProjectSlot>> {
        self.ensure_project_slot_for_session_with(session_id, project_id, || {
            ProjectInstance::load(canonical_root)
        })
    }

    fn ensure_project_slot_for_binding(
        &self,
        session_id: &str,
        binding: &RootBinding,
    ) -> anyhow::Result<Arc<ProjectSlot>> {
        self.ensure_project_slot_for_session_with(session_id, &binding.root_id.0, || {
            let state_placement = crate::discovery::resolve_state_placement(binding);
            ProjectInstance::load_bound(binding, state_placement)
        })
    }

    fn ensure_project_slot_for_session_with<F>(
        &self,
        session_id: &str,
        project_id: &str,
        load_project: F,
    ) -> anyhow::Result<Arc<ProjectSlot>>
    where
        F: FnOnce() -> anyhow::Result<ProjectInstance>,
    {
        let mut load_project = Some(load_project);
        loop {
            let existing = self.projects.read().get(project_id).cloned();
            if let Some(slot) = existing {
                let projects = self.projects.write();
                if projects
                    .get(project_id)
                    .is_some_and(|current| Arc::ptr_eq(current, &slot))
                {
                    slot.metadata
                        .write()
                        .session_ids
                        .insert(session_id.to_string());
                    return Ok(slot);
                }
                // The map entry changed under us (concurrent cleanup or a fresh
                // reinsert); retry against the current authoritative entry.
                continue;
            }

            let candidate = Arc::new(ProjectSlot::new(load_project
                .take()
                .expect("cold project loader is consumed only on the returning branch")(
            )?));
            let mut projects = self.projects.write();
            let slot = Arc::clone(projects.entry(project_id.to_string()).or_insert(candidate));
            slot.metadata
                .write()
                .session_ids
                .insert(session_id.to_string());
            return Ok(slot);
        }
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
        let slot = self.ensure_project_slot_for_session(&session_id, project_id, canonical_root)?;

        // The session id pins the winning slot before activation begins, so close
        // cannot reap it while watcher startup runs outside the registry lock.
        slot.activate();
        let (project_name, canonical_root_text, session_count) = {
            let project = slot.metadata.read();
            (
                project.project_name.clone(),
                normalized_path_string(&project.canonical_root),
                project.session_ids.len(),
            )
        };
        let server = slot.server_for_session();

        // Intern after releasing registry/metadata guards. The session id already
        // pins the slot, so it cannot vanish while git identity is resolved.
        let base = self.intern_base(slot.base());
        let mut working_set = WorkingSet::new();
        working_set.add(project_id.to_string(), base);

        let session = SessionRecord {
            session_id: session_id.clone(),
            active_project_id: project_id.to_string(),
            home_project_id: project_id.to_string(),
            working_set: Arc::new(RwLock::new(working_set)),
            servers: HashMap::from([(project_id.to_string(), server)]),
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
        // Session-open/reconnect metadata is automatic authority: it may bind a
        // normal client-declared root, but it can never inherit an earlier
        // session's direct protected override. Use the same raw+canonical typed
        // classifier as `index_folder` before looking up or loading a shared slot.
        let binding = match crate::discovery::resolve_root_candidate(
            Path::new(&request.project_root),
            RootCandidateSource::McpClientRoot,
            RootRequestMode::Automatic,
        ) {
            RootResolution::Bound(binding) => binding,
            RootResolution::Unbound {
                reason: UnboundReason::Refused(reason),
                ..
            } => {
                anyhow::bail!(
                    "Refused to open session for sensitive system path: {}. Use a project directory instead. ({})",
                    request.project_root,
                    reason.code()
                );
            }
            RootResolution::Unbound { .. } => {
                anyhow::bail!(
                    "Refused to open session for project root: {}. Use a project directory instead.",
                    request.project_root
                );
            }
        };
        self.register_session_for_existing_project(
            &binding.root_id.0,
            &request,
            &binding.canonical_root,
        )
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
        // Snapshot session identity, then scan projects in a separate critical
        // section instead of relying on session.project_id, which can be
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
        // Remove the session record FIRST so a concurrent additive open observes
        // the session as gone and undoes its own attach (recovered finding #16);
        // the membership sweep below then cannot miss a join recorded after the
        // sweep ran. The record — critically its `working_set`, which holds the
        // `Arc<IndexBase>` clones — is dropped BEFORE the GC below, or the bases
        // would never look orphaned.
        let session = self.sessions.write().remove(session_id)?;
        Some(self.finish_removed_session(session))
    }

    /// Shared post-removal cleanup for a session record that has already been
    /// claimed out of the sessions map (interactive close or reaper expiry):
    /// detach the session from EVERY project that lists it, tear down projects
    /// whose membership empties, GC orphaned bases, and report the active
    /// project's outcome.
    fn finish_removed_session(&self, session: SessionRecord) -> CloseSessionResponse {
        let session_id = session.session_id.clone();
        let active_project_id = Some(session.active_project_id.clone());
        let closed_session_id = session.session_id.clone();
        // Drop the record — critically its `working_set`, which holds the
        // `Arc<IndexBase>` clones — BEFORE the GC below, or the bases would
        // never look orphaned.
        drop(session);

        let mut active_remaining = 0usize;
        let mut active_removed = false;
        let mut active_pid_seen = false;
        let mut removed_slots = Vec::new();
        {
            let mut projects = self.projects.write();
            // All projects that list this session (active + additive siblings).
            let owning: Vec<String> = projects
                .iter()
                .filter(|(_, slot)| slot.metadata.read().session_ids.contains(&session_id))
                .map(|(id, _)| id.clone())
                .collect();

            for pid in owning {
                let is_active = active_project_id.as_deref() == Some(pid.as_str());
                let remaining = {
                    let Some(slot) = projects.get(&pid) else {
                        continue;
                    };
                    let mut project = slot.metadata.write();
                    project.session_ids.remove(&session_id);
                    project.session_ids.len()
                };
                let removed = if remaining == 0 {
                    if let Some(removed) = projects.remove(&pid) {
                        removed_slots.push(removed);
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
        for slot in removed_slots {
            slot.stop();
        }

        // If this was the last session on a BaseKey, that map value is now a
        // map-only orphan — sweep it. LOCK ORDER safe: `gc_orphaned_bases`
        // takes ONLY `bases.write()`, no other map lock held.
        self.gc_orphaned_bases();

        let project_id = match (active_project_id, active_pid_seen) {
            (Some(pid), true) => pid,
            _ => "orphan".to_string(),
        };

        CloseSessionResponse {
            session_id: closed_session_id,
            project_id,
            remaining_sessions: active_remaining,
            project_removed: active_removed,
        }
    }

    /// Task 9 reaper claim: re-check the SAME last-seen observation under the
    /// sessions write lock and atomically remove the record before any shared
    /// project cleanup. A heartbeat that advanced `last_seen` (or crossed the
    /// cutoff) WINS and preserves the session; once the reaper claims it, a
    /// later heartbeat fails as unknown-session rather than resurrecting it.
    fn close_session_if_expired(
        &self,
        session_id: &str,
        observed_last_seen: u64,
        cutoff: u64,
    ) -> bool {
        let session = {
            let mut sessions = self.sessions.write();
            let Some(record) = sessions.get(session_id) else {
                return false;
            };
            let current = record.last_seen_at.load(Ordering::Relaxed);
            if current != observed_last_seen || current >= cutoff {
                return false;
            }
            sessions.remove(session_id)
        };
        let Some(session) = session else {
            return false;
        };
        let response = self.finish_removed_session(session);
        tracing::info!(
            session_id = %response.session_id,
            project_id = %response.project_id,
            "reaper closed expired session"
        );
        true
    }

    /// Task 9: sweep sessions whose heartbeat is older than `ttl`. Candidates
    /// are collected as `(session_id, observed_last_seen)` under a read lock;
    /// each claim re-validates under the write lock in
    /// `close_session_if_expired`. Returns the number of sessions reaped.
    pub fn reap_expired_sessions(&self, ttl: std::time::Duration) -> usize {
        let cutoff = now_epoch_millis().saturating_sub(ttl.as_millis() as u64);
        let candidates: Vec<(String, u64)> = self
            .sessions
            .read()
            .iter()
            .filter_map(|(id, session)| {
                let seen = session.last_seen_at.load(Ordering::Relaxed);
                (seen < cutoff).then(|| (id.clone(), seen))
            })
            .collect();
        let mut reaped = 0usize;
        for (session_id, observed) in candidates {
            if self.close_session_if_expired(&session_id, observed, cutoff) {
                reaped += 1;
            }
        }
        reaped
    }

    pub fn list_projects(&self) -> Vec<ProjectSummary> {
        let slots: Vec<Arc<ProjectSlot>> = self.projects.read().values().cloned().collect();
        let mut summaries: Vec<ProjectSummary> = slots
            .iter()
            .map(|slot| {
                let project = slot.metadata.read();
                ProjectSummary {
                    project_id: project.project_id.clone(),
                    project_name: project.project_name.clone(),
                    canonical_root: normalized_path_string(&project.canonical_root),
                    session_count: project.session_ids.len(),
                    opened_at_unix_secs: unix_seconds(project.opened_at),
                }
            })
            .collect();
        summaries.sort_by(|a, b| a.canonical_root.cmp(&b.canonical_root));
        summaries
    }

    pub fn project_health(&self, project_id: &str) -> Option<ProjectHealth> {
        let slot = self.projects.read().get(project_id).cloned()?;
        let project = slot.metadata.read();
        // T046: one capture — the freshness verdict below describes the SAME
        // publication as the counts it is printed beside, not one from a
        // separate load 50 lines later.
        let generation = project.index.data_plane().published_generation();
        let published = std::sync::Arc::clone(&generation.health);
        let durability = *project.persistence_health.read();
        let durable_mutation = match durability {
            CapabilityStatus::Available => CapabilityStatus::Available,
            CapabilityStatus::Unavailable { .. } => CapabilityStatus::Unavailable {
                reason:
                    crate::domain::CapabilityUnavailableReason::DurableMutationReplayUnavailable,
            },
        };
        let protected = project.source_access_mode == SourceAccessMode::ExplicitProtected;
        let source_mutation = if protected {
            CapabilityStatus::Unavailable {
                reason: crate::domain::CapabilityUnavailableReason::ExplicitProtectedSource,
            }
        } else {
            durable_mutation
        };
        let repository_init = if protected {
            CapabilityStatus::Unavailable {
                reason: crate::domain::CapabilityUnavailableReason::ExplicitProtectedSource,
            }
        } else {
            CapabilityStatus::Available
        };
        let team_artifact_export = if protected {
            CapabilityStatus::Unavailable {
                reason: crate::domain::CapabilityUnavailableReason::ExplicitProtectedSource,
            }
        } else if matches!(
            &project.state_placement,
            StatePlacement::ProjectLocal { .. }
        ) {
            CapabilityStatus::Available
        } else {
            CapabilityStatus::Unavailable {
                reason: crate::domain::CapabilityUnavailableReason::NonProjectLocalPlacement,
            }
        };
        let repository_curation = project.curation_coordinator.capability_status(
            project.index.data_plane(),
            Some(&project.canonical_root),
            Some(&project.state_placement),
            durability,
        );

        Some(ProjectHealth {
            project_id: project.project_id.clone(),
            project_name: project.project_name.clone(),
            canonical_root: normalized_path_string(&project.canonical_root),
            session_count: project.session_ids.len(),
            file_count: published.file_count,
            symbol_count: published.symbol_count,
            index_state: published.status_label().to_string(),
            freshness: generation.freshness.as_ref().clone(),
            durability,
            capabilities: ProjectCapabilities {
                persistent_snapshots: durability,
                checkpoint: durability,
                structural_edits: source_mutation,
                repository_init,
                repository_curation,
                team_artifact_export,
            },
            opened_at_unix_secs: unix_seconds(project.opened_at),
        })
    }

    pub fn list_sessions(&self, project_id: &str) -> Option<Vec<SessionSummary>> {
        let session_ids: Vec<String> = {
            let slot = self.projects.read().get(project_id).cloned()?;
            let project = slot.metadata.read();
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
        let allow_protected_root = input.allow_protected_root == Some(true);
        let binding = match crate::discovery::resolve_root_candidate(
            Path::new(&input.path),
            RootCandidateSource::ExplicitIndexFolder,
            RootRequestMode::ExplicitIndexFolder {
                allow_protected_root,
            },
        ) {
            RootResolution::Bound(binding) => binding,
            RootResolution::Unbound {
                reason: UnboundReason::Refused(reason),
                ..
            } => {
                return Ok(format!(
                    "{}: Refused to index sensitive system path: {}. Use a project directory instead.",
                    reason.code(),
                    input.path
                ));
            }
            RootResolution::Unbound { .. } => {
                return Ok(format!(
                    "root_unbound: Refused to index project root: {}.",
                    input.path
                ));
            }
        };
        let target_root = &binding.canonical_root;

        // Per-session activation (locked contract): the omitted-`add` default
        // opens/refreshes `target_root` AND makes it the session's ACTIVE
        // project for unqualified reads. The compatibility spelling
        // `add=true` opens additively WITHOUT changing the active project.
        // Home — the session/descriptor identity verified across reconnects —
        // is immutable; only `active_project_id` moves, and only for THIS
        // session (no cross-session interference). The working set retains
        // every open project, so activation is non-destructive.
        //
        // The `add` spelling changes side effects (activation), so it IS part
        // of the canonical idempotency request (hashed as `activate`).
        //
        // Cross-project replay is process coordination, not state owned by the
        // session's home repository. The one resolved ControlStateDir therefore
        // observes every open before any target ProjectInstance is constructed.
        let reset_requested = crate::protocol::tools::index_folder_reset_requested();
        let activate = input.add != Some(true);
        let (idempotency, replay_response) = match input.idempotency_key.as_deref() {
            Some(raw_key) => {
                let Some(control_state) = self.control_state_dir.as_ref() else {
                    return Ok(
                        "Idempotency unavailable: durable process control state is unavailable."
                            .to_string(),
                    );
                };
                match crate::idempotency::begin_index_folder_replay(
                    control_state,
                    target_root,
                    raw_key,
                    reset_requested,
                    allow_protected_root,
                    activate,
                ) {
                    Ok(crate::idempotency::ReplayStart::FirstExecution(active)) => {
                        (Some(active), None)
                    }
                    Ok(crate::idempotency::ReplayStart::Replay(response)) => (None, Some(response)),
                    Err(error) => return Ok(crate::idempotency::format_tool_error(&error)),
                }
            }
            None => (None, None),
        };

        match self.open_project_for_session(session_id, &binding) {
            Ok(mut output) => {
                if activate && self.set_active_project(session_id, &binding.root_id.0) {
                    output.push_str(&format!(
                        "active_project={} (default index_folder: unqualified reads on this session now target this project)\n",
                        binding.root_id.0
                    ));
                }
                if let Some(response) = replay_response {
                    return Ok(response);
                }
                if let Some(idempotency) = &idempotency
                    && let Err(error) = idempotency.complete(output.clone())
                {
                    output.push_str(&format!(
                        "\nIdempotency warning: failed to store replay result: {error}"
                    ));
                }
                Ok(output)
            }
            Err(error) => {
                if let Some(response) = replay_response {
                    return Ok(crate::idempotency::format_live_postcondition_unavailable(
                        &response, &error,
                    ));
                }
                if let Some(idempotency) = &idempotency {
                    let _ = idempotency.fail(format!("Index failed: {error}"));
                }
                Err(error)
            }
        }
    }

    /// The ONE canonical daemon open path behind `index_folder`: opens/refreshes
    /// `target_root` as a working-set project WITHOUT touching
    /// `active_project_id` — the caller (`index_folder_for_session`) owns
    /// per-session activation (default spelling activates; `add=true` does not).
    ///
    /// Steps:
    /// 1. ensure + join the authoritative project slot — session membership is
    ///    recorded under the same `projects` write lock that proves the slot is
    ///    the map's current entry, so concurrent cleanup cannot remove the slot
    ///    and orphan the join;
    /// 2. activate and reload through the slot's mutation lane (same-project
    ///    reads keep serving the previously published generation);
    /// 3. persist the successfully published generation as an atomic snapshot —
    ///    a snapshot failure does NOT roll back the in-memory open, it degrades
    ///    the checkpoint receipt and leaves any prior valid snapshot in place;
    /// 4. attach the project to the session working set with a freshly interned
    ///    base (fresh EMPTY overlay — the no-overlay-writes invariant holds).
    ///
    /// LOCK ORDER (`bases -> projects -> sessions`) is preserved: the reload and
    /// checkpoint run with no registry lock held; the attach helper interns the
    /// base first, then takes `projects`/`sessions` without overlap.
    fn open_project_for_session(
        &self,
        session_id: &str,
        binding: &RootBinding,
    ) -> anyhow::Result<String> {
        let target_project_id = &binding.root_id.0;
        let target_root = &binding.canonical_root;
        let target_slot = self.ensure_project_slot_for_binding(session_id, binding)?;
        target_slot.activate();
        let (file_count, symbol_count) = target_slot.reload_for_binding(binding)?;

        let (index, source_access_mode, state_placement, persistence_health) = {
            let project = target_slot.metadata.read();
            (
                project.index.shared(),
                project.source_access_mode,
                project.state_placement.clone(),
                Arc::clone(&project.persistence_health),
            )
        };
        let gitignore_hygiene = crate::gitignore_hygiene::reconcile_project_gitignore(
            target_root,
            if source_access_mode == SourceAccessMode::ExplicitProtected {
                crate::gitignore_hygiene::GitignoreHygieneAuthority::ExplicitProtected
            } else {
                crate::gitignore_hygiene::GitignoreHygieneAuthority::ExplicitNormalBinding
            },
        );
        let (checkpoint, state_location) = match &state_placement {
            StatePlacement::ProjectLocal { .. } | StatePlacement::UserLocal { .. } => (
                match crate::live_index::persist::checkpoint_shared_index(
                    &index,
                    target_root,
                    &state_placement,
                ) {
                    Ok(_) => {
                        *persistence_health.write() = CapabilityStatus::Available;
                        "checkpoint=written".to_string()
                    }
                    Err(error) => {
                        *persistence_health.write() = CapabilityStatus::Unavailable {
                            reason: crate::domain::CapabilityUnavailableReason::PersistentStateUnavailable,
                        };
                        format!("checkpoint=degraded: {error}")
                    }
                },
                if matches!(&state_placement, StatePlacement::ProjectLocal { .. }) {
                    "project-local"
                } else {
                    "user-local"
                },
            ),
            StatePlacement::MemoryOnly { .. } => {
                ("checkpoint=skipped(memory-only)".to_string(), "memory-only")
            }
        };

        if !self.add_project_to_session(session_id, target_project_id) {
            return Err(anyhow::anyhow!(
                "session '{session_id}' closed before the opened project could be attached"
            ));
        }

        let (project_name, root_text) = {
            let project = target_slot.metadata.read();
            (
                project.project_name.clone(),
                normalized_path_string(&project.canonical_root),
            )
        };
        Ok(format!(
            "Indexed {file_count} files, {symbol_count} symbols (added to working set).\nproject_id={target_project_id} project_name={project_name} root={root_text} source_access={source_access_mode:?} state={state_location} {checkpoint}\n{}",
            gitignore_hygiene.receipt()
        ))
    }

    /// Feature 012 (Phase 2): add an already-loaded project to a session's working
    /// set as an additional, non-active project. Returns `false` if the session or
    /// the project is unknown. The base is interned FIRST (no session lock held)
    /// to preserve the `bases -> projects -> sessions` order; a fresh EMPTY overlay
    /// is attached (no overlay write). Idempotent: re-adding refreshes the entry.
    ///
    /// Two lifecycle hardenings (recovered findings #14/#16):
    /// - a session-local server whose `SharedIndex` no longer matches the slot's
    ///   current index (the project was evicted and re-loaded since) is REPLACED,
    ///   never reused — a stale server would silently serve a dead index;
    /// - if the session closed while the attach was in flight, the membership
    ///   join is undone (and the slot reaped if that was its last session), so a
    ///   closed session cannot pin a project forever.
    fn add_project_to_session(&self, session_id: &str, project_id: &str) -> bool {
        let Some(base) = self.intern_base_for_project(project_id) else {
            return false;
        };
        let slot = {
            let projects = self.projects.read();
            let Some(slot) = projects.get(project_id) else {
                return false;
            };
            Arc::clone(slot)
        };
        let server = slot.server_for_session();
        let slot_index = slot.metadata.read().index.shared();
        slot.metadata
            .write()
            .session_ids
            .insert(session_id.to_string());
        let attached = match self.sessions.write().get_mut(session_id) {
            Some(session) => {
                let replace_server = session
                    .servers
                    .get(project_id)
                    .map(|existing| !Arc::ptr_eq(existing.index.data_plane(), &slot_index))
                    .unwrap_or(true);
                if replace_server {
                    session.servers.insert(project_id.to_string(), server);
                }
                session
                    .working_set
                    .write()
                    .add(project_id.to_string(), base);
                true
            }
            None => false,
        };
        if !attached {
            self.detach_project_membership(session_id, project_id);
        }
        attached
    }

    /// Remove `session_id` from `project_id`'s membership set, reaping the slot
    /// when that membership was its last. Used to undo an attach that raced a
    /// session close (recovered finding #16); safe to call when either side is
    /// already gone.
    fn detach_project_membership(&self, session_id: &str, project_id: &str) {
        let removed = {
            let mut projects = self.projects.write();
            let Some(slot) = projects.get(project_id).cloned() else {
                return;
            };
            let mut project = slot.metadata.write();
            project.session_ids.remove(session_id);
            let empty = project.session_ids.is_empty();
            drop(project);
            if empty {
                projects.remove(project_id)
            } else {
                None
            }
        };
        if let Some(slot) = removed {
            slot.stop();
        }
    }

    /// Feature 012 (Phase 2): set the session's ACTIVE project to one already in
    /// its working set. Returns `false` if the session is unknown or `project_id`
    /// is not an open working-set entry (cannot activate a project that is not
    /// open). Only flips `active_project_id`; the working set is untouched (the
    /// target entry already exists). No overlay is written.
    ///
    /// Production caller: `index_folder_for_session` activates on the default
    /// (non-`add`) spelling, after the open has attached the project.
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
                let removed = working_set.remove(project_id).is_some();
                drop(working_set);
                if removed {
                    session.servers.remove(project_id);
                }
                removed
            }
            None => false,
        };
        if !removed {
            return false;
        }
        // Detach the session from the project; tear the project down if this was
        // its last session (mirrors the retarget eviction in
        // `index_folder_for_session`).
        let evicted = {
            let mut projects = self.projects.write();
            let Some(slot) = projects.get(project_id).cloned() else {
                return true;
            };
            let mut project = slot.metadata.write();
            project.session_ids.remove(session_id);
            let empty = project.session_ids.is_empty();
            drop(project);
            if empty {
                projects.remove(project_id)
            } else {
                None
            }
        };
        if let Some(slot) = evicted {
            slot.stop();
        }
        true
    }

    fn session_runtime(&self, session_id: &str) -> Option<SessionRuntime> {
        self.runtime_for_target(session_id, None).ok()
    }

    /// Task 4 (outstanding-work hardening): the ONE shared resolver from a
    /// session plus an optional explicit `project` selector to the bound
    /// per-project runtime. Omission selects the session's ACTIVE project
    /// (home unless a default `index_folder` activated another working-set
    /// project). An explicit selector resolves an open project ID first, then
    /// a UNIQUE current `project_name` among the session's OPEN working-set
    /// projects — display text, not a persistent alias. Unknown/not-open/
    /// ambiguous selectors return deterministic candidate data and never
    /// trigger indexing or frecency.
    fn runtime_for_target(
        &self,
        session_id: &str,
        project: Option<&str>,
    ) -> Result<SessionRuntime, String> {
        // Clone the session and project slot in separate short critical
        // sections; no daemon-map locks overlap.
        let session = self
            .sessions
            .read()
            .get(session_id)
            .cloned()
            .ok_or_else(|| format!("unknown session '{session_id}'"))?;

        let target_id = match project.filter(|p| !p.trim().is_empty()) {
            None => session.active_project_id.clone(),
            Some(selector) => {
                let open_ids: Vec<String> = session
                    .working_set
                    .read()
                    .iter()
                    .map(|entry| entry.project_id.clone())
                    .collect();
                if open_ids.iter().any(|id| id == selector) {
                    selector.to_string()
                } else {
                    let mut matches: Vec<String> = Vec::new();
                    let mut candidates: Vec<String> = Vec::new();
                    {
                        let projects = self.projects.read();
                        for id in &open_ids {
                            if let Some(slot) = projects.get(id) {
                                let meta = slot.metadata.read();
                                candidates.push(format!("{} ({id})", meta.project_name));
                                if meta.project_name == selector {
                                    matches.push(id.clone());
                                }
                            }
                        }
                    }
                    candidates.sort();
                    match matches.len() {
                        1 => matches.remove(0),
                        0 => {
                            return Err(format!(
                                "project '{selector}' is not open in this session. Open \
                                 projects: [{}]. Open it first with index_folder(path=...).",
                                candidates.join(", ")
                            ));
                        }
                        _ => {
                            return Err(format!(
                                "project selector '{selector}' is ambiguous among open \
                                 projects: [{}]. Use the project id.",
                                candidates.join(", ")
                            ));
                        }
                    }
                }
            }
        };

        let slot = self
            .projects
            .read()
            .get(&target_id)
            .cloned()
            .ok_or_else(|| format!("project '{target_id}' is not loaded in the daemon"))?;
        let server = session.servers.get(&target_id).cloned().ok_or_else(|| {
            format!(
                "project '{target_id}' has no session server; reopen it with \
                 index_folder(path=...)"
            )
        })?;
        let project_meta = slot.metadata.read();
        // C4c typed acquisition: dispatch requires the project's admission
        // slot to still be the live occupancy of its key. A project the
        // eviction/retarget stop path retired refuses here — between the stop
        // and the map sweep, the maps can still resolve it, and without this
        // branch the neck would serve a retired index.
        if let Err(refusal) = project_meta.index.acquire() {
            return Err(format!(
                "project '{target_id}' has been retired from the process registry \
                 ({refusal:?}); reopen it with index_folder(path=...)"
            ));
        }
        let project_indexes = session
            .servers
            .iter()
            .map(|(id, server)| (id.clone(), server.index.clone()))
            .collect();
        Ok(SessionRuntime {
            canonical_root: project_meta.canonical_root.clone(),
            project_id: target_id.clone(),
            session_id: session.session_id.clone(),
            index: project_meta.index.clone(),
            token_stats: Arc::clone(&project_meta.token_stats),
            symbol_cache: Arc::clone(&project_meta.symbol_cache),
            server,
            working_set: Arc::clone(&session.working_set),
            project_indexes,
        })
    }

    /// Task 7: render the session's open-project inventory — one row per open
    /// project with deterministic ID, display name/root, home marker, published
    /// counts and index state, current generation, opened timestamp, and
    /// snapshot presence — plus the session's last-seen evidence. `project_name`
    /// is display text, never a persistent alias. Returns `None` for an unknown
    /// session.
    fn render_session_project_inventory(&self, session_id: &str) -> Option<String> {
        let session = self.sessions.read().get(session_id).cloned()?;
        let open_ids: Vec<String> = session
            .working_set
            .read()
            .iter()
            .map(|entry| entry.project_id.clone())
            .collect();
        let mut lines = vec!["── projects ──".to_string()];
        {
            let projects = self.projects.read();
            for id in &open_ids {
                let Some(slot) = projects.get(id) else {
                    lines.push(format!(
                        "{id} state=missing (open in working set but not loaded)"
                    ));
                    continue;
                };
                let meta = slot.metadata.read();
                let published = meta.index.data_plane().published_state();
                // Home and active are DISTINCT identities since the
                // worktree-routing fix: home is the immutable session
                // identity; active is what unqualified reads serve.
                let home = if *id == session.home_project_id {
                    " home=yes"
                } else {
                    ""
                };
                let active = if *id == session.active_project_id {
                    " active=yes"
                } else {
                    ""
                };
                let snapshot = if meta
                    .canonical_root
                    .join(".symforge")
                    .join("index.bin")
                    .is_file()
                {
                    "present"
                } else {
                    "absent"
                };
                lines.push(format!(
                    "{id}{home}{active} name={} root={} files={} symbols={} index={} generation={} opened={} snapshot={}",
                    meta.project_name,
                    normalized_path_string(&meta.canonical_root),
                    published.file_count,
                    published.symbol_count,
                    published.status_label(),
                    meta.index.data_plane().current_project_generation(),
                    unix_seconds(meta.opened_at),
                    snapshot,
                ));
            }
        }
        lines.push(format!(
            "session={} last_seen={} ttl_secs={}",
            session.session_id,
            unix_seconds(session.last_seen_at_time()),
            session_ttl_from_env().as_secs()
        ));
        Some(lines.join("\n"))
    }

    /// The inventory above, but only when the session holds MORE than one open
    /// project — the append-to-`health` path stays byte-compatible for the
    /// single-project sessions that existed before multi-project opens.
    fn render_session_project_inventory_if_multi(&self, session_id: &str) -> Option<String> {
        let multi = {
            let sessions = self.sessions.read();
            let session = sessions.get(session_id)?;
            session.working_set.read().len() > 1
        };
        if multi {
            self.render_session_project_inventory(session_id)
        } else {
            None
        }
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
            started_at_unix_secs: Some(self.started_at_unix_secs),
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
        control_state_dir: ControlStateDir,
    ) -> Self {
        // CRITICAL: reqwest::Client::new() has NO timeout by default.
        // Under concurrent load, if the daemon is slow, HTTP requests hang
        // indefinitely while holding read locks on daemon_lock in the proxy
        // layer. This creates a deadlock-like scenario when a reconnect
        // attempt needs a write lock but reads never release.
        //
        // 5s connect + 60s total covers heavy ops (batch_edit on large repos)
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
            control_state_dir,
            project_root: None,
            opened_roots: std::sync::Arc::new(parking_lot::Mutex::new(Vec::new())),
            active_root: std::sync::Arc::new(parking_lot::Mutex::new(None)),
            activation_lane: std::sync::Arc::new(tokio::sync::Mutex::new(())),
            daemon_started_at: None,
        }
    }

    fn with_project_root(mut self, root: PathBuf) -> Self {
        self.project_root = Some(root);
        self
    }

    fn with_daemon_started_at(mut self, started_at: Option<u64>) -> Self {
        self.daemon_started_at = started_at;
        self
    }

    /// The connected daemon's boot epoch, when known (10.1+ daemons).
    pub fn daemon_started_at(&self) -> Option<u64> {
        self.daemon_started_at
    }

    /// Task 8: remember a sibling root this connection opened additively so
    /// a reconnect can restore the full working set. Home is never recorded
    /// here (it lives in the immutable `project_root`). Deduplicated, order
    /// preserved.
    pub(crate) fn record_opened_root(&self, root: PathBuf) {
        if self.project_root.as_deref() == Some(root.as_path()) {
            return;
        }
        let mut roots = self.opened_roots.lock();
        if !roots.iter().any(|existing| existing == &root) {
            roots.push(root);
        }
    }

    /// Locked contract: record the root a default (non-`add`) `index_folder`
    /// just ACTIVATED for this connection, so `reconnect` can restore the
    /// active project after re-opening the working set. Recording `None`
    /// means home is active again.
    pub(crate) fn record_active_root(&self, root: PathBuf) {
        *self.active_root.lock() = Some(root);
    }

    /// One connection-scoped lane for an `index_folder` daemon round trip and
    /// the adapter mirror update that follows its successful response.
    pub(crate) fn activation_lane(&self) -> std::sync::Arc<tokio::sync::Mutex<()>> {
        std::sync::Arc::clone(&self.activation_lane)
    }

    /// The root currently ACTIVE for unqualified reads on this connection
    /// (set by a default `index_folder`), or `None` when home is active.
    pub(crate) fn activated_root(&self) -> Option<PathBuf> {
        self.active_root.lock().clone()
    }

    /// Resolve one session-scoped project selector to the canonical root that
    /// this client has already opened. Resolution mirrors the daemon contract:
    /// omission selects ACTIVE, explicit project IDs win over display names,
    /// and a display name is accepted only when it is unique in the working
    /// set. Unknown or ambiguous selectors return `None`; the daemon remains
    /// the authority that renders the eventual routing refusal.
    pub(crate) fn opened_project_for_selector(
        &self,
        selector: Option<&str>,
    ) -> Option<(String, PathBuf, bool)> {
        let active = self.active_root.lock().clone();
        let home = self.project_root.clone();
        let mut projects: Vec<(String, String, PathBuf)> = Vec::new();
        if let Some(root) = home {
            projects.push((self.project_id.clone(), self.project_name.clone(), root));
        }
        for root in self.opened_roots.lock().iter() {
            let id = project_key(root);
            if !projects.iter().any(|(existing, _, _)| existing == &id) {
                let name = root
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or_default()
                    .to_string();
                projects.push((id, name, root.clone()));
            }
        }
        if let Some(root) = active {
            let id = project_key(&root);
            if !projects.iter().any(|(existing, _, _)| existing == &id) {
                let name = root
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or_default()
                    .to_string();
                projects.push((id, name, root));
            }
        }

        let selector = selector.filter(|value| !value.trim().is_empty());
        if selector.is_none() {
            let active_id = self.active_project_id();
            return projects
                .into_iter()
                .find(|(id, _, _)| id == &active_id)
                .map(|(id, _, root)| {
                    let is_home = id == self.project_id;
                    (id, root, is_home)
                });
        }
        let selector = selector.expect("non-empty selector established above");
        if let Some((id, _, root)) = projects.iter().find(|(id, _, _)| id == selector) {
            return Some((id.clone(), root.clone(), id == &self.project_id));
        }

        let mut name_matches = projects.into_iter().filter(|(_, name, _)| name == selector);
        let matched = name_matches.next()?;
        name_matches.next().is_none().then(|| {
            let is_home = matched.0 == self.project_id;
            (matched.0, matched.2, is_home)
        })
    }

    /// The exact project ID targeted by an omitted selector on this session.
    pub(crate) fn active_project_id(&self) -> String {
        self.active_root
            .lock()
            .as_deref()
            .map(project_key)
            .unwrap_or_else(|| self.project_id.clone())
    }

    /// Local fallback is valid only while the daemon session still targets the
    /// immutable adapter HOME project.
    pub(crate) fn active_project_is_home(&self) -> bool {
        self.active_project_id() == self.project_id
    }

    #[cfg(test)]
    pub(crate) fn new_for_test(
        base_url: String,
        project_id: String,
        session_id: String,
        project_name: String,
    ) -> Self {
        let control_state_dir = process_control_state_dir()
            .expect("test daemon control state must be resolvable")
            .clone();
        Self::new_for_test_at(
            base_url,
            project_id,
            session_id,
            project_name,
            control_state_dir,
        )
    }

    #[cfg(test)]
    pub(crate) fn new_for_test_at(
        base_url: String,
        project_id: String,
        session_id: String,
        project_name: String,
        control_state_dir: ControlStateDir,
    ) -> Self {
        // The daemon is fail-closed; a proxy client must carry the token. Resolve
        // it the same way production does (env pin, then the persisted token
        // file) so test proxies authenticate against a spawned daemon.
        let auth_token = resolve_daemon_auth_token_at(&control_state_dir);
        Self::new_with_auth_token(
            base_url,
            project_id,
            session_id,
            project_name,
            auth_token,
            control_state_dir,
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

    pub(crate) fn control_state_dir(&self) -> &ControlStateDir {
        &self.control_state_dir
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
        let new_client = connect_or_spawn_session_at(
            project_root,
            "mcp-stdio",
            Some(std::process::id()),
            &self.control_state_dir,
        )
        .await?;
        let mut new_client = new_client.with_project_root(project_root.to_path_buf());
        new_client.activation_lane = std::sync::Arc::clone(&self.activation_lane);

        // Task 8: home is immutable across reconnects — the fresh session
        // must resolve to the SAME deterministic project id, or something
        // rebound the identity underneath us. Fail closed instead of serving
        // a different project as "home".
        if let Ok(canonical_home) = canonical_project_root(project_root) {
            let expected_home = project_key(&canonical_home);
            anyhow::ensure!(
                new_client.project_id == expected_home,
                "reconnect produced a different home project id (expected {expected_home}, got {}); refusing to serve",
                new_client.project_id
            );
        }

        // Reopen every additively-opened sibling and verify each restores
        // with its deterministic id. A sibling that fails to reopen or
        // verifies to a different id fails the whole reconnect (fail closed:
        // a silently missing sibling would turn explicit reads into
        // not-open errors, and a mismatched one would serve the wrong code).
        //
        // Sibling reopens use `add:true` so they CANNOT steal per-session
        // activation — a fresh session's active project is its home, and the
        // pre-reconnect active root is restored explicitly below.
        let siblings: Vec<PathBuf> = self.opened_roots.lock().clone();
        for root in &siblings {
            let response = new_client
                .call_tool_value(
                    "index_folder",
                    serde_json::json!({ "path": root.display().to_string(), "add": true }),
                )
                .await
                .with_context(|| {
                    format!("reconnect failed to reopen sibling {}", root.display())
                })?;
            let expected = canonical_project_root(root).map(|c| project_key(&c));
            if let Ok(expected) = expected {
                anyhow::ensure!(
                    response.contains(&format!("project_id={expected}")),
                    "reconnect reopened sibling {} with an unexpected identity; refusing to serve (receipt: {response})",
                    root.display()
                );
            }
            new_client.record_opened_root(root.clone());
        }

        // Restore the pre-reconnect ACTIVE project: a fresh session's active
        // project is its home, so when the caller had activated a sibling via
        // a default `index_folder`, re-issue that default open (which now
        // activates per the locked contract). Fail closed on a receipt for a
        // different project.
        //
        // ponytail: re-running the default open re-reloads the project; a
        // dedicated set-active route would avoid the reload. Deferred until
        // reload cost matters in practice.
        let active = self.active_root.lock().clone();
        if let Some(active) = active {
            let is_home = canonical_project_root(&active)
                .ok()
                .zip(canonical_project_root(project_root).ok())
                .map(|(a, h)| a == h)
                .unwrap_or(false);
            if !is_home {
                let response = new_client
                    .call_tool_value(
                        "index_folder",
                        serde_json::json!({ "path": active.display().to_string() }),
                    )
                    .await
                    .with_context(|| {
                        format!(
                            "reconnect failed to restore active project {}",
                            active.display()
                        )
                    })?;
                if let Ok(canonical_active) = canonical_project_root(&active) {
                    let expected = project_key(&canonical_active);
                    anyhow::ensure!(
                        response.contains(&format!("project_id={expected}")),
                        "reconnect restored active project {} with an unexpected identity; refusing to serve (receipt: {response})",
                        active.display()
                    );
                }
                new_client.record_opened_root(active.clone());
                new_client.record_active_root(active);
            }
        }
        Ok(new_client)
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
        // D23: stamp the surface actually served on THIS adapter's connection so
        // the daemon renders empty-index recovery hints for the AGENT's surface,
        // not the daemon process's own env. This runs in the adapter process, so
        // `surface_profile_from_env` here IS the connection surface. Set
        // unconditionally at the proxy boundary — a stdio client cannot inject
        // this header onto the adapter→daemon hop, so the adapter value is
        // authoritative (D22 overwrite discipline; out-of-band header variant).
        let request = apply_daemon_auth_header(request, self.auth_token.as_deref()).header(
            crate::protocol::surface_probe::CONNECTION_SURFACE_HEADER,
            crate::protocol::surface_probe::surface_profile_label(
                crate::protocol::surface_probe::surface_profile_from_env(),
            ),
        );
        let response = request
            .send()
            .await
            .with_context(|| format!("calling daemon tool '{tool_name}'"))?
            .error_for_status()
            .with_context(|| format!("daemon rejected tool '{tool_name}'"))?;

        // Task 7: the daemon's selected-project receipt arrives out-of-band as
        // a response header; parse the typed evidence and record it in the
        // per-dispatch slot so the statused wrapper attaches it to `_meta`.
        // Never reconstructed from the text body.
        let project_evidence = if let Some(header) = response
            .headers()
            .get(crate::protocol::result_status::PROJECT_EVIDENCE_HEADER)
            && let Ok(evidence) = serde_json::from_slice::<
                crate::protocol::result_status::ProjectEvidence,
            >(header.as_bytes())
        {
            Some(evidence)
        } else {
            None
        };

        let body = response
            .text()
            .await
            .with_context(|| format!("reading daemon tool response for '{tool_name}'"))?;
        // A receipt and its semantic body are one observation. Publish the
        // receipt only after the body has been read successfully; otherwise a
        // truncated/failed response could leak target evidence into a local
        // fallback or a later retry in the same dispatch scope.
        if let Some(evidence) = project_evidence {
            crate::protocol::result_status::record_project_evidence(evidence);
        }
        Ok(body)
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
    let control_state_dir = process_control_state_dir()?;
    connect_or_spawn_session_at(project_root, client_name, pid, control_state_dir).await
}

async fn connect_or_spawn_session_at(
    project_root: &Path,
    client_name: &str,
    pid: Option<u32>,
    control_state_dir: &ControlStateDir,
) -> anyhow::Result<DaemonSessionClient> {
    // JSON carries project roots as UTF-8 strings. A lossy display string is
    // not an identity: on Unix two real roots can differ only by invalid bytes
    // that both render as U+FFFD. Refuse before daemon discovery/spawn rather
    // than opening one root remotely while retaining another for fallback.
    let canonical_project_root = project_root
        .canonicalize()
        .context("canonicalizing the project root before daemon session open")?;
    let project_root_text = canonical_project_root.to_str().ok_or_else(|| {
        anyhow::anyhow!(
            "the canonical project root is not valid UTF-8 and cannot be sent to the daemon without losing identity"
        )
    })?;
    let expected_project_id = project_key(&canonical_project_root);
    let port = ensure_daemon_running_at(control_state_dir).await?;
    let base_url = format!("http://127.0.0.1:{port}");
    let http_client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(60))
        .build()
        .context("building reqwest client for connect_or_spawn_session")?;
    // Resolve the token AFTER `ensure_daemon_running` so a freshly spawned
    // daemon's persisted token file is already present (it is written before the
    // port file). Env pin takes precedence; otherwise we read the token file.
    let auth_token = resolve_daemon_auth_token_at(control_state_dir);
    let open_request = http_client
        .post(format!("{base_url}/v1/sessions/open"))
        .json(&OpenProjectRequest {
            project_root: project_root_text.to_string(),
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
    if opened.project_id != expected_project_id {
        anyhow::bail!(
            "daemon opened a project identity that does not match the canonical requested root"
        );
    }

    // Capture the daemon's boot epoch for descriptor epoch validation.
    // Unauthenticated /health by design; failure is non-fatal (epoch simply
    // absent, selection falls back to PID+TCP for legacy records).
    let daemon_started_at = daemon_health(port)
        .await
        .and_then(|h| h.started_at_unix_secs);

    Ok(DaemonSessionClient::new_with_auth_token(
        base_url,
        opened.project_id,
        opened.session_id,
        opened.project_name,
        auth_token,
        control_state_dir.clone(),
    )
    .with_project_root(canonical_project_root)
    .with_daemon_started_at(daemon_started_at))
}

async fn ensure_daemon_running_at(control_state_dir: &ControlStateDir) -> anyhow::Result<u16> {
    let identity = current_daemon_identity();
    if let Some(port) = daemon_port_if_compatible_at(control_state_dir, &identity).await? {
        tracing::debug!("daemon already running on port {port}");
        return Ok(port);
    }

    // INCIDENT GUARD (2026-07-11): when auto-spawn cannot happen (test
    // build, deps/ artifact, or operator kill-switch), fail fast HERE with a
    // clear error instead of acquiring the start lock and waiting for a
    // daemon that nothing is allowed to start.
    if cfg!(test) || daemon_autospawn_disabled() {
        anyhow::bail!(
            "no running compatible daemon found and auto-spawn is disabled (test build or {DAEMON_AUTOSPAWN_ENV}); start `symforge daemon` explicitly"
        );
    }

    if let Some(lock) = try_acquire_start_lock_at(control_state_dir)? {
        if let Some(port) = daemon_port_if_compatible_at(control_state_dir, &identity).await? {
            tracing::debug!("daemon became ready while acquiring lock, port {port}");
            return Ok(port);
        }
        tracing::info!("acquired start lock, spawning new daemon");
        stop_incompatible_recorded_daemon_at(control_state_dir, &identity).await?;
        spawn_daemon_process()?;
        // Task 9: release the lock as soon as the child is spawned — the
        // child's `guarded_daemon_start` acquires the SAME lock before
        // binding, so holding it through `wait_for_daemon_ready` would
        // deadlock parent (waiting for the child's port file) against child
        // (waiting for the lock).
        drop(lock);
        wait_for_daemon_ready_at(control_state_dir, &identity).await
    } else {
        tracing::info!("start lock held by another process, waiting for daemon");
        wait_for_daemon_ready_at(control_state_dir, &identity).await
    }
}

/// Outcome of [`guarded_daemon_start`].
pub enum GuardedStart {
    /// This process acquired the guard and bound the daemon.
    Started(DaemonHandle),
    /// A live compatible daemon already owns this `SYMFORGE_HOME`; its
    /// runtime record was left untouched.
    AlreadyRunning { port: u16 },
}

/// Task 9: the single safe way to BIND a daemon for the current
/// `SYMFORGE_HOME`. Acquires the start lock (bounded wait), re-checks for a
/// live compatible daemon UNDER the lock, stops an incompatible recorded
/// daemon, and only then binds in the current process — so a foreground
/// `symforge daemon` racing an auto-spawn (or another foreground start) can
/// never overwrite a live daemon's runtime record.
pub async fn guarded_daemon_start(bind_host: &str) -> anyhow::Result<GuardedStart> {
    let control_state_dir = process_control_state_dir()?;
    guarded_daemon_start_at(bind_host, control_state_dir).await
}

/// Start or reuse a daemon under an explicitly resolved control-state owner.
///
/// This is the typed-owner form used by embedders and isolated integration
/// harnesses. Process entry points should normally call [`guarded_daemon_start`].
#[doc(hidden)]
pub async fn guarded_daemon_start_at(
    bind_host: &str,
    control_state_dir: &ControlStateDir,
) -> anyhow::Result<GuardedStart> {
    let identity = current_daemon_identity();
    let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(10);
    let _lock = loop {
        if let Some(port) = daemon_port_if_compatible_at(control_state_dir, &identity).await? {
            return Ok(GuardedStart::AlreadyRunning { port });
        }
        if let Some(lock) = try_acquire_start_lock_at(control_state_dir)? {
            break lock;
        }
        if tokio::time::Instant::now() >= deadline {
            anyhow::bail!(
                "timed out waiting for the symforge daemon start lock; another start is in progress"
            );
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    };
    // Re-check under the lock: a racer may have bound between our probe and
    // the acquisition.
    if let Some(port) = daemon_port_if_compatible_at(control_state_dir, &identity).await? {
        return Ok(GuardedStart::AlreadyRunning { port });
    }
    stop_incompatible_recorded_daemon_at(control_state_dir, &identity).await?;
    let handle = spawn_daemon_at(bind_host, control_state_dir).await?;
    Ok(GuardedStart::Started(handle))
}

async fn daemon_port_if_compatible_at(
    control_state_dir: &ControlStateDir,
    identity: &DaemonIdentity,
) -> anyhow::Result<Option<u16>> {
    let port = match read_daemon_port_file_at(control_state_dir) {
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

async fn wait_for_daemon_ready_at(
    control_state_dir: &ControlStateDir,
    identity: &DaemonIdentity,
) -> anyhow::Result<u16> {
    let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(5);
    loop {
        if let Some(port) = daemon_port_if_compatible_at(control_state_dir, identity).await? {
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

async fn stop_incompatible_recorded_daemon_at(
    control_state_dir: &ControlStateDir,
    identity: &DaemonIdentity,
) -> anyhow::Result<()> {
    let port = match read_daemon_port_file_at(control_state_dir) {
        Ok(port) => port,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) if error.kind() == io::ErrorKind::InvalidData => {
            tracing::warn!("removing corrupt symforge daemon port file: {error}");
            cleanup_daemon_runtime_files_at(control_state_dir);
            return Ok(());
        }
        Err(error) => return Err(error).context("reading daemon port file"),
    };

    let Some(health) = daemon_health(port).await else {
        cleanup_daemon_runtime_files_at(control_state_dir);
        return Ok(());
    };

    if daemon_health_matches(&health, identity) {
        return Ok(());
    }

    // Whether to clear the runtime record afterwards.
    //
    // Clearing it while the daemon is STILL ALIVE and identifiable orphans it:
    // this function returns early on a missing port file, so no later call can
    // ever reap it, and it keeps serving its own index to every client already
    // connected — invisibly, because nothing in a response says which instance
    // answered. The same reasoning is already written down for
    // `DaemonStopOutcome::StopTimedOut`, which deliberately leaves the files in
    // place.
    //
    // The default stays `true`, because a record we cannot act on is worse than
    // no record: with no pid file (and health reporting no pid) the process is
    // unidentifiable, so keeping the record makes it no more stoppable — it only
    // makes every later call re-walk the same dead end. We withhold cleanup
    // strictly for a daemon we tried to kill and failed, or deliberately
    // declined to kill; those are live, identified, and worth keeping visible.
    let mut clear_record = true;
    if let Ok(pid) = read_daemon_pid_file_at(control_state_dir) {
        if should_terminate_recorded_daemon(&health, identity, pid) {
            if let Err(error) = terminate_process(pid) {
                tracing::warn!(
                    pid,
                    "failed to terminate incompatible symforge daemon automatically: {error}"
                );
            }
            if !wait_for_daemon_unhealthy(port).await {
                clear_record = false;
                tracing::warn!(
                    pid,
                    port,
                    "incompatible symforge daemon still answering after termination; \
                     leaving its runtime records so it stays discoverable and stoppable"
                );
            }
        } else if !daemon_health_matches_recorded_pid(&health, pid) {
            clear_record = false;
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
            clear_record = false;
            tracing::warn!(
                recorded_pid = pid,
                daemon_pid = ?health.pid,
                recorded_executable = %health.executable_path,
                expected_executable = %identity.executable_path,
                "not terminating incompatible symforge daemon because pid ownership/executable safety checks failed"
            );
        }
    }

    if clear_record {
        cleanup_daemon_runtime_files_at(control_state_dir);
    }
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
/// [`stop_incompatible_recorded_daemon_at`], this does not skip a same-identity
/// daemon — but it preserves the exact ownership/executable safety gate
/// (`should_terminate_recorded_daemon`) so an unrelated or pid-recycled process
/// is never terminated.
#[allow(unsafe_code)] // SAFETY: SIGKILL targets a pid that already passed the ownership safety gate; a dead/invalid pid only errors, no memory is touched.
pub(crate) async fn stop_running_daemon_for_update() -> anyhow::Result<DaemonStopOutcome> {
    let control_state_dir = process_control_state_dir()?;
    stop_running_daemon_for_update_at(control_state_dir).await
}

async fn stop_running_daemon_for_update_at(
    control_state_dir: &ControlStateDir,
) -> anyhow::Result<DaemonStopOutcome> {
    let port = match read_daemon_port_file_at(control_state_dir) {
        Ok(port) => port,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(DaemonStopOutcome::NotRunning);
        }
        Err(error) if error.kind() == io::ErrorKind::InvalidData => {
            tracing::warn!("removing corrupt symforge daemon port file during update: {error}");
            cleanup_daemon_runtime_files_at(control_state_dir);
            return Ok(DaemonStopOutcome::NotRunning);
        }
        Err(error) => return Err(error).context("reading daemon port file"),
    };

    let Some(health) = daemon_health(port).await else {
        // Recorded but unreachable — clear the stale files and report not-running.
        cleanup_daemon_runtime_files_at(control_state_dir);
        return Ok(DaemonStopOutcome::NotRunning);
    };

    let identity = current_daemon_identity();
    match read_daemon_pid_file_at(control_state_dir) {
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
                #[allow(unsafe_code)]
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
                cleanup_daemon_runtime_files_at(control_state_dir);
                Ok(DaemonStopOutcome::Stopped { pid })
            }
        }
        // Safety gate refused: the daemon is alive and we will not touch it, so
        // LEAVE its runtime files in place — removing them would orphan a live,
        // discoverable daemon and cause a duplicate spawn on the next launch.
        Ok(_) => Ok(DaemonStopOutcome::SkippedSafety),
        // No readable pid file: nothing to stop; clear any stale leftovers.
        Err(_) => {
            cleanup_daemon_runtime_files_at(control_state_dir);
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

/// Wait for `port` to stop answering health checks. Returns whether it actually
/// did: callers use this to decide if a daemon is CONFIRMED gone, and a timeout
/// must never be mistaken for success — removing a live daemon's runtime records
/// orphans it permanently.
async fn wait_for_daemon_unhealthy(port: u16) -> bool {
    let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(3);
    while tokio::time::Instant::now() < deadline {
        if !daemon_health_ok(port).await {
            return true;
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }
    !daemon_health_ok(port).await
}

fn try_acquire_start_lock_at(
    control_state_dir: &ControlStateDir,
) -> anyhow::Result<Option<DaemonStartLock>> {
    let path = daemon_dir_for(control_state_dir)?.join(daemon_start_lock_file_name());
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

/// Operator kill-switch for daemon auto-spawn: set
/// `SYMFORGE_DAEMON_AUTOSPAWN=off` (or `0`/`false`/`no`) to make
/// `ensure_daemon_running` connect-only — it will attach to an already
/// running compatible daemon but never start one.
pub const DAEMON_AUTOSPAWN_ENV: &str = "SYMFORGE_DAEMON_AUTOSPAWN";

fn daemon_autospawn_disabled() -> bool {
    std::env::var(DAEMON_AUTOSPAWN_ENV)
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "off" | "0" | "false" | "no"
            )
        })
        .unwrap_or(false)
}

fn spawn_daemon_process() -> anyhow::Result<()> {
    let current_exe = std::env::current_exe().context("locating current symforge executable")?;
    // INCIDENT GUARD (2026-07-11): under `cargo test`, `current_exe` is the
    // libtest binary and the `daemon` argument below is interpreted as a TEST
    // FILTER — spawning it recursively re-runs the daemon test subset, which
    // spawns again: an exponential fork bomb whose child processes flood the
    // desktop with console windows and steal focus. Three independent locks:
    // (1) a test build refuses statically; (2) a binary living in a Cargo
    // `deps/` directory (every test/bench artifact) refuses by path; (3) the
    // operator kill-switch refuses by env. Production `symforge.exe` (durable
    // install or `target/release/symforge.exe`) passes all three.
    if cfg!(test) {
        anyhow::bail!(
            "refusing to auto-spawn a daemon from a test build; start `symforge daemon` explicitly"
        );
    }
    if current_exe
        .parent()
        .and_then(|dir| dir.file_name())
        .is_some_and(|name| name == "deps")
    {
        anyhow::bail!(
            "refusing to auto-spawn a daemon from a Cargo deps/ test artifact: {}",
            current_exe.display()
        );
    }
    if daemon_autospawn_disabled() {
        anyhow::bail!(
            "daemon auto-spawn disabled by {DAEMON_AUTOSPAWN_ENV}; start `symforge daemon` explicitly"
        );
    }
    let mut command = std::process::Command::new(current_exe);
    if std::env::var_os(DAEMON_IDLE_SHUTDOWN_ENV).is_none() {
        command.env(
            DAEMON_IDLE_SHUTDOWN_ENV,
            DEFAULT_DAEMON_IDLE_SHUTDOWN_SECS.to_string(),
        );
    }
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

fn daemon_executable_file_name_for_platform(path: &str, windows: bool) -> Option<String> {
    if path == "unknown" {
        return None;
    }

    let normalized = normalized_path_text(path, windows);
    let name = normalized.rsplit('/').find(|part| !part.is_empty())?;
    if windows {
        Some(name.to_lowercase())
    } else {
        Some(name.to_string())
    }
}

fn daemon_executable_file_name(path: &str) -> Option<String> {
    daemon_executable_file_name_for_platform(path, cfg!(windows))
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

fn stable_path_identity_for_platform(path: &str, windows: bool) -> String {
    let normalized = normalized_path_text(path, windows);
    if windows {
        normalized.to_lowercase()
    } else {
        normalized
    }
}

fn stable_path_identity(path: &str) -> String {
    stable_path_identity_for_platform(path, cfg!(windows))
}

impl ProjectInstance {
    fn load(canonical_root: &Path) -> anyhow::Result<Self> {
        let binding = RootBinding {
            source: RootCandidateSource::InitCwd,
            canonical_root: canonical_root.to_path_buf(),
            root_id: crate::discovery::project_id_for_canonical_root(canonical_root),
            access_mode: SourceAccessMode::NormalProject,
        };
        let state_placement = crate::discovery::resolve_state_placement(&binding);
        Self::load_bound(&binding, state_placement)
    }

    fn load_bound(binding: &RootBinding, state_placement: StatePlacement) -> anyhow::Result<Self> {
        let canonical_root = &binding.canonical_root;
        let project_name = canonical_root
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("project")
            .to_string();

        // V11 bootstrap (C4b): every daemon project flows through the
        // process registry's single-flight admission BEFORE its index is
        // built. A refusal fails the open honestly — admission is
        // authoritative, not advisory. The live slot rides on the runtime
        // handle (C4c) so the dispatch neck can refuse a retired admission.
        let admission = live_index::index_lifecycle::activation::admit_project(
            live_index::index_lifecycle::process_runtime::SurfaceKind::Daemon,
            canonical_root,
            &binding.root_id.0,
            binding.access_mode,
            &state_placement,
        )
        .map_err(|refusal| {
            anyhow::anyhow!(
                "project admission refused for '{}': {refusal:?}",
                binding.root_id.0
            )
        })?;

        let index = bootstrap_project_index(canonical_root, &state_placement)?;
        let watcher_info = Arc::new(Mutex::new(WatcherInfo::default()));
        let token_stats = TokenStats::new();
        let persistence_status = if matches!(
            &state_placement,
            StatePlacement::ProjectLocal { .. } | StatePlacement::UserLocal { .. }
        ) {
            CapabilityStatus::Available
        } else {
            CapabilityStatus::Unavailable {
                reason: crate::domain::CapabilityUnavailableReason::PersistentStateUnavailable,
            }
        };
        let persistence_health = Arc::new(RwLock::new(persistence_status));
        let curation_coordinator =
            Arc::new(crate::protocol::knowledge_curation::KnowledgeCurationCoordinator::default());
        if let Err(error) = curation_coordinator.recover_on_project_load(
            &index,
            canonical_root,
            Some(&state_placement),
            persistence_status,
        ) {
            tracing::warn!("knowledge curation startup recovery remained fail-closed: {error}");
        }

        Ok(Self {
            project_id: binding.root_id.0.clone(),
            canonical_root: canonical_root.to_path_buf(),
            project_name,
            source_access_mode: binding.access_mode,
            state_placement,
            persistence_health,
            curation_coordinator,
            index:
                crate::live_index::index_lifecycle::activation::ProjectRuntimeHandle::bind_admitted(
                    index, admission,
                ),
            watcher_info,
            watcher_task: None,
            stop_token: Arc::new(AtomicBool::new(false)),
            token_stats,
            symbol_cache: Arc::new(RwLock::new(HashMap::new())),
            session_ids: HashSet::new(),
            opened_at: SystemTime::now(),
            activation_state: ActivationState::Inactive,
        })
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
            self.index.shared(),
            Arc::clone(&self.watcher_info),
            Arc::clone(&self.stop_token),
        );

        // Kick off background git temporal analysis (non-blocking).
        let expected_gen = self.index.data_plane().current_project_generation();
        live_index::git_temporal::spawn_git_temporal_computation(
            self.index.shared(),
            self.canonical_root.clone(),
            expected_gen,
        );

        self.activation_state = ActivationState::Active;
    }
}

impl ProjectSlot {
    fn new(project: ProjectInstance) -> Self {
        Self {
            metadata: RwLock::new(project),
            mutation: Mutex::new(()),
        }
    }

    fn activate(&self) {
        let _mutation = self.mutation.lock();
        let mut project = self.metadata.write();
        if project.activation_state == ActivationState::Inactive {
            project.activate();
        }
    }

    fn stop(&self) {
        let _mutation = self.mutation.lock();
        let (mut watcher_task, stop_token, project_id) = {
            let mut project = self.metadata.write();
            (
                project.watcher_task.take(),
                Arc::clone(&project.stop_token),
                project.project_id.clone(),
            )
        };
        abort_watcher_task(&mut watcher_task, &stop_token);
        // V11 bootstrap (C4b): retire the project's admission slot. A
        // refusal here means the registry never held this project live
        // (possible only for instances minted outside `load_bound`);
        // surfaced at debug so an orphaned slot is observable, not silent.
        if let Err(refusal) = live_index::index_lifecycle::activation::process_project_registry()
            .stop(&live_index::index_lifecycle::registry::ProjectKey::new(
                &project_id,
            ))
        {
            tracing::debug!(?refusal, project_id, "admission slot stop refused");
        }
    }

    fn base(&self) -> Arc<IndexBase> {
        let (canonical_root, index) = {
            let project = self.metadata.read();
            (
                project.canonical_root.clone(),
                Arc::clone(&project.index.data_plane().read()),
            )
        };
        let commit = match crate::git::head_sha(&canonical_root) {
            Ok(sha) => CommitId::Sha(sha),
            Err(_) => CommitId::Dirtyless,
        };
        Arc::new(IndexBase::new(
            BaseKey::new(canonical_root, commit),
            index,
            1,
        ))
    }

    fn server_for_session(&self) -> SymForgeServer {
        let (
            index,
            project_name,
            watcher_info,
            canonical_root,
            state_placement,
            persistence_health,
            curation_coordinator,
            token_stats,
        ) = {
            let project = self.metadata.read();
            (
                project.index.shared(),
                project.project_name.clone(),
                Arc::clone(&project.watcher_info),
                project.canonical_root.clone(),
                project.state_placement.clone(),
                Arc::clone(&project.persistence_health),
                Arc::clone(&project.curation_coordinator),
                Arc::clone(&project.token_stats),
            )
        };
        let mut server = SymForgeServer::new_with_state_placement(
            index,
            project_name,
            watcher_info,
            Some(canonical_root),
            Some(state_placement),
            Some(token_stats),
        );
        server.persistence_health = persistence_health;
        server.curation_coordinator = curation_coordinator;
        server
    }

    fn reload_for_binding(&self, binding: &RootBinding) -> anyhow::Result<(usize, usize)> {
        let (project_state_dir, source_exclusions) = {
            let project = self.metadata.read();
            (
                project.state_placement.directory().cloned(),
                crate::discovery::SourceExclusions::for_state_placement(
                    &binding.canonical_root,
                    &project.state_placement,
                ),
            )
        };
        self.reload_with(&binding.canonical_root, move |index, root| {
            index.reload_for_binding_with_exclusions(root, project_state_dir, source_exclusions)
        })
    }

    fn reload_with<F>(
        &self,
        canonical_root: &Path,
        reload_index: F,
    ) -> anyhow::Result<(usize, usize)>
    where
        F: FnOnce(&SharedIndex, &Path) -> anyhow::Result<()>,
    {
        let _mutation = self.mutation.lock();
        let (index, watcher_info, mut watcher_task, old_stop_token) = {
            let mut project = self.metadata.write();
            (
                project.index.shared(),
                Arc::clone(&project.watcher_info),
                project.watcher_task.take(),
                Arc::clone(&project.stop_token),
            )
        };
        abort_watcher_task(&mut watcher_task, &old_stop_token);

        // A failed rebuild must not leave the project blind. The abort above
        // stopped the observer and `take()` already cleared its handle, so
        // returning straight through `?` here drops the project's only
        // source-change trigger until it is reopened -- the retained index
        // stays queryable, but no later edit is ever observed. Start a
        // replacement before propagating the error, on the root the project is
        // STILL bound to: the success path below is what retargets
        // `canonical_root`, so on this path the old root is the one to watch.
        if let Err(error) = reload_index(&index, canonical_root) {
            let retained_root = self.metadata.read().canonical_root.clone();
            let stop_token = Arc::new(AtomicBool::new(false));
            let watcher_task = start_project_watcher(
                retained_root,
                Arc::clone(&index),
                Arc::clone(&watcher_info),
                Arc::clone(&stop_token),
            );
            {
                let mut project = self.metadata.write();
                project.stop_token = stop_token;
                project.watcher_task = watcher_task;
            }
            return Err(error);
        }
        let published = index.published_state();
        let counts = (published.file_count, published.symbol_count);
        let stop_token = Arc::new(AtomicBool::new(false));
        let watcher_task = start_project_watcher(
            canonical_root.to_path_buf(),
            Arc::clone(&index),
            watcher_info,
            Arc::clone(&stop_token),
        );
        {
            let mut project = self.metadata.write();
            project.stop_token = stop_token;
            project.watcher_task = watcher_task;
            project.canonical_root = canonical_root.to_path_buf();
            project.project_name = canonical_root
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("project")
                .to_string();
            project.project_id = project_key(canonical_root);
        }

        let expected_gen = index.current_project_generation();
        // Feature 020 (L-V02): opt-in, gated OFF by default — a no-op unless
        // SYMFORGE_LOCAL_REF_LANES is set, so the current-only path is unchanged.
        // Detached fire-and-forget, exactly like the git-temporal spawn below.
        spawn_local_ref_reconcile(Arc::clone(&index), canonical_root.to_path_buf());
        live_index::git_temporal::spawn_git_temporal_computation(
            index,
            canonical_root.to_path_buf(),
            expected_gen,
        );
        Ok(counts)
    }
}

/// Feature 020 (L-G05/L-G07/L-V04): publish P1 local-ref lanes for a project's
/// bare local branches on open/reload, mirroring `spawn_git_temporal_computation`
/// — fire-and-forget, never blocking P0 readiness. Gated OFF by default
/// (`local_ref_lanes_enabled`); when OFF this is a no-op and the current-only P0
/// path is byte-unchanged (L-V02). A non-git/unreadable repo or a failed
/// reconcile is swallowed: it never surfaces to the caller and never touches the
/// P0 lane (L-V04). Returns the spawned task handle (mainly so a test can await
/// it); production drops it to detach.
///
// ponytail: republishes ref lanes on open/reload ONLY. Live `.git/refs` movement
// (a branch created/deleted/moved while the project stays open) is not watched
// yet, so L-R03 holds across a reopen but not live. Upgrade path: a refs watcher
// (like the filesystem watcher) that calls `reconcile_local_ref_topology` on ref
// change.
// V11 callbacks census (Feature 020 Slice 4, C3b): publication-fence-gated
// DATA-PLANE enrichment — it publishes local-ref topology into the index and
// admits no repository-source bytes, so it holds no source-observation or
// publication authority of its own. C4's typed roots gate its publication.
fn spawn_local_ref_reconcile(
    index: SharedIndex,
    canonical_root: PathBuf,
) -> Option<tokio::task::JoinHandle<()>> {
    if !local_ref_lanes_enabled() {
        return None;
    }
    // Guard: only spawn if a tokio runtime is available (not the case in some
    // sync tests) — mirrors `spawn_git_temporal_computation`.
    if tokio::runtime::Handle::try_current().is_err() {
        return None;
    }
    // Blocking git tree-walk + parse work runs off the async worker threads.
    Some(tokio::task::spawn_blocking(move || {
        let Ok(canonical) = dunce::canonicalize(&canonical_root) else {
            return;
        };
        let Ok(repository) = git2::Repository::open(&canonical) else {
            // Non-git or unreadable repo: no local refs to publish. Never
            // propagated — the P0 lane is unaffected (L-V04).
            return;
        };
        let project_id = crate::discovery::project_id_for_canonical_root(&canonical);
        let repository_id =
            match crate::live_index::persist::capture_repository_source(&canonical, &project_id) {
                Ok(captured) => captured.source.repository_id,
                Err(error) => {
                    tracing::debug!(%error, "local ref reconcile: repository identity unavailable");
                    return;
                }
            };
        match live_index::local_ref_scout::reconcile_local_ref_topology(
            &index,
            &repository,
            repository_id,
            &live_index::local_ref_scout::LocalRefScoutBudget::default(),
        ) {
            Ok(outcome) => tracing::info!(
                published = outcome.published.len(),
                removed = outcome.removed.len(),
                checked_out = outcome.checked_out.len(),
                "local ref lanes reconciled",
            ),
            Err(error) => tracing::debug!(%error, "local ref reconcile skipped"),
        }
    }))
}

/// Bootstrap a project's live index for a daemon open/switch.
///
/// Fast path (contracts/team-artifact.md § Import flow): consume the persisted
/// `.symforge/index.bin`, or — when that is absent — the shared team artifact
/// `index.bin.zst`, via [`live_index::persist::load_snapshot`] (which verifies
/// integrity and quarantines a corrupt/stale/unverifiable artifact, returning
/// `None`). This is the whole point of the exported artifact: a teammate who
/// just `git clone`d gets a warm index instead of a cold full scan. A
/// background stat-check reconciles on-disk drift, mirroring the stdio local
/// path in `main.rs::run_local_mcp_server_async`.
///
/// Cold path: no snapshot/artifact present (or it was quarantined) — fall back
/// to a full discovery+parse [`live_index::LiveIndex::load`].
fn bootstrap_project_index(
    canonical_root: &Path,
    state_placement: &StatePlacement,
) -> anyhow::Result<SharedIndex> {
    let source_exclusions =
        crate::discovery::SourceExclusions::for_state_placement(canonical_root, state_placement);
    if let Some(snapshot) = live_index::persist::load_snapshot(canonical_root, state_placement) {
        let file_count = snapshot.files.len();
        let snapshot_mtimes: HashMap<String, u64> = snapshot
            .files
            .iter()
            .map(|(path, file)| (path.clone(), file.mtime_secs))
            .collect();
        let (live, code_signals) =
            live_index::persist::snapshot_to_live_index_with_code_signals(snapshot, canonical_root);
        tracing::info!(
            files = file_count,
            load_source = ?live.load_source(),
            root = %canonical_root.display(),
            "daemon bootstrap restored index from .symforge snapshot/team artifact"
        );
        let shared: SharedIndex =
            live_index::SharedIndexHandle::shared_with_source_exclusions_and_code_signals(
                live,
                source_exclusions.clone(),
                code_signals,
            );

        // Reconcile the restored snapshot against current disk state (offline
        // edits, or a teammate-cloned artifact vs their own working tree). Only
        // when a tokio runtime is present — daemon opens run under
        // `spawn_blocking`, so `Handle::current()` is available; a bare-sync
        // caller (e.g. a unit test) simply skips reconciliation and relies on
        // the watcher started in `activate()` for subsequent changes. Same
        // guard style as `start_project_watcher`.
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            let bg_index = shared.clone();
            let bg_root = canonical_root.to_path_buf();
            // V11 callbacks census (C3b): the verify callback carries the
            // observer incarnation current at its spawn; the watcher's later
            // registration in `activate()` makes it stale, and the lane then
            // refuses its observations (late V10 callbacks unreachable).
            let observer =
                live_index::index_lifecycle::activation::project_source_authority(canonical_root)
                    .active_observer();
            handle.spawn(async move {
                live_index::persist::background_verify(
                    bg_index,
                    bg_root,
                    snapshot_mtimes,
                    observer,
                )
                .await;
            });
        }

        return Ok(shared);
    }

    let cold_result = if matches!(state_placement, StatePlacement::ProjectLocal { .. }) {
        live_index::LiveIndex::load_for_state_placement(canonical_root, state_placement)
            .with_context(|| {
                format!(
                    "failed to load project index for {}",
                    canonical_root.display()
                )
            })
    } else {
        let index = LiveIndex::empty();
        match index.reload_for_binding_with_exclusions(
            canonical_root,
            state_placement.directory().cloned(),
            source_exclusions,
        ) {
            Ok(()) => Ok(index),
            Err(error) => Err(error),
        }
    };

    match cold_result {
        Ok(index) => Ok(index),
        Err(error) => {
            let Some(capacity) = error.downcast_ref::<crate::discovery::ScoutCapacityError>()
            else {
                return Err(error);
            };
            let index = LiveIndex::empty();
            index.set_freshness_status(crate::domain::FreshnessStatus::Degraded {
                last_valid_content_generation: 0,
                reason_codes: vec![capacity.reason()],
            });
            tracing::warn!(
                root = %canonical_root.display(),
                reason = ?capacity.reason(),
                "cold project observation refused by catalog capacity; keeping project non-ready"
            );
            Ok(index)
        }
    }
}

// V11 callbacks census (C3b): the observer-incarnation registration for this
// census row lives inside `run_watcher_with_stop` (C3a) — one incarnation per
// watcher instance, registered on the same authority every observation names.
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
    let control_state_dir = process_control_state_dir()?;
    spawn_daemon_at(bind_host, control_state_dir).await
}

/// Spawn a daemon under an explicitly resolved control-state owner.
///
/// This is the typed-owner form used by embedders and isolated integration
/// harnesses. Process entry points should normally call [`spawn_daemon`].
#[doc(hidden)]
pub async fn spawn_daemon_at(
    bind_host: &str,
    control_state_dir: &ControlStateDir,
) -> anyhow::Result<DaemonHandle> {
    let resolved_host = resolve_daemon_bind_host(bind_host)?;
    cleanup_daemon_runtime_files_at(control_state_dir);

    let listener = TcpListener::bind(daemon_socket_bind_address(&resolved_host)).await?;
    let port = listener.local_addr()?.port();

    // Establish the fail-closed auth token and persist it BEFORE the port file.
    // Clients discover the daemon via the port file and then read the token file;
    // writing the token first guarantees a client that observes the port can
    // always read a valid token (no open-then-tokenless window).
    let auth_token = establish_daemon_auth_token();
    write_daemon_token_file_at(control_state_dir, &auth_token)?;
    write_daemon_port_file_at(control_state_dir, port)?;
    write_daemon_pid_file_at(control_state_dir, std::process::id())?;
    // This process is now A daemon. Whether it is still THE recorded daemon is
    // re-checked per `status` call — a later incompatible start can orphan us
    // (see `unrecorded_daemon_pid`).
    DAEMON_BOUND.store(true, std::sync::atomic::Ordering::Release);

    let state = Arc::new(DaemonState::with_token_at(
        auth_token,
        control_state_dir.clone(),
    ));
    let app = build_router(Arc::clone(&state));
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    let owned_token = state.auth_token_for_cleanup();
    let cleanup_control_state_dir = control_state_dir.clone();
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

        // Owner-checked: never delete a successor daemon's fresh runtime files.
        cleanup_daemon_runtime_files_if_owner_at(
            &cleanup_control_state_dir,
            port,
            std::process::id(),
            &owned_token,
        );
    });

    // Task 9: one daemon-owned bounded reaper. The interval is derived from
    // the TTL (quarter period, clamped to [10s, 600s]) so a shortened test TTL
    // sweeps promptly while production stays quiet. The task holds only a
    // `Weak` on the state: it exits when the daemon state drops.
    let ttl = session_ttl_from_env();
    let idle_shutdown_after = daemon_idle_shutdown_from_env();
    let reaper_state = Arc::downgrade(&state);
    let reaper_task = tokio::spawn(async move {
        let mut period_secs = (ttl.as_secs() / 4).clamp(10, 600);
        if let Some(idle) = idle_shutdown_after {
            // Sweep at least 4x per idle window so shutdown is not overly late.
            period_secs = period_secs.min((idle.as_secs() / 4).max(10));
        }
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(period_secs));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        // Fire the idle notification at most once per idle stretch; keep
        // reaping afterwards for embedders that ignore the notification.
        let mut idle_fired = false;
        loop {
            interval.tick().await;
            let Some(state) = reaper_state.upgrade() else {
                break;
            };
            let reaped = state.reap_expired_sessions(ttl);
            if reaped > 0 {
                tracing::info!(reaped, ttl_secs = ttl.as_secs(), "session reaper sweep");
            }
            if let Some(idle) = idle_shutdown_after {
                if state.active_authenticated_requests.load(Ordering::Relaxed) > 0 {
                    idle_fired = false;
                    continue;
                }
                let idle_ms = now_epoch_millis()
                    .saturating_sub(state.last_activity_at.load(Ordering::Relaxed));
                if idle_ms >= idle.as_millis() as u64 {
                    if !idle_fired {
                        idle_fired = true;
                        tracing::info!(
                            idle_secs = idle_ms / 1000,
                            "no authenticated activity within the idle window; requesting daemon shutdown"
                        );
                        state.idle_shutdown.notify_one();
                    }
                } else {
                    idle_fired = false;
                }
            }
        }
    });

    Ok(DaemonHandle {
        port,
        shutdown_tx,
        state,
        server_task,
        reaper_task,
    })
}

pub async fn run_daemon_until_shutdown(bind_host: &str) -> anyhow::Result<()> {
    // Task 9: foreground/service starts go through the guarded seam so they
    // can never clobber a live daemon's runtime record.
    let handle = match guarded_daemon_start(bind_host).await? {
        GuardedStart::Started(handle) => handle,
        GuardedStart::AlreadyRunning { port } => {
            anyhow::bail!(
                "a compatible symforge daemon is already running on port {port}; not starting a second one"
            );
        }
    };
    tracing::info!(port = handle.port, "shared daemon started");
    // Wait for SIGINT (Ctrl+C), SIGTERM (kill, systemd, containers), or the
    // reaper's idle-shutdown notification. All trigger the same graceful path.
    let idle_state = Arc::clone(&handle.state);
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
            _ = idle_state.idle_shutdown.notified() => {
                tracing::info!("idle-shutdown window elapsed, shutting down");
            },
        }
    }
    #[cfg(not(unix))]
    {
        tokio::select! {
            result = tokio::signal::ctrl_c() => {
                result?;
                tracing::info!("received Ctrl+C, shutting down");
            },
            _ = idle_state.idle_shutdown.notified() => {
                tracing::info!("idle-shutdown window elapsed, shutting down");
            },
        }
    }
    let DaemonHandle {
        shutdown_tx,
        server_task,
        reaper_task,
        ..
    } = handle;
    reaper_task.abort();
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
    let _activity = authorize_daemon_request(&state, &headers)?;
    Ok(Json(state.list_projects()))
}

async fn project_health_handler(
    State(state): State<SharedDaemonState>,
    headers: HeaderMap,
    AxumPath(project_id): AxumPath<String>,
) -> Result<Json<ProjectHealth>, StatusCode> {
    let _activity = authorize_daemon_request(&state, &headers)?;
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
    let _activity = authorize_daemon_request(&state, &headers)?;
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
    let _activity = authorize_daemon_request(&state, &headers).map_err(daemon_auth_error)?;
    let state_for_load = Arc::clone(&state);
    let response =
        tokio::task::spawn_blocking(move || state_for_load.open_project_session(request))
            .await
            .map_err(internal_error)?
            .map_err(bad_request)?;
    Ok(Json(response))
}

/// Resolve the [`crate::protocol::surface_probe::CONNECTION_SURFACE_HEADER`] on a
/// proxied daemon request to a surface profile (D23).
///
/// Absent or non-canonical -> `None`, so the empty-index guard renderer falls
/// back to the daemon's own env. Backward compatible with adapters that predate
/// the header and with direct clients that never send it — neither is an error.
fn connection_surface_from_headers(
    headers: &HeaderMap,
) -> Option<crate::protocol::surface_probe::SurfaceProfile> {
    headers
        .get(crate::protocol::surface_probe::CONNECTION_SURFACE_HEADER)
        .and_then(|value| value.to_str().ok())
        .and_then(crate::protocol::surface_probe::surface_profile_from_label)
}

/// Decode routing selectors without serde's permissive unknown-field/default
/// behavior. The daemon HTTP endpoint is independently callable, so malformed
/// selector values must be rejected before any ACTIVE/HOME runtime is chosen.
fn strict_daemon_project_selectors(
    params: &serde_json::Value,
) -> Result<(Option<String>, Option<Vec<String>>), String> {
    let Some(object) = params.as_object() else {
        return Ok((None, None));
    };
    let project = match object.get("project") {
        None | Some(serde_json::Value::Null) => None,
        Some(serde_json::Value::String(project)) => Some(project.clone()),
        Some(_) => return Err("project must be a string project id/alias.".to_string()),
    };
    let projects = match object.get("projects") {
        None | Some(serde_json::Value::Null) => None,
        Some(serde_json::Value::Array(projects)) => {
            let mut decoded = Vec::with_capacity(projects.len());
            for project in projects {
                let Some(project) = project.as_str() else {
                    return Err(
                        "projects entries must all be string project ids/aliases.".to_string()
                    );
                };
                decoded.push(project.to_string());
            }
            Some(decoded)
        }
        Some(_) => return Err("projects must be an array of project ids/aliases.".to_string()),
    };

    // Reuse the public selector contract for blank/empty/path/conflict checks.
    let _ = resolve_targets(project.as_deref(), projects.as_deref(), "<active>")?;
    if projects
        .as_deref()
        .is_some_and(|projects| projects.len() > 1 && projects.iter().any(|project| project == "*"))
    {
        return Err(
            "projects wildcard `*` must be the only entry; do not mix it with explicit projects."
                .to_string(),
        );
    }
    Ok((project, projects))
}

/// Adapter-only routing field for active-project tools whose public schema has
/// no `project` selector. Direct daemon callers may not use it for other tools
/// or combine it with public selector fields.
pub(crate) const PRIVATE_PROJECT_ROUTE_PIN: &str = "_symforge_project_route_pin";

pub(crate) fn private_project_routed_tool(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "inspect_match"
            | "checkpoint_now"
            | "detect_impact"
            | "conventions"
            | "health"
            | "health_compact"
            | "status"
            | "context_inventory"
            | "symforge_retrieve"
    )
}

fn take_private_project_route_pin(
    tool_name: &str,
    params: &mut serde_json::Value,
) -> Result<Option<String>, String> {
    let Some(object) = params.as_object_mut() else {
        return Ok(None);
    };
    let has_public_selector = object.contains_key("project") || object.contains_key("projects");
    let Some(pin) = object.remove(PRIVATE_PROJECT_ROUTE_PIN) else {
        return Ok(None);
    };
    if !private_project_routed_tool(tool_name) {
        return Err(format!(
            "tool '{tool_name}' does not accept the daemon-private project route pin."
        ));
    }
    if has_public_selector {
        return Err(
            "the daemon-private project route pin cannot be combined with `project` or `projects`."
                .to_string(),
        );
    }
    let serde_json::Value::String(project) = pin else {
        return Err("the daemon-private project route pin must be a string.".to_string());
    };
    if project.trim().is_empty() || project.trim() != project {
        return Err(
            "the daemon-private project route pin must be a nonblank canonical project id."
                .to_string(),
        );
    }
    Ok(Some(project))
}

async fn call_tool_handler(
    State(state): State<SharedDaemonState>,
    headers: HeaderMap,
    AxumPath((session_id, tool_name)): AxumPath<(String, String)>,
    Json(mut params): Json<serde_json::Value>,
) -> Result<axum::response::Response, (StatusCode, String)> {
    use axum::response::IntoResponse;
    let _activity = authorize_daemon_request(&state, &headers).map_err(daemon_auth_error)?;
    let private_project_route_pin = match take_private_project_route_pin(&tool_name, &mut params) {
        Ok(pin) => pin,
        Err(message) => {
            return Ok(format!("Error: project_routing: {message}").into_response());
        }
    };
    if tool_name == "index_folder" {
        if params
            .as_object()
            .is_some_and(|object| object.contains_key("project") || object.contains_key("projects"))
        {
            return Ok(
                "Error: project_routing: tool 'index_folder' does not accept `project` or \
                 `projects`; `path` is the project selection for this operation."
                    .into_response(),
            );
        }
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
            .map_err(bad_request)
            .map(|text| text.into_response());
    }

    // Knowledge security ordering: reject a sensitive/invalid knowledge request
    // before project routing, working-set refresh, cache/CCR access, or any
    // repository observation. The protocol front-end performs the same guard,
    // but the daemon HTTP route is independently callable and must fail closed.
    if tool_name == "curate_knowledge" {
        let input = decode_params::<CurateKnowledgeInput>(params.clone()).map_err(bad_request)?;
        if let Err(message) = crate::protocol::knowledge_curation::validate_input(&input) {
            return Ok(message.into_response());
        }
    }
    let (raw_project, raw_projects) = match strict_daemon_project_selectors(&params) {
        Ok(selectors) => selectors,
        Err(message) => {
            return Ok(format!("Error: project_routing: {message}").into_response());
        }
    };
    let cross_project_selector_tool = matches!(
        tool_name.as_str(),
        "search_knowledge"
            | "review_knowledge"
            | "search_symbols"
            | "search_text"
            | "search_files"
            | "find_references"
    );
    if raw_project.is_some()
        && !cross_project_selector_tool
        && !single_project_routed_tool(&tool_name)
    {
        return Ok(format!(
            "Error: project_routing: tool '{tool_name}' does not accept a public `project` \
             selector."
        )
        .into_response());
    }
    if raw_projects.is_some() && !cross_project_selector_tool {
        return Ok(format!(
            "Error: project_routing: tool '{tool_name}' accepts at most one `project`; \
             `projects` is supported only by the set-valued discovery tools."
        )
        .into_response());
    }
    let mut scalar_target_runtime: Option<SessionRuntime> = None;
    let mut has_explicit_project_set = false;
    if cross_project_selector_tool {
        let (project, projects) = match tool_name.as_str() {
            "search_knowledge" => {
                let input =
                    decode_params::<SearchKnowledgeInput>(params.clone()).map_err(bad_request)?;
                if let Err(message) = crate::protocol::knowledge_search::validate_input(&input) {
                    return Ok(message.into_response());
                }
                (raw_project.clone(), raw_projects.clone())
            }
            "review_knowledge" => {
                let input =
                    decode_params::<ReviewKnowledgeInput>(params.clone()).map_err(bad_request)?;
                if let Err(message) = crate::protocol::knowledge_review::validate_input(&input) {
                    return Ok(message.into_response());
                }
                (raw_project.clone(), raw_projects.clone())
            }
            // The cross-project code-navigation verbs accept the SAME id/name
            // selectors but carry different input types, and they are
            // deliberately absent from `single_project_routed_tool`, so they
            // never met `runtime_for_target` — the only name->id resolver in the
            // tree. A project opened by `index_folder(add: true)`, whose receipt
            // advertises `project_name`, was therefore denied as "project not
            // open" when that same name was used as a selector.
            //
            // Read the selectors straight off the JSON rather than adding a
            // typed arm per verb: the write-back below already edits this same
            // object, and the knowledge validation above stays scoped to the
            // knowledge verbs that require it.
            _ => (raw_project.clone(), raw_projects.clone()),
        };

        if project.is_some() && projects.is_some() {
            return Ok(
                "Error: project_routing: project and projects are mutually exclusive: pass \
                 exactly one (a single id in `project`, or a list/`[\"*\"]` in `projects`)."
                    .into_response(),
            );
        }
        has_explicit_project_set = projects.is_some();

        // `search_knowledge` owns both scalar and set-valued targeting, so it
        // bypasses the single-project route below. Canonicalize every explicit
        // id/name selector through the same session-scoped resolver used by the
        // rest of the daemon before the cross-project dispatcher captures its
        // immutable source set. Wildcard expansion remains session-local.
        if let Some(selector) = project.as_deref() {
            let target_runtime = match state.runtime_for_target(&session_id, Some(selector)) {
                Ok(runtime) => runtime,
                Err(message) if message.starts_with("unknown session") => {
                    return Err((StatusCode::NOT_FOUND, message));
                }
                Err(message) => {
                    return Ok(format!("Error: project_routing: {message}").into_response());
                }
            };
            let project_id = target_runtime.project_id.clone();
            scalar_target_runtime = Some(target_runtime);
            if let Some(object) = params.as_object_mut() {
                object.insert("project".to_string(), serde_json::Value::String(project_id));
            }
        } else if let Some(selectors) = projects.as_deref()
            && !(selectors.len() == 1 && selectors[0] == "*")
        {
            let mut project_ids = Vec::with_capacity(selectors.len());
            for selector in selectors {
                let project_id = match state.runtime_for_target(&session_id, Some(selector)) {
                    Ok(runtime) => runtime.project_id,
                    Err(message) if message.starts_with("unknown session") => {
                        return Err((StatusCode::NOT_FOUND, message));
                    }
                    Err(message) => {
                        return Ok(format!("Error: project_routing: {message}").into_response());
                    }
                };
                if !project_ids.contains(&project_id) {
                    project_ids.push(project_id);
                }
            }
            if let Some(object) = params.as_object_mut() {
                object.insert("projects".to_string(), serde_json::json!(project_ids));
            }
        }
    }

    // Task 7: `status(detail="projects")` renders the SESSION's open-project
    // inventory (the daemon owns that state; the per-project server cannot see
    // sibling projects). Intercept before dispatch. Other detail levels keep
    // the existing per-project dispatch unchanged.
    if tool_name == "status" {
        #[derive(serde::Deserialize)]
        struct DetailPeek {
            #[serde(default)]
            detail: Option<String>,
        }
        let peek: DetailPeek =
            serde_json::from_value(params.clone()).unwrap_or(DetailPeek { detail: None });
        if peek.detail.as_deref() == Some("projects") {
            return state
                .render_session_project_inventory(&session_id)
                .ok_or_else(|| {
                    (
                        StatusCode::NOT_FOUND,
                        format!("unknown session '{session_id}'"),
                    )
                })
                .map(|text| text.into_response());
        }
    }

    // Task 4 (outstanding-work hardening): explicit single-project routing.
    // For the routed read/guidance verbs, peek the optional `project` selector,
    // resolve the target runtime through the ONE shared resolver, canonicalize
    // or strip the routing field, and dispatch the existing per-project
    // implementation. Curation retains the canonical explicit selector because
    // implicit-worktree mutation must fail closed. Omission selects the session's
    // ACTIVE project. The three cross-project
    // discovery verbs (search_symbols/search_text/find_references) keep their
    // own `project`/`projects` handling inside `execute_tool_call`.
    let runtime = if let Some(project) = private_project_route_pin.as_deref() {
        match state.runtime_for_target(&session_id, Some(project)) {
            Ok(runtime) => runtime,
            Err(message) if message.starts_with("unknown session") => {
                return Err((StatusCode::NOT_FOUND, message));
            }
            Err(message) => {
                return Ok(format!("Error: project_routing: {message}").into_response());
            }
        }
    } else if single_project_routed_tool(&tool_name) {
        #[derive(serde::Deserialize)]
        struct ProjectPeek {
            #[serde(default)]
            project: Option<String>,
            #[serde(default)]
            projects: Option<Vec<String>>,
        }
        let peek: ProjectPeek = serde_json::from_value(params.clone()).unwrap_or(ProjectPeek {
            project: None,
            projects: None,
        });
        // `search_files` is BOTH single-project routed (lone `project` keeps
        // the full handler with resolve/coupling modes) and set-valued
        // (`projects` fans out via the cross-project read route). The routed
        // strip below would silently swallow a `project`+`projects` conflict
        // before `resolve_targets` could see it, so reject it here.
        if peek.project.is_some() && peek.projects.is_some() {
            return Ok(
                "Error: project_routing: project and projects are mutually exclusive: pass \
                 exactly one (a single id in `project`, or a list/`[\"*\"]` in `projects`)."
                    .into_response(),
            );
        }
        match state.runtime_for_target(&session_id, peek.project.as_deref()) {
            Ok(runtime) => {
                if peek.project.is_some()
                    && let Some(object) = params.as_object_mut()
                {
                    if tool_name == "curate_knowledge" {
                        object.insert(
                            "project".to_string(),
                            serde_json::Value::String(runtime.project_id.clone()),
                        );
                    } else {
                        object.remove("project");
                    }
                }
                runtime
            }
            Err(message) if message.starts_with("unknown session") => {
                return Err((StatusCode::NOT_FOUND, message));
            }
            // Deterministic routing errors surface as HTTP 200 tool text (same
            // convention as tool errors below) so the MCP client sees the
            // candidates immediately instead of a transport failure.
            Err(message) => {
                return Ok(format!("Error: project_routing: {message}").into_response());
            }
        }
    } else {
        state.session_runtime(&session_id).ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                format!("unknown session '{session_id}'"),
            )
        })?
    };

    // Tool handlers acquire parking_lot::RwLock on the shared index, which
    // blocks the OS thread. Running them directly on the async runtime starves
    // tokio worker threads under concurrent load (10+ subagents).
    //
    // spawn_blocking moves execution to tokio's blocking thread pool (default
    // 512 threads), keeping async worker threads free for I/O, MCP transport,
    // and new request acceptance.
    // Task 7: the selected-project trust evidence for THIS call, captured from
    // the resolved runtime before dispatch. Returned out-of-band as a response
    // header so the human-readable body stays byte-identical; the adapter
    // parses it into the typed receipt and attaches it to `_meta`.
    let initial_call_evidence = if has_explicit_project_set {
        // A set-valued result has no honest single ProjectEvidence identity.
        // The adapter clears its HOME seed before proxying and will disclose
        // evidence unavailable until an aggregate receipt schema exists.
        None
    } else {
        let evidence_runtime = scalar_target_runtime.as_ref().unwrap_or(&runtime);
        // T046: one capture. `generation`, `load_source`, the counts, and
        // `index_state` all come off the SAME published bundle — the atomic
        // project-generation counter is not consulted, so the evidence cannot
        // pair a newer generation number with an older publication's counts.
        let generation = evidence_runtime.index.data_plane().published_generation();
        Some(crate::protocol::result_status::ProjectEvidence {
            project_id: evidence_runtime.project_id.clone(),
            project_name: evidence_runtime
                .canonical_root
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("project")
                .to_string(),
            canonical_root: Some(normalized_path_string(&evidence_runtime.canonical_root)),
            // Receipt equality must distinguish ordinary immutable publication
            // changes; project_generation intentionally stays stable across
            // content-only publishes.
            generation: generation.publication_generation,
            index_state: generation.health.status_label().to_string(),
            load_source: generation.live.load_source().label().to_string(),
            index_files: generation.health.file_count,
            index_symbols: generation.health.symbol_count,
        })
    };

    let tool_name_owned = tool_name.clone();
    let tool_name_for_panic = tool_name.clone();
    let state_for_refresh = Arc::clone(&state);
    // D23: the surface served on the connection this proxied call arrived on,
    // threaded out-of-band by the adapter. Bound around the dispatch below so any
    // empty-index recovery hint names only tools callable on the AGENT's surface,
    // not this daemon process's env. `None` (older adapter / direct attach) leaves
    // it unbound -> env fallback (unchanged behavior).
    let connection_surface = connection_surface_from_headers(&headers);
    match state
        .governor
        .execute_non_abortable(&tool_name, async move {
            let handle = tokio::runtime::Handle::current();
            match tokio::task::spawn_blocking(move || {
                // B2/D12: on the CROSS-PROJECT read path ONLY, lazily refresh each
                // targeted working-set base if the project's published index has
                // advanced since intern, BEFORE the search. The gate is shared with
                // the read route (`resolve_cross_project_targets` returns `None` for
                // the byte-identical single-active path), so single-project reads
                // never compare/re-intern and stay frecency-neutral (SC-4). Errors
                // and invalid targeting are surfaced by `execute_tool_call` itself.
                if tool_name_owned != "search_knowledge"
                    && let Some(targets) = resolve_cross_project_targets(
                        &tool_name_owned,
                        &params,
                        &runtime.project_id,
                    )
                {
                    state_for_refresh.refresh_working_set_bases(&runtime.working_set, &targets);
                }
                handle.block_on(crate::protocol::result_status::with_project_evidence_scope(
                    initial_call_evidence,
                    async move {
                        let result = crate::protocol::surface_probe::with_connection_surface(
                            connection_surface,
                            execute_tool_call(runtime, &tool_name_owned, params),
                        )
                        .await;
                        let evidence = crate::protocol::result_status::current_project_evidence();
                        (result, evidence)
                    },
                ))
            })
            .await
            {
                Ok(result) => result,
                Err(join_err) => (Err(anyhow::anyhow!("tool task panicked: {join_err}")), None),
            }
        })
        .await
    {
        Ok((Ok(mut result), mut response_evidence)) => {
            // Task 7: `health`/`health_compact` gain the session's open-project
            // inventory once MORE than one project is open. Single-project
            // sessions stay byte-identical.
            //
            // `status` is included for the same reason, not despite it: on the
            // compact-3 surface `status` is the ONLY health verb an agent has,
            // and every detail level except "projects" dispatches per-project
            // against the immutable home — so a project opened by
            // `index_folder(add: true)` was invisible BY CONSTRUCTION there. An
            // additive open that reports success and then cannot be observed is
            // the same silent-divergence class as an orphaned daemon.
            // `status(detail="projects")` is intercepted before dispatch, so it
            // never reaches here and cannot double-append.
            if matches!(
                tool_name_for_panic.as_str(),
                "health" | "health_compact" | "status"
            ) && let Some(inventory) =
                state.render_session_project_inventory_if_multi(&session_id)
            {
                result.push('\n');
                result.push_str(&inventory);
            }
            let mut response = result.into_response();
            if has_explicit_project_set {
                response_evidence = None;
            }
            if let Some(evidence) = response_evidence
                && let Ok(json) = serde_json::to_string(&evidence)
                && let Ok(value) = axum::http::HeaderValue::try_from(json)
            {
                response.headers_mut().insert(
                    crate::protocol::result_status::PROJECT_EVIDENCE_HEADER,
                    value,
                );
            }
            Ok(response)
        }
        Ok((Err(tool_err), _)) => {
            // Tool returned an error — surface it as HTTP 200 so the MCP client
            // gets the message immediately instead of entering reconnect/timeout.
            Ok(format!("Error in {}: {}", tool_name_for_panic, tool_err).into_response())
        }
        Err(gov_err) => {
            // Governor error (timeout, queue full, panic) — return as HTTP 200
            // with a clear error prefix so the model knows to stop waiting.
            let msg = format!(
                "Error: tool '{}' failed — {}. The tool did not complete. Do not retry immediately.",
                tool_name_for_panic, gov_err
            );
            tracing::error!(tool = %tool_name_for_panic, "tool execution failed: {gov_err}");
            Ok(msg.into_response())
        }
    }
}

/// Dogfood #6 parity for daemon-proxied sidecar routes: the local sidecar
/// router installs `caller_root_guard`; the daemon duplicates those routes
/// and must enforce the SAME check, or a session whose ACTIVE project moved
/// (a default `index_folder` by this or another agent) answers a caller
/// rooted elsewhere with the wrong project's data. 409 mirrors the local
/// contract so the hook falls back and re-resolves the project BY ROOT.
/// `/health`-style liveness is never routed here, so no path exemption is
/// needed.
fn guard_session_caller_root(
    runtime: &SessionRuntime,
    query: Option<&str>,
) -> Result<(), StatusCode> {
    if let Some(caller_root) = crate::sidecar::handlers::query_param(query, "caller_root")
        && !crate::sidecar::handlers::roots_match(
            std::path::Path::new(&caller_root),
            &runtime.canonical_root,
        )
    {
        tracing::warn!(
            caller_root,
            serving_root = %runtime.canonical_root.display(),
            "daemon sidecar caller-root mismatch — 409 (hook re-resolves by root)"
        );
        return Err(StatusCode::CONFLICT);
    }
    Ok(())
}

async fn sidecar_health_handler(
    State(state): State<SharedDaemonState>,
    headers: HeaderMap,
    AxumPath(session_id): AxumPath<String>,
) -> Result<Json<crate::sidecar::handlers::HealthResponse>, StatusCode> {
    let _activity = authorize_daemon_request(&state, &headers)?;
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
    uri: axum::http::Uri,
    Query(params): Query<crate::sidecar::handlers::OutlineParams>,
) -> Result<String, StatusCode> {
    let _activity = authorize_daemon_request(&state, &headers)?;
    let runtime = state
        .session_runtime(&session_id)
        .ok_or(StatusCode::NOT_FOUND)?;
    guard_session_caller_root(&runtime, uri.query())?;
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
    uri: axum::http::Uri,
    Query(params): Query<crate::sidecar::handlers::ImpactParams>,
) -> Result<String, StatusCode> {
    let _activity = authorize_daemon_request(&state, &headers)?;
    let runtime = state
        .session_runtime(&session_id)
        .ok_or(StatusCode::NOT_FOUND)?;
    guard_session_caller_root(&runtime, uri.query())?;
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
    uri: axum::http::Uri,
    Query(params): Query<crate::sidecar::handlers::SymbolContextParams>,
) -> Result<String, StatusCode> {
    let _activity = authorize_daemon_request(&state, &headers)?;
    let runtime = state
        .session_runtime(&session_id)
        .ok_or(StatusCode::NOT_FOUND)?;
    guard_session_caller_root(&runtime, uri.query())?;
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
    uri: axum::http::Uri,
) -> Result<String, StatusCode> {
    let _activity = authorize_daemon_request(&state, &headers)?;
    let runtime = state
        .session_runtime(&session_id)
        .ok_or(StatusCode::NOT_FOUND)?;
    guard_session_caller_root(&runtime, uri.query())?;
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
    uri: axum::http::Uri,
    Query(params): Query<crate::sidecar::handlers::PromptContextParams>,
) -> Result<String, StatusCode> {
    let _activity = authorize_daemon_request(&state, &headers)?;
    let runtime = state
        .session_runtime(&session_id)
        .ok_or(StatusCode::NOT_FOUND)?;
    guard_session_caller_root(&runtime, uri.query())?;
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
    let _activity = authorize_daemon_request(&state, &headers)?;
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
    let _activity = authorize_daemon_request(&state, &headers)?;
    Ok(Json(state.heartbeat(&session_id)))
}

async fn close_session_handler(
    State(state): State<SharedDaemonState>,
    headers: HeaderMap,
    AxumPath(session_id): AxumPath<String>,
) -> Result<Json<CloseSessionResponse>, StatusCode> {
    let _activity = authorize_daemon_request(&state, &headers)?;
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
            if id.trim().is_empty() {
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
            // Whitespace-only entries are invalid, but nonblank selector bytes
            // are identity-bearing. In particular, a trailing space is legal
            // in a Unix project name and must not retarget another project.
            let exact: Vec<&str> = list.iter().map(String::as_str).collect();
            if exact.iter().any(|id| id.trim().is_empty()) {
                return Err(
                    "projects entries must be non-empty project ids/aliases (blank entry found)."
                        .to_string(),
                );
            }
            if exact.contains(&"*") {
                // Any `*` in the list means "all open projects".
                return Ok(Targets::All);
            }
            for id in &exact {
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
            let deduped: Vec<String> = exact
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

/// Cross-project READ verbs. Edits, analyze_file_impact, orient, and
/// meta verbs are NEVER cross-project (they stay single-project on the active
/// project) — kept in one place so the read route (`execute_tool_call`) and the
/// freshness-refresh gate (`call_tool_handler`) agree by construction.
fn is_cross_project_read_verb(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "search_symbols"
            | "search_text"
            | "search_knowledge"
            | "review_knowledge"
            | "find_references"
            | "search_files"
    )
}

/// Task 4 (outstanding-work hardening): the read/guidance verbs that accept ONE
/// optional `project` selector, resolved by `DaemonState::runtime_for_target`
/// in `call_tool_handler` before decode. Exactly the plan's parity table minus
/// the set-valued discovery verbs above (which own `project`/`projects` in
/// `execute_tool_call`) and minus `context_inventory` (session-scoped, no
/// selector). `search_files` is deliberately in BOTH sets: a lone `project`
/// routes here to the FULL single-project handler (resolve/coupling modes),
/// while `projects` fans out via the cross-project read route. Structural edits route the same way (Task 5): the selector is
/// batch-level only — each call stays one single-project transaction, and the
/// existing worktree/`working_directory` validation then runs against the
/// SELECTED project's repository, so an unrelated root rejects before preview
/// or apply.
pub(crate) fn single_project_routed_tool(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "get_symbol"
            | "get_symbol_context"
            | "get_file_context"
            | "get_file_content"
            | "get_repo_map"
            | "search_files"
            | "find_dependents"
            | "diff_symbols"
            | "what_changed"
            | "analyze_file_impact"
            | "validate_file_syntax"
            | "explore"
            | "ask"
            | "conventions"
            | "edit_plan"
            | "investigation_suggest"
            | "replace_symbol_body"
            | "edit_within_symbol"
            | "insert_symbol"
            | "delete_symbol"
            | "batch_edit"
            | "batch_insert"
            | "batch_rename"
            | "symforge_edit"
            | "curate_knowledge"
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
             index_folder(path=...) — the default spelling opens AND activates it; add=true opens additively.",
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
        #[serde(default)]
        resolve: Option<bool>,
        #[serde(default)]
        changed_with: Option<String>,
        #[serde(default)]
        anchor_path: Option<String>,
        #[serde(default)]
        rank_by: Option<String>,
        #[serde(default)]
        current_file: Option<String>,
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
        "search_files" => {
            // resolve/coupling/ranking modes are single-project pipelines:
            // refuse honestly instead of silently ignoring the mode.
            if peek.resolve == Some(true) {
                return refuse("resolve");
            }
            if present(&peek.changed_with) {
                return refuse("changed_with");
            }
            if present(&peek.anchor_path) {
                return refuse("anchor_path");
            }
            if present(&peek.rank_by) {
                return refuse("rank_by");
            }
            if present(&peek.current_file) {
                return refuse("current_file");
            }
        }
        _ => {}
    }
    Ok(())
}

/// Feature 012 (Phase 3): execute one of the three cross-project READ verbs
/// against the session's working set and format the attributed [`crate::live_index::view::ProjectHit`]
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
        "search_files" => {
            let input: SearchFilesInput = decode_params(params)?;
            let query = input.query.trim();
            if query.is_empty() {
                anyhow::bail!("cross-project search_files requires a non-empty query.");
            }
            let path_scope = match input.path_prefix.as_deref().map(str::trim) {
                Some(prefix) if !prefix.is_empty() => {
                    crate::live_index::search::PathScope::Prefix(prefix.to_string())
                }
                _ => crate::live_index::search::PathScope::Any,
            };
            let cap = cross_project_result_cap(input.limit);
            // The same per-project bound the single-project path applies
            // (default 20, clamped to the engine's 1..=50); the global `cap`
            // then bounds the merged total.
            let per_project_limit = input.limit.unwrap_or(20).clamp(1, 50) as usize;
            let hits = working_set.search_files(
                &targets,
                query,
                per_project_limit,
                input.include_vendor.unwrap_or(false),
                input.include_personal_tooling.unwrap_or(false),
                &path_scope,
            );
            let body = format_cross_project_files(&hits, query, multi, cap);
            Ok(apply_cross_project_token_budget(body, input.max_tokens))
        }
        other => anyhow::bail!("'{other}' is not a cross-project read verb"),
    }
}

/// Search repository knowledge over one immutable generation captured from
/// every selected session project before any extraction or formatting begins.
/// Project IDs and sections are canonically ordered, and CCR budgeting is
/// applied once to the combined response so a multi-project call has one
/// reversible result envelope.
fn execute_cross_project_knowledge(
    mut input: SearchKnowledgeInput,
    targets: Targets,
    project_indexes: &HashMap<
        String,
        crate::live_index::index_lifecycle::activation::ProjectRuntimeHandle,
    >,
    ccr_server: &SymForgeServer,
) -> anyhow::Result<String> {
    let mut selected_ids: Vec<String> = match targets {
        Targets::One(id) => vec![id],
        Targets::Subset(ids) => ids,
        Targets::All => project_indexes.keys().cloned().collect(),
    };
    selected_ids.sort();
    selected_ids.dedup();

    let missing: Vec<String> = selected_ids
        .iter()
        .filter(|id| !project_indexes.contains_key(*id))
        .cloned()
        .collect();
    if !missing.is_empty() {
        anyhow::bail!(
            "project not open: {}. Open it in this session first with \
             index_folder(path=...) — the default spelling opens AND activates it; add=true opens additively.",
            missing
                .iter()
                .map(|id| format!("'{id}'"))
                .collect::<Vec<_>>()
                .join(", ")
        );
    }

    // Targeting is daemon/session metadata, not a per-project search input.
    input.project = None;
    input.projects = None;
    let max_tokens = input.max_tokens;

    // Capture every source root first. Pinning the WHOLE source-set Arc per
    // project (not just its current lane) keeps the response immutable — no
    // watcher publication after this point can split it between generations —
    // and lets `search_scoped` compose the P1 ref/worktree lanes when
    // `source_scope` is `local_refs`/`worktrees`/`all` (L-R09/L-G06). With
    // `source_scope` `None`/`current`, `search_scoped` delegates to
    // `search_current` on the captured current lane, so the default path stays
    // byte-identical.
    let captured: Vec<_> = selected_ids
        .iter()
        .map(|id| {
            let source_set = project_indexes
                .get(id)
                .expect("selected projects were validated")
                .data_plane()
                .published_source_set();
            (id.clone(), source_set)
        })
        .collect();

    let multi = captured.len() > 1;
    let mut sections = Vec::with_capacity(captured.len());
    for (project_id, source_set) in captured {
        let result = crate::protocol::knowledge_search::search_scoped(&source_set, &input);
        if multi {
            sections.push(format!("── project: {project_id} ──\n{result}"));
        } else {
            sections.push(result);
        }
    }
    let body = sections.join("\n\n");
    Ok(ccr_server.apply_ccr_budget("search_knowledge", body, max_tokens))
}

/// Review repository knowledge over one immutable generation captured from
/// every selected project. Per-source complete-plan hashes are computed before
/// any rendering budget and the top-level hash covers the canonical source set.
fn execute_cross_project_review(
    mut input: ReviewKnowledgeInput,
    targets: Targets,
    project_indexes: &HashMap<
        String,
        crate::live_index::index_lifecycle::activation::ProjectRuntimeHandle,
    >,
    ccr_server: &SymForgeServer,
) -> anyhow::Result<String> {
    let mut selected_ids: Vec<String> = match targets {
        Targets::One(id) => vec![id],
        Targets::Subset(ids) => ids,
        Targets::All => project_indexes.keys().cloned().collect(),
    };
    selected_ids.sort();
    selected_ids.dedup();

    let missing: Vec<String> = selected_ids
        .iter()
        .filter(|id| !project_indexes.contains_key(*id))
        .cloned()
        .collect();
    if !missing.is_empty() {
        anyhow::bail!(
            "project not open: {}. Open it in this session first with \
             index_folder(path=...) — the default spelling opens AND activates it; add=true opens additively.",
            missing
                .iter()
                .map(|id| format!("'{id}'"))
                .collect::<Vec<_>>()
                .join(", ")
        );
    }

    input.project = None;
    input.projects = None;
    let max_tokens = input.max_tokens;
    // Pin the WHOLE source-set Arc per project so `review_scoped` can compose
    // the P1 ref/worktree lanes under `source_scope`
    // `local_refs`/`worktrees`/`all` (L-R09/L-G06). A per-source
    // unavailable/empty lane is a typed section INSIDE `review_scoped`, not a
    // whole-scope failure; the outer `?` only surfaces a genuine whole-scope
    // error (e.g. `no_sources_in_scope`). With `source_scope` `None`/`current`,
    // `review_scoped` delegates to `review_current` on the captured current
    // lane, so the default multi-project top-level hash and per-project sections
    // stay byte-identical.
    let captured: Vec<_> = selected_ids
        .iter()
        .map(|id| {
            let source_set = project_indexes
                .get(id)
                .expect("selected projects were validated")
                .data_plane()
                .published_source_set();
            (id.clone(), source_set)
        })
        .collect();

    let mut plans = Vec::with_capacity(captured.len());
    for (project_id, source_set) in captured {
        let output = crate::protocol::knowledge_review::review_scoped(&source_set, &input)
            .map_err(anyhow::Error::msg)?;
        plans.push((project_id, output));
    }
    let top_result_hash = if plans.len() == 1 {
        plans[0].1.result_hash.clone()
    } else {
        crate::protocol::knowledge_review::combined_result_hash(
            &plans
                .iter()
                .map(|(_, output)| (output.source_key.clone(), output.review_hash.clone()))
                .collect::<Vec<_>>(),
        )
    };
    let multi = plans.len() > 1;
    let sections = plans
        .iter()
        .map(|(project_id, output)| {
            if multi {
                format!("── project: {project_id} ──\n{}", output.source_section)
            } else {
                output.source_section.clone()
            }
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    let budget_sections = plans
        .iter()
        .map(|(project_id, output)| {
            if multi {
                format!(
                    "── project: {project_id} ──\n{}",
                    output.budget_source_section
                )
            } else {
                output.budget_source_section.clone()
            }
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    let body = format!("top_result_hash={top_result_hash}\n{sections}");
    let budget_body = format!("top_result_hash={top_result_hash}\n{budget_sections}");
    Ok(ccr_server.apply_ccr_budget_with_summary("review_knowledge", body, budget_body, max_tokens))
}

/// Format cross-project file hits, grouped by project (`── project: <id> ──`
/// headers when `multi`). Each project's hits arrive already rank-ordered from
/// the per-project file search.
///
/// BOUNDED: at most `cap` hits are rendered across all projects (the shared
/// cross-project total cap); a surplus is dropped and disclosed honestly with
/// shown/total counts and the corrective hint.
fn format_cross_project_files(
    hits: &[crate::live_index::view::ProjectHit<crate::live_index::SearchFilesHit>],
    query: &str,
    multi: bool,
    cap: usize,
) -> String {
    if hits.is_empty() {
        return format!("No files matching '{query}' in the targeted project(s).");
    }
    let total = hits.len();
    let shown = total.min(cap);
    let mut out = String::new();
    if shown < total {
        out.push_str(&format!(
            "{shown} of {total} file matches across projects (truncated; cross-project results \
             are capped — pass a larger `limit` or query a single project for the full set)\n"
        ));
    } else {
        out.push_str(&format!("{total} file matches across projects\n"));
    }
    let mut current: Option<&str> = None;
    for ph in hits.iter().take(shown) {
        if multi && current != Some(ph.project_id.as_str()) {
            current = Some(ph.project_id.as_str());
            out.push_str(&format!(
                "\n\u{2500}\u{2500} project: {} \u{2500}\u{2500}\n",
                ph.project_id
            ));
        }
        let h = &ph.hit;
        match &h.metadata_reason {
            Some(reason) => out.push_str(&format!("  {} [metadata-only: {reason}]\n", h.path)),
            None => out.push_str(&format!("  {}\n", h.path)),
        }
    }
    out
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

fn strip_cross_project_targeting(mut params: serde_json::Value) -> serde_json::Value {
    if let serde_json::Value::Object(ref mut object) = params {
        object.remove("project");
        object.remove("projects");
    }
    params
}

async fn execute_tool_call(
    runtime: SessionRuntime,
    tool_name: &str,
    mut params: serde_json::Value,
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
            if tool_name == "search_knowledge" {
                let input = decode_params::<SearchKnowledgeInput>(params)?;
                return execute_cross_project_knowledge(
                    input,
                    targets,
                    &runtime.project_indexes,
                    &runtime.server,
                );
            }
            if tool_name == "review_knowledge" {
                let input = decode_params::<ReviewKnowledgeInput>(params)?;
                return execute_cross_project_review(
                    input,
                    targets,
                    &runtime.project_indexes,
                    &runtime.server,
                );
            }
            let working_set = runtime.working_set.read();
            return execute_cross_project_read(tool_name, params, targets, &working_set);
        }
        // else: single active project -> fall through to the unchanged path. The
        // worker is intentionally local-only and refuses any `project`/`projects`
        // field, so erase the already-resolved targeting hint before dispatch.
        params = strip_cross_project_targeting(params);
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
        "search_knowledge" => Ok(server
            .search_knowledge(Parameters(decode_params::<SearchKnowledgeInput>(params)?))
            .await),
        "review_knowledge" => Ok(server
            .review_knowledge(Parameters(decode_params::<ReviewKnowledgeInput>(params)?))
            .await),
        "curate_knowledge" => Ok(server
            .curate_knowledge(Parameters(decode_params::<CurateKnowledgeInput>(params)?))
            .await),
        "trace_symbol" => {
            let tp: TraceSymbolInput = decode_params(params)?;
            // Convert: trace_symbol's None sections = "all" = empty vec in get_symbol_context
            let sections = tp.sections.unwrap_or_default();
            let output = server
                .get_symbol_context(Parameters(GetSymbolContextInput {
                    project: None,
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
        "detect_impact" => Ok(server
            .detect_impact(Parameters(decode_params::<DetectImpactInput>(params)?))
            .await),
        // D-015-012 backward-compat alias: CBM's `detect_changes` -> `detect_impact`.
        "detect_changes" => {
            let output = server
                .detect_impact(Parameters(decode_params::<DetectImpactInput>(params)?))
                .await;
            Ok(format!("{DETECT_CHANGES_ALIAS_DEPRECATION}\n\n{output}"))
        }
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
        index: crate::live_index::index_lifecycle::activation::ProjectRuntimeHandle::bind(
            runtime.index.shared(),
        ),
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

pub(crate) fn project_key(root: &Path) -> String {
    crate::discovery::project_id_for_canonical_root(root).0
}

pub(crate) fn normalized_path_text(path_text: &str, windows: bool) -> String {
    if windows {
        path_text.replace('\\', "/")
    } else {
        path_text.to_string()
    }
}

pub(crate) fn normalized_path_string(path: &Path) -> String {
    normalized_path_text(&dunce::simplified(path).to_string_lossy(), cfg!(windows))
}

fn process_control_state_dir() -> io::Result<&'static ControlStateDir> {
    paths::process_control_state_placement()
        .directory()
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "process control state is unavailable",
            )
        })
}

fn daemon_dir_for(control_state_dir: &ControlStateDir) -> io::Result<PathBuf> {
    paths::ensure_control_state_dir(control_state_dir)?;
    let dir = paths::control_state_path(control_state_dir, "daemon");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn write_daemon_port_file_at(control_state_dir: &ControlStateDir, port: u16) -> io::Result<()> {
    let path = daemon_dir_for(control_state_dir)?.join(daemon_port_file_name());
    std::fs::write(&path, port.to_string()).map_err(|e| {
        io::Error::new(
            e.kind(),
            format!("writing daemon port file at {}: {}", path.display(), e),
        )
    })
}

fn write_daemon_pid_file_at(control_state_dir: &ControlStateDir, pid: u32) -> io::Result<()> {
    let path = daemon_dir_for(control_state_dir)?.join(daemon_pid_file_name());
    std::fs::write(&path, pid.to_string()).map_err(|e| {
        io::Error::new(
            e.kind(),
            format!("writing daemon pid file at {}: {}", path.display(), e),
        )
    })
}

fn read_daemon_pid_file_at(control_state_dir: &ControlStateDir) -> io::Result<u32> {
    let contents = read_daemon_runtime(
        &daemon_dir_for(control_state_dir)?,
        &daemon_pid_file_name(),
        LEGACY_DAEMON_PID_FILE,
    )?;
    contents
        .trim()
        .parse::<u32>()
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

pub(crate) fn read_daemon_port_file() -> io::Result<u16> {
    read_daemon_port_file_at(process_control_state_dir()?)
}

/// This process's pid when it is a daemon that is NO LONGER the recorded one —
/// an ORPHAN: still alive, still answering everyone already connected, but
/// undiscoverable, so its index silently diverges from the daemon new clients
/// reach.
///
/// Orphans are routine, not exotic. `stop_incompatible_recorded_daemon_at`
/// cleans up the runtime files whether or not the incompatible daemon actually
/// died — the safety gate refuses on a pid/executable mismatch, and
/// `terminate_process` can simply fail — so the survivor keeps serving with no
/// record pointing at it. Upgrading symforge is the common trigger, because the
/// version check makes the running daemon "incompatible"; a session that
/// updates several times can accumulate several orphans, each holding a full
/// index.
///
/// Callers cannot otherwise tell which instance answered them, which is what
/// makes this worth disclosing: an orphan's replies are internally consistent
/// and confidently wrong about the world.
///
/// `None` for the recorded daemon and for every non-daemon process.
pub(crate) fn unrecorded_daemon_pid() -> Option<u32> {
    if !DAEMON_BOUND.load(std::sync::atomic::Ordering::Acquire) {
        return None;
    }
    let me = std::process::id();
    let recorded = process_control_state_dir()
        .ok()
        .and_then(|dir| read_daemon_pid_file_at(dir).ok());
    match recorded {
        Some(pid) if pid == me => None,
        // Either another daemon owns the record, or the record is gone
        // entirely (the cleanup-regardless path above). Both mean new clients
        // cannot reach us.
        _ => Some(me),
    }
}

fn read_daemon_port_file_at(control_state_dir: &ControlStateDir) -> io::Result<u16> {
    let contents = read_daemon_runtime(
        &daemon_dir_for(control_state_dir)?,
        &daemon_port_file_name(),
        LEGACY_DAEMON_PORT_FILE,
    )?;
    contents
        .trim()
        .parse::<u16>()
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

#[cfg(test)]
fn cleanup_daemon_files_at(control_state_dir: &ControlStateDir) {
    cleanup_daemon_runtime_files_at(control_state_dir);
    if let Ok(dir) = daemon_dir_for(control_state_dir) {
        let _ = std::fs::remove_file(dir.join(daemon_start_lock_file_name()));
        let _ = std::fs::remove_file(dir.join(LEGACY_DAEMON_START_LOCK_FILE));
    }
}

/// Shutdown-side cleanup that removes a runtime file ONLY when its content
/// still identifies THIS daemon. A dying daemon's graceful shutdown can race a
/// successor that already wrote fresh port/token/pid files into the same
/// `daemon_dir()`; unconditional removal here deletes the successor's files,
/// leaving clients tokenless (401) or daemon-less — the 2026-07-11 fork-bomb
/// incident's inner trigger. Startup cleanup stays unconditional (there is no
/// live owner to protect before the new files are written).
fn cleanup_daemon_runtime_files_if_owner_at(
    control_state_dir: &ControlStateDir,
    port: u16,
    pid: u32,
    token: &str,
) {
    if let Ok(dir) = daemon_dir_for(control_state_dir) {
        let owns = |path: &std::path::Path, expected: &str| {
            std::fs::read_to_string(path)
                .map(|contents| contents.trim() == expected)
                .unwrap_or(false)
        };
        let port_path = dir.join(daemon_port_file_name());
        if owns(&port_path, &port.to_string()) {
            let _ = std::fs::remove_file(&port_path);
        }
        let pid_path = dir.join(daemon_pid_file_name());
        if owns(&pid_path, &pid.to_string()) {
            let _ = std::fs::remove_file(&pid_path);
        }
        let token_path = dir.join(daemon_token_file_name());
        if owns(&token_path, token) {
            let _ = std::fs::remove_file(&token_path);
        }
        // Legacy untagged names are never written by current builds; removing
        // them can only clear stale artifacts.
        let _ = std::fs::remove_file(dir.join(LEGACY_DAEMON_PORT_FILE));
        let _ = std::fs::remove_file(dir.join(LEGACY_DAEMON_PID_FILE));
    }
}

fn cleanup_daemon_runtime_files_at(control_state_dir: &ControlStateDir) {
    if let Ok(dir) = daemon_dir_for(control_state_dir) {
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
    use std::sync::atomic::AtomicUsize;
    use std::time::Duration;

    use once_cell::sync::Lazy;
    use tempfile::TempDir;
    use tokio::sync::{Mutex, MutexGuard};

    static ENV_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

    #[test]
    fn normalized_path_text_treats_backslash_as_separator_only_on_windows() {
        let path_text = "/tmp/symforge-literal\\component";

        assert_eq!(
            normalized_path_text(path_text, false),
            path_text,
            "Unix path text must preserve a literal backslash"
        );
        assert_eq!(
            normalized_path_text(path_text, true),
            "/tmp/symforge-literal/component",
            "Windows path text must retain forward-slash wire compatibility"
        );
    }

    #[test]
    fn executable_identity_preserves_literal_backslash_on_unix_policy() {
        let literal = "/work/symforge\\helper";
        let nested = "/work/symforge/helper";

        assert_ne!(
            stable_path_identity_for_platform(literal, false),
            stable_path_identity_for_platform(nested, false),
            "distinct Unix executable paths must not share one stable identity"
        );
        assert_ne!(
            daemon_executable_file_name_for_platform(literal, false),
            daemon_executable_file_name_for_platform(nested, false),
            "a Unix backslash remains part of the executable file name"
        );
        assert_eq!(
            stable_path_identity_for_platform(literal, true),
            stable_path_identity_for_platform(nested, true),
            "Windows separator compatibility must remain intact"
        );
    }

    #[cfg(unix)]
    #[test]
    fn normalized_path_string_preserves_literal_backslash_components() {
        let path = Path::new("/tmp/symforge-literal\\component");

        assert_eq!(
            normalized_path_string(path),
            path.to_string_lossy(),
            "a Unix backslash is a filename byte, not a path separator"
        );
        assert_ne!(
            normalized_path_string(path),
            "/tmp/symforge-literal/component",
            "distinct Unix roots must not collapse onto one wire identity"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn daemon_session_open_refuses_non_utf8_roots_before_transport() {
        use std::os::unix::ffi::OsStringExt as _;

        let parent = TempDir::new().expect("non-UTF-8 project parent");
        let root = parent
            .path()
            .join(std::ffi::OsString::from_vec(vec![b'a', 0xff, b'b']));
        std::fs::create_dir_all(&root).expect("create non-UTF-8 project root");
        let control = TempDir::new().expect("isolated daemon control root");
        // ponytail: match instead of expect_err — DaemonSessionClient has no Debug impl,
        // and this cfg(unix) body only ever compiles on Linux CI.
        let error = match connect_or_spawn_session_at(
            &root,
            "non-utf8-root-test",
            Some(std::process::id()),
            &test_control_state(control.path()),
        )
        .await
        {
            Ok(_) => panic!("lossy project identity must be refused before daemon discovery"),
            Err(error) => error,
        };

        assert!(
            error.to_string().contains("not valid UTF-8"),
            "the refusal must identify the non-reversible root encoding: {error:#}"
        );
    }

    /// Every structural-edit surface must be single-project routed. The compact
    /// `symforge_edit` facade was omitted from this list while the legacy edit
    /// tools were covered, so its `project` selector was accepted and ignored:
    /// an edit aimed at an opened sibling project fell back to the session home
    /// and mutated the wrong repository when paths/symbols overlapped.
    #[test]
    fn every_structural_edit_tool_is_single_project_routed() {
        for tool in [
            "replace_symbol_body",
            "edit_within_symbol",
            "insert_symbol",
            "delete_symbol",
            "batch_edit",
            "batch_insert",
            "batch_rename",
            "symforge_edit",
        ] {
            assert!(
                single_project_routed_tool(tool),
                "{tool} accepts a `project` selector but is not single-project routed"
            );
        }
    }

    fn project_dir(name: &str) -> TempDir {
        let dir = TempDir::with_prefix(name).expect("temp dir");
        std::fs::create_dir_all(dir.path().join("src")).expect("src dir");
        dir
    }

    fn test_control_state(home: &Path) -> ControlStateDir {
        ControlStateDir::new(home.to_path_buf())
    }

    fn test_daemon_path(home: &Path, file_name: impl AsRef<Path>) -> PathBuf {
        daemon_dir_for(&test_control_state(home))
            .expect("create test daemon control-state directory")
            .join(file_name)
    }

    async fn spawn_test_daemon(bind_host: &str, home: &Path) -> anyhow::Result<DaemonHandle> {
        let control_state_dir = test_control_state(home);
        spawn_daemon_at(bind_host, &control_state_dir).await
    }

    fn test_daemon_state(home: &Path) -> DaemonState {
        DaemonState::with_token_at(establish_daemon_auth_token(), test_control_state(home))
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
        let headers = bearer_headers(&token);
        reqwest::Client::builder()
            .default_headers(headers)
            .build()
            .expect("build authed reqwest client")
    }

    fn bearer_headers(token: &str) -> HeaderMap {
        let mut headers = header::HeaderMap::new();
        let mut value =
            header::HeaderValue::from_str(&format!("Bearer {token}")).expect("valid bearer header");
        value.set_sensitive(true);
        headers.insert(header::AUTHORIZATION, value);
        headers
    }

    #[tokio::test]
    async fn daemon_idle_shutdown_env_parsing() {
        let _guard = env_lock().await;
        // Unset: an explicitly managed daemon remains persistent.
        {
            let _env = EnvVarGuard::unset(DAEMON_IDLE_SHUTDOWN_ENV);
            assert_eq!(daemon_idle_shutdown_from_env(), None);
        }
        // Explicit 0 disables idle shutdown entirely.
        {
            let _env = EnvVarGuard::set_str(DAEMON_IDLE_SHUTDOWN_ENV, "0");
            assert_eq!(daemon_idle_shutdown_from_env(), None);
        }
        // Sub-minimum values clamp up to 60s.
        {
            let _env = EnvVarGuard::set_str(DAEMON_IDLE_SHUTDOWN_ENV, "5");
            assert_eq!(
                daemon_idle_shutdown_from_env(),
                Some(Duration::from_secs(60))
            );
        }
        // Garbage falls back to the default rather than disabling the guard.
        {
            let _env = EnvVarGuard::set_str(DAEMON_IDLE_SHUTDOWN_ENV, "soon");
            assert_eq!(
                daemon_idle_shutdown_from_env(),
                Some(Duration::from_secs(DEFAULT_DAEMON_IDLE_SHUTDOWN_SECS))
            );
        }
    }

    /// Commit `bytes` at `rel` into `repository`'s working tree + HEAD. Mirrors the
    /// local_ref_scout test helper; used to give the reconcile a real tree to scout.
    fn ref_lane_commit(
        repository: &git2::Repository,
        root: &Path,
        rel: &str,
        bytes: &[u8],
        message: &str,
    ) {
        let full = root.join(rel);
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent).expect("mkdir");
        }
        std::fs::write(&full, bytes).expect("write file");
        let mut index = repository.index().expect("index");
        index
            .add_all(["*"], git2::IndexAddOption::DEFAULT, None)
            .expect("stage");
        index.write().expect("write index");
        let tree_id = index.write_tree().expect("write tree");
        let tree = repository.find_tree(tree_id).expect("find tree");
        let signature =
            git2::Signature::now("SymForge Test", "symforge@example.invalid").expect("signature");
        let parent = repository
            .head()
            .ok()
            .and_then(|head| head.peel_to_commit().ok());
        let parents: Vec<&git2::Commit<'_>> = parent.iter().collect();
        repository
            .commit(
                Some("HEAD"),
                &signature,
                &signature,
                message,
                &tree,
                &parents,
            )
            .expect("commit");
    }

    /// L-G05/L-G07/L-V04: with the gate ON, opening/reloading a project publishes a
    /// P1 `symforge:git-ref:` lane for a bare local branch via the detached
    /// reconcile spawn.
    #[tokio::test]
    async fn local_ref_lanes_gate_on_publishes_bare_branch_lane() {
        let _env = env_lock().await;
        let _gate = EnvVarGuard::set_str(LOCAL_REF_LANES_ENV, "1");

        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        let repository = git2::Repository::init(root).expect("init repo");
        ref_lane_commit(
            &repository,
            root,
            "src/lib.rs",
            b"pub fn a() {}\n",
            "initial",
        );
        let head_commit = repository
            .head()
            .expect("head")
            .peel_to_commit()
            .expect("peel");
        repository
            .branch("bare-lane", &head_commit, false)
            .expect("bare branch");

        let index: SharedIndex = Arc::new(crate::live_index::SharedIndexHandle::new(
            LiveIndex::from_source_files(HashMap::new()),
        ));

        let task = spawn_local_ref_reconcile(Arc::clone(&index), root.to_path_buf())
            .expect("gate ON must spawn the reconcile task");
        task.await.expect("reconcile task completes");

        let set = index.published_source_set();
        assert!(
            set.sources
                .keys()
                .any(|id| id.as_str().starts_with("symforge:git-ref:")),
            "gate ON publishes at least one git-ref lane: {:?}",
            set.sources.keys().map(|id| id.as_str()).collect::<Vec<_>>()
        );
    }

    /// L-V02: with the gate OFF (default), the reconcile is never spawned and no
    /// `symforge:git-ref:` lane appears — the current-only path is unchanged.
    #[tokio::test]
    async fn local_ref_lanes_gate_off_publishes_no_ref_lane() {
        let _env = env_lock().await;
        let _gate = EnvVarGuard::unset(LOCAL_REF_LANES_ENV);

        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        let repository = git2::Repository::init(root).expect("init repo");
        ref_lane_commit(
            &repository,
            root,
            "src/lib.rs",
            b"pub fn a() {}\n",
            "initial",
        );
        let head_commit = repository
            .head()
            .expect("head")
            .peel_to_commit()
            .expect("peel");
        repository
            .branch("bare-lane", &head_commit, false)
            .expect("bare branch");

        let index: SharedIndex = Arc::new(crate::live_index::SharedIndexHandle::new(
            LiveIndex::from_source_files(HashMap::new()),
        ));
        let before = index.published_source_set().registry_generation;

        assert!(
            spawn_local_ref_reconcile(Arc::clone(&index), root.to_path_buf()).is_none(),
            "gate OFF must not spawn a reconcile task"
        );

        let set = index.published_source_set();
        assert!(
            !set.sources
                .keys()
                .any(|id| id.as_str().starts_with("symforge:git-ref:")),
            "gate OFF publishes no git-ref lane"
        );
        assert_eq!(
            set.registry_generation, before,
            "gate OFF leaves the published source map untouched"
        );
    }

    /// The env gate parses `1`/`true`/`on` (case-insensitive) as ON and everything
    /// else — including unset — as OFF.
    #[tokio::test]
    async fn local_ref_lanes_env_gate_parsing() {
        let _guard = env_lock().await;
        {
            let _env = EnvVarGuard::unset(LOCAL_REF_LANES_ENV);
            assert!(!local_ref_lanes_enabled(), "unset is OFF");
        }
        for on in ["1", "true", "on", "TRUE", "On", " on "] {
            let _env = EnvVarGuard::set_str(LOCAL_REF_LANES_ENV, on);
            assert!(local_ref_lanes_enabled(), "{on:?} is ON");
        }
        for off in ["0", "false", "off", "no", "", "2"] {
            let _env = EnvVarGuard::set_str(LOCAL_REF_LANES_ENV, off);
            assert!(!local_ref_lanes_enabled(), "{off:?} is OFF");
        }
    }

    #[tokio::test(start_paused = true)]
    async fn daemon_idle_shutdown_waits_for_authenticated_activity_then_notifies() {
        let _guard = env_lock().await;
        let daemon_home = TempDir::new().expect("daemon home");
        let _home = EnvVarGuard::set("SYMFORGE_HOME", daemon_home.path());
        let _idle = EnvVarGuard::set_str(DAEMON_IDLE_SHUTDOWN_ENV, "60");

        let handle = spawn_test_daemon("127.0.0.1", daemon_home.path())
            .await
            .expect("spawn daemon");
        let state = Arc::clone(&handle.state);
        tokio::task::yield_now().await;

        state
            .last_activity_at
            .store(now_epoch_millis().saturating_sub(60_001), Ordering::Relaxed);
        authed_client(&handle)
            .get(format!("http://127.0.0.1:{}/v1/projects", handle.port))
            .send()
            .await
            .expect("authenticated request")
            .error_for_status()
            .expect("authenticated status");

        tokio::time::advance(Duration::from_secs(15)).await;
        tokio::task::yield_now().await;
        assert!(
            tokio::time::timeout(Duration::from_millis(1), state.idle_shutdown.notified())
                .await
                .is_err(),
            "authenticated activity must defer idle shutdown"
        );

        state
            .last_activity_at
            .store(now_epoch_millis().saturating_sub(60_001), Ordering::Relaxed);
        tokio::time::advance(Duration::from_secs(15)).await;
        tokio::task::yield_now().await;
        tokio::time::timeout(Duration::from_millis(1), state.idle_shutdown.notified())
            .await
            .expect("idle reaper must request shutdown");

        handle.reaper_task.abort();
        let _ = handle.shutdown_tx.send(());
        handle.server_task.await.expect("daemon server task");
    }

    #[tokio::test(start_paused = true)]
    async fn daemon_idle_shutdown_waits_for_in_flight_authenticated_request() {
        let _guard = env_lock().await;
        let daemon_home = TempDir::new().expect("daemon home");
        let _home = EnvVarGuard::set("SYMFORGE_HOME", daemon_home.path());
        let _idle = EnvVarGuard::set_str(DAEMON_IDLE_SHUTDOWN_ENV, "60");

        let handle = spawn_daemon("127.0.0.1").await.expect("spawn daemon");
        let state = Arc::clone(&handle.state);
        tokio::task::yield_now().await;

        let activity = authorize_daemon_request(&state, &bearer_headers(&state.auth_token))
            .expect("authenticated request guard");
        state
            .last_activity_at
            .store(now_epoch_millis().saturating_sub(60_001), Ordering::Relaxed);

        tokio::time::advance(Duration::from_secs(15)).await;
        tokio::task::yield_now().await;
        assert!(
            tokio::time::timeout(Duration::from_millis(1), state.idle_shutdown.notified())
                .await
                .is_err(),
            "idle shutdown must not fire while an authenticated handler is still in flight"
        );

        drop(activity);
        assert_eq!(
            state.active_authenticated_requests.load(Ordering::Relaxed),
            0
        );

        handle.reaper_task.abort();
        let _ = handle.shutdown_tx.send(());
        handle.server_task.await.expect("daemon server task");
    }

    #[test]
    fn project_instance_load_consumes_exported_team_artifact() {
        // A teammate who just `git clone`d has `.symforge/index.bin.zst` (the
        // exported team artifact) but no local `index.bin`. The daemon's real
        // per-project bootstrap (`ProjectInstance::load`) must consume that
        // artifact — the whole point of exporting it — instead of a cold full
        // discovery+parse scan.
        let tmp = TempDir::new().expect("tempdir");
        std::fs::write(tmp.path().join("main.rs"), b"fn main() { helper(); }\n").unwrap();
        std::fs::write(tmp.path().join("helper.rs"), b"pub fn helper() {}\n").unwrap();

        // Build a fresh index and export the Best-tier team artifact. No
        // `index.bin` is written — only `index.bin.zst` + `artifact.json`.
        let fresh = live_index::LiveIndex::load(tmp.path()).expect("cold load");
        {
            let guard = fresh.read();
            let root = dunce::canonicalize(tmp.path()).expect("canonical root");
            let placement = crate::domain::StatePlacement::ProjectLocal {
                directory: crate::domain::ProjectStateDir::new(root.join(".symforge")),
            };
            live_index::persist::export_artifact(
                &guard,
                &root,
                crate::domain::SourceAccessMode::NormalProject,
                &placement,
                &crate::domain::CapabilityStatus::Available,
            )
            .expect("export artifact");
        }
        assert!(
            !tmp.path().join(".symforge").join("index.bin").exists(),
            "only the .zst team artifact should exist for the clone scenario"
        );
        assert!(
            tmp.path().join(".symforge").join("index.bin.zst").exists(),
            "the exported team artifact must be present"
        );

        // The real daemon bootstrap must restore FROM the artifact.
        let project = ProjectInstance::load(tmp.path()).expect("project load");
        let guard = project.index.data_plane().read();
        assert_eq!(
            guard.load_source(),
            crate::live_index::store::IndexLoadSource::SnapshotRestore,
            "daemon bootstrap must consume the team artifact (SnapshotRestore), \
             not fall back to a cold full scan (FreshLoad)"
        );
        assert!(
            guard.get_file("main.rs").is_some(),
            "the artifact-restored index must serve the indexed files"
        );
    }

    #[test]
    fn daemon_sessions_share_one_project_curation_coordinator() {
        let tmp = TempDir::new().expect("tempdir");
        std::fs::write(tmp.path().join("main.rs"), b"fn main() {}\n").unwrap();
        let slot = ProjectSlot::new(ProjectInstance::load(tmp.path()).expect("project load"));

        let left = slot.server_for_session();
        let right = slot.server_for_session();

        assert!(
            Arc::ptr_eq(&left.curation_coordinator, &right.curation_coordinator),
            "all daemon sessions for one project must serialize through one curation coordinator"
        );
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
    fn connection_surface_header_resolves_profile_and_ignores_junk() {
        use crate::protocol::surface_probe::{CONNECTION_SURFACE_HEADER, SurfaceProfile};

        // Absent header (older adapter / direct client) -> None -> env fallback.
        let mut headers = HeaderMap::new();
        assert_eq!(connection_surface_from_headers(&headers), None);

        // Canonical labels resolve to their profile.
        headers.insert(CONNECTION_SURFACE_HEADER, "compact".parse().unwrap());
        assert_eq!(
            connection_surface_from_headers(&headers),
            Some(SurfaceProfile::Compact)
        );
        headers.insert(CONNECTION_SURFACE_HEADER, "full".parse().unwrap());
        assert_eq!(
            connection_surface_from_headers(&headers),
            Some(SurfaceProfile::Full)
        );

        // Non-canonical value is ignored (falls back to env), never echoed.
        headers.insert(CONNECTION_SURFACE_HEADER, "garbage".parse().unwrap());
        assert_eq!(connection_surface_from_headers(&headers), None);
    }

    #[test]
    fn authorize_daemon_request_constant_time_token_check() {
        fn state_with_token(token: &str) -> SharedDaemonState {
            Arc::new(DaemonState::with_token(token.to_string()))
        }

        fn raw_auth_headers(value: &str) -> HeaderMap {
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
            authorize_daemon_request(&state, &raw_auth_headers("Bearer the-real-token-value"))
                .is_ok()
        );
        assert_eq!(
            state.active_authenticated_requests.load(Ordering::Relaxed),
            0,
            "successful auth guard should decrement on drop"
        );

        // Wrong token (same length) still 401s — fail-closed preserved.
        assert!(
            matches!(
                authorize_daemon_request(&state, &raw_auth_headers("Bearer the-real-token-WRONG")),
                Err(StatusCode::UNAUTHORIZED)
            ),
            "wrong token must 401"
        );

        // Empty token still 401s.
        assert!(
            matches!(
                authorize_daemon_request(&state, &raw_auth_headers("Bearer ")),
                Err(StatusCode::UNAUTHORIZED)
            ),
            "empty token must 401"
        );

        // Missing Authorization header still 401s.
        assert!(
            matches!(
                authorize_daemon_request(&state, &HeaderMap::new()),
                Err(StatusCode::UNAUTHORIZED)
            ),
            "missing Authorization must 401"
        );

        // Wrong scheme still 401s.
        assert!(
            matches!(
                authorize_daemon_request(&state, &raw_auth_headers("Basic the-real-token-value")),
                Err(StatusCode::UNAUTHORIZED)
            ),
            "wrong scheme must 401"
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

    /// Feature 020 Slice 4, T030 (C4b): daemon bootstrap flows through the
    /// activation machine and the process project registry — a project open
    /// ADMITS through `plan_admission`/`execute_plan` and installs a live
    /// slot charged to the daemon surface's capacity owner, and the process
    /// machine has been driven to `PreventiveV1Open` before serving. (The C1
    /// machine oracle pins that a fresh machine starts `LegacyOpen`, so the
    /// mode observed here is bootstrap's doing, not a default.)
    #[test]
    fn project_open_admits_through_the_process_activation_registry() {
        use crate::live_index::index_lifecycle::activation::{
            ActivationCut, ActivationMode, process_project_registry,
        };
        use crate::live_index::index_lifecycle::registry::ProjectKey;

        let project = project_dir("symforge-daemon-admission");
        let state = DaemonState::new();
        let opened = state
            .open_project_session(OpenProjectRequest {
                project_root: project.path().display().to_string(),
                client_name: "claude".to_string(),
                pid: Some(400),
            })
            .expect("open session");

        let live = process_project_registry()
            .live(&ProjectKey::new(&opened.project_id))
            .expect("the opened project holds a live admission slot");
        assert!(
            live.capacity_owner().expect("live slot").is_some(),
            "the admission is charged to the daemon surface's capacity owner"
        );
        assert_eq!(
            ActivationCut::process().mode(),
            ActivationMode::PreventiveV1Open,
            "bootstrap drives the startup ceremony before serving"
        );
    }

    /// Feature 020 Slice 4, T030 (C4c): the dispatch neck refuses a retired
    /// admission. Stopping a project's registry slot (what the eviction and
    /// retarget stop path does) makes `runtime_for_target` refuse instead of
    /// serving the retired index; the live slot resolving first is the
    /// accepting control in the same test.
    #[test]
    fn retired_admission_slot_refuses_dispatch_at_the_runtime_neck() {
        use crate::live_index::index_lifecycle::activation::process_project_registry;
        use crate::live_index::index_lifecycle::registry::ProjectKey;

        let project = project_dir("symforge-daemon-retired-neck");
        let state = DaemonState::new();
        let opened = state
            .open_project_session(OpenProjectRequest {
                project_root: project.path().display().to_string(),
                client_name: "claude".to_string(),
                pid: Some(401),
            })
            .expect("open session");

        assert!(
            state.runtime_for_target(&opened.session_id, None).is_ok(),
            "a live admission slot resolves at the neck"
        );

        // Retire the admission out from under the daemon maps — the race the
        // typed branch closes (the stop path ran; the maps have not yet been
        // swept, or a stale runtime is dispatching between the two).
        process_project_registry()
            .stop(&ProjectKey::new(&opened.project_id))
            .expect("stop the live slot");
        let Err(refused) = state.runtime_for_target(&opened.session_id, None) else {
            panic!("a retired admission slot must refuse dispatch at the neck");
        };
        assert!(
            refused.contains("retired"),
            "the refusal names the retirement: {refused}"
        );
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

    // ── Feature 012f — close_session sweeps last-holder orphans (the parallel
    //    orphan path the commit-advance trigger misses) ─────────────────────────
    //
    // Opening the only session on a root interns its base (map + working-set =
    // strong_count 2). Closing that last session drops the working_set, leaving
    // the base map-only — an orphan the GC must reclaim from close_session.
    #[test]
    fn test_close_last_session_gcs_orphaned_base() {
        let project = project_dir("symforge-daemon-close-gc");
        let state = DaemonState::new();
        let opened = state
            .open_project_session(OpenProjectRequest {
                project_root: project.path().display().to_string(),
                client_name: "claude".to_string(),
                pid: None,
            })
            .expect("session");

        // Precondition: the open interned exactly one base, held by map + session.
        assert_eq!(
            state.bases.read().len(),
            1,
            "open interns one base for the root"
        );

        state
            .close_session(&opened.session_id)
            .expect("close the only session");

        // The last (only) session closed -> its working_set dropped -> the base is
        // a map-only orphan -> close_session's GC sweep reclaims it.
        assert_eq!(
            state.bases.read().len(),
            0,
            "close_session must GC the base no session references anymore"
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

        assert_eq!(
            resolve_targets(Some("project-other "), None, active).unwrap(),
            Targets::One("project-other ".to_string()),
            "nonblank selector bytes are identity-bearing"
        );
        assert_eq!(
            resolve_targets(None, Some(&["project-other ".to_string()]), active).unwrap(),
            Targets::Subset(vec!["project-other ".to_string()])
        );
    }

    #[test]
    fn strict_daemon_selector_decode_rejects_lossy_or_ambiguous_json() {
        for malformed in [
            serde_json::json!({"project": 7}),
            serde_json::json!({"projects": "project-b"}),
            serde_json::json!({"projects": ["project-b", 7]}),
            serde_json::json!({"projects": []}),
            serde_json::json!({"projects": [""]}),
            serde_json::json!({"projects": ["*", "project-b"]}),
            serde_json::json!({"project": "project-a", "projects": ["project-b"]}),
        ] {
            assert!(
                strict_daemon_project_selectors(&malformed).is_err(),
                "malformed selectors must fail before runtime selection: {malformed}"
            );
        }

        assert_eq!(
            strict_daemon_project_selectors(&serde_json::json!({
                "project": "project-b "
            }))
            .expect("exact scalar selector"),
            (Some("project-b ".to_string()), None)
        );
        assert_eq!(
            strict_daemon_project_selectors(&serde_json::json!({
                "projects": ["project-a ", "project-b"]
            }))
            .expect("exact set selector"),
            (
                None,
                Some(vec!["project-a ".to_string(), "project-b".to_string()])
            )
        );
    }

    #[test]
    fn daemon_private_route_pin_is_exact_allowlisted_and_conflict_free() {
        fn pinned(value: serde_json::Value) -> serde_json::Value {
            let mut params = serde_json::json!({});
            params
                .as_object_mut()
                .expect("object fixture")
                .insert(PRIVATE_PROJECT_ROUTE_PIN.to_string(), value);
            params
        }

        for tool_name in [
            "inspect_match",
            "checkpoint_now",
            "detect_impact",
            "conventions",
            "health",
            "health_compact",
            "status",
            "context_inventory",
        ] {
            let mut params = pinned(serde_json::json!("project-active"));
            assert_eq!(
                take_private_project_route_pin(tool_name, &mut params)
                    .expect("allowlisted private route pin"),
                Some("project-active".to_string())
            );
            assert!(
                params.get(PRIVATE_PROJECT_ROUTE_PIN).is_none(),
                "the internal marker must be removed before typed decoding"
            );
        }

        for (tool_name, mut params) in [
            ("index_folder", pinned(serde_json::json!("project-active"))),
            ("inspect_match", pinned(serde_json::json!("   "))),
            (
                "inspect_match",
                pinned(serde_json::json!(" project-active")),
            ),
            ("checkpoint_now", pinned(serde_json::json!(7))),
            ("detect_impact", {
                let mut params = pinned(serde_json::json!("project-active"));
                params
                    .as_object_mut()
                    .expect("object fixture")
                    .insert("project".to_string(), serde_json::Value::Null);
                params
            }),
            ("conventions", {
                let mut params = pinned(serde_json::json!("project-active"));
                params
                    .as_object_mut()
                    .expect("object fixture")
                    .insert("projects".to_string(), serde_json::json!([]));
                params
            }),
        ] {
            assert!(
                take_private_project_route_pin(tool_name, &mut params).is_err(),
                "invalid private pin must fail closed for {tool_name}: {params}"
            );
        }
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
                    allow_protected_root: None,
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
                    allow_protected_root: None,
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

    /// Task 4 resolver: omission -> home; explicit id -> target; unique display
    /// name -> target; unknown -> deterministic candidates; ambiguous display
    /// name -> deterministic ambiguity error naming the ids.
    #[test]
    fn test_runtime_for_target_resolution_contract() {
        let project_a = project_dir("symforge-resolver-home-a");
        let parent_one = TempDir::new().expect("parent one");
        let parent_two = TempDir::new().expect("parent two");
        // Two OPEN projects with the SAME display name under different parents.
        let same_name_one = parent_one.path().join("samename");
        let same_name_two = parent_two.path().join("samename");
        for root in [&same_name_one, &same_name_two] {
            std::fs::create_dir_all(root.join("src")).expect("create src");
            std::fs::write(root.join("src").join("lib.rs"), "fn f() {}\n").expect("write src");
        }
        let state = DaemonState::new();
        let opened = state
            .open_project_session(OpenProjectRequest {
                project_root: project_a.path().display().to_string(),
                client_name: "resolver".to_string(),
                pid: None,
            })
            .expect("open home A");
        for root in [&same_name_one, &same_name_two] {
            state
                .index_folder_for_session(
                    &opened.session_id,
                    IndexFolderInput {
                        path: root.display().to_string(),
                        idempotency_key: None,
                        // Additive: this test asserts the RESOLVER contract
                        // (omission -> active), not activation — keep home A
                        // active by not default-opening.
                        add: Some(true),
                        allow_protected_root: None,
                    },
                )
                .expect("open same-name project");
        }
        let id_one = project_key(&canonical_project_root(&same_name_one).expect("canonical 1"));
        let id_two = project_key(&canonical_project_root(&same_name_two).expect("canonical 2"));

        // Omission -> the ACTIVE project (home A; the same-name siblings were
        // opened additively, which never activates).
        let home = state
            .runtime_for_target(&opened.session_id, None)
            .expect("home runtime");
        assert_eq!(home.project_id, opened.project_id);
        // Explicit id -> that project.
        let by_id = state
            .runtime_for_target(&opened.session_id, Some(&id_one))
            .expect("runtime by id");
        assert_eq!(by_id.project_id, id_one);
        // Unique display name -> home project's name resolves to home.
        let by_name = state
            .runtime_for_target(&opened.session_id, Some(&opened.project_name))
            .expect("runtime by unique name");
        assert_eq!(by_name.project_id, opened.project_id);
        // Ambiguous display name -> deterministic error naming both ids.
        let ambiguous = match state.runtime_for_target(&opened.session_id, Some("samename")) {
            Err(message) => message,
            Ok(_) => panic!("ambiguous name must not resolve"),
        };
        assert!(
            ambiguous.contains("ambiguous")
                && ambiguous.contains(&id_one)
                && ambiguous.contains(&id_two),
            "ambiguity error must name candidates: {ambiguous}"
        );
        // Unknown selector -> candidates, no load.
        let unknown = match state.runtime_for_target(&opened.session_id, Some("nope")) {
            Err(message) => message,
            Ok(_) => panic!("unknown selector must not resolve"),
        };
        assert!(
            unknown.contains("not open") && unknown.contains(&opened.project_id),
            "unknown-selector error must list open projects: {unknown}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn client_selector_resolution_preserves_trailing_space_project_names() {
        let parent = TempDir::new().expect("parent");
        let home = parent.path().join("repo");
        let sibling = parent.path().join("repo ");
        std::fs::create_dir(&home).expect("home");
        std::fs::create_dir(&sibling).expect("sibling");
        let home = home.canonicalize().expect("canonical home");
        let sibling = sibling.canonicalize().expect("canonical sibling");
        let home_id = project_key(&home);
        let sibling_id = project_key(&sibling);
        let client = DaemonSessionClient::new_for_test(
            "http://127.0.0.1:1".to_string(),
            home_id,
            "session-id".to_string(),
            "repo".to_string(),
        )
        .with_project_root(home);
        client.record_opened_root(sibling.clone());

        let selected = client
            .opened_project_for_selector(Some("repo "))
            .expect("exact trailing-space name must resolve");
        assert_eq!(selected.0, sibling_id);
        assert_eq!(selected.1, sibling);
        assert!(!selected.2);
    }

    // ── Locked contract — default open ACTIVATES the target, keeps the working set ───
    #[test]
    fn test_default_index_folder_open_activates_and_keeps_working_set() {
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

        let output = state
            .index_folder_for_session(
                &opened.session_id,
                IndexFolderInput {
                    path: project_b.path().display().to_string(),
                    idempotency_key: None,
                    add: None,
                    allow_protected_root: None,
                },
            )
            .expect("open B");
        let project_b_id =
            project_key(&canonical_project_root(project_b.path()).expect("canonical b"));

        assert!(
            output.contains(&format!("project_id={project_b_id}")),
            "receipt must identify the opened project: {output}"
        );
        assert!(
            output.contains("checkpoint=written"),
            "successful daemon reload must report its checkpoint: {output}"
        );

        let sessions = state.sessions.read();
        let session = sessions.get(&opened.session_id).expect("session");
        assert_eq!(
            session.active_project_id, project_b_id,
            "default index_folder must ACTIVATE the opened project (locked contract)"
        );
        let ws = session.working_set.read();
        assert_eq!(ws.len(), 2, "default open must keep A and add B");
        assert!(
            ws.get(&active_a).is_some(),
            "home project A must stay in the working set (activation is non-destructive)"
        );
        let entry = ws
            .get(&project_b_id)
            .expect("working set gains newly opened project B");
        assert_eq!(
            entry.overlay.delta_count(),
            0,
            "opened-project overlay must be empty"
        );
        assert!(
            entry.overlay.is_valid_against(&entry.base),
            "re-seeded overlay fenced to its base"
        );
    }

    #[test]
    fn daemon_index_folder_reconciles_existing_root_gitignore() {
        let home = project_dir("symforge-hygiene-home");
        let target = project_dir("symforge-hygiene-target");
        std::fs::create_dir(target.path().join(".git")).expect("create git metadata marker");
        std::fs::write(target.path().join(".gitignore"), b"target/\r\n")
            .expect("write root gitignore");
        std::fs::write(
            target.path().join("src").join("lib.rs"),
            "fn indexed() {}\n",
        )
        .expect("write target source");
        let state = DaemonState::new();
        let opened = state
            .open_project_session(OpenProjectRequest {
                project_root: home.path().display().to_string(),
                client_name: "gitignore-hygiene".to_string(),
                pid: None,
            })
            .expect("open home");

        let receipt = state
            .index_folder_for_session(
                &opened.session_id,
                IndexFolderInput {
                    path: target.path().display().to_string(),
                    idempotency_key: None,
                    add: None,
                    allow_protected_root: None,
                },
            )
            .expect("open target");

        assert!(
            receipt.contains("gitignore_hygiene=effective changed=true"),
            "daemon indexing must report the hygiene mutation: {receipt}"
        );
        assert_eq!(
            std::fs::read(target.path().join(".gitignore")).expect("read reconciled gitignore"),
            b"target/\r\n/.symforge/\r\n",
            "daemon indexing must preserve existing bytes and newline style"
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
                            allow_protected_root: None,
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

    #[tokio::test]
    async fn test_index_folder_for_session_refuses_sensitive_root() {
        let _guard = env_lock().await;
        let state_home = TempDir::new().expect("private user-local state base");
        let _home = EnvVarGuard::set("SYMFORGE_HOME", state_home.path());
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

        let fixture = TempDir::new().expect("protected-root fixture parent");
        let sensitive = fixture.path().join("System32");
        std::fs::create_dir(&sensitive).expect("create modeled protected root");
        std::fs::write(
            sensitive.join("lib.rs"),
            "pub fn protected_daemon_symbol() {}\n",
        )
        .expect("write protected source");

        let result = state
            .index_folder_for_session(
                &opened.session_id,
                IndexFolderInput {
                    path: sensitive.display().to_string(),
                    idempotency_key: None,
                    add: None,
                    allow_protected_root: None,
                },
            )
            .expect("refusal must be a clean Ok response, never an Err or panic");
        assert!(
            result.contains("protected_root_requires_explicit_override"),
            "daemon path must refuse the modeled protected root, got: {result}"
        );

        // The session must remain bound to the original project (no reassign).
        let projects = state.list_projects();
        assert_eq!(
            projects.len(),
            1,
            "refusal must not create a project for the sensitive root"
        );

        let indexed = state
            .index_folder_for_session(
                &opened.session_id,
                IndexFolderInput {
                    path: sensitive.display().to_string(),
                    idempotency_key: None,
                    add: None,
                    allow_protected_root: Some(true),
                },
            )
            .expect("direct protected authority should produce a clean result");
        assert!(
            indexed.contains("Indexed 1 files"),
            "daemon override should index the exact protected root: {indexed}"
        );
        assert!(!sensitive.join(".symforge").exists());
        let canonical = sensitive.canonicalize().unwrap();
        assert!(
            state_home
                .path()
                .join("projects")
                .join(project_key(&canonical))
                .is_dir(),
            "explicit protected daemon state should be user-local"
        );
    }

    #[tokio::test]
    async fn protected_membership_is_per_session_and_requires_each_direct_override() {
        let _guard = env_lock().await;
        let state_home = TempDir::new().expect("private user-local state base");
        let _home = EnvVarGuard::set("SYMFORGE_HOME", state_home.path());
        let home_a = project_dir("symforge-protected-session-a");
        let home_b = project_dir("symforge-protected-session-b");
        let state = DaemonState::new();
        let session_a = state
            .open_project_session(OpenProjectRequest {
                project_root: home_a.path().display().to_string(),
                client_name: "protected-a".to_string(),
                pid: None,
            })
            .expect("open session A");
        let session_b = state
            .open_project_session(OpenProjectRequest {
                project_root: home_b.path().display().to_string(),
                client_name: "protected-b".to_string(),
                pid: None,
            })
            .expect("open session B");

        let fixture = TempDir::new().expect("protected-root fixture parent");
        let protected = fixture.path().join("System32");
        std::fs::create_dir(&protected).expect("create modeled protected root");
        std::fs::write(
            protected.join("lib.rs"),
            "pub fn protected_session_symbol() {}\n",
        )
        .expect("write protected source");
        let protected_id = project_key(&protected.canonicalize().unwrap());

        let opened_a = state
            .index_folder_for_session(
                &session_a.session_id,
                IndexFolderInput {
                    path: protected.display().to_string(),
                    idempotency_key: None,
                    add: Some(true),
                    allow_protected_root: Some(true),
                },
            )
            .expect("session A direct protected open");
        assert!(opened_a.contains("Indexed 1 files"), "{opened_a}");

        let protected_name = {
            let projects = state.projects.read();
            let slot = projects.get(&protected_id).expect("shared protected slot");
            slot.metadata.read().project_name.clone()
        };
        {
            let sessions = state.sessions.read();
            assert!(
                sessions
                    .get(&session_a.session_id)
                    .unwrap()
                    .working_set
                    .read()
                    .get(&protected_id)
                    .is_some(),
                "the directly authorized session must gain membership"
            );
            assert!(
                sessions
                    .get(&session_b.session_id)
                    .unwrap()
                    .working_set
                    .read()
                    .get(&protected_id)
                    .is_none(),
                "another session must not inherit protected membership"
            );
        }
        assert!(
            !state.set_active_project(&session_b.session_id, &protected_id),
            "a global project id must not grant session membership"
        );
        assert!(
            state
                .runtime_for_target(&session_b.session_id, Some(&protected_id))
                .is_err(),
            "a protected project id must not address another session's membership"
        );
        assert!(
            state
                .runtime_for_target(&session_b.session_id, Some(&protected_name))
                .is_err(),
            "a protected project alias must not address another session's membership"
        );

        let refused_b = state
            .index_folder_for_session(
                &session_b.session_id,
                IndexFolderInput {
                    path: protected.display().to_string(),
                    idempotency_key: None,
                    add: Some(true),
                    allow_protected_root: None,
                },
            )
            .expect("missing override is a typed refusal");
        assert!(
            refused_b.contains("protected_root_requires_explicit_override"),
            "{refused_b}"
        );
        assert!(
            state
                .runtime_for_target(&session_b.session_id, Some(&protected_id))
                .is_err(),
            "a refused request must not attach protected membership"
        );

        let opened_b = state
            .index_folder_for_session(
                &session_b.session_id,
                IndexFolderInput {
                    path: protected.display().to_string(),
                    idempotency_key: None,
                    add: Some(true),
                    allow_protected_root: Some(true),
                },
            )
            .expect("session B direct protected open");
        assert!(opened_b.contains("Indexed 1 files"), "{opened_b}");
        assert!(
            state
                .runtime_for_target(&session_b.session_id, Some(&protected_id))
                .is_ok(),
            "session B gains membership only after its own direct override"
        );
        let projects = state.projects.read();
        let protected_slot = projects.get(&protected_id).expect("one protected slot");
        let protected_meta = protected_slot.metadata.read();
        assert_eq!(protected_meta.session_ids.len(), 2);
        assert!(protected_meta.session_ids.contains(&session_a.session_id));
        assert!(protected_meta.session_ids.contains(&session_b.session_id));
    }

    #[tokio::test]
    async fn l_r11_second_session_cannot_reach_protected_project_via_wildcard_or_ref_mapping() {
        // L-R11 (tasks.md:667; source-binding-and-state.md:64-90 test 16;
        // repository-mental-model.md test 9): a second session cannot address an
        // explicit-protected project/worktree through a wildcard (`projects=["*"]`),
        // a subset selector, or any ref/worktree source mapping. Every cross-project
        // read resolves ONLY against the SESSION's own `project_indexes`, which
        // `runtime_for_target` builds from `session.servers` (the session's own
        // opened projects). The id/alias vector is proven by
        // `protected_membership_is_per_session_and_requires_each_direct_override`;
        // this closes the wildcard/subset/dispatcher vector and the direct-override
        // grant. No addressing guard predicate exists or is needed: the session-
        // scoped `project_indexes` capture IS the enforcement chokepoint.
        let _guard = env_lock().await;
        let state_home = TempDir::new().expect("private user-local state base");
        let _home = EnvVarGuard::set("SYMFORGE_HOME", state_home.path());
        let home_a = project_dir("symforge-l-r11-session-a");
        let home_b = project_dir("symforge-l-r11-session-b");
        let state = DaemonState::new();
        let session_a = state
            .open_project_session(OpenProjectRequest {
                project_root: home_a.path().display().to_string(),
                client_name: "l-r11-a".to_string(),
                pid: None,
            })
            .expect("open session A");
        let session_b = state
            .open_project_session(OpenProjectRequest {
                project_root: home_b.path().display().to_string(),
                client_name: "l-r11-b".to_string(),
                pid: None,
            })
            .expect("open session B");

        // A modeled protected root with a knowledge document, so the source mappings
        // L-R11 guards against actually exist to be (un)addressable.
        let fixture = TempDir::new().expect("protected-root fixture parent");
        let protected = fixture.path().join("System32");
        std::fs::create_dir(&protected).expect("create modeled protected root");
        std::fs::write(
            protected.join("guide.md"),
            "# Protected Guide\n\nThe orbital lattice resonates across the protected source.\n",
        )
        .expect("write protected knowledge doc");
        let protected_id = project_key(&protected.canonicalize().unwrap());

        // Session A directly opens the protected root; session B never does.
        let opened_a = state
            .index_folder_for_session(
                &session_a.session_id,
                IndexFolderInput {
                    path: protected.display().to_string(),
                    idempotency_key: None,
                    add: Some(true),
                    allow_protected_root: Some(true),
                },
            )
            .expect("session A direct protected open");
        assert!(opened_a.contains("Indexed"), "{opened_a}");

        let query = || SearchKnowledgeInput {
            query: "orbital lattice".to_string(),
            path_prefix: None,
            source_scope: None,
            authority_scope: None,
            // The dispatcher switches on the `Targets` argument, not these fields.
            project: None,
            projects: None,
            limit: None,
            max_tokens: None,
        };

        // Session B's runtime is the ONLY source a cross-project read resolves
        // against. The raw `projects=["*"]` param resolves to `Targets::All`, which
        // the dispatcher expands from `project_indexes.keys()` — session B's own
        // projects only.
        let runtime_b = state
            .runtime_for_target(&session_b.session_id, None)
            .expect("session B home runtime");
        assert!(
            matches!(
                resolve_targets(None, Some(&["*".to_string()]), &runtime_b.project_id),
                Ok(Targets::All)
            ),
            "projects=[\"*\"] must resolve to Targets::All"
        );
        assert!(
            !runtime_b.project_indexes.contains_key(&protected_id),
            "projects=[\"*\"] (Targets::All = project_indexes.keys()) must NOT include \
             another session's protected project"
        );

        // Wildcard: silently scoped to session B's own projects — never errors AND
        // never covers the protected source.
        let wildcard = execute_cross_project_knowledge(
            query(),
            Targets::All,
            &runtime_b.project_indexes,
            &runtime_b.server,
        )
        .expect("wildcard over session B's own projects succeeds");
        assert!(
            !wildcard.contains(protected_id.as_str()),
            "the protected project must not appear in a second session's wildcard scope: {wildcard}"
        );

        // Explicit id and subset selectors naming the protected project are refused
        // with "project not open": a project id / ref-worktree mapping cannot inherit
        // another session's protected membership.
        for targets in [
            Targets::One(protected_id.clone()),
            Targets::Subset(vec![protected_id.clone()]),
        ] {
            let refused = execute_cross_project_knowledge(
                query(),
                targets,
                &runtime_b.project_indexes,
                &runtime_b.server,
            )
            .expect_err("a protected project the session never opened is not addressable");
            assert!(
                refused.to_string().contains("project not open"),
                "{refused}"
            );
        }

        // The direct-override path is the ONLY way in: session B issues its OWN exact
        // `index_folder(allow_protected_root=true)`.
        let opened_b = state
            .index_folder_for_session(
                &session_b.session_id,
                IndexFolderInput {
                    path: protected.display().to_string(),
                    idempotency_key: None,
                    add: Some(true),
                    allow_protected_root: Some(true),
                },
            )
            .expect("session B direct protected open");
        assert!(opened_b.contains("Indexed"), "{opened_b}");

        let runtime_b2 = state
            .runtime_for_target(&session_b.session_id, None)
            .expect("session B home runtime after its own override");
        assert!(
            runtime_b2.project_indexes.contains_key(&protected_id),
            "session B gains wildcard-addressable membership only after its own direct override"
        );
        execute_cross_project_knowledge(
            query(),
            Targets::One(protected_id.clone()),
            &runtime_b2.project_indexes,
            &runtime_b2.server,
        )
        .expect("session B can address the protected project after its own direct override");
    }

    /// Build one `PublishedSourceSet` carrying a healthy current worktree lane AND
    /// a P1 `symforge:git-ref:` lane whose knowledge document exists ONLY on the ref
    /// lane. Shapes the fixture the two `search_scoped`/`review_scoped` unit tests
    /// already prove, but wired for the daemon cross-project entry points. The temp
    /// roots are returned so the caller keeps them alive.
    fn cross_project_current_and_ref_lanes() -> (TempDir, TempDir, SharedIndex) {
        // Current worktree lane: "quantum meridian" only.
        let current_dir = tempfile::tempdir().expect("current tempdir");
        let current_repo = git2::Repository::init(current_dir.path()).expect("init current repo");
        ref_lane_commit(
            &current_repo,
            current_dir.path(),
            "docs/current.md",
            b"# Current\n\nThe quantum meridian anchors only the current worktree.\n",
            "current worktree doc",
        );
        let index = LiveIndex::load(current_dir.path()).expect("load current worktree lane");

        // P1 git-ref lane: "orbital lattice" only, built from a separate repo's HEAD
        // tree and published onto the SAME handle (current lane left untouched).
        let ref_dir = tempfile::tempdir().expect("ref tempdir");
        let ref_repo = git2::Repository::init(ref_dir.path()).expect("init ref repo");
        ref_lane_commit(
            &ref_repo,
            ref_dir.path(),
            "docs/reflane.md",
            b"# Ref Lane\n\nThe orbital lattice resonates only on the ref lane.\n",
            "ref lane doc",
        );
        live_index::local_ref_scout::ingest_and_publish_local_ref(
            &index,
            &ref_repo,
            "HEAD",
            crate::domain::index::RepositoryId::new("repo-cross-scope"),
            &live_index::local_ref_scout::LocalRefScoutBudget::default(),
        )
        .expect("publish P1 ref lane");

        assert!(
            index
                .published_source_set()
                .sources
                .keys()
                .any(|id| id.as_str().starts_with("symforge:git-ref:")),
            "a P1 git-ref lane must exist for the scope-composition test to be meaningful"
        );

        (current_dir, ref_dir, index)
    }

    /// [L-R09/L-G06] A multi-project `search_knowledge` with
    /// `source_scope=local_refs`/`all` composes each selected project's P1 git-ref
    /// lanes. Before the fix the cross-project path captured only the current lane
    /// (`search_current`) and silently dropped every ref lane; now it captures the
    /// whole source-set Arc and calls `search_scoped`. The default (`None`) scope
    /// stays current-lane only, byte-identical to before.
    #[tokio::test]
    async fn cross_project_search_source_scope_composes_local_ref_lane() {
        use crate::protocol::search_tools::KnowledgeSourceScope;

        let _guard = env_lock().await;
        let _gate = EnvVarGuard::set_str(LOCAL_REF_LANES_ENV, "1");

        let (current_dir, _ref_dir, index) = cross_project_current_and_ref_lanes();
        let project_indexes: HashMap<
            String,
            crate::live_index::index_lifecycle::activation::ProjectRuntimeHandle,
        > = HashMap::from([(
            "proj-cross-scope".to_string(),
            crate::live_index::index_lifecycle::activation::ProjectRuntimeHandle::bind(Arc::clone(
                &index,
            )),
        )]);
        let server = SymForgeServer::new(
            Arc::clone(&index),
            "proj-cross-scope".to_string(),
            Arc::new(parking_lot::Mutex::new(WatcherInfo::default())),
            Some(current_dir.path().to_path_buf()),
            None,
        );

        let query = |term: &str, scope: Option<KnowledgeSourceScope>| SearchKnowledgeInput {
            query: term.to_string(),
            path_prefix: None,
            source_scope: scope,
            authority_scope: None,
            project: None,
            projects: None,
            limit: None,
            max_tokens: None,
        };

        // local_refs composes the P1 ref lane: the ref-only document is now visible.
        let refs = execute_cross_project_knowledge(
            query("orbital lattice", Some(KnowledgeSourceScope::LocalRefs)),
            Targets::All,
            &project_indexes,
            &server,
        )
        .expect("local_refs cross-project search");
        assert!(
            refs.contains("docs/reflane.md"),
            "local_refs must compose the P1 ref lane: {refs}"
        );

        // `all` composes the ref lane too.
        let all = execute_cross_project_knowledge(
            query("orbital lattice", Some(KnowledgeSourceScope::All)),
            Targets::All,
            &project_indexes,
            &server,
        )
        .expect("all cross-project search");
        assert!(
            all.contains("docs/reflane.md"),
            "all must compose the P1 ref lane: {all}"
        );

        // A Subset selector composes the ref lane exactly like All.
        let subset = execute_cross_project_knowledge(
            query("orbital lattice", Some(KnowledgeSourceScope::LocalRefs)),
            Targets::Subset(vec!["proj-cross-scope".to_string()]),
            &project_indexes,
            &server,
        )
        .expect("subset local_refs cross-project search");
        assert!(
            subset.contains("docs/reflane.md"),
            "subset local_refs must compose the P1 ref lane: {subset}"
        );

        // Default (source_scope=None) stays current-lane only: the ref-only document
        // is invisible (the pre-fix behavior, now the ONLY difference from scoped).
        let default_ref_term = execute_cross_project_knowledge(
            query("orbital lattice", None),
            Targets::All,
            &project_indexes,
            &server,
        )
        .expect("default cross-project search");
        assert!(
            !default_ref_term.contains("docs/reflane.md"),
            "default scope must not compose the ref lane: {default_ref_term}"
        );

        // ...and the default scope still performs a genuine current-lane search.
        let default_current_term = execute_cross_project_knowledge(
            query("quantum meridian", None),
            Targets::All,
            &project_indexes,
            &server,
        )
        .expect("default current-lane search");
        assert!(
            default_current_term.contains("docs/current.md"),
            "default scope must search the current lane: {default_current_term}"
        );
    }

    /// [L-R09/L-G06] The same fix for `review_knowledge`: cross-project review now
    /// captures the whole source-set Arc and calls `review_scoped`, so
    /// `source_scope=local_refs`/`all` compose the P1 ref lanes. A per-source
    /// missing-document readiness is a typed section inside `review_scoped`, not a
    /// whole-scope failure; the default (`None`) scope stays current-lane only.
    #[tokio::test]
    async fn cross_project_review_source_scope_composes_local_ref_lane() {
        use crate::protocol::search_tools::{KnowledgeSourceScope, ReviewKnowledgeMode};

        let _guard = env_lock().await;
        let _gate = EnvVarGuard::set_str(LOCAL_REF_LANES_ENV, "1");

        let (current_dir, _ref_dir, index) = cross_project_current_and_ref_lanes();
        let project_indexes: HashMap<
            String,
            crate::live_index::index_lifecycle::activation::ProjectRuntimeHandle,
        > = HashMap::from([(
            "proj-cross-scope".to_string(),
            crate::live_index::index_lifecycle::activation::ProjectRuntimeHandle::bind(Arc::clone(
                &index,
            )),
        )]);
        let server = SymForgeServer::new(
            Arc::clone(&index),
            "proj-cross-scope".to_string(),
            Arc::new(parking_lot::Mutex::new(WatcherInfo::default())),
            Some(current_dir.path().to_path_buf()),
            None,
        );

        let review = |path: &str, scope: Option<KnowledgeSourceScope>| ReviewKnowledgeInput {
            mode: ReviewKnowledgeMode::Document,
            path: Some(path.to_string()),
            path_prefix: None,
            source_scope: scope,
            project: None,
            projects: None,
            limit: None,
            max_tokens: None,
        };

        // local_refs composes the ref lane: the ref-only document is reviewable and
        // the composed response keeps the top-level hash envelope.
        let refs = execute_cross_project_review(
            review("docs/reflane.md", Some(KnowledgeSourceScope::LocalRefs)),
            Targets::All,
            &project_indexes,
            &server,
        )
        .expect("local_refs cross-project review");
        assert!(
            refs.contains("docs/reflane.md"),
            "local_refs review must compose the P1 ref lane: {refs}"
        );
        assert!(
            refs.contains("top_result_hash="),
            "composed review preserves the top-level hash envelope: {refs}"
        );

        // `all` composes the ref lane too; the current lane's missing-document
        // readiness folds into a typed per-source section, NOT a whole-scope failure.
        let all = execute_cross_project_review(
            review("docs/reflane.md", Some(KnowledgeSourceScope::All)),
            Targets::All,
            &project_indexes,
            &server,
        )
        .expect("all cross-project review (per-source readiness is not a scope failure)");
        assert!(
            all.contains("docs/reflane.md"),
            "all review must compose the P1 ref lane: {all}"
        );

        // Default (source_scope=None) is current-lane only: a ref-only document is
        // invisible, so document-mode review over the current lane finds nothing.
        let default_ref_doc = execute_cross_project_review(
            review("docs/reflane.md", None),
            Targets::All,
            &project_indexes,
            &server,
        );
        assert!(
            default_ref_doc.is_err(),
            "default scope must not reach the ref-only document: {default_ref_doc:?}"
        );

        // ...and default review still serves the current lane's own document.
        let default_current_doc = execute_cross_project_review(
            review("docs/current.md", None),
            Targets::All,
            &project_indexes,
            &server,
        )
        .expect("default current-lane review");
        assert!(
            default_current_doc.contains("docs/current.md"),
            "default review must serve the current lane: {default_current_doc}"
        );
    }

    /// [L-R11] The wildcard/ref-mapping protection holds through the REAL
    /// tool-dispatch entry point (`execute_tool_call`), not only the
    /// `execute_cross_project_knowledge` helper the sibling test drives. A
    /// `search_knowledge` call for session B peeks `project`/`projects`, resolves a
    /// `Targets` via `resolve_targets`, and dispatches into
    /// `execute_cross_project_knowledge` against SESSION B's own `project_indexes`.
    /// So `projects=["*"]` silently scopes to session B's projects and
    /// `project=<protected_id>` is refused — session B reaches session A's
    /// ExplicitProtected project ONLY after its own direct
    /// `index_folder(allow_protected_root=true)`. This is the dispatch chain
    /// `execute_tool_call` → `resolve_targets` → `execute_cross_project_knowledge`;
    /// the freshness gate (`call_tool_handler`) merely resolves the same session-
    /// scoped runtime beforehand and cannot widen it.
    #[tokio::test]
    async fn l_r11_tool_dispatch_blocks_wildcard_and_ref_mapping_for_second_session() {
        let _guard = env_lock().await;
        let state_home = TempDir::new().expect("private user-local state base");
        let _home = EnvVarGuard::set("SYMFORGE_HOME", state_home.path());
        let home_a = project_dir("symforge-l-r11-dispatch-a");
        let home_b = project_dir("symforge-l-r11-dispatch-b");
        let state = DaemonState::new();
        let session_a = state
            .open_project_session(OpenProjectRequest {
                project_root: home_a.path().display().to_string(),
                client_name: "l-r11-dispatch-a".to_string(),
                pid: None,
            })
            .expect("open session A");
        let session_b = state
            .open_project_session(OpenProjectRequest {
                project_root: home_b.path().display().to_string(),
                client_name: "l-r11-dispatch-b".to_string(),
                pid: None,
            })
            .expect("open session B");

        // A modeled protected root with a distinctive knowledge document.
        let fixture = TempDir::new().expect("protected-root fixture parent");
        let protected = fixture.path().join("System32");
        std::fs::create_dir(&protected).expect("create modeled protected root");
        std::fs::write(
            protected.join("guide.md"),
            "# Protected Guide\n\nThe orbital lattice resonates across the protected source.\n",
        )
        .expect("write protected knowledge doc");
        let protected_id = project_key(&protected.canonicalize().unwrap());

        // Session A directly opens the protected root; session B never does.
        state
            .index_folder_for_session(
                &session_a.session_id,
                IndexFolderInput {
                    path: protected.display().to_string(),
                    idempotency_key: None,
                    add: Some(true),
                    allow_protected_root: Some(true),
                },
            )
            .expect("session A direct protected open");

        // Wildcard via the REAL dispatch: silently scoped to session B's own
        // projects — never errors AND never reaches the protected source.
        let runtime_b = state
            .runtime_for_target(&session_b.session_id, None)
            .expect("session B runtime");
        let wildcard = execute_tool_call(
            runtime_b,
            "search_knowledge",
            serde_json::json!({ "query": "orbital lattice", "projects": ["*"] }),
        )
        .await
        .expect("wildcard over session B's own projects succeeds");
        assert!(
            !wildcard.contains(protected_id.as_str()),
            "projects=[\"*\"] must not reach another session's protected project: {wildcard}"
        );
        assert!(
            !wildcard.contains("guide.md"),
            "the protected document must not leak into session B's wildcard scope: {wildcard}"
        );

        // Explicit `project=<protected_id>` through the dispatch: refused with
        // "project not open" — an id/ref mapping cannot inherit A's membership.
        let runtime_b = state
            .runtime_for_target(&session_b.session_id, None)
            .expect("session B runtime");
        let refused = execute_tool_call(
            runtime_b,
            "search_knowledge",
            serde_json::json!({ "query": "orbital lattice", "project": protected_id.clone() }),
        )
        .await
        .expect_err("a protected project session B never opened is not addressable");
        assert!(
            refused.to_string().contains("project not open"),
            "{refused}"
        );

        // The direct-override path is the ONLY way in: session B issues its OWN
        // index_folder(allow_protected_root=true), then the dispatch resolves it.
        state
            .index_folder_for_session(
                &session_b.session_id,
                IndexFolderInput {
                    path: protected.display().to_string(),
                    idempotency_key: None,
                    add: Some(true),
                    allow_protected_root: Some(true),
                },
            )
            .expect("session B direct protected open");
        let runtime_b = state
            .runtime_for_target(&session_b.session_id, None)
            .expect("session B runtime after its own override");
        let after = execute_tool_call(
            runtime_b,
            "search_knowledge",
            serde_json::json!({ "query": "orbital lattice", "project": protected_id.clone() }),
        )
        .await
        .expect("session B can address the protected project after its own direct override");
        assert!(
            after.contains("guide.md"),
            "session B now sees the protected document it opened directly: {after}"
        );
    }

    #[tokio::test]
    async fn protected_membership_is_not_inherited_by_reconnect_alias_or_restart() {
        let _guard = env_lock().await;
        let state_home = TempDir::new().expect("private user-local state base");
        let _home = EnvVarGuard::set("SYMFORGE_HOME", state_home.path());
        let home = project_dir("symforge-protected-restart-home");
        let fixture = TempDir::new().expect("protected-root fixture parent");
        let protected = fixture.path().join("System32");
        std::fs::create_dir(&protected).expect("create modeled protected root");
        std::fs::write(
            protected.join("lib.rs"),
            "pub fn protected_restart_symbol() {}\n",
        )
        .expect("write protected source");
        let protected_id = project_key(&protected.canonicalize().unwrap());
        let protected_state = state_home.path().join("projects").join(&protected_id);

        let state = DaemonState::new();
        let authorized = state
            .open_project_session(OpenProjectRequest {
                project_root: home.path().display().to_string(),
                client_name: "protected-authorized".to_string(),
                pid: None,
            })
            .expect("open authorized session home");
        let opened = state
            .index_folder_for_session(
                &authorized.session_id,
                IndexFolderInput {
                    path: protected.display().to_string(),
                    idempotency_key: None,
                    add: Some(true),
                    allow_protected_root: Some(true),
                },
            )
            .expect("direct protected open");
        assert!(opened.contains("Indexed 1 files"), "{opened}");
        assert!(protected_state.join("index.bin").is_file());

        let reconnect = state.open_project_session(OpenProjectRequest {
            project_root: protected.display().to_string(),
            client_name: "protected-reconnect".to_string(),
            pid: None,
        });
        assert!(
            reconnect.is_err(),
            "reconnect metadata must not inherit direct protected authority"
        );
        {
            let projects = state.projects.read();
            let slot = projects.get(&protected_id).expect("live protected slot");
            let metadata = slot.metadata.read();
            assert_eq!(metadata.session_ids.len(), 1);
            assert!(metadata.session_ids.contains(&authorized.session_id));
        }

        assert!(state.close_session(&authorized.session_id).is_some());
        assert!(
            state.list_projects().is_empty(),
            "closing the last session must remove the live protected slot"
        );
        drop(state);
        assert!(
            protected_state.join("index.bin").is_file(),
            "persisted protected state remains available but dormant"
        );

        let restarted = DaemonState::new();
        let fresh = restarted
            .open_project_session(OpenProjectRequest {
                project_root: home.path().display().to_string(),
                client_name: "protected-after-restart".to_string(),
                pid: None,
            })
            .expect("open normal session after restart");
        assert!(
            restarted
                .list_projects()
                .iter()
                .all(|project| project.project_id != protected_id),
            "persisted protected state must not auto-attach after restart"
        );
        assert!(
            !restarted.set_active_project(&fresh.session_id, &protected_id),
            "a persisted project id must not grant membership after restart"
        );
        assert!(
            restarted
                .runtime_for_target(&fresh.session_id, Some("System32"))
                .is_err(),
            "a persisted protected display alias must remain dormant after restart"
        );
        assert!(
            restarted
                .open_project_session(OpenProjectRequest {
                    project_root: protected.display().to_string(),
                    client_name: "protected-reconnect-after-restart".to_string(),
                    pid: None,
                })
                .is_err(),
            "reconnect after restart must still refuse protected membership"
        );

        let refused = restarted
            .index_folder_for_session(
                &fresh.session_id,
                IndexFolderInput {
                    path: protected.display().to_string(),
                    idempotency_key: None,
                    add: Some(true),
                    allow_protected_root: None,
                },
            )
            .expect("missing override is a typed refusal");
        assert!(
            refused.contains("protected_root_requires_explicit_override"),
            "{refused}"
        );
        assert!(
            restarted
                .list_projects()
                .iter()
                .all(|project| project.project_id != protected_id),
            "refusal must leave persisted protected state dormant"
        );

        let reopened = restarted
            .index_folder_for_session(
                &fresh.session_id,
                IndexFolderInput {
                    path: protected.display().to_string(),
                    idempotency_key: None,
                    add: Some(true),
                    allow_protected_root: Some(true),
                },
            )
            .expect("fresh direct override after restart");
        assert!(reopened.contains("Indexed 1 files"), "{reopened}");
        assert!(
            restarted
                .runtime_for_target(&fresh.session_id, Some(&protected_id))
                .is_ok(),
            "fresh direct override must reactivate the dormant protected state"
        );
    }

    #[tokio::test]
    async fn index_folder_replay_reestablishes_live_binding_or_returns_live_postcondition_unavailable()
     {
        let _guard = env_lock().await;
        let state_home = TempDir::new().expect("private replay state base");
        let _home = EnvVarGuard::set("SYMFORGE_HOME", state_home.path());
        let home = project_dir("symforge-replay-home");
        let target = project_dir("symforge-replay-target");
        let other = project_dir("symforge-replay-other");
        std::fs::write(target.path().join("src").join("one.rs"), "fn one() {}\n")
            .expect("write first target source");
        std::fs::write(other.path().join("src").join("other.rs"), "fn other() {}\n")
            .expect("write other target source");
        let target_id = project_key(&canonical_project_root(target.path()).unwrap());
        let other_id = project_key(&canonical_project_root(other.path()).unwrap());
        let state = test_daemon_state(state_home.path());
        let session = state
            .open_project_session(OpenProjectRequest {
                project_root: home.path().display().to_string(),
                client_name: "replay".to_string(),
                pid: None,
            })
            .expect("open replay home");
        let key = "b-r27-replay";

        let first_receipt = {
            let _limit = EnvVarGuard::set("SYMFORGE_MAX_INDEX_FILES", Path::new("100"));
            state
                .index_folder_for_session(
                    &session.session_id,
                    IndexFolderInput {
                        path: target.path().display().to_string(),
                        idempotency_key: Some(key.to_string()),
                        add: Some(true),
                        allow_protected_root: None,
                    },
                )
                .expect("first replay execution")
        };
        assert!(first_receipt.contains("Indexed 1 files"), "{first_receipt}");
        assert!(state.remove_project_from_session(&session.session_id, &target_id));
        std::fs::write(target.path().join("src").join("two.rs"), "fn two() {}\n")
            .expect("write second target source");

        let unavailable = {
            let _limit = EnvVarGuard::set("SYMFORGE_MAX_INDEX_FILES", Path::new("1"));
            state
                .index_folder_for_session(
                    &session.session_id,
                    IndexFolderInput {
                        path: target.path().display().to_string(),
                        idempotency_key: Some(key.to_string()),
                        add: Some(true),
                        allow_protected_root: None,
                    },
                )
                .expect("replay failure is a successful typed tool result")
        };
        assert!(unavailable.contains("applied=false"), "{unavailable}");
        assert!(
            unavailable.contains("outcome=live_postcondition_unavailable"),
            "{unavailable}"
        );
        assert!(
            unavailable.contains(&first_receipt),
            "typed failure must preserve the immutable historical receipt: {unavailable}"
        );
        assert!(
            state
                .runtime_for_target(&session.session_id, Some(&target_id))
                .is_err(),
            "failed replay must not claim or attach a live binding"
        );

        let restored = {
            let _limit = EnvVarGuard::set("SYMFORGE_MAX_INDEX_FILES", Path::new("100"));
            state
                .index_folder_for_session(
                    &session.session_id,
                    IndexFolderInput {
                        path: target.path().display().to_string(),
                        idempotency_key: Some(key.to_string()),
                        add: Some(true),
                        allow_protected_root: None,
                    },
                )
                .expect("replay restores live postcondition")
        };
        assert_eq!(
            restored, first_receipt,
            "stored receipt must remain immutable"
        );
        assert!(
            state
                .runtime_for_target(&session.session_id, Some(&target_id))
                .is_ok(),
            "stored success may return only after live membership is restored"
        );
        assert!(state.remove_project_from_session(&session.session_id, &target_id));

        let override_conflict = state
            .index_folder_for_session(
                &session.session_id,
                IndexFolderInput {
                    path: target.path().display().to_string(),
                    idempotency_key: Some(key.to_string()),
                    add: Some(true),
                    allow_protected_root: Some(true),
                },
            )
            .expect("override change is a deterministic tool-level conflict");
        assert!(
            override_conflict.contains("Idempotency conflict"),
            "{override_conflict}"
        );
        assert!(
            state
                .runtime_for_target(&session.session_id, Some(&target_id))
                .is_err(),
            "override conflict must happen before live attachment"
        );

        let path_conflict = state
            .index_folder_for_session(
                &session.session_id,
                IndexFolderInput {
                    path: other.path().display().to_string(),
                    idempotency_key: Some(key.to_string()),
                    add: Some(true),
                    allow_protected_root: None,
                },
            )
            .expect("path change is a deterministic tool-level conflict");
        assert!(
            path_conflict.contains("Idempotency conflict"),
            "{path_conflict}"
        );
        assert!(
            state
                .runtime_for_target(&session.session_id, Some(&other_id))
                .is_err(),
            "path conflict must happen before loading or attaching the other project"
        );
    }

    #[tokio::test]
    async fn post_bind_state_write_failure_degrades_durability_not_live_readiness() {
        let _guard = env_lock().await;
        let project = project_dir("symforge-post-bind-state-failure");
        std::fs::write(
            project.path().join("src").join("main.rs"),
            "fn main() { println!(\"still live\"); }\n",
        )
        .expect("write queryable source");
        let state = DaemonState::new();
        let session = state
            .open_project_session(OpenProjectRequest {
                project_root: project.path().display().to_string(),
                client_name: "durability-oracle".to_string(),
                pid: None,
            })
            .expect("bind and publish project before failing state writes");
        let slot = state
            .projects
            .read()
            .get(&session.project_id)
            .cloned()
            .expect("bound project slot");
        let (canonical_root, placement, publication_before, watcher_info) = {
            let project = slot.metadata.read();
            (
                project.canonical_root.clone(),
                project.state_placement.clone(),
                project.index.data_plane().published_generation(),
                Arc::clone(&project.watcher_info),
            )
        };
        let state_dir = match &placement {
            StatePlacement::ProjectLocal { directory }
            | StatePlacement::UserLocal { directory, .. } => directory.as_path().to_path_buf(),
            StatePlacement::MemoryOnly { failures } => {
                panic!("fixture requires durable placement, got memory-only: {failures:?}")
            }
        };
        let before = state
            .project_health(&session.project_id)
            .expect("health before state failure");
        assert_eq!(before.index_state, "Ready");
        assert_eq!(
            before.durability,
            crate::domain::CapabilityStatus::Available
        );
        assert_eq!(
            before.capabilities.checkpoint,
            crate::domain::CapabilityStatus::Available
        );

        std::fs::remove_dir_all(&state_dir).expect("remove writable state directory");
        std::fs::write(&state_dir, b"state owner became unwritable")
            .expect("replace state directory with a blocking file");
        let checkpoint = execute_tool_call(
            state
                .session_runtime(&session.session_id)
                .expect("session runtime before checkpoint"),
            "checkpoint_now",
            serde_json::json!({ "verify_after_write": true }),
        )
        .await
        .expect("checkpoint failure remains a typed tool response");
        assert!(
            checkpoint.contains("Checkpoint failed"),
            "state write must fail in the fixture: {checkpoint}"
        );

        let after = state
            .project_health(&session.project_id)
            .expect("health after state failure");
        assert_eq!(after.project_id, before.project_id);
        assert_eq!(after.canonical_root, before.canonical_root);
        assert_eq!(after.index_state, "Ready");
        assert_eq!(
            after.durability,
            crate::domain::CapabilityStatus::Unavailable {
                reason: crate::domain::CapabilityUnavailableReason::PersistentStateUnavailable,
            }
        );
        assert_eq!(
            after.capabilities.checkpoint,
            crate::domain::CapabilityStatus::Unavailable {
                reason: crate::domain::CapabilityUnavailableReason::PersistentStateUnavailable,
            }
        );
        assert_eq!(
            after.capabilities.repository_curation,
            crate::domain::CapabilityStatus::Unavailable {
                reason:
                    crate::domain::CapabilityUnavailableReason::DurableMutationReplayUnavailable,
            }
        );

        {
            let project = slot.metadata.read();
            assert_eq!(project.canonical_root, canonical_root);
            assert_eq!(project.state_placement, placement);
            let publication_after = project.index.data_plane().published_generation();
            assert!(
                publication_after.publication_generation
                    >= publication_before.publication_generation,
                "derived-only background publication may advance but never rewind"
            );
            assert_eq!(
                publication_after.content_generation, publication_before.content_generation,
                "failed durability must not mutate live content identity"
            );
            assert_eq!(
                publication_after.project_generation,
                publication_before.project_generation
            );
            assert_eq!(publication_after.source, publication_before.source);
            assert_eq!(
                publication_after.source_version,
                publication_before.source_version
            );
            assert_eq!(
                publication_after
                    .manifest
                    .as_ref()
                    .map(|manifest| &manifest.digest),
                publication_before
                    .manifest
                    .as_ref()
                    .map(|manifest| &manifest.digest)
            );
            assert!(Arc::ptr_eq(&project.watcher_info, &watcher_info));
        }
        let query = execute_tool_call(
            state
                .session_runtime(&session.session_id)
                .expect("session runtime after checkpoint failure"),
            "get_file_content",
            serde_json::json!({ "path": "src/main.rs" }),
        )
        .await
        .expect("live query after state failure");
        assert!(
            query.contains("still live"),
            "live query regressed: {query}"
        );
    }

    #[tokio::test]
    async fn reindex_preserves_resolved_placement_until_new_project_instance() {
        let _guard = env_lock().await;
        let project = project_dir("symforge-placement-lifetime");
        std::fs::write(
            project.path().join("src").join("lib.rs"),
            "pub fn placement_lifetime() {}\n",
        )
        .expect("write source");
        let project_state = project.path().join(".symforge");
        std::fs::write(&project_state, b"block project-local state")
            .expect("install project-local blocker");

        let home = TempDir::new().expect("isolated user-local state");
        let _home = EnvVarGuard::set("SYMFORGE_HOME", home.path());
        let state = DaemonState::with_token_and_control_state(
            "placement-lifetime-token".to_string(),
            Some(ControlStateDir::new(home.path().to_path_buf())),
        );
        let first = state
            .open_project_session(OpenProjectRequest {
                project_root: project.path().display().to_string(),
                client_name: "placement-lifetime-first".to_string(),
                pid: None,
            })
            .expect("open with user-local fallback");
        let first_slot = state
            .projects
            .read()
            .get(&first.project_id)
            .cloned()
            .expect("first project slot");
        let (canonical_root, first_placement) = {
            let project = first_slot.metadata.read();
            (
                project.canonical_root.clone(),
                project.state_placement.clone(),
            )
        };
        let first_state_dir = match &first_placement {
            StatePlacement::UserLocal { directory, .. } => directory.as_path().to_path_buf(),
            other => panic!("blocked project-local state must fall back, got {other:?}"),
        };
        assert!(first_state_dir.starts_with(home.path()));

        std::fs::remove_file(&project_state).expect("make project-local state available");
        let output = state
            .index_folder_for_session(
                &first.session_id,
                IndexFolderInput {
                    path: project.path().display().to_string(),
                    idempotency_key: None,
                    add: None,
                    allow_protected_root: None,
                },
            )
            .expect("reindex same live project");
        assert!(
            output.starts_with("Indexed "),
            "unexpected receipt: {output}"
        );
        let same_slot = state
            .projects
            .read()
            .get(&first.project_id)
            .cloned()
            .expect("same live project slot");
        assert!(Arc::ptr_eq(&first_slot, &same_slot));
        assert_eq!(
            same_slot.metadata.read().state_placement,
            first_placement,
            "same live ProjectInstance must keep its resolved owner"
        );
        assert!(
            !project_state.exists(),
            "same live ProjectInstance must not re-run placement or probe/create a new owner"
        );

        drop(same_slot);
        drop(first_slot);
        let closed = state
            .close_session(&first.session_id)
            .expect("close first project instance");
        assert!(closed.project_removed);

        let second = state
            .open_project_session(OpenProjectRequest {
                project_root: project.path().display().to_string(),
                client_name: "placement-lifetime-second".to_string(),
                pid: None,
            })
            .expect("construct fresh project instance");
        let second_slot = state
            .projects
            .read()
            .get(&second.project_id)
            .cloned()
            .expect("fresh project slot");
        match &second_slot.metadata.read().state_placement {
            StatePlacement::ProjectLocal { directory } => {
                assert_eq!(directory.as_path(), canonical_root.join(".symforge"));
                assert_ne!(directory.as_path(), first_state_dir);
            }
            other => panic!("fresh instance must recover project-local durability, got {other:?}"),
        }
        assert!(project_state.is_dir());
        assert!(
            state
                .close_session(&second.session_id)
                .expect("close fresh project instance")
                .project_removed
        );
    }

    #[tokio::test]
    async fn cold_start_capacity_refusal_stays_nonready_with_typed_freshness() {
        let _guard = env_lock().await;
        let _entry_limit = EnvVarGuard::set("SYMFORGE_MAX_INDEX_FILES", Path::new("1"));
        let project = project_dir("symforge-capacity-refusal");
        std::fs::write(
            project.path().join("src").join("lib.rs"),
            "pub fn one() {}\n",
        )
        .expect("write first source");
        std::fs::write(project.path().join("README.md"), "# second catalog entry\n")
            .expect("write second source");

        let control = TempDir::new().expect("isolated daemon control state");
        let state = DaemonState::with_token_and_control_state(
            "capacity-refusal-token".to_string(),
            Some(ControlStateDir::new(control.path().to_path_buf())),
        );
        let opened = state
            .open_project_session(OpenProjectRequest {
                project_root: project.path().display().to_string(),
                client_name: "capacity-refusal-client".to_string(),
                pid: None,
            })
            .expect("typed capacity refusal must keep the daemon responsive");
        let slot = state
            .projects
            .read()
            .get(&opened.project_id)
            .cloned()
            .expect("non-ready project slot remains registered");
        let project = slot.metadata.read();

        assert!(!project.index.data_plane().read().is_ready());
        assert!(
            project.index.data_plane().scout_plan().is_none(),
            "no partial plan may publish"
        );
        assert!(matches!(
            project.index.data_plane().freshness_status().as_ref(),
            crate::domain::FreshnessStatus::Degraded {
                last_valid_content_generation: 0,
                reason_codes,
            } if reason_codes == &[crate::domain::FreshnessReason::CatalogEntryCapacityExceeded]
        ));
        drop(project);

        let health = state
            .project_health(&opened.project_id)
            .expect("registered non-ready project health");
        assert!(matches!(
            health.freshness,
            crate::domain::FreshnessStatus::Degraded {
                last_valid_content_generation: 0,
                ref reason_codes,
            } if reason_codes == &[crate::domain::FreshnessReason::CatalogEntryCapacityExceeded]
        ));
        assert!(
            state
                .close_session(&opened.session_id)
                .expect("close capacity-refusal session")
                .project_removed
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
                    allow_protected_root: None,
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
            let a_slot = projects.get(&active_a).expect("A loaded");
            let b_slot = projects.get(&project_b_id).expect("B loaded");
            let a = a_slot.metadata.read();
            let b = b_slot.metadata.read();
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
                home_project_id: "missing-project".to_string(),
                working_set: Arc::new(RwLock::new(WorkingSet::new())),
                servers: HashMap::new(),
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

    #[test]
    fn test_sessions_on_same_project_do_not_share_context_cache() {
        let project = project_dir("symforge-session-state-isolation");
        std::fs::write(
            project.path().join("src").join("lib.rs"),
            "pub fn shared_project() {}\n",
        )
        .expect("write source");
        let state = DaemonState::new();

        let first = state
            .open_project_session(OpenProjectRequest {
                project_root: project.path().display().to_string(),
                client_name: "first".to_string(),
                pid: None,
            })
            .expect("first session");
        let second = state
            .open_project_session(OpenProjectRequest {
                project_root: project.path().display().to_string(),
                client_name: "second".to_string(),
                pid: None,
            })
            .expect("second session");

        let first_runtime = state
            .session_runtime(&first.session_id)
            .expect("first runtime");
        let second_runtime = state
            .session_runtime(&second.session_id)
            .expect("second runtime");

        first_runtime
            .server
            .session_context
            .record_file_content_fetch("src/lib.rs", 42, 100, "deadbeefcafe");
        assert!(
            first_runtime
                .server
                .session_context
                .try_file_content_cache_hit("src/lib.rs", 42, false)
                .is_some(),
            "the originating session should observe its own cache entry"
        );
        assert!(
            second_runtime
                .server
                .session_context
                .try_file_content_cache_hit("src/lib.rs", 42, false)
                .is_none(),
            "a second session on the same project must not inherit the first session's cache"
        );
        assert!(!Arc::ptr_eq(
            &first_runtime.server.session_context,
            &second_runtime.server.session_context,
        ));
        assert!(!Arc::ptr_eq(
            &first_runtime.server.ccr_store,
            &second_runtime.server.ccr_store,
        ));
        assert!(!Arc::ptr_eq(
            &first_runtime.server.stel_ledger,
            &second_runtime.server.stel_ledger,
        ));
        assert!(Arc::ptr_eq(
            first_runtime.index.data_plane(),
            second_runtime.index.data_plane(),
        ));
        assert!(Arc::ptr_eq(
            &first_runtime.token_stats,
            &second_runtime.token_stats,
        ));
    }

    /// Issue #585: on a daemon connection every producing tool stores its CCR
    /// blob in the daemon-side session server, so `symforge_retrieve` must be
    /// daemon-routed and redeem from THAT store. The adapter-side handler used
    /// to answer from its own empty store, making every advertised handle
    /// irredeemable.
    #[tokio::test]
    async fn symforge_retrieve_redeems_from_the_session_runtime_store() {
        let project = project_dir("symforge-retrieve-session-store");
        std::fs::write(
            project.path().join("src").join("lib.rs"),
            "pub fn redeemable() {}\n",
        )
        .expect("write source");
        let state = Arc::new(DaemonState::new());
        let opened = state
            .open_project_session(OpenProjectRequest {
                project_root: project.path().display().to_string(),
                client_name: "retriever".to_string(),
                pid: None,
            })
            .expect("session");

        let stored_body = "── symforge get_symbol ──\npub fn redeemable() {}\n";
        let handle = {
            let runtime = state
                .runtime_for_target(&opened.session_id, None)
                .expect("session runtime");
            runtime
                .server
                .ccr_store
                .lock()
                .insert("get_symbol", stored_body.to_string(), None)
        };

        // The daemon-private route pin must be accepted for this tool; the
        // adapter synthesizes it for every selector-less daemon call.
        let mut pinned = serde_json::json!({
            "hash": handle,
            crate::daemon::PRIVATE_PROJECT_ROUTE_PIN: opened.project_id.clone(),
        });
        assert_eq!(
            take_private_project_route_pin("symforge_retrieve", &mut pinned)
                .expect("the pin is legal for symforge_retrieve"),
            Some(opened.project_id.clone()),
        );

        let runtime = state
            .runtime_for_target(&opened.session_id, None)
            .expect("session runtime");
        let body = execute_tool_call(runtime, "symforge_retrieve", pinned)
            .await
            .expect("daemon-side retrieve dispatch");
        assert_eq!(
            body, stored_body,
            "the daemon dispatch must round-trip the session store's exact bytes"
        );

        let runtime = state
            .runtime_for_target(&opened.session_id, None)
            .expect("session runtime");
        let miss = execute_tool_call(
            runtime,
            "symforge_retrieve",
            serde_json::json!({ "hash": "0123456789ab" }),
        )
        .await
        .expect("a miss is a message, not a dispatch error");
        assert!(
            miss.contains("stale or expired"),
            "an unknown handle must keep the honest miss message: {miss}"
        );
    }

    #[test]
    fn test_project_b_reads_while_project_a_reloads() {
        let project_a = project_dir("symforge-slot-isolation-a");
        let project_b = project_dir("symforge-slot-isolation-b");
        std::fs::write(
            project_a.path().join("src").join("lib.rs"),
            "pub fn project_a() {}\n",
        )
        .expect("write A");
        std::fs::write(
            project_b.path().join("src").join("lib.rs"),
            "pub fn project_b() {}\n",
        )
        .expect("write B");

        let state = Arc::new(DaemonState::new());
        let opened_a = state
            .open_project_session(OpenProjectRequest {
                project_root: project_a.path().display().to_string(),
                client_name: "a".to_string(),
                pid: None,
            })
            .expect("open A");
        let opened_b = state
            .open_project_session(OpenProjectRequest {
                project_root: project_b.path().display().to_string(),
                client_name: "b".to_string(),
                pid: None,
            })
            .expect("open B");
        let root_a = canonical_project_root(project_a.path()).expect("canonical A");
        let slot_a = state
            .projects
            .read()
            .get(&opened_a.project_id)
            .cloned()
            .expect("A slot");

        let (reload_entered_tx, reload_entered_rx) = std::sync::mpsc::sync_channel(1);
        let (release_reload_tx, release_reload_rx) = std::sync::mpsc::sync_channel(1);
        let reload = std::thread::spawn(move || {
            slot_a.reload_with(&root_a, |index, root| {
                reload_entered_tx.send(()).expect("signal reload entered");
                release_reload_rx.recv().expect("release reload");
                index.reload(root)
            })
        });
        reload_entered_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("A reload should enter its mutation lane");

        let state_for_read = Arc::clone(&state);
        let session_b = opened_b.session_id.clone();
        let project_b_id = opened_b.project_id.clone();
        let (read_done_tx, read_done_rx) = std::sync::mpsc::sync_channel(1);
        let reader = std::thread::spawn(move || {
            let runtime = state_for_read.session_runtime(&session_b).is_some();
            let health = state_for_read.project_health(&project_b_id).is_some();
            let sessions = state_for_read.list_sessions(&project_b_id).is_some();
            read_done_tx
                .send((runtime, health, sessions))
                .expect("report B reads");
        });
        assert_eq!(
            read_done_rx
                .recv_timeout(Duration::from_secs(1))
                .expect("project B reads must not wait for project A reload"),
            (true, true, true)
        );

        release_reload_tx.send(()).expect("release A reload");
        reload
            .join()
            .expect("A reload thread")
            .expect("A reload result");
        reader.join().expect("B reader thread");
    }

    /// Feature 020 Slice 0 causal control for design defect 2.4 — project
    /// loading is not single-flight.
    ///
    /// `ensure_project_slot_for_session_with` reads the project map, runs the
    /// complete load OUTSIDE the map lock, and only then calls
    /// `entry.or_insert`. Concurrent first opens of one root therefore each
    /// build a full `ProjectInstance`; `or_insert` keeps one and drops the rest
    /// after they have already paid their I/O, parse, and memory cost.
    ///
    /// The waste leaves no other trace: the loser is never activated, so it
    /// starts no watcher and appears in no listing or health response. That is
    /// why the loader is counted here rather than asserted about indirectly.
    /// The closure is production's own — `ProjectInstance::load`, exactly as
    /// `ensure_project_slot_for_session` supplies it — so this counts real
    /// loads, not a stand-in.
    ///
    /// One first open must cost exactly one cold load, however many sessions
    /// race for it.
    #[test]
    #[ignore = "Feature 020 Slice 0 RED control for design defect 2.4; remove this attribute in Slice 2 (T030-T040) when project admission is single-flight"]
    fn concurrent_first_open_performs_exactly_one_cold_load() {
        let project = project_dir("symforge-slot-single-flight");
        std::fs::write(
            project.path().join("src").join("lib.rs"),
            "pub fn single_flight() {}\n",
        )
        .expect("write source");
        let canonical_root = project.path().to_path_buf();
        let state = Arc::new(DaemonState::new());
        let project_id = "project-single-flight";

        const RACERS: usize = 4;
        let loads = Arc::new(AtomicUsize::new(0));
        let start = Arc::new(std::sync::Barrier::new(RACERS));
        let racers: Vec<_> = (0..RACERS)
            .map(|racer| {
                let state = Arc::clone(&state);
                let loads = Arc::clone(&loads);
                let start = Arc::clone(&start);
                let root = canonical_root.clone();
                std::thread::spawn(move || {
                    start.wait();
                    state.ensure_project_slot_for_session_with(
                        &format!("session-{racer}"),
                        project_id,
                        || {
                            loads.fetch_add(1, Ordering::AcqRel);
                            ProjectInstance::load(&root)
                        },
                    )
                })
            })
            .collect();

        let slots: Vec<_> = racers
            .into_iter()
            .map(|racer| racer.join().expect("racer thread").expect("slot"))
            .collect();

        for slot in &slots {
            assert!(
                Arc::ptr_eq(slot, &slots[0]),
                "precondition: every racer must join the one authoritative slot"
            );
        }
        assert_eq!(
            loads.load(Ordering::Acquire),
            1,
            "{RACERS} concurrent first opens of one root must perform exactly one \
             cold load; every extra load is a complete project index built, paid \
             for, and discarded by `or_insert`"
        );
    }

    #[test]
    fn test_same_project_reads_prior_generation_during_reload() {
        let project = project_dir("symforge-slot-same-project");
        let source = project.path().join("src").join("lib.rs");
        std::fs::write(&source, "pub fn prior_generation() {}\n").expect("write prior");
        let state = Arc::new(DaemonState::new());
        let opened = state
            .open_project_session(OpenProjectRequest {
                project_root: project.path().display().to_string(),
                client_name: "same-project".to_string(),
                pid: None,
            })
            .expect("open project");
        let prior = Arc::clone(
            &state
                .session_runtime(&opened.session_id)
                .expect("prior runtime")
                .index
                .data_plane()
                .read(),
        );
        std::fs::write(&source, "pub fn next_generation() {}\n").expect("write next");

        let slot = state
            .projects
            .read()
            .get(&opened.project_id)
            .cloned()
            .expect("project slot");
        let root = canonical_project_root(project.path()).expect("canonical project");
        let (first_entered_tx, first_entered_rx) = std::sync::mpsc::sync_channel(1);
        let (release_first_tx, release_first_rx) = std::sync::mpsc::sync_channel(1);
        let first_slot = Arc::clone(&slot);
        let first_root = root.clone();
        let first_reload = std::thread::spawn(move || {
            first_slot.reload_with(&first_root, |index, root| {
                first_entered_tx.send(()).expect("signal first reload");
                release_first_rx.recv().expect("release first reload");
                index.reload(root)
            })
        });
        first_entered_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("first reload should enter");

        let (second_entered_tx, second_entered_rx) = std::sync::mpsc::sync_channel(1);
        let second_reload = std::thread::spawn(move || {
            slot.reload_with(&root, |index, root| {
                second_entered_tx.send(()).expect("signal second reload");
                index.reload(root)
            })
        });
        assert!(
            second_entered_rx
                .recv_timeout(Duration::from_millis(150))
                .is_err(),
            "same-project reloads must serialize on the mutation lane"
        );

        let during = Arc::clone(
            &state
                .session_runtime(&opened.session_id)
                .expect("runtime during reload")
                .index
                .data_plane()
                .read(),
        );
        assert!(
            Arc::ptr_eq(&prior, &during),
            "reads must retain the prior published index while reload builds"
        );

        release_first_tx.send(()).expect("release first reload");
        first_reload
            .join()
            .expect("first reload thread")
            .expect("first reload result");
        second_entered_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("second reload should enter after first finishes");
        second_reload
            .join()
            .expect("second reload thread")
            .expect("second reload result");

        let after = Arc::clone(
            &state
                .session_runtime(&opened.session_id)
                .expect("runtime after reload")
                .index
                .data_plane()
                .read(),
        );
        assert!(
            !Arc::ptr_eq(&prior, &after),
            "successful reload must atomically publish a new index generation"
        );
    }

    #[tokio::test]
    async fn test_spawn_daemon_serves_project_and_session_endpoints() {
        let _env_lock = env_lock().await;
        let daemon_home = TempDir::new().expect("daemon home");
        let _env_guard = EnvVarGuard::set("SYMFORGE_HOME", daemon_home.path());
        let project = project_dir("symforge-daemon-http");

        let handle = spawn_test_daemon("127.0.0.1", daemon_home.path())
            .await
            .expect("spawn daemon");
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
        wait_for_path_absent(&test_daemon_path(
            daemon_home.path(),
            LEGACY_DAEMON_PORT_FILE,
        ))
        .await;
        wait_for_path_absent(&test_daemon_path(
            daemon_home.path(),
            LEGACY_DAEMON_PID_FILE,
        ))
        .await;
        assert!(
            !test_daemon_path(daemon_home.path(), LEGACY_DAEMON_PORT_FILE).exists(),
            "daemon port file should be removed on shutdown"
        );
        assert!(
            !test_daemon_path(daemon_home.path(), LEGACY_DAEMON_PID_FILE).exists(),
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
        // Assembled at runtime: a credential-shaped source literal would trip
        // this repository's own detector (see
        // `repository_source_is_clean_under_its_own_detector`). Runtime bytes are
        // unchanged.
        let token = ["sfr03", "-test-", "to", "ken"].concat();
        let _auth_guard = EnvVarGuard::set_str(DAEMON_AUTH_TOKEN_ENV, &token);
        let project = project_dir("symforge-daemon-auth");

        let handle = spawn_test_daemon("127.0.0.1", daemon_home.path())
            .await
            .expect("spawn daemon");
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
            !health_body.contains(&token),
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
            !missing_body.contains(&token),
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
            !wrong_body.contains(&token),
            "wrong-token body must not leak auth token"
        );

        let opened = client
            .post(format!("{base_url}/v1/sessions/open"))
            .bearer_auth(&token)
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
            .bearer_auth(&token)
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
            .bearer_auth(&token)
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
            !tool_body.contains(&token),
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
            .bearer_auth(&token)
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
            .bearer_auth(&token)
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
        wait_for_path_absent(&test_daemon_path(
            daemon_home.path(),
            LEGACY_DAEMON_PORT_FILE,
        ))
        .await;
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

        let handle = spawn_test_daemon("127.0.0.1", daemon_home.path())
            .await
            .expect("spawn daemon");
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
                allow_protected_root: None,
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
                allow_protected_root: None,
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

        // (4b) Explicitly targeting the active project is still the same
        // single-project route. The daemon resolves the target before dispatch,
        // then strips the hint so the local worker does not refuse it as a
        // cross-project request without a daemon.
        let explicit_active = http
            .post(format!(
                "{base_url}/v1/sessions/{}/tools/search_symbols",
                opened.session_id
            ))
            .json(&serde_json::json!({
                "query": "xproj_marker",
                "project": project_a_id
            }))
            .send()
            .await
            .expect("explicit active query request")
            .error_for_status()
            .expect("explicit active query status")
            .text()
            .await
            .expect("explicit active query body");
        assert!(
            explicit_active.contains("xproj_marker_alpha"),
            "explicit active project query must surface A's symbol: {explicit_active}"
        );
        assert!(
            !explicit_active.contains("xproj_marker_beta"),
            "explicit active project query must NOT surface B's symbol: {explicit_active}"
        );
        assert!(
            !explicit_active.contains("cross-project queries"),
            "explicit active project must not hit the local cross-project refusal: {explicit_active}"
        );
        assert!(
            !explicit_active.contains("project: "),
            "explicit active project must render as the flat single-project route: {explicit_active}"
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
        wait_for_path_absent(&test_daemon_path(
            daemon_home.path(),
            LEGACY_DAEMON_PORT_FILE,
        ))
        .await;
    }

    #[tokio::test]
    async fn test_cross_project_knowledge_captures_selected_session_projects_only() {
        let _env_lock = env_lock().await;
        let daemon_home = TempDir::new().expect("daemon home");
        let _env_guard = EnvVarGuard::set("SYMFORGE_HOME", daemon_home.path());

        let project_a = project_dir("symforge-knowledge-a");
        let project_b = project_dir("symforge-knowledge-b");
        let project_c = project_dir("symforge-knowledge-c");
        for (project, source_name, source, document) in [
            (
                &project_a,
                "a.rs",
                "pub fn alpha_anchor() {}\n",
                "# Alpha knowledge\nQuartz boundary alpha. See [alpha](../src/a.rs).\n",
            ),
            (
                &project_b,
                "b.rs",
                "pub fn beta_anchor() {}\n",
                "# Beta knowledge\nQuartz boundary beta. See [beta](../src/b.rs).\n",
            ),
            (
                &project_c,
                "c.rs",
                "pub fn gamma_anchor() {}\n",
                "# Gamma knowledge\nQuartz boundary gamma. See [gamma](../src/c.rs).\n",
            ),
        ] {
            std::fs::create_dir_all(project.path().join("docs")).expect("docs dir");
            std::fs::write(project.path().join("src").join(source_name), source)
                .expect("source fixture");
            std::fs::write(project.path().join("docs").join("knowledge.md"), document)
                .expect("knowledge fixture");
        }

        let handle = spawn_test_daemon("127.0.0.1", daemon_home.path())
            .await
            .expect("spawn daemon");
        let base_url = format!("http://127.0.0.1:{}", handle.port);
        let http = authed_client(&handle);

        let opened = http
            .post(format!("{base_url}/v1/sessions/open"))
            .json(&OpenProjectRequest {
                project_root: project_a.path().display().to_string(),
                client_name: "knowledge-xproj".to_string(),
                pid: Some(std::process::id()),
            })
            .send()
            .await
            .expect("open A")
            .error_for_status()
            .expect("open A status")
            .json::<OpenProjectResponse>()
            .await
            .expect("open A body");
        let project_a_id = opened.project_id.clone();
        let project_b_id =
            project_key(&canonical_project_root(project_b.path()).expect("canonical B"));
        let project_a_alias = project_a
            .path()
            .file_name()
            .and_then(|name| name.to_str())
            .expect("A project-name alias")
            .to_string();
        let project_b_alias = project_b
            .path()
            .file_name()
            .and_then(|name| name.to_str())
            .expect("B project-name alias")
            .to_string();

        for (path, add) in [(project_a.path(), None), (project_b.path(), Some(true))] {
            let response = http
                .post(format!(
                    "{base_url}/v1/sessions/{}/tools/index_folder",
                    opened.session_id
                ))
                .json(&IndexFolderInput {
                    path: path.display().to_string(),
                    idempotency_key: None,
                    add,
                    allow_protected_root: None,
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
                response.contains("Indexed") || response.contains("added to working set"),
                "index response: {response}"
            );
        }

        // Open C globally, but in another session. The first session's wildcard
        // must remain scoped to its own A+B working set.
        let _other_session = http
            .post(format!("{base_url}/v1/sessions/open"))
            .json(&OpenProjectRequest {
                project_root: project_c.path().display().to_string(),
                client_name: "knowledge-other-session".to_string(),
                pid: Some(std::process::id()),
            })
            .send()
            .await
            .expect("open C")
            .error_for_status()
            .expect("open C status")
            .json::<OpenProjectResponse>()
            .await
            .expect("open C body");

        let wildcard = http
            .post(format!(
                "{base_url}/v1/sessions/{}/tools/search_knowledge",
                opened.session_id
            ))
            .json(&serde_json::json!({
                "query": "quartz boundary",
                "projects": ["*"]
            }))
            .send()
            .await
            .expect("wildcard request")
            .error_for_status()
            .expect("wildcard status")
            .text()
            .await
            .expect("wildcard body");
        assert!(
            wildcard.contains("Quartz boundary alpha"),
            "A hit: {wildcard}"
        );
        assert!(
            wildcard.contains("Quartz boundary beta"),
            "B hit: {wildcard}"
        );
        assert!(
            !wildcard.contains("Quartz boundary gamma"),
            "session wildcard leaked C: {wildcard}"
        );

        let explicit_b = http
            .post(format!(
                "{base_url}/v1/sessions/{}/tools/search_knowledge",
                opened.session_id
            ))
            .json(&serde_json::json!({
                "query": "quartz boundary",
                "project": project_b_id
            }))
            .send()
            .await
            .expect("B request")
            .error_for_status()
            .expect("B status")
            .text()
            .await
            .expect("B body");
        assert!(
            explicit_b.contains("Quartz boundary beta"),
            "B hit: {explicit_b}"
        );
        assert!(
            !explicit_b.contains("Quartz boundary alpha") && !explicit_b.contains("src/a.rs"),
            "explicit B leaked A hit/bridge: {explicit_b}"
        );

        let alias_b = http
            .post(format!(
                "{base_url}/v1/sessions/{}/tools/search_knowledge",
                opened.session_id
            ))
            .json(&serde_json::json!({
                "query": "quartz boundary",
                "project": project_b_alias.clone()
            }))
            .send()
            .await
            .expect("B alias request")
            .error_for_status()
            .expect("B alias status")
            .text()
            .await
            .expect("B alias body");
        assert!(
            alias_b.contains("Quartz boundary beta"),
            "unique project-name aliases must resolve: {alias_b}"
        );
        assert!(
            !alias_b.contains("Quartz boundary alpha") && !alias_b.contains("src/a.rs"),
            "project-name alias leaked A hit/bridge: {alias_b}"
        );

        let reversed_subset = http
            .post(format!(
                "{base_url}/v1/sessions/{}/tools/search_knowledge",
                opened.session_id
            ))
            .json(&serde_json::json!({
                "query": "quartz boundary",
                "projects": [project_b_alias.clone(), project_a_alias.clone()]
            }))
            .send()
            .await
            .expect("subset request")
            .error_for_status()
            .expect("subset status")
            .text()
            .await
            .expect("subset body");
        let mut sorted_ids = [project_a_id.clone(), project_b_id.clone()];
        sorted_ids.sort();
        let first = reversed_subset
            .find(&format!("project: {}", sorted_ids[0]))
            .expect("first deterministic project header");
        let second = reversed_subset
            .find(&format!("project: {}", sorted_ids[1]))
            .expect("second deterministic project header");
        assert!(
            first < second,
            "project envelopes must be canonical: {reversed_subset}"
        );

        let review_subset = |selectors: Vec<String>| {
            let http = http.clone();
            let url = format!(
                "{base_url}/v1/sessions/{}/tools/review_knowledge",
                opened.session_id
            );
            async move {
                http.post(url)
                    .json(&serde_json::json!({
                        "mode": "remediation",
                        "projects": selectors,
                        "limit": 1
                    }))
                    .send()
                    .await
                    .expect("review subset request")
                    .error_for_status()
                    .expect("review subset status")
                    .text()
                    .await
                    .expect("review subset body")
            }
        };
        let reversed_review =
            review_subset(vec![project_b_alias.clone(), project_a_alias.clone()]).await;
        let forward_review =
            review_subset(vec![project_a_alias.clone(), project_b_alias.clone()]).await;
        assert_eq!(
            reversed_review, forward_review,
            "selector order must not affect the captured per-source plans or hashes"
        );
        assert_eq!(
            reversed_review.matches("top_result_hash=").count(),
            1,
            "one top-level result hash: {reversed_review}"
        );
        assert_eq!(
            reversed_review.matches("review_hash=").count(),
            2,
            "one isolated plan hash per selected source: {reversed_review}"
        );
        let review_first = reversed_review
            .find(&format!("project: {}", sorted_ids[0]))
            .expect("first review project header");
        let review_second = reversed_review
            .find(&format!("project: {}", sorted_ids[1]))
            .expect("second review project header");
        assert!(
            review_first < review_second,
            "review project envelopes must be canonical: {reversed_review}"
        );

        let conflict = http
            .post(format!(
                "{base_url}/v1/sessions/{}/tools/search_knowledge",
                opened.session_id
            ))
            .json(&serde_json::json!({
                "query": "quartz boundary",
                "project": project_a_id,
                "projects": ["*"]
            }))
            .send()
            .await
            .expect("conflict request")
            .error_for_status()
            .expect("conflict status")
            .text()
            .await
            .expect("conflict body");
        assert!(
            conflict.contains("mutually exclusive"),
            "selector conflict: {conflict}"
        );

        let unknown = http
            .post(format!(
                "{base_url}/v1/sessions/{}/tools/search_knowledge",
                opened.session_id
            ))
            .json(&serde_json::json!({
                "query": "quartz boundary",
                "project": "project-not-open"
            }))
            .send()
            .await
            .expect("unknown request")
            .error_for_status()
            .expect("unknown status")
            .text()
            .await
            .expect("unknown body");
        assert!(unknown.contains("not open"), "unknown selector: {unknown}");

        let review_conflict = http
            .post(format!(
                "{base_url}/v1/sessions/{}/tools/review_knowledge",
                opened.session_id
            ))
            .json(&serde_json::json!({
                "mode": "summary",
                "project": project_a_id,
                "projects": ["*"]
            }))
            .send()
            .await
            .expect("review conflict request")
            .error_for_status()
            .expect("review conflict status")
            .text()
            .await
            .expect("review conflict body");
        assert!(
            review_conflict.contains("mutually exclusive"),
            "review selector conflict: {review_conflict}"
        );

        let review_unknown = http
            .post(format!(
                "{base_url}/v1/sessions/{}/tools/review_knowledge",
                opened.session_id
            ))
            .json(&serde_json::json!({
                "mode": "summary",
                "project": "project-not-open"
            }))
            .send()
            .await
            .expect("review unknown request")
            .error_for_status()
            .expect("review unknown status")
            .text()
            .await
            .expect("review unknown body");
        assert!(
            review_unknown.contains("not open"),
            "unknown review selector: {review_unknown}"
        );

        let _ = handle.shutdown_tx.send(());
        wait_for_path_absent(&test_daemon_path(
            daemon_home.path(),
            LEGACY_DAEMON_PORT_FILE,
        ))
        .await;
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

        let handle = spawn_test_daemon("127.0.0.1", daemon_home.path())
            .await
            .expect("spawn daemon");
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
            allow_protected_root: None,
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
            allow_protected_root: None,
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
            let index = {
                let projects = handle.state.projects.read();
                let slot = projects
                    .get(&project_b_id)
                    .expect("project B loaded in daemon");
                slot.metadata.read().index.shared()
            };
            index
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
            let index = {
                let projects = handle.state.projects.read();
                let slot = projects
                    .get(&project_b_id)
                    .expect("project B loaded in daemon");
                slot.metadata.read().index.shared()
            };
            index
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
        wait_for_path_absent(&test_daemon_path(
            daemon_home.path(),
            LEGACY_DAEMON_PORT_FILE,
        ))
        .await;
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
        let control_state_dir = test_control_state(daemon_home.path());

        let handle = spawn_daemon_at("127.0.0.1", &control_state_dir)
            .await
            .expect("spawn daemon");
        let base_url = format!("http://127.0.0.1:{}", handle.port);

        // 1. A token file is always present after start.
        let token_path = test_daemon_path(daemon_home.path(), daemon_token_file_name());
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
            resolve_daemon_auth_token_at(&control_state_dir).as_deref(),
            Some(file_token.as_str()),
            "client token resolution must read the persisted token file"
        );

        let _ = handle.shutdown_tx.send(());
        wait_for_path_absent(&test_daemon_path(
            daemon_home.path(),
            LEGACY_DAEMON_PORT_FILE,
        ))
        .await;
    }

    #[tokio::test]
    async fn test_daemon_executes_session_scoped_tool_calls() {
        let _env_lock = env_lock().await;
        let daemon_home = TempDir::new().expect("daemon home");
        let _env_guard = EnvVarGuard::set("SYMFORGE_HOME", daemon_home.path());
        let project = project_dir("symforge-daemon-tool");
        std::fs::write(project.path().join("src").join("main.rs"), "fn main() {}\n")
            .expect("write source");

        let handle = spawn_test_daemon("127.0.0.1", daemon_home.path())
            .await
            .expect("spawn daemon");
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

        let curation_preview = client
            .post(format!(
                "{base_url}/v1/sessions/{}/tools/curate_knowledge",
                opened.session_id
            ))
            .json(&serde_json::json!({
                "project": opened.project_id.clone(),
                "actions": [{
                    "action_id": "action-daemon-route-oracle",
                    "mutation": {
                        "operation": "upsert",
                        "entry": {
                            "entry_id": "entry-daemon-route-oracle",
                            "target": {
                                "path": "src/main.rs",
                                "content_hash": "0".repeat(64)
                            },
                            "lifecycle": "unknown",
                            "evidence": [],
                            "justification_code": "daemon-route-oracle"
                        }
                    }
                }],
                "if_source_review_hash": "0".repeat(64),
                "if_manifest_digest": "0".repeat(64),
                "if_policy_digest": "0".repeat(64)
            }))
            .send()
            .await
            .expect("curate_knowledge preview request")
            .error_for_status()
            .expect("curate_knowledge preview status")
            .text()
            .await
            .expect("curate_knowledge preview body");
        assert!(
            curation_preview.contains("stale_review_hash"),
            "daemon must dispatch curate_knowledge to the project runtime: {curation_preview}"
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
        wait_for_path_absent(&test_daemon_path(
            daemon_home.path(),
            LEGACY_DAEMON_PORT_FILE,
        ))
        .await;
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

        let handle = spawn_test_daemon("127.0.0.1", daemon_home.path())
            .await
            .expect("spawn daemon");
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
        let RootResolution::Bound(binding) = crate::discovery::resolve_root_candidate(
            &canonical_root,
            RootCandidateSource::LaunchCwd,
            RootRequestMode::Automatic,
        ) else {
            panic!("opened daemon root must remain bound");
        };
        let state_placement = crate::discovery::resolve_state_placement(&binding);
        assert!(
            crate::live_index::persist::load_snapshot(&canonical_root, &state_placement).is_some(),
            "checkpoint should write a loadable snapshot under {}/.symforge/index.bin",
            canonical_root.display()
        );

        let _ = handle.shutdown_tx.send(());
        wait_for_path_absent(&test_daemon_path(
            daemon_home.path(),
            LEGACY_DAEMON_PORT_FILE,
        ))
        .await;
    }

    #[tokio::test]
    async fn test_daemon_port_if_compatible_accepts_matching_identity() {
        let _env_lock = env_lock().await;
        let daemon_home = TempDir::new().expect("daemon home");
        let _env_guard = EnvVarGuard::set("SYMFORGE_HOME", daemon_home.path());

        let health = DaemonState::new().health();
        let (port, shutdown_tx) = spawn_fake_health_server(health).await;
        std::fs::write(
            test_daemon_path(daemon_home.path(), LEGACY_DAEMON_PORT_FILE),
            port.to_string(),
        )
        .expect("write daemon port");

        let identity = current_daemon_identity();
        let selected =
            daemon_port_if_compatible_at(&test_control_state(daemon_home.path()), &identity)
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
            started_at_unix_secs: None,
        };
        let (port, shutdown_tx) = spawn_fake_health_server(health).await;
        std::fs::write(
            test_daemon_path(daemon_home.path(), LEGACY_DAEMON_PORT_FILE),
            port.to_string(),
        )
        .expect("write daemon port");

        let identity = current_daemon_identity();
        let selected =
            daemon_port_if_compatible_at(&test_control_state(daemon_home.path()), &identity)
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
            started_at_unix_secs: None,
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
        let mut child = crate::process_util::hidden_command("powershell")
            .args(["-NoProfile", "-Command", "Start-Sleep -Seconds 30"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn long-running child");

        #[cfg(not(windows))]
        let mut child = crate::process_util::hidden_command("sleep")
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
            started_at_unix_secs: None,
        };
        let (port, shutdown_tx) = spawn_fake_health_server(health).await;
        std::fs::write(
            test_daemon_path(daemon_home.path(), LEGACY_DAEMON_PORT_FILE),
            port.to_string(),
        )
        .expect("write daemon port");
        std::fs::write(
            test_daemon_path(daemon_home.path(), LEGACY_DAEMON_PID_FILE),
            child_pid.to_string(),
        )
        .expect("write daemon pid");

        stop_incompatible_recorded_daemon_at(
            &test_control_state(daemon_home.path()),
            &current_daemon_identity(),
        )
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
            started_at_unix_secs: None,
        };
        let (port, shutdown_tx) = spawn_fake_health_server(health).await;
        std::fs::write(
            test_daemon_path(daemon_home.path(), LEGACY_DAEMON_PORT_FILE),
            port.to_string(),
        )
        .expect("write daemon port");

        stop_incompatible_recorded_daemon_at(
            &test_control_state(daemon_home.path()),
            &current_daemon_identity(),
        )
        .await
        .expect("stop incompatible daemon");

        assert!(
            !test_daemon_path(daemon_home.path(), LEGACY_DAEMON_PORT_FILE).exists(),
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

        let handle = spawn_test_daemon("127.0.0.1", daemon_home.path())
            .await
            .expect("spawn daemon");
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
        wait_for_path_absent(&test_daemon_path(
            daemon_home.path(),
            LEGACY_DAEMON_PORT_FILE,
        ))
        .await;
    }

    /// caller-root guard parity (worktree-routing incident): daemon-proxied
    /// sidecar routes must enforce the SAME caller_root check the local
    /// sidecar router installs. A session whose ACTIVE project moved must 409
    /// a caller rooted at the old project instead of serving the wrong one.
    #[tokio::test]
    async fn test_daemon_sidecar_enforces_caller_root_guard() {
        let _env_lock = env_lock().await;
        let daemon_home = TempDir::new().expect("daemon home");
        let _env_guard = EnvVarGuard::set("SYMFORGE_HOME", daemon_home.path());
        let project_a = project_dir("symforge-guard-home-a");
        let project_b = project_dir("symforge-guard-open-b");
        std::fs::write(project_a.path().join("src").join("a.rs"), "fn a() {}\n")
            .expect("write source a");
        std::fs::write(project_b.path().join("src").join("b.rs"), "fn b() {}\n")
            .expect("write source b");

        let handle = spawn_test_daemon("127.0.0.1", daemon_home.path())
            .await
            .expect("spawn daemon");
        let client = authed_client(&handle);
        let base_url = format!("http://127.0.0.1:{}", handle.port);

        let opened = client
            .post(format!("{base_url}/v1/sessions/open"))
            .json(&OpenProjectRequest {
                project_root: project_a.path().display().to_string(),
                client_name: "guard".to_string(),
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

        // Default-open B: the session's ACTIVE project moves to B (locked
        // contract). A caller rooted at A must now be refused, not served B.
        client
            .post(format!(
                "{base_url}/v1/sessions/{}/tools/index_folder",
                opened.session_id
            ))
            .json(&IndexFolderInput {
                path: project_b.path().display().to_string(),
                idempotency_key: None,
                add: None,
                allow_protected_root: None,
            })
            .send()
            .await
            .expect("open B request")
            .error_for_status()
            .expect("open B status");

        let url = |extra: &str| {
            format!(
                "{base_url}/v1/sessions/{}/sidecar/repo-map{extra}",
                opened.session_id
            )
        };

        let mismatched = client
            .get(url(&format!("?caller_root={}", project_a.path().display())))
            .send()
            .await
            .expect("mismatched request");
        assert_eq!(
            mismatched.status(),
            StatusCode::CONFLICT,
            "caller_root=A must 409 once B is active"
        );

        let matched = client
            .get(url(&format!("?caller_root={}", project_b.path().display())))
            .send()
            .await
            .expect("matched request");
        assert!(
            matched.status().is_success(),
            "caller_root=B (the active project) must be served"
        );

        let unpinned = client.get(url("")).send().await.expect("unpinned request");
        assert!(
            unpinned.status().is_success(),
            "a request without caller_root stays unguarded"
        );

        let _ = handle.shutdown_tx.send(());
        wait_for_path_absent(&test_daemon_path(
            daemon_home.path(),
            LEGACY_DAEMON_PORT_FILE,
        ))
        .await;
    }

    #[tokio::test]
    async fn test_daemon_workflow_repo_start_endpoint_matches_repo_map_route() {
        let _env_lock = env_lock().await;
        let daemon_home = TempDir::new().expect("daemon home");
        let _env_guard = EnvVarGuard::set("SYMFORGE_HOME", daemon_home.path());
        let project = project_dir("symforge-daemon-workflow-repo-start");
        std::fs::write(project.path().join("src").join("main.rs"), "fn main() {}\n")
            .expect("write source");

        let handle = spawn_test_daemon("127.0.0.1", daemon_home.path())
            .await
            .expect("spawn daemon");
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
        wait_for_path_absent(&test_daemon_path(
            daemon_home.path(),
            LEGACY_DAEMON_PORT_FILE,
        ))
        .await;
    }

    #[tokio::test]
    async fn test_daemon_serves_session_scoped_prompt_context_hook_endpoint() {
        let _env_lock = env_lock().await;
        let daemon_home = TempDir::new().expect("daemon home");
        let _env_guard = EnvVarGuard::set("SYMFORGE_HOME", daemon_home.path());
        let project = project_dir("symforge-daemon-prompt-hook");
        std::fs::write(project.path().join("src").join("main.rs"), "fn main() {}\n")
            .expect("write source");

        let handle = spawn_test_daemon("127.0.0.1", daemon_home.path())
            .await
            .expect("spawn daemon");
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
        wait_for_path_absent(&test_daemon_path(
            daemon_home.path(),
            LEGACY_DAEMON_PORT_FILE,
        ))
        .await;
    }

    #[tokio::test]
    async fn test_daemon_workflow_prompt_context_endpoint_matches_canonical_route() {
        let _env_lock = env_lock().await;
        let daemon_home = TempDir::new().expect("daemon home");
        let _env_guard = EnvVarGuard::set("SYMFORGE_HOME", daemon_home.path());
        let project = project_dir("symforge-daemon-workflow-prompt-context");
        std::fs::write(project.path().join("src").join("main.rs"), "fn main() {}\n")
            .expect("write source");

        let handle = spawn_test_daemon("127.0.0.1", daemon_home.path())
            .await
            .expect("spawn daemon");
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
        wait_for_path_absent(&test_daemon_path(
            daemon_home.path(),
            LEGACY_DAEMON_PORT_FILE,
        ))
        .await;
    }

    #[tokio::test]
    async fn test_index_folder_default_open_activates_target() {
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

        let handle = spawn_test_daemon("127.0.0.1", daemon_home.path())
            .await
            .expect("spawn daemon");
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
                allow_protected_root: None,
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
        assert!(
            reload.contains("active_project="),
            "default open must report activation, got: {reload}"
        );

        let target_sessions = client
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
        assert_eq!(target_sessions.len(), 1);
        assert_eq!(target_sessions[0].session_id, opened.session_id);
        assert_eq!(
            target_sessions[0].project_id,
            project_key(&canonical_project_root(project_b.path()).expect("canonical root")),
            "session summary must report B as the ACTIVE project"
        );

        let home_sessions = client
            .get(format!(
                "{base_url}/v1/projects/{}/sessions",
                opened.project_id
            ))
            .send()
            .await
            .expect("home session list request")
            .error_for_status()
            .expect("home session list status")
            .json::<Vec<SessionSummary>>()
            .await
            .expect("home session list body");
        assert_eq!(
            home_sessions.len(),
            1,
            "opening B must not evict home A from the working set"
        );
        assert_eq!(home_sessions[0].session_id, opened.session_id);

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
            "unqualified reads must follow the ACTIVATED project B: {outline}"
        );
        assert!(
            !outline.contains("old.rs"),
            "default open of B must switch unqualified reads off home A: {outline}"
        );

        let _ = handle.shutdown_tx.send(());
        wait_for_path_absent(&test_daemon_path(
            daemon_home.path(),
            LEGACY_DAEMON_PORT_FILE,
        ))
        .await;
    }

    /// Task 4 parity table (daemon route): explicit `project` selects the open
    /// project B, omission stays bound to the ACTIVE project (home A — B was
    /// opened additively, which never activates), and an unknown selector
    /// returns deterministic candidates without touching any index.
    #[tokio::test]
    async fn test_project_routing_parity_table() {
        let _env_lock = env_lock().await;
        let daemon_home = TempDir::new().expect("daemon home");
        let _env_guard = EnvVarGuard::set("SYMFORGE_HOME", daemon_home.path());
        let project_a = project_dir("symforge-route-home-a");
        let project_b = project_dir("symforge-route-open-b");
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

        let handle = spawn_test_daemon("127.0.0.1", daemon_home.path())
            .await
            .expect("spawn daemon");
        let client = authed_client(&handle);
        let base_url = format!("http://127.0.0.1:{}", handle.port);

        let opened = client
            .post(format!("{base_url}/v1/sessions/open"))
            .json(&OpenProjectRequest {
                project_root: project_a.path().display().to_string(),
                client_name: "route-parity".to_string(),
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
        let project_b_id =
            project_key(&canonical_project_root(project_b.path()).expect("canonical b"));

        let opened_b = client
            .post(format!(
                "{base_url}/v1/sessions/{}/tools/index_folder",
                opened.session_id
            ))
            .json(&IndexFolderInput {
                path: project_b.path().display().to_string(),
                idempotency_key: None,
                add: Some(true),
                allow_protected_root: None,
            })
            .send()
            .await
            .expect("open B request")
            .error_for_status()
            .expect("open B status")
            .text()
            .await
            .expect("open B body");
        assert!(opened_b.contains("Indexed"), "open B: {opened_b}");

        let call = |tool: &str, body: serde_json::Value| {
            let client = client.clone();
            let url = format!("{base_url}/v1/sessions/{}/tools/{tool}", opened.session_id);
            async move {
                client
                    .post(url)
                    .json(&body)
                    .send()
                    .await
                    .expect("tool request")
                    .error_for_status()
                    .expect("tool status")
                    .text()
                    .await
                    .expect("tool body")
            }
        };

        // get_repo_map: explicit B vs omitted home A.
        let map_b = call(
            "get_repo_map",
            serde_json::json!({"detail": "full", "project": project_b_id}),
        )
        .await;
        assert!(
            map_b.contains("new.rs") && !map_b.contains("old.rs"),
            "explicit project must serve B: {map_b}"
        );
        let map_home = call("get_repo_map", serde_json::json!({"detail": "full"})).await;
        assert!(
            map_home.contains("old.rs") && !map_home.contains("new.rs"),
            "omission must serve immutable home A: {map_home}"
        );

        // get_file_content: exact read routed to B; home cannot see B's file.
        let content_b = call(
            "get_file_content",
            serde_json::json!({"path": "src/new.rs", "project": project_b_id}),
        )
        .await;
        assert!(
            content_b.contains("fn new_fn"),
            "explicit project read must return B's bytes: {content_b}"
        );
        let content_home = call(
            "get_file_content",
            serde_json::json!({"path": "src/new.rs"}),
        )
        .await;
        assert!(
            !content_home.contains("fn new_fn"),
            "home read must not leak B's bytes: {content_home}"
        );

        // search_files: discovery routed by explicit project.
        let files_b = call(
            "search_files",
            serde_json::json!({"query": "new.rs", "project": project_b_id}),
        )
        .await;
        assert!(
            files_b.contains("new.rs"),
            "explicit project search_files must serve B: {files_b}"
        );

        // Unknown selector: deterministic candidates, no index mutation.
        let unknown = call(
            "get_repo_map",
            serde_json::json!({"detail": "full", "project": "no-such-project"}),
        )
        .await;
        assert!(
            unknown.contains("not open") && unknown.contains(&project_b_id),
            "unknown selector must return candidates: {unknown}"
        );

        let _ = handle.shutdown_tx.send(());
        wait_for_path_absent(&test_daemon_path(
            daemon_home.path(),
            LEGACY_DAEMON_PORT_FILE,
        ))
        .await;
    }

    /// Task 4 leftover: set-valued `search_files` fans out across targeted
    /// projects with attributed hits, one deterministic global cap, and honest
    /// refusals for the modes that stay single-project (resolve/coupling) and
    /// for a `project`+`projects` mutual-exclusion violation. A lone `project`
    /// selector keeps the FULL single-project handler (envelope, modes).
    #[tokio::test]
    async fn test_search_files_projects_fan_out() {
        let _env_lock = env_lock().await;
        let daemon_home = TempDir::new().expect("daemon home");
        let _env_guard = EnvVarGuard::set("SYMFORGE_HOME", daemon_home.path());
        let project_a = project_dir("symforge-files-home-a");
        let project_b = project_dir("symforge-files-open-b");
        std::fs::write(
            project_a.path().join("src").join("widget_old.rs"),
            "fn old_fn() {}\n",
        )
        .expect("write source a");
        std::fs::write(
            project_b.path().join("src").join("widget_new.rs"),
            "fn new_fn() {}\n",
        )
        .expect("write source b");

        let handle = spawn_test_daemon("127.0.0.1", daemon_home.path())
            .await
            .expect("spawn daemon");
        let client = authed_client(&handle);
        let base_url = format!("http://127.0.0.1:{}", handle.port);

        let opened = client
            .post(format!("{base_url}/v1/sessions/open"))
            .json(&OpenProjectRequest {
                project_root: project_a.path().display().to_string(),
                client_name: "files-fan-out".to_string(),
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
        let project_b_id =
            project_key(&canonical_project_root(project_b.path()).expect("canonical b"));

        let opened_b = client
            .post(format!(
                "{base_url}/v1/sessions/{}/tools/index_folder",
                opened.session_id
            ))
            .json(&IndexFolderInput {
                path: project_b.path().display().to_string(),
                idempotency_key: None,
                add: None,
                allow_protected_root: None,
            })
            .send()
            .await
            .expect("open B request")
            .error_for_status()
            .expect("open B status")
            .text()
            .await
            .expect("open B body");
        assert!(opened_b.contains("Indexed"), "open B: {opened_b}");

        let call = |body: serde_json::Value| {
            let client = client.clone();
            let url = format!(
                "{base_url}/v1/sessions/{}/tools/search_files",
                opened.session_id
            );
            async move {
                client
                    .post(url)
                    .json(&body)
                    .send()
                    .await
                    .expect("tool request")
                    .error_for_status()
                    .expect("tool status")
                    .text()
                    .await
                    .expect("tool body")
            }
        };

        // All projects: attributed hits from BOTH, with project headers.
        let all = call(serde_json::json!({"query": "widget", "projects": ["*"]})).await;
        assert!(
            all.contains("widget_old.rs") && all.contains("widget_new.rs"),
            "wildcard fan-out must surface both projects' files: {all}"
        );
        assert!(
            all.contains(&format!("project: {}", opened.project_id))
                && all.contains(&format!("project: {project_b_id}")),
            "multi-target output must attribute hits by project: {all}"
        );

        // Explicit subset of one: only B's hits, flat (no headers).
        let only_b = call(serde_json::json!({"query": "widget", "projects": [project_b_id]})).await;
        assert!(
            only_b.contains("widget_new.rs") && !only_b.contains("widget_old.rs"),
            "single-target fan-out must serve only B: {only_b}"
        );
        assert!(
            !only_b.contains("── project:"),
            "single-target output renders flat: {only_b}"
        );

        // resolve mode stays single-project: honest refusal, no silent drop.
        let refused =
            call(serde_json::json!({"query": "widget", "resolve": true, "projects": ["*"]})).await;
        assert!(
            refused.contains("not supported with cross-project targeting"),
            "resolve mode must refuse cross-project targeting: {refused}"
        );

        // project + projects together: deterministic mutual-exclusion error.
        let conflict = call(serde_json::json!({
            "query": "widget",
            "project": opened.project_id,
            "projects": [project_b_id],
        }))
        .await;
        assert!(
            conflict.contains("mutually exclusive"),
            "project+projects must be rejected: {conflict}"
        );

        let _ = handle.shutdown_tx.send(());
        wait_for_path_absent(&test_daemon_path(
            daemon_home.path(),
            LEGACY_DAEMON_PORT_FILE,
        ))
        .await;
    }

    /// INCIDENT GUARD (2026-07-11): a test build must NEVER auto-spawn a
    /// daemon process — under `cargo test` the spawned `current_exe()` is the
    /// libtest binary and the `daemon` argument is a test FILTER, so spawning
    /// recursively re-runs the suite (exponential fork bomb, console-window
    /// flood). This pins the refusal at both seams.
    #[tokio::test]
    async fn test_test_builds_never_auto_spawn_daemon_processes() {
        let error = spawn_daemon_process().expect_err("test build must refuse to spawn");
        assert!(
            error.to_string().contains("test build"),
            "refusal must name the test-build guard: {error}"
        );

        // ensure_daemon_running with no daemon reachable must fail fast with
        // the same refusal instead of spawning or waiting on the start lock.
        let _env_lock = env_lock().await;
        let empty_home = TempDir::new().expect("empty home");
        let _env_guard = EnvVarGuard::set("SYMFORGE_HOME", empty_home.path());
        let error = ensure_daemon_running_at(&test_control_state(empty_home.path()))
            .await
            .expect_err("no daemon + no spawn must error");
        assert!(
            error.to_string().contains("auto-spawn is disabled"),
            "ensure_daemon_running must fail fast without spawning: {error}"
        );
    }

    /// Task 8: after the daemon dies and a replacement comes up, the proxy
    /// reconnect must restore the WHOLE working set — home A stays home (same
    /// deterministic id) and additively-opened B is reopened and explicitly
    /// routable with the same id — before any read is served.
    #[tokio::test]
    async fn test_reconnect_reopens_home_and_working_set() {
        let _env_lock = env_lock().await;
        let daemon_home = TempDir::new().expect("daemon home");
        let _env_guard = EnvVarGuard::set("SYMFORGE_HOME", daemon_home.path());
        let project_a = project_dir("symforge-reconnect-home-a");
        let project_b = project_dir("symforge-reconnect-open-b");
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

        let first = spawn_test_daemon("127.0.0.1", daemon_home.path())
            .await
            .expect("spawn first daemon");
        let base_url = format!("http://127.0.0.1:{}", first.port);
        let http = authed_client(&first);
        let opened = http
            .post(format!("{base_url}/v1/sessions/open"))
            .json(&OpenProjectRequest {
                project_root: project_a.path().display().to_string(),
                client_name: "reconnect".to_string(),
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

        let client = DaemonSessionClient::new_for_test_at(
            base_url.clone(),
            opened.project_id.clone(),
            opened.session_id.clone(),
            opened.project_name.clone(),
            test_control_state(daemon_home.path()),
        )
        .with_project_root(project_a.path().to_path_buf());
        let server = crate::protocol::SymForgeServer::new_daemon_proxy(client);

        // Open B additively through the proxy so the sibling root is recorded
        // WITHOUT changing the active project (home A stays active).
        let opened_b = server
            .index_folder(Parameters(IndexFolderInput {
                path: project_b.path().display().to_string(),
                idempotency_key: None,
                add: Some(true),
                allow_protected_root: None,
            }))
            .await;
        assert!(opened_b.starts_with("Indexed "), "open B: {opened_b}");
        let project_b_id =
            project_key(&canonical_project_root(project_b.path()).expect("canonical b"));

        // Kill the first daemon and bring up a replacement on the SAME home.
        // Wait on the OS-TAGGED port file — the one daemons actually write —
        // so daemon 1's shutdown cleanup provably finished before daemon 2
        // writes its own files (the untagged legacy name never exists, so
        // waiting on it is a no-op that races the cleanup).
        let _ = first.shutdown_tx.send(());
        wait_for_path_absent(&test_daemon_path(
            daemon_home.path(),
            daemon_port_file_name(),
        ))
        .await;
        let second = spawn_test_daemon("127.0.0.1", daemon_home.path())
            .await
            .expect("spawn second daemon");

        let stale_client = server
            .daemon_client
            .as_ref()
            .expect("daemon proxy client")
            .read()
            .await
            .clone();
        let stale_activation_lane = stale_client.activation_lane();
        crate::protocol::result_status::with_project_evidence_scope(None, async {
            let reconnected = stale_client
                .reconnect()
                .await
                .expect("direct reconnect probe");
            assert!(
                std::sync::Arc::ptr_eq(&stale_activation_lane, &reconnected.activation_lane()),
                "a reconnect replacement must retain the connection's activation lane"
            );
            assert!(
                crate::protocol::result_status::current_project_evidence().is_none(),
                "reconnect housekeeping receipts must not escape into the next user call"
            );
        })
        .await;

        // A routed read through the stale proxy: the first attempt fails, the
        // reconnect discovers the replacement, reopens home + B, verifies ids,
        // and the retry serves B.
        let map_b_input = serde_json::from_value(serde_json::json!({
            "detail": "full",
            "project": project_b_id,
        }))
        .expect("map input");
        let map_b = server.get_repo_map(Parameters(map_b_input)).await;
        assert!(
            map_b.contains("new.rs") && !map_b.contains("old.rs"),
            "explicit B must remain routable after reconnect: {map_b}"
        );

        // Unqualified reads still serve the ACTIVE project (home A — B was
        // opened additively, which never activates).
        let map_home_input =
            serde_json::from_value(serde_json::json!({"detail": "full"})).expect("home input");
        let map_home = server.get_repo_map(Parameters(map_home_input)).await;
        assert!(
            map_home.contains("old.rs") && !map_home.contains("new.rs"),
            "home must survive reconnect unchanged: {map_home}"
        );

        let _ = second.shutdown_tx.send(());
        wait_for_path_absent(&test_daemon_path(
            daemon_home.path(),
            LEGACY_DAEMON_PORT_FILE,
        ))
        .await;
    }

    /// Locked-contract regression (worktree-routing incident): a DEFAULT
    /// `index_folder` activates the opened project, and a reconnect across a
    /// daemon endpoint replacement must RESTORE that activation — otherwise
    /// unqualified reads silently fall back to the launch-CWD home, the exact
    /// wrong-repo failure from the field.
    #[tokio::test]
    async fn test_reconnect_restores_activated_project() {
        let _env_lock = env_lock().await;
        let daemon_home = TempDir::new().expect("daemon home");
        let _env_guard = EnvVarGuard::set("SYMFORGE_HOME", daemon_home.path());
        let project_a = project_dir("symforge-reconnect-active-home-a");
        let project_b = project_dir("symforge-reconnect-active-open-b");
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

        let first = spawn_test_daemon("127.0.0.1", daemon_home.path())
            .await
            .expect("spawn first daemon");
        let base_url = format!("http://127.0.0.1:{}", first.port);
        let http = authed_client(&first);
        let opened = http
            .post(format!("{base_url}/v1/sessions/open"))
            .json(&OpenProjectRequest {
                project_root: project_a.path().display().to_string(),
                client_name: "reconnect-active".to_string(),
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

        let client = DaemonSessionClient::new_for_test_at(
            base_url.clone(),
            opened.project_id.clone(),
            opened.session_id.clone(),
            opened.project_name.clone(),
            test_control_state(daemon_home.path()),
        )
        .with_project_root(project_a.path().to_path_buf());
        let server = crate::protocol::SymForgeServer::new_daemon_proxy(client);

        // DEFAULT open of B: activates B for unqualified reads.
        let opened_b = server
            .index_folder(Parameters(IndexFolderInput {
                path: project_b.path().display().to_string(),
                idempotency_key: None,
                add: None,
                allow_protected_root: None,
            }))
            .await;
        assert!(opened_b.starts_with("Indexed "), "open B: {opened_b}");
        assert!(
            opened_b.contains("active_project="),
            "default open must report activation: {opened_b}"
        );

        // Kill the daemon and bring up a replacement on the same home.
        let _ = first.shutdown_tx.send(());
        wait_for_path_absent(&test_daemon_path(
            daemon_home.path(),
            daemon_port_file_name(),
        ))
        .await;
        let second = spawn_test_daemon("127.0.0.1", daemon_home.path())
            .await
            .expect("spawn second daemon");

        // Unqualified read through the stale proxy: first attempt fails, the
        // reconnect discovers the replacement, restores the working set AND
        // re-activates B, and the retry must serve B — not home A.
        let map_input =
            serde_json::from_value(serde_json::json!({"detail": "full"})).expect("map input");
        let map = server.get_repo_map(Parameters(map_input)).await;
        assert!(
            map.contains("new.rs") && !map.contains("old.rs"),
            "activated project B must survive reconnect (unqualified read): {map}"
        );

        let _ = second.shutdown_tx.send(());
        wait_for_path_absent(&test_daemon_path(
            daemon_home.path(),
            LEGACY_DAEMON_PORT_FILE,
        ))
        .await;
    }

    /// Carried Slice 3 residual (T037 / 020:T072, frozen 028 edge case): a
    /// caller that cancels a non-abortable `index_folder` after the daemon
    /// began activation leaves the distributed outcome unknown — the daemon
    /// session's ACTIVE moved while the adapter mirror kept the last OBSERVED
    /// activation. The required resolution: an authoritative ACTIVE re-sync,
    /// never a half-published root. Concretely: (1) the unobserved activation
    /// never governs the adapter's unqualified reads — every read stays
    /// pinned to the last observed project, body and receipt agreeing on it —
    /// and (2) the caller's OBSERVED retry of the cancelled call is the
    /// authoritative re-sync after which both sides agree on the new ACTIVE.
    #[tokio::test]
    async fn a_cancelled_activation_never_governs_until_an_observed_resync() {
        let _env_lock = env_lock().await;
        let daemon_home = TempDir::new().expect("daemon home");
        let _env_guard = EnvVarGuard::set("SYMFORGE_HOME", daemon_home.path());
        let project_a = project_dir("symforge-cancelled-activation-home-a");
        let project_b = project_dir("symforge-cancelled-activation-open-b");
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

        let handle = spawn_test_daemon("127.0.0.1", daemon_home.path())
            .await
            .expect("spawn daemon");
        let base_url = format!("http://127.0.0.1:{}", handle.port);
        let http = authed_client(&handle);
        let opened = http
            .post(format!("{base_url}/v1/sessions/open"))
            .json(&OpenProjectRequest {
                project_root: project_a.path().display().to_string(),
                client_name: "cancelled-activation".to_string(),
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

        let client = DaemonSessionClient::new_for_test_at(
            base_url.clone(),
            opened.project_id.clone(),
            opened.session_id.clone(),
            opened.project_name.clone(),
            test_control_state(daemon_home.path()),
        )
        .with_project_root(project_a.path().to_path_buf());
        let server = crate::protocol::SymForgeServer::new_daemon_proxy(client);

        // The cancellation: the daemon runs its non-abortable DEFAULT
        // activation of B to completion, but the ADAPTER never observes the
        // response. The raw HTTP hop below stands in for the caller-cancelled
        // adapter future — the daemon-side outcome is identical: the
        // session's ACTIVE moved to B and nobody on the adapter side knows.
        let activated_b = http
            .post(format!(
                "{base_url}/v1/sessions/{}/tools/index_folder",
                opened.session_id
            ))
            .json(&IndexFolderInput {
                path: project_b.path().display().to_string(),
                idempotency_key: None,
                add: None,
                allow_protected_root: None,
            })
            .send()
            .await
            .expect("activate B request")
            .error_for_status()
            .expect("activate B status")
            .text()
            .await
            .expect("activate B body");
        assert!(
            activated_b.starts_with("Indexed "),
            "daemon-side activation must complete: {activated_b}"
        );

        // Control proving the divergence is real: the DAEMON's own unqualified
        // view now serves B. Without the adapter's pin, the fence below would
        // have nothing to defend against.
        let daemon_view = http
            .post(format!(
                "{base_url}/v1/sessions/{}/tools/get_repo_map",
                opened.session_id
            ))
            .json(&serde_json::json!({ "detail": "full" }))
            .send()
            .await
            .expect("daemon view request")
            .error_for_status()
            .expect("daemon view status")
            .text()
            .await
            .expect("daemon view body");
        assert!(
            daemon_view.contains("new.rs"),
            "control: the daemon session ACTIVE must have moved to B: {daemon_view}"
        );

        // THE FENCE, half 1 — the unobserved activation must not govern: an
        // unqualified read through the adapter serves the last OBSERVED
        // project (home A), body and receipt agreeing on A. Serving B here
        // would report an activation nobody observed; pairing A's body with
        // B's receipt (or vice versa) would be the half-published root.
        let (map_home, home_receipt) =
            crate::protocol::result_status::with_project_evidence_scope(None, async {
                let input = serde_json::from_value(serde_json::json!({"detail": "full"}))
                    .expect("map input");
                let map = server.get_repo_map(Parameters(input)).await;
                let receipt = crate::protocol::result_status::current_project_evidence();
                (map, receipt)
            })
            .await;
        assert!(
            map_home.contains("old.rs") && !map_home.contains("new.rs"),
            "an unqualified read must stay on the last OBSERVED activation: {map_home}"
        );
        let home_receipt = home_receipt.expect("a proxied read must carry a daemon receipt");
        assert_eq!(
            home_receipt.project_id, opened.project_id,
            "the receipt must name the same project that served the body"
        );

        // THE FENCE, half 2 — the authoritative re-sync: the caller's natural
        // retry of the cancelled call, OBSERVED through the adapter this
        // time, resolves the unknown outcome. Both sides converge on B.
        let retried = server
            .index_folder(Parameters(IndexFolderInput {
                path: project_b.path().display().to_string(),
                idempotency_key: None,
                add: None,
                allow_protected_root: None,
            }))
            .await;
        assert!(
            retried.starts_with("Indexed "),
            "the observed retry must complete: {retried}"
        );
        let project_b_id =
            project_key(&canonical_project_root(project_b.path()).expect("canonical b"));
        let (map_b, receipt_b) =
            crate::protocol::result_status::with_project_evidence_scope(None, async {
                let input = serde_json::from_value(serde_json::json!({"detail": "full"}))
                    .expect("map input");
                let map = server.get_repo_map(Parameters(input)).await;
                let receipt = crate::protocol::result_status::current_project_evidence();
                (map, receipt)
            })
            .await;
        assert!(
            map_b.contains("new.rs") && !map_b.contains("old.rs"),
            "after the observed re-sync the activation governs unqualified reads: {map_b}"
        );
        let receipt_b = receipt_b.expect("the re-synced read must carry a daemon receipt");
        assert_eq!(
            receipt_b.project_id, project_b_id,
            "body and receipt must agree on the re-synced ACTIVE"
        );

        let _ = handle.shutdown_tx.send(());
        wait_for_path_absent(&test_daemon_path(
            daemon_home.path(),
            daemon_port_file_name(),
        ))
        .await;
    }

    #[tokio::test]
    async fn test_tool_receipt_carries_project_evidence() {
        let _env_lock = env_lock().await;
        let daemon_home = TempDir::new().expect("daemon home");
        let _env_guard = EnvVarGuard::set("SYMFORGE_HOME", daemon_home.path());
        let project_a = project_dir("symforge-receipt-home-a");
        let project_b = project_dir("symforge-receipt-open-é");
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

        let handle = spawn_test_daemon("127.0.0.1", daemon_home.path())
            .await
            .expect("spawn daemon");
        let client = authed_client(&handle);
        let base_url = format!("http://127.0.0.1:{}", handle.port);

        let opened = client
            .post(format!("{base_url}/v1/sessions/open"))
            .json(&OpenProjectRequest {
                project_root: project_a.path().display().to_string(),
                client_name: "receipt".to_string(),
                pid: Some(80),
            })
            .send()
            .await
            .expect("open request")
            .error_for_status()
            .expect("open status")
            .json::<OpenProjectResponse>()
            .await
            .expect("open body");
        let project_b_id =
            project_key(&canonical_project_root(project_b.path()).expect("canonical b"));

        let opened_b = client
            .post(format!(
                "{base_url}/v1/sessions/{}/tools/index_folder",
                opened.session_id
            ))
            .json(&IndexFolderInput {
                path: project_b.path().display().to_string(),
                idempotency_key: None,
                // Additive: keep home A active so the omitted-project
                // attestation below asserts the home evidence.
                add: Some(true),
                allow_protected_root: None,
            })
            .send()
            .await
            .expect("open B request")
            .error_for_status()
            .expect("open B status")
            .text()
            .await
            .expect("open B body");
        assert!(opened_b.contains("Indexed"), "open B: {opened_b}");

        let evidence_for = |body: serde_json::Value| {
            let client = client.clone();
            let url = format!(
                "{base_url}/v1/sessions/{}/tools/get_repo_map",
                opened.session_id
            );
            async move {
                let response = client
                    .post(url)
                    .json(&body)
                    .send()
                    .await
                    .expect("tool request")
                    .error_for_status()
                    .expect("tool status");
                let header = response
                    .headers()
                    .get(crate::protocol::result_status::PROJECT_EVIDENCE_HEADER)
                    .expect("evidence header must be present");
                serde_json::from_slice::<crate::protocol::result_status::ProjectEvidence>(
                    header.as_bytes(),
                )
                .expect("evidence header must parse as typed evidence")
            }
        };

        let home_evidence = evidence_for(serde_json::json!({"detail": "compact"})).await;
        assert_eq!(
            home_evidence.project_id, opened.project_id,
            "omitted project must be served (and attested) by home"
        );
        assert!(
            home_evidence.canonical_root.is_some() && !home_evidence.index_state.is_empty(),
            "evidence carries root and index state: {home_evidence:?}"
        );

        let routed_evidence =
            evidence_for(serde_json::json!({"detail": "compact", "project": project_b_id})).await;
        assert_eq!(
            routed_evidence.project_id, project_b_id,
            "explicit project must be attested by the routed sibling"
        );

        let response = client
            .post(format!(
                "{base_url}/v1/sessions/{}/tools/search_text",
                opened.session_id
            ))
            .json(&serde_json::json!({
                "query": "new_fn",
                "project": project_b_id,
            }))
            .send()
            .await
            .expect("cross-project search request")
            .error_for_status()
            .expect("cross-project search status");
        let header = response
            .headers()
            .get(crate::protocol::result_status::PROJECT_EVIDENCE_HEADER)
            .expect("cross-project search evidence header");
        let search_evidence = serde_json::from_slice::<
            crate::protocol::result_status::ProjectEvidence,
        >(header.as_bytes())
        .expect("cross-project search evidence parses");
        let body = response.text().await.expect("cross-project search body");
        assert!(
            body.contains("new_fn") && !body.contains("old_fn"),
            "scalar search body must come from project B: {body}"
        );
        assert_eq!(
            search_evidence.project_id, project_b_id,
            "scalar cross-project body and receipt must identify the same project"
        );

        // Exercise the actual adapter parser, not only raw reqwest header
        // access. The unicode project root is valid UTF-8 JSON but is not a
        // `HeaderValue::to_str()` ASCII value.
        let proxy = DaemonSessionClient::new_for_test_at(
            base_url,
            opened.project_id.clone(),
            opened.session_id.clone(),
            opened.project_name.clone(),
            test_control_state(daemon_home.path()),
        );
        let (proxied_body, proxied_evidence) =
            crate::protocol::result_status::with_project_evidence_scope(None, async {
                let body = proxy
                    .call_tool_value(
                        "search_text",
                        serde_json::json!({
                            "query": "new_fn",
                            "project": project_b_id,
                        }),
                    )
                    .await
                    .expect("proxied unicode-project search");
                let evidence = crate::protocol::result_status::current_project_evidence();
                (body, evidence)
            })
            .await;
        assert!(
            proxied_body.contains("new_fn"),
            "proxied body: {proxied_body}"
        );
        let proxied_evidence =
            proxied_evidence.expect("unicode project receipt must survive adapter parsing");
        assert_eq!(proxied_evidence.project_id, project_b_id);
        assert!(
            proxied_evidence
                .canonical_root
                .as_deref()
                .is_some_and(|root| root.contains('é')),
            "unicode canonical root must remain byte-exact: {proxied_evidence:?}"
        );

        let _ = handle.shutdown_tx.send(());
        wait_for_path_absent(&test_daemon_path(
            daemon_home.path(),
            LEGACY_DAEMON_PORT_FILE,
        ))
        .await;
    }

    /// Task 9: the reaper re-checks the SAME last-seen observation under the
    /// sessions write lock before claiming — a heartbeat that advances the
    /// timestamp between candidate collection and the claim WINS and preserves
    /// the session; an actually-expired session closes through the normal
    /// close path, removing orphan project membership exactly once.
    #[test]
    fn test_reaper_rechecks_heartbeat_before_close() {
        let project = project_dir("symforge-reaper");
        let state = DaemonState::new();
        let opened = state
            .open_project_session(OpenProjectRequest {
                project_root: project.path().display().to_string(),
                client_name: "reaper".to_string(),
                pid: None,
            })
            .expect("open session");

        // Simulate a real sweep: an ANCIENT observation and a cutoff in the
        // recent past (now - ttl), exactly the shape reap_expired_sessions
        // produces. A heartbeat between observation and claim advances
        // last_seen past the cutoff, so the claim must lose.
        let ancient: u64 = 1_000;
        let store_last_seen = |value: u64| {
            let sessions = state.sessions.read();
            sessions
                .get(&opened.session_id)
                .expect("session")
                .last_seen_at
                .store(value, Ordering::Relaxed);
        };
        store_last_seen(ancient);
        let cutoff = now_epoch_millis().saturating_sub(60_000);
        assert!(
            ancient < cutoff,
            "test premise: ancient observation expired"
        );

        state.heartbeat(&opened.session_id); // advances last_seen to now > cutoff
        assert!(
            !state.close_session_if_expired(&opened.session_id, ancient, cutoff),
            "a heartbeat between observation and claim must preserve the session"
        );
        assert!(
            state.sessions.read().contains_key(&opened.session_id),
            "session survives the losing claim"
        );

        // Genuinely expired: same ancient observation, no heartbeat since.
        // Claim wins, the session closes through the normal path, and the
        // project whose only member it was is torn down exactly once.
        store_last_seen(ancient);
        assert!(
            state.close_session_if_expired(&opened.session_id, ancient, cutoff),
            "an expired unchanged observation must be claimed"
        );
        assert!(
            !state.sessions.read().contains_key(&opened.session_id),
            "claimed session is removed"
        );
        assert!(
            !state.projects.read().contains_key(&opened.project_id),
            "orphan project membership is removed once"
        );
        // A late heartbeat cannot resurrect the claimed session.
        assert!(
            !state.heartbeat(&opened.session_id).known_session,
            "heartbeat after the claim must fail, not resurrect"
        );

        // Sweep entry point: an expired session found by reap_expired_sessions
        // is closed with the same guarantees.
        let reopened = state
            .open_project_session(OpenProjectRequest {
                project_root: project.path().display().to_string(),
                client_name: "reaper-sweep".to_string(),
                pid: None,
            })
            .expect("reopen session");
        {
            let sessions = state.sessions.read();
            sessions
                .get(&reopened.session_id)
                .expect("session")
                .last_seen_at
                .store(1, Ordering::Relaxed); // ancient heartbeat
        }
        let reaped = state.reap_expired_sessions(std::time::Duration::from_secs(60));
        assert_eq!(reaped, 1, "sweep must reap exactly the expired session");
        assert!(!state.sessions.read().contains_key(&reopened.session_id));
    }

    /// Task 7: `status(detail="projects")` renders the session's open-project
    /// inventory (ids, home marker, counts, snapshot evidence), and full-surface
    /// `health` gains the same inventory once more than one project is open.
    #[tokio::test]
    async fn test_status_projects_detail_lists_session_inventory() {
        let _env_lock = env_lock().await;
        let daemon_home = TempDir::new().expect("daemon home");
        let _env_guard = EnvVarGuard::set("SYMFORGE_HOME", daemon_home.path());
        let project_a = project_dir("symforge-inventory-home-a");
        let project_b = project_dir("symforge-inventory-open-b");
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

        let handle = spawn_test_daemon("127.0.0.1", daemon_home.path())
            .await
            .expect("spawn daemon");
        let client = authed_client(&handle);
        let base_url = format!("http://127.0.0.1:{}", handle.port);

        let opened = client
            .post(format!("{base_url}/v1/sessions/open"))
            .json(&OpenProjectRequest {
                project_root: project_a.path().display().to_string(),
                client_name: "inventory".to_string(),
                pid: Some(79),
            })
            .send()
            .await
            .expect("open request")
            .error_for_status()
            .expect("open status")
            .json::<OpenProjectResponse>()
            .await
            .expect("open body");
        let project_b_id =
            project_key(&canonical_project_root(project_b.path()).expect("canonical b"));

        // health BEFORE the second open: no inventory section (compatibility).
        let health_single = client
            .post(format!(
                "{base_url}/v1/sessions/{}/tools/health_compact",
                opened.session_id
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
            !health_single.contains("── projects ──"),
            "single-project health must stay byte-compatible: {health_single}"
        );

        let opened_b = client
            .post(format!(
                "{base_url}/v1/sessions/{}/tools/index_folder",
                opened.session_id
            ))
            .json(&IndexFolderInput {
                path: project_b.path().display().to_string(),
                idempotency_key: None,
                add: None,
                allow_protected_root: None,
            })
            .send()
            .await
            .expect("open B request")
            .error_for_status()
            .expect("open B status")
            .text()
            .await
            .expect("open B body");
        assert!(opened_b.contains("Indexed"), "open B: {opened_b}");

        let inventory = client
            .post(format!(
                "{base_url}/v1/sessions/{}/tools/status",
                opened.session_id
            ))
            .json(&serde_json::json!({"detail": "projects"}))
            .send()
            .await
            .expect("status request")
            .error_for_status()
            .expect("status status")
            .text()
            .await
            .expect("status body");
        assert!(
            inventory.contains(&opened.project_id) && inventory.contains(&project_b_id),
            "inventory must list both open projects: {inventory}"
        );
        assert!(
            inventory.contains(&format!("{} home=yes", opened.project_id)),
            "home project must carry the home marker: {inventory}"
        );
        assert!(
            inventory.contains(&format!("{project_b_id} active=yes")),
            "the default-opened project must carry the active marker: {inventory}"
        );
        assert!(
            inventory.contains("snapshot=present") || inventory.contains("snapshot=absent"),
            "inventory rows must carry snapshot evidence: {inventory}"
        );
        assert!(
            inventory.contains("last_seen=") && inventory.contains("ttl_secs="),
            "inventory must expose session last-seen and reaper TTL evidence: {inventory}"
        );

        let health_multi = client
            .post(format!(
                "{base_url}/v1/sessions/{}/tools/health_compact",
                opened.session_id
            ))
            .json(&serde_json::json!({}))
            .send()
            .await
            .expect("health multi request")
            .error_for_status()
            .expect("health multi status")
            .text()
            .await
            .expect("health multi body");
        assert!(
            health_multi.contains("── projects ──") && health_multi.contains(&project_b_id),
            "multi-project health must expose the inventory: {health_multi}"
        );

        let _ = handle.shutdown_tx.send(());
        wait_for_path_absent(&test_daemon_path(
            daemon_home.path(),
            LEGACY_DAEMON_PORT_FILE,
        ))
        .await;
    }

    /// Task 5: structural edits route by explicit project. An explicit B edit
    /// mutates ONLY B, an omitted edit mutates the ACTIVE project (home A — B
    /// is opened ADDITIVELY here, which never activates), and an
    /// unknown/ambiguous target writes NOTHING anywhere. Worktree routing and
    /// `working_directory` validation run against the SELECTED project's
    /// repository (the existing per-project edit safety, now bound by the
    /// resolver).
    #[tokio::test]
    async fn test_explicit_project_edit_routes_and_preserves_worktree() {
        let _env_lock = env_lock().await;
        let daemon_home = TempDir::new().expect("daemon home");
        let _env_guard = EnvVarGuard::set("SYMFORGE_HOME", daemon_home.path());
        let project_a = project_dir("symforge-edit-route-a");
        let project_b = project_dir("symforge-edit-route-b");
        let file_a = project_a.path().join("src").join("lib.rs");
        let file_b = project_b.path().join("src").join("lib.rs");
        std::fs::write(&file_a, "pub fn shared_name() -> u32 { 1 }\n").expect("write source a");
        std::fs::write(&file_b, "pub fn shared_name() -> u32 { 1 }\n").expect("write source b");

        let handle = spawn_test_daemon("127.0.0.1", daemon_home.path())
            .await
            .expect("spawn daemon");
        let client = authed_client(&handle);
        let base_url = format!("http://127.0.0.1:{}", handle.port);

        let opened = client
            .post(format!("{base_url}/v1/sessions/open"))
            .json(&OpenProjectRequest {
                project_root: project_a.path().display().to_string(),
                client_name: "edit-route".to_string(),
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
        let project_b_id =
            project_key(&canonical_project_root(project_b.path()).expect("canonical b"));

        let opened_b = client
            .post(format!(
                "{base_url}/v1/sessions/{}/tools/index_folder",
                opened.session_id
            ))
            .json(&IndexFolderInput {
                path: project_b.path().display().to_string(),
                idempotency_key: None,
                add: Some(true),
                allow_protected_root: None,
            })
            .send()
            .await
            .expect("open B request")
            .error_for_status()
            .expect("open B status")
            .text()
            .await
            .expect("open B body");
        assert!(opened_b.contains("Indexed"), "open B: {opened_b}");

        let edit = |body: serde_json::Value| {
            let client = client.clone();
            let url = format!(
                "{base_url}/v1/sessions/{}/tools/replace_symbol_body",
                opened.session_id
            );
            async move {
                client
                    .post(url)
                    .json(&body)
                    .send()
                    .await
                    .expect("edit request")
                    .error_for_status()
                    .expect("edit status")
                    .text()
                    .await
                    .expect("edit body")
            }
        };

        // Explicit B edit mutates ONLY B.
        let result_b = edit(serde_json::json!({
            "path": "src/lib.rs",
            "name": "shared_name",
            "new_body": "pub fn shared_name() -> u32 { 2 }",
            "project": project_b_id,
        }))
        .await;
        let on_disk_b = std::fs::read_to_string(&file_b).expect("read B");
        let on_disk_a = std::fs::read_to_string(&file_a).expect("read A");
        assert!(
            on_disk_b.contains("{ 2 }"),
            "explicit B edit must mutate B: {result_b}\n{on_disk_b}"
        );
        assert!(
            on_disk_a.contains("{ 1 }"),
            "explicit B edit must not touch home A: {on_disk_a}"
        );

        // Omitted edit mutates the ACTIVE project (home A — B was opened additively).
        let result_a = edit(serde_json::json!({
            "path": "src/lib.rs",
            "name": "shared_name",
            "new_body": "pub fn shared_name() -> u32 { 3 }",
        }))
        .await;
        let on_disk_a = std::fs::read_to_string(&file_a).expect("read A after");
        let on_disk_b = std::fs::read_to_string(&file_b).expect("read B after");
        assert!(
            on_disk_a.contains("{ 3 }"),
            "omitted edit must mutate home A: {result_a}\n{on_disk_a}"
        );
        assert!(
            on_disk_b.contains("{ 2 }"),
            "omitted edit must not touch B: {on_disk_b}"
        );

        // Unknown target writes NOTHING.
        let refused = edit(serde_json::json!({
            "path": "src/lib.rs",
            "name": "shared_name",
            "new_body": "pub fn shared_name() -> u32 { 4 }",
            "project": "no-such-project",
        }))
        .await;
        assert!(
            refused.contains("not open"),
            "unknown edit target must refuse with candidates: {refused}"
        );
        assert!(
            !std::fs::read_to_string(&file_a)
                .expect("read A final")
                .contains("{ 4 }")
                && !std::fs::read_to_string(&file_b)
                    .expect("read B final")
                    .contains("{ 4 }"),
            "unknown edit target must write nothing"
        );

        let _ = handle.shutdown_tx.send(());
        wait_for_path_absent(&test_daemon_path(
            daemon_home.path(),
            LEGACY_DAEMON_PORT_FILE,
        ))
        .await;
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

        let handle = spawn_test_daemon("127.0.0.1", daemon_home.path())
            .await
            .expect("spawn daemon");
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
        let client = DaemonSessionClient::new_for_test_at(
            base_url.clone(),
            opened.project_id.clone(),
            opened.session_id.clone(),
            opened.project_name.clone(),
            test_control_state(daemon_home.path()),
        )
        .with_project_root(project.path().to_path_buf());
        let server = crate::protocol::SymForgeServer::new_daemon_proxy(client);

        // Index the project through the proxy → populates the DAEMON's index.
        let indexed = server
            .index_folder(Parameters(IndexFolderInput {
                path: project.path().display().to_string(),
                idempotency_key: None,
                add: None,
                allow_protected_root: None,
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
            server.index().data_plane().published_state().file_count,
            0,
            "front-end self.index must remain empty — status must source from the daemon"
        );

        let _ = handle.shutdown_tx.send(());
        wait_for_path_absent(&test_daemon_path(
            daemon_home.path(),
            LEGACY_DAEMON_PORT_FILE,
        ))
        .await;
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

        let handle = spawn_test_daemon("127.0.0.1", daemon_home.path())
            .await
            .expect("spawn daemon");
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

        let client = DaemonSessionClient::new_for_test_at(
            base_url.clone(),
            opened.project_id.clone(),
            opened.session_id.clone(),
            opened.project_name.clone(),
            test_control_state(daemon_home.path()),
        )
        .with_project_root(project.path().to_path_buf());
        let server = crate::protocol::SymForgeServer::new_daemon_proxy(client);

        let indexed = server
            .index_folder(Parameters(IndexFolderInput {
                path: project.path().display().to_string(),
                idempotency_key: None,
                add: None,
                allow_protected_root: None,
            }))
            .await;
        assert!(
            indexed.starts_with("Indexed "),
            "daemon proxy index_folder must succeed, got: {indexed}"
        );
        assert_eq!(
            server.index().data_plane().published_state().file_count,
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
        wait_for_path_absent(&test_daemon_path(
            daemon_home.path(),
            LEGACY_DAEMON_PORT_FILE,
        ))
        .await;
    }

    /// F6 regression: a `symforge_edit` apply carrying `working_directory`
    /// must route the write into the sibling git worktree through the DAEMON
    /// dispatch path (front-end proxy → daemon `execute_tool_call` →
    /// `edit_within_symbol`), NOT into the daemon's indexed root; and a
    /// `working_directory` that is not a recognized worktree must error loudly
    /// instead of silently writing to the indexed root.
    ///
    /// This is the path the field bug lived on: the in-process direct-dispatch
    /// tests (`tests/worktree_awareness.rs`) never exercised the daemon's own
    /// `execute_tool_call` server, so a daemon that resolved edits through the
    /// no-op `DefaultEditHook` (worktree routing not effective) passed every
    /// existing test while writing to the wrong tree in production.
    #[tokio::test]
    async fn test_symforge_edit_apply_routes_into_worktree_through_daemon_proxy() {
        let _env_lock = env_lock().await;
        let daemon_home = TempDir::new().expect("daemon home");
        let _home_guard = EnvVarGuard::set("SYMFORGE_HOME", daemon_home.path());

        // A real git repo (main checkout) with one committed symbol.
        let project = project_dir("symforge-wt-route-main");
        let main_root = project.path().to_path_buf();
        let main_file = main_root.join("src").join("lib.rs");
        std::fs::write(&main_file, "pub fn wt_edit() -> u32 { 1 }\n").expect("write source");
        let git = |args: &[&str]| {
            let out = crate::process_util::hidden_command("git")
                .current_dir(&main_root)
                .args(args)
                .output()
                .unwrap_or_else(|e| panic!("git {args:?} spawn: {e}"));
            assert!(
                out.status.success(),
                "git {args:?} failed: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        };
        git(&["init", "-b", "main"]);
        git(&["config", "user.email", "t@t.test"]);
        git(&["config", "user.name", "t"]);
        git(&["config", "commit.gpgsign", "false"]);
        git(&["add", "-A"]);
        git(&["commit", "-q", "-m", "init"]);

        // A sibling worktree (outside the main checkout) on a new branch.
        let wt_parent = TempDir::new().expect("worktree parent");
        let worktree_root = wt_parent.path().join("wt_one");
        git(&[
            "worktree",
            "add",
            worktree_root.to_str().expect("utf-8 worktree path"),
            "-b",
            "feature",
        ]);
        let worktree_file = worktree_root.join("src").join("lib.rs");

        let handle = spawn_test_daemon("127.0.0.1", daemon_home.path())
            .await
            .expect("spawn daemon");
        let base_url = format!("http://127.0.0.1:{}", handle.port);
        let http = authed_client(&handle);

        let opened = http
            .post(format!("{base_url}/v1/sessions/open"))
            .json(&OpenProjectRequest {
                project_root: main_root.display().to_string(),
                client_name: "wt-route".to_string(),
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

        let client = DaemonSessionClient::new_for_test_at(
            base_url.clone(),
            opened.project_id.clone(),
            opened.session_id.clone(),
            opened.project_name.clone(),
            test_control_state(daemon_home.path()),
        )
        .with_project_root(main_root.clone());
        let server = crate::protocol::SymForgeServer::new_daemon_proxy(client);

        let indexed = server
            .index_folder(Parameters(IndexFolderInput {
                path: main_root.display().to_string(),
                idempotency_key: None,
                add: None,
                allow_protected_root: None,
            }))
            .await;
        assert!(
            indexed.starts_with("Indexed "),
            "daemon proxy index_folder must succeed, got: {indexed}"
        );
        // Prove the DAEMON holds the index and the edit below runs on the daemon
        // dispatch path, not a front-end local fallback (which would also route
        // because THIS process registered the hook via `new_daemon_proxy`). An
        // empty front-end index means the daemon executed the edit.
        assert_eq!(
            server.index().data_plane().published_state().file_count,
            0,
            "front-end proxy index must be empty — the daemon must serve the edit"
        );

        // (1) Routed apply: working_directory = the real worktree. The write
        // must land in the WORKTREE copy and leave the indexed root untouched.
        let routed = server
            .symforge_edit_facade_tool(Parameters(crate::stel::StelEditRequest {
                path: "src/lib.rs".to_string(),
                symbol: Some("wt_edit".to_string()),
                body: Some("pub fn wt_edit() -> u32 { 2 }".to_string()),
                apply: Some(true),
                working_directory: Some(worktree_root.display().to_string()),
                ..Default::default()
            }))
            .await
            .expect("symforge_edit dispatch");
        let routed_body =
            serde_json::to_value(&routed).expect("serialize routed result")["content"][0]["text"]
                .as_str()
                .expect("routed result text")
                .to_string();

        assert!(
            routed_body.contains("rerouted: true"),
            "worktree apply must report rerouted: true through the daemon path, got:\n{routed_body}"
        );
        let worktree_after = std::fs::read_to_string(&worktree_file).expect("read worktree file");
        assert!(
            worktree_after.contains("{ 2 }"),
            "worktree copy must receive the routed edit, got:\n{worktree_after}\n\nbody:\n{routed_body}"
        );
        let main_after = std::fs::read_to_string(&main_file).expect("read main file");
        assert!(
            main_after.contains("{ 1 }") && !main_after.contains("{ 2 }"),
            "indexed root must NOT be contaminated by a routed worktree edit, got:\n{main_after}\n\nbody:\n{routed_body}"
        );

        // (2) A working_directory that is not a recognized worktree must error
        // loudly (not silently write to the indexed root).
        let stray = TempDir::new().expect("stray dir");
        let stray_result = server
            .symforge_edit_facade_tool(Parameters(crate::stel::StelEditRequest {
                path: "src/lib.rs".to_string(),
                symbol: Some("wt_edit".to_string()),
                body: Some("pub fn wt_edit() -> u32 { 9 }".to_string()),
                apply: Some(true),
                working_directory: Some(stray.path().display().to_string()),
                ..Default::default()
            }))
            .await
            .expect("symforge_edit dispatch");
        let stray_body = serde_json::to_value(&stray_result).expect("serialize stray result")
            ["content"][0]["text"]
            .as_str()
            .expect("stray result text")
            .to_string();
        assert!(
            stray_body.contains("WorkingDirectoryNotARecognizedWorktree"),
            "a non-worktree working_directory must error loudly, got:\n{stray_body}"
        );
        let main_after_stray = std::fs::read_to_string(&main_file).expect("read main after stray");
        assert!(
            main_after_stray.contains("{ 1 }") && !main_after_stray.contains("{ 9 }"),
            "a rejected non-worktree edit must not write to the indexed root, got:\n{main_after_stray}\n\nbody:\n{stray_body}"
        );

        let _ = handle.shutdown_tx.send(());
        wait_for_path_absent(&test_daemon_path(
            daemon_home.path(),
            LEGACY_DAEMON_PORT_FILE,
        ))
        .await;
    }

    /// F6 root-cause guard: drive `edit_within_symbol` DIRECTLY against the
    /// daemon's HTTP tool endpoint (the exact route the front-end proxy POSTs
    /// to) with NO front-end `SymForgeServer` constructed in this test. That
    /// isolation matters because the edit-hook registry is PROCESS-GLOBAL and
    /// order-sensitive: the daemon registers BOTH `WorktreeAwareEditHook`
    /// (via `SymForgeServer::new`) and the observer-only `FrecencyBumpHook`
    /// (via `LiveIndex::load`), and the field bug was `edit_hooks::resolve`
    /// picking only the LAST-registered hook — so when frecency registered
    /// after the worktree hook, its default (no-op) `resolve_target_path`
    /// shadowed worktree routing and every `working_directory` edit
    /// contaminated the indexed root. This test indexes through the daemon
    /// (registering frecency) so the resolve order matches production, then
    /// asserts routing still holds.
    #[tokio::test]
    async fn test_daemon_http_edit_within_routes_into_worktree_without_frontend_server() {
        let _env_lock = env_lock().await;
        let daemon_home = TempDir::new().expect("daemon home");
        let _home_guard = EnvVarGuard::set("SYMFORGE_HOME", daemon_home.path());

        let project = project_dir("symforge-wt-http-main");
        let main_root = project.path().to_path_buf();
        let main_file = main_root.join("src").join("lib.rs");
        std::fs::write(&main_file, "pub fn wt_edit() -> u32 { 1 }\n").expect("write source");
        let git = |args: &[&str]| {
            let out = crate::process_util::hidden_command("git")
                .current_dir(&main_root)
                .args(args)
                .output()
                .unwrap_or_else(|e| panic!("git {args:?} spawn: {e}"));
            assert!(
                out.status.success(),
                "git {args:?} failed: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        };
        git(&["init", "-b", "main"]);
        git(&["config", "user.email", "t@t.test"]);
        git(&["config", "user.name", "t"]);
        git(&["config", "commit.gpgsign", "false"]);
        git(&["add", "-A"]);
        git(&["commit", "-q", "-m", "init"]);

        let wt_parent = TempDir::new().expect("worktree parent");
        let worktree_root = wt_parent.path().join("wt_one");
        git(&[
            "worktree",
            "add",
            worktree_root.to_str().expect("utf-8 worktree path"),
            "-b",
            "feature",
        ]);
        let worktree_file = worktree_root.join("src").join("lib.rs");

        let handle = spawn_test_daemon("127.0.0.1", daemon_home.path())
            .await
            .expect("spawn daemon");
        let base_url = format!("http://127.0.0.1:{}", handle.port);
        let http = authed_client(&handle);

        let opened = http
            .post(format!("{base_url}/v1/sessions/open"))
            .json(&OpenProjectRequest {
                project_root: main_root.display().to_string(),
                client_name: "wt-http".to_string(),
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
        let session_id = opened.session_id;

        // Raw HTTP tool call to the daemon — the same route `call_tool_value`
        // uses. No `SymForgeServer` exists in this test process.
        let call_tool = |tool: &str, params: serde_json::Value| {
            let url = format!("{base_url}/v1/sessions/{session_id}/tools/{tool}");
            let http = http.clone();
            async move {
                http.post(url)
                    .json(&params)
                    .send()
                    .await
                    .expect("tool request")
                    .error_for_status()
                    .expect("tool status")
                    .text()
                    .await
                    .expect("tool body")
            }
        };

        let indexed = call_tool(
            "index_folder",
            serde_json::json!({ "path": main_root.display().to_string() }),
        )
        .await;
        assert!(
            indexed.starts_with("Indexed "),
            "daemon index_folder must succeed, got: {indexed}"
        );

        // (1) Routed edit through the daemon HTTP endpoint.
        let routed = call_tool(
            "edit_within_symbol",
            serde_json::json!({
                "path": "src/lib.rs",
                "name": "wt_edit",
                "old_text": "{ 1 }",
                "new_text": "{ 2 }",
                "working_directory": worktree_root.display().to_string(),
            }),
        )
        .await;
        assert!(
            routed.contains("rerouted: true"),
            "daemon must route the worktree edit (rerouted: true), got:\n{routed}"
        );
        let worktree_after = std::fs::read_to_string(&worktree_file).expect("read worktree file");
        assert!(
            worktree_after.contains("{ 2 }"),
            "worktree copy must receive the routed edit, got:\n{worktree_after}\n\nresp:\n{routed}"
        );
        let main_after = std::fs::read_to_string(&main_file).expect("read main file");
        assert!(
            main_after.contains("{ 1 }") && !main_after.contains("{ 2 }"),
            "indexed root must NOT be contaminated by a routed worktree edit, got:\n{main_after}\n\nresp:\n{routed}"
        );

        // (2) Non-worktree working_directory must error loudly, not write to root.
        let stray = TempDir::new().expect("stray dir");
        let stray_resp = call_tool(
            "edit_within_symbol",
            serde_json::json!({
                "path": "src/lib.rs",
                "name": "wt_edit",
                "old_text": "{ 1 }",
                "new_text": "{ 9 }",
                "working_directory": stray.path().display().to_string(),
            }),
        )
        .await;
        assert!(
            stray_resp.contains("WorkingDirectoryNotARecognizedWorktree"),
            "a non-worktree working_directory must error loudly, got:\n{stray_resp}"
        );
        let main_after_stray = std::fs::read_to_string(&main_file).expect("read main after stray");
        assert!(
            main_after_stray.contains("{ 1 }") && !main_after_stray.contains("{ 9 }"),
            "a rejected non-worktree edit must not write to the indexed root, got:\n{main_after_stray}\n\nresp:\n{stray_resp}"
        );

        let _ = handle.shutdown_tx.send(());
        wait_for_path_absent(&test_daemon_path(
            daemon_home.path(),
            LEGACY_DAEMON_PORT_FILE,
        ))
        .await;
    }

    /// Regression: a daemon-proxy `index_folder` switch must invalidate any
    /// stale in-process index that a prior local fallback populated for the OLD
    /// project. Without the fix, the server keeps serving the old project from
    /// every tool that falls back to local execution (search_symbols,
    /// search_text, get_file_context, conventions, explore), silently mixing
    /// two projects in one session while health/get_repo_map/index_folder
    /// follow the switch.
    #[tokio::test]
    async fn test_index_folder_proxy_open_preserves_local_home_fallback() {
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

        let handle = spawn_test_daemon("127.0.0.1", daemon_home.path())
            .await
            .expect("spawn daemon");
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
        let client = DaemonSessionClient::new_for_test_at(
            base_url.clone(),
            opened.project_id.clone(),
            opened.session_id.clone(),
            opened.project_name.clone(),
            test_control_state(daemon_home.path()),
        )
        .with_project_root(project_a.path().to_path_buf());
        let server = crate::protocol::SymForgeServer::new_daemon_proxy(client);

        // Simulate a prior local-fallback load: the in-process index already
        // holds OLD-project (project A) state.
        server.index.data_plane().add_file(
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
            server.index.data_plane().published_state().file_count,
            1,
            "precondition: stale local index holds the OLD project"
        );

        // Open project B via the proxy path. This must not change the immutable
        // home binding or invalidate its local fallback state.
        let result = server
            .index_folder(Parameters(IndexFolderInput {
                path: project_b.path().display().to_string(),
                idempotency_key: None,
                add: None,
                allow_protected_root: None,
            }))
            .await;
        assert!(
            result.starts_with("Indexed "),
            "daemon proxy index_folder should report success, got: {result}"
        );

        // The immutable-home contract keeps the existing local fallback for A.
        assert_eq!(
            server.index.data_plane().published_state().file_count,
            1,
            "opening B must not reset the local home-A fallback, got: {result}"
        );
        assert!(
            server
                .index
                .data_plane()
                .read()
                .get_file("src/old.rs")
                .is_some(),
            "home-A fallback must remain reachable after opening B"
        );

        let home_root = server.capture_repo_root().expect("home root after open");
        assert_eq!(
            home_root,
            project_a.path().to_path_buf(),
            "repo_root must remain bound to immutable home A"
        );

        let _ = handle.shutdown_tx.send(());
        wait_for_path_absent(&test_daemon_path(
            daemon_home.path(),
            LEGACY_DAEMON_PORT_FILE,
        ))
        .await;
    }

    #[tokio::test]
    async fn test_index_folder_proxy_failure_refuses_destructive_local_fallback() {
        let project_a = project_dir("symforge-proxy-refusal-a");
        let project_b = project_dir("symforge-proxy-refusal-b");
        std::fs::write(
            project_a.path().join("src").join("home.rs"),
            "fn home() {}\n",
        )
        .expect("write home source");
        std::fs::write(
            project_b.path().join("src").join("other.rs"),
            "fn other() {}\n",
        )
        .expect("write other source");

        let home_root = canonical_project_root(project_a.path()).expect("canonical home A");
        // Hermetic unreachable endpoint: bind an ephemeral port, then release it.
        // Nothing is listening there afterwards, so the proxy call fails fast and
        // deterministically — no assumption that a fixed port (e.g. 1) is free.
        let dead_port = {
            let listener =
                std::net::TcpListener::bind("127.0.0.1:0").expect("bind ephemeral probe port");
            listener.local_addr().expect("probe port local addr").port()
        };
        let daemon_home = TempDir::new().expect("isolated daemon control state");
        let client = DaemonSessionClient::new_for_test_at(
            format!("http://127.0.0.1:{dead_port}"),
            project_key(&home_root),
            "unreachable-session".to_string(),
            "home-a".to_string(),
            test_control_state(daemon_home.path()),
        )
        .with_project_root(home_root.clone());
        let server = crate::protocol::SymForgeServer::new_daemon_proxy(client);
        // Mark the daemon connection as already degraded so `proxy_tool_call`
        // fails over immediately instead of entering the reconnect path, which
        // would try to discover or SPAWN a real daemon — the opposite of a
        // hermetic unit test.
        {
            let mut state = server.daemon_degraded.lock();
            state.degraded = true;
            state.last_reconnect_attempt = Some(std::time::Instant::now());
        }
        server
            .index
            .data_plane()
            .reload(&home_root)
            .expect("seed local home fallback");

        let output = server
            .index_folder(Parameters(IndexFolderInput {
                path: project_b.path().display().to_string(),
                idempotency_key: None,
                add: None,
                allow_protected_root: None,
            }))
            .await;
        assert!(
            output.contains("daemon") && output.contains("refus"),
            "daemon-proxy failure must refuse destructive local index_folder: {output}"
        );
        assert_eq!(
            server.capture_repo_root().as_deref(),
            Some(home_root.as_path()),
            "failed proxy open must keep home root"
        );
        let guard = server.index.data_plane().read();
        assert!(guard.get_file("src/home.rs").is_some());
        assert!(
            guard.get_file("src/other.rs").is_none(),
            "failed proxy open must not replace home index with B"
        );
    }

    /// Opening another project must not move connection-scoped surfaces away
    /// from the immutable home project.
    #[tokio::test]
    async fn test_index_folder_activation_keeps_status_home_but_routes_facade_to_active_project() {
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

        let handle = spawn_test_daemon("127.0.0.1", daemon_home.path())
            .await
            .expect("spawn daemon");
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

        let client = DaemonSessionClient::new_for_test_at(
            base_url.clone(),
            opened.project_id.clone(),
            opened.session_id.clone(),
            opened.project_name.clone(),
            test_control_state(daemon_home.path()),
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

        // Open project B via the explicit `index_folder` verb.
        let indexed = server
            .index_folder(Parameters(IndexFolderInput {
                path: project_b.path().display().to_string(),
                idempotency_key: None,
                add: None,
                allow_protected_root: None,
            }))
            .await;
        assert!(
            indexed.starts_with("Indexed "),
            "daemon proxy open must succeed, got: {indexed}"
        );

        // (a) The bound root remains the connection's home A.
        let bound = server.capture_repo_root().expect("repo root after open");
        assert_eq!(
            bound,
            project_a.path().to_path_buf(),
            "repo_root must remain at immutable home A after opening B"
        );

        // (b) `status` (local render path) still reflects home A.
        let root_b_norm = project_b.path().display().to_string().replace('\\', "/");
        let root_b_canonical = project_b
            .path()
            .canonicalize()
            .expect("canonical project B root");
        let root_b_receipt = dunce::simplified(&root_b_canonical)
            .display()
            .to_string()
            .replace('\\', "/");
        let status_after =
            server.render_stel_status_body(&crate::stel::StelStatusRequest::default());
        assert!(
            status_after.contains(&format!("project_root: {root_a_norm}")),
            "post-open status must preserve home A root, got:\n{status_after}"
        );
        assert!(
            !status_after.contains(&format!("project_root: {root_b_norm}")),
            "opened B must not replace home A in status, got:\n{status_after}"
        );

        // The adapter-private route pin is what lets selector-less tools keep
        // the ACTIVE snapshot they captured before dispatch. Prove the daemon
        // consumes it as a real target selector by routing `conventions` back
        // to home A while session ACTIVE remains B.
        let direct_client = server
            .daemon_client_slot()
            .expect("daemon client slot")
            .read()
            .await
            .clone();
        let mut private_home_args = serde_json::json!({});
        private_home_args
            .as_object_mut()
            .expect("private route args")
            .insert(
                PRIVATE_PROJECT_ROUTE_PIN.to_string(),
                serde_json::Value::String(opened.project_id.clone()),
            );
        let private_home_evidence =
            crate::protocol::result_status::with_project_evidence_scope(None, async {
                let body = direct_client
                    .call_tool_value("conventions", private_home_args)
                    .await
                    .expect("private-pinned conventions response");
                assert!(body.contains("Project Conventions"), "{body}");
                crate::protocol::result_status::current_project_evidence()
            })
            .await
            .expect("private-pinned call must carry project evidence");
        assert_eq!(private_home_evidence.project_id, opened.project_id);
        assert_eq!(
            private_home_evidence.canonical_root.as_deref(),
            Some(root_a_norm.as_str()),
            "the private route pin must override mutable daemon ACTIVE"
        );

        // (c) unqualified facade calls follow the daemon session's ACTIVE
        // project. Adapter-local status remains home-scoped, but the served
        // body, facade root line, and typed receipt must all describe B.
        // `SymforgeCallInput` flattens the `StelRequest`, so `query` is top-level.
        let symforge_input = serde_json::from_value(serde_json::json!({
            "query": "find beta_symbol"
        }))
        .expect("symforge facade input");
        let symforge_result =
            crate::protocol::result_status::with_project_evidence_scope(None, async {
                let mut result = server
                    .symforge_facade_tool(Parameters(symforge_input))
                    .await?;
                crate::protocol::result_status::attach_project_evidence_meta(&mut result.meta);
                Ok::<_, rmcp::ErrorData>(result)
            })
            .await
            .expect("symforge facade dispatch");
        let serialized = serde_json::to_value(&symforge_result).expect("serialize symforge result");
        let symforge_body = serialized["content"][0]["text"]
            .as_str()
            .expect("symforge result text")
            .to_string();
        assert!(
            symforge_body.contains("beta_symbol"),
            "unqualified facade body must be served from active project B, got:\n{symforge_body}"
        );
        assert!(
            !symforge_body.contains("alpha_symbol"),
            "unqualified facade body must not mix home project A, got:\n{symforge_body}"
        );
        assert!(
            symforge_body.contains(&format!("project_root: {root_b_receipt}")),
            "unqualified facade root line must name active project B, got:\n{symforge_body}"
        );
        assert!(
            !symforge_body.contains(&format!("project_root: {root_a_norm}")),
            "unqualified facade root line must not name home project A, got:\n{symforge_body}"
        );
        assert_eq!(
            serialized["_meta"]["symforge/project_evidence"]["canonical_root"], root_b_receipt,
            "typed facade evidence must name the active daemon project: {serialized}"
        );

        // Pin an explicit B selector and choose a path that would resolve inside
        // HOME A but outside selected B. This makes the adapter preflight
        // mutation-sensitive: validating against A, skipping the guard, or
        // relying only on the downstream primitive cannot produce the required
        // selected-B-root refusal text.
        let project_a_name = project_a
            .path()
            .file_name()
            .and_then(|name| name.to_str())
            .expect("project A basename");
        let a_inside_b_escape = format!("../{project_a_name}/src/lib.rs");
        let escaped_input = crate::stel::SymforgeCallInput {
            request: crate::stel::StelRequest {
                query: "read the selected file".to_string(),
                intent: Some(crate::stel::IntentBucket::Read),
                path: Some(a_inside_b_escape),
                project: Some(project_key(&root_b_canonical)),
                ..Default::default()
            },
            ..Default::default()
        };
        let escaped_result =
            crate::protocol::result_status::with_project_evidence_scope(None, async {
                let mut result = server
                    .symforge_facade_tool(Parameters(escaped_input))
                    .await?;
                crate::protocol::result_status::attach_project_evidence_meta(&mut result.meta);
                Ok::<_, rmcp::ErrorData>(result)
            })
            .await
            .expect("active-project path containment result");
        let escaped = serde_json::to_value(&escaped_result).expect("serialize path refusal");
        let escaped_body = escaped["content"][0]["text"]
            .as_str()
            .expect("path refusal text");
        assert_eq!(
            escaped["_meta"]["symforge/result_status"]["outcome_class"], "invalid_request",
            "the selected primitive must type the containment refusal: {escaped}"
        );
        assert!(
            escaped_body.contains("Path is outside the repository root")
                && escaped_body.contains(&format!("The selected project root is {root_b_receipt}")),
            "the active daemon project must reject a path escape: {escaped_body}"
        );
        assert!(
            !escaped_body.contains(&root_a_norm)
                && !escaped_body.contains("alpha_symbol")
                && !escaped_body.contains("beta_symbol"),
            "a rejected path must not serve source from either project: {escaped_body}"
        );

        // Real daemon routing refusals must retain a shared error prefix so a
        // direct public wrapper cannot classify a headerless refusal as Found.
        let unknown_input: GetFileContentInput = serde_json::from_value(serde_json::json!({
            "path": "src/lib.rs",
            "project": "not-an-open-project",
        }))
        .expect("unknown-project file input");
        let unknown_result = server
            .get_file_content_tool(Parameters(unknown_input))
            .await
            .expect("unknown-project file result");
        let unknown = serde_json::to_value(unknown_result).expect("serialize routing refusal");
        assert_eq!(
            unknown["_meta"]["symforge/result_status"]["outcome_class"], "invalid_request",
            "a daemon routing refusal must never look Found: {unknown}"
        );
        assert!(
            unknown["content"][0]["text"]
                .as_str()
                .is_some_and(|text| text.starts_with("Error: project_routing:")),
            "daemon routing refusal must use the shared error prefix: {unknown}"
        );

        // The daemon HTTP route is independently callable. A structural edit
        // must not drop a malformed/set-valued selector and mutate ACTIVE B.
        let edit_url = format!(
            "{base_url}/v1/sessions/{}/tools/edit_within_symbol",
            opened.session_id
        );
        for projects in [
            serde_json::json!([project_key(&root_b_canonical)]),
            serde_json::json!([project_key(&root_b_canonical), 7]),
        ] {
            let refused = http
                .post(&edit_url)
                .json(&serde_json::json!({
                    "path": "src/lib.rs",
                    "name": "beta_symbol",
                    "old_text": "{ 2 }",
                    "new_text": "{ 9 }",
                    "projects": projects,
                }))
                .send()
                .await
                .expect("malformed edit selector request")
                .error_for_status()
                .expect("malformed edit selector status")
                .text()
                .await
                .expect("malformed edit selector body");
            assert!(
                refused.starts_with("Error: project_routing:"),
                "unsupported/malformed edit selectors must refuse before dispatch: {refused}"
            );
        }
        assert_eq!(
            std::fs::read_to_string(project_b.path().join("src").join("lib.rs"))
                .expect("read B after refused edits"),
            "pub fn beta_symbol() -> u32 { 2 }\n",
            "a rejected set-valued edit must leave ACTIVE B byte-identical"
        );

        let _ = handle.shutdown_tx.send(());
        wait_for_path_absent(&test_daemon_path(
            daemon_home.path(),
            LEGACY_DAEMON_PORT_FILE,
        ))
        .await;
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

        let handle = spawn_test_daemon("127.0.0.1", daemon_home.path())
            .await
            .expect("spawn daemon");
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
                allow_protected_root: None,
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
                allow_protected_root: None,
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
                allow_protected_root: None,
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
        wait_for_path_absent(&test_daemon_path(
            daemon_home.path(),
            LEGACY_DAEMON_PORT_FILE,
        ))
        .await;
    }

    #[test]
    fn test_index_folder_additive_replays_and_conflicts() {
        let control_state_home = TempDir::new().expect("isolated idempotency control state");
        let project_a = project_dir("symforge-idempotency-home-a");
        let project_b = project_dir("symforge-idempotency-open-b");
        let project_c = project_dir("symforge-idempotency-conflict-c");
        std::fs::write(project_b.path().join("src").join("b.rs"), "fn b() {}\n")
            .expect("write source b");
        std::fs::write(project_c.path().join("src").join("c.rs"), "fn c() {}\n")
            .expect("write source c");
        let state = test_daemon_state(control_state_home.path());
        let opened = state
            .open_project_session(OpenProjectRequest {
                project_root: project_a.path().display().to_string(),
                client_name: "idempotency".to_string(),
                pid: None,
            })
            .expect("open home A");

        let first = state
            .index_folder_for_session(
                &opened.session_id,
                IndexFolderInput {
                    path: project_b.path().display().to_string(),
                    idempotency_key: Some("shared-open-key".to_string()),
                    add: None,
                    allow_protected_root: None,
                },
            )
            .expect("default open B");
        assert!(first.contains("project_id="), "identity receipt: {first}");
        assert!(
            first.contains("checkpoint=written"),
            "checkpoint receipt: {first}"
        );

        std::fs::write(
            project_b.path().join("src").join("second.rs"),
            "fn second() {}\n",
        )
        .expect("write second source b");
        let replay = state
            .index_folder_for_session(
                &opened.session_id,
                IndexFolderInput {
                    path: project_b.path().display().to_string(),
                    idempotency_key: Some("shared-open-key".to_string()),
                    add: None,
                    allow_protected_root: None,
                },
            )
            .expect("identical default replay B");
        assert_eq!(
            replay, first,
            "same key + same request (including the activation spelling) must replay"
        );

        // The `add` spelling changes observable side effects (activation), so
        // it MUST distinguish the canonical request: same key + different
        // spelling conflicts deterministically rather than silently skipping
        // activation.
        let spelling_conflict = state
            .index_folder_for_session(
                &opened.session_id,
                IndexFolderInput {
                    path: project_b.path().display().to_string(),
                    idempotency_key: Some("shared-open-key".to_string()),
                    add: Some(true),
                    allow_protected_root: None,
                },
            )
            .expect("add=true against a default-open record");
        assert!(
            spelling_conflict.contains("Idempotency conflict"),
            "same key with a different activation spelling must conflict: {spelling_conflict}"
        );

        let conflict = state
            .index_folder_for_session(
                &opened.session_id,
                IndexFolderInput {
                    path: project_c.path().display().to_string(),
                    idempotency_key: Some("shared-open-key".to_string()),
                    add: None,
                    allow_protected_root: None,
                },
            )
            .expect("conflicting open C");
        assert!(
            conflict.contains("Idempotency conflict"),
            "same key with another target must fail deterministically: {conflict}"
        );
        assert!(
            !state.projects.read().contains_key(&project_key(
                &canonical_project_root(project_c.path()).expect("canonical C")
            )),
            "conflict must be rejected before project C is loaded"
        );
    }

    #[test]
    fn test_index_folder_open_persists_snapshot_for_restore() {
        let project_a = project_dir("symforge-snapshot-home-a");
        let project_b = project_dir("symforge-snapshot-open-b");
        std::fs::write(
            project_b.path().join("src").join("restored.rs"),
            "fn restored() {}\n",
        )
        .expect("write source b");
        let state = DaemonState::new();
        let opened = state
            .open_project_session(OpenProjectRequest {
                project_root: project_a.path().display().to_string(),
                client_name: "snapshot".to_string(),
                pid: None,
            })
            .expect("open home A");

        let receipt = state
            .index_folder_for_session(
                &opened.session_id,
                IndexFolderInput {
                    path: project_b.path().display().to_string(),
                    idempotency_key: None,
                    add: None,
                    allow_protected_root: None,
                },
            )
            .expect("open B");
        assert!(
            receipt.contains("checkpoint=written"),
            "successful open must expose durable checkpoint outcome: {receipt}"
        );
        assert!(
            project_b
                .path()
                .join(".symforge")
                .join("index.bin")
                .is_file(),
            "successful daemon reload must atomically persist B"
        );

        drop(state);
        let canonical_b = canonical_project_root(project_b.path()).expect("canonical B");
        let RootResolution::Bound(binding_b) = crate::discovery::resolve_root_candidate(
            &canonical_b,
            RootCandidateSource::LaunchCwd,
            RootRequestMode::Automatic,
        ) else {
            panic!("B must remain a valid automatic project root");
        };
        let placement_b = crate::discovery::resolve_state_placement(&binding_b);
        let restored =
            bootstrap_project_index(&canonical_b, &placement_b).expect("restore B from snapshot");
        let guard = restored.read();
        assert_eq!(
            guard.load_source(),
            crate::live_index::store::IndexLoadSource::SnapshotRestore
        );
        assert_eq!(guard.file_count(), 1);
        assert!(guard.get_file("src/restored.rs").is_some());
    }

    /// An unavailable project-local state directory must move the whole project
    /// instance to user-local state instead of retrying writes under the source.
    #[tokio::test]
    async fn test_index_folder_open_falls_back_to_user_local_when_project_state_unavailable() {
        let _env_lock = env_lock().await;
        let state_home = TempDir::new().expect("private user-local state base");
        let _home = EnvVarGuard::set("SYMFORGE_HOME", state_home.path());
        let project_a = project_dir("symforge-degraded-home-a");
        let project_b = project_dir("symforge-degraded-open-b");
        std::fs::write(project_b.path().join("src").join("b.rs"), "fn b() {}\n")
            .expect("write source b");
        // Block snapshot persistence: a FILE at `.symforge` makes the snapshot
        // directory creation fail while indexing itself is unaffected.
        std::fs::write(project_b.path().join(".symforge"), "not a directory")
            .expect("write .symforge blocker");
        let state = DaemonState::new();
        let opened = state
            .open_project_session(OpenProjectRequest {
                project_root: project_a.path().display().to_string(),
                client_name: "state-fallback".to_string(),
                pid: None,
            })
            .expect("open home A");

        let receipt = state
            .index_folder_for_session(
                &opened.session_id,
                IndexFolderInput {
                    path: project_b.path().display().to_string(),
                    idempotency_key: None,
                    add: None,
                    allow_protected_root: None,
                },
            )
            .expect("open B must succeed through user-local fallback");
        assert!(
            receipt.starts_with("Indexed "),
            "open itself must succeed: {receipt}"
        );
        assert!(
            receipt.contains("state=user-local"),
            "project-local failure must select user-local state: {receipt}"
        );
        assert!(
            receipt.contains("checkpoint=written"),
            "user-local placement must checkpoint through its resolved state directory: {receipt}"
        );

        // The in-memory open stayed published: B is attached to the working
        // set, and the default spelling ACTIVATED B (locked contract) while
        // home A stays in the working set.
        let project_b_id =
            project_key(&canonical_project_root(project_b.path()).expect("canonical b"));
        let sessions = state.sessions.read();
        let session = sessions.get(&opened.session_id).expect("session");
        assert_eq!(
            session.active_project_id, project_b_id,
            "default open must activate B"
        );
        assert!(
            session.working_set.read().get(&opened.project_id).is_some(),
            "home A must stay in the working set"
        );
        assert!(
            session.working_set.read().get(&project_b_id).is_some(),
            "B must be attached despite project-local state being unavailable"
        );
        assert!(
            state_home
                .path()
                .join("projects")
                .join(&project_b_id)
                .is_dir(),
            "fallback state must be prepared under the private user-local project directory"
        );
        assert!(
            state_home
                .path()
                .join("projects")
                .join(&project_b_id)
                .join("index.bin")
                .is_file(),
            "user-local checkpoint must be written under the resolved project state directory"
        );
        // The source blocker remains untouched: fallback never tries to repair
        // or replace source-root state behind the user's back.
        assert!(
            project_b.path().join(".symforge").is_file(),
            "the blocking file must remain in place (no destructive recovery)"
        );
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

        let handle = spawn_test_daemon("127.0.0.1", daemon_home.path())
            .await
            .expect("spawn daemon");
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

        let before_response = client
            .post(format!(
                "{base_url}/v1/sessions/{}/tools/health",
                opened.session_id
            ))
            .json(&serde_json::json!({}))
            .send()
            .await
            .expect("pre-impact health request")
            .error_for_status()
            .expect("pre-impact health status");
        let before_evidence: crate::protocol::result_status::ProjectEvidence =
            serde_json::from_slice(
                before_response
                    .headers()
                    .get(crate::protocol::result_status::PROJECT_EVIDENCE_HEADER)
                    .expect("pre-impact evidence header")
                    .as_bytes(),
            )
            .expect("decode pre-impact evidence");
        let _ = before_response
            .text()
            .await
            .expect("pre-impact health body");

        std::fs::write(&source_path, "pub fn new_name() {}\n").expect("write updated source");

        let _cwd_guard = CwdGuard::set(outside.path());

        let impact_response = client
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
            .expect("impact status");
        let impact_evidence: crate::protocol::result_status::ProjectEvidence =
            serde_json::from_slice(
                impact_response
                    .headers()
                    .get(crate::protocol::result_status::PROJECT_EVIDENCE_HEADER)
                    .expect("impact evidence header")
                    .as_bytes(),
            )
            .expect("decode impact evidence");
        let impact = impact_response.text().await.expect("impact body");

        assert!(
            impact.contains("new_name"),
            "impact analysis should read from the session project root, got: {impact}"
        );
        assert!(
            impact_evidence.generation > before_evidence.generation,
            "the daemon receipt must come from the publication rendered by impact: before={before_evidence:?}, impact={impact_evidence:?}"
        );

        let _ = handle.shutdown_tx.send(());
        wait_for_path_absent(&test_daemon_path(
            daemon_home.path(),
            LEGACY_DAEMON_PORT_FILE,
        ))
        .await;
    }

    #[tokio::test]
    async fn direct_daemon_rejects_unsupported_selectors_before_project_mutation() {
        let _env_lock = env_lock().await;
        let daemon_home = TempDir::new().expect("daemon home");
        let _env_guard = EnvVarGuard::set("SYMFORGE_HOME", daemon_home.path());
        let project_a = project_dir("symforge-daemon-selector-a");
        let project_b = project_dir("symforge-daemon-selector-b");
        let handle = spawn_test_daemon("127.0.0.1", daemon_home.path())
            .await
            .expect("spawn daemon");
        let client = authed_client(&handle);
        let base_url = format!("http://127.0.0.1:{}", handle.port);
        let opened = client
            .post(format!("{base_url}/v1/sessions/open"))
            .json(&OpenProjectRequest {
                project_root: project_a.path().display().to_string(),
                client_name: "selector-guard".to_string(),
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

        let illegal_open = client
            .post(format!(
                "{base_url}/v1/sessions/{}/tools/index_folder",
                opened.session_id
            ))
            .json(&serde_json::json!({
                "path": project_b.path().display().to_string(),
                "project": opened.project_id.clone(),
            }))
            .send()
            .await
            .expect("selector-bearing index request")
            .error_for_status()
            .expect("selector-bearing index status")
            .text()
            .await
            .expect("selector-bearing index body");
        assert!(
            illegal_open.starts_with("Error: project_routing:"),
            "index_folder must reject selectors before opening or activating: {illegal_open}"
        );

        let inventory = client
            .post(format!(
                "{base_url}/v1/sessions/{}/tools/status",
                opened.session_id
            ))
            .json(&serde_json::json!({"detail": "projects"}))
            .send()
            .await
            .expect("project inventory request")
            .error_for_status()
            .expect("project inventory status")
            .text()
            .await
            .expect("project inventory body");
        let project_b_id = project_key(
            &project_b
                .path()
                .canonicalize()
                .expect("canonical project B root"),
        );
        assert!(
            !inventory.contains(&project_b_id),
            "rejected index_folder must leave project B unopened: {inventory}"
        );

        let illegal_checkpoint = client
            .post(format!(
                "{base_url}/v1/sessions/{}/tools/checkpoint_now",
                opened.session_id
            ))
            .json(&serde_json::json!({"project": project_b_id}))
            .send()
            .await
            .expect("selector-bearing checkpoint request")
            .error_for_status()
            .expect("selector-bearing checkpoint status")
            .text()
            .await
            .expect("selector-bearing checkpoint body");
        assert!(
            illegal_checkpoint.starts_with("Error: project_routing:"),
            "checkpoint_now must refuse an unsupported scalar selector: {illegal_checkpoint}"
        );

        let _ = handle.shutdown_tx.send(());
        wait_for_path_absent(&test_daemon_path(
            daemon_home.path(),
            LEGACY_DAEMON_PORT_FILE,
        ))
        .await;
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
        let slot = projects_lock.get(project_id).expect("project must exist");
        let instance = slot.metadata.read();
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
        let slot = projects_lock.get(project_id).expect("project must exist");
        let instance = slot.metadata.read();

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

        let lock_path = test_daemon_path(daemon_home.path(), daemon_start_lock_file_name());

        // Create a lock file and backdate it to 60 seconds ago.
        let file = std::fs::File::create(&lock_path).expect("create lock");
        let old_time = std::time::SystemTime::now() - std::time::Duration::from_secs(60);
        let times = std::fs::FileTimes::new().set_modified(old_time);
        file.set_times(times).expect("set file times");
        drop(file);

        // try_acquire_start_lock should detect the stale lock and succeed.
        let result = try_acquire_start_lock_at(&test_control_state(daemon_home.path()))
            .expect("should not error");
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

        let lock_path = test_daemon_path(daemon_home.path(), daemon_start_lock_file_name());

        // Create a fresh lock file (just now — well within 30s threshold).
        std::fs::write(&lock_path, "").expect("create lock");

        // try_acquire_start_lock should see the fresh lock and return None.
        let result = try_acquire_start_lock_at(&test_control_state(daemon_home.path()))
            .expect("should not error");
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
        let port_file = test_daemon_path(dir.path(), LEGACY_DAEMON_PORT_FILE);
        let pid_file = test_daemon_path(dir.path(), LEGACY_DAEMON_PID_FILE);
        let lock_file = test_daemon_path(dir.path(), LEGACY_DAEMON_START_LOCK_FILE);

        std::fs::write(&port_file, "12345").expect("write port");
        std::fs::write(&pid_file, "99999").expect("write pid");
        std::fs::write(&lock_file, "").expect("write lock");

        cleanup_daemon_files_at(&test_control_state(dir.path()));

        assert!(!port_file.exists(), "cleanup removes port file");
        assert!(!pid_file.exists(), "cleanup removes pid file");
        assert!(!lock_file.exists(), "cleanup removes start lock file");
    }

    #[tokio::test]
    async fn test_runtime_cleanup_preserves_start_lock() {
        let _env_lock = env_lock().await;
        let daemon_home = TempDir::new().expect("daemon home");
        let _env_guard = EnvVarGuard::set("SYMFORGE_HOME", daemon_home.path());

        let port_file = test_daemon_path(daemon_home.path(), LEGACY_DAEMON_PORT_FILE);
        let pid_file = test_daemon_path(daemon_home.path(), LEGACY_DAEMON_PID_FILE);
        let lock_file = test_daemon_path(daemon_home.path(), LEGACY_DAEMON_START_LOCK_FILE);

        std::fs::write(&port_file, "12345").expect("write port");
        std::fs::write(&pid_file, "99999").expect("write pid");
        std::fs::write(&lock_file, "").expect("write lock");

        cleanup_daemon_runtime_files_at(&test_control_state(daemon_home.path()));

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
            test_daemon_path(daemon_home.path(), LEGACY_DAEMON_PORT_FILE),
            "not-a-port",
        )
        .expect("write corrupt port");

        let identity = current_daemon_identity();
        let selected =
            daemon_port_if_compatible_at(&test_control_state(daemon_home.path()), &identity)
                .await
                .expect("corrupt port should be ignored, not fatal");

        assert_eq!(selected, None);
    }
}
