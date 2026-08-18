//! Feature 020 V11 Slice 4 observer-handoff oracles (T054).
//!
//! RED-first for the pair T054+T061: authored and observed failing against
//! `todo!()` seams before any machinery existed. Every rejection case asserts
//! the accepting path in the same test — an accumulator that baselines
//! everything satisfies a lone latch assertion perfectly.

use std::collections::BTreeMap;
use std::panic::{AssertUnwindSafe, catch_unwind};

use symforge::live_index::index_lifecycle::observer::{
    CoalescingAccumulator, CutKind, HandoffRefusal, LatchCause, ObserverId, ObserverSlot,
};

// ── the frozen-named oracle ────────────────────────────────────────────────

/// TEST-OBSERVER (T054). The name is pinned by
/// `contracts/lifecycle-oracle-traceability-v11.md` as a `planned_exact`
/// target; do not rename it without amending that contract.
///
/// Cut tokens are STRICTLY monotonic — gap or no gap — and EVERY reported
/// gap is latched into a full-baseline cut: no gap can slip between cuts as
/// an incremental. The positive control proves ungapped cuts stay
/// incremental, so the latch is a decision, not a default.
#[test]
fn stable_token_cut_latches_every_gap() {
    let mut accumulator = CoalescingAccumulator::new(8);

    // Ungapped: incremental, and the token advances.
    accumulator.observe(1, || 100);
    let first = accumulator.cut();
    assert_eq!(first.kind, CutKind::Incremental);
    assert_eq!(first.invalidations, BTreeMap::from([(1, 100)]));

    // A gap: the very next cut is a FULL baseline, token still advancing.
    accumulator.observe(2, || 200);
    accumulator.report_gap();
    let gapped = accumulator.cut();
    assert!(
        gapped.token > first.token,
        "cut tokens must be strictly monotonic"
    );
    assert_eq!(
        gapped.kind,
        CutKind::FullBaseline {
            cause: LatchCause::Gap
        },
        "a reported gap must latch into a full baseline"
    );

    // A second gap latches AGAIN — no gap escapes, ever.
    accumulator.report_gap();
    let regapped = accumulator.cut();
    assert!(regapped.token > gapped.token);
    assert_eq!(
        regapped.kind,
        CutKind::FullBaseline {
            cause: LatchCause::Gap
        }
    );

    // And after the baselines, the stream is incremental again.
    accumulator.observe(3, || 300);
    let recovered = accumulator.cut();
    assert!(recovered.token > regapped.token);
    assert_eq!(recovered.kind, CutKind::Incremental);
}

// ── coalescing and the bound ───────────────────────────────────────────────

/// Repeated observations of one source coalesce to the NEWEST stamp, and
/// growth beyond the bound is a latched safety transition — never a silent
/// drop.
#[test]
fn coalescing_is_bounded_and_exhaustion_latches_not_drops() {
    let mut accumulator = CoalescingAccumulator::new(2);

    // Coalescing: three observations of one source are ONE invalidation,
    // carrying the newest stamp.
    accumulator.observe(1, || 100);
    accumulator.observe(1, || 101);
    accumulator.observe(1, || 102);
    assert_eq!(
        accumulator.pending(),
        1,
        "same-source observations must coalesce"
    );
    let cut = accumulator.cut();
    assert_eq!(cut.invalidations, BTreeMap::from([(1, 102)]));

    // Exhaustion: a third DISTINCT source over a bound of two latches the
    // safety transition, and the next cut is the retaining full baseline.
    accumulator.observe(1, || 110);
    accumulator.observe(2, || 120);
    accumulator.observe(3, || 130);
    let exhausted = accumulator.cut();
    assert_eq!(
        exhausted.kind,
        CutKind::FullBaseline {
            cause: LatchCause::CapacityExhausted
        },
        "overflow must latch, and the baseline is the retention mechanism"
    );
}

/// Cuts drain the accumulator: what one cut carried, the next must not.
#[test]
fn cuts_are_monotonic_and_drain_the_accumulator() {
    let mut accumulator = CoalescingAccumulator::new(8);
    accumulator.observe(1, || 100);
    accumulator.observe(2, || 200);

    let first = accumulator.cut();
    assert_eq!(first.invalidations.len(), 2);
    assert_eq!(accumulator.pending(), 0, "a cut must drain the accumulator");

    accumulator.observe(3, || 300);
    let second = accumulator.cut();
    assert!(second.token > first.token);
    assert_eq!(
        second.invalidations,
        BTreeMap::from([(3, 300)]),
        "a cut must never re-carry drained invalidations"
    );
}

// ── unwind retention ───────────────────────────────────────────────────────

/// An observation whose payload derivation UNWINDS is retained: the change
/// is known even though its content is not, so the scope latches dirty and
/// the next cut is a full baseline. Nothing observed is ever lost to an
/// unwind.
#[test]
fn ingress_unwind_retains_the_observation() {
    let mut accumulator = CoalescingAccumulator::new(8);

    let unwound = catch_unwind(AssertUnwindSafe(|| {
        accumulator.observe(7, || panic!("injected derivation unwind"));
    }));
    assert!(unwound.is_err(), "the injected unwind must actually unwind");

    let cut = accumulator.cut();
    assert_eq!(
        cut.kind,
        CutKind::FullBaseline {
            cause: LatchCause::ScopeDirty
        },
        "an unwound observation must be retained as a dirty-scope baseline"
    );

    // Positive control: a healthy derivation stays incremental.
    accumulator.observe(7, || 700);
    assert_eq!(accumulator.cut().kind, CutKind::Incremental);
}

// ── handoff ────────────────────────────────────────────────────────────────

/// The predecessor DRAINS before the successor activates: completion
/// refuses while undelivered cuts remain, drains deliver in order, and the
/// successor's first obligation is a full post-barrier baseline.
#[test]
fn predecessor_drains_before_the_successor_activates() {
    let mut slot = ObserverSlot::new(ObserverId(1));
    assert_eq!(slot.active(), ObserverId(1));

    let mut accumulator = CoalescingAccumulator::new(8);
    accumulator.observe(1, || 100);
    let pending_cut = accumulator.cut();
    let pending_token = pending_cut.token;
    slot.queue_undelivered(pending_cut);

    slot.begin_handoff();
    assert_eq!(
        slot.complete_handoff(ObserverId(2)).unwrap_err(),
        HandoffRefusal::PredecessorStillDraining { undelivered: 1 },
        "completion must refuse while the predecessor still holds cuts"
    );
    assert_eq!(
        slot.active(),
        ObserverId(1),
        "the predecessor holds the slot until drained"
    );

    let delivered = slot.deliver_pending();
    assert_eq!(delivered.len(), 1);
    assert_eq!(delivered[0].token, pending_token);

    let baseline = slot
        .complete_handoff(ObserverId(2))
        .expect("a drained predecessor hands off");
    assert_eq!(
        baseline,
        CutKind::FullBaseline {
            cause: LatchCause::HandoffBarrier
        },
        "the successor's first obligation is the post-barrier baseline"
    );
    assert_eq!(slot.active(), ObserverId(2));
}

/// Completing a handoff that never began refuses — the slot is stable, not
/// stealable.
#[test]
fn the_slot_cannot_be_stolen_without_a_handoff() {
    let mut slot = ObserverSlot::new(ObserverId(1));
    assert_eq!(
        slot.complete_handoff(ObserverId(9)).unwrap_err(),
        HandoffRefusal::NoHandoffInProgress
    );
    assert_eq!(slot.active(), ObserverId(1));

    // Positive control: the legitimate protocol still works.
    slot.begin_handoff();
    slot.deliver_pending();
    slot.complete_handoff(ObserverId(9))
        .expect("an empty predecessor hands off immediately");
    assert_eq!(slot.active(), ObserverId(9));
}
