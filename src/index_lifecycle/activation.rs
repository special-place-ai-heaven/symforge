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

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

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
