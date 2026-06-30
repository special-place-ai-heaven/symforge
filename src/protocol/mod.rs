pub mod ccr;
pub mod conventions;
pub(crate) mod edit;
pub(crate) mod edit_format;
pub mod edit_hooks;
pub mod edit_plan;
pub(crate) mod edit_tools;
pub mod explore;
pub mod format;
pub mod investigation;
pub mod prompts;
pub(crate) mod read_tools;
pub mod resources;
pub mod result_status;
pub(crate) mod search_format;
pub(crate) mod search_tools;
pub mod session;
pub mod smart_query;
pub mod surface_probe;
pub mod tools;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use parking_lot::{Mutex, RwLock};

use rmcp::RoleServer;
use rmcp::handler::server::router::prompt::PromptRouter;
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::model::{
    ListResourceTemplatesResult, ListResourcesResult, ListToolsResult, PaginatedRequestParams,
    ReadResourceRequestParams, ReadResourceResult, ServerCapabilities, ServerInfo, Tool,
};
use rmcp::service::RequestContext;
use rmcp::{ServerHandler, prompt_handler, tool_handler};

use crate::live_index::SharedIndex;
use crate::protocol::result_status::OutcomeClass;
use crate::sidecar::TokenStats;
use crate::watcher::WatcherInfo;

/// Tracks in-flight durable-ledger `spawn_blocking` writes so [`crate::server::serve::run`]
/// can drain them on shutdown instead of dropping economics events scheduled
/// just before SIGINT/SIGTERM (review finding P2-3).
///
/// Dependency-free: an atomic in-flight counter polled by [`Self::drain`]. Each
/// scheduled write holds a [`LedgerWriteGuard`] whose `Drop` decrements the
/// counter when the blocking write returns.
#[cfg(feature = "server")]
#[derive(Debug, Default)]
pub struct LedgerWriteTracker {
    in_flight: std::sync::atomic::AtomicUsize,
}

#[cfg(feature = "server")]
impl LedgerWriteTracker {
    /// Register a write about to be scheduled; the returned guard decrements the
    /// in-flight count on drop (i.e. when the blocking write completes).
    fn begin(self: &std::sync::Arc<Self>) -> LedgerWriteGuard {
        self.in_flight
            .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        LedgerWriteGuard(std::sync::Arc::clone(self))
    }

    /// Number of durable writes still in flight.
    fn pending(&self) -> usize {
        self.in_flight.load(std::sync::atomic::Ordering::Acquire)
    }

    /// Wait until every scheduled durable write completes, or `timeout` elapses
    /// (whichever comes first). Polls the in-flight counter; on timeout it logs
    /// the residual count rather than blocking shutdown indefinitely.
    pub async fn drain(&self, timeout: std::time::Duration) {
        let deadline = std::time::Instant::now() + timeout;
        while self.pending() != 0 {
            if std::time::Instant::now() >= deadline {
                tracing::warn!(
                    pending = self.pending(),
                    "durable ledger drain timed out; some economics events may be unwritten"
                );
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }
}

/// Guard returned by [`LedgerWriteTracker::begin`]; decrements the in-flight
/// count when the blocking durable write completes (on drop).
#[cfg(feature = "server")]
struct LedgerWriteGuard(std::sync::Arc<LedgerWriteTracker>);

#[cfg(feature = "server")]
impl Drop for LedgerWriteGuard {
    fn drop(&mut self) {
        self.0
            .in_flight
            .fetch_sub(1, std::sync::atomic::Ordering::AcqRel);
    }
}

#[cfg(all(test, feature = "server"))]
mod ledger_write_tracker_tests {
    use super::LedgerWriteTracker;
    use std::sync::Arc;
    use std::time::Duration;

    #[tokio::test]
    async fn drain_returns_after_pending_write_completes() {
        let tracker = Arc::new(LedgerWriteTracker::default());
        let guard = tracker.begin();
        assert_eq!(tracker.pending(), 1);
        // Release the guard shortly; drain must wait for it, then return.
        let t2 = Arc::clone(&tracker);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(20)).await;
            drop(guard);
            let _ = &t2;
        });
        tracker.drain(Duration::from_secs(2)).await;
        assert_eq!(
            tracker.pending(),
            0,
            "drain returned with a write still pending"
        );
    }

    #[tokio::test]
    async fn drain_times_out_without_hanging_on_a_stuck_write() {
        let tracker = Arc::new(LedgerWriteTracker::default());
        let _held = tracker.begin(); // never released => simulates a stuck write
        let start = std::time::Instant::now();
        tracker.drain(Duration::from_millis(50)).await;
        assert!(
            start.elapsed() >= Duration::from_millis(50),
            "drain returned before the timeout elapsed"
        );
        assert_eq!(tracker.pending(), 1, "stuck write must still be counted");
    }
}
/// The MCP server struct.
///
/// Holds a `SharedIndex` and a `ToolRouter` generated by the `#[tool_router]` macro.
/// Holds a `PromptRouter` generated by the `#[prompt_router]` macro.
/// The `project_name` field is passed through to `format::repo_outline`.
/// The `watcher_info` field is read by the health tool to report live watcher state.
/// The `repo_root` field is used by `index_folder` to restart the watcher at a new root.
/// The `token_stats` field provides live hook token savings to the health tool (AD-4 simpler path).
#[derive(Clone)]
pub struct SymForgeServer {
    pub(crate) index: SharedIndex,
    pub(crate) tool_router: ToolRouter<Self>,
    pub(crate) prompt_router: PromptRouter<Self>,
    pub(crate) project_name: String,
    pub(crate) watcher_info: Arc<Mutex<WatcherInfo>>,
    /// MUST NOT be held across `.await`. Use `.lock().take()` and
    /// `.lock().replace(...)` around async work; `parking_lot::Mutex` is
    /// non-async and held-across-await would deadlock the runtime.
    /// Some only in local-stdio mode where this server owns its watcher.
    /// None in daemon-proxy mode and daemon-degraded mode.
    pub(crate) watcher_handle: Arc<Mutex<Option<crate::watcher::WatcherTaskHandle>>>,
    /// Root directory the watcher is currently watching. Stored in shared mutable
    /// state so local stdio tools can keep using the latest project root after
    /// `index_folder` rebinds the server to a new workspace.
    pub(crate) repo_root: Arc<RwLock<Option<PathBuf>>>,
    /// Shared token stats from the HTTP sidecar. Present when the sidecar is running.
    /// The health tool reads this to display token savings accumulated during the session.
    pub(crate) token_stats: Option<Arc<TokenStats>>,
    /// Optional daemon-backed proxy session. When present, tool calls are forwarded to the
    /// shared daemon-owned project runtime instead of the local in-process index.
    /// Wrapped in `tokio::sync::RwLock` so reconnection can swap in a fresh client.
    pub(crate) daemon_client: Option<Arc<tokio::sync::RwLock<crate::daemon::DaemonSessionClient>>>,
    /// Set to `true` after a failed reconnection attempt. While degraded,
    /// calls make one success probe but avoid repeated reconnect attempts.
    pub(crate) daemon_degraded: Arc<AtomicBool>,
    /// Session context tracking: records what the LLM has fetched this session.
    pub(crate) session_context: Arc<session::SessionContext>,
    /// Per-session CCR blob store for reversible bulk compression (011).
    pub(crate) ccr_store: Arc<Mutex<ccr::CcrStore>>,
    /// In-memory STEL L4 ledger for compact `symforge` invocations (no persistence yet).
    pub(crate) stel_ledger: Arc<Mutex<crate::stel::ledger::SessionLedger>>,
    /// Tracks edit-tool calls that omitted `working_directory` while the
    /// transitional worktree observability knob was on. Surfaced by the
    /// `health` tool as a rolling "last hour" signal.
    pub(crate) worktree_misuse: Arc<crate::worktree::WorktreeMisuseCounter>,
    /// Bounded analytics queue. Disabled by default until an explicit local
    /// analytics configuration installs an enabled recorder.
    pub(crate) analytics_recorder: Arc<RwLock<crate::analytics::AnalyticsRecorder>>,
    /// Durable STEL L4 economics ledger. `Some` when `serve::run` (US3) or the
    /// stdio/daemon-proxy bootstrap (US1 T020/T021) wires the opened store in;
    /// `None` otherwise. The same `Arc` is shared with `ServerRuntime` so there
    /// is exactly one durable ledger path (no economics double-count). When
    /// `Some` it may be [`crate::stel::ledger_store::StelLedgerStore::Disabled`]
    /// if the DB could not open — write-through then degrades to a logged no-op
    /// (FR-011).
    ///
    /// Server-gated: embed durability is DEFERRED — `protocol` is crate-root
    /// server-gated (`src/lib.rs`), so an `any(server, embed)` cfg here would be
    /// dead under embed. Reaching the store from embed needs a structural
    /// protocol-free seam (out of US1 scope; spec FR-001 note).
    #[cfg(feature = "server")]
    pub(crate) stel_ledger_store: Option<Arc<crate::stel::ledger_store::StelLedgerStore>>,
    /// In-flight durable-ledger write tracker (P2-3): lets `serve::run` drain
    /// pending `spawn_blocking` economics writes on shutdown so events accepted
    /// just before SIGINT/SIGTERM are not lost.
    #[cfg(feature = "server")]
    pub(crate) ledger_writes: Arc<LedgerWriteTracker>,
}

fn default_analytics_db_path(repo_root: Option<&Path>) -> PathBuf {
    repo_root
        .map(|root| crate::paths::symforge_db_path(root, crate::paths::ANALYTICS_DB_NAME))
        // No project root: fall back to a relative `.symforge/analytics.db`,
        // byte-identical to the prior `PathBuf::from(SYMFORGE_ANALYTICS_DB_PATH)`.
        .unwrap_or_else(|| {
            Path::new(crate::paths::SYMFORGE_DIR_NAME).join(crate::paths::ANALYTICS_DB_NAME)
        })
}

fn disabled_analytics_recorder(repo_root: Option<&Path>) -> crate::analytics::AnalyticsRecorder {
    crate::analytics::AnalyticsRecorder::disabled(default_analytics_db_path(repo_root))
}

fn estimate_tokens(response_bytes: u64) -> u64 {
    response_bytes.saturating_add(3) / 4
}

impl SymForgeServer {
    pub(crate) fn tool_router() -> ToolRouter<Self> {
        Self::core_tool_router() + Self::edit_tool_router()
    }

    /// Create a new server with the given shared index, project name, and watcher state.
    ///
    /// `token_stats` is optional — when `Some`, the health tool will include a token savings
    /// section showing per-hook-type fire counts and estimated tokens saved this session.
    pub fn new(
        index: SharedIndex,
        project_name: String,
        watcher_info: Arc<Mutex<WatcherInfo>>,
        repo_root: Option<PathBuf>,
        token_stats: Option<Arc<TokenStats>>,
    ) -> Self {
        crate::worktree::register_if_feature_enabled();
        let analytics_recorder = disabled_analytics_recorder(repo_root.as_deref());
        Self {
            index,
            tool_router: Self::tool_router(),
            prompt_router: Self::prompt_router(),
            project_name,
            watcher_info,
            watcher_handle: Arc::new(Mutex::new(None)),
            repo_root: Arc::new(RwLock::new(repo_root)),
            token_stats,
            daemon_client: None,
            daemon_degraded: Arc::new(AtomicBool::new(false)),
            session_context: Arc::new(session::SessionContext::new()),
            ccr_store: Arc::new(Mutex::new(ccr::CcrStore::new())),
            stel_ledger: Arc::new(Mutex::new(crate::stel::ledger::SessionLedger::new())),
            worktree_misuse: Arc::new(crate::worktree::WorktreeMisuseCounter::new()),
            analytics_recorder: Arc::new(RwLock::new(analytics_recorder)),
            #[cfg(feature = "server")]
            stel_ledger_store: None,
            #[cfg(feature = "server")]
            ledger_writes: Arc::new(LedgerWriteTracker::default()),
        }
    }

    pub fn new_daemon_proxy(daemon_client: crate::daemon::DaemonSessionClient) -> Self {
        use crate::watcher::WatcherInfo;

        crate::worktree::register_if_feature_enabled();
        let project_name = daemon_client.project_name().to_string();
        let repo_root = daemon_client.project_root().map(|p| p.to_path_buf());
        let analytics_recorder = disabled_analytics_recorder(repo_root.as_deref());
        Self {
            index: crate::live_index::LiveIndex::empty(),
            tool_router: Self::tool_router(),
            prompt_router: Self::prompt_router(),
            project_name,
            watcher_info: Arc::new(Mutex::new(WatcherInfo::default())),
            watcher_handle: Arc::new(Mutex::new(None)),
            repo_root: Arc::new(RwLock::new(repo_root)),
            token_stats: None,
            daemon_client: Some(Arc::new(tokio::sync::RwLock::new(daemon_client))),
            daemon_degraded: Arc::new(AtomicBool::new(false)),
            session_context: Arc::new(session::SessionContext::new()),
            ccr_store: Arc::new(Mutex::new(ccr::CcrStore::new())),
            stel_ledger: Arc::new(Mutex::new(crate::stel::ledger::SessionLedger::new())),
            worktree_misuse: Arc::new(crate::worktree::WorktreeMisuseCounter::new()),
            analytics_recorder: Arc::new(RwLock::new(analytics_recorder)),
            #[cfg(feature = "server")]
            stel_ledger_store: None,
            #[cfg(feature = "server")]
            ledger_writes: Arc::new(LedgerWriteTracker::default()),
        }
    }

    /// Attach a durable STEL economics ledger store to this server (US3/T028).
    ///
    /// Builder-style; consumed by `server::serve::build_serve_runtime` so the
    /// same `Arc<StelLedgerStore>` is shared with `ServerRuntime`. This is the
    /// single durable ledger path — the in-memory `SessionLedger` write and the
    /// durable write-through both happen in `finalize_symforge_with_ledger`, so
    /// no economics row is counted twice.
    #[cfg(feature = "server")]
    pub fn with_stel_ledger_store(
        mut self,
        store: Arc<crate::stel::ledger_store::StelLedgerStore>,
    ) -> Self {
        self.stel_ledger_store = Some(store);
        self
    }

    /// Write one finalized ledger event through to the durable store (US3/T028).
    ///
    /// Called from `finalize_symforge_with_ledger` AFTER the in-memory
    /// `SessionLedger` push. The durable write degrades to a logged no-op on a
    /// store error and never fails the request (FR-011): `StelLedgerStore::record`
    /// no-ops when `Disabled` and logs-and-continues on an insert error. On
    /// stdio/embed builds (`server` feature off) this is a compile-time no-op.
    ///
    /// P2-C (resolved — non-blocking durable write): `StelLedgerStore::record`
    /// takes a `std::sync::Mutex<Connection>` and runs a sync INSERT under an
    /// up-to-5000ms busy-timeout. Running that inline on the tokio worker serving
    /// `/mcp` could stall concurrent HTTP clients. So when a tokio runtime is
    /// present we offload the blocking write onto `spawn_blocking` (the store is
    /// behind `Arc`, the event is cloned), and the request task returns without
    /// waiting on the DB lock. The in-memory `SessionLedger` push stays
    /// synchronous/immediate in `finalize_symforge_with_ledger`; only this
    /// durable write moves off the hot path. When no runtime is present (sync
    /// tests / embed call sites) we record synchronously so events are never
    /// lost. Either way `record` degrades silently on store error.
    #[cfg(feature = "server")]
    fn persist_ledger_event_durably(&self, event: &crate::stel::types::StelLedgerEvent) {
        let Some(store) = self.stel_ledger_store.as_ref() else {
            return;
        };
        match tokio::runtime::Handle::try_current() {
            Ok(handle) => {
                // Offload the blocking SQLite write off the request task so the
                // DB lock / busy-timeout never stalls the tokio worker serving
                // MCP. Fire-and-forget: the durable write is best-effort and
                // degrades silently inside `record` on any store error.
                let store = Arc::clone(store);
                let event = event.clone();
                // P2-3: register the write so serve::run can drain in-flight
                // durable writes on shutdown instead of dropping them.
                let guard = self.ledger_writes.begin();
                handle.spawn_blocking(move || {
                    let _guard = guard; // decrements the in-flight count on completion
                    store.record(&event);
                    // T031: after the new sample lands, run the tuning pass and
                    // persist an accepted candidate — off the hot path, in the
                    // same blocking task, so the auto-tune never stalls the MCP
                    // worker. Degrades silently on any store error.
                    Self::maybe_persist_tuning(&store);
                });
            }
            Err(_) => {
                // No tokio runtime (sync test / embed context): record inline so
                // events are never dropped. There is no async worker to protect
                // here, so blocking is acceptable.
                store.record(event);
                Self::maybe_persist_tuning(store);
            }
        }
    }

    /// The in-flight durable-ledger write tracker (P2-3), exposed so
    /// `serve::run` can drain pending economics writes on shutdown.
    #[cfg(feature = "server")]
    pub(crate) fn ledger_write_tracker(&self) -> &Arc<LedgerWriteTracker> {
        &self.ledger_writes
    }

    /// Run a tuning pass over the durable samples and persist an accepted
    /// candidate (feature 013, T031 / FR-008 audited gated action).
    ///
    /// Reads the current-estimator samples, derives + held-out-validates a
    /// candidate `response_correction_factor` against the correction currently in
    /// force (identity `1.0`, or the active tuning's factor — the D13 hysteresis
    /// anchor), and on an accepted `Tuned` verdict writes it via
    /// `store_active_tuning` with the audit fields (factor, sample_size,
    /// error_before/after, tuned_at). Idempotent: when the accepted candidate's
    /// factor equals the already-stored one, it is NOT re-written (no SQLite
    /// churn, no oscillation). Non-blocking off the hot path (called inside the durable
    /// write's `spawn_blocking`); a `Disabled`/absent store degrades to a no-op
    /// and never serves a bad tuning. NO frecency bump (Principle V).
    #[cfg(feature = "server")]
    fn maybe_persist_tuning(store: &crate::stel::ledger_store::StelLedgerStore) {
        use crate::stel::calibration::{
            CalibrationVerdict, NO_CORRECTION_FACTOR, PredictionSample, compute_calibration_verdict,
        };
        use crate::stel::controller::active_tuning_in_force;
        use crate::stel::ledger_store::{CURRENT_ESTIMATOR_VERSION, LEDGER_RETENTION_MAX};

        // Newest-first current-version samples (excludes pre-013). Bounded by the
        // retention cap so the pass is O(cap) at worst.
        let Ok(records) =
            store.samples_for_estimator(CURRENT_ESTIMATOR_VERSION, LEDGER_RETENTION_MAX)
        else {
            return;
        };
        let samples: Vec<PredictionSample> = records.iter().map(PredictionSample::from).collect();

        // In-force correction factor = the active tuning's if present (D13
        // hysteresis anchor: a re-tune must beat the correction already LIVE),
        // else the identity 1.0 (no tuning). The validate gate scores a candidate
        // against this, so a re-tune must out-perform what is already applied.
        let active = store
            .load_active_tuning(CURRENT_ESTIMATOR_VERSION)
            .ok()
            .flatten();
        let in_force = active_tuning_in_force(active.clone(), CURRENT_ESTIMATOR_VERSION);
        let in_force_factor = in_force
            .as_ref()
            .map_or(NO_CORRECTION_FACTOR, |c| c.response_correction_factor);

        let (verdict, candidate) = compute_calibration_verdict(&samples, in_force_factor);
        if !matches!(verdict, CalibrationVerdict::Tuned { .. }) {
            return;
        }
        let Some(mut candidate) = candidate else {
            return;
        };

        // Idempotence / oscillation guard: if the accepted candidate's correction
        // equals what is already stored, do not re-write (the validate gate already
        // requires a >= margin beat over the in-force factor, so this only fires on
        // an exact-equal stored factor — pure churn avoidance).
        if let Some(existing) = active.as_ref()
            && existing.response_correction_factor == candidate.response_correction_factor
        {
            return;
        }

        candidate.tuned_at_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        // Audited gated action (FR-008): store_active_tuning persists the new
        // correction factor, sample_size, error_before/after (the held-out real
        // residual baseline + corrected), tuned_at. Degrades silently on a store
        // error (never fails a request; never a bad tuning).
        if let Err(error) = store.store_active_tuning(&candidate) {
            tracing::warn!(error = %error, "stel tuning persist failed; keeping prior constants");
        } else {
            tracing::info!(
                response_correction_factor = candidate.response_correction_factor,
                sample_size = candidate.sample_size,
                error_before = candidate.error_before,
                error_after = candidate.error_after,
                "stel auto-tune accepted: persisted calibrated correction (013 US2)"
            );
        }
    }

    /// The validated tuned-constant set currently IN FORCE for the current
    /// estimator version (feature 013, T032 / FR-006), or `None`.
    ///
    /// Loads the active tuning from the durable store and applies the R3 in-force
    /// rule via [`active_tuning_in_force`]: a tuning whose `estimator_version`
    /// does not match the current estimator is NOT returned, so a stale-version
    /// set never silently applies. A `Disabled`/absent store yields `None` and
    /// the economics fall back to the static floors — never serves a bad tuning
    /// (FR-003). Read-only; no frecency bump (Principle V).
    ///
    /// [`active_tuning_in_force`]: crate::stel::controller::active_tuning_in_force
    #[cfg(feature = "server")]
    pub(crate) fn active_tuning_for_economics(
        &self,
    ) -> Option<crate::stel::ledger_store::TunedEstimateConstants> {
        use crate::stel::controller::active_tuning_in_force;
        use crate::stel::ledger_store::CURRENT_ESTIMATOR_VERSION;

        let store = self.stel_ledger_store.as_ref()?;
        let loaded = store.load_active_tuning(CURRENT_ESTIMATOR_VERSION).ok()?;
        active_tuning_in_force(loaded, CURRENT_ESTIMATOR_VERSION)
    }

    /// Compute the honest [`CalibrationVerdict`] from the DURABLE calibration
    /// state for the `status` surface (feature 013, T033 / FR-009).
    ///
    /// `Tuned` is returned ONLY when an active tuning is in force AND it carries a
    /// before/after held-out error artifact (`error_before > error_after`),
    /// reading the artifact straight off the persisted set so the surface never
    /// claims `tuned` without it. Otherwise the verdict reflects the durable
    /// sample count (`Deferred` / `Accumulating(n/min)`). When no durable store is
    /// wired, returns `None` so the caller keeps the in-memory view. When a wired
    /// store cannot answer the sample query, pins the verdict to `Deferred` so a
    /// stale in-memory session cannot claim `Tuned` while no validated tuning is
    /// readable. Read-only; no frecency bump.
    ///
    /// [`CalibrationVerdict`]: crate::stel::calibration::CalibrationVerdict
    #[cfg(feature = "server")]
    pub(crate) fn durable_calibration_verdict(
        &self,
    ) -> Option<crate::stel::calibration::CalibrationVerdict> {
        use crate::stel::calibration::{CalibrationVerdict, TUNING_MIN_CORPUS};
        use crate::stel::ledger_store::{CURRENT_ESTIMATOR_VERSION, LEDGER_RETENTION_MAX};

        let store = self.stel_ledger_store.as_ref()?;
        // A wired but failing store cannot prove any tuning is in force.
        let records =
            match store.samples_for_estimator(CURRENT_ESTIMATOR_VERSION, LEDGER_RETENTION_MAX) {
                Ok(records) => records,
                Err(_) => return Some(CalibrationVerdict::Deferred),
            };
        let n = records.len();

        // An active, in-force tuning with a real reduction artifact reads `Tuned`.
        if let Some(active) = self.active_tuning_for_economics()
            && active.error_before > active.error_after
        {
            return Some(CalibrationVerdict::Tuned {
                sample_size: active.sample_size as usize,
                error_before: active.error_before,
                error_after: active.error_after,
            });
        }

        if n == 0 {
            Some(CalibrationVerdict::Deferred)
        } else {
            Some(CalibrationVerdict::Accumulating {
                n,
                min: TUNING_MIN_CORPUS,
            })
        }
    }

    /// Clear accumulated calibration for the current estimator (feature 013,
    /// T037 / FR-011 operator reset). Returns the number of sample rows cleared,
    /// or `None` when no durable store is wired. Never rebuilds the index.
    #[cfg(feature = "server")]
    pub(crate) fn reset_calibration(&self) -> Option<usize> {
        use crate::stel::ledger_store::CURRENT_ESTIMATOR_VERSION;
        let store = self.stel_ledger_store.as_ref()?;
        store
            .clear_calibration_for_estimator(CURRENT_ESTIMATOR_VERSION)
            .ok()
    }

    #[cfg(not(feature = "server"))]
    #[inline]
    fn persist_ledger_event_durably(&self, _event: &crate::stel::types::StelLedgerEvent) {}

    /// Durable-ledger subsystem state for the `status` tool (US3/T029
    /// restart-survival; N-3 / TR-17 / FR-008).
    ///
    /// Maps the wired durable store's [`subsystem_state`] onto the
    /// feature-independent surface enum. Reports `Unavailable` when no store is
    /// attached, `Disabled { reason }` for a wired-but-failing store (open failed
    /// at startup or live query failed — N-3, never swallowed), and `Durable`
    /// otherwise. On embed builds (`server` feature off) this is a compile-time
    /// `Unavailable`.
    ///
    /// Reachability note (honest scope of N-3/FR-008): a durable store is wired by
    /// `server::serve::run` (the `/mcp` surface) AND by the stdio + daemon-proxy
    /// bootstrap (US1 T020/T021). On the daemon-proxy `status` path the proxy
    /// itself holds the store, so `status_stel_tool` overlays THIS value (among
    /// all proxy-owned lines) onto the proxied worker body
    /// (`overlay_proxy_status_lines`) — the operator sees the proxy's real
    /// `Durable`/`Disabled` state, not the storeless worker's `unavailable`
    /// (D2-ROOT). It stays `Unavailable` only when no store is attached anywhere.
    ///
    /// [`subsystem_state`]: crate::stel::ledger_store::StelLedgerStore::subsystem_state
    #[cfg(feature = "server")]
    fn durable_ledger_summary_for_status(&self) -> crate::stel::status::DurableLedgerState {
        use crate::stel::ledger_store::LedgerSubsystemState;
        use crate::stel::status::{DurableLedgerState, DurableLedgerSummary};

        let Some(store) = self.stel_ledger_store.as_ref() else {
            return DurableLedgerState::Unavailable;
        };
        match store.subsystem_state() {
            LedgerSubsystemState::Durable { summary } => {
                DurableLedgerState::Durable(DurableLedgerSummary {
                    total_events: summary.total_events,
                    total_net_vs_manual: summary.total_net_vs_manual,
                    session_count: summary.session_count,
                })
            }
            LedgerSubsystemState::Disabled { reason } => DurableLedgerState::Disabled { reason },
        }
    }

    #[cfg(not(feature = "server"))]
    #[inline]
    fn durable_ledger_summary_for_status(&self) -> crate::stel::status::DurableLedgerState {
        crate::stel::status::DurableLedgerState::Unavailable
    }

    /// Accessor for tests.
    pub fn index(&self) -> &SharedIndex {
        &self.index
    }

    /// Accessor for STEL ledger integration tests.
    #[doc(hidden)]
    pub fn stel_ledger(&self) -> &Arc<Mutex<crate::stel::ledger::SessionLedger>> {
        &self.stel_ledger
    }

    /// Return the MCP tool definitions advertised by this server.
    ///
    /// This is a read-only view over the generated router metadata so integration
    /// tests and diagnostics can validate client compatibility without spinning up
    /// a full stdio transport.
    pub fn tool_definitions() -> Vec<Tool> {
        Self::tool_router().list_all()
    }

    pub(crate) fn capture_repo_root(&self) -> Option<PathBuf> {
        self.repo_root.read().clone()
    }

    /// Record a frecency bump for the given paths against the bound workspace.
    ///
    /// No-op when no repo root is bound (the feature has nothing to anchor
    /// the per-workspace store to). Forwards to
    /// [`crate::live_index::frecency::bump`], which itself resolves session,
    /// persistent, or disabled collection policy.
    pub(crate) fn bump_frecency(&self, paths: &[PathBuf]) {
        if let Some(root) = self.capture_repo_root() {
            crate::live_index::frecency::bump(&root, paths);
        }
    }

    pub(crate) fn effective_repo_root_for_git_tools(&self) -> Option<PathBuf> {
        self.capture_repo_root()
            .or_else(crate::discovery::find_project_root)
    }

    pub(crate) fn set_repo_root(&self, repo_root: Option<PathBuf>) {
        *self.repo_root.write() = repo_root.clone();
        if self.analytics_recorder.read().status().disabled {
            *self.analytics_recorder.write() = disabled_analytics_recorder(repo_root.as_deref());
        }
    }

    /// Bump the worktree-awareness misuse counter when an edit handler
    /// was called without `working_directory` while worktree routing is active.
    /// No-op when the caller supplied the parameter or policy disables routing.
    /// The counter surfaces in `health` output as a rolling "last hour" signal
    /// for callers that should already be passing worktree routing context.
    pub(crate) fn note_worktree_misuse_if_active(&self, working_directory: Option<&str>) {
        if working_directory.is_none()
            && crate::worktree::routing_policy_from_env()
                == crate::capability::WorktreeRoutingPolicy::ExplicitCallTime
        {
            self.worktree_misuse.record_missing_working_directory();
        }
    }

    /// Record token savings from an MCP tool response so the health counter accumulates.
    pub(crate) fn record_read_savings(&self, saved_tokens: u64) {
        if let Some(ref stats) = self.token_stats {
            stats
                .read_saved_tokens
                .fetch_add(saved_tokens, std::sync::atomic::Ordering::Relaxed);
            stats
                .read_fires
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
    }

    /// Record token savings from a named MCP tool, tracking per-tool breakdown.
    pub(crate) fn record_tool_savings_named(
        &self,
        tool_name: &str,
        estimated_raw_tokens: u64,
        output_tokens: u64,
    ) {
        if let Some(ref stats) = self.token_stats {
            let saved = estimated_raw_tokens.saturating_sub(output_tokens);
            stats
                .read_saved_tokens
                .fetch_add(saved, std::sync::atomic::Ordering::Relaxed);
            stats
                .read_fires
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            stats.record_tool_tokens(tool_name, output_tokens, saved);
        }
    }

    /// Apply token budget with CCR offload for eligible discovery tools (011).
    pub(crate) fn apply_ccr_budget(
        &self,
        tool_name: &str,
        result: String,
        max_tokens: Option<u64>,
    ) -> String {
        let budget = ccr::resolve_tool_max_tokens(tool_name, max_tokens);
        let Some(tokens) = budget else {
            return result;
        };
        if ccr::profile_for_tool(tool_name).is_some_and(|p| p.ccr_eligible)
            && result.len() > tokens as usize * 4
        {
            let summary = format::enforce_token_budget(result.clone(), Some(tokens));
            let mut store = self.ccr_store.lock();
            return ccr::apply_ccr_overflow(&mut store, tool_name, summary, result, tokens);
        }
        format::enforce_token_budget(result, budget)
    }

    pub(crate) fn format_read_cache_hit(
        &self,
        meta: &crate::protocol::session::SessionCacheHitMeta,
        reason: &str,
    ) -> String {
        self.session_context.record_cache_hit();
        format::format_session_cache_hit_body(meta, reason)
    }

    pub(crate) fn compression_economics(&self) -> crate::protocol::ccr::CcrEconomics {
        self.ccr_store.lock().economics()
    }

    pub fn session_compression_heuristic(
        &self,
    ) -> crate::protocol::ccr::SessionCompressionHeuristic {
        let cache_hits = self.session_context.snapshot().cache_hit_count;
        crate::protocol::ccr::SessionCompressionHeuristic::from_parts(
            cache_hits,
            self.compression_economics(),
        )
    }

    pub(crate) fn record_tool_completion(
        &self,
        tool_name: &'static str,
        response_text: &str,
        duration: Duration,
        outcome_class: OutcomeClass,
    ) {
        let recorder = self.analytics_recorder.read().clone();
        let response_bytes = response_text.len() as u64;
        let observation = crate::analytics::AnalyticsObservation::new(
            tool_name,
            crate::analytics::AnalyticsSurface::Tool,
            recorder.configured_scope(),
            response_bytes,
            Some(estimate_tokens(response_bytes)),
            duration,
            !outcome_class.is_error(),
            outcome_class,
        );
        let _ = recorder.enqueue(observation);
    }

    #[cfg(test)]
    pub(crate) fn set_analytics_recorder_for_tests(
        &self,
        recorder: crate::analytics::AnalyticsRecorder,
    ) {
        *self.analytics_recorder.write() = recorder;
    }

    #[cfg(test)]
    pub(crate) fn analytics_queue_status_for_tests(
        &self,
    ) -> crate::analytics::AnalyticsQueueStatus {
        self.analytics_recorder.read().status()
    }

    /// Forward a tool call to the daemon. Returns:
    /// - `Some(result)` on success
    /// - `None` on connection failure (after one reconnect attempt), signalling
    ///   the caller to fall through to local execution
    pub(crate) async fn proxy_tool_call<T>(&self, tool_name: &str, params: &T) -> Option<String>
    where
        T: serde::Serialize,
    {
        let daemon_lock = self.daemon_client.as_ref()?;

        let was_degraded = self.daemon_degraded.load(Ordering::Relaxed);

        let value = match serde_json::to_value(params) {
            Ok(value) => value,
            Err(error) => return Some(format!("Daemon proxy serialization failed: {error}")),
        };

        // First attempt.
        // IMPORTANT: The read lock on daemon_lock is held for the duration of
        // the HTTP call. If this hangs, it blocks reconnect (which needs a
        // write lock). We wrap in a timeout so read locks release promptly
        // on failure — the reqwest client also has its own timeout as a
        // backstop, but this ensures lock release within a tighter window.
        {
            let client = daemon_lock.read().await;
            match tokio::time::timeout(
                std::time::Duration::from_secs(10),
                client.call_tool_value(tool_name, value.clone()),
            )
            .await
            {
                Ok(Ok(result)) => {
                    self.daemon_degraded.store(false, Ordering::Relaxed);
                    return Some(result);
                }
                Ok(Err(error)) => {
                    tracing::warn!(
                        tool = tool_name,
                        "daemon proxy call failed, attempting reconnect: {error}"
                    );
                }
                Err(_elapsed) => {
                    tracing::warn!(
                        tool = tool_name,
                        "daemon proxy call timed out after 10s, attempting reconnect"
                    );
                }
            }
        }

        if was_degraded {
            tracing::warn!(
                tool = tool_name,
                "daemon proxy probe failed while degraded, falling back to local execution"
            );
            self.ensure_local_index().await;
            return None;
        }

        // Reconnect attempt — take a write lock so only one caller does this.
        let reconnected = {
            let mut client = daemon_lock.write().await;
            match client.reconnect().await {
                Ok(new_client) => {
                    tracing::info!("daemon reconnected successfully");
                    *client = new_client;
                    true
                }
                Err(reconnect_error) => {
                    tracing::warn!(
                        "daemon reconnect failed, falling back to local execution: {reconnect_error}"
                    );
                    self.daemon_degraded.store(true, Ordering::Relaxed);
                    self.ensure_local_index().await;
                    false
                }
            }
        };

        if !reconnected {
            return None;
        }

        // Retry with the fresh client.
        {
            let client = daemon_lock.read().await;
            match tokio::time::timeout(
                std::time::Duration::from_secs(30),
                client.call_tool_value(tool_name, value),
            )
            .await
            {
                Ok(Ok(result)) => {
                    self.daemon_degraded.store(false, Ordering::Relaxed);
                    Some(result)
                }
                Ok(Err(error)) => {
                    tracing::warn!(
                        tool = tool_name,
                        "daemon proxy call failed after reconnect, falling back to local: {error}"
                    );
                    self.daemon_degraded.store(true, Ordering::Relaxed);
                    self.ensure_local_index().await;
                    None
                }
                Err(_elapsed) => {
                    tracing::warn!(
                        tool = tool_name,
                        "daemon proxy call timed out after reconnect, falling back to local"
                    );
                    self.daemon_degraded.store(true, Ordering::Relaxed);
                    self.ensure_local_index().await;
                    None
                }
            }
        }
    }

    pub(crate) async fn proxy_tool_call_without_params(&self, tool_name: &str) -> Option<String> {
        self.proxy_tool_call(tool_name, &serde_json::json!({}))
            .await
    }

    /// Lazily initialize the local index when the daemon is unreachable.
    /// Uses `SharedIndexHandle::reload` to populate the empty in-process index
    /// from disk, enabling graceful degradation to local-only mode.
    ///
    /// Root-mismatch invalidation: the early return fires only when the index is
    /// non-empty AND was built from the SAME root we are now targeting. If the
    /// target `repo_root` changed (a project switch via any `set_repo_root`
    /// caller) the recorded root no longer matches, so we fall through and
    /// reload from the current root instead of serving the previous project.
    /// This closes the stale-project class structurally — no caller has to
    /// remember to call `reset_to_empty` for correctness (that path still works
    /// and remains as defense in depth). Both roots are normalized via the same
    /// [`normalize_root`](crate::live_index::store::normalize_root) helper so the
    /// steady state (same project, repeated calls) never reloads on a cosmetic
    /// `\\?\` / trailing-slash / separator / case difference.
    async fn ensure_local_index(&self) {
        {
            let mut watcher = self.watcher_info.lock();
            *watcher = WatcherInfo::detached_local_fallback();
        }

        let repo_root = self.capture_repo_root();

        // Early return only when the index is non-empty AND its recorded root
        // matches the current target root (normalized compare). A non-empty
        // index built from a different root is stale: fall through to reload.
        let published = self.index.published_state();
        if published.file_count > 0 {
            let target_root = repo_root
                .as_deref()
                .map(crate::live_index::store::normalize_root);
            match (&published.indexed_root, &target_root) {
                // Same project: serve the loaded index, no reload.
                (Some(indexed), Some(target)) if indexed == target => return,
                // No target root to compare against (cannot do better than the
                // already-loaded index): keep serving it rather than dropping to
                // an empty index we cannot repopulate.
                (_, None) => return,
                // Root mismatch (or an index with no recorded root): stale —
                // fall through and reload from the current target root.
                _ => {}
            }
        }

        if let Some(root) = repo_root {
            tracing::info!(
                root = %root.display(),
                "daemon unreachable — loading local index as fallback"
            );
            match self.index.reload(&root) {
                Ok(()) => {
                    let published = self.index.published_state();
                    tracing::info!(
                        files = published.file_count,
                        symbols = published.symbol_count,
                        "local fallback index loaded"
                    );

                    // Spawn git temporal computation so co-change queries work
                    // after daemon degradation (mirrors index_folder behaviour).
                    let expected_gen = self.index.current_project_generation();
                    crate::live_index::git_temporal::spawn_git_temporal_computation(
                        Arc::clone(&self.index),
                        root,
                        expected_gen,
                    );
                }
                Err(error) => {
                    tracing::error!("failed to load local fallback index: {error}");
                }
            }
        } else {
            tracing::warn!("daemon unreachable and no repo root available for local fallback");
        }
    }

    /// Bind the workspace from MCP client-declared roots when nothing else did.
    ///
    /// Called from [`ServerHandler::on_initialized`] after the client sends
    /// `notifications/initialized`, this queries the client's `roots/list` and
    /// resolves a workspace root with strict precedence:
    /// `SYMFORGE_WORKSPACE_ROOT` > client roots > launch-CWD walk
    /// ([`crate::discovery::resolve_workspace_root`]).
    ///
    /// Precedence is honored against the *already-bound* server state rather
    /// than re-deciding it: startup resolved the env override and the CWD walk
    /// before the transport came up. The env override always wins — if it is set
    /// and valid the bound root came from it and this is a deliberate no-op.
    ///
    /// Two paths let declared client roots bind/retarget despite an existing
    /// binding, both narrowly gated (see the body):
    ///   - the keystone home-CWD case where neither env nor CWD bound anything
    ///     (Cursor from a home directory: `repo_root: None`, empty index); and
    ///   - (012 D4-A) a daemon-proxy session that came up bound to a CWD-derived
    ///     root with NO env override — there client roots are authoritative
    ///     (`roots > CWD`) and retarget the live session, fixing the wrong-repo
    ///     binding that a stale launch CWD would otherwise pin forever.
    ///
    /// In both cases the resolved root drives the existing [`Self::index_folder`]
    /// path, which loads the local in-process index or, in a daemon-proxy server,
    /// proxies the rebind to `index_folder_for_session` (the per-session retarget).
    /// A local (non-proxy) server that is already bound still returns immediately,
    /// preserving its loaded index byte-for-byte.
    ///
    /// Failures are logged and swallowed: a client that declares no roots, an
    /// unreachable `roots/list`, or a forbidden root must never break the
    /// session — the server simply stays in its existing (empty) state, exactly
    /// as before this hook existed.
    // ponytail: SEP-2577 deprecates the entire MCP Roots capability (Root,
    // ListRootsResult, peer.list_roots()) with NO replacement in rmcp 2.0 — it is
    // slated for removal from the spec, not renamed. `roots/list` is still the only
    // way a client can declare its workspace, so we keep using it and scope the
    // allow to this fn. Upgrade path: when the spec settles on a successor
    // capability (or rmcp removes Roots), migrate this single function.
    // https://github.com/modelcontextprotocol/modelcontextprotocol/pull/2577
    #[allow(deprecated)]
    async fn bind_workspace_from_client_roots(&self, peer: &rmcp::Peer<RoleServer>) {
        // Precedence: `SYMFORGE_WORKSPACE_ROOT (env) > client roots > CWD walk`.
        //
        // Two cases let declared client roots win over an already-bound root, both
        // gated so the single-harness happy path and the local (non-daemon) loaded
        // index are never disturbed:
        //
        //   1. Nothing was bound at startup (env + CWD both resolved nothing):
        //      client roots are the only signal — bind from them. This is the
        //      original home-CWD launcher fix (Cursor) and is unconditional.
        //
        //   2. (012 D4-A, per-connection RETARGET) A daemon-proxy session is
        //      ALWAYS bound at startup (to whatever the launch CWD resolved), so
        //      case 1 never fires for it and a stale CWD binding would silently
        //      win forever — the #1 field wrong-repo bug. When the bound root did
        //      NOT come from the env override, the client's declared roots are
        //      authoritative (`roots > CWD`) and must retarget the session via the
        //      existing `index_folder` → `index_folder_for_session` path.
        //
        // The env override always wins (case skipped) so `env > roots` holds, and
        // a local (non-proxy) server keeps its exact prior behavior: it returns
        // here whenever it is already bound, never reloading a loaded index.
        if self.capture_repo_root().is_some() {
            let bound_from_env = crate::discovery::workspace_root_env_override().is_some();
            let is_daemon_proxy = self.daemon_client.is_some();
            if bound_from_env || !is_daemon_proxy {
                return;
            }
            tracing::debug!(
                "daemon-proxy bound from CWD (no env override); allowing client roots to retarget"
            );
        }

        // Spec correctness: only issue `roots/list` when the client actually
        // declared the `roots` capability at `initialize`. A client that did not
        // advertise roots (capabilities `{}`) has no obligation to answer a
        // `roots/list` request and may never reply, which would hang this hook
        // (and the session) indefinitely. `peer_info()` is the client's stored
        // `InitializeRequestParams`; absent or roots-less capabilities => skip.
        let declares_roots = peer
            .peer_info()
            .map(|info| info.capabilities.roots.is_some())
            .unwrap_or(false);
        if !declares_roots {
            tracing::debug!("client did not declare the roots capability; workspace stays unbound");
            return;
        }

        // Defense in depth: even a roots-declaring client could stall. Bound the
        // request so a non-answering peer cannot hang the session — on timeout we
        // simply leave the workspace unbound, identical to the no-roots path.
        let roots = match tokio::time::timeout(std::time::Duration::from_secs(5), peer.list_roots())
            .await
        {
            Ok(Ok(result)) => result.roots,
            Ok(Err(error)) => {
                // Client declined / transport error. Stay unbound (pre-hook behavior).
                tracing::debug!(%error, "client roots/list failed; workspace stays unbound");
                return;
            }
            Err(_elapsed) => {
                tracing::warn!(
                    "client roots/list did not respond within 5s; workspace stays unbound"
                );
                return;
            }
        };

        if roots.is_empty() {
            tracing::debug!("client declared no roots; workspace stays unbound");
            return;
        }

        let root_uris: Vec<String> = roots.into_iter().map(|root| root.uri).collect();

        // env is None here: reaching this point guarantees the bound root did
        // NOT come from the env override (the gate above returns early when
        // `workspace_root_env_override().is_some()`), so passing None cannot let
        // a client root jump ahead of the env override — `env > roots` holds.
        let Some(resolved) = crate::discovery::resolve_workspace_root(None, &root_uris, None)
        else {
            tracing::info!(
                roots = ?root_uris,
                "no usable workspace among client roots (forbidden/unparseable); staying unbound"
            );
            return;
        };

        tracing::info!(
            root = %resolved.display(),
            "binding workspace from MCP client roots (no env/CWD root at startup)"
        );

        // Drive the existing index path. `index_folder` handles both the local
        // in-process reload (this server) and, in a daemon-proxy server, the
        // proxied session rebind — so no new index plumbing is introduced.
        let input = crate::protocol::tools::IndexFolderInput {
            path: resolved.display().to_string(),
            idempotency_key: None,
            add: None,
        };
        let result = self
            .index_folder(rmcp::handler::server::wrapper::Parameters(input))
            .await;
        tracing::info!(outcome = %result, "workspace bind from client roots complete");
    }

    /// Test-only dispatcher that routes a tool call by name and JSON payload.
    ///
    /// Mirrors the tool-name match in `daemon::execute_tool_call`, but takes
    /// `&self` so parity tests can exercise handlers directly without a live
    /// daemon or MCP transport. Only the tools needed by the `tests/` suites
    /// are wired up here.
    #[doc(hidden)]
    pub async fn dispatch_tool_for_tests(
        &self,
        tool_name: &str,
        params: serde_json::Value,
    ) -> String {
        use rmcp::handler::server::wrapper::Parameters;
        fn decode<T: serde::de::DeserializeOwned>(params: serde_json::Value) -> Result<T, String> {
            serde_json::from_value(params).map_err(|e| format!("invalid tool parameters: {e}"))
        }
        macro_rules! call {
            ($method:ident, $input:ty) => {{
                match decode::<$input>(params) {
                    Ok(input) => self.$method(Parameters(input)).await,
                    Err(e) => e,
                }
            }};
        }
        match tool_name {
            "replace_symbol_body" => call!(replace_symbol_body, edit::ReplaceSymbolBodyInput),
            "insert_symbol" => call!(insert_symbol, edit::InsertSymbolInput),
            "delete_symbol" => call!(delete_symbol, edit::DeleteSymbolInput),
            "edit_within_symbol" => call!(edit_within_symbol, edit::EditWithinSymbolInput),
            "batch_edit" => call!(batch_edit, edit::BatchEditInput),
            "batch_rename" => call!(batch_rename, edit::BatchRenameInput),
            "batch_insert" => call!(batch_insert, edit::BatchInsertInput),
            "search_files" => call!(search_files, tools::SearchFilesInput),
            "search_text" => call!(search_text, tools::SearchTextInput),
            "search_symbols" => call!(search_symbols, tools::SearchSymbolsInput),
            "get_file_context" => call!(get_file_context, tools::GetFileContextInput),
            "get_file_content" => call!(get_file_content, tools::GetFileContentInput),
            "get_symbol" => call!(get_symbol, tools::GetSymbolInput),
            "get_symbol_context" => call!(get_symbol_context, tools::GetSymbolContextInput),
            "find_references" => call!(find_references, tools::FindReferencesInput),
            "find_dependents" => call!(find_dependents, tools::FindDependentsInput),
            "explore" => call!(explore, tools::ExploreInput),
            "get_repo_map" => call!(get_repo_map, tools::GetRepoMapInput),
            "context_inventory" => self.context_inventory().await,
            "symforge_retrieve" => call!(symforge_retrieve, read_tools::SymforgeRetrieveInput),
            "health" => call!(health, tools::HealthInput),
            "health_compact" => self.health_compact().await,
            "conventions" => self.conventions().await,
            other => format!("dispatch_tool_for_tests: unknown tool '{other}'"),
        }
    }

    /// Test-only dispatcher that preserves public `CallToolResult` status metadata.
    ///
    /// This intentionally wires only handlers that already emit the SFB09 result-status
    /// contract. It lets conformance tests replay canonical JSON requests without
    /// starting an MCP transport or accepting string-only fake success paths.
    #[doc(hidden)]
    pub async fn dispatch_tool_result_for_tests(
        &self,
        tool_name: &str,
        params: serde_json::Value,
    ) -> Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
        use rmcp::handler::server::wrapper::Parameters;

        fn invalid_request_result(
            tool_name: &str,
            message: impl Into<String>,
        ) -> Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
            let text = format!(
                "Invalid request for tool `{tool_name}`: {}\nRecovery: compare the request JSON with the advertised input schema from tool_definitions().",
                message.into()
            );
            Ok(
                result_status::ResultStatus::new(result_status::OutcomeClass::InvalidRequest)
                    .into_call_tool_result(text),
            )
        }

        fn decode<T: serde::de::DeserializeOwned>(params: serde_json::Value) -> Result<T, String> {
            serde_json::from_value(params).map_err(|e| format!("invalid tool parameters: {e}"))
        }

        macro_rules! call_statused {
            ($method:ident, $input:ty) => {{
                match decode::<$input>(params) {
                    Ok(input) => self.$method(Parameters(input)).await,
                    Err(error) => invalid_request_result(tool_name, error),
                }
            }};
        }

        match tool_name {
            "replace_symbol_body" => {
                call_statused!(replace_symbol_body_tool, edit::ReplaceSymbolBodyInput)
            }
            "batch_edit" => call_statused!(batch_edit_tool, edit::BatchEditInput),
            "batch_insert" => call_statused!(batch_insert_tool, edit::BatchInsertInput),
            "search_files" => call_statused!(search_files_tool, tools::SearchFilesInput),
            "search_text" => call_statused!(search_text_tool, tools::SearchTextInput),
            "search_symbols" => call_statused!(search_symbols_tool, tools::SearchSymbolsInput),
            "get_file_content" => {
                call_statused!(get_file_content_tool, tools::GetFileContentInput)
            }
            "get_symbol" => call_statused!(get_symbol_tool, tools::GetSymbolInput),
            "get_symbol_context" => {
                call_statused!(get_symbol_context_tool, tools::GetSymbolContextInput)
            }
            "find_references" => call_statused!(find_references_tool, tools::FindReferencesInput),
            "symforge" => call_statused!(symforge_facade_tool, crate::stel::SymforgeCallInput),
            "symforge_edit" => {
                call_statused!(symforge_edit_facade_tool, crate::stel::StelEditRequest)
            }
            "status" => call_statused!(status_stel_tool, crate::stel::StelStatusRequest),
            other => {
                let text = format!(
                    "Unsupported tool `{other}` in public conformance harness.\nRecovery: add a statused dispatcher branch before adding the case, or remove the case from the public corpus."
                );
                Ok(
                    result_status::ResultStatus::new(result_status::OutcomeClass::InvalidRequest)
                        .into_call_tool_result(text),
                )
            }
        }
    }
}

/// Wire `SymForgeServer` as an MCP `ServerHandler`.
///
/// The `#[tool_handler]` macro delegates tool dispatch to `self.tool_router`
/// and supplies the `call_tool` / `list_tools` implementations automatically.
#[tool_handler(router = self.tool_router)]
#[prompt_handler(router = self.prompt_router)]
impl ServerHandler for SymForgeServer {
    fn get_info(&self) -> ServerInfo {
        // Override rmcp's `from_build_env` default, which expands
        // `CARGO_CRATE_NAME`/`CARGO_PKG_VERSION` inside the rmcp crate and would
        // otherwise identify this server as "rmcp"/<rmcp version>. `env!` here
        // expands in the symforge crate, staying in sync with Cargo.toml.
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_prompts()
                .enable_resources()
                .build(),
        )
        .with_server_info(rmcp::model::Implementation::new(
            "symforge",
            env!("CARGO_PKG_VERSION"),
        ))
    }

    fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListResourcesResult, rmcp::ErrorData>> + Send + '_
    {
        std::future::ready(Ok(ListResourcesResult {
            resources: self.resource_definitions(),
            ..Default::default()
        }))
    }

    fn list_resource_templates(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListResourceTemplatesResult, rmcp::ErrorData>>
    + Send
    + '_ {
        std::future::ready(Ok(ListResourceTemplatesResult {
            resource_templates: self.resource_template_definitions(),
            ..Default::default()
        }))
    }

    fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ReadResourceResult, rmcp::ErrorData>> + Send + '_
    {
        let uri = request.uri;
        async move { self.read_resource_uri(&uri).await }
    }

    /// MCP `tools/call` dispatch with a central compact-surface gate (P1-A).
    ///
    /// The `#[tool_handler]` macro only generates `call_tool` when the impl block
    /// does not already define one; by providing this method we replace the
    /// generated body while preserving its exact router delegation. The added
    /// gate enforces [`FR-008`] at dispatch (not just at `tools/list`): when the
    /// active surface is [`SurfaceProfile::Compact`], a `tools/call` for any tool
    /// name NOT in the advertised compact-3 set
    /// ([`crate::stel::surface::COMPACT_TOOL_NAMES`]) is rejected with an MCP
    /// `InvalidRequest` error. Full and Meta surfaces are unaffected, so the
    /// documented `SYMFORGE_SURFACE=full` opt-out still reaches every legacy
    /// tool. This is shared by both transports because stdio and the HTTP `/mcp`
    /// serve path dispatch through the same `ServerHandler::call_tool`.
    async fn call_tool(
        &self,
        request: rmcp::model::CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
        surface_probe::enforce_compact_surface(request.name.as_ref())?;
        let tcc = rmcp::handler::server::tool::ToolCallContext::new(self, request, context);
        self.tool_router.call(tcc).await
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, rmcp::ErrorData> {
        let profile = surface_probe::surface_profile_from_env();
        let tools = match profile {
            surface_probe::SurfaceProfile::Compact => crate::stel::compact_surface_tools(),
            _ => surface_probe::list_tools_for_profile(profile),
        };
        Ok(ListToolsResult {
            tools,
            meta: None,
            next_cursor: None,
        })
    }

    /// React to the client's `notifications/initialized`.
    ///
    /// After the handshake completes, the client's MCP `roots` capability (the
    /// open workspace folder it declared) becomes queryable via `roots/list`.
    /// We use it to bind the workspace when the launch CWD and
    /// `SYMFORGE_WORKSPACE_ROOT` resolved nothing at startup — the keystone
    /// "index won't load" case for home-CWD launchers (Cursor). Precedence
    /// (`SYMFORGE_WORKSPACE_ROOT` > client roots > launch-CWD walk) is enforced
    /// inside [`Self::bind_workspace_from_client_roots`], which is a deliberate
    /// no-op whenever startup already bound a root. Shared by both transports
    /// because stdio and the HTTP `/mcp` serve path dispatch through the same
    /// `ServerHandler`.
    async fn on_initialized(&self, context: rmcp::service::NotificationContext<RoleServer>) {
        tracing::info!("client initialized");
        self.bind_workspace_from_client_roots(&context.peer).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::Router;
    use axum::extract::State;
    use axum::routing::post;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::net::TcpListener;

    #[derive(Clone)]
    struct FakeToolState {
        calls: Arc<AtomicUsize>,
        body: Arc<String>,
    }

    async fn fake_tool_handler(State(state): State<FakeToolState>) -> String {
        state.calls.fetch_add(1, Ordering::Relaxed);
        state.body.as_ref().clone()
    }

    async fn spawn_fake_tool_server(
        body: &str,
    ) -> (String, tokio::sync::oneshot::Sender<()>, Arc<AtomicUsize>) {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind fake daemon tool server");
        let base_url = format!("http://{}", listener.local_addr().expect("listener addr"));
        let calls = Arc::new(AtomicUsize::new(0));
        let state = FakeToolState {
            calls: Arc::clone(&calls),
            body: Arc::new(body.to_string()),
        };
        let app = Router::new()
            .route(
                "/v1/sessions/{session_id}/tools/{tool_name}",
                post(fake_tool_handler),
            )
            .with_state(state);
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            let shutdown = async move {
                let _ = shutdown_rx.await;
            };
            let _ = axum::serve(listener, app)
                .with_graceful_shutdown(shutdown)
                .await;
        });
        (base_url, shutdown_tx, calls)
    }

    #[tokio::test]
    async fn daemon_degraded_clears_on_next_success() {
        let (base_url, shutdown, calls) = spawn_fake_tool_server("daemon-ok").await;
        let daemon_client = crate::daemon::DaemonSessionClient::new_for_test(
            base_url,
            "project-id".to_string(),
            "session-id".to_string(),
            "project-name".to_string(),
        );
        let server = SymForgeServer::new_daemon_proxy(daemon_client);
        server.daemon_degraded.store(true, Ordering::Relaxed);

        let result = server
            .proxy_tool_call("health", &serde_json::json!({}))
            .await;
        let _ = shutdown.send(());

        assert_eq!(result.as_deref(), Some("daemon-ok"));
        assert_eq!(
            calls.load(Ordering::Relaxed),
            1,
            "degraded proxy should still probe the daemon once"
        );
        assert!(
            !server.daemon_degraded.load(Ordering::Relaxed),
            "successful proxy call should clear daemon_degraded"
        );
    }

    /// Feature 013 US1 (T021): a daemon-PROXY server with a durable
    /// `StelLedgerStore` attached records the `symforge` economics event through
    /// to that store. This is the load-bearing invariant the daemon-default
    /// stdio wiring relies on: in the operator's real (daemon-backed) deployment
    /// the `symforge` compact tool — the ONLY ledger-recording tool — runs on
    /// the PROXY (the daemon worker's `execute_tool_call` has no `symforge`
    /// arm), so the durable store must be attached on the proxy path
    /// (`run_remote_mcp_server_async`) for durable accumulation to work. Here a
    /// fake daemon returns the served body for proxied primitives; the proxy
    /// itself captures the economics and write-throughs to the attached store.
    ///
    /// We call `symforge_stel_handler` directly (the inner handler, below the
    /// `SYMFORGE_SURFACE` facade gate) so the test needs no process-global env
    /// mutation and is deterministic under the parallel harness.
    #[tokio::test]
    async fn daemon_proxy_with_durable_store_records_symforge_event_through() {
        // Fake daemon returns a non-empty body for any proxied primitive so the
        // serve path produces a real served body to finalize.
        let (base_url, shutdown, _calls) =
            spawn_fake_tool_server("references to cfg_if:\n  src/lib.rs:1").await;
        let daemon_client = crate::daemon::DaemonSessionClient::new_for_test(
            base_url,
            "project-id".to_string(),
            "session-id".to_string(),
            "project-name".to_string(),
        );

        // Attach an in-memory durable store on the PROXY path — exactly what
        // `run_remote_mcp_server_async` does (T021), just in-memory for the test.
        let store = Arc::new(
            crate::stel::ledger_store::StelLedgerStore::open_in_memory("stdio-daemon-test")
                .expect("in-memory durable store"),
        );
        let server = SymForgeServer::new_daemon_proxy(daemon_client)
            .with_stel_ledger_store(Arc::clone(&store));

        // The proxy holds the store; durable accumulation starts at zero.
        assert_eq!(store.summary().expect("summary").total_events, 0);

        // Drive a real economics invocation through the inner handler (skips the
        // env-gated facade). The proxy fetches the served body from the fake
        // daemon, then captures + write-throughs the economics event.
        let request = crate::stel::StelRequest {
            query: "who references cfg_if".to_string(),
            ..Default::default()
        };
        let _ = server
            .symforge_stel_handler(&request)
            .await
            .expect("symforge_stel_handler dispatch");
        let _ = shutdown.send(());

        // The in-memory ledger recorded one event synchronously...
        assert_eq!(
            server.stel_ledger().lock().len(),
            1,
            "the proxy's in-memory ledger must record the event"
        );

        // ...and the DURABLE store on the proxy eventually holds it (the write is
        // offloaded onto spawn_blocking — poll briefly). This is the T021 proof:
        // durable accumulation works on the daemon-backed stdio path.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let total = loop {
            let total = store.summary().map(|s| s.total_events).unwrap_or(0);
            if total >= 1 || std::time::Instant::now() >= deadline {
                break total;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        };
        assert_eq!(
            total, 1,
            "the durable store attached on the daemon-proxy path must record the symforge event"
        );
        let recent = store.recent(10).expect("recent durable rows");
        assert_eq!(recent.len(), 1);
        assert_eq!(
            recent[0].session_id, "stdio-daemon-test",
            "the durable row carries the proxy store's session id"
        );
    }

    // ── ensure_local_index: root-mismatch invalidation (M2) ──────────────────

    /// Build a local-mode server (no daemon proxy) starting from an empty index,
    /// targeting `repo_root`. Mirrors the production `SymForgeServer::new` wiring
    /// used by the local stdio path.
    fn make_local_server(repo_root: Option<std::path::PathBuf>) -> SymForgeServer {
        let index = crate::live_index::LiveIndex::empty();
        let watcher_info = Arc::new(Mutex::new(WatcherInfo::default()));
        SymForgeServer::new(
            index,
            "test_project".to_string(),
            watcher_info,
            repo_root,
            None,
        )
    }

    /// Write a single `.rs` file under `dir` so a real local reload produces a
    /// non-empty index whose contents identify the project.
    fn seed_project(dir: &std::path::Path, file_name: &str, body: &str) {
        std::fs::write(dir.join(file_name), body).expect("seed project source file");
    }

    /// Core M2 regression: a changed `repo_root` (WITHOUT `reset_to_empty`) must
    /// force `ensure_local_index` to reload the new root instead of serving the
    /// previous project. Fails without the root-mismatch guard.
    #[tokio::test]
    async fn ensure_local_index_reloads_on_root_mismatch_without_reset() {
        let dir_a = tempfile::TempDir::new().expect("temp dir A");
        let dir_b = tempfile::TempDir::new().expect("temp dir B");
        seed_project(dir_a.path(), "alpha.rs", "fn alpha_only() {}\n");
        seed_project(dir_b.path(), "beta.rs", "fn beta_only() {}\n");

        // 1. Load project A.
        let server = make_local_server(Some(dir_a.path().to_path_buf()));
        server.ensure_local_index().await;

        let published_a = server.index.published_state();
        assert!(
            published_a.file_count > 0,
            "project A should have loaded a non-empty index"
        );
        assert_eq!(
            published_a.indexed_root.as_deref(),
            Some(crate::live_index::store::normalize_root(dir_a.path()).as_path()),
            "loaded index must record project A's normalized root"
        );
        assert!(
            server.index.read().get_file("alpha.rs").is_some(),
            "project A's file should be present after the first load"
        );

        // 2. Switch the target root to B WITHOUT calling reset_to_empty.
        server.set_repo_root(Some(dir_b.path().to_path_buf()));

        // 3. The next ensure_local_index must detect the root mismatch and reload B.
        server.ensure_local_index().await;

        let published_b = server.index.published_state();
        assert_eq!(
            published_b.indexed_root.as_deref(),
            Some(crate::live_index::store::normalize_root(dir_b.path()).as_path()),
            "after a root switch, ensure_local_index must serve project B's root"
        );
        assert!(
            server.index.read().get_file("beta.rs").is_some(),
            "project B's file must be present after the root-mismatch reload"
        );
        assert!(
            server.index.read().get_file("alpha.rs").is_none(),
            "project A's stale file must be gone after switching to project B"
        );
    }

    /// Steady-state guard: repeated `ensure_local_index` calls against the SAME
    /// root must NOT rebuild the index. A spurious reload bumps the project
    /// generation, so an unchanged generation (and unchanged recorded root)
    /// proves no reload occurred — verifying normalization makes the common path
    /// cheap rather than reloading on every call.
    #[tokio::test]
    async fn ensure_local_index_does_not_reload_on_repeated_same_root_calls() {
        let dir_a = tempfile::TempDir::new().expect("temp dir A");
        seed_project(dir_a.path(), "alpha.rs", "fn alpha_only() {}\n");

        let server = make_local_server(Some(dir_a.path().to_path_buf()));

        // First call performs the one and only load.
        server.ensure_local_index().await;
        let gen_after_first = server.index.current_project_generation();
        let root_after_first = server.index.published_state().indexed_root.clone();
        assert!(
            server.index.published_state().file_count > 0,
            "first ensure_local_index call should have loaded project A"
        );

        // Repeated calls with the same root must be no-ops (no reload).
        for _ in 0..3 {
            server.ensure_local_index().await;
        }

        assert_eq!(
            server.index.current_project_generation(),
            gen_after_first,
            "repeated same-root ensure_local_index calls must not reload (project generation must not change)"
        );
        assert_eq!(
            server.index.published_state().indexed_root,
            root_after_first,
            "repeated same-root calls must leave the recorded root unchanged"
        );
    }

    /// SF-STRESS-022: `initialize` serverInfo must identify symforge, not the
    /// rmcp framework default. Without `with_server_info`, rmcp's
    /// `Implementation::from_build_env` reports "rmcp"/<rmcp version>.
    #[test]
    fn get_info_reports_symforge_name_and_crate_version() {
        let server = make_local_server(None);
        let info = ServerHandler::get_info(&server);
        assert_eq!(
            info.server_info.name, "symforge",
            "serverInfo.name must identify symforge, not the rmcp framework"
        );
        assert_eq!(
            info.server_info.version,
            env!("CARGO_PKG_VERSION"),
            "serverInfo.version must track the symforge crate version"
        );
    }
}
