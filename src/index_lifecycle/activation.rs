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

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use super::authority::{
    AuthorityRefusal, BindingAuthority, CurrentPublication, MutationGrantInput, ObserverToken,
    SourceRuntime,
};
use super::mutation::{PermitDrainSignal, RefreshTicket, SourceMutationPermit};
use super::physical_root::{PhysicalRootLease, WriteReceipt};
use super::transition::{self, TransitionKind};
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

// ── The write-lane bridge (T064, C2) ───────────────────────────────────────

/// One project root's live source authority: the bridge the cut's writer
/// lanes acquire mutation permits from.
///
/// MID-CUT BRIDGING CLAIM, recorded: construction seeds the runtime
/// `Current` on a fresh publication because the V10 data plane it stands
/// beside serves queries today; C4 ties construction to observed bootstrap
/// state and C3 replaces the fresh [`ObserverToken`] minted at each
/// reconcile with the real accumulated observer cut. Neither simplification
/// weakens the write path itself: every write still requires a granted
/// permit, publishes non-`Current` first, and returns to `Current` only
/// through a fresh publication (FR-043).
#[derive(Debug)]
pub struct ProjectSourceAuthority {
    root: PathBuf,
    inner: Mutex<AuthorityInner>,
}

#[derive(Debug)]
struct AuthorityInner {
    runtime: SourceRuntime,
    lease: Arc<PhysicalRootLease>,
    outstanding: Arc<PermitDrainSignal>,
}

/// A granted, in-progress write on one project source. Non-Clone; ends by
/// [`WriteAuthority::finish_committed`], [`WriteAuthority::finish_no_side_effect`],
/// or drop (which drains the permit and leaves the source non-`Current` —
/// honest stranding until the C3 re-scout lane lands).
#[derive(Debug)]
pub struct WriteAuthority {
    authority: Arc<ProjectSourceAuthority>,
    permit: SourceMutationPermit,
    frozen_publication: PublicationIdentity,
}

impl ProjectSourceAuthority {
    /// The bridge for one project root. See the type doc for the recorded
    /// mid-cut construction claim.
    pub fn for_root(root: &Path) -> Arc<Self> {
        let lease = Arc::new(PhysicalRootLease::take(root));
        let binding = BindingAuthority::bind(lease.identity());
        let publication = CurrentPublication::promote(binding, ObserverToken::fresh());
        Arc::new(Self {
            root: root.to_path_buf(),
            inner: Mutex::new(AuthorityInner {
                runtime: SourceRuntime::current(publication),
                lease,
                // Unarmed: reports ended until a permit arms it, which is
                // what lets the first transition treat "no permit ever" as
                // drained rather than as an optional check.
                outstanding: Arc::new(PermitDrainSignal::new()),
            }),
        })
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
        let permit = SourceMutationPermit::grant(grant, inner.lease.clone(), drain.clone())?;
        inner.outstanding = drain;
        Ok(WriteAuthority {
            authority: self.clone(),
            permit,
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
        let mut guard = self.inner.lock().expect("project source authority lock");
        let inner = &mut *guard;
        inner.runtime.retire_permit(frozen_publication);
        let outstanding = inner.outstanding.clone();
        let new_lease = Arc::new(PhysicalRootLease::take(self.root.as_path()));
        let incoming = BindingAuthority::bind(new_lease.identity());
        let old_lease = inner.lease.clone();
        transition::apply(
            &mut inner.runtime,
            TransitionKind::Reload,
            &old_lease,
            incoming,
            // MID-CUT: a fresh token stands in for the accumulated observer
            // cut until C3 wires the real accumulator (execution map, seal
            // section).
            ObserverToken::fresh(),
            &outstanding,
        )?;
        inner.lease = new_lease;
        Ok(ticket)
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
        match self.permit.start_side_effect() {
            Ok(()) => {}
            // Continuing the batch: the side effect is already in flight.
            Err(AuthorityRefusal::SideEffectAlreadyInFlight) => {}
            Err(refusal) => return Err(refusal),
        }
        self.permit.replace_beneath(relative, contents)
    }

    /// Commit the observed side effect and return the source to `Current`
    /// through a fresh publication: retire the permit, then apply the sealed
    /// Freeze -> Drain -> Install transition under a fresh root lease and
    /// binding.
    pub fn finish_committed(
        self,
        receipt: WriteReceipt,
    ) -> Result<RefreshTicket, AuthorityRefusal> {
        let WriteAuthority {
            authority,
            mut permit,
            frozen_publication,
        } = self;
        let ticket = permit.commit(receipt)?;
        authority.reconcile_returned(frozen_publication, ticket)
    }

    /// Terminate with the permit's own observation that nothing was written
    /// through its lease (the permit never began a side effect), and return
    /// the source to `Current` through a fresh publication — FR-043 applies
    /// to no-ops too. Refuses once a side effect has begun.
    pub fn finish_no_side_effect(self) -> Result<RefreshTicket, AuthorityRefusal> {
        let WriteAuthority {
            authority,
            mut permit,
            frozen_publication,
        } = self;
        let ticket = permit.no_side_effect()?;
        authority.reconcile_returned(frozen_publication, ticket)
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
    fn process_machine_is_one_per_process_and_starts_legacy_open() {
        let first = ActivationCut::process();
        let second = ActivationCut::process();
        assert!(
            std::ptr::eq(first, second),
            "process() returns the one process-wide machine"
        );
        // No oracle in this module advances the process machine; the fresh
        // machines above keep scenario state out of the singleton.
        assert_eq!(first.mode(), ActivationMode::LegacyOpen);
    }
}

/// Feature 020 V11, T064 — write-lane bridge oracles.
#[cfg(all(test, feature = "server"))]
mod write_authority_oracles {
    use super::ProjectSourceAuthority;
    use crate::live_index::index_lifecycle::authority::AuthorityRefusal;

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
    fn dropped_write_authority_strands_non_current_until_re_scout() {
        let root = a_temp_root("dropped");
        let authority = ProjectSourceAuthority::for_root(&root);

        let write = authority.acquire_write().expect("grant");
        drop(write);

        // The drained permit's write state is unobserved, so the bridge must
        // NOT claim Current. Recovery is the C3 re-scout lane's obligation,
        // recorded in the execution map; until then this is honest stranding.
        assert!(
            !authority.is_current(),
            "a dropped permit must not restore Current without evidence"
        );
        let refusal = authority
            .acquire_write()
            .expect_err("a stranded source refuses new grants");
        assert!(matches!(refusal, AuthorityRefusal::PhaseNotCurrent { .. }));
        std::fs::remove_dir_all(&root).ok();
    }
}
