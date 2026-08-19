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
    AuthorityIdentity, GenerationAuthority, ObserverToken, OperationKind, OperationReceipt,
    PublicationIdentity, RetryAdvice, SourceRefusal, SourceRefusalKind,
};
// Consumed only by the C3-gated probes; the plain embed build sheds it with
// them (the embed-gate unused-import class CLAUDE.md documents).
#[cfg(all(test, feature = "server"))]
use crate::lifecycle_identity::GenerationIdentity;

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
    ///
    /// Every `*_for_test` probe carries `all(test, feature = "server")`.
    /// The recorded ACTIVATION PRECONDITION is discharged: the T047 oracle
    /// suite moved in-crate (`dark_runtime_oracles`, end of this file) at
    /// the start of the cut, so the probes compile only under the lib's own
    /// `test` cfg and no longer ship in the published server binary. The
    /// embed build sheds them through the `server` conjunct, exactly as the
    /// Slice 0 predicate rule reads `all(..)`.
    #[cfg(all(test, feature = "server"))]
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
// Until the T066 activation wiring gives these production consumers, the only
// consumers are the in-crate `dark_runtime_oracles`; the non-test lib build
// honestly sees them as dead. The allows are scoped to not(test) so a
// post-activation regression to genuine deadness still lints in test builds.
#[derive(Debug)]
#[cfg_attr(not(test), allow(dead_code))]
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
    #[cfg(all(test, feature = "server"))]
    pub fn rollback_with_no_side_effect_proof_for_test(self) {}
}

/// Receipt for a dark shutdown. Nothing was spawned, so the wait completes
/// immediately and the report can only honestly count zero joins.
#[derive(Debug)]
pub struct DarkShutdownReceipt {
    _sealed: (),
}

impl DarkShutdownReceipt {
    #[cfg(all(test, feature = "server"))]
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
    #[cfg_attr(not(test), allow(dead_code))]
    root: String,
    _key: ProjectKey,
    #[cfg_attr(not(test), allow(dead_code))]
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
    #[cfg(all(test, feature = "server"))]
    pub fn admit_loading_source_for_test(&self, _name: &str) -> DarkSourceHandle {
        self.insert_source(SourceRuntimeState::Loading)
    }

    /// Admit a source straight at `Current` with a complete verified
    /// generation — the fixture family's unconditional evidence.
    #[cfg(all(test, feature = "server"))]
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
    #[cfg(all(test, feature = "server"))]
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
    #[cfg(all(test, feature = "server"))]
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
            Err(self.refusal(OperationKind::RefreshSource, None))
        }
    }

    #[cfg(all(test, feature = "server"))]
    pub fn block_source_for_test(&self, source: &DarkSourceHandle) {
        self.transition(source, |state| {
            let retained = retained_of(state);
            SourceRuntimeState::Blocked {
                _retained: retained,
            }
        });
    }

    #[cfg(all(test, feature = "server"))]
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
    #[cfg(all(test, feature = "server"))]
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
            return Err(self.refusal(OperationKind::AcquireRuntime, None));
        };
        if entry.tombstoned {
            return Err(self.refusal(OperationKind::AcquireRuntime, None));
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
            } => Err(self.refusal(OperationKind::AcquireRuntime, Some(retained.identity()))),
            SourceRuntimeState::Loading
            | SourceRuntimeState::Blocked { .. }
            | SourceRuntimeState::Stopping { .. } => {
                Err(self.refusal(OperationKind::AcquireRuntime, None))
            }
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
            return Err(self.refusal(OperationKind::AcquireRuntime, None));
        }
        let revalidated = self.publication.load();
        if revalidated.publication_identity() != first.publication_identity() {
            // Drift between load and revalidate: refuse rather than compose
            // two publications; the retrying capture is Slice 4 work.
            return Err(self.refusal(OperationKind::AcquireRuntime, None));
        }
        Ok(SourceView {
            publication_identity: revalidated.publication_identity(),
            observer_token: None,
        })
    }

    /// Begin the dark shutdown. Nothing was spawned, so there is nothing to
    /// signal; the receipt's wait reports what was observed: zero joins.
    #[cfg(all(test, feature = "server"))]
    pub fn begin_shutdown_for_test(&self) -> DarkShutdownReceipt {
        DarkShutdownReceipt { _sealed: () }
    }

    // ── internals ──────────────────────────────────────────────────────────

    #[cfg_attr(not(test), allow(dead_code))]
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

    #[cfg_attr(not(test), allow(dead_code))]
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
    #[cfg_attr(not(test), allow(dead_code))]
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
    /// Mint the runtime's honest dark refusal. The receipt is the C5-ruled
    /// dark constructor — argument identity is NOT claimed on these lanes —
    /// and the kind names the operation that actually refused, per call site.
    fn refusal(&self, kind: OperationKind, evidence: Option<AuthorityIdentity>) -> SourceRefusal {
        SourceRefusal::for_runtime(
            SourceRefusalKind::SourceUnavailable,
            OperationReceipt::for_dark_refusal(kind),
            RetryAdvice::OnEvent,
            evidence,
        )
    }
}

#[cfg_attr(not(test), allow(dead_code))]
fn retained_of(state: SourceRuntimeState) -> Option<Arc<VerifiedGeneration>> {
    match state {
        SourceRuntimeState::Current { generation } => Some(generation),
        SourceRuntimeState::Refreshing { retained, .. } => Some(retained),
        SourceRuntimeState::Blocked { _retained } => _retained,
        SourceRuntimeState::Stopping { _retained } => _retained,
        SourceRuntimeState::Loading => None,
    }
}

/// Feature 020 V11, T047 — the dark source runtime oracles.
///
/// Moved in-crate verbatim from `tests/runtime_dark_v11.rs` at the start of
/// the Slice 4 activation cut, discharging the recorded precondition on the
/// `*_for_test` doors (see `DarkRuntimeFactory::for_test_root`): with the
/// oracles inside the crate, every fixture door tightens to
/// `all(test, feature = "server")` and stops shipping in the release binary.
/// Only import paths changed in the move; the ten T047 oracles and the one
/// T049 boundary-wait oracle keep their names and bodies.
#[cfg(all(test, feature = "server"))]
mod dark_runtime_oracles {
    use std::sync::Arc;

    use super::super::embedded::{EmbeddedSourceFactory, ReceiptWaitError};
    use super::super::public_api::{
        EmbeddedSourceSpec, ProcessRuntimeApi, SourceRuntimePhase as PublicSourceRuntimePhase,
    };
    use super::super::registry::ProjectKey;
    use super::{DarkRuntimeFactory, ProjectIndexRuntime, SourceRuntimePhase};
    use crate::protocol::format::claim_provenance::SourceRefusalKind;

    // ── Fixtures ───────────────────────────────────────────────────────────
    // Local to this module, per the Slice 2 oracle convention. The dark
    // factory is the ONLY entry: nothing here reaches a production
    // constructor, and the factory's evidence stand-ins are the recorded
    // fixture family — sealed shapes, unconditional admission, Slice 4
    // supplies the refusing evidence.

    fn a_project_key(name: &str) -> ProjectKey {
        ProjectKey::new(name)
    }

    fn a_dark_runtime(root: &str) -> ProjectIndexRuntime {
        DarkRuntimeFactory::for_test_root(root).project_runtime(a_project_key("project-a"))
    }

    // ── A20/R20B: strict acquisition is closed on COMPLETENESS ─────────────

    #[test]
    fn loading_blocked_stopping_refuse_strict_acquisition() {
        // data-model.md:1539-1549, byte-frozen: "Only a COMPLETE verified
        // generation may be queried. `Loading` retains none and is therefore
        // not queryable. ... `Blocked` and `Stopping` retain zero or one for
        // recovery and accounting, and those are NOT queryable."
        let runtime = a_dark_runtime("root-a");
        let source = runtime.admit_loading_source_for_test("src-a");

        let refusal = runtime
            .acquire_strict(&source)
            .expect_err("Loading retains nothing and must refuse strict acquisition");
        assert_eq!(refusal.kind(), SourceRefusalKind::SourceUnavailable);

        runtime.block_source_for_test(&source);
        let refusal = runtime
            .acquire_strict(&source)
            .expect_err("Blocked retentions are recovery evidence, never a lane");
        assert_eq!(refusal.kind(), SourceRefusalKind::SourceUnavailable);

        runtime.stop_source_for_test(&source);
        let refusal = runtime
            .acquire_strict(&source)
            .expect_err("Stopping retentions are accounting, never a lane");
        assert_eq!(refusal.kind(), SourceRefusalKind::SourceUnavailable);

        // GREEN-CONTROL: a source promoted to Current IS strictly acquirable,
        // so the three refusals above are about the states, not a lane that
        // always refuses.
        let current = runtime.admit_current_source_for_test("src-b");
        let lease = runtime
            .acquire_strict(&current)
            .expect("a COMPLETE verified generation is the one acquirable thing");
        assert!(lease.generation().scope_certificate_is_complete());
    }

    // ── R20A: Refreshing serves its retention until a permit is granted ────

    #[test]
    fn refreshing_serves_retained_only_until_a_permit_is_granted() {
        // data-model.md:1541-1545: "`Refreshing` retains exactly one, and it
        // REMAINS QUERYABLE only while that refresh has issued NO mutation
        // permit — a reload building a successor elsewhere leaves the
        // retained bytes untouched (F020-V11-R20A)."
        let runtime = a_dark_runtime("root-a");
        let source = runtime.admit_current_source_for_test("src-a");
        let retained = runtime
            .acquire_strict(&source)
            .expect("current is acquirable")
            .generation()
            .identity();

        runtime.begin_reload_refresh_for_test(&source);
        let lease = runtime
            .acquire_strict(&source)
            .expect("a reload-entered Refreshing keeps serving its retention");
        assert_eq!(
            lease.generation().identity(),
            retained,
            "the retention is the SAME complete generation, not a rebuild"
        );

        let _permit = runtime
            .grant_mutation_permit_for_test(&source)
            .expect("the refresh may issue its permit");
        let refusal = runtime
            .acquire_strict(&source)
            .expect_err("the moment a permit exists, the retention stops being a lane");
        assert_eq!(refusal.kind(), SourceRefusalKind::SourceUnavailable);
    }

    // ── The grant is ITSELF a publication ──────────────────────────────────

    #[test]
    fn permit_grant_is_itself_a_publication() {
        // contracts/source-binding-and-state.md:275-278 says granting the
        // permit atomically PUBLISHES non-current Refreshing before any side
        // effect can run. The dark runtime has no side-effect lane yet, so
        // the before-side-effects half is unobservable until Slice 4 — an
        // earlier draft of this test asserted post-grant `Refreshing`, which
        // was already true BEFORE the grant and therefore observed nothing
        // (the C11 review finding). What IS observable and falsifiable now:
        // the grant goes through the publication root — a fresh never-reused
        // publication identity — and the retention's refusal is thereby a
        // PUBLISHED fact, not side-band state.
        let runtime = a_dark_runtime("root-a");
        let source = runtime.admit_current_source_for_test("src-a");
        runtime.begin_reload_refresh_for_test(&source);

        let before = runtime.publication_root().load().publication_identity();
        let permit = runtime
            .grant_mutation_permit_for_test(&source)
            .expect("grant");
        let after = runtime.publication_root().load().publication_identity();
        assert_ne!(
            before, after,
            "the grant must PUBLISH — a grant that only flips side-band state \
             leaves the publication identity unchanged and fails here"
        );
        assert!(
            runtime.acquire_strict(&source).is_err(),
            "after the published grant the retention stops being a lane"
        );
        let _ends_without_committing = permit;
    }

    // ── FR-043: no terminal permit path restores the prior Current ─────────

    #[test]
    fn no_terminal_permit_path_restores_prior_current() {
        // data-model.md:1502-1509 + source-binding-and-state.md:310-314:
        // "Every terminal path that can return the same live binding to
        // `Current`, including a valid `NoSideEffectProof`, does so only
        // through fresh candidate publication" — commit, rollback, and drop
        // all leave the source non-current until a complete successor
        // installs.
        let runtime = a_dark_runtime("root-a");

        // Arm 1: the permit is DROPPED without committing.
        let source = runtime.admit_current_source_for_test("src-a");
        runtime.begin_reload_refresh_for_test(&source);
        let permit = runtime
            .grant_mutation_permit_for_test(&source)
            .expect("grant");
        // Terminal path one: the permit simply ENDS, uncommitted.
        let _ = permit;
        assert!(
            runtime.acquire_strict(&source).is_err(),
            "a dropped permit must not restore the prior publication"
        );

        // Arm 2: the permit rolls back with a no-side-effect proof.
        let source_b = runtime.admit_current_source_for_test("src-b");
        runtime.begin_reload_refresh_for_test(&source_b);
        let permit = runtime
            .grant_mutation_permit_for_test(&source_b)
            .expect("grant");
        permit.rollback_with_no_side_effect_proof_for_test();
        assert!(
            runtime.acquire_strict(&source_b).is_err(),
            "even a proven no-op rollback returns to Current only through a \
             fresh candidate publication, never by restoring the prior one"
        );
    }

    // ── One publication root; a sealed transition rebases one source ───────

    #[test]
    fn sealed_transition_rebases_one_source_and_preserves_siblings() {
        // data-model.md:1528-1537: the registry-owned ArcSwap of the project
        // runtime publication is the SOLE publication root; a sealed
        // transition exact-matches its retained token, rebases the one
        // source, and every sibling's Arc survives identical.
        let runtime = a_dark_runtime("root-a");
        let source_a = runtime.admit_current_source_for_test("src-a");
        let source_b = runtime.admit_current_source_for_test("src-b");

        let before = runtime.publication_root().load();
        let sibling_before = before.source_publication(&source_b).expect("sibling");

        runtime.begin_reload_refresh_for_test(&source_a);

        let after = runtime.publication_root().load();
        assert_ne!(
            before.publication_identity(),
            after.publication_identity(),
            "a transition publishes a NEW never-reused publication identity"
        );
        let sibling_after = after.source_publication(&source_b).expect("sibling");
        assert!(
            Arc::ptr_eq(&sibling_before, &sibling_after),
            "the untouched sibling's publication is the SAME Arc, not a rebuild"
        );
    }

    // ── capture_source_view: atomic, validated, no invented token ──────────

    #[test]
    fn capture_source_view_is_atomic_and_invents_no_token() {
        // data-model.md:1556-1564: the capture loads the source publication,
        // acquires its token accumulator WHEN PRESENT, reloads the root,
        // exact-validates both, and retries on drift. A source that has no
        // observer token yields a view WITHOUT one — the capture never
        // invents.
        let runtime = a_dark_runtime("root-a");
        let source = runtime.admit_current_source_for_test("src-a");

        let view = runtime
            .capture_source_view(&source)
            .expect("a stable root yields a coherent capture");
        assert_eq!(
            view.publication_identity(),
            runtime.publication_root().load().publication_identity(),
            "the view is validated against the root it was captured under"
        );
        assert!(
            view.observer_token().is_none(),
            "no observer is registered in the dark runtime, so the capture \
             must not invent a token"
        );
    }

    // ── Stopped is derived from the tombstone, not a sixth state ───────────

    #[test]
    fn stopped_phase_derives_from_tombstone_not_a_machine_state() {
        // The internal machine is closed at exactly five states
        // (data-model.md:1413-1449); the PUBLIC SourceRuntimePhase has six
        // variants (public-api-v11.json:2372-2387) — the sixth, Stopped, is
        // produced only by registry tombstoning after Stopping completes.
        let runtime = a_dark_runtime("root-a");
        let source = runtime.admit_current_source_for_test("src-a");

        runtime.stop_source_for_test(&source);
        assert_eq!(runtime.source_phase(&source), SourceRuntimePhase::Stopping);

        runtime.complete_stop_for_test(&source);
        assert_eq!(
            runtime.source_phase(&source),
            SourceRuntimePhase::Stopped,
            "Stopped is the tombstone's phase; the machine itself never holds it"
        );
    }

    // ── VerifiedGeneration is total or absent ──────────────────────────────

    #[test]
    fn verified_generation_requires_exact_authority_and_complete_artifacts() {
        // data-model.md:1511-1526: the private constructor requires the
        // canonical manifest, every required artifact, the complete observer
        // cut, and the charged allocation — and the generation retains its
        // EXACT authority Arc.
        let runtime = a_dark_runtime("root-a");
        let source = runtime.admit_current_source_for_test("src-a");
        let lease = runtime.acquire_strict(&source).expect("current");

        let generation = lease.generation();
        assert!(generation.scope_certificate_is_complete());
        let authority_a = generation.generation_authority();
        let authority_b = generation.generation_authority();
        assert!(
            Arc::ptr_eq(&authority_a, &authority_b),
            "the generation retains ONE exact authority Arc, never a re-mint"
        );
    }

    // ── The V11 handle: infallible begin_close, self-wait refused at wait ──

    #[test]
    fn begin_close_is_infallible_and_self_wait_fails_at_wait() {
        // public-api-v11.json:1284-1296: begin_close(&self) ->
        // SourceCloseReceipt is infallible; the guard against waiting on
        // yourself moved to the WAIT, which returns ReceiptWaitError rather
        // than deadlocking.
        let factory = EmbeddedSourceFactory::new();
        let handle = factory
            .open(a_project_key("src-a"))
            .expect("an open registry admits a fresh key");

        let receipt = handle.begin_close();
        let report = receipt
            .wait_for_test()
            .expect("an ordinary wait on the close receipt completes");
        assert!(report.finalized());

        let handle_b = factory
            .open(a_project_key("src-b"))
            .expect("an open registry admits a second key");
        let error = handle_b
            .self_wait_probe_for_test()
            .expect_err("waiting on your own close from inside the finalizer refuses");
        let _ = error;
    }

    // ── ShutdownReport reports only what was observed ──────────────────────

    #[test]
    fn shutdown_report_reflects_observed_counts_only() {
        // public-api-v11.json:2280-2307 gives ShutdownReport a joined_workers
        // count; process_runtime.rs and embedded.rs are FORBIDDEN from
        // spawning threads, timers, or tasks, and the reporting invariant
        // forbids claiming a join nothing observed. The dark runtime
        // therefore reports EXACTLY zero — a plausible non-zero here would be
        // fabricated completion.
        let runtime = a_dark_runtime("root-a");

        let receipt = runtime.begin_shutdown_for_test();
        let report = receipt
            .wait_for_test()
            .expect("shutdown of a runtime that spawned nothing completes");
        assert_eq!(
            report.joined_workers(),
            0,
            "nothing was spawned, so nothing was joined; any other number is \
             a report of an unobserved completion"
        );
    }

    // ── T049: the boundary's contract waits and the open refusal mapping ───

    #[test]
    fn contract_waits_guard_self_wait_and_open_refuses_a_held_source() {
        // The T049 wrap list gave the boundary its contract-shaped lanes;
        // each guard gets its refusing case AND its accepting pair here.

        // begin_close's receipt: the contract wait refuses from inside the
        // source's own finalizer, and completes with the observed truth
        // outside it. A close that PERFORMED the shutdown is not
        // already-terminal.
        let factory = EmbeddedSourceFactory::new();
        let handle = factory
            .open(ProjectKey::new("src-close"))
            .expect("an open registry admits a fresh key");
        // C4: the view's phase comes from the flag the handle OWNS — open
        // means Loading, and after the close below it must say Stopped,
        // because reporting Loading for a torn-down source is a claim about
        // something that no longer exists.
        assert_eq!(
            handle.runtime_view().phase,
            PublicSourceRuntimePhase::Loading
        );
        let receipt = handle.begin_close();
        assert_eq!(
            handle.runtime_view().phase,
            PublicSourceRuntimePhase::Stopped,
            "a closed handle must not report Loading"
        );
        let error = handle
            .finalize(|| receipt.wait(std::time::Instant::now()))
            .expect_err("the contract wait carries the self-wait guard, not just wait_for_test");
        assert_eq!(error, ReceiptWaitError::WouldSelfWait);
        let report = receipt
            .wait(std::time::Instant::now())
            .expect("outside the finalizer the same wait completes");
        assert!(
            !report.already_terminal,
            "this close performed the shutdown, so reporting it as \
             joined-terminal would be a false claim about who tore the \
             source down"
        );
        assert_eq!(
            report.terminal_source_version, 0,
            "dark sources hold version 0"
        );
        // C13, the accepting pair: a SECOND begin_close on the same source
        // JOINS an already-terminal close, and its report must say so.
        let joined = handle
            .begin_close()
            .wait(std::time::Instant::now())
            .expect("joining an already-terminal close completes");
        assert!(
            joined.already_terminal,
            "the second close joined a terminal source; reporting it as \
             having performed the shutdown would claim work this call did \
             not do"
        );

        // The process runtime's shutdown receipt reports observed zeros —
        // the dark runtime closed no holder's source and joined no workers.
        let runtime = ProcessRuntimeApi::acquire().expect("the dark acquisition admits");
        let shutdown = runtime
            .begin_shutdown()
            .wait(std::time::Instant::now())
            .expect("a shutdown that owned nothing completes");
        assert_eq!(shutdown.closed_sources, 0);
        assert_eq!(shutdown.joined_workers, 0);

        // open_embedded_source: sole-handle admission is the accepting case;
        // a second open of the held source refuses as SelectionUnavailable —
        // the selection exists but is unavailable until its holder closes.
        let root = std::path::PathBuf::from("dark-open-root");
        let held = runtime
            .open_embedded_source(EmbeddedSourceSpec::current_worktree(root.clone()))
            .expect("the first open of a source admits");
        let refusal = runtime
            .open_embedded_source(EmbeddedSourceSpec::current_worktree(root))
            .expect_err("the sole handle is out; a second open must refuse");
        assert_eq!(refusal.kind_name(), "SelectionUnavailable");
        drop(held);
    }
}

// ── Frozen seam anchors (C5) ───────────────────────────────────────────────

/// SEAM-STATE anchor: the project's derived-state directory placement.
/// Every V11 checkpoint and team artifact persists beneath it — never
/// beneath a protected source root (the frozen FR-051 placement rule).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectStateDir(pub std::path::PathBuf);

/// SEAM-STATE anchor: the opt-in team artifact's observed state at the
/// runtime seam (`None` until an export completed; the byte count is the
/// export receipt's own measurement, frozen FR-051).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TeamArtifactState {
    /// Bytes of the last completed export, when one has completed.
    pub exported_bytes: Option<usize>,
}
