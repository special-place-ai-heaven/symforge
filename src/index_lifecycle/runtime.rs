//! Feature 020 V11, T047 — the dark source runtime.
//!
//! The closed five-state machine from `data-model.md:1413-1449`, the strict
//! acquisition rule REFREEZE-pinned as F020-V11-A20 (`:1539-1554`), FR-043's
//! no-restore terminal paths, and the single publication root with sealed
//! per-source rebasing (`:1528-1537`) — all reachable ONLY through
//! [`DarkRuntimeFactory`]. Nothing in production constructs any of this;
//! Slice 4's activation cut is the only planned caller.
//!
//! **Payload simplifications are recorded, not hidden.** The frozen machine
//! carries observer phases, mutation epochs, revocation packages, and
//! `NonCurrentWork`; this dark slice implements the fields its ten oracles
//! exercise and the evidence document's D-ledger carries the rest as Slice 4
//! obligations. The five state NAMES are the frozen ones, the public phase has
//! the contract's six variants, and `Stopped` is derived from the registry
//! tombstone — never a sixth machine state.
//!
//! **Stand-in constructors.** Every `*_for_test` admission mints its evidence
//! unconditionally, the recorded fixture family: sealed shapes, `Ok` until the
//! real observer and candidate machinery of Slice 4 supplies refusing
//! evidence. Do not "complete" them with checks they cannot observe.
//!
//! **This module must not import from `protocol`.** Its refusal vocabulary
//! comes from `crate::lifecycle_identity`, the shared ungated home — the embed
//! gate on this module's first commit is what proved the relocation need.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use arc_swap::ArcSwap;

use crate::lifecycle_identity::{
    AuthorityIdentity, GenerationAuthority, GenerationIdentity, ObserverToken, OperationKind,
    OperationReceipt, PublicationIdentity, RetryAdvice, SourceRefusal, SourceRefusalKind,
};

use super::registry::ProjectKey;

// ── The dark factory ───────────────────────────────────────────────────────

/// The ONLY door into the V11 runtime during Slice 3. Holds the physical root
/// every admitted source binds beneath.
#[derive(Debug)]
pub struct DarkRuntimeFactory {
    root: String,
}

impl DarkRuntimeFactory {
    /// Fixture constructor: the real root lease arrives with Slice 4.
    pub fn for_test_root(root: &str) -> Self {
        Self {
            root: root.to_string(),
        }
    }

    /// Mint one project runtime beneath this factory's root.
    pub fn project_runtime(&self, key: ProjectKey) -> ProjectIndexRuntime {
        ProjectIndexRuntime {
            root: self.root.clone(),
            _key: key,
            next_source: AtomicU64::new(1),
            sources: Mutex::new(HashMap::new()),
            publication: ProjectPublicationRoot {
                inner: ArcSwap::new(Arc::new(ProjectRuntimePublication {
                    publication_identity: PublicationIdentity::fresh(),
                    sources: HashMap::new(),
                })),
            },
        }
    }
}

// ── Public phase and internal machine ──────────────────────────────────────

/// The PUBLIC phase vocabulary — the contract's six variants. The internal
/// machine has five states; `Stopped` exists only here, derived from the
/// registry tombstone after `Stopping` completes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceRuntimePhase {
    Loading,
    Current,
    Refreshing,
    Blocked,
    Stopping,
    Stopped,
}

/// The closed machine, frozen at exactly these five states
/// (`data-model.md:1413-1449`). Private: the names are not public
/// constructors, exactly as the corpus stamps them.
#[derive(Debug)]
enum SourceRuntimeState {
    Loading,
    Current {
        generation: Arc<VerifiedGeneration>,
    },
    Refreshing {
        retained: Arc<VerifiedGeneration>,
        /// F020-V11-R20A: the retention is a queryable lane ONLY while this
        /// refresh has issued no mutation permit.
        permit_issued: bool,
    },
    Blocked {
        /// Recovery evidence, never a lane (F020-V11-R20B).
        _retained: Option<Arc<VerifiedGeneration>>,
    },
    Stopping {
        /// Accounting, never a lane (F020-V11-R20B).
        _retained: Option<Arc<VerifiedGeneration>>,
    },
}

#[derive(Debug)]
struct SourceEntry {
    state: SourceRuntimeState,
    /// The registry tombstone. `Stopped` is THIS, not a machine state.
    tombstoned: bool,
}

/// Opaque handle naming one admitted source within its runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DarkSourceHandle(u64);

// ── VerifiedGeneration and the strict lease ────────────────────────────────

/// A COMPLETE verified generation. The frozen skeleton carries manifests,
/// certificates, ledgers, and charged roots (`data-model.md:1382-1400`); the
/// dark slice retains the exact authority Arc and the completeness verdict its
/// oracles pin, with the remaining fields recorded as Slice 4 obligations.
#[derive(Debug)]
pub struct VerifiedGeneration {
    generation_authority: Arc<GenerationAuthority>,
    complete: bool,
}

impl VerifiedGeneration {
    /// The generation's own identity is its AUTHORITY's identity — one exact
    /// Arc, retained, never re-minted (`data-model.md:1523-1526`).
    pub fn identity(&self) -> AuthorityIdentity {
        self.generation_authority.identity()
    }

    pub fn scope_certificate_is_complete(&self) -> bool {
        self.complete
    }

    pub fn generation_authority(&self) -> Arc<GenerationAuthority> {
        Arc::clone(&self.generation_authority)
    }
}

/// A strict acquisition: proof that a COMPLETE verified generation was
/// leasable at acquisition time.
#[derive(Debug)]
pub struct StrictLease {
    generation: Arc<VerifiedGeneration>,
}

impl StrictLease {
    pub fn generation(&self) -> Arc<VerifiedGeneration> {
        Arc::clone(&self.generation)
    }
}

// ── The publication root ───────────────────────────────────────────────────

/// The SOLE publication root for a project's runtime state
/// (`data-model.md:1528-1537`): one `ArcSwap` whose every swap carries a
/// never-reused publication identity.
#[derive(Debug)]
pub struct ProjectPublicationRoot {
    inner: ArcSwap<ProjectRuntimePublication>,
}

impl ProjectPublicationRoot {
    pub fn load(&self) -> Arc<ProjectRuntimePublication> {
        self.inner.load_full()
    }
}

/// One published snapshot of every source's public state.
#[derive(Debug)]
pub struct ProjectRuntimePublication {
    publication_identity: PublicationIdentity,
    sources: HashMap<u64, Arc<SourceStatePublication>>,
}

impl ProjectRuntimePublication {
    pub fn publication_identity(&self) -> PublicationIdentity {
        self.publication_identity
    }

    pub fn source_publication(
        &self,
        source: &DarkSourceHandle,
    ) -> Option<Arc<SourceStatePublication>> {
        self.sources.get(&source.0).cloned()
    }
}

/// The published per-source record. A sealed transition replaces exactly ONE
/// of these; every sibling's Arc survives identical.
#[derive(Debug)]
pub struct SourceStatePublication {
    phase: SourceRuntimePhase,
}

impl SourceStatePublication {
    pub fn phase(&self) -> SourceRuntimePhase {
        self.phase
    }
}

/// A validated capture of one source's published view
/// (`data-model.md:1556-1564`): loaded, revalidated against the root, and
/// carrying an observer token only when one is actually registered.
#[derive(Debug)]
pub struct SourceView {
    publication_identity: PublicationIdentity,
    observer_token: Option<ObserverToken>,
}

impl SourceView {
    pub fn publication_identity(&self) -> PublicationIdentity {
        self.publication_identity
    }

    pub fn observer_token(&self) -> Option<ObserverToken> {
        self.observer_token
    }
}

// ── Permits and shutdown receipts ──────────────────────────────────────────

/// The dark mutation permit. FR-043 is a property of the MACHINE, not of this
/// value: once granted, no terminal path of the permit — commit, rollback with
/// a no-side-effect proof, or drop — restores the prior `Current`; return
/// happens only through fresh candidate publication, which is Slice 4.
#[derive(Debug)]
pub struct DarkMutationPermit {
    _sealed: (),
}

impl DarkMutationPermit {
    /// A proven no-op rollback. Consumes the permit; the source stays
    /// non-current, because FR-043 forbids restoring the prior publication
    /// even when nothing was written.
    pub fn rollback_with_no_side_effect_proof_for_test(self) {}
}

/// Receipt for a dark shutdown. Nothing was spawned, so the wait completes
/// immediately and the report can only honestly count zero joins.
#[derive(Debug)]
pub struct DarkShutdownReceipt {
    _sealed: (),
}

impl DarkShutdownReceipt {
    pub fn wait_for_test(self) -> Result<DarkShutdownReport, SourceRefusal> {
        Ok(DarkShutdownReport { joined_workers: 0 })
    }
}

/// What the shutdown OBSERVED. `joined_workers` is zero because the dark
/// modules spawn no threads, timers, or tasks — any other number would report
/// a completion nothing witnessed.
#[derive(Debug)]
pub struct DarkShutdownReport {
    joined_workers: u64,
}

impl DarkShutdownReport {
    pub fn joined_workers(&self) -> u64 {
        self.joined_workers
    }
}

// ── The project runtime ────────────────────────────────────────────────────

/// The per-project V11 runtime: owns the source machines and the one
/// publication root. SEAM-pinned name.
#[derive(Debug)]
pub struct ProjectIndexRuntime {
    root: String,
    _key: ProjectKey,
    next_source: AtomicU64,
    sources: Mutex<HashMap<u64, SourceEntry>>,
    publication: ProjectPublicationRoot,
}

impl ProjectIndexRuntime {
    pub fn publication_root(&self) -> &ProjectPublicationRoot {
        &self.publication
    }

    /// Admit a source that is still loading: it retains NOTHING and is
    /// therefore not queryable.
    pub fn admit_loading_source_for_test(&self, _name: &str) -> DarkSourceHandle {
        self.insert_source(SourceRuntimeState::Loading)
    }

    /// Admit a source straight at `Current` with a complete verified
    /// generation — the fixture family's unconditional evidence.
    pub fn admit_current_source_for_test(&self, _name: &str) -> DarkSourceHandle {
        let generation = Arc::new(VerifiedGeneration {
            generation_authority: Arc::new(GenerationAuthority::captured(
                AuthorityIdentity::fresh(),
                GenerationIdentity::fresh(),
                self.root.clone(),
            )),
            complete: true,
        });
        self.insert_source(SourceRuntimeState::Current { generation })
    }

    /// Enter a reload-shaped refresh: the current generation becomes the
    /// retention, untouched, and stays a queryable lane until a permit exists.
    pub fn begin_reload_refresh_for_test(&self, source: &DarkSourceHandle) {
        self.transition(source, |state| match state {
            SourceRuntimeState::Current { generation } => SourceRuntimeState::Refreshing {
                retained: generation,
                permit_issued: false,
            },
            other => other,
        });
    }

    /// Grant the refresh its mutation permit. From this instant the retention
    /// stops being a lane (R20A), BEFORE any side effect can run.
    pub fn grant_mutation_permit_for_test(
        &self,
        source: &DarkSourceHandle,
    ) -> Result<DarkMutationPermit, SourceRefusal> {
        let mut granted = false;
        self.transition(source, |state| match state {
            SourceRuntimeState::Refreshing { retained, .. } => {
                granted = true;
                SourceRuntimeState::Refreshing {
                    retained,
                    permit_issued: true,
                }
            }
            other => other,
        });
        if granted {
            Ok(DarkMutationPermit { _sealed: () })
        } else {
            Err(self.refusal(None))
        }
    }

    pub fn block_source_for_test(&self, source: &DarkSourceHandle) {
        self.transition(source, |state| {
            let retained = retained_of(state);
            SourceRuntimeState::Blocked {
                _retained: retained,
            }
        });
    }

    pub fn stop_source_for_test(&self, source: &DarkSourceHandle) {
        self.transition(source, |state| {
            let retained = retained_of(state);
            SourceRuntimeState::Stopping {
                _retained: retained,
            }
        });
    }

    /// Complete the stop: the REGISTRY tombstones the source. The machine
    /// itself never holds a `Stopped` state — the public phase derives it.
    pub fn complete_stop_for_test(&self, source: &DarkSourceHandle) {
        let mut sources = self.sources.lock().expect("runtime lock");
        if let Some(entry) = sources.get_mut(&source.0) {
            entry.tombstoned = true;
        }
        drop(sources);
        self.republish(Some(source.0));
    }

    /// The strict acquisition, closed on COMPLETENESS (F020-V11-A20):
    /// `Current` leases; a permit-free `Refreshing` leases its retention;
    /// everything else refuses.
    pub fn acquire_strict(&self, source: &DarkSourceHandle) -> Result<StrictLease, SourceRefusal> {
        let sources = self.sources.lock().expect("runtime lock");
        let Some(entry) = sources.get(&source.0) else {
            return Err(self.refusal(None));
        };
        if entry.tombstoned {
            return Err(self.refusal(None));
        }
        match &entry.state {
            SourceRuntimeState::Current { generation } => Ok(StrictLease {
                generation: Arc::clone(generation),
            }),
            SourceRuntimeState::Refreshing {
                retained,
                permit_issued: false,
            } => Ok(StrictLease {
                generation: Arc::clone(retained),
            }),
            SourceRuntimeState::Refreshing {
                retained,
                permit_issued: true,
            } => Err(self.refusal(Some(retained.identity()))),
            SourceRuntimeState::Loading
            | SourceRuntimeState::Blocked { .. }
            | SourceRuntimeState::Stopping { .. } => Err(self.refusal(None)),
        }
    }

    /// The public phase: the tombstone wins, then the machine state maps.
    pub fn source_phase(&self, source: &DarkSourceHandle) -> SourceRuntimePhase {
        let sources = self.sources.lock().expect("runtime lock");
        let Some(entry) = sources.get(&source.0) else {
            return SourceRuntimePhase::Stopped;
        };
        if entry.tombstoned {
            return SourceRuntimePhase::Stopped;
        }
        match &entry.state {
            SourceRuntimeState::Loading => SourceRuntimePhase::Loading,
            SourceRuntimeState::Current { .. } => SourceRuntimePhase::Current,
            SourceRuntimeState::Refreshing { .. } => SourceRuntimePhase::Refreshing,
            SourceRuntimeState::Blocked { .. } => SourceRuntimePhase::Blocked,
            SourceRuntimeState::Stopping { .. } => SourceRuntimePhase::Stopping,
        }
    }

    /// Load, revalidate against the root, and return a view that carries an
    /// observer token only when one is registered — the capture never invents
    /// (`data-model.md:1556-1564`). The dark runtime registers no observers,
    /// and single-threaded revalidation cannot drift, so one validated pass
    /// suffices; the retry loop arrives with the real observer machinery.
    pub fn capture_source_view(
        &self,
        source: &DarkSourceHandle,
    ) -> Result<SourceView, SourceRefusal> {
        let first = self.publication.load();
        if first.source_publication(source).is_none() {
            return Err(self.refusal(None));
        }
        let revalidated = self.publication.load();
        if revalidated.publication_identity() != first.publication_identity() {
            // Drift between load and revalidate: refuse rather than compose
            // two publications; the retrying capture is Slice 4 work.
            return Err(self.refusal(None));
        }
        Ok(SourceView {
            publication_identity: revalidated.publication_identity(),
            observer_token: None,
        })
    }

    /// Begin the dark shutdown. Nothing was spawned, so there is nothing to
    /// signal; the receipt's wait reports what was observed: zero joins.
    pub fn begin_shutdown_for_test(&self) -> DarkShutdownReceipt {
        DarkShutdownReceipt { _sealed: () }
    }

    // ── internals ──────────────────────────────────────────────────────────

    fn insert_source(&self, state: SourceRuntimeState) -> DarkSourceHandle {
        let id = self.next_source.fetch_add(1, Ordering::Relaxed);
        self.sources.lock().expect("runtime lock").insert(
            id,
            SourceEntry {
                state,
                tombstoned: false,
            },
        );
        self.republish(Some(id));
        DarkSourceHandle(id)
    }

    fn transition(
        &self,
        source: &DarkSourceHandle,
        apply: impl FnOnce(SourceRuntimeState) -> SourceRuntimeState,
    ) {
        let mut sources = self.sources.lock().expect("runtime lock");
        if let Some(entry) = sources.remove(&source.0) {
            let next = apply(entry.state);
            sources.insert(
                source.0,
                SourceEntry {
                    state: next,
                    tombstoned: entry.tombstoned,
                },
            );
        }
        drop(sources);
        self.republish(Some(source.0));
    }

    /// Publish a successor: a NEW never-reused publication identity, the
    /// affected source's record replaced, and every sibling's Arc carried
    /// over IDENTICAL — the sealed per-source rebase of `data-model.md:1528`.
    fn republish(&self, changed: Option<u64>) {
        let sources = self.sources.lock().expect("runtime lock");
        let previous = self.publication.load();
        let mut published: HashMap<u64, Arc<SourceStatePublication>> = HashMap::new();
        for (id, entry) in sources.iter() {
            let phase = if entry.tombstoned {
                SourceRuntimePhase::Stopped
            } else {
                match &entry.state {
                    SourceRuntimeState::Loading => SourceRuntimePhase::Loading,
                    SourceRuntimeState::Current { .. } => SourceRuntimePhase::Current,
                    SourceRuntimeState::Refreshing { .. } => SourceRuntimePhase::Refreshing,
                    SourceRuntimeState::Blocked { .. } => SourceRuntimePhase::Blocked,
                    SourceRuntimeState::Stopping { .. } => SourceRuntimePhase::Stopping,
                }
            };
            let record = match (changed, previous.sources.get(id)) {
                (Some(changed_id), Some(existing)) if *id != changed_id => Arc::clone(existing),
                (_, Some(existing)) if existing.phase == phase => Arc::clone(existing),
                _ => Arc::new(SourceStatePublication { phase }),
            };
            published.insert(*id, record);
        }
        self.publication
            .inner
            .store(Arc::new(ProjectRuntimePublication {
                publication_identity: PublicationIdentity::fresh(),
                sources: published,
            }));
    }

    /// A strict-acquisition refusal, naming the evidence that was examined
    /// when there was any: a permit-holding refresh names its retention; the
    /// evidence-free states name nothing rather than minting.
    fn refusal(&self, evidence: Option<AuthorityIdentity>) -> SourceRefusal {
        SourceRefusal::for_runtime(
            SourceRefusalKind::SourceUnavailable,
            OperationReceipt::for_test(OperationKind::AcquireRuntime),
            RetryAdvice::OnEvent,
            evidence,
        )
    }
}

fn retained_of(state: SourceRuntimeState) -> Option<Arc<VerifiedGeneration>> {
    match state {
        SourceRuntimeState::Current { generation } => Some(generation),
        SourceRuntimeState::Refreshing { retained, .. } => Some(retained),
        SourceRuntimeState::Blocked { _retained } => _retained,
        SourceRuntimeState::Stopping { _retained } => _retained,
        SourceRuntimeState::Loading => None,
    }
}
