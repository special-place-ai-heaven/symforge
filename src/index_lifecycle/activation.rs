//! Feature 020 V11, T066 — the activation mode machine.
//!
//! `LegacyOpen -> LegacyClosing -> PreventiveV1Open`, monotonic (no reverse
//! edge exists as API), process-wide (one machine per process behind
//! [`ActivationCut::process`]), and non-configurable (no environment or
//! config read can select a mode; the only way a mode changes is the typed
//! transition evidence below). The frozen lane list is T066's own: every
//! tool/resource/prompt query, cache/CCR/retrieval, sidecar/hook, and
//! finalization lane registers before the legacy gate may begin closing.
//!
//! `LegacyClosing` is the drain window from the 028 data model: the legacy
//! gate drains, cache/CCR invalidate, responses finalize. The machine cannot
//! observe a lane's internals, so drain evidence is the CONSUMPTION of that
//! lane's non-Clone [`LaneRegistration`] token by its owner — the one party
//! that can observe its own drain. A premature confirmation (before the
//! drain window opens) consumes the token and refuses, deliberately
//! stranding the machine short of `PreventiveV1Open`: a mis-sequenced
//! bootstrap fails loudly at the cut, never silently half-open.
//!
//! Companion invariant (enforced by construction across the cut, recorded
//! here): the two publication roots are never simultaneously authoritative —
//! legacy authority serves only in `LegacyOpen`, the V11 publication root
//! serves only in `PreventiveV1Open`, and the window between them is
//! drain-only.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use super::adapters::{AdapterRefusal, execute_plan, plan_admission};
use super::authority::{
    AuthorityRefusal, BindingAuthority, CurrentPublication, MutationGrantInput, ObserverToken,
    SourceRuntime,
};
use super::candidate::{
    CandidateSource, IsolatedCandidate, ProjectArtifactRoot, SourceContentToken, SourceId,
    SourceObservation,
};
use super::capacity::{OwnerIdentity, ProcessCapacityPool};
use super::mutation::{PermitDrainSignal, RefreshTicket, SourceMutationPermit};
use super::observer::{CoalescingAccumulator, ObservationCut, ObserverId, ObserverSlot};
use super::physical_root::{PhysicalRootIdentity, PhysicalRootLease, WriteReceipt};
use super::process_runtime::{ProcessIndexRuntime, SurfaceKind};
use super::registry::{
    LiveProjectSlot, ProjectKey, ProjectRegistry, RegistryRefusal, RootProtection,
    StatePlacement as AdmissionStatePlacement,
};
use super::supervisor::SourceSupervisor;
use super::transition::{self, TransitionKind};
use crate::domain::index::CatalogPath;
use crate::lifecycle_identity::PublicationIdentity;

/// The closed three-mode machine (frozen T066). Monotonic: the enum has no
/// reverse edge and the type exposes no API that revisits an earlier mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivationMode {
    /// Legacy authority serves; V11 lanes register.
    LegacyOpen,
    /// The drain window: nothing new enters the legacy gate, registered
    /// lanes confirm their drains, caches/CCR invalidate, responses finalize.
    LegacyClosing,
    /// The preventive lifecycle is the only live mode.
    PreventiveV1Open,
}

/// The closed lane set from the frozen T066 text: "every tool/resource/prompt
/// query, cache/CCR/retrieval, sidecar/hook, and finalization lane".
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RegisteredLane {
    ToolQuery,
    ResourceQuery,
    PromptQuery,
    Cache,
    Ccr,
    Retrieval,
    Sidecar,
    Hook,
    Finalization,
}

impl RegisteredLane {
    /// Every lane the cut registers; `begin_closing` refuses while any is
    /// missing, because an unregistered lane is exactly the unmatched
    /// ingress SC-001 forbids.
    pub const ALL: [RegisteredLane; 9] = [
        RegisteredLane::ToolQuery,
        RegisteredLane::ResourceQuery,
        RegisteredLane::PromptQuery,
        RegisteredLane::Cache,
        RegisteredLane::Ccr,
        RegisteredLane::Retrieval,
        RegisteredLane::Sidecar,
        RegisteredLane::Hook,
        RegisteredLane::Finalization,
    ];
}

/// Typed refusals. Every variant is producible by a caller of this module;
/// none is speculative.
#[derive(Debug, PartialEq, Eq)]
pub enum ActivationRefusal {
    /// The lane already holds a registration on this machine.
    LaneAlreadyRegistered(RegisteredLane),
    /// `begin_closing` found lanes that never registered.
    LanesUnregistered(Vec<RegisteredLane>),
    /// `open_preventive` found registered lanes that never confirmed drain.
    LanesUndrained(Vec<RegisteredLane>),
    /// The operation is legal only in `expected`; the machine is in `actual`.
    WrongMode {
        expected: ActivationMode,
        actual: ActivationMode,
    },
    /// The registration token was minted by a different machine instance.
    ForeignRegistration,
}

/// One lane's registration on one machine. Non-Clone: consuming it in
/// [`ActivationCut::confirm_drained`] is the drain evidence, and evidence
/// that could be duplicated would let one drain vouch twice.
#[derive(Debug)]
pub struct LaneRegistration {
    machine: u64,
    lane: RegisteredLane,
}

impl LaneRegistration {
    /// Which lane this registration binds.
    pub fn lane(&self) -> RegisteredLane {
        self.lane
    }
}

#[derive(Debug, Default)]
struct LaneTable {
    registered: BTreeSet<RegisteredLane>,
    drained: BTreeSet<RegisteredLane>,
}

/// The process-wide activation machine. Contract seam
/// `src/index_lifecycle/activation.rs::ActivationCut` in the frozen
/// retirement inventory (hooks, compatibility_aliases categories).
#[derive(Debug)]
pub struct ActivationCut {
    identity: u64,
    mode: AtomicU8,
    lanes: Mutex<LaneTable>,
}

static NEXT_MACHINE_IDENTITY: AtomicU64 = AtomicU64::new(1);
static PROCESS_MACHINE: OnceLock<ActivationCut> = OnceLock::new();

const MODE_LEGACY_OPEN: u8 = 0;
const MODE_LEGACY_CLOSING: u8 = 1;
const MODE_PREVENTIVE_V1_OPEN: u8 = 2;

fn mode_of(raw: u8) -> ActivationMode {
    match raw {
        MODE_LEGACY_OPEN => ActivationMode::LegacyOpen,
        MODE_LEGACY_CLOSING => ActivationMode::LegacyClosing,
        _ => ActivationMode::PreventiveV1Open,
    }
}

impl ActivationCut {
    fn new() -> Self {
        ActivationCut {
            identity: NEXT_MACHINE_IDENTITY.fetch_add(1, Ordering::Relaxed),
            mode: AtomicU8::new(MODE_LEGACY_OPEN),
            lanes: Mutex::new(LaneTable::default()),
        }
    }

    /// The one process-wide machine. Mode selection is non-configurable:
    /// this accessor reads no environment and takes no parameters, and no
    /// other constructor is reachable from production code.
    pub fn process() -> &'static ActivationCut {
        PROCESS_MACHINE.get_or_init(ActivationCut::new)
    }

    /// A fresh machine for in-crate oracles only, so scenario isolation
    /// never leaks through the process singleton.
    #[cfg(all(test, feature = "server"))]
    pub(crate) fn fresh_for_oracles() -> Self {
        ActivationCut::new()
    }

    /// The current mode. Lock-free; safe from any lane.
    pub fn mode(&self) -> ActivationMode {
        mode_of(self.mode.load(Ordering::Acquire))
    }

    /// Register one lane while the legacy gate is still open.
    pub fn register_lane(
        &self,
        lane: RegisteredLane,
    ) -> Result<LaneRegistration, ActivationRefusal> {
        // Mode is checked under the lane lock so `begin_closing` can never
        // race past a registration it did not count.
        let mut table = self.lanes.lock().expect("activation lane lock");
        let actual = self.mode();
        if actual != ActivationMode::LegacyOpen {
            return Err(ActivationRefusal::WrongMode {
                expected: ActivationMode::LegacyOpen,
                actual,
            });
        }
        if !table.registered.insert(lane) {
            return Err(ActivationRefusal::LaneAlreadyRegistered(lane));
        }
        Ok(LaneRegistration {
            machine: self.identity,
            lane,
        })
    }

    /// Close the legacy gate and open the drain window. Refuses while any
    /// lane of the closed set has not registered.
    pub fn begin_closing(&self) -> Result<(), ActivationRefusal> {
        let table = self.lanes.lock().expect("activation lane lock");
        let actual = self.mode();
        if actual != ActivationMode::LegacyOpen {
            return Err(ActivationRefusal::WrongMode {
                expected: ActivationMode::LegacyOpen,
                actual,
            });
        }
        let missing: Vec<RegisteredLane> = RegisteredLane::ALL
            .iter()
            .copied()
            .filter(|lane| !table.registered.contains(lane))
            .collect();
        if !missing.is_empty() {
            return Err(ActivationRefusal::LanesUnregistered(missing));
        }
        self.mode.store(MODE_LEGACY_CLOSING, Ordering::Release);
        Ok(())
    }

    /// One lane's owner confirms its drain by consuming its registration.
    /// Consumption is unconditional: a refused confirmation (wrong window,
    /// foreign token) deliberately strands the machine short of
    /// `PreventiveV1Open` — see the module doc.
    pub fn confirm_drained(&self, registration: LaneRegistration) -> Result<(), ActivationRefusal> {
        let mut table = self.lanes.lock().expect("activation lane lock");
        // A foreign token is refused before the window check: it says
        // nothing about THIS machine's lanes in any mode.
        if registration.machine != self.identity {
            return Err(ActivationRefusal::ForeignRegistration);
        }
        let actual = self.mode();
        if actual != ActivationMode::LegacyClosing {
            return Err(ActivationRefusal::WrongMode {
                expected: ActivationMode::LegacyClosing,
                actual,
            });
        }
        table.drained.insert(registration.lane);
        Ok(())
    }

    /// Open the preventive mode. Refuses while any registered lane has not
    /// confirmed its drain.
    pub fn open_preventive(&self) -> Result<(), ActivationRefusal> {
        let table = self.lanes.lock().expect("activation lane lock");
        let actual = self.mode();
        if actual != ActivationMode::LegacyClosing {
            return Err(ActivationRefusal::WrongMode {
                expected: ActivationMode::LegacyClosing,
                actual,
            });
        }
        let undrained: Vec<RegisteredLane> = table
            .registered
            .iter()
            .copied()
            .filter(|lane| !table.drained.contains(lane))
            .collect();
        if !undrained.is_empty() {
            return Err(ActivationRefusal::LanesUndrained(undrained));
        }
        self.mode.store(MODE_PREVENTIVE_V1_OPEN, Ordering::Release);
        Ok(())
    }
}

// ── The write-lane bridge (T064, C2) and observation lane (T029, C3) ───────

/// One project root's live source authority: the bridge the cut's writer
/// lanes acquire mutation permits from, and the observation lane the
/// watcher/facade admissions and background callbacks report through.
///
/// MID-CUT BRIDGING CLAIM, recorded: construction seeds the runtime
/// `Current` on a fresh publication because the V10 data plane it stands
/// beside serves queries today; C4 ties construction to observed bootstrap
/// state. Since C3 every publication carries the ACTIVE observer
/// incarnation's token (the `ObserverId`/`ObserverToken` unification the
/// observer module recorded as T064 work), and every permit return consumes
/// the accumulated observation cut. Neither remaining simplification
/// weakens the write path itself: every write still requires a granted
/// permit, publishes non-`Current` first, and returns to `Current` only
/// through a fresh publication (FR-043).
#[derive(Debug)]
pub struct ProjectSourceAuthority {
    root: PathBuf,
    /// The identity of this root's FIRST lease, presented for project
    /// admission (C4b). Every open of one canonicalized root converges on
    /// this authority and therefore presents the same physical identity —
    /// which is what lets concurrent opens join one admission. Mid-cut
    /// residual: a root physically replaced at the same path keeps its
    /// admission identity until C5's transitions own rebinding.
    admission_root: PhysicalRootIdentity,
    inner: Mutex<AuthorityInner>,
    // Separate mutex, strict ordering: lane state is always taken and
    // RELEASED before `inner` is locked (see `reconcile_returned`), so the
    // watcher thread's observations never deadlock against a writer.
    lane: Mutex<ObservationLane>,
}

#[derive(Debug)]
struct AuthorityInner {
    runtime: SourceRuntime,
    lease: Arc<PhysicalRootLease>,
    outstanding: Arc<PermitDrainSignal>,
}

/// The C3 observation lane: one active observer incarnation (watcher or
/// embed facade), the bounded coalescing accumulator, and the per-source
/// supervisor + isolated-candidate pipeline every admission drives —
/// PERMIT-FREE, per the frozen contract (observation never mutates source
/// bytes; it has no business in the mutation lane).
///
/// D1 applies: this is the AUTHORITY plane. The LiveIndex data plane keeps
/// serving admissions itself mid-cut; the lane runs the frozen lifecycle
/// semantics beside it (dark stamp payloads) until C4/C5 make it the root.
#[derive(Debug)]
struct ObservationLane {
    slot: ObserverSlot,
    /// The authority-side token paired with the active `ObserverId`.
    active_token: ObserverToken,
    accumulator: CoalescingAccumulator,
    supervisors: BTreeMap<u64, SourceSupervisor>,
    artifact_root: ProjectArtifactRoot,
    pool: Arc<ProcessCapacityPool>,
    owner: OwnerIdentity,
    next_observer: u64,
    next_stamp: u64,
    /// Probe for the wiring oracles: the cut the most recent permit return
    /// consumed.
    last_reconcile_cut: Option<ObservationCut>,
}

/// Bounded distinct pending sources before the accumulator latches the
/// capacity-exhausted full baseline.
const OBSERVATION_ACCUMULATOR_BOUND: usize = 4096;

/// Dark capacity budget for the observation lane's candidate builds. Real
/// derivation payloads (and the C8 conservation oracle) replace this.
/// The dark per-surface observation budget (C7/C8 replace it with a
/// measured value); public so the benchmark receipt records the exact
/// pre-granted capacity vector it ran under.
pub const OBSERVATION_CAPACITY_BYTES: u64 = 1 << 30;

fn observation_source_id(relative: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    relative.hash(&mut hasher);
    hasher.finish()
}

/// A granted, in-progress write on one project source. Non-Clone; ends by
/// [`WriteAuthority::finish_committed`], [`WriteAuthority::finish_no_side_effect`],
/// or drop — since C3 a drop RECOVERS: the permit drains, the scope latches
/// dirty (the write's outcome is unobserved), and the source returns to
/// `Current` under a full-baseline observation instead of stranding.
#[derive(Debug)]
pub struct WriteAuthority {
    authority: Arc<ProjectSourceAuthority>,
    /// `None` only after a finish path has taken it; `Drop` treats that as
    /// "already terminal, nothing to recover".
    permit: Option<SourceMutationPermit>,
    frozen_publication: PublicationIdentity,
}

impl ProjectSourceAuthority {
    /// The bridge for one project root. See the type doc for the recorded
    /// mid-cut construction claim.
    pub fn for_root(root: &Path) -> Arc<Self> {
        // Dormant while idle: the OS directory handle exists only during a
        // permit cycle (see `PhysicalRootLease::parked`), so an idle root
        // stays user-movable.
        let lease = Arc::new(PhysicalRootLease::take(root).parked());
        let admission_root = lease.identity();
        let binding = BindingAuthority::bind(lease.identity());
        // The seed publication carries the SEED observer incarnation's
        // token — the same identity `active_token` starts on — so even the
        // first publication names a real registration, not an anonymous
        // mint.
        let seed_token = ObserverToken::fresh();
        let publication = CurrentPublication::promote(binding, seed_token);
        let pool = ProcessCapacityPool::new();
        let owner = pool.root(OBSERVATION_CAPACITY_BYTES);
        Arc::new(Self {
            root: root.to_path_buf(),
            admission_root,
            inner: Mutex::new(AuthorityInner {
                runtime: SourceRuntime::current(publication),
                lease,
                // Unarmed: reports ended until a permit arms it, which is
                // what lets the first transition treat "no permit ever" as
                // drained rather than as an optional check.
                outstanding: Arc::new(PermitDrainSignal::new()),
            }),
            lane: Mutex::new(ObservationLane {
                slot: ObserverSlot::new(ObserverId(1)),
                active_token: seed_token,
                accumulator: CoalescingAccumulator::new(OBSERVATION_ACCUMULATOR_BOUND),
                supervisors: BTreeMap::new(),
                artifact_root: ProjectArtifactRoot::empty(),
                pool,
                owner,
                next_observer: 1,
                next_stamp: 0,
                last_reconcile_cut: None,
            }),
        })
    }

    /// Register a fresh observer incarnation (watcher (re)start, facade
    /// attach): a drain-before-successor handoff on the slot, a fresh
    /// authority-side token, and the successor's post-barrier full-baseline
    /// obligation latched into the cut stream. Late callbacks holding the
    /// predecessor's id are refused by every observation entry point below.
    pub fn register_observer(&self) -> ObserverId {
        let mut lane = self.lane.lock().expect("observation lane lock");
        lane.next_observer += 1;
        let successor = ObserverId(lane.next_observer);
        lane.slot
            .begin_handoff(successor)
            .expect("registration completes each handoff before returning");
        // The lane consumes cuts at reconcile, so the predecessor never
        // holds undelivered cuts here; deliver anyway so the refusal path
        // stays unreachable by construction, not by luck.
        let _ = lane.slot.deliver_pending();
        let kind = lane
            .slot
            .complete_handoff()
            .expect("predecessor delivered above");
        debug_assert_eq!(
            kind,
            super::observer::CutKind::FullBaseline {
                cause: super::observer::LatchCause::HandoffBarrier
            }
        );
        // Thread the returned obligation into the cut stream (T064).
        lane.accumulator.latch_handoff_barrier();
        lane.active_token = ObserverToken::fresh();
        successor
    }

    /// The observer incarnation currently holding the slot.
    pub fn active_observer(&self) -> ObserverId {
        self.lane
            .lock()
            .expect("observation lane lock")
            .slot
            .active()
    }

    /// Observe one admitted source through the isolated candidate pipeline,
    /// permit-free: supervisor attempt -> delta candidate -> the single
    /// commit point -> accumulator. A stale incarnation is refused — the
    /// late-callback unreachability the frozen contract demands.
    pub fn observe_admission(
        &self,
        observer: ObserverId,
        relative: &str,
    ) -> Result<(), ObserverId> {
        let mut lane = self.lane.lock().expect("observation lane lock");
        if lane.slot.active() != observer {
            return Err(lane.slot.active());
        }
        lane.observe_admission_locked(relative);
        Ok(())
    }

    /// Observe one removal. The dark candidate pipeline carries no removal
    /// payload (recorded bridging simplification): the removal lands as an
    /// accumulator invalidation only.
    pub fn observe_removal(&self, observer: ObserverId, relative: &str) -> Result<(), ObserverId> {
        let mut lane = self.lane.lock().expect("observation lane lock");
        if lane.slot.active() != observer {
            return Err(lane.slot.active());
        }
        lane.next_stamp += 1;
        let stamp = lane.next_stamp;
        lane.accumulator
            .observe(observation_source_id(relative), || stamp);
        Ok(())
    }

    /// Report a lost/overflowed observation stream (watcher overflow):
    /// latches the gap so the next cut is a full baseline.
    pub fn report_gap(&self, observer: ObserverId) -> Result<(), ObserverId> {
        let mut lane = self.lane.lock().expect("observation lane lock");
        if lane.slot.active() != observer {
            return Err(lane.slot.active());
        }
        lane.accumulator.report_gap();
        Ok(())
    }

    /// [`Self::observe_admission`] attributed to the CURRENT incarnation —
    /// the synchronous facade entry (embed `update_file_from_disk`), which
    /// holds no id across time and so cannot be a late callback.
    pub fn observe_admission_active(&self, relative: &str) {
        let mut lane = self.lane.lock().expect("observation lane lock");
        lane.observe_admission_locked(relative);
    }

    /// [`Self::observe_removal`] attributed to the current incarnation.
    pub fn observe_removal_active(&self, relative: &str) {
        let mut lane = self.lane.lock().expect("observation lane lock");
        lane.next_stamp += 1;
        let stamp = lane.next_stamp;
        lane.accumulator
            .observe(observation_source_id(relative), || stamp);
    }

    /// Measurement probe (C8, T069): the observation lane's capacity ledger
    /// as `(charged now, pre-granted, outstanding charges, unknown refunds)`.
    /// Reads the pool's own counters — the probe reports the ledger, never a
    /// cached belief.
    pub fn observation_capacity_ledger(&self) -> (u64, u64, usize, u64) {
        let lane = self.lane.lock().expect("observation lane lock");
        (
            lane.pool.charged(lane.owner),
            OBSERVATION_CAPACITY_BYTES,
            lane.pool.outstanding_charges(lane.owner),
            lane.pool.unknown_refunds(),
        )
    }

    /// Measurement probe (C8, T069): retained observation artifacts as
    /// `(sources, dark retained bytes)`. The dark payload weight is the same
    /// one the candidate pipeline reserves with (one byte per observed
    /// source), so `retained + candidate <= pregranted` is measured in the
    /// pipeline's own units; the sealed artifact machinery replaces the
    /// weight with real bytes when it lands.
    pub fn retained_observation_artifacts(&self) -> (usize, u64) {
        let lane = self.lane.lock().expect("observation lane lock");
        let publication = lane.artifact_root.load();
        (publication.sources.len(), publication.sources.len() as u64)
    }

    /// Wiring-oracle probe: how many observation candidates for `relative`
    /// have reached the single commit point.
    pub fn committed_observations(&self, relative: &str) -> u64 {
        let lane = self.lane.lock().expect("observation lane lock");
        lane.supervisors
            .get(&observation_source_id(relative))
            .map(|supervisor| supervisor.committed_generations())
            .unwrap_or(0)
    }

    /// Wiring-oracle probe: the cut the most recent permit return consumed.
    pub fn last_reconcile_cut(&self) -> Option<ObservationCut> {
        self.lane
            .lock()
            .expect("observation lane lock")
            .last_reconcile_cut
            .clone()
    }

    /// Whether the source is live `Current` (queryable, grantable).
    pub fn is_current(&self) -> bool {
        self.inner
            .lock()
            .expect("project source authority lock")
            .runtime
            .live_publication()
            .is_some()
    }

    /// The live publication identity, when `Current`.
    pub fn current_publication(&self) -> Option<PublicationIdentity> {
        self.inner
            .lock()
            .expect("project source authority lock")
            .runtime
            .live_publication()
            .map(|publication| publication.publication())
    }

    /// The binding this authority presents for project admission: a fresh
    /// binding identity over the STABLE admission-root identity (see the
    /// field doc), so every open of one canonicalized root can join one
    /// registry occupancy.
    pub fn admission_binding(&self) -> BindingAuthority {
        BindingAuthority::bind(self.admission_root)
    }

    /// How many mutation grants this source has ever issued. The wiring
    /// oracles pin routing on this: a write that went through the permit
    /// lane moved the counter; one that bypassed it did not.
    pub fn grants_issued(&self) -> u64 {
        self.inner
            .lock()
            .expect("project source authority lock")
            .runtime
            .grants_issued()
    }

    /// Acquire the single write authority for this source: grant against the
    /// live `Current` publication (which publishes non-`Current` before the
    /// permit exists), then mint the permit pinned to this root's lease.
    pub fn acquire_write(self: &Arc<Self>) -> Result<WriteAuthority, AuthorityRefusal> {
        let mut inner = self.inner.lock().expect("project source authority lock");
        let presented = match inner.runtime.live_publication() {
            Some(publication) => publication.publication(),
            None => {
                return Err(AuthorityRefusal::PhaseNotCurrent {
                    phase: inner.runtime.phase(),
                });
            }
        };
        let grant = inner
            .runtime
            .request_mutation_grant(MutationGrantInput::LiveCurrent(presented))?;
        let frozen_publication = grant.published_non_current().publication();
        let drain = Arc::new(PermitDrainSignal::new());
        // Unpark the dormant lease for exactly this permit cycle. Stored only
        // on a successful grant: a refused grant drops the reopened handle
        // and the authority stays dormant.
        let live_lease = Arc::new(inner.lease.reopened());
        let permit = SourceMutationPermit::grant(grant, live_lease.clone(), drain.clone())?;
        inner.lease = live_lease;
        inner.outstanding = drain;
        Ok(WriteAuthority {
            authority: self.clone(),
            permit: Some(permit),
            frozen_publication,
        })
    }

    /// [`Self::acquire_write`] with bounded patience for a sibling writer:
    /// retries `PhaseNotCurrent` (~2s at 25ms steps) before surfacing the
    /// refusal honestly. C4 replaces cross-lane patience with structural
    /// serialization at bootstrap; until then every writer lane shares this
    /// policy (protocol/edit.rs carries its own identical C2 loop).
    pub fn acquire_write_serialized(self: &Arc<Self>) -> Result<WriteAuthority, AuthorityRefusal> {
        let mut attempts = 0u32;
        loop {
            match self.acquire_write() {
                Ok(write) => return Ok(write),
                Err(refusal @ AuthorityRefusal::PhaseNotCurrent { .. }) => {
                    attempts += 1;
                    if attempts >= 80 {
                        return Err(refusal);
                    }
                    std::thread::sleep(std::time::Duration::from_millis(25));
                }
                Err(refusal) => return Err(refusal),
            }
        }
    }

    /// Return the source to `Current` through a fresh publication after its
    /// permit reached a terminal path: retire the permit, then the sealed
    /// Freeze -> Drain -> Install transition under a fresh lease and binding.
    fn reconcile_returned(
        &self,
        frozen_publication: PublicationIdentity,
        ticket: RefreshTicket,
    ) -> Result<RefreshTicket, AuthorityRefusal> {
        self.return_to_current(frozen_publication)?;
        Ok(ticket)
    }

    /// The shared return path: consume the accumulated observation cut
    /// (lane lock, released before `inner` is taken — the documented lock
    /// order), then retire the permit and apply the sealed transition. The
    /// returning publication carries the ACTIVE observer incarnation's
    /// token.
    fn return_to_current(
        &self,
        frozen_publication: PublicationIdentity,
    ) -> Result<(), AuthorityRefusal> {
        let observer_cut = {
            let mut lane = self.lane.lock().expect("observation lane lock");
            let cut = lane.accumulator.cut();
            lane.last_reconcile_cut = Some(cut);
            lane.active_token
        };
        let mut guard = self.inner.lock().expect("project source authority lock");
        let inner = &mut *guard;
        inner.runtime.retire_permit(frozen_publication);
        let outstanding = inner.outstanding.clone();
        // The incoming lease is stored DORMANT: its capability opens at the
        // next permit cycle's acquire, so the returned-to-`Current` root is
        // immediately user-movable again.
        let new_lease = Arc::new(PhysicalRootLease::take(self.root.as_path()).parked());
        let incoming = BindingAuthority::bind(new_lease.identity());
        let old_lease = inner.lease.clone();
        transition::apply(
            &mut inner.runtime,
            TransitionKind::Reload,
            &old_lease,
            incoming,
            observer_cut,
            &outstanding,
        )?;
        inner.lease = new_lease;
        Ok(())
    }

    /// The C3 re-scout recovery lane: a write authority dropped without a
    /// finish path leaves a write of UNOBSERVED outcome behind. Latch the
    /// scope dirty (the next cut is a full baseline over the possibly
    /// half-written scope), then return the source to `Current` exactly
    /// like a terminal return. Failure here leaves the source non-`Current`
    /// and observable as such — recovery is attempted, never asserted.
    fn recover_stranded(
        &self,
        frozen_publication: PublicationIdentity,
    ) -> Result<(), AuthorityRefusal> {
        self.lane
            .lock()
            .expect("observation lane lock")
            .accumulator
            .latch_scope_dirty();
        self.return_to_current(frozen_publication)
    }
}

impl ObservationLane {
    /// One admitted source through the pipeline: supervisor attempt ->
    /// isolated delta candidate (dark stamp payload) -> the single commit
    /// point -> accumulator. A refused candidate (capacity, drift,
    /// supersession) latches a gap: the CHANGE is retained as a
    /// full-baseline obligation, never dropped.
    fn observe_admission_locked(&mut self, relative: &str) {
        let source_raw = observation_source_id(relative);
        self.next_stamp += 1;
        let stamp = self.next_stamp;
        let supervisor = self.supervisors.entry(source_raw).or_default();
        let attempt = supervisor.begin_attempt();
        let source = SourceId(source_raw);
        let expected = self
            .artifact_root
            .load()
            .sources
            .get(&source)
            .map(|artifacts| artifacts.token);
        let candidate = IsolatedCandidate::prepare_delta(
            &self.pool,
            self.owner,
            &attempt,
            CandidateSource {
                id: source,
                observation: SourceObservation::Content {
                    path: CatalogPath {
                        public_id: relative.to_string(),
                        normalized_utf8: Some(relative.to_string()),
                    },
                    token: SourceContentToken(stamp),
                    bytes: 1,
                },
            },
            expected,
            |_| stamp,
        );
        let committed = match candidate {
            Ok(candidate) => candidate.commit(&self.artifact_root).is_ok(),
            Err(_) => false,
        };
        if committed {
            self.accumulator.observe(source_raw, || stamp);
        } else {
            self.accumulator.report_gap();
        }
    }
}

impl WriteAuthority {
    /// Write one path beneath the authorized root. The first write begins the
    /// permit's side effect; later writes continue it (a batch is one
    /// authority, many receipts).
    pub fn write(
        &mut self,
        relative: &Path,
        contents: &[u8],
    ) -> Result<WriteReceipt, AuthorityRefusal> {
        let permit = self
            .permit
            .as_mut()
            .expect("permit lives until a finish path");
        match permit.start_side_effect() {
            Ok(()) => {}
            // Continuing the batch: the side effect is already in flight.
            Err(AuthorityRefusal::SideEffectAlreadyInFlight) => {}
            Err(refusal) => return Err(refusal),
        }
        permit.replace_beneath(relative, contents)
    }

    /// Begin a DELEGATED side effect: the caller is about to run its own
    /// contract-pinned durability protocol (curation policy lane) beneath
    /// this authority's root. The permit goes in flight FIRST, so a protocol
    /// failure drops through the re-scout recovery lane as a write of
    /// unobserved outcome, never as a claimed no-op.
    pub fn begin_delegated(&mut self) -> Result<(), AuthorityRefusal> {
        let permit = self
            .permit
            .as_mut()
            .expect("permit lives until a finish path");
        match permit.start_side_effect() {
            Ok(()) | Err(AuthorityRefusal::SideEffectAlreadyInFlight) => Ok(()),
            Err(refusal) => Err(refusal),
        }
    }

    /// Attest the delegated protocol's outcome: the pinned lease re-reads
    /// `relative` and mints a receipt only if it observes exactly the
    /// authorized post-image. `Ok(None)` is a mismatch — no receipt exists,
    /// and dropping this authority is the caller's honest terminal (the
    /// recovery lane returns the source to `Current` scope-dirty).
    pub fn attest_delegated(
        &mut self,
        relative: &Path,
        expected: &[u8],
    ) -> Result<Option<WriteReceipt>, AuthorityRefusal> {
        self.permit
            .as_mut()
            .expect("permit lives until a finish path")
            .attest_delegated_beneath(relative, expected)
    }

    /// Commit the observed side effect and return the source to `Current`
    /// through a fresh publication: retire the permit, then apply the sealed
    /// Freeze -> Drain -> Install transition under a fresh root lease and
    /// binding. A commit REFUSAL leaves the permit in place, so the drop
    /// recovery below still returns the source to `Current`.
    pub fn finish_committed(
        mut self,
        receipt: WriteReceipt,
    ) -> Result<RefreshTicket, AuthorityRefusal> {
        let ticket = self
            .permit
            .as_mut()
            .expect("permit lives until a finish path")
            .commit(receipt)?;
        drop(self.permit.take());
        self.authority
            .clone()
            .reconcile_returned(self.frozen_publication, ticket)
    }

    /// Terminate with the permit's own observation that nothing was written
    /// through its lease (the permit never began a side effect), and return
    /// the source to `Current` through a fresh publication — FR-043 applies
    /// to no-ops too. Refuses once a side effect has begun (the drop
    /// recovery then owns the return).
    pub fn finish_no_side_effect(mut self) -> Result<RefreshTicket, AuthorityRefusal> {
        let ticket = self
            .permit
            .as_mut()
            .expect("permit lives until a finish path")
            .no_side_effect()?;
        drop(self.permit.take());
        self.authority
            .clone()
            .reconcile_returned(self.frozen_publication, ticket)
    }
}

impl Drop for WriteAuthority {
    /// The re-scout recovery lane (C3): a write authority dropped while its
    /// permit is still live drains the permit and returns the source to
    /// `Current` under a scope-dirty full-baseline observation. A finish
    /// path already took the permit, so its drop is a no-op here.
    fn drop(&mut self) {
        if let Some(permit) = self.permit.take() {
            drop(permit);
            let _ = self.authority.recover_stranded(self.frozen_publication);
        }
    }
}

static PROJECT_AUTHORITIES: OnceLock<Mutex<HashMap<PathBuf, Arc<ProjectSourceAuthority>>>> =
    OnceLock::new();

/// The process registry of per-root write authorities. C2's self-provisioning
/// seam for the writer lanes; C4 moves ownership into runtime bootstrap.
pub fn project_source_authority(root: &Path) -> Arc<ProjectSourceAuthority> {
    // One authority per PHYSICAL root: spelling variants (canonical, \\?\
    // extended, relative) must converge on one lease and one serialization
    // point, or two writer lanes on the same repository would not contend.
    // Canonicalization failure (root vanished mid-call) falls back to the
    // literal key rather than refusing here: the acquire path surfaces the
    // real I/O failure with its own evidence.
    let key = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let registry = PROJECT_AUTHORITIES.get_or_init(|| Mutex::new(HashMap::new()));
    let mut map = registry.lock().expect("project authority registry lock");
    map.entry(key.clone())
        .or_insert_with(|| ProjectSourceAuthority::for_root(&key))
        .clone()
}

// ── The per-project runtime handle (D1, C4) ────────────────────────────────

/// D1 (Feature 020 Slice 4, T030, C4): the per-project runtime handle — the
/// ONLY holder of a project's `SharedIndex` data plane in daemon, protocol,
/// server, sidecar, or embed state. The frozen publication_roots census
/// retires every bare `index: SharedIndex` field; replacing those field
/// types with this handle drives the compiler over every touch site, and
/// the two accessors below are the ENUMERABLE door the C4 neck
/// acquisitions progressively narrow — a `data_plane()` call site is a
/// member of the rerouting set by construction, not by grep luck.
///
/// Mid-cut bridging claim, recorded: the accessors expose the V10 data
/// plane unchanged (behavior-preserving ownership move); typed acquisition
/// branches arrive at the dispatch necks with C4's bootstrap and gating
/// commits, not by rewriting each handler body.
// No Debug derive: `SharedIndexHandle` itself carries none.
#[derive(Clone)]
pub struct ProjectRuntimeHandle {
    // Named `data_plane`, not `index`: the frozen census derivation counts
    // every `index: SharedIndex(Handle)` field as a V10 publication root,
    // and this — the sole authorized holder — is the replacement, not a root.
    data_plane: crate::live_index::store::SharedIndex,
    // The admission slot this runtime serves under (C4c). Carried ONLY where
    // a surface can retire slots — the daemon, whose eviction/retarget stop
    // path revokes them. Stdio/serve admissions are process-lifetime with no
    // stop path, so a carried slot there would feed a refusal branch nothing
    // can emit; those handles stay `None` until C5's typed bootstrap.
    admission: Option<Arc<super::registry::LiveProjectSlot>>,
}

impl ProjectRuntimeHandle {
    /// Bind a handle over the data plane it owns, outside any admission
    /// (stdio/serve/standalone-sidecar surfaces, transient test indexes).
    pub fn bind(index: crate::live_index::store::SharedIndex) -> Self {
        Self {
            data_plane: index,
            admission: None,
        }
    }

    /// Bind a handle over the data plane it owns, under the admission slot
    /// the surface's [`admit_project`] call installed for it.
    pub fn bind_admitted(
        index: crate::live_index::store::SharedIndex,
        admission: Arc<super::registry::LiveProjectSlot>,
    ) -> Self {
        Self {
            data_plane: index,
            admission: Some(admission),
        }
    }

    /// The typed acquisition branch for a dispatch neck (C4c): the data
    /// plane is handed out only while the carried admission slot is still
    /// the live occupancy of its key. The evidence is the registry's own
    /// shared revocation flag — the slot the stop path revokes IS the slot
    /// this handle asks — so a project retired between map lookup and
    /// dispatch refuses instead of serving a retired index.
    pub fn acquire(
        &self,
    ) -> Result<&crate::live_index::store::SharedIndex, super::registry::RegistryRefusal> {
        if let Some(slot) = &self.admission
            && !slot.is_live()
        {
            return Err(super::registry::RegistryRefusal::Tombstoned { slot: slot.slot() });
        }
        Ok(&self.data_plane)
    }

    /// The owned V10 data plane, borrowed.
    pub fn data_plane(&self) -> &crate::live_index::store::SharedIndex {
        &self.data_plane
    }

    /// A clone of the owned data plane for spawn captures.
    pub fn shared(&self) -> crate::live_index::store::SharedIndex {
        Arc::clone(&self.data_plane)
    }

    /// The source authority for the handle's currently bound root, when one
    /// is bound — the same per-root instance every writer and observation
    /// lane converges on through the canonicalized registry.
    pub fn authority(&self) -> Option<Arc<ProjectSourceAuthority>> {
        let root = self.data_plane.read().indexed_root.clone()?;
        Some(project_source_authority(&root))
    }
}

// ── Process bootstrap: ceremony, surfaces, admission (T030, C4b) ───────────

/// Dark process capacity budget: every surface can attach at the observation
/// lane's dark budget. C7/C8 replace the dark constants with measured ones.
const PROCESS_CAPACITY_BYTES: u64 = (SurfaceKind::ALL.len() as u64) * OBSERVATION_CAPACITY_BYTES;

static PROCESS_INDEX_RUNTIME: OnceLock<Arc<ProcessIndexRuntime>> = OnceLock::new();
static PROCESS_PROJECT_REGISTRY: OnceLock<Arc<ProjectRegistry>> = OnceLock::new();
/// Serializes the one-time ceremony and surface attaches so concurrent
/// surface starts cannot race either.
static PROCESS_BOOTSTRAP: Mutex<()> = Mutex::new(());

/// The one process capacity runtime every surface attaches to.
pub fn process_index_runtime() -> Arc<ProcessIndexRuntime> {
    PROCESS_INDEX_RUNTIME
        .get_or_init(|| ProcessIndexRuntime::incarnate(PROCESS_CAPACITY_BYTES))
        .clone()
}

/// The process project registry — the admission table bootstrap owns. The
/// C4b registry-ownership move: projects enter through [`admit_project`],
/// never by a lane minting its own slot.
pub fn process_project_registry() -> Arc<ProjectRegistry> {
    PROCESS_PROJECT_REGISTRY
        .get_or_init(ProjectRegistry::new)
        .clone()
}

/// Drive the process activation machine through its startup ceremony and
/// attach `surface` to the capacity runtime. Idempotent per process and per
/// surface.
///
/// The ceremony runs BEFORE the surface serves its first request, which is
/// what makes each drain confirmation truthful: at that moment the
/// bootstrapper IS every lane's owner, and it can observe that nothing has
/// entered the legacy gate because serving has not begun. After this PR's
/// compile-time flip there is no legacy traffic left to drain at runtime;
/// the machine records that observation as typed evidence instead of
/// assuming it.
pub fn activate_surface(surface: SurfaceKind) {
    let _bootstrap = PROCESS_BOOTSTRAP.lock().expect("process bootstrap lock");
    let machine = ActivationCut::process();
    if machine.mode() == ActivationMode::LegacyOpen {
        let registrations: Vec<LaneRegistration> = RegisteredLane::ALL
            .iter()
            .map(|lane| {
                machine
                    .register_lane(*lane)
                    .expect("the bootstrap lock is held and the machine is LegacyOpen")
            })
            .collect();
        machine
            .begin_closing()
            .expect("every lane registered above");
        for registration in registrations {
            machine
                .confirm_drained(registration)
                .expect("the drain window opened above");
        }
        machine
            .open_preventive()
            .expect("every registered lane confirmed above");
    }
    let runtime = process_index_runtime();
    if runtime.owner_for(surface).is_none() {
        runtime
            .attach(surface, OBSERVATION_CAPACITY_BYTES)
            .expect("all surfaces fit the process budget by construction");
    }
}

/// One admitted project bootstrap (C4b): plan against the process runtime
/// and admit through the process registry — single-flight per key, and a
/// live occupancy joins when root and placement agree — then install as
/// live. Self-activating: the surface's ceremony and capacity attach run
/// first, so a bare test constructing a project without a daemon still
/// admits honestly. The per-root authority is created (or joined) through
/// the same canonicalized registry every writer lane converges on, and the
/// admission presents its stable admission-root identity so every open of
/// one root names the same physical root.
pub fn admit_project(
    surface: SurfaceKind,
    canonical_root: &Path,
    project_id: &str,
    access_mode: crate::domain::index::SourceAccessMode,
    placement: &crate::domain::StatePlacement,
) -> Result<Arc<LiveProjectSlot>, AdapterRefusal> {
    use crate::domain::index::SourceAccessMode as Mode;

    activate_surface(surface);
    let runtime = process_index_runtime();
    let registry = process_project_registry();
    let key = ProjectKey::new(project_id);
    let protection = match access_mode {
        Mode::NormalProject => RootProtection::Normal,
        // A protected root binds only when the operator asked for it
        // explicitly; the explicitness IS the authorization.
        Mode::ExplicitProtected => RootProtection::Protected,
    };
    let authorized = matches!(access_mode, Mode::ExplicitProtected);
    let requested = match placement {
        crate::domain::StatePlacement::ProjectLocal { .. } => AdmissionStatePlacement::ProjectLocal,
        crate::domain::StatePlacement::UserLocal { .. } => AdmissionStatePlacement::UserLocal,
        crate::domain::StatePlacement::MemoryOnly { .. } => AdmissionStatePlacement::MemoryOnly,
    };
    let plan = plan_admission(
        &runtime,
        surface,
        key.clone(),
        protection,
        authorized,
        requested,
    )?;
    let binding = project_source_authority(canonical_root).admission_binding();
    let (_slot, owner) = execute_plan(&registry, &plan, binding)?;
    match registry.install(&key, Some(owner)) {
        Ok(live) => Ok(live),
        // Not pending any more: either a concurrent opener installed between
        // our admit and install, or the admit above JOINED an occupancy that
        // is already live. Both resolve to the same live slot.
        Err(RegistryRefusal::NotAdmitted) => registry.live(&key).map_err(AdapterRefusal::Registry),
        Err(refusal) => Err(AdapterRefusal::Registry(refusal)),
    }
}

/// Feature 020 V11, T066 — activation machine oracles (in-crate per the
/// discharged fixture-door precondition; see `runtime.rs`).
#[cfg(all(test, feature = "server"))]
mod activation_oracles {
    use super::{ActivationCut, ActivationMode, ActivationRefusal, RegisteredLane};

    fn register_all(machine: &ActivationCut) -> Vec<super::LaneRegistration> {
        RegisteredLane::ALL
            .iter()
            .map(|lane| machine.register_lane(*lane).expect("fresh lane registers"))
            .collect()
    }

    #[test]
    fn starts_legacy_open_and_advances_only_forward() {
        let machine = ActivationCut::fresh_for_oracles();
        assert_eq!(machine.mode(), ActivationMode::LegacyOpen);

        let registrations = register_all(&machine);
        machine.begin_closing().expect("all lanes registered");
        assert_eq!(machine.mode(), ActivationMode::LegacyClosing);

        for registration in registrations {
            machine
                .confirm_drained(registration)
                .expect("drain confirms in the drain window");
        }
        machine.open_preventive().expect("all lanes drained");
        assert_eq!(machine.mode(), ActivationMode::PreventiveV1Open);

        // Monotonic: nothing re-enters an earlier mode. Registration and a
        // second closing both refuse with the mode they found.
        let refusal = machine
            .register_lane(RegisteredLane::Hook)
            .expect_err("registration after the cut refuses");
        assert_eq!(
            refusal,
            ActivationRefusal::WrongMode {
                expected: ActivationMode::LegacyOpen,
                actual: ActivationMode::PreventiveV1Open,
            }
        );
        assert_eq!(
            machine.begin_closing(),
            Err(ActivationRefusal::WrongMode {
                expected: ActivationMode::LegacyOpen,
                actual: ActivationMode::PreventiveV1Open,
            })
        );
        assert_eq!(
            machine.open_preventive(),
            Err(ActivationRefusal::WrongMode {
                expected: ActivationMode::LegacyClosing,
                actual: ActivationMode::PreventiveV1Open,
            })
        );
    }

    #[test]
    fn closing_refuses_until_every_lane_is_registered() {
        let machine = ActivationCut::fresh_for_oracles();
        for lane in &RegisteredLane::ALL[..8] {
            machine.register_lane(*lane).expect("register");
        }
        // The refusal NAMES the missing lane — an unregistered lane is the
        // unmatched ingress SC-001 forbids, not a count.
        assert_eq!(
            machine.begin_closing(),
            Err(ActivationRefusal::LanesUnregistered(vec![
                RegisteredLane::Finalization
            ]))
        );
        assert_eq!(machine.mode(), ActivationMode::LegacyOpen);

        // Positive control: registering the ninth lane makes the same call
        // succeed, so the refusal above was about the gap, not the gate.
        machine
            .register_lane(RegisteredLane::Finalization)
            .expect("register the ninth");
        machine.begin_closing().expect("now complete");
        assert_eq!(machine.mode(), ActivationMode::LegacyClosing);
    }

    #[test]
    fn preventive_refuses_until_every_lane_confirms_drain() {
        let machine = ActivationCut::fresh_for_oracles();
        let mut registrations = register_all(&machine);
        machine.begin_closing().expect("registered");

        let held_back = registrations.pop().expect("nine registrations");
        let held_lane = held_back.lane();
        for registration in registrations {
            machine.confirm_drained(registration).expect("drain");
        }
        assert_eq!(
            machine.open_preventive(),
            Err(ActivationRefusal::LanesUndrained(vec![held_lane]))
        );
        assert_eq!(machine.mode(), ActivationMode::LegacyClosing);

        // Positive control: the last confirmation opens the door.
        machine.confirm_drained(held_back).expect("last drain");
        machine.open_preventive().expect("all drained");
        assert_eq!(machine.mode(), ActivationMode::PreventiveV1Open);
    }

    #[test]
    fn registration_is_once_and_machine_bound() {
        let machine_a = ActivationCut::fresh_for_oracles();
        let machine_b = ActivationCut::fresh_for_oracles();

        let token = machine_a
            .register_lane(RegisteredLane::Cache)
            .expect("first registration");
        let refusal = machine_a
            .register_lane(RegisteredLane::Cache)
            .expect_err("a second registration of the same lane refuses");
        assert_eq!(
            refusal,
            ActivationRefusal::LaneAlreadyRegistered(RegisteredLane::Cache)
        );

        // A token minted by machine A carries A's identity; machine B
        // refuses it even in the right window.
        for lane in RegisteredLane::ALL {
            machine_b.register_lane(lane).expect("register on b");
        }
        machine_b.begin_closing().expect("b closes");
        assert_eq!(
            machine_b.confirm_drained(token),
            Err(ActivationRefusal::ForeignRegistration)
        );
    }

    #[test]
    fn premature_drain_confirmation_refuses_and_strands() {
        let machine = ActivationCut::fresh_for_oracles();
        let token = machine
            .register_lane(RegisteredLane::Sidecar)
            .expect("register");
        // Confirming before the drain window consumes the token and refuses.
        assert_eq!(
            machine.confirm_drained(token),
            Err(ActivationRefusal::WrongMode {
                expected: ActivationMode::LegacyClosing,
                actual: ActivationMode::LegacyOpen,
            })
        );
        // The strand is deliberate: the machine can now never open, because
        // the sidecar lane's only token is spent. Every OTHER lane registers
        // and drains normally, so the final refusal names exactly the
        // stranded lane. A mis-sequenced bootstrap fails loudly here rather
        // than half-opening.
        let others: Vec<super::LaneRegistration> = RegisteredLane::ALL
            .iter()
            .copied()
            .filter(|lane| *lane != RegisteredLane::Sidecar)
            .map(|lane| machine.register_lane(lane).expect("register"))
            .collect();
        machine.begin_closing().expect("registration is complete");
        for registration in others {
            machine.confirm_drained(registration).expect("drain");
        }
        assert_eq!(
            machine.open_preventive(),
            Err(ActivationRefusal::LanesUndrained(vec![
                RegisteredLane::Sidecar
            ]))
        );
    }

    #[test]
    fn process_machine_is_one_per_process_and_never_precedes_legacy_open() {
        let first = ActivationCut::process();
        let second = ActivationCut::process();
        assert!(
            std::ptr::eq(first, second),
            "process() returns the one process-wide machine"
        );
        // Since C4b the production bootstrap (`activate_surface`) drives the
        // process machine to `PreventiveV1Open`, and any daemon/serve test
        // that ran earlier in this serial binary has legitimately done so.
        // The START mode is pinned on a fresh machine by
        // `starts_legacy_open_and_advances_only_forward`; the singleton claim
        // left to pin here is monotonicity's floor: the machine is never in a
        // state outside the closed three-mode set, and if the ceremony ran it
        // is exactly `PreventiveV1Open`, never half-open.
        assert!(
            matches!(
                first.mode(),
                ActivationMode::LegacyOpen | ActivationMode::PreventiveV1Open
            ),
            "the process machine is never observed mid-drain outside the ceremony"
        );
    }
}

/// Feature 020 V11, T064 — write-lane bridge oracles.
#[cfg(all(test, feature = "server"))]
mod write_authority_oracles {
    use super::ProjectSourceAuthority;
    use crate::live_index::index_lifecycle::authority::AuthorityRefusal;
    use crate::live_index::index_lifecycle::observer::{CutKind, LatchCause};

    fn a_temp_root(tag: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "symforge-write-authority-{tag}-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).expect("create oracle root");
        root
    }

    #[test]
    fn write_cycle_publishes_non_current_then_returns_current_fresh() {
        let root = a_temp_root("cycle");
        let authority = ProjectSourceAuthority::for_root(&root);
        assert!(authority.is_current(), "the bridge seeds Current");
        let before = authority
            .current_publication()
            .expect("Current has a publication");

        let mut write = authority.acquire_write().expect("grant on Current");
        assert!(
            !authority.is_current(),
            "the grant itself published non-Current before any byte moved"
        );

        let receipt = write
            .write(std::path::Path::new("oracle.txt"), b"written under permit")
            .expect("confined write beneath the leased root");
        write.finish_committed(receipt).expect("commit and return");

        assert!(authority.is_current(), "the source returned to Current");
        let after = authority
            .current_publication()
            .expect("Current has a publication");
        assert_ne!(
            before, after,
            "return is through a FRESH publication, never a restore"
        );
        assert_eq!(
            std::fs::read(root.join("oracle.txt")).expect("the write landed"),
            b"written under permit",
            "the permit's lease wrote the actual bytes"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn one_outstanding_write_authority_at_a_time() {
        let root = a_temp_root("single");
        let authority = ProjectSourceAuthority::for_root(&root);

        let write = authority.acquire_write().expect("first grant");
        let refusal = authority
            .acquire_write()
            .expect_err("a second grant while one is outstanding refuses");
        assert!(
            matches!(refusal, AuthorityRefusal::PhaseNotCurrent { .. }),
            "the refusal names the non-Current phase, got {refusal:?}"
        );

        // Positive control: finishing the first restores grantability.
        write
            .finish_no_side_effect()
            .expect("an unstarted permit attests no side effect");
        authority
            .acquire_write()
            .expect("after the return to Current a fresh grant succeeds");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn no_side_effect_refuses_once_a_side_effect_began() {
        let root = a_temp_root("nse");
        let authority = ProjectSourceAuthority::for_root(&root);

        let mut write = authority.acquire_write().expect("grant");
        write
            .write(std::path::Path::new("touched.txt"), b"bytes")
            .expect("write");
        let refusal = write
            .finish_no_side_effect()
            .expect_err("a begun side effect cannot be attested away");
        assert!(
            matches!(refusal, AuthorityRefusal::SideEffectAlreadyInFlight),
            "the refusal names the in-flight side effect, got {refusal:?}"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn no_side_effect_return_is_also_a_fresh_publication() {
        let root = a_temp_root("nse-fresh");
        let authority = ProjectSourceAuthority::for_root(&root);
        let before = authority.current_publication().expect("publication");

        let write = authority.acquire_write().expect("grant");
        write
            .finish_no_side_effect()
            .expect("unstarted permit attests");
        let after = authority.current_publication().expect("publication");
        assert_ne!(
            before, after,
            "FR-043: even a proven no-op returns through a fresh publication"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn dropped_write_authority_recovers_current_through_a_scope_dirty_baseline() {
        let root = a_temp_root("dropped");
        let authority = ProjectSourceAuthority::for_root(&root);
        let before = authority.current_publication().expect("publication");

        let write = authority.acquire_write().expect("grant");
        drop(write);

        // The C3 re-scout recovery lane: the drained permit's write state is
        // unobserved, so the return is through a FRESH publication whose
        // consumed observation cut is a FULL baseline latched ScopeDirty —
        // never a silent restore, and no longer a strand.
        assert!(
            authority.is_current(),
            "recovery returns the source to Current"
        );
        let after = authority.current_publication().expect("publication");
        assert_ne!(
            before, after,
            "recovery is a fresh publication, never a restore"
        );
        match authority
            .last_reconcile_cut()
            .expect("recovery consumed a cut")
            .kind
        {
            CutKind::FullBaseline {
                cause: LatchCause::ScopeDirty,
            } => {}
            other => panic!("recovery must consume a scope-dirty full baseline, got {other:?}"),
        }
        authority
            .acquire_write()
            .expect("a recovered source grants again");
        std::fs::remove_dir_all(&root).ok();
    }
}

/// Feature 020 V11, T029 — observation-lane oracles (C3). The drop-recovery
/// inversion above is this commit's observed RED (the C2 stranding oracle
/// failed against the new expectation before the recovery lane existed);
/// these pin the lane's admission, incarnation, and gap semantics.
#[cfg(all(test, feature = "server"))]
mod observation_lane_oracles {
    use super::ProjectSourceAuthority;
    use crate::live_index::index_lifecycle::observer::{CutKind, LatchCause};

    fn a_temp_root(tag: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "symforge-observation-lane-{tag}-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).expect("create oracle root");
        root
    }

    fn consume_cut(authority: &std::sync::Arc<ProjectSourceAuthority>) -> CutKind {
        let write = authority.acquire_write().expect("grant");
        write.finish_no_side_effect().expect("no-op return");
        authority
            .last_reconcile_cut()
            .expect("the return consumed a cut")
            .kind
    }

    #[test]
    fn admissions_drive_the_candidate_pipeline_and_coalesce_into_one_cut() {
        let root = a_temp_root("admission");
        let authority = ProjectSourceAuthority::for_root(&root);
        assert_eq!(authority.committed_observations("src/a.rs"), 0);

        authority.observe_admission_active("src/a.rs");
        authority.observe_admission_active("src/a.rs");

        assert_eq!(
            authority.committed_observations("src/a.rs"),
            2,
            "each admission reaches the single commit point (new membership, \
             then an exact-validated delta over it)"
        );
        match consume_cut(&authority) {
            CutKind::Incremental { invalidations } => {
                assert_eq!(
                    invalidations.len(),
                    1,
                    "repeated observations of one source coalesce"
                );
            }
            other => panic!("clean admissions produce an incremental cut, got {other:?}"),
        }
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_new_incarnation_refuses_late_callbacks_and_forces_the_barrier_baseline() {
        let root = a_temp_root("incarnation");
        let authority = ProjectSourceAuthority::for_root(&root);
        let stale = authority.active_observer();

        let fresh = authority.register_observer();
        assert_ne!(stale, fresh);

        // The predecessor's late callback is unreachable: every observation
        // entry refuses the stale id and names the active incarnation.
        assert_eq!(authority.observe_admission(stale, "src/a.rs"), Err(fresh));
        assert_eq!(authority.observe_removal(stale, "src/a.rs"), Err(fresh));
        assert_eq!(authority.report_gap(stale), Err(fresh));
        assert_eq!(
            authority.committed_observations("src/a.rs"),
            0,
            "a refused observation commits nothing"
        );

        // The successor's first consumed cut is the post-barrier baseline.
        match consume_cut(&authority) {
            CutKind::FullBaseline {
                cause: LatchCause::HandoffBarrier,
            } => {}
            other => panic!("the handoff obligation is a full baseline, got {other:?}"),
        }
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_reported_gap_forces_the_full_baseline_over_later_admissions() {
        let root = a_temp_root("gap");
        let authority = ProjectSourceAuthority::for_root(&root);
        let observer = authority.active_observer();

        authority.report_gap(observer).expect("active incarnation");
        authority
            .observe_admission(observer, "src/a.rs")
            .expect("active incarnation");

        match consume_cut(&authority) {
            CutKind::FullBaseline {
                cause: LatchCause::Gap,
            } => {}
            other => panic!("the first latch cause wins the baseline, got {other:?}"),
        }
        std::fs::remove_dir_all(&root).ok();
    }
}
