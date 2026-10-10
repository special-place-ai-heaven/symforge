//! Embedded registration, sole-handle ownership, and engine-only source workers.
//!
//! The embed feature deliberately excludes the server's notify-based watcher. An
//! embedded source therefore owns a small metadata-scouting worker which builds and
//! refreshes the same [`crate::live_index::store::SharedIndex`] used by the other
//! surfaces. The worker enters through the process admission/runtime seams and is
//! joined synchronously when its source or owning process runtime closes.
//!
//! The ownership rule is *sole handle*: one open source has exactly one handle,
//! and that handle is the only thing that can close it. Two handles to one source
//! would let one close while the other still believes it holds an open source —
//! the shape where a caller reads from something already torn down.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar};
use std::time::Duration;

use super::registry::ProjectKey;
use crate::domain::{FreshnessStatus, RootBinding, StatePlacement};

const EMBED_OBSERVER_POLL: Duration = Duration::from_millis(200);
const REFRESHING_VISIBILITY_WINDOW: Duration = Duration::from_millis(25);

/// Identity of one embedded open. Never reused, including across reopen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EmbeddedIdentity(std::num::NonZeroU64);

static NEXT_EMBEDDED: AtomicU64 = AtomicU64::new(1);

impl EmbeddedIdentity {
    /// Mint a fresh never-reused embedded identity.
    pub fn fresh() -> Self {
        let raw = NEXT_EMBEDDED.fetch_add(1, Ordering::Relaxed);
        Self(std::num::NonZeroU64::new(raw).expect("embedded counter starts at 1"))
    }

    /// Raw counter value, crate-only, for the embed boundary's kind-prefixed
    /// rendering and nothing else.
    pub(crate) fn raw(&self) -> u64 {
        self.0.get()
    }
}

/// Why an embedded request was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmbedRefusal {
    /// This source already has a live handle; there is only ever one.
    SourceAlreadyOpen {
        /// The identity currently holding it.
        held_by: EmbeddedIdentity,
    },
    /// Closing here would wait on the thread doing the closing.
    ///
    /// Returned instead of deadlocking. A finalizer that closes the source it is
    /// finalizing is waiting for itself, and blocking would hang the embedder's
    /// thread with no diagnosis.
    WouldSelfWait,
    /// The handle has already closed.
    AlreadyClosed,
}

/// What a close actually did.
///
/// Records whether THIS call performed the shutdown or joined one already done,
/// so a caller cannot mistake a coalesced close for having been the one that
/// tore the source down.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloseReceipt {
    identity: EmbeddedIdentity,
    performed_shutdown: bool,
    was_final_owner: bool,
}

impl CloseReceipt {
    /// The source that was closed.
    pub fn identity(&self) -> EmbeddedIdentity {
        self.identity
    }

    /// Whether this call performed the shutdown, rather than joining one that
    /// had already happened.
    pub fn performed_shutdown(&self) -> bool {
        self.performed_shutdown
    }

    /// Whether this was the last open source in the registration.
    pub fn was_final_owner(&self) -> bool {
        self.was_final_owner
    }
}

thread_local! {
    /// Which source this thread is finalizing, if any, so a close attempted
    /// from inside that source's own finalizer can refuse rather than wait on
    /// itself.
    ///
    /// The identity is the whole point. A bare `bool` refused ANY close made
    /// from inside ANY finalizer, so `a.finalize(|| b.close())` reported that
    /// closing `b` would wait on itself — a diagnosis naming something that did
    /// not happen, while `b` was left open.
    static FINALIZING: std::cell::Cell<Option<EmbeddedIdentity>> =
        const { std::cell::Cell::new(None) };
}

/// Mints embedded source handles, and is the only thing that can.
///
/// Named by the frozen `ORACLE-EMBED-FOUNDATION` seam. Construction of a handle
/// is private to this type, which is what makes "one incarnation owns at most
/// one handle" enforceable rather than advisory.
#[derive(Debug, Default)]
pub struct EmbeddedSourceFactory {
    open: std::sync::Mutex<HashMap<ProjectKey, OpenEmbeddedSource>>,
    shutdown: AtomicBool,
}

#[derive(Debug)]
struct OpenEmbeddedSource {
    identity: EmbeddedIdentity,
    owner: Option<EmbeddedIdentity>,
    opening: bool,
    binding: Option<Arc<EmbeddedBinding>>,
}

#[cfg(feature = "__test-internals")]
static PANIC_AFTER_START_ROOT: std::sync::Mutex<Option<ProjectKey>> = std::sync::Mutex::new(None);

#[cfg(feature = "__test-internals")]
pub fn panic_after_start_for_test(root: &std::path::Path) {
    let crate::domain::RootResolution::Bound(binding) = crate::discovery::resolve_root_candidate(
        root,
        crate::domain::RootCandidateSource::McpClientRoot,
        crate::domain::RootRequestMode::Automatic,
    ) else {
        panic!("test root must resolve to an embedded binding");
    };
    *PANIC_AFTER_START_ROOT
        .lock()
        .expect("embedded panic hook mutex") = Some(ProjectKey::new(&binding.root_id.0));
}

#[cfg(feature = "__test-internals")]
fn take_panic_after_start_for_test(key: &ProjectKey) -> bool {
    let mut configured = PANIC_AFTER_START_ROOT
        .lock()
        .expect("embedded panic hook mutex");
    if configured.as_ref() == Some(key) {
        configured.take();
        true
    } else {
        false
    }
}

#[cfg(feature = "__test-internals")]
struct JoinEnteredState {
    entered: std::sync::Mutex<bool>,
    reached: Condvar,
}

#[cfg(feature = "__test-internals")]
static JOIN_ENTERED: std::sync::OnceLock<
    std::sync::Mutex<HashMap<PathBuf, Arc<JoinEnteredState>>>,
> = std::sync::OnceLock::new();

/// Wait until `shutdown` has dropped the factory mutex and is at `worker.join()`.
#[cfg(feature = "__test-internals")]
pub struct JoinEnteredForTest {
    root: PathBuf,
    state: Arc<JoinEnteredState>,
}

#[cfg(feature = "__test-internals")]
pub fn watch_join_entered_for_test(root: &std::path::Path) -> JoinEnteredForTest {
    let root = crate::live_index::store::normalize_root(root);
    let state = Arc::new(JoinEnteredState {
        entered: std::sync::Mutex::new(false),
        reached: Condvar::new(),
    });
    let watches = JOIN_ENTERED.get_or_init(|| std::sync::Mutex::new(HashMap::new()));
    let previous = watches
        .lock()
        .expect("join-entered watch registry")
        .insert(root.clone(), Arc::clone(&state));
    assert!(
        previous.is_none(),
        "a root can hold only one join-entered watch"
    );
    JoinEnteredForTest { root, state }
}

#[cfg(feature = "__test-internals")]
impl JoinEnteredForTest {
    pub fn wait_until_entered(&self, timeout: Duration) -> bool {
        let entered = self.state.entered.lock().expect("join-entered watch");
        let (entered, _) = self
            .state
            .reached
            .wait_timeout_while(entered, timeout, |entered| !*entered)
            .expect("join-entered watch");
        *entered
    }
}

#[cfg(feature = "__test-internals")]
impl Drop for JoinEnteredForTest {
    fn drop(&mut self) {
        let watches = JOIN_ENTERED.get_or_init(|| std::sync::Mutex::new(HashMap::new()));
        let mut watches = watches.lock().expect("join-entered watch registry");
        if watches
            .get(&self.root)
            .is_some_and(|current| Arc::ptr_eq(current, &self.state))
        {
            watches.remove(&self.root);
        }
    }
}

#[cfg(feature = "__test-internals")]
fn notify_join_entered(root: &std::path::Path) {
    let root = crate::live_index::store::normalize_root(root);
    let Some(state) = JOIN_ENTERED
        .get()
        .and_then(|watches| watches.lock().ok()?.get(&root).cloned())
    else {
        return;
    };
    *state.entered.lock().expect("join-entered watch") = true;
    state.reached.notify_all();
}

#[cfg(feature = "__test-internals")]
struct OpenAfterReserveState {
    holding: std::sync::Mutex<bool>,
    entered: std::sync::Mutex<bool>,
    reached: Condvar,
}

#[cfg(feature = "__test-internals")]
static OPEN_AFTER_RESERVE: std::sync::OnceLock<
    std::sync::Mutex<HashMap<ProjectKey, Arc<OpenAfterReserveState>>>,
> = std::sync::OnceLock::new();

/// Parks `open_bound` after it has reserved the root and before it admits.
#[cfg(feature = "__test-internals")]
pub struct OpenAfterReserveHoldForTest {
    key: ProjectKey,
    state: Arc<OpenAfterReserveState>,
}

#[cfg(feature = "__test-internals")]
pub fn hold_open_after_reserve_for_test(root: &std::path::Path) -> OpenAfterReserveHoldForTest {
    let crate::domain::RootResolution::Bound(binding) = crate::discovery::resolve_root_candidate(
        root,
        crate::domain::RootCandidateSource::McpClientRoot,
        crate::domain::RootRequestMode::Automatic,
    ) else {
        panic!("test root must resolve to an embedded binding");
    };
    let key = ProjectKey::new(&binding.root_id.0);
    let state = Arc::new(OpenAfterReserveState {
        holding: std::sync::Mutex::new(true),
        entered: std::sync::Mutex::new(false),
        reached: Condvar::new(),
    });
    let holds = OPEN_AFTER_RESERVE.get_or_init(|| std::sync::Mutex::new(HashMap::new()));
    let previous = holds
        .lock()
        .expect("open-after-reserve hold registry")
        .insert(key.clone(), Arc::clone(&state));
    assert!(
        previous.is_none(),
        "a root can hold only one open-after-reserve gate"
    );
    OpenAfterReserveHoldForTest { key, state }
}

#[cfg(feature = "__test-internals")]
impl OpenAfterReserveHoldForTest {
    pub fn wait_until_entered(&self, timeout: Duration) -> bool {
        let entered = self.state.entered.lock().expect("open-after-reserve hold");
        let (entered, _) = self
            .state
            .reached
            .wait_timeout_while(entered, timeout, |entered| !*entered)
            .expect("open-after-reserve hold");
        *entered
    }

    pub fn release(&self) {
        *self.state.holding.lock().expect("open-after-reserve hold") = false;
        self.state.reached.notify_all();
    }
}

#[cfg(feature = "__test-internals")]
impl Drop for OpenAfterReserveHoldForTest {
    fn drop(&mut self) {
        self.release();
        let holds = OPEN_AFTER_RESERVE.get_or_init(|| std::sync::Mutex::new(HashMap::new()));
        let mut holds = holds.lock().expect("open-after-reserve hold registry");
        if holds
            .get(&self.key)
            .is_some_and(|current| Arc::ptr_eq(current, &self.state))
        {
            holds.remove(&self.key);
        }
    }
}

#[cfg(feature = "__test-internals")]
fn pause_open_after_reserve_for_test(key: &ProjectKey) {
    let Some(state) = OPEN_AFTER_RESERVE
        .get()
        .and_then(|holds| holds.lock().ok()?.get(key).cloned())
    else {
        return;
    };
    let mut entered = state.entered.lock().expect("open-after-reserve hold");
    if *entered {
        return;
    }
    *entered = true;
    drop(entered);
    state.reached.notify_all();
    let holding = state.holding.lock().expect("open-after-reserve hold");
    drop(
        state
            .reached
            .wait_while(holding, |holding| *holding)
            .expect("open-after-reserve hold"),
    );
}

struct OpenRollback {
    factory: Arc<EmbeddedSourceFactory>,
    key: ProjectKey,
    identity: EmbeddedIdentity,
    binding: Option<Arc<EmbeddedBinding>>,
    committed: bool,
}

impl OpenRollback {
    fn reserve(
        factory: Arc<EmbeddedSourceFactory>,
        key: ProjectKey,
        identity: EmbeddedIdentity,
    ) -> Self {
        Self {
            factory,
            key,
            identity,
            binding: None,
            committed: false,
        }
    }

    fn bind(&mut self, binding: Arc<EmbeddedBinding>) {
        self.binding = Some(binding);
    }

    fn commit(mut self) {
        self.committed = true;
    }
}

impl Drop for OpenRollback {
    fn drop(&mut self) {
        if self.committed {
            return;
        }
        if let Some(binding) = &self.binding {
            binding.shutdown();
        }
        let mut open = self
            .factory
            .open
            .lock()
            .expect("embedded registration mutex");
        if open
            .get(&self.key)
            .is_some_and(|source| source.identity == self.identity && source.opening)
        {
            open.remove(&self.key);
        }
        self.factory
            .shutdown
            .store(open.is_empty(), Ordering::Release);
        drop(open);
        let _ = super::activation::process_project_registry().stop(&self.key);
    }
}

#[derive(Debug)]
pub(crate) enum EmbeddedOpenError {
    SourceAlreadyOpen,
    AdmissionUnavailable,
    WorkerUnavailable,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct EmbeddedShutdownReport {
    pub closed_sources: u64,
    pub joined_workers: u64,
}

#[derive(Debug, Clone)]
struct EmbeddedRuntimeState {
    phase: super::public_api::SourceRuntimePhase,
    current_publication_identity: Option<String>,
    observer_epoch: u64,
    source_version: u64,
}

#[derive(Debug, Default)]
struct WorkerControl {
    stop: bool,
    refresh_requested: bool,
    /// An `index_folder` reset deleted the snapshot scope; the next reload
    /// that publishes Current records the reset generation (MCP marks it
    /// only after its reload succeeds).
    reset_pending: bool,
}

struct EmbeddedBinding {
    identity: EmbeddedIdentity,
    key: ProjectKey,
    root: PathBuf,
    state_placement: StatePlacement,
    runtime: super::activation::ProjectRuntimeHandle,
    #[cfg(feature = "embed")]
    authority: Arc<super::activation::ProjectSourceAuthority>,
    #[cfg(feature = "embed")]
    state_anchor: Option<AdmittedStateAnchor>,
    #[cfg(feature = "embed")]
    restored_mtimes: std::sync::Mutex<Option<HashMap<String, u64>>>,
    #[cfg(feature = "embed")]
    git_view: std::sync::Mutex<Option<Arc<super::embed_git::PreparedGitView>>>,
    #[cfg(feature = "embed")]
    open_reset: Option<crate::embed::parity::source_options::SnapshotResetReceipt>,
    state: std::sync::Mutex<EmbeddedRuntimeState>,
    control: std::sync::Mutex<WorkerControl>,
    wake: Condvar,
    worker: std::sync::Mutex<Option<std::thread::JoinHandle<()>>>,
    shutdown_started: AtomicBool,
    progress: Arc<crate::live_index::store::ReloadProgressSink>,
}

/// Original selected state child. Parked between operations so an idle source
/// does not hold Windows directory handles, while every reopen checks identity.
#[cfg(feature = "embed")]
#[derive(Clone)]
pub(super) struct AdmittedStateAnchor {
    lease: Arc<super::physical_root::PhysicalRootLease>,
    stable_key: [u8; 16],
}

#[cfg(feature = "embed")]
impl AdmittedStateAnchor {
    fn capture(
        placement: &StatePlacement,
        authority: &super::activation::ProjectSourceAuthority,
    ) -> Result<Option<Self>, EmbeddedOpenError> {
        let opened = match placement {
            StatePlacement::MemoryOnly { .. } => return Ok(None),
            StatePlacement::ProjectLocal { .. } => authority
                .with_source_anchor_read(|root| {
                    root.child_directory(std::path::Path::new(".symforge"), false)
                })
                .map_err(|_| EmbeddedOpenError::AdmissionUnavailable)?,
            StatePlacement::UserLocal { directory, .. } => {
                super::physical_root::PhysicalRootLease::take(directory.as_path())
            }
        };
        let stable_key = opened
            .opened_stable_key()
            .ok_or(EmbeddedOpenError::AdmissionUnavailable)?;
        Ok(Some(Self {
            lease: Arc::new(opened.parked()),
            stable_key,
        }))
    }

    pub(super) fn lease(&self) -> &super::physical_root::PhysicalRootLease {
        &self.lease
    }

    pub(super) fn stable_key(&self) -> [u8; 16] {
        self.stable_key
    }
}

impl std::fmt::Debug for EmbeddedBinding {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("EmbeddedBinding")
            .field("identity", &self.identity)
            .field("root", &self.root)
            .field("state", &self.state.lock().expect("embedded state mutex"))
            .finish_non_exhaustive()
    }
}

impl EmbeddedBinding {
    fn new(
        identity: EmbeddedIdentity,
        key: ProjectKey,
        root: PathBuf,
        state_placement: StatePlacement,
        runtime: super::activation::ProjectRuntimeHandle,
        #[cfg(feature = "embed")] authority: Arc<super::activation::ProjectSourceAuthority>,
        #[cfg(feature = "embed")] state_anchor: Option<AdmittedStateAnchor>,
        #[cfg(feature = "embed")] restored_mtimes: Option<HashMap<String, u64>>,
        #[cfg(feature = "embed")] open_reset: Option<
            crate::embed::parity::source_options::SnapshotResetReceipt,
        >,
    ) -> Arc<Self> {
        Arc::new(Self {
            identity,
            key,
            root,
            state_placement,
            runtime,
            #[cfg(feature = "embed")]
            authority,
            #[cfg(feature = "embed")]
            state_anchor,
            #[cfg(feature = "embed")]
            restored_mtimes: std::sync::Mutex::new(restored_mtimes),
            #[cfg(feature = "embed")]
            git_view: std::sync::Mutex::new(None),
            #[cfg(feature = "embed")]
            open_reset: open_reset.clone(),
            state: std::sync::Mutex::new(EmbeddedRuntimeState {
                phase: super::public_api::SourceRuntimePhase::Loading,
                current_publication_identity: None,
                observer_epoch: 0,
                source_version: 0,
            }),
            control: std::sync::Mutex::new(WorkerControl {
                #[cfg(feature = "embed")]
                reset_pending: open_reset.is_some(),
                ..WorkerControl::default()
            }),
            wake: Condvar::new(),
            worker: std::sync::Mutex::new(None),
            shutdown_started: AtomicBool::new(false),
            progress: crate::live_index::store::ReloadProgressSink::new(),
        })
    }

    fn start(self: &Arc<Self>) -> std::io::Result<()> {
        let worker_binding = Arc::clone(self);
        let worker = std::thread::Builder::new()
            .name(format!("symforge-embed-{}", self.identity.raw()))
            .spawn(move || worker_binding.run())?;
        *self.worker.lock().expect("embedded worker mutex") = Some(worker);
        Ok(())
    }

    fn run(&self) {
        {
            let mut state = self.state.lock().expect("embedded state mutex");
            state.observer_epoch = 1;
        }

        #[cfg(feature = "embed")]
        let restored = self
            .restored_mtimes
            .lock()
            .expect("embedded restored seed mutex")
            .take();
        #[cfg(feature = "embed")]
        let mut observed_fingerprint = if let Some(mtimes) = restored {
            crate::live_index::persist::background_verify_bound(
                self.runtime.data_plane(),
                &self.root,
                &mtimes,
                &self.authority,
                || self.shutdown_started.load(Ordering::Acquire),
            );
            self.publish_observed_current()
        } else {
            self.reload_and_publish()
        };
        #[cfg(not(feature = "embed"))]
        let mut observed_fingerprint = self.reload_and_publish();
        self.complete_pending_reset();
        loop {
            let mut control = self.control.lock().expect("embedded control mutex");
            let (next, _) = self
                .wake
                .wait_timeout_while(control, EMBED_OBSERVER_POLL, |control| {
                    !control.stop && !control.refresh_requested
                })
                .expect("embedded control mutex");
            control = next;
            if control.stop {
                break;
            }
            let explicit_refresh = std::mem::take(&mut control.refresh_requested);
            drop(control);

            let next_fingerprint = self.observe_fingerprint();
            if self.shutdown_started.load(Ordering::Acquire) {
                break;
            }
            let disk_changed = match (&observed_fingerprint, &next_fingerprint) {
                (Some(previous), Some(current)) => previous != current,
                (None, Some(_)) | (Some(_), None) => true,
                (None, None) => false,
            };
            let blocked = self.state.lock().expect("embedded state mutex").phase
                == super::public_api::SourceRuntimePhase::Blocked;
            if explicit_refresh || disk_changed || blocked {
                self.set_phase(super::public_api::SourceRuntimePhase::Refreshing);
                if self.wait_refresh_visibility_or_stop() {
                    break;
                }
                observed_fingerprint = self.reload_and_publish();
                if self.shutdown_started.load(Ordering::Acquire) {
                    break;
                }
                self.complete_pending_reset();
            } else {
                observed_fingerprint = next_fingerprint;
            }
        }
        self.set_phase(super::public_api::SourceRuntimePhase::Stopped);
        #[cfg(feature = "embed")]
        self.git_view
            .lock()
            .expect("embedded Git view mutex")
            .take();
    }

    /// Record the `index_folder` reset generation once a reload after the
    /// reset has published Current; otherwise keep it pending for the next.
    fn complete_pending_reset(&self) {
        let mut control = self.control.lock().expect("embedded control mutex");
        if !control.reset_pending {
            return;
        }
        let current = self.state.lock().expect("embedded state mutex").phase
            == super::public_api::SourceRuntimePhase::Current;
        if current {
            control.reset_pending = false;
            drop(control);
            self.runtime.data_plane().mark_index_folder_reset();
        }
    }

    fn wait_refresh_visibility_or_stop(&self) -> bool {
        let control = self.control.lock().expect("embedded control mutex");
        let (control, _) = self
            .wake
            .wait_timeout_while(control, REFRESHING_VISIBILITY_WINDOW, |control| {
                !control.stop
            })
            .expect("embedded control mutex");
        control.stop
    }

    fn reload_and_publish(&self) -> Option<String> {
        #[cfg(feature = "embed")]
        if self.authority.verify_physical_root_anchor().is_err() {
            self.set_blocked();
            return None;
        }
        self.progress.reset();
        let _progress = crate::live_index::store::register_reload_progress(
            &self.root,
            Arc::clone(&self.progress),
        );
        let exclusions = crate::discovery::SourceExclusions::for_state_placement(
            &self.root,
            &self.state_placement,
        );
        let result = self
            .runtime
            .data_plane()
            .reload_for_binding_with_exclusions_cancellable(
                &self.root,
                self.state_placement.directory().cloned(),
                exclusions,
                &self.shutdown_started,
            );
        if let Err(error) = result {
            if self.shutdown_started.load(Ordering::Acquire)
                || crate::live_index::store::reload_was_cancelled(&error)
            {
                return None;
            }
            self.set_blocked();
            return None;
        }

        self.publish_observed_current()
    }

    /// MCP's synchronous exact-path freshen before a targeted read
    /// (`freshen_exact_path_for_targeted_retrieval`): re-admit one changed
    /// path through the shared single-file seam and publish the result as the
    /// next Current publication, so the read captured after it serves the
    /// bytes on disk. Only a Current source is freshened; every other phase
    /// belongs to the worker, and the capture that follows refuses it.
    // ponytail: the single-file seam reads by path beneath the bound root, not
    // through the original-root capability; a replaced root is caught after the
    // fact because every served read re-verifies its bytes through that
    // capability and refuses a mismatch as a stale publication.
    #[cfg(feature = "embed")]
    fn freshen_exact_path(
        &self,
        relative_path: &str,
    ) -> Result<(), super::guidance::freshen::TargetedFreshenRefusal> {
        if self.shutdown_started.load(Ordering::Acquire)
            || self.authority.verify_physical_root_anchor().is_err()
            || self.state.lock().expect("embedded state mutex").phase
                != super::public_api::SourceRuntimePhase::Current
        {
            return Ok(());
        }
        let shared = self.runtime.data_plane();
        let expected_gen = shared.current_project_generation();
        if super::guidance::freshen::freshen_exact_path(
            shared,
            expected_gen,
            &self.root,
            &self.authority,
            relative_path,
        )? {
            self.publish_freshened();
        }
        Ok(())
    }

    /// Advance the Current publication to the data plane's after a freshen,
    /// unless the worker has taken the source out of Current meanwhile.
    #[cfg(feature = "embed")]
    fn publish_freshened(&self) {
        let mut state = self.state.lock().expect("embedded state mutex");
        let published = self.runtime.data_plane().published_generation();
        if self.shutdown_started.load(Ordering::Acquire)
            || state.phase != super::public_api::SourceRuntimePhase::Current
            || !matches!(published.freshness.as_ref(), FreshnessStatus::Current)
        {
            return;
        }
        state.source_version = state.source_version.saturating_add(1).max(1);
        state.current_publication_identity = Some(format!(
            "embed-publication-{}-{}",
            self.identity.raw(),
            published.publication_generation
        ));
    }

    fn publish_observed_current(&self) -> Option<String> {
        #[cfg(feature = "embed")]
        if self.authority.verify_physical_root_anchor().is_err() {
            self.set_blocked();
            return None;
        }
        if self.shutdown_started.load(Ordering::Acquire) {
            return None;
        }

        let Some(fingerprint) = self.observe_fingerprint() else {
            if self.shutdown_started.load(Ordering::Acquire) {
                return None;
            }
            self.set_blocked();
            return None;
        };
        let published = self.runtime.data_plane().published_generation();
        if !matches!(published.freshness.as_ref(), FreshnessStatus::Current) {
            self.set_blocked();
            return Some(fingerprint);
        }
        let mut state = self.state.lock().expect("embedded state mutex");
        if self.shutdown_started.load(Ordering::Acquire) {
            return None;
        }
        state.source_version = state.source_version.saturating_add(1).max(1);
        state.current_publication_identity = Some(format!(
            "embed-publication-{}-{}",
            self.identity.raw(),
            published.publication_generation
        ));
        state.phase = super::public_api::SourceRuntimePhase::Current;
        Some(fingerprint)
    }

    fn observe_fingerprint(&self) -> Option<String> {
        if self.shutdown_started.load(Ordering::Acquire) {
            return None;
        }
        #[cfg(feature = "embed")]
        self.authority.verify_physical_root_anchor().ok()?;
        let exclusions = crate::discovery::SourceExclusions::for_state_placement(
            &self.root,
            &self.state_placement,
        );
        let plan = crate::discovery::scout_repository_with_exclusions_cancellable(
            &self.root,
            &exclusions,
            &self.shutdown_started,
        )
        .ok()?;
        if self.shutdown_started.load(Ordering::Acquire) {
            return None;
        }
        #[cfg(feature = "embed")]
        self.authority.verify_physical_root_anchor().ok()?;
        Some(crate::hash::digest_hex(format!("{plan:?}").as_bytes()))
    }

    fn set_phase(&self, phase: super::public_api::SourceRuntimePhase) {
        let mut state = self.state.lock().expect("embedded state mutex");
        if self.shutdown_started.load(Ordering::Acquire)
            && !matches!(
                phase,
                super::public_api::SourceRuntimePhase::Stopping
                    | super::public_api::SourceRuntimePhase::Stopped
            )
        {
            return;
        }
        state.phase = phase;
    }

    fn set_blocked(&self) {
        let mut state = self.state.lock().expect("embedded state mutex");
        if self.shutdown_started.load(Ordering::Acquire) {
            return;
        }
        state.phase = super::public_api::SourceRuntimePhase::Blocked;
        state.current_publication_identity = None;
    }

    fn runtime_view(&self) -> super::public_api::SourceRuntimeView {
        let state = self.state.lock().expect("embedded state mutex").clone();
        super::public_api::SourceRuntimeView {
            binding_identity: format!("source-{}", self.identity.raw()),
            current_publication_identity: state.current_publication_identity,
            observer_epoch: state.observer_epoch,
            phase: state.phase,
            source_version: state.source_version,
        }
    }

    fn current_claim(
        &self,
        operation: crate::lifecycle_identity::OperationKind,
        normalized_arguments: &[u8],
    ) -> Result<CurrentEmbedClaim, super::public_api::EmbedSourceRefusal> {
        let state = self.state.lock().expect("embedded state mutex");
        if state.phase != super::public_api::SourceRuntimePhase::Current {
            let retry = match state.phase {
                super::public_api::SourceRuntimePhase::Blocked => {
                    crate::lifecycle_identity::RetryAdvice::Operator
                }
                super::public_api::SourceRuntimePhase::Stopped
                | super::public_api::SourceRuntimePhase::Stopping => {
                    crate::lifecycle_identity::RetryAdvice::Never
                }
                _ => crate::lifecycle_identity::RetryAdvice::OnEvent,
            };
            return Err(super::public_api::bound_source_refusal(
                crate::lifecycle_identity::SourceRefusalKind::SourceUnavailable,
                operation,
                retry,
                normalized_arguments,
            ));
        }
        let publication_identity = state
            .current_publication_identity
            .clone()
            .expect("Current embedded source has a publication identity");
        Ok(CurrentEmbedClaim {
            binding_identity: format!("source-{}", self.identity.raw()),
            publication_identity,
            source_version: state.source_version,
        })
    }

    fn request_refresh(&self, reset: bool) -> Option<u64> {
        let state = self.state.lock().expect("embedded state mutex");
        if matches!(
            state.phase,
            super::public_api::SourceRuntimePhase::Stopped
                | super::public_api::SourceRuntimePhase::Stopping
        ) {
            return None;
        }
        let version = state.source_version;
        drop(state);
        let mut control = self.control.lock().expect("embedded control mutex");
        control.refresh_requested = true;
        control.reset_pending |= reset;
        self.wake.notify_all();
        Some(version)
    }

    fn shutdown(&self) -> (u64, u64) {
        if self.shutdown_started.swap(true, Ordering::AcqRel) {
            let version = self
                .state
                .lock()
                .expect("embedded state mutex")
                .source_version;
            return (version, 0);
        }
        self.set_phase(super::public_api::SourceRuntimePhase::Stopping);
        {
            let mut control = self.control.lock().expect("embedded control mutex");
            control.stop = true;
            self.wake.notify_all();
        }
        let worker = self.worker.lock().expect("embedded worker mutex").take();
        // Factory mutex is already dropped by close_one/shutdown_owner.
        // Signal before join so an unrelated open can race the parked worker.
        #[cfg(feature = "__test-internals")]
        notify_join_entered(&self.root);
        let joined = worker
            .map(|worker| {
                let _ = worker.join();
                1
            })
            .unwrap_or(0);
        self.set_phase(super::public_api::SourceRuntimePhase::Stopped);
        if let Err(refusal) = super::activation::process_project_registry().stop(&self.key) {
            tracing::debug!(?refusal, "embedded admission stop refused");
        }
        let version = self
            .state
            .lock()
            .expect("embedded state mutex")
            .source_version;
        (version, joined)
    }
}

struct CurrentEmbedClaim {
    binding_identity: String,
    publication_identity: String,
    source_version: u64,
}

fn normalized_path_prefix(prefix: Option<&str>) -> Option<String> {
    prefix.map(|prefix| {
        prefix
            .replace('\\', "/")
            .trim_start_matches("./")
            .trim_end_matches('/')
            .to_string()
    })
}

fn path_matches_prefix(path: &str, prefix: Option<&str>) -> bool {
    match normalized_path_prefix(prefix) {
        None => true,
        Some(prefix) if prefix.is_empty() => true,
        Some(prefix) => crate::live_index::search::PathScope::prefix(prefix).matches(path),
    }
}

fn knowledge_lifecycle_label(
    lifecycle: crate::live_index::knowledge_authority::KnowledgeLifecycle,
) -> String {
    format!("{lifecycle:?}").to_ascii_lowercase()
}

fn knowledge_coverage_label(
    coverage: &crate::live_index::knowledge_bridge::DerivedCoverage,
) -> String {
    match coverage {
        crate::live_index::knowledge_bridge::DerivedCoverage::Complete => "complete".to_string(),
        crate::live_index::knowledge_bridge::DerivedCoverage::Loading => "loading".to_string(),
        crate::live_index::knowledge_bridge::DerivedCoverage::Truncated { .. } => {
            "truncated".to_string()
        }
    }
}

fn knowledge_withheld_label(
    reason: crate::live_index::knowledge_retrieve::KnowledgeWithheldReason,
) -> String {
    match reason {
        crate::live_index::knowledge_retrieve::KnowledgeWithheldReason::PolicyWithheld => {
            "policy_withheld".to_string()
        }
    }
}

fn bounded_preview(line: &str) -> String {
    const MAX_PREVIEW_CHARS: usize = 500;
    let mut chars = line.chars();
    let preview: String = chars.by_ref().take(MAX_PREVIEW_CHARS).collect();
    if chars.next().is_some() {
        format!("{preview}…")
    } else {
        preview
    }
}

impl EmbeddedSourceFactory {
    /// A registration with nothing open.
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Test-internal dark registration retained for the Slice-2 ownership
    /// oracles. Production opens always use [`Self::open_bound`].
    #[cfg(feature = "__test-internals")]
    pub fn open(self: &Arc<Self>, key: ProjectKey) -> Result<EmbeddedSourceHandle, EmbedRefusal> {
        let mut open = self.open.lock().expect("embedded registration mutex");
        if let Some(held_by) = open.get(&key) {
            return Err(EmbedRefusal::SourceAlreadyOpen {
                held_by: held_by.identity,
            });
        }
        let identity = EmbeddedIdentity::fresh();
        open.insert(
            key.clone(),
            OpenEmbeddedSource {
                identity,
                owner: None,
                opening: false,
                binding: None,
            },
        );
        self.shutdown.store(false, Ordering::Release);
        Ok(EmbeddedSourceHandle {
            identity,
            key,
            registration: Arc::clone(self),
            binding: None,
            closed: AtomicBool::new(false),
            _not_unwind_safe: std::marker::PhantomData,
        })
    }

    /// Open `binding`, yielding the sole handle for it and starting its worker.
    ///
    /// Refuses if a handle is already live for this key. Handing out a second
    /// handle would let one close while the other still believes it holds an
    /// open source.
    pub(crate) fn open_bound(
        self: &Arc<Self>,
        binding: RootBinding,
        state_placement: StatePlacement,
        owner: EmbeddedIdentity,
    ) -> Result<EmbeddedSourceHandle, EmbeddedOpenError> {
        self.open_bound_with_reset(binding, state_placement, owner, false)
    }

    /// `open_bound` plus the MCP `index_folder` snapshot reset. A failed reset
    /// refuses the open as admission-unavailable before anything is restored.
    pub(crate) fn open_bound_with_reset(
        self: &Arc<Self>,
        binding: RootBinding,
        state_placement: StatePlacement,
        owner: EmbeddedIdentity,
        reset_snapshot_state: bool,
    ) -> Result<EmbeddedSourceHandle, EmbeddedOpenError> {
        let key = ProjectKey::new(&binding.root_id.0);
        let identity = EmbeddedIdentity::fresh();
        {
            let mut open = self.open.lock().expect("embedded registration mutex");
            if open.contains_key(&key) {
                return Err(EmbeddedOpenError::SourceAlreadyOpen);
            }
            open.insert(
                key.clone(),
                OpenEmbeddedSource {
                    identity,
                    owner: Some(owner),
                    opening: true,
                    binding: None,
                },
            );
            self.shutdown.store(false, Ordering::Release);
        }

        #[cfg(feature = "__test-internals")]
        pause_open_after_reserve_for_test(&key);

        let mut rollback = OpenRollback::reserve(Arc::clone(self), key.clone(), identity);
        let admission = super::activation::admit_project_with_outcome(
            super::process_runtime::SurfaceKind::Embed,
            &binding.canonical_root,
            &binding.root_id.0,
            binding.access_mode,
            &state_placement,
        )
        .map_err(|_| EmbeddedOpenError::AdmissionUnavailable)?;
        #[cfg(feature = "embed")]
        let authority = {
            let candidate = super::activation::project_source_authority(&binding.canonical_root);
            let admitted_root = admission
                .slot()
                .binding()
                .map_err(|_| EmbeddedOpenError::AdmissionUnavailable)?
                .physical_root();
            if candidate.admission_binding().physical_root() != admitted_root {
                return Err(EmbeddedOpenError::AdmissionUnavailable);
            }
            candidate
        };
        #[cfg(feature = "embed")]
        let state_anchor = AdmittedStateAnchor::capture(&state_placement, &authority)?;
        // MCP `index_folder` reset: delete the snapshot scope after admission
        // and before restore, so the open cannot warm-load what was reset.
        #[cfg(feature = "embed")]
        let open_reset = if reset_snapshot_state {
            let report = crate::live_index::persist::reset_snapshot_state(
                &binding.canonical_root,
                &state_placement,
            )
            .map_err(|_| EmbeddedOpenError::AdmissionUnavailable)?;
            Some(crate::embed::parity::source_options::SnapshotResetReceipt::from_report(&report))
        } else {
            None
        };
        #[cfg(not(feature = "embed"))]
        let _ = reset_snapshot_state;
        #[cfg(feature = "embed")]
        let restored = super::embed_restore::load_admitted_snapshot(
            &binding.canonical_root,
            &state_placement,
            state_anchor.as_ref(),
            &authority,
        )
        .map_err(|_| EmbeddedOpenError::AdmissionUnavailable)?;
        #[cfg(feature = "embed")]
        let (index, restored_mtimes) = match restored {
            Some(seed) => (seed.index, Some(seed.mtimes)),
            None => (crate::live_index::store::LiveIndex::empty(), None),
        };
        #[cfg(not(feature = "embed"))]
        let index = crate::live_index::store::LiveIndex::empty();
        let runtime =
            super::activation::ProjectRuntimeHandle::bind_admitted(index, admission.into_slot());
        let source = EmbeddedBinding::new(
            identity,
            key.clone(),
            binding.canonical_root,
            state_placement,
            runtime,
            #[cfg(feature = "embed")]
            authority,
            #[cfg(feature = "embed")]
            state_anchor,
            #[cfg(feature = "embed")]
            restored_mtimes,
            #[cfg(feature = "embed")]
            open_reset,
        );
        rollback.bind(Arc::clone(&source));
        if source.start().is_err() {
            return Err(EmbeddedOpenError::WorkerUnavailable);
        }
        #[cfg(feature = "__test-internals")]
        if take_panic_after_start_for_test(&key) {
            panic!("embedded open panic injected after worker start");
        }

        let promoted = {
            let mut open = self.open.lock().expect("embedded registration mutex");
            match open.get_mut(&key) {
                Some(reservation) if reservation.identity == identity && reservation.opening => {
                    reservation.opening = false;
                    reservation.binding = Some(Arc::clone(&source));
                    true
                }
                _ => false,
            }
        };
        if !promoted {
            return Err(EmbeddedOpenError::WorkerUnavailable);
        }
        rollback.commit();

        Ok(EmbeddedSourceHandle {
            identity,
            key,
            registration: Arc::clone(self),
            binding: Some(source),
            closed: AtomicBool::new(false),
            _not_unwind_safe: std::marker::PhantomData,
        })
    }

    /// How many sources are open.
    pub fn open_count(&self) -> usize {
        self.open.lock().expect("embedded registration mutex").len()
    }

    /// Whether the factory is currently shut down: the final owner closed and
    /// nothing has been opened since.
    pub fn has_shut_down(&self) -> bool {
        self.shutdown.load(Ordering::Acquire)
    }

    /// Close one source. Returns whether this call performed the shutdown.
    ///
    /// The registration entry stays until join finishes so a same-root reopen
    /// still refuses, but the mutex is not held across `join` — an unrelated
    /// root must be able to open while this worker is still unwinding.
    fn close_one(&self, key: &ProjectKey, identity: EmbeddedIdentity) -> (bool, bool, u64, u64) {
        let (matched, source) = {
            let open = self.open.lock().expect("embedded registration mutex");
            let matched = open
                .get(key)
                .is_some_and(|source| source.identity == identity);
            let source = open
                .get(key)
                .filter(|source| source.identity == identity)
                .and_then(|source| source.binding.as_ref().map(Arc::clone));
            (matched, source)
        };
        let (terminal_version, joined_workers) = source
            .as_ref()
            .map(|source| source.shutdown())
            .unwrap_or((0, 0));
        let (performed, final_owner) = if matched {
            let mut open = self.open.lock().expect("embedded registration mutex");
            if open
                .get(key)
                .is_some_and(|source| source.identity == identity)
            {
                open.remove(key);
                let final_owner = open.is_empty();
                if final_owner {
                    self.shutdown.store(true, Ordering::Release);
                }
                (true, final_owner)
            } else {
                (false, false)
            }
        } else {
            (false, false)
        };
        (performed, final_owner, terminal_version, joined_workers)
    }

    pub(crate) fn shutdown_owner(&self, owner: EmbeddedIdentity) -> EmbeddedShutdownReport {
        let closing: Vec<(ProjectKey, Option<Arc<EmbeddedBinding>>)> = {
            let open = self.open.lock().expect("embedded registration mutex");
            open.iter()
                .filter(|(_, source)| source.owner == Some(owner))
                .map(|(key, source)| (key.clone(), source.binding.clone()))
                .collect()
        };
        let mut closed_sources = 0u64;
        let mut joined_workers = 0u64;
        for (_, binding) in &closing {
            let joined = binding
                .as_ref()
                .map(|binding| binding.shutdown().1)
                .unwrap_or(0);
            closed_sources = closed_sources.saturating_add(1);
            joined_workers = joined_workers.saturating_add(joined);
        }
        let mut open = self.open.lock().expect("embedded registration mutex");
        for (key, _) in &closing {
            if open
                .get(key)
                .is_some_and(|source| source.owner == Some(owner))
            {
                open.remove(key);
            }
        }
        self.shutdown.store(open.is_empty(), Ordering::Release);
        EmbeddedShutdownReport {
            closed_sources,
            joined_workers,
        }
    }
}

#[cfg(feature = "embed")]
impl super::public_api::ProcessRuntimeApi {
    /// Open a source with state placement selected by the trusted host. These
    /// options are Rust capabilities, never fields decoded from a room query.
    pub fn open_embedded_source_with_options(
        &self,
        spec: super::public_api::EmbeddedSourceSpec,
        options: crate::embed::parity::source_options::EmbeddedOpenOptions,
    ) -> Result<EmbeddedSourceHandle, super::public_api::EmbedSourceRefusal> {
        use super::public_api::{
            OperationKind, RetryAdvice, SourceRefusalKind, bound_source_refusal,
        };
        use crate::embed::parity::source_options::{StateSelectionError, select_state_placement};
        let normalized = format!(
            "current_worktree={:?};state={:?};allow_protected_root={};reset_snapshot_state={};replay_control_directory={:?}",
            spec.root,
            options.state,
            options.allow_protected_root,
            options.reset_snapshot_state,
            options.replay_control_directory
        );
        let refuse = |kind, retry| {
            bound_source_refusal(
                kind,
                OperationKind::OpenEmbeddedSource,
                retry,
                normalized.as_bytes(),
            )
        };
        // MCP `index_folder` resolves an explicit root; without the override it
        // admits exactly what automatic resolution admits.
        let (candidate_source, request_mode) = if options.allow_protected_root {
            (
                crate::domain::RootCandidateSource::ExplicitIndexFolder,
                crate::domain::RootRequestMode::ExplicitIndexFolder {
                    allow_protected_root: true,
                },
            )
        } else {
            (
                crate::domain::RootCandidateSource::McpClientRoot,
                crate::domain::RootRequestMode::Automatic,
            )
        };
        let binding = match crate::discovery::resolve_root_candidate(
            &spec.root,
            candidate_source,
            request_mode,
        ) {
            crate::domain::RootResolution::Bound(binding) => binding,
            crate::domain::RootResolution::Unbound { .. } => {
                return Err(refuse(
                    SourceRefusalKind::InvalidSelection,
                    RetryAdvice::Operator,
                ));
            }
        };
        if options
            .replay_control_directory
            .as_deref()
            .is_some_and(|directory| !directory.is_absolute() || !directory.is_dir())
        {
            return Err(refuse(
                SourceRefusalKind::InvalidSelection,
                RetryAdvice::Operator,
            ));
        }
        let placement =
            select_state_placement(&binding, &options).map_err(|error| match error {
                StateSelectionError::InvalidSelection => {
                    refuse(SourceRefusalKind::InvalidSelection, RetryAdvice::Operator)
                }
                StateSelectionError::Unavailable => refuse(
                    SourceRefusalKind::AdmissionUnavailable,
                    RetryAdvice::Operator,
                ),
            })?;
        self.owner
            .factory()
            .open_bound_with_reset(
                binding,
                placement,
                self.owner.identity(),
                options.reset_snapshot_state,
            )
            .map_err(|error| match error {
                EmbeddedOpenError::SourceAlreadyOpen => refuse(
                    SourceRefusalKind::SelectionUnavailable,
                    RetryAdvice::OnEvent,
                ),
                EmbeddedOpenError::AdmissionUnavailable => refuse(
                    SourceRefusalKind::AdmissionUnavailable,
                    RetryAdvice::Automatic,
                ),
                EmbeddedOpenError::WorkerUnavailable => {
                    refuse(SourceRefusalKind::SourceUnavailable, RetryAdvice::Automatic)
                }
            })
    }
}

#[derive(Debug)]
pub(crate) struct EmbeddedRuntimeOwner {
    identity: EmbeddedIdentity,
    factory: Arc<EmbeddedSourceFactory>,
}

impl EmbeddedRuntimeOwner {
    pub(crate) fn new(factory: Arc<EmbeddedSourceFactory>) -> Arc<Self> {
        Arc::new(Self {
            identity: EmbeddedIdentity::fresh(),
            factory,
        })
    }

    pub(crate) fn factory(&self) -> &Arc<EmbeddedSourceFactory> {
        &self.factory
    }

    pub(crate) fn identity(&self) -> EmbeddedIdentity {
        self.identity
    }

    pub(crate) fn shutdown(&self) -> EmbeddedShutdownReport {
        self.factory.shutdown_owner(self.identity)
    }
}

impl Drop for EmbeddedRuntimeOwner {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// The sole handle to one embedded source.
///
/// Not `Clone`: sole ownership is the invariant, and a clonable handle would
/// mean two owners could each believe they hold the source.
#[derive(Debug)]
pub struct EmbeddedSourceHandle {
    identity: EmbeddedIdentity,
    key: ProjectKey,
    registration: Arc<EmbeddedSourceFactory>,
    binding: Option<Arc<EmbeddedBinding>>,
    closed: AtomicBool,
    // T049: the contract pins the handle NOT UnwindSafe/RefUnwindSafe.
    _not_unwind_safe: super::public_api::NotUnwindSafe,
}

impl EmbeddedSourceHandle {
    /// Prepare a bounded disposable Git view using a trusted host workspace.
    /// This lifecycle operation grants no room permission to prepare other state.
    #[cfg(feature = "embed")]
    pub fn prepare_git_view(
        &self,
        options: &crate::embed::parity::source_options::GitPreparationOptions,
        control: &crate::embed::parity::host::OperationControl,
    ) -> Result<
        crate::embed::parity::source_options::GitPreparationClaim,
        crate::embed::parity::source_options::GitPreparationRefusal,
    > {
        use crate::embed::parity::source_options::{
            GitPreparationRefusal, GitPreparationRefusalKind,
        };
        let unavailable = || GitPreparationRefusal {
            kind: GitPreparationRefusalKind::SourceUnavailable,
        };
        let snapshot = self
            .capture_query_snapshot(b"symforge.embed.git-preparation.v1")
            .map_err(|_| unavailable())?;
        let binding = self.binding.as_ref().ok_or_else(unavailable)?;
        let mut current = binding.git_view.lock().expect("embedded Git view mutex");
        let (prepared, claim) =
            super::embed_git::prepare(&snapshot, options, control, current.as_ref())?;
        let final_snapshot = self
            .capture_query_snapshot(b"symforge.embed.git-preparation.v1")
            .map_err(|_| unavailable())?;
        if final_snapshot.serving_publication_identity != snapshot.serving_publication_identity
            || binding.shutdown_started.load(Ordering::Acquire)
        {
            return Err(unavailable());
        }
        control.check().map_err(|stop| GitPreparationRefusal {
            kind: match stop {
                crate::embed::parity::host::OperationStop::Cancelled => {
                    GitPreparationRefusalKind::Cancelled
                }
                crate::embed::parity::host::OperationStop::DeadlineExceeded => {
                    GitPreparationRefusalKind::DeadlineExceeded
                }
            },
        })?;
        *current = Some(prepared);
        Ok(claim)
    }

    /// Open the current prepared Git view exactly as production does.
    #[cfg(all(feature = "embed", feature = "__test-internals"))]
    pub fn prepared_git_repository_for_test(&self) -> Option<git2::Repository> {
        let view = self.binding.as_ref()?.git_view.lock().ok()?.clone()?;
        view.repository().ok()
    }

    /// Execute a bounded query against one admitted, immutable publication.
    #[cfg(feature = "embed")]
    pub fn query(
        &self,
        request: &crate::embed::parity::QueryRequest,
        limits: crate::embed::parity::QueryLimits,
    ) -> Result<super::embed_query::QueryClaim, super::embed_query::QueryRefusal> {
        super::embed_query::execute(self, request, limits)
    }

    /// Execute with the host's deadline and shared cancellation signal.
    /// Stops are checked around engine calls and while projecting rows; a single
    /// parser/search call runs to its next safe checkpoint. The convenience
    /// `query` method imposes output bounds without choosing a host deadline.
    #[cfg(feature = "embed")]
    pub fn query_with_control(
        &self,
        request: &crate::embed::parity::QueryRequest,
        limits: crate::embed::parity::QueryLimits,
        control: &crate::embed::parity::host::OperationControl,
    ) -> Result<super::embed_query::QueryClaim, super::embed_query::QueryRefusal> {
        super::embed_query::execute_with_control(self, request, limits, control)
    }

    /// Create an independent context history and retrieval cache bound to this
    /// admitted source incarnation.
    #[cfg(feature = "embed")]
    pub fn new_query_session(
        &self,
    ) -> Result<crate::embed::parity::session::QuerySession, super::public_api::EmbedSourceRefusal>
    {
        let snapshot = self.capture_query_snapshot(b"symforge.embed.session.v1")?;
        Ok(super::embed_session::QuerySession::new(&snapshot))
    }

    /// Create a session with trusted bounds no larger than the engine defaults.
    #[cfg(feature = "embed")]
    pub fn new_query_session_with_limits(
        &self,
        limits: crate::embed::parity::session::SessionCacheLimits,
    ) -> Result<
        crate::embed::parity::session::QuerySession,
        crate::embed::parity::session::SessionCreationRefusal,
    > {
        use crate::embed::parity::session::{SessionCacheLimits, SessionCreationRefusal};
        let defaults = SessionCacheLimits::default();
        if limits.max_bytes > defaults.max_bytes || limits.max_entries > defaults.max_entries {
            return Err(SessionCreationRefusal::InvalidLimits);
        }
        let snapshot = self
            .capture_query_snapshot(b"symforge.embed.session.v1")
            .map_err(|_| SessionCreationRefusal::SourceUnavailable)?;
        Ok(super::embed_session::QuerySession::with_limits(
            &snapshot, limits,
        ))
    }

    /// Execute with explicit source-bound context history and optional host
    /// cancellation. A session from another source is refused.
    #[cfg(feature = "embed")]
    pub fn query_with_session(
        &self,
        request: &crate::embed::parity::QueryRequest,
        limits: crate::embed::parity::QueryLimits,
        session: &crate::embed::parity::session::QuerySession,
        control: Option<&crate::embed::parity::host::OperationControl>,
    ) -> Result<super::embed_query::QueryClaim, super::embed_query::QueryRefusal> {
        super::embed_query::execute_with_session(self, request, limits, session, control)
    }

    /// Execute with explicit host rights for optional derived-state preparation.
    /// Read-only queries use the default policy; a grant does not create state
    /// placement authority or change the source bound to this handle.
    #[cfg(feature = "embed")]
    pub fn query_with_policy(
        &self,
        request: &crate::embed::parity::QueryRequest,
        limits: crate::embed::parity::QueryLimits,
        policy: crate::embed::parity::QueryPolicy,
        session: Option<&crate::embed::parity::session::QuerySession>,
        control: Option<&crate::embed::parity::host::OperationControl>,
    ) -> Result<super::embed_query::QueryClaim, super::embed_query::QueryRefusal> {
        super::embed_query::execute_with_policy(self, request, limits, policy, session, control)
    }

    /// Run a trusted synchronous envelope check before committing read bookkeeping.
    /// The callback must not reenter this session and must not deliver the result
    /// before this method succeeds; cancellation is checked again after admission.
    #[cfg(feature = "embed")]
    pub(crate) fn query_with_policy_admitted(
        &self,
        request: &crate::embed::parity::QueryRequest,
        limits: crate::embed::parity::QueryLimits,
        policy: crate::embed::parity::QueryPolicy,
        session: Option<&crate::embed::parity::session::QuerySession>,
        control: Option<&crate::embed::parity::host::OperationControl>,
        admit: impl FnOnce(&super::embed_query::QueryClaim) -> bool,
    ) -> Result<super::embed_query::QueryClaim, super::embed_query::QueryRefusal> {
        super::embed_query::execute_with_policy_admitted(
            self, request, limits, policy, session, control, admit,
        )
    }

    /// Original admitted placement for verifying completed replay records while
    /// a refresh is in progress. This carries no query or mutation authority.
    #[cfg(feature = "embed")]
    pub(super) fn bound_replay_placement(&self) -> Option<(PathBuf, Option<PathBuf>)> {
        if self.closed.load(Ordering::Acquire) {
            return None;
        }
        let binding = self.binding.as_ref()?;
        let state = binding.state.lock().expect("embedded state mutex");
        if binding.shutdown_started.load(Ordering::Acquire)
            || !matches!(
                state.phase,
                super::public_api::SourceRuntimePhase::Loading
                    | super::public_api::SourceRuntimePhase::Current
                    | super::public_api::SourceRuntimePhase::Refreshing
            )
            || self.closed.load(Ordering::Acquire)
        {
            return None;
        }
        Some((
            binding.root.clone(),
            binding
                .state_placement
                .directory()
                .map(|directory| directory.as_path().to_path_buf()),
        ))
    }

    /// Original read capability for verifying completed postimages while the
    /// source refreshes. It never re-resolves authority from a path spelling.
    #[cfg(feature = "embed")]
    pub(super) fn bound_replay_authority(
        &self,
    ) -> Option<Arc<super::activation::ProjectSourceAuthority>> {
        self.bound_replay_placement()?;
        Some(Arc::clone(&self.binding.as_ref()?.authority))
    }

    #[cfg(feature = "embed")]
    pub(super) fn bound_state_anchor(&self) -> Option<AdmittedStateAnchor> {
        self.bound_replay_placement()?;
        self.binding.as_ref()?.state_anchor.clone()
    }

    /// Freshen one exact path before a targeted read; see the binding's
    /// `freshen_exact_path`.
    #[cfg(feature = "embed")]
    pub(super) fn freshen_exact_path(
        &self,
        relative_path: &str,
    ) -> Result<(), super::guidance::freshen::TargetedFreshenRefusal> {
        match &self.binding {
            Some(binding) if !self.closed.load(Ordering::Acquire) => {
                binding.freshen_exact_path(relative_path)
            }
            _ => Ok(()),
        }
    }

    /// Internal read capture shared by parity queries and edit previews. It is
    /// never a write permit. Capture source state and its matching data plane
    /// together, refusing a publication swap instead of mislabelling its bytes.
    #[cfg(feature = "embed")]
    pub(super) fn capture_query_snapshot(
        &self,
        normalized: &[u8],
    ) -> Result<super::embed_query::EmbeddedQuerySnapshot, super::public_api::EmbedSourceRefusal>
    {
        use super::public_api::{
            OperationKind, RetryAdvice, SourceRefusalKind, SourceRuntimePhase,
        };
        let refuse = |kind, retry| {
            super::public_api::bound_source_refusal(
                kind,
                OperationKind::IndexCensus,
                retry,
                normalized,
            )
        };
        if self.closed.load(Ordering::Acquire) {
            return Err(refuse(
                SourceRefusalKind::SourceUnavailable,
                RetryAdvice::Never,
            ));
        }
        let binding = self
            .binding
            .as_ref()
            .ok_or_else(|| refuse(SourceRefusalKind::SourceUnavailable, RetryAdvice::Never))?;
        let state = binding.state.lock().expect("embedded state mutex");
        if state.phase != SourceRuntimePhase::Current {
            return Err(refuse(
                SourceRefusalKind::SourceUnavailable,
                RetryAdvice::OnEvent,
            ));
        }
        let index = binding
            .runtime
            .acquire()
            .map_err(|_| refuse(SourceRefusalKind::AdmissionUnavailable, RetryAdvice::Never))?;
        let authority = Arc::clone(&binding.authority);
        authority
            .verify_physical_root_anchor()
            .map_err(|_| refuse(SourceRefusalKind::SourceUnavailable, RetryAdvice::OnEvent))?;
        let authority_publication = authority
            .current_publication()
            .ok_or_else(|| refuse(SourceRefusalKind::SourceUnavailable, RetryAdvice::OnEvent))?;
        let source_set = index.published_source_set();
        let generation = source_set.current_generation();
        let publication_identity = format!(
            "embed-publication-{}-{}",
            self.identity.raw(),
            generation.publication_generation
        );
        if state.current_publication_identity.as_deref() != Some(&publication_identity)
            || !matches!(generation.freshness.as_ref(), FreshnessStatus::Current)
            || authority.current_publication() != Some(authority_publication)
            || self.closed.load(Ordering::Acquire)
        {
            return Err(refuse(
                SourceRefusalKind::SourceUnavailable,
                RetryAdvice::OnEvent,
            ));
        }
        let binding_identity = format!("source-{}", self.identity.raw());
        let serving_publication_identity = super::embed_query::serving_publication_identity(
            &binding_identity,
            &publication_identity,
            state.source_version,
        );
        Ok(super::embed_query::EmbeddedQuerySnapshot {
            root: binding.root.clone(),
            project_state: binding.state_placement.directory().cloned(),
            state_dir: binding
                .state_placement
                .directory()
                .map(|directory| directory.as_path().to_path_buf()),
            source_set,
            generation,
            binding_identity,
            publication_identity,
            serving_publication_identity,
            source_version: state.source_version,
            authority_publication,
            authority,
            state_anchor: binding.state_anchor.clone(),
        })
    }

    /// Health capture: the bound data plane, root and placement in ANY live
    /// phase. Health reports Loading/Blocked state from the published set like
    /// the MCP handler; it never answers a query, so it needs no Current claim.
    #[cfg(feature = "embed")]
    pub(super) fn capture_health_context(
        &self,
    ) -> Option<(crate::live_index::SharedIndex, PathBuf, StatePlacement)> {
        if self.closed.load(Ordering::Acquire) {
            return None;
        }
        let binding = self.binding.as_ref()?;
        let index = binding.runtime.acquire().ok()?;
        Some((
            Arc::clone(index),
            binding.root.clone(),
            binding.state_placement.clone(),
        ))
    }

    /// Internal curation context. A read capture is insufficient to authorize
    /// apply: the coordinator must separately require host authority and its
    /// own durability, review and mutation fences.
    #[cfg(feature = "embed")]
    pub(super) fn capture_curation_context(
        &self,
        normalized: &[u8],
    ) -> Result<
        (
            super::embed_query::EmbeddedQuerySnapshot,
            crate::live_index::SharedIndex,
            StatePlacement,
        ),
        super::public_api::EmbedSourceRefusal,
    > {
        let snapshot = self.capture_query_snapshot(normalized)?;
        let refuse = || {
            super::public_api::bound_source_refusal(
                super::public_api::SourceRefusalKind::SourceUnavailable,
                super::public_api::OperationKind::IndexCensus,
                super::public_api::RetryAdvice::OnEvent,
                normalized,
            )
        };
        let binding = self.binding.as_ref().ok_or_else(refuse)?;
        let index = binding.runtime.acquire().map_err(|_| refuse())?;
        if self.closed.load(Ordering::Acquire)
            || !Arc::ptr_eq(&index.published_generation(), &snapshot.generation)
        {
            return Err(refuse());
        }
        Ok((snapshot, Arc::clone(index), binding.state_placement.clone()))
    }

    /// This open's identity.
    pub fn identity(&self) -> EmbeddedIdentity {
        self.identity
    }

    /// The key it holds open.
    pub fn key(&self) -> &ProjectKey {
        &self.key
    }

    /// Whether this handle is still open.
    pub fn is_open(&self) -> bool {
        !self.closed.load(Ordering::Acquire)
            && self.binding.as_ref().is_none_or(|binding| {
                !matches!(
                    binding.runtime_view().phase,
                    super::public_api::SourceRuntimePhase::Stopped
                        | super::public_api::SourceRuntimePhase::Stopping
                )
            })
    }

    /// Close the source.
    ///
    /// Coalesces with `Drop`: whichever runs first performs the shutdown, and
    /// the other reports that it joined rather than performed it. Refuses with
    /// [`EmbedRefusal::WouldSelfWait`] when called from inside a finalizer,
    /// because waiting there is waiting on the calling thread itself.
    pub fn close(&self) -> Result<CloseReceipt, EmbedRefusal> {
        if FINALIZING.with(std::cell::Cell::get) == Some(self.identity) {
            return Err(EmbedRefusal::WouldSelfWait);
        }
        if self.closed.swap(true, Ordering::AcqRel) {
            return Err(EmbedRefusal::AlreadyClosed);
        }
        let (performed, final_owner, _, _) = self.registration.close_one(&self.key, self.identity);
        Ok(CloseReceipt {
            identity: self.identity,
            performed_shutdown: performed,
            was_final_owner: final_owner,
        })
    }

    /// V11 (T047, E1 ruling): begin the close INFALLIBLY. The Slice 2 `close`
    /// refuses a self-wait AT CLOSE; the V11 contract relocates that guard to
    /// the WAIT — beginning a close is always legal, and only waiting on your
    /// own close from inside the finalizer refuses. An already-closed source
    /// yields a receipt that joined rather than performed, same as `Drop`
    /// coalescing.
    pub fn begin_close(&self) -> SourceCloseReceipt {
        let performed = if self.closed.swap(true, Ordering::AcqRel) {
            false
        } else {
            let (performed, _final_owner, terminal_version, _joined_workers) =
                self.registration.close_one(&self.key, self.identity);
            return SourceCloseReceipt {
                identity: self.identity,
                performed_shutdown: performed,
                terminal_source_version: terminal_version,
                _not_unwind_safe: std::marker::PhantomData,
            };
        };
        SourceCloseReceipt {
            identity: self.identity,
            performed_shutdown: performed,
            terminal_source_version: self
                .binding
                .as_ref()
                .map(|binding| binding.runtime_view().source_version)
                .unwrap_or(0),
            _not_unwind_safe: std::marker::PhantomData,
        }
    }

    /// Capture the source's lifecycle state atomically.
    pub fn runtime_view(&self) -> super::public_api::SourceRuntimeView {
        self.binding.as_ref().map_or_else(
            || super::public_api::SourceRuntimeView {
                binding_identity: format!("source-{}", self.identity.raw()),
                current_publication_identity: None,
                observer_epoch: 0,
                phase: if self.closed.load(Ordering::Acquire) {
                    super::public_api::SourceRuntimePhase::Stopped
                } else {
                    super::public_api::SourceRuntimePhase::Loading
                },
                source_version: 0,
            },
            |binding| binding.runtime_view(),
        )
    }

    /// Search one pinned current generation and return its claim provenance.
    pub fn search_symbols(
        &self,
        request: &super::public_api::SymbolSearchRequest,
    ) -> Result<
        super::public_api::EmbedClaim<super::public_api::SymbolSearchResult>,
        super::public_api::EmbedSourceRefusal,
    > {
        let normalized = format!(
            "query={:?};path_prefix={:?};limit={}",
            request.query, request.path_prefix, request.limit
        );
        if self.closed.load(Ordering::Acquire) {
            return Err(super::public_api::bound_source_refusal(
                crate::lifecycle_identity::SourceRefusalKind::SourceUnavailable,
                crate::lifecycle_identity::OperationKind::SearchSymbols,
                crate::lifecycle_identity::RetryAdvice::Never,
                normalized.as_bytes(),
            ));
        }
        let Some(binding) = self.binding.as_ref() else {
            return Err(super::public_api::dark_unbound_refusal(
                crate::lifecycle_identity::OperationKind::SearchSymbols,
            ));
        };
        let claim = binding.current_claim(
            crate::lifecycle_identity::OperationKind::SearchSymbols,
            normalized.as_bytes(),
        )?;
        let index = binding.runtime.acquire().map_err(|_| {
            super::public_api::bound_source_refusal(
                crate::lifecycle_identity::SourceRefusalKind::SourceUnavailable,
                crate::lifecycle_identity::OperationKind::SearchSymbols,
                crate::lifecycle_identity::RetryAdvice::Never,
                normalized.as_bytes(),
            )
        })?;
        let live = index.read();
        let query = request.query.as_deref().map(str::to_lowercase);
        let mut matches = Vec::new();
        for (path, file) in live.all_files() {
            if !path_matches_prefix(path, request.path_prefix.as_deref()) {
                continue;
            }
            for symbol in &file.symbols {
                if query
                    .as_deref()
                    .is_some_and(|query| !symbol.name.to_lowercase().contains(query))
                {
                    continue;
                }
                matches.push((
                    symbol.sort_order,
                    super::public_api::SymbolMatch {
                        name: symbol.name.clone(),
                        kind: symbol.kind.to_string(),
                        path: path.clone(),
                        start_line: symbol.line_range.0.saturating_add(1),
                        end_line: symbol.line_range.1.saturating_add(1),
                    },
                ));
            }
        }
        matches.sort_by(|(left_order, left), (right_order, right)| {
            left.path
                .cmp(&right.path)
                .then_with(|| left.start_line.cmp(&right.start_line))
                .then_with(|| left_order.cmp(right_order))
                .then_with(|| left.name.cmp(&right.name))
                .then_with(|| left.kind.cmp(&right.kind))
        });
        let limit = request.limit as usize;
        let truncated = matches.len() > limit;
        let matches = matches
            .into_iter()
            .take(limit)
            .map(|(_, value)| value)
            .collect();
        Ok(super::public_api::live_claim(
            super::public_api::SymbolSearchResult { matches, truncated },
            crate::lifecycle_identity::OperationKind::SearchSymbols,
            normalized.as_bytes(),
            &claim.binding_identity,
            &claim.publication_identity,
            claim.source_version,
        ))
    }

    /// Search stored byte-exact file content from one pinned current generation.
    pub fn search_text(
        &self,
        request: &super::public_api::TextSearchRequest,
    ) -> Result<
        super::public_api::EmbedClaim<super::public_api::TextSearchResult>,
        super::public_api::EmbedSourceRefusal,
    > {
        let normalized = format!(
            "query={:?};path_prefix={:?};limit={};case_sensitive={}",
            request.query, request.path_prefix, request.limit, request.case_sensitive
        );
        if request.query.is_empty() {
            return Err(super::public_api::bound_source_refusal(
                crate::lifecycle_identity::SourceRefusalKind::InvalidSelection,
                crate::lifecycle_identity::OperationKind::SearchText,
                crate::lifecycle_identity::RetryAdvice::Never,
                normalized.as_bytes(),
            ));
        }
        if self.closed.load(Ordering::Acquire) {
            return Err(super::public_api::bound_source_refusal(
                crate::lifecycle_identity::SourceRefusalKind::SourceUnavailable,
                crate::lifecycle_identity::OperationKind::SearchText,
                crate::lifecycle_identity::RetryAdvice::Never,
                normalized.as_bytes(),
            ));
        }
        let Some(binding) = self.binding.as_ref() else {
            return Err(super::public_api::dark_unbound_refusal(
                crate::lifecycle_identity::OperationKind::SearchText,
            ));
        };
        let claim = binding.current_claim(
            crate::lifecycle_identity::OperationKind::SearchText,
            normalized.as_bytes(),
        )?;
        let matcher = regex::RegexBuilder::new(&regex::escape(&request.query))
            .case_insensitive(!request.case_sensitive)
            .build()
            .map_err(|_| {
                super::public_api::bound_source_refusal(
                    crate::lifecycle_identity::SourceRefusalKind::InvalidSelection,
                    crate::lifecycle_identity::OperationKind::SearchText,
                    crate::lifecycle_identity::RetryAdvice::Never,
                    normalized.as_bytes(),
                )
            })?;
        let index = binding.runtime.acquire().map_err(|_| {
            super::public_api::bound_source_refusal(
                crate::lifecycle_identity::SourceRefusalKind::SourceUnavailable,
                crate::lifecycle_identity::OperationKind::SearchText,
                crate::lifecycle_identity::RetryAdvice::Never,
                normalized.as_bytes(),
            )
        })?;
        let live = index.read();
        let limit = request.limit as usize;
        let mut matches = Vec::new();
        let mut truncated = false;
        'files: for (path, file) in live.all_files() {
            if !path_matches_prefix(path, request.path_prefix.as_deref()) {
                continue;
            }
            let Ok(content) = std::str::from_utf8(&file.content) else {
                continue;
            };
            let mut line_start = 0usize;
            for (line_index, line) in content.split_inclusive('\n').enumerate() {
                let searchable = line.strip_suffix('\n').unwrap_or(line);
                for found in matcher.find_iter(searchable) {
                    if matches.len() == limit {
                        truncated = true;
                        break 'files;
                    }
                    let byte_start = line_start.saturating_add(found.start());
                    let byte_end = line_start.saturating_add(found.end());
                    matches.push(super::public_api::TextMatch {
                        path: path.clone(),
                        line: u32::try_from(line_index.saturating_add(1)).unwrap_or(u32::MAX),
                        byte_start: byte_start as u64,
                        byte_end: byte_end as u64,
                        preview: bounded_preview(searchable.trim_end_matches('\r')),
                    });
                }
                line_start = line_start.saturating_add(line.len());
            }
        }
        Ok(super::public_api::live_claim(
            super::public_api::TextSearchResult { matches, truncated },
            crate::lifecycle_identity::OperationKind::SearchText,
            normalized.as_bytes(),
            &claim.binding_identity,
            &claim.publication_identity,
            claim.source_version,
        ))
    }

    /// Census one pinned current generation's parsed files, including zeros.
    pub fn index_census(
        &self,
    ) -> Result<
        super::public_api::EmbedClaim<super::public_api::IndexCensus>,
        super::public_api::EmbedSourceRefusal,
    > {
        let normalized = b"index-census";
        if self.closed.load(Ordering::Acquire) {
            return Err(super::public_api::bound_source_refusal(
                crate::lifecycle_identity::SourceRefusalKind::SourceUnavailable,
                crate::lifecycle_identity::OperationKind::IndexCensus,
                crate::lifecycle_identity::RetryAdvice::Never,
                normalized,
            ));
        }
        let Some(binding) = self.binding.as_ref() else {
            return Err(super::public_api::dark_unbound_refusal(
                crate::lifecycle_identity::OperationKind::IndexCensus,
            ));
        };
        let claim = binding.current_claim(
            crate::lifecycle_identity::OperationKind::IndexCensus,
            normalized,
        )?;
        let index = binding.runtime.acquire().map_err(|_| {
            super::public_api::bound_source_refusal(
                crate::lifecycle_identity::SourceRefusalKind::SourceUnavailable,
                crate::lifecycle_identity::OperationKind::IndexCensus,
                crate::lifecycle_identity::RetryAdvice::Never,
                normalized,
            )
        })?;
        let generation = index.published_generation();
        let parsed: std::collections::BTreeSet<String> = generation
            .live
            .all_files()
            .filter(|(_, file)| {
                matches!(
                    file.parse_status,
                    crate::live_index::store::ParseStatus::Parsed
                        | crate::live_index::store::ParseStatus::PartialParse { .. }
                )
            })
            .map(|(path, _)| path.clone())
            .collect();
        crate::live_index::store::pause_census_after_first_read_for_test(&binding.root);
        let files: Vec<super::public_api::CensusFile> = generation
            .outline
            .files
            .iter()
            .filter(|file| parsed.contains(&file.relative_path))
            .map(|file| super::public_api::CensusFile {
                path: file.relative_path.clone(),
                language: file.language.name().to_string(),
                symbol_count: u32::try_from(file.symbol_count).unwrap_or(u32::MAX),
            })
            .collect();
        let total_files = files.len() as u64;
        let total_symbols = files.iter().map(|file| u64::from(file.symbol_count)).sum();
        Ok(super::public_api::live_claim(
            super::public_api::IndexCensus {
                total_files,
                total_symbols,
                files,
            },
            crate::lifecycle_identity::OperationKind::IndexCensus,
            normalized,
            &claim.binding_identity,
            &claim.publication_identity,
            claim.source_version,
        ))
    }

    /// Binding-owned reload counters. Always succeeds; zeros if unbound.
    pub fn index_progress(&self) -> super::public_api::IndexProgress {
        self.binding
            .as_ref()
            .map_or_else(super::public_api::IndexProgress::default, |binding| {
                let (files_discovered, files_parsed, symbols_found) = binding.progress.snapshot();
                super::public_api::IndexProgress {
                    files_discovered,
                    files_parsed,
                    symbols_found,
                }
            })
    }

    /// Search admitted knowledge from one pinned current generation.
    pub fn search_knowledge(
        &self,
        request: &super::public_api::KnowledgeSearchRequest,
    ) -> Result<
        super::public_api::EmbedClaim<super::public_api::KnowledgeSearchResult>,
        super::public_api::EmbedSourceRefusal,
    > {
        let normalized = format!(
            "query={:?};path_prefix={:?};limit={}",
            request.query, request.path_prefix, request.limit
        );
        if self.closed.load(Ordering::Acquire) {
            return Err(super::public_api::bound_source_refusal(
                crate::lifecycle_identity::SourceRefusalKind::SourceUnavailable,
                crate::lifecycle_identity::OperationKind::SearchKnowledge,
                crate::lifecycle_identity::RetryAdvice::Never,
                normalized.as_bytes(),
            ));
        }
        let Some(binding) = self.binding.as_ref() else {
            return Err(super::public_api::dark_unbound_refusal(
                crate::lifecycle_identity::OperationKind::SearchKnowledge,
            ));
        };
        let parsed = crate::live_index::knowledge_retrieve::KnowledgeRetrieveRequest::parse(
            &request.query,
            request.path_prefix.as_deref(),
            crate::live_index::knowledge_retrieve::KnowledgeRetrieveAuthorityScope::Default,
            request.limit as usize,
        )
        .map_err(|_| {
            super::public_api::bound_source_refusal(
                crate::lifecycle_identity::SourceRefusalKind::InvalidSelection,
                crate::lifecycle_identity::OperationKind::SearchKnowledge,
                crate::lifecycle_identity::RetryAdvice::Never,
                normalized.as_bytes(),
            )
        })?;
        let claim = binding.current_claim(
            crate::lifecycle_identity::OperationKind::SearchKnowledge,
            normalized.as_bytes(),
        )?;
        let index = binding.runtime.acquire().map_err(|_| {
            super::public_api::bound_source_refusal(
                crate::lifecycle_identity::SourceRefusalKind::SourceUnavailable,
                crate::lifecycle_identity::OperationKind::SearchKnowledge,
                crate::lifecycle_identity::RetryAdvice::Never,
                normalized.as_bytes(),
            )
        })?;
        let generation = index.published_generation();
        let retrieved = crate::live_index::knowledge_retrieve::retrieve_knowledge(
            &[
                crate::live_index::knowledge_retrieve::KnowledgeRetrieveLane {
                    generation: &generation,
                    label: "current",
                },
            ],
            &parsed,
        );
        let matches = retrieved
            .hits
            .into_iter()
            .map(|hit| super::public_api::KnowledgeMatch {
                path: hit.path,
                heading_path: hit.heading_path,
                preview: hit.preview,
                content_hash: hit.content_hash,
                provenance_ids: hit.authority.provenance_ids,
                relationship_evidence: hit
                    .relationship_evidence
                    .iter()
                    .map(|evidence| evidence.preview_token())
                    .collect(),
                authority: knowledge_lifecycle_label(hit.authority.lifecycle),
                coverage: knowledge_coverage_label(&hit.authority.coverage),
            })
            .collect();
        Ok(super::public_api::live_claim(
            super::public_api::KnowledgeSearchResult {
                matches,
                truncated: retrieved.truncated,
                withheld_count: retrieved.withheld_count as u64,
                withheld_reasons: retrieved
                    .withheld_reasons
                    .into_iter()
                    .map(knowledge_withheld_label)
                    .collect(),
            },
            crate::lifecycle_identity::OperationKind::SearchKnowledge,
            normalized.as_bytes(),
            &claim.binding_identity,
            &claim.publication_identity,
            claim.source_version,
        ))
    }

    #[cfg(feature = "__test-internals")]
    pub fn revoke_admission_for_test(&self) {
        if let Some(binding) = &self.binding {
            binding.runtime.revoke_admission_for_test();
        }
    }

    /// Signal reload cancel without joining the worker. Close still joins.
    #[cfg(feature = "__test-internals")]
    pub fn cancel_reload_for_test(&self) {
        if let Some(binding) = &self.binding {
            binding.shutdown_started.store(true, Ordering::Release);
            let mut control = binding.control.lock().expect("embedded control mutex");
            control.stop = true;
            binding.wake.notify_all();
        }
    }

    /// Queue a refresh on this source's worker.
    pub fn request_refresh(
        &self,
    ) -> Result<super::public_api::EmbedRefreshTicket, super::public_api::EmbedSourceRefusal> {
        let normalized = b"refresh-current-worktree";
        if self.closed.load(Ordering::Acquire) {
            return Err(super::public_api::bound_source_refusal(
                crate::lifecycle_identity::SourceRefusalKind::SourceUnavailable,
                crate::lifecycle_identity::OperationKind::RefreshSource,
                crate::lifecycle_identity::RetryAdvice::Never,
                normalized,
            ));
        }
        let Some(binding) = self.binding.as_ref() else {
            return Err(super::public_api::dark_unbound_refusal(
                crate::lifecycle_identity::OperationKind::RefreshSource,
            ));
        };
        let Some(source_version) = binding.request_refresh(false) else {
            return Err(super::public_api::bound_source_refusal(
                crate::lifecycle_identity::SourceRefusalKind::SourceUnavailable,
                crate::lifecycle_identity::OperationKind::RefreshSource,
                crate::lifecycle_identity::RetryAdvice::Never,
                normalized,
            ));
        };
        Ok(super::public_api::refresh_ticket(
            normalized,
            source_version,
        ))
    }

    /// MCP `index_folder` in-place reset: delete the persisted snapshot scope
    /// through the shared `persist::reset_snapshot_state`, then request a full
    /// reload. The reset generation is recorded once that reload publishes.
    #[cfg(feature = "embed")]
    pub fn request_refresh_with_reset(
        &self,
    ) -> Result<
        (
            super::public_api::EmbedRefreshTicket,
            crate::embed::parity::source_options::SnapshotResetReceipt,
        ),
        super::public_api::EmbedSourceRefusal,
    > {
        let normalized = b"refresh-current-worktree;reset_snapshot_state=true";
        let refuse = |kind, retry| {
            super::public_api::bound_source_refusal(
                kind,
                crate::lifecycle_identity::OperationKind::RefreshSource,
                retry,
                normalized,
            )
        };
        if self.closed.load(Ordering::Acquire) {
            return Err(refuse(
                crate::lifecycle_identity::SourceRefusalKind::SourceUnavailable,
                crate::lifecycle_identity::RetryAdvice::Never,
            ));
        }
        let Some(binding) = self.binding.as_ref() else {
            return Err(super::public_api::dark_unbound_refusal(
                crate::lifecycle_identity::OperationKind::RefreshSource,
            ));
        };
        let report = crate::live_index::persist::reset_snapshot_state(
            &binding.root,
            &binding.state_placement,
        )
        .map_err(|_| {
            refuse(
                crate::lifecycle_identity::SourceRefusalKind::AdmissionUnavailable,
                crate::lifecycle_identity::RetryAdvice::Operator,
            )
        })?;
        let Some(source_version) = binding.request_refresh(true) else {
            return Err(refuse(
                crate::lifecycle_identity::SourceRefusalKind::SourceUnavailable,
                crate::lifecycle_identity::RetryAdvice::Never,
            ));
        };
        Ok((
            super::public_api::refresh_ticket(normalized, source_version),
            crate::embed::parity::source_options::SnapshotResetReceipt::from_report(&report),
        ))
    }

    /// The snapshot reset performed by `EmbeddedOpenOptions::reset_snapshot_state`,
    /// or `None` when the open did not reset.
    #[cfg(feature = "embed")]
    pub fn open_reset_receipt(
        &self,
    ) -> Option<crate::embed::parity::source_options::SnapshotResetReceipt> {
        self.binding.as_ref()?.open_reset.clone()
    }

    /// Fixture probe for the relocated guard: arms the finalizer for THIS
    /// source and attempts to wait on its own close receipt from inside it —
    /// which must refuse with [`ReceiptWaitError::WouldSelfWait`] rather than
    /// deadlock.
    #[cfg(all(test, feature = "server"))]
    pub fn self_wait_probe_for_test(&self) -> Result<SourceCloseReport, ReceiptWaitError> {
        let receipt = self.begin_close();
        self.finalize(|| receipt.wait_for_test())
    }

    /// Run `finalizer` with self-wait detection armed for THIS source.
    ///
    /// A finalizer that tries to close this source is refused rather than
    /// deadlocked; a finalizer that closes some other source is none of this
    /// source's business and proceeds. The previous value is restored even if
    /// the finalizer panics, so one bad finalizer cannot poison every later
    /// close on this thread, and nesting two finalizers cannot leave the outer
    /// one disarmed.
    pub fn finalize<R>(&self, finalizer: impl FnOnce() -> R) -> R {
        struct Guard(Option<EmbeddedIdentity>);
        impl Drop for Guard {
            fn drop(&mut self) {
                FINALIZING.with(|flag| flag.set(self.0));
            }
        }
        let _guard = Guard(FINALIZING.with(|flag| flag.replace(Some(self.identity))));
        finalizer()
    }
}

/// Waiting on a receipt can refuse; the wait is where the self-wait guard
/// lives in V11, so the error is typed rather than a deadlock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReceiptWaitError {
    /// The deadline passed before the wait completed. A CONTRACT variant
    /// (T049): dark waits complete immediately, so nothing in Slice 3 can
    /// produce it — Slice 4's real waits can. T047's transcription omitted
    /// it, the same defect class as the invented `ServerExit::Clean`, caught
    /// by the dependent-positive fixture once its feature gate was honest.
    DeadlineElapsed,
    /// The wait was attempted from inside this source's own finalizer:
    /// waiting there is waiting on the calling thread itself.
    WouldSelfWait,
}

// T049: `Display` and `Error` are contract-pinned direct impls on this atom.
impl std::fmt::Display for ReceiptWaitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DeadlineElapsed => {
                write!(f, "the deadline passed before the wait completed")
            }
            Self::WouldSelfWait => write!(
                f,
                "waiting on this receipt from inside its own source's finalizer \
                 would wait on the calling thread itself"
            ),
        }
    }
}

impl std::error::Error for ReceiptWaitError {}

/// Receipt for a completed V11 `begin_close`.
#[derive(Debug)]
pub struct SourceCloseReceipt {
    identity: EmbeddedIdentity,
    performed_shutdown: bool,
    terminal_source_version: u64,
    // T049: the contract pins the receipt NOT UnwindSafe/RefUnwindSafe.
    _not_unwind_safe: super::public_api::NotUnwindSafe,
}

impl SourceCloseReceipt {
    /// The contract wait (T049): refuses a self-wait and otherwise returns the
    /// teardown result already observed by synchronous close.
    pub fn wait(
        &self,
        deadline: std::time::Instant,
    ) -> Result<super::public_api::SourceCloseReport, ReceiptWaitError> {
        let _ = deadline;
        if FINALIZING.with(std::cell::Cell::get) == Some(self.identity) {
            return Err(ReceiptWaitError::WouldSelfWait);
        }
        Ok(super::public_api::SourceCloseReport {
            already_terminal: !self.performed_shutdown,
            terminal_source_version: self.terminal_source_version,
        })
    }

    /// Wait for the close to finalize, reporting the INTERNAL observation
    /// record (T047's oracle shape). Refuses a self-wait; completes
    /// immediately otherwise, because the close performed synchronously and
    /// only observed completions may be reported.
    #[cfg(all(test, feature = "server"))]
    pub fn wait_for_test(&self) -> Result<SourceCloseReport, ReceiptWaitError> {
        if FINALIZING.with(std::cell::Cell::get) == Some(self.identity) {
            return Err(ReceiptWaitError::WouldSelfWait);
        }
        Ok(SourceCloseReport {
            finalized: true,
            performed_shutdown: self.performed_shutdown,
        })
    }
}

/// What the close wait OBSERVED.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceCloseReport {
    finalized: bool,
    performed_shutdown: bool,
}

impl SourceCloseReport {
    pub fn finalized(&self) -> bool {
        self.finalized
    }

    pub fn performed_shutdown(&self) -> bool {
        self.performed_shutdown
    }
}

impl Drop for EmbeddedSourceHandle {
    fn drop(&mut self) {
        // Coalesce with an explicit close. A handle dropped without closing must
        // still release the source, or the registration would believe a source
        // is open that nothing holds.
        //
        // `Drop` deliberately does NOT consult `FINALIZING`. The self-wait
        // hazard `close` refuses is about WAITING, and `close_one` takes a
        // mutex and returns — it never waits on the finalizer. Refusing here
        // would be worse than the hazard: a `Drop` cannot report a refusal, so
        // the source would simply stay open forever with nothing holding it.
        // This is stated rather than left as an inconsistency between the two
        // paths, which is how it read before: `a.finalize(|| drop(b))` was
        // permitted while `a.finalize(|| b.close())` was refused.
        if !self.closed.swap(true, Ordering::AcqRel) {
            let _ = self.registration.close_one(&self.key, self.identity);
        }
    }
}
