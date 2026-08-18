//! Feature 020 V11 observer accumulator and handoff (Slice 4, T061 — dark).
//!
//! The bounded coalescing accumulator, monotonic invalidation cuts,
//! scope-dirty/gap latches, stable observer handoff, and the full successor
//! baseline (frozen tasks.md T061). Every latch cause forces a FULL
//! baseline cut — a gap, a dirty scope, exhausted capacity, or a handoff
//! barrier is never a silent drop; the baseline is the retention mechanism.
//!
//! Dark payload simplifications, in the `runtime.rs` idiom: observations
//! are (source, stamp) pairs and stamp derivation is an injected closure so
//! oracles can drive unwinds; the live `src/watcher/` event vocabulary is
//! adapted at the cut (T064), not here — the darkness sweep forbids this
//! module's name in live files. The authority SEMANTICS — strictly
//! monotonic cut tokens, coalescing with a hard bound,
//! predecessor-drain-before-successor with a bound successor and
//! producer-fenced queueing, and unwind retention — are exact. Two deltas
//! are NOT and are recorded cut obligations (T064): the latch clears on cut
//! EMISSION, not on rescan PROOF (the acceptance oracle's
//! "remains latched until a complete rescan proof" needs the consumer
//! acknowledgment seam the live wiring owns — a consumer aborting a rescan
//! must re-latch via `report_gap`), and the successor's post-barrier
//! baseline obligation is returned as a value, not yet threaded into any
//! accumulator's cut stream. The `ObserverSlot`/`ObserverId` pair here is
//! the accumulator-side MECHANICS complement of
//! `authority.rs::ObserverPhase`/`ObserverToken` (the per-source
//! authority-side projection); unifying the two identities is T064 work.
//!
//! **Nothing in production calls this module.** Only the Slice 4 oracle
//! suites and this directory do; activation (T064/T066) is the only planned
//! production caller.

use std::collections::BTreeMap;

/// One cut's identity: strictly monotonic per accumulator, gap or no gap.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct CutToken(pub u64);

/// Why a cut was forced to a full baseline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LatchCause {
    /// The observation stream reported a gap (lost/overflowed events).
    Gap,
    /// An observation's payload derivation unwound: the CHANGE is known,
    /// its content is not — the scope is dirty.
    ScopeDirty,
    /// The bounded accumulator refused to grow: the safety transition.
    CapacityExhausted,
    /// A completed observer handoff: the successor starts from a barrier.
    HandoffBarrier,
}

/// What one cut demands of its consumer. The invalidations map lives
/// INSIDE `Incremental`: a baseline cut carrying a partial map is
/// unrepresentable, so no consumer can mistake an exhausted cut's residue
/// for a complete invalidation set.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CutKind {
    /// Apply exactly these invalidations.
    Incremental { invalidations: BTreeMap<u64, u64> },
    /// Rebuild the whole scope; the cause is DIAGNOSTIC only — every cause
    /// forces the same full baseline, and no consumer may branch on it for
    /// correctness (the first cause wins until the baseline clears it).
    FullBaseline { cause: LatchCause },
}

/// One monotonic invalidation cut.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObservationCut {
    pub token: CutToken,
    pub kind: CutKind,
}

/// The bounded coalescing accumulator (T061). Repeated observations of one
/// source coalesce to the newest stamp; distinct-source growth beyond the
/// bound latches `CapacityExhausted` instead of dropping. The FIRST latch
/// cause wins until a baseline cut clears it.
#[derive(Debug)]
pub struct CoalescingAccumulator {
    bound: usize,
    pending: BTreeMap<u64, u64>,
    latch: Option<LatchCause>,
    next_token: u64,
}

/// Armed across a payload derivation: if the derivation unwinds, the drop
/// latches the scope dirty — the observed CHANGE is retained even though
/// its content never arrived.
struct UnwindRetention<'a> {
    latch: &'a mut Option<LatchCause>,
    armed: bool,
}

impl Drop for UnwindRetention<'_> {
    fn drop(&mut self) {
        if self.armed {
            self.latch.get_or_insert(LatchCause::ScopeDirty);
        }
    }
}

impl CoalescingAccumulator {
    /// A fresh accumulator bounded to `bound` distinct pending sources.
    pub fn new(bound: usize) -> Self {
        Self {
            bound,
            pending: BTreeMap::new(),
            latch: None,
            next_token: 1,
        }
    }

    /// Observe `source` with a stamp computed by `derive`. If `derive`
    /// UNWINDS, the observation is retained: the scope latches dirty, so
    /// the change cannot be lost even though its content is unknown. A
    /// DISTINCT source beyond the bound latches the safety transition —
    /// the baseline cut is the retention mechanism, never a drop.
    pub fn observe(&mut self, source: u64, derive: impl FnOnce() -> u64) {
        if !self.pending.contains_key(&source) && self.pending.len() >= self.bound {
            self.latch.get_or_insert(LatchCause::CapacityExhausted);
            return;
        }
        let stamp = {
            let mut retention = UnwindRetention {
                latch: &mut self.latch,
                armed: true,
            };
            let stamp = derive();
            retention.armed = false;
            drop(retention);
            stamp
        };
        self.pending.insert(source, stamp);
    }

    /// The observation stream reported a gap: latch it.
    pub fn report_gap(&mut self) {
        self.latch.get_or_insert(LatchCause::Gap);
    }

    /// A completed observer handoff: the successor's first cut must be a
    /// full post-barrier baseline. This threads the value
    /// [`ObserverSlot::complete_handoff`] returns into the cut stream —
    /// the cut obligation this module's header recorded for T064.
    pub fn latch_handoff_barrier(&mut self) {
        self.latch.get_or_insert(LatchCause::HandoffBarrier);
    }

    /// An observed change whose content outcome is unknown (a stranded
    /// write authority, an aborted rescan): the scope is dirty. The same
    /// latch [`UnwindRetention`] arms across a payload derivation, exposed
    /// for the T064 recovery lane, which observes the dirtiness directly
    /// rather than through an unwind.
    pub fn latch_scope_dirty(&mut self) {
        self.latch.get_or_insert(LatchCause::ScopeDirty);
    }

    /// Pending distinct sources (coalesced).
    pub fn pending(&self) -> usize {
        self.pending.len()
    }

    /// Produce the next cut: strictly monotonic token, always. A latched
    /// accumulator emits a FULL baseline cut (clearing the latch); an
    /// unlatched one emits the coalesced incremental invalidations. Either
    /// way the accumulator drains.
    pub fn cut(&mut self) -> ObservationCut {
        let token = CutToken(self.next_token);
        self.next_token += 1;
        let pending = std::mem::take(&mut self.pending);
        let kind = match self.latch.take() {
            // The baseline SUBSUMES the drained pending set: carrying a
            // partial map here is unrepresentable by construction.
            Some(cause) => CutKind::FullBaseline { cause },
            None => CutKind::Incremental {
                invalidations: pending,
            },
        };
        ObservationCut { token, kind }
    }
}

/// Why a handoff or queue operation refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HandoffRefusal {
    /// The predecessor still holds undelivered cuts.
    PredecessorStillDraining { undelivered: usize },
    /// No handoff has begun.
    NoHandoffInProgress,
    /// A handoff is already in progress toward a bound successor.
    HandoffAlreadyInProgress { bound_successor: ObserverId },
    /// The producing observer is not the active one.
    NotTheActiveObserver { producer: ObserverId },
}

/// Identity of one observer occupying the slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ObserverId(pub u64);

/// The stable observer slot: one active observer, drain-before-successor
/// handoff, and a post-barrier full baseline for the successor.
#[derive(Debug)]
pub struct ObserverSlot {
    active: ObserverId,
    undelivered: Vec<(ObserverId, ObservationCut)>,
    pending_successor: Option<ObserverId>,
}

impl ObserverSlot {
    pub fn new(first: ObserverId) -> Self {
        Self {
            active: first,
            undelivered: Vec::new(),
            pending_successor: None,
        }
    }

    /// The observer currently holding the slot.
    pub fn active(&self) -> ObserverId {
        self.active
    }

    /// Queue a cut `producer` has produced but not yet delivered. Only the
    /// ACTIVE observer may queue: an old observer's late callback can never
    /// affect the successor's incarnation.
    pub fn queue_undelivered(
        &mut self,
        producer: ObserverId,
        cut: ObservationCut,
    ) -> Result<(), HandoffRefusal> {
        if producer != self.active {
            return Err(HandoffRefusal::NotTheActiveObserver { producer });
        }
        self.undelivered.push((producer, cut));
        Ok(())
    }

    /// Begin handing the slot to `successor`: the successor is BOUND here
    /// (completion cannot be redirected), the predecessor drains first, and
    /// a second begin refuses rather than silently rebinding. A self-handoff
    /// (successor == active) is a legitimate re-registration that still
    /// forces the post-barrier baseline.
    pub fn begin_handoff(&mut self, successor: ObserverId) -> Result<(), HandoffRefusal> {
        if let Some(bound_successor) = self.pending_successor {
            return Err(HandoffRefusal::HandoffAlreadyInProgress { bound_successor });
        }
        self.pending_successor = Some(successor);
        Ok(())
    }

    /// Deliver every undelivered predecessor cut, in order.
    pub fn deliver_pending(&mut self) -> Vec<ObservationCut> {
        std::mem::take(&mut self.undelivered)
            .into_iter()
            .map(|(_, cut)| cut)
            .collect()
    }

    /// Complete the handoff toward the successor BOUND at begin. Refuses
    /// while the predecessor still holds undelivered cuts; on success the
    /// successor is active and its FIRST obligation is a full post-barrier
    /// baseline.
    pub fn complete_handoff(&mut self) -> Result<CutKind, HandoffRefusal> {
        let Some(successor) = self.pending_successor else {
            return Err(HandoffRefusal::NoHandoffInProgress);
        };
        if !self.undelivered.is_empty() {
            return Err(HandoffRefusal::PredecessorStillDraining {
                undelivered: self.undelivered.len(),
            });
        }
        self.active = successor;
        self.pending_successor = None;
        Ok(CutKind::FullBaseline {
            cause: LatchCause::HandoffBarrier,
        })
    }
}
