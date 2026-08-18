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
//! monotonic cut tokens, latch-clears-only-through-baseline, coalescing
//! with a hard bound, predecessor-drain-before-successor, post-barrier full
//! baseline, and unwind retention — are exact.
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

/// What one cut demands of its consumer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CutKind {
    /// Apply exactly these invalidations.
    Incremental,
    /// Rebuild the whole scope; the latch cause says why.
    FullBaseline { cause: LatchCause },
}

/// One monotonic invalidation cut.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObservationCut {
    pub token: CutToken,
    pub kind: CutKind,
    pub invalidations: BTreeMap<u64, u64>,
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
        let kind = match self.latch.take() {
            Some(cause) => CutKind::FullBaseline { cause },
            None => CutKind::Incremental,
        };
        ObservationCut {
            token,
            kind,
            invalidations: std::mem::take(&mut self.pending),
        }
    }
}

/// Why a handoff refused to complete.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HandoffRefusal {
    /// The predecessor still holds undelivered cuts.
    PredecessorStillDraining { undelivered: usize },
    /// No handoff has begun.
    NoHandoffInProgress,
}

/// Identity of one observer occupying the slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ObserverId(pub u64);

/// The stable observer slot: one active observer, drain-before-successor
/// handoff, and a post-barrier full baseline for the successor.
#[derive(Debug)]
pub struct ObserverSlot {
    active: ObserverId,
    undelivered: Vec<ObservationCut>,
    handoff_in_progress: bool,
}

impl ObserverSlot {
    pub fn new(first: ObserverId) -> Self {
        Self {
            active: first,
            undelivered: Vec::new(),
            handoff_in_progress: false,
        }
    }

    /// The observer currently holding the slot.
    pub fn active(&self) -> ObserverId {
        self.active
    }

    /// Queue a cut the active observer has produced but not yet delivered.
    pub fn queue_undelivered(&mut self, cut: ObservationCut) {
        self.undelivered.push(cut);
    }

    /// Begin handing the slot to a successor: the predecessor drains first.
    pub fn begin_handoff(&mut self) {
        self.handoff_in_progress = true;
    }

    /// Deliver every undelivered predecessor cut, in order.
    pub fn deliver_pending(&mut self) -> Vec<ObservationCut> {
        std::mem::take(&mut self.undelivered)
    }

    /// Complete the handoff. Refuses while the predecessor still holds
    /// undelivered cuts; on success the successor is active and its FIRST
    /// obligation is a full post-barrier baseline.
    pub fn complete_handoff(&mut self, successor: ObserverId) -> Result<CutKind, HandoffRefusal> {
        if !self.handoff_in_progress {
            return Err(HandoffRefusal::NoHandoffInProgress);
        }
        if !self.undelivered.is_empty() {
            return Err(HandoffRefusal::PredecessorStillDraining {
                undelivered: self.undelivered.len(),
            });
        }
        self.active = successor;
        self.handoff_in_progress = false;
        Ok(CutKind::FullBaseline {
            cause: LatchCause::HandoffBarrier,
        })
    }
}
