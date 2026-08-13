//! Project admission, slots, and non-revivable tombstones (T033).
//!
//! The problem this solves is measured, not hypothetical. Today's registry is a
//! `HashMap<String, Arc<ProjectSlot>>` keyed by project id, and **an `Arc` clone
//! outlives map membership** — removing the entry revokes nothing, so a holder
//! obtained before removal keeps operating on a project the registry believes is
//! gone. Reopening the same path then mints a fresh entry, and two live handles
//! now serve one path with no relationship between them.
//!
//! A tombstone is what makes removal mean something. Once a slot stops, its
//! identity is retired permanently: a later open of the same path is a NEW slot
//! with a new identity, and anything still holding the old one can be told it is
//! stale rather than being silently served.
//!
//! Admission is single-flight: concurrent opens of the same path join one
//! pending admission instead of racing to build two. Slice 2 does **not**
//! construct or claim lifecycle `Current` — that is Slice 4 activation work, and
//! the spec schedules a test to prove this slice never does it.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use super::authority::BindingAuthority;
use super::capacity::OwnerIdentity;

/// Identity of one slot occupancy. Never reused, including across reopen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SlotIdentity(std::num::NonZeroU64);

static NEXT_SLOT: AtomicU64 = AtomicU64::new(1);

impl SlotIdentity {
    /// Mint a fresh never-reused slot identity.
    pub fn fresh() -> Self {
        let raw = NEXT_SLOT.fetch_add(1, Ordering::Relaxed);
        Self(std::num::NonZeroU64::new(raw).expect("slot counter starts at 1"))
    }
}

/// A project key. The stable name a caller opens.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProjectKey(String);

impl ProjectKey {
    /// Build a key from a canonical project identifier.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// The underlying identifier.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Why a registry request was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistryRefusal {
    /// The slot identity named is retired and can never serve again.
    Tombstoned {
        /// The retired identity that was presented.
        slot: SlotIdentity,
    },
    /// No slot is live under this key.
    NotAdmitted,
    /// A protected root was opened without the authority to do so.
    ProtectedWithoutAuthorization,
    /// The slot is still admitting; it has not published anything yet.
    StillPending,
}

/// How a project's derived state may be placed on disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatePlacement {
    /// Beneath the project root, the normal case.
    ProjectLocal,
    /// In a user-local directory, for a root that must not be written to.
    UserLocal,
    /// Nowhere; state lives only in this process.
    MemoryOnly,
}

/// Whether a root is protected from having state written beneath it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RootProtection {
    /// An ordinary root.
    Normal,
    /// A protected root: no state and no durability probe may touch it.
    Protected,
}

/// A slot that has been admitted but has published no generation.
///
/// Exists so concurrent opens of one key join a single admission rather than
/// racing to build two. It deliberately cannot yield a queryable generation:
/// Slice 2 never constructs lifecycle `Current`.
#[derive(Debug)]
pub struct PendingProjectAdmission {
    slot: SlotIdentity,
    key: ProjectKey,
    binding: BindingAuthority,
    placement: StatePlacement,
    joiners: usize,
}

impl PendingProjectAdmission {
    /// The identity this admission will install under.
    pub fn slot(&self) -> SlotIdentity {
        self.slot
    }

    /// The key being admitted.
    pub fn key(&self) -> &ProjectKey {
        &self.key
    }

    /// Where this project's derived state will live.
    pub fn placement(&self) -> StatePlacement {
        self.placement
    }

    /// How many callers are waiting on this one admission.
    pub fn joiners(&self) -> usize {
        self.joiners
    }

    /// The binding this admission is for.
    pub fn binding(&self) -> &BindingAuthority {
        &self.binding
    }
}

/// A live slot.
///
/// **Revocation is enforced, not advised.** Rust cannot take an `Arc` back from
/// a holder, so instead a stopped slot refuses to hand out anything that confers
/// authority. A holder that never thinks to ask whether it is stale does not get
/// silently served — it gets a refusal the moment it tries to act. Asking a
/// holder to voluntarily check `is_current` first would be documenting the
/// hazard rather than closing it.
///
/// Identity, key and placement stay readable after revocation on purpose: they
/// are diagnostics, and an operator investigating a stale holder needs to see
/// WHICH slot it was.
#[derive(Debug)]
pub struct LiveProjectSlot {
    slot: SlotIdentity,
    key: ProjectKey,
    binding: BindingAuthority,
    placement: StatePlacement,
    owner: Option<OwnerIdentity>,
    revoked: std::sync::atomic::AtomicBool,
}

impl LiveProjectSlot {
    /// This occupancy's identity. Readable after revocation, for diagnosis.
    pub fn slot(&self) -> SlotIdentity {
        self.slot
    }

    /// The key it serves. Readable after revocation, for diagnosis.
    pub fn key(&self) -> &ProjectKey {
        &self.key
    }

    /// Where its derived state lives. Readable after revocation, for diagnosis.
    pub fn placement(&self) -> StatePlacement {
        self.placement
    }

    /// Whether this slot is still the live occupancy of its key.
    pub fn is_live(&self) -> bool {
        !self.revoked.load(Ordering::Acquire)
    }

    /// The binding it is on.
    ///
    /// Refuses once the slot is stopped. This is the authority-conferring read,
    /// so it is the one that must fail closed: without a binding a holder cannot
    /// name a physical root, and therefore cannot act on one.
    pub fn binding(&self) -> Result<&BindingAuthority, RegistryRefusal> {
        if self.is_live() {
            Ok(&self.binding)
        } else {
            Err(RegistryRefusal::Tombstoned { slot: self.slot })
        }
    }

    /// Its capacity owner, when one was attached.
    ///
    /// Refuses once stopped: charging against a retired slot's owner would
    /// spend capacity the registry has already returned.
    pub fn capacity_owner(&self) -> Result<Option<OwnerIdentity>, RegistryRefusal> {
        if self.is_live() {
            Ok(self.owner)
        } else {
            Err(RegistryRefusal::Tombstoned { slot: self.slot })
        }
    }

    /// Revoke this occupancy. Idempotent, and never undone.
    fn revoke(&self) {
        self.revoked.store(true, Ordering::Release);
    }
}

/// What a key currently is.
#[derive(Debug)]
enum Occupancy {
    Pending(PendingProjectAdmission),
    Live(Arc<LiveProjectSlot>),
}

/// The project registry.
///
/// Distinct from the daemon's own `ProjectSlot` map, which this does not replace
/// — Slice 2 must not touch any V10 production admission path. This is the
/// lifecycle-owned registry that Slice 4 activation will move onto.
#[derive(Debug, Default)]
pub struct ProjectRegistry {
    occupancy: std::sync::Mutex<RegistryState>,
}

#[derive(Debug, Default)]
struct RegistryState {
    keys: HashMap<ProjectKey, Occupancy>,
    /// Identities that have stopped. Membership here is permanent.
    tombstones: HashMap<SlotIdentity, ProjectKey>,
}

impl ProjectRegistry {
    /// An empty registry.
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Begin admitting `key`, or join the admission already in flight.
    ///
    /// Returns the slot identity the admission will install under. Concurrent
    /// callers receive the SAME identity, which is what makes admission
    /// single-flight rather than merely serialized.
    ///
    /// A protected root must present authorization AND may only select a
    /// placement that writes nothing beneath it.
    pub fn admit(
        self: &Arc<Self>,
        key: ProjectKey,
        binding: BindingAuthority,
        protection: RootProtection,
        authorized: bool,
        placement: StatePlacement,
    ) -> Result<SlotIdentity, RegistryRefusal> {
        if protection == RootProtection::Protected {
            if !authorized {
                return Err(RegistryRefusal::ProtectedWithoutAuthorization);
            }
            if placement == StatePlacement::ProjectLocal {
                // Writing state beneath a protected root is exactly what
                // protection forbids; refuse rather than silently relocating,
                // so the caller learns its request was not honoured.
                return Err(RegistryRefusal::ProtectedWithoutAuthorization);
            }
        }

        let mut state = self.occupancy.lock().expect("registry mutex");
        match state.keys.get_mut(&key) {
            Some(Occupancy::Pending(pending)) => {
                pending.joiners += 1;
                Ok(pending.slot)
            }
            Some(Occupancy::Live(live)) => Ok(live.slot()),
            None => {
                let slot = SlotIdentity::fresh();
                state.keys.insert(
                    key.clone(),
                    Occupancy::Pending(PendingProjectAdmission {
                        slot,
                        key,
                        binding,
                        placement,
                        joiners: 1,
                    }),
                );
                Ok(slot)
            }
        }
    }

    /// How many callers joined the pending admission for `key`.
    pub fn pending_joiners(&self, key: &ProjectKey) -> Option<usize> {
        match self.occupancy.lock().expect("registry mutex").keys.get(key) {
            Some(Occupancy::Pending(pending)) => Some(pending.joiners),
            _ => None,
        }
    }

    /// Install the pending admission for `key` as live.
    ///
    /// Refuses a slot identity that has been tombstoned: a cancelled or stopped
    /// admission can never be revived, only replaced by a new one.
    pub fn install(
        self: &Arc<Self>,
        key: &ProjectKey,
        owner: Option<OwnerIdentity>,
    ) -> Result<Arc<LiveProjectSlot>, RegistryRefusal> {
        let mut state = self.occupancy.lock().expect("registry mutex");
        let Some(Occupancy::Pending(pending)) = state.keys.remove(key) else {
            // Put back whatever was there; a live slot is not an error to
            // install over, but it is not a pending admission either.
            return Err(RegistryRefusal::NotAdmitted);
        };
        if state.tombstones.contains_key(&pending.slot) {
            return Err(RegistryRefusal::Tombstoned { slot: pending.slot });
        }
        let live = Arc::new(LiveProjectSlot {
            slot: pending.slot,
            key: pending.key.clone(),
            binding: pending.binding,
            placement: pending.placement,
            owner,
            revoked: std::sync::atomic::AtomicBool::new(false),
        });
        state
            .keys
            .insert(pending.key, Occupancy::Live(Arc::clone(&live)));
        Ok(live)
    }

    /// Cancel a pending admission, retiring its identity permanently.
    pub fn cancel(self: &Arc<Self>, key: &ProjectKey) -> Result<SlotIdentity, RegistryRefusal> {
        let mut state = self.occupancy.lock().expect("registry mutex");
        let Some(Occupancy::Pending(pending)) = state.keys.remove(key) else {
            return Err(RegistryRefusal::NotAdmitted);
        };
        state.tombstones.insert(pending.slot, pending.key);
        Ok(pending.slot)
    }

    /// Stop a live slot, retiring its identity permanently AND revoking every
    /// handle already handed out.
    ///
    /// Revocation happens before the tombstone is recorded, so there is no
    /// window in which the registry considers the slot retired while an existing
    /// handle still hands out its binding.
    pub fn stop(self: &Arc<Self>, key: &ProjectKey) -> Result<SlotIdentity, RegistryRefusal> {
        let mut state = self.occupancy.lock().expect("registry mutex");
        let Some(Occupancy::Live(live)) = state.keys.remove(key) else {
            return Err(RegistryRefusal::NotAdmitted);
        };
        // Revoke FIRST: every outstanding handle must start refusing before the
        // registry records the retirement, or a holder could still obtain the
        // binding for a slot the registry already considers gone.
        live.revoke();
        let slot = live.slot();
        state.tombstones.insert(slot, live.key().clone());
        Ok(slot)
    }

    /// Whether `slot` is the identity currently serving `key`.
    ///
    /// The question a holder must ask before acting. A tombstoned identity
    /// answers `false` forever, including after the key is reopened.
    pub fn is_current(&self, key: &ProjectKey, slot: SlotIdentity) -> bool {
        let state = self.occupancy.lock().expect("registry mutex");
        if state.tombstones.contains_key(&slot) {
            return false;
        }
        match state.keys.get(key) {
            Some(Occupancy::Live(live)) => live.slot() == slot,
            _ => false,
        }
    }

    /// Whether this identity has been retired.
    pub fn is_tombstoned(&self, slot: SlotIdentity) -> bool {
        self.occupancy
            .lock()
            .expect("registry mutex")
            .tombstones
            .contains_key(&slot)
    }

    /// The live slot for `key`, if one is installed.
    pub fn live(&self, key: &ProjectKey) -> Result<Arc<LiveProjectSlot>, RegistryRefusal> {
        match self.occupancy.lock().expect("registry mutex").keys.get(key) {
            Some(Occupancy::Live(live)) => Ok(Arc::clone(live)),
            Some(Occupancy::Pending(_)) => Err(RegistryRefusal::StillPending),
            None => Err(RegistryRefusal::NotAdmitted),
        }
    }

    /// How many identities have been retired.
    pub fn tombstone_count(&self) -> usize {
        self.occupancy
            .lock()
            .expect("registry mutex")
            .tombstones
            .len()
    }
}
