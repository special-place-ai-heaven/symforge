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
}

struct EmbeddedBinding {
    identity: EmbeddedIdentity,
    key: ProjectKey,
    root: PathBuf,
    state_placement: StatePlacement,
    runtime: super::activation::ProjectRuntimeHandle,
    state: std::sync::Mutex<EmbeddedRuntimeState>,
    control: std::sync::Mutex<WorkerControl>,
    wake: Condvar,
    worker: std::sync::Mutex<Option<std::thread::JoinHandle<()>>>,
    shutdown_started: AtomicBool,
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
    ) -> Arc<Self> {
        Arc::new(Self {
            identity,
            key,
            root,
            state_placement,
            runtime,
            state: std::sync::Mutex::new(EmbeddedRuntimeState {
                phase: super::public_api::SourceRuntimePhase::Loading,
                current_publication_identity: None,
                observer_epoch: 0,
                source_version: 0,
            }),
            control: std::sync::Mutex::new(WorkerControl::default()),
            wake: Condvar::new(),
            worker: std::sync::Mutex::new(None),
            shutdown_started: AtomicBool::new(false),
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

        let mut observed_fingerprint = self.reload_and_publish();
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
            } else {
                observed_fingerprint = next_fingerprint;
            }
        }
        self.set_phase(super::public_api::SourceRuntimePhase::Stopped);
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

    fn request_refresh(&self) -> Option<u64> {
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
        let joined = self
            .worker
            .lock()
            .expect("embedded worker mutex")
            .take()
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

        let mut rollback = OpenRollback::reserve(Arc::clone(self), key.clone(), identity);
        let admission = super::activation::admit_project_with_outcome(
            super::process_runtime::SurfaceKind::Embed,
            &binding.canonical_root,
            &binding.root_id.0,
            binding.access_mode,
            &state_placement,
        )
        .map_err(|_| EmbeddedOpenError::AdmissionUnavailable)?;
        let index = crate::live_index::store::LiveIndex::empty();
        let runtime =
            super::activation::ProjectRuntimeHandle::bind_admitted(index, admission.into_slot());
        let source = EmbeddedBinding::new(
            identity,
            key.clone(),
            binding.canonical_root,
            state_placement,
            runtime,
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
        let path_prefix = normalized_path_prefix(request.path_prefix.as_deref());
        let mut matches = Vec::new();
        for (path, file) in live.all_files() {
            if path_prefix
                .as_deref()
                .is_some_and(|prefix| !path.starts_with(prefix))
            {
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
        let path_prefix = normalized_path_prefix(request.path_prefix.as_deref());
        let limit = request.limit as usize;
        let mut matches = Vec::new();
        let mut truncated = false;
        'files: for (path, file) in live.all_files() {
            if path_prefix
                .as_deref()
                .is_some_and(|prefix| !path.starts_with(prefix))
            {
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
        let Some(source_version) = binding.request_refresh() else {
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
