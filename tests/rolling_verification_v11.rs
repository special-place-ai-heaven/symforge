//! Feature 020 V11 Slice 4 rolling-verification oracles (T055).
//!
//! RED-first for the pair T055+T062: authored and observed failing against
//! `todo!()` seams before any machinery existed. Every rejection case asserts
//! the accepting path in the same test — a verifier that refuses everything
//! satisfies a lone negative perfectly.

use std::collections::BTreeMap;
use std::time::Duration;

use symforge::live_index::index_lifecycle::verification::{
    DEFAULT_MAX_COMPLETE_VERIFICATION_PASS, DEFAULT_MAX_VERIFICATION_BYTES,
    DEFAULT_MAX_VERIFICATION_ENTRIES, MAX_CURRENT_UNVERIFIED_AGE, MonotonicInstant,
    NonCurrentCause, PassScheduler, PolicyVersion, RESERVED_VERIFICATION_BYTES_PER_SECOND,
    RESERVED_VERIFICATION_ENTRIES_PER_SECOND, RollingVerifier, VerificationFeasibilityReceipt,
    VerificationRefusal, VerificationScopeReceipt, VerificationWorkBound,
};

// ── fixtures ───────────────────────────────────────────────────────────────

const POLICY: PolicyVersion = PolicyVersion(7);
const T0: MonotonicInstant = MonotonicInstant(1_000_000);

fn scope_entries(count: u64) -> BTreeMap<u64, u64> {
    (0..count).map(|id| (id, 100 + id)).collect()
}

fn sealed(count: u64) -> VerificationScopeReceipt {
    VerificationScopeReceipt::seal(POLICY, scope_entries(count))
}

fn feasibility_for(receipt: &VerificationScopeReceipt) -> VerificationFeasibilityReceipt {
    VerificationFeasibilityReceipt::reserve(
        VerificationWorkBound::for_scope(receipt, 1_024, 4).expect("tiny scope is affordable"),
    )
}

fn promoted(count: u64) -> RollingVerifier {
    let receipt = sealed(count);
    let feasibility = feasibility_for(&receipt);
    RollingVerifier::promote(receipt, feasibility, T0).expect("feasibility is bound to this scope")
}

fn just_before_deadline() -> MonotonicInstant {
    MonotonicInstant(T0.0 + u64::try_from(MAX_CURRENT_UNVERIFIED_AGE.as_millis()).unwrap() - 1)
}

fn at_deadline() -> MonotonicInstant {
    T0.plus(MAX_CURRENT_UNVERIFIED_AGE)
}

// ── the frozen-named oracle ────────────────────────────────────────────────

/// TEST-ROLLING-VERIFICATION (T055). The name is pinned by
/// `contracts/lifecycle-oracle-traceability-v11.md` as a `planned_exact`
/// target; do not rename it without amending that contract.
///
/// FAIR: concurrently rolling passes each make progress — the scheduler
/// rotates, none starves. RESUMABLE: a pass interrupted mid-scope continues
/// from its cursor and completes without re-verifying finished entries.
/// FENCED: a proof refresh seals a NEW receipt and a record bound to the old
/// one can never advance the deadline — the old proof is immutable, not
/// updated in place.
#[test]
fn rolling_passes_are_fair_resumable_and_fenced() {
    // Fair rotation over registered passes.
    let mut scheduler = PassScheduler::new();
    scheduler.register(1);
    scheduler.register(2);
    let grants: Vec<u64> = (0..4)
        .map(|_| scheduler.next_grant().expect("passes queued"))
        .collect();
    assert_eq!(
        grants,
        vec![1, 2, 1, 2],
        "the scheduler must rotate, not starve"
    );

    // Resumable: verify in two chunks with an interruption between them.
    let mut verifier = promoted(4);
    let mut pass = verifier.begin_pass(POLICY).expect("policy matches");
    pass.verify_chunk(scope_entries(4).into_iter().take(2).collect())
        .expect("first chunk verifies");
    assert_eq!(
        pass.remaining().len(),
        2,
        "the cursor must reflect finished work"
    );
    pass.verify_chunk(scope_entries(4).into_iter().skip(2).collect())
        .expect("the resumed chunk verifies");
    let record = pass.complete().expect("the whole scope was verified");
    let advanced_at = MonotonicInstant(T0.0 + 60_000);
    verifier
        .advance(record, advanced_at)
        .expect("a complete record advances");
    assert!(
        verifier.is_current(at_deadline()),
        "the advanced deadline must move with the record"
    );

    // Fenced: a refresh seals a NEW receipt; the stale pass's record refuses.
    let mut fenced = promoted(2);
    let stale_pass = {
        let mut pass = fenced.begin_pass(POLICY).expect("policy matches");
        pass.verify_chunk(scope_entries(2)).expect("chunk verifies");
        pass
    };
    fenced
        .refresh_scope(VerificationScopeReceipt::seal(POLICY, scope_entries(2)))
        .expect("a same-policy refresh is legitimate");
    let stale_record = stale_pass
        .complete()
        .expect("the stale pass still completes locally");
    assert_eq!(
        fenced
            .advance(stale_record, MonotonicInstant(T0.0 + 1_000))
            .unwrap_err(),
        VerificationRefusal::ProofFenced,
        "a record bound to a refreshed-away receipt must not advance"
    );
    // Positive control: a pass against the NEW receipt advances.
    let mut fresh_pass = fenced.begin_pass(POLICY).expect("policy matches");
    fresh_pass
        .verify_chunk(scope_entries(2))
        .expect("chunk verifies");
    let fresh_record = fresh_pass.complete().expect("whole scope");
    fenced
        .advance(fresh_record, MonotonicInstant(T0.0 + 2_000))
        .expect("the fresh receipt's record advances");
}

// ── sealed scope ───────────────────────────────────────────────────────────

/// The receipt is sealed at discovery: a pass over a SUBSET refuses with the
/// exact shortfall, extra entries refuse, mismatched stamps refuse — and the
/// exact whole scope completes.
#[test]
fn scope_discovery_seals_the_receipt_and_no_pass_may_narrow_it() {
    let mut verifier = promoted(3);

    // Subset: silent narrowing refused with the counts.
    let mut narrow = verifier.begin_pass(POLICY).expect("policy matches");
    narrow
        .verify_chunk(scope_entries(3).into_iter().take(2).collect())
        .expect("chunks verify");
    match narrow.complete().unwrap_err() {
        VerificationRefusal::NotWholeScope {
            missing,
            extra,
            mismatched,
        } => {
            assert_eq!((missing, extra, mismatched), (1, 0, 0));
        }
        other => panic!("a narrowed pass must refuse as NotWholeScope, got {other:?}"),
    }

    // Extra entries: a pass may not widen either.
    let mut wide = verifier.begin_pass(POLICY).expect("policy matches");
    let mut extra_entries = scope_entries(3);
    extra_entries.insert(99, 999);
    wide.verify_chunk(extra_entries).expect("chunks verify");
    match wide.complete().unwrap_err() {
        VerificationRefusal::NotWholeScope {
            missing,
            extra,
            mismatched,
        } => {
            assert_eq!((missing, extra, mismatched), (0, 1, 0));
        }
        other => panic!("a widened pass must refuse as NotWholeScope, got {other:?}"),
    }

    // Mismatched stamp: verifying different bytes is not verifying the scope.
    let mut skewed = verifier.begin_pass(POLICY).expect("policy matches");
    let mut skewed_entries = scope_entries(3);
    skewed_entries.insert(1, 555);
    skewed.verify_chunk(skewed_entries).expect("chunks verify");
    match skewed.complete().unwrap_err() {
        VerificationRefusal::NotWholeScope {
            missing,
            extra,
            mismatched,
        } => {
            assert_eq!((missing, extra, mismatched), (0, 0, 1));
        }
        other => panic!("a stamp-skewed pass must refuse as NotWholeScope, got {other:?}"),
    }

    // Positive control: the exact whole scope completes and advances.
    let mut exact = verifier.begin_pass(POLICY).expect("policy matches");
    exact.verify_chunk(scope_entries(3)).expect("chunks verify");
    let record = exact.complete().expect("exact whole scope");
    verifier
        .advance(record, MonotonicInstant(T0.0 + 1_000))
        .expect("the exact record advances");
}

/// A same-stamp rewrite during a pass stays racy-clean; a different stamp
/// dirties the scope and the pass's record cannot advance.
#[test]
fn same_stamp_rewrites_stay_racy_clean() {
    // Racy-clean: rewritten with the SAME stamp mid-pass.
    let mut clean = promoted(2);
    let mut pass = clean.begin_pass(POLICY).expect("policy matches");
    pass.verify_chunk(scope_entries(2)).expect("chunks verify");
    clean.observe_rewrite(0, 100);
    let record = pass.complete().expect("whole scope");
    clean
        .advance(record, MonotonicInstant(T0.0 + 1_000))
        .expect("a same-stamp rewrite must not dirty the scope");

    // Dirty: rewritten with a DIFFERENT stamp mid-pass.
    let mut dirty = promoted(2);
    let mut pass = dirty.begin_pass(POLICY).expect("policy matches");
    pass.verify_chunk(scope_entries(2)).expect("chunks verify");
    dirty.observe_rewrite(0, 4_242);
    let record = pass.complete().expect("whole scope");
    let before = dirty.is_current(just_before_deadline());
    assert_eq!(
        dirty
            .advance(record, MonotonicInstant(T0.0 + 1_000))
            .unwrap_err(),
        VerificationRefusal::ScopeDirty,
        "a changed-stamp rewrite must dirty the scope"
    );
    assert_eq!(
        dirty.is_current(just_before_deadline()),
        before,
        "a refused advance must not move the deadline either way"
    );
}

// ── the exact deadline ─────────────────────────────────────────────────────

/// Frozen FR-049's boundary, exactly: strictly-before the fixed 15-minute
/// age remains eligible; AT the deadline the verifier latches
/// `VerificationOverdueLatched` BEFORE any strict acquisition is served.
#[test]
fn the_deadline_boundary_is_exact_and_latches_before_acquisition() {
    let mut verifier = promoted(2);

    // Just-before: eligible, acquirable, no latch.
    assert!(verifier.is_current(just_before_deadline()));
    verifier
        .acquire_strict(just_before_deadline())
        .expect("just-before the deadline remains eligible");
    assert_eq!(verifier.non_current_cause(), None);

    // AT the deadline: the latch lands first, then the refusal.
    assert!(
        !verifier.is_current(at_deadline()),
        "AT the deadline is overdue, not eligible"
    );
    assert_eq!(
        verifier.acquire_strict(at_deadline()).unwrap_err(),
        VerificationRefusal::OverdueLatched
    );
    assert_eq!(
        verifier.non_current_cause(),
        Some(NonCurrentCause::OverdueLatched),
        "the latch must be observable after the refusal"
    );

    // Latched is sticky: even an earlier instant no longer acquires.
    assert!(verifier.acquire_strict(just_before_deadline()).is_err());
}

/// Frozen FR-049's latch-clear clause, exactly: "Only a fresh complete
/// exact-bound verification and publication may clear the latch." A latched
/// OVERDUE verifier can still verify — begin a pass, complete the whole
/// scope, and advance — and that fresh complete record clears the latch;
/// strict leases resume. The other non-Current causes stay re-scout-only:
/// a lost reservation has no capacity to verify with, and a policy mismatch
/// invalidates the scope itself (pair-2 review, MAJOR finding).
#[test]
fn a_fresh_complete_verification_clears_the_overdue_latch() {
    let mut verifier = promoted(2);
    assert_eq!(
        verifier.acquire_strict(at_deadline()).unwrap_err(),
        VerificationRefusal::OverdueLatched
    );
    assert_eq!(
        verifier.non_current_cause(),
        Some(NonCurrentCause::OverdueLatched)
    );

    // A mismatched-policy probe against the latched verifier refuses WITHOUT
    // relabeling the latch: the recorded cause stays honest.
    assert_eq!(
        verifier.begin_pass(PolicyVersion(9)).unwrap_err(),
        VerificationRefusal::PolicyMismatch
    );
    assert_eq!(
        verifier.non_current_cause(),
        Some(NonCurrentCause::OverdueLatched),
        "an unrelated later probe must not relabel the latch"
    );

    // The latched verifier may still VERIFY (never lease): the FR's named
    // clear-path must be representable.
    let mut pass = verifier
        .begin_pass(POLICY)
        .expect("an overdue-latched verifier must still be verifiable");
    pass.verify_chunk(scope_entries(2)).expect("chunks verify");
    let record = pass.complete().expect("whole scope");
    let cleared_at = at_deadline().plus(Duration::from_secs(60));
    verifier
        .advance(record, cleared_at)
        .expect("a fresh complete verification clears the overdue latch");
    assert_eq!(verifier.non_current_cause(), None);
    verifier
        .acquire_strict(MonotonicInstant(cleared_at.0 + 1))
        .expect("strict leases resume after the latch clears");

    // Negative control: a lost reservation is NOT clearable by verification.
    let mut lost = promoted(2);
    lost.reservation_lost();
    assert_eq!(
        lost.begin_pass(POLICY).unwrap_err(),
        VerificationRefusal::NonCurrent(NonCurrentCause::ReservationLost),
        "only the overdue latch is verification-clearable"
    );
}

/// Partial, cancelled, and resumed work never extends the deadline: only a
/// complete exact-identity whole-scope record does.
#[test]
fn partial_cancelled_or_resumed_work_never_extends_the_deadline() {
    let mut verifier = promoted(4);

    // Partial progress, then a cancelled pass, then a resumed partial pass.
    let mut abandoned = verifier.begin_pass(POLICY).expect("policy matches");
    abandoned
        .verify_chunk(scope_entries(4).into_iter().take(2).collect())
        .expect("chunks verify");
    drop(abandoned); // cancelled

    let mut resumed = verifier.begin_pass(POLICY).expect("policy matches");
    resumed
        .verify_chunk(scope_entries(4).into_iter().take(3).collect())
        .expect("chunks verify");
    assert_eq!(resumed.remaining().len(), 1, "still partial by design");

    // None of that moved the deadline: AT the original deadline it latches.
    assert!(!verifier.is_current(at_deadline()));
    assert_eq!(
        verifier.acquire_strict(at_deadline()).unwrap_err(),
        VerificationRefusal::OverdueLatched
    );

    // Positive control on a fresh verifier: a COMPLETE record extends.
    let mut extended = promoted(4);
    let mut pass = extended.begin_pass(POLICY).expect("policy matches");
    pass.verify_chunk(scope_entries(4)).expect("chunks verify");
    let record = pass.complete().expect("whole scope");
    let advanced_at = MonotonicInstant(T0.0 + 120_000);
    extended
        .advance(record, advanced_at)
        .expect("complete record advances");
    assert!(
        extended.is_current(at_deadline()),
        "the deadline must now anchor at the record"
    );
    assert!(!extended.is_current(advanced_at.plus(MAX_CURRENT_UNVERIFIED_AGE)));
}

// ── work bounds and feasibility ────────────────────────────────────────────

/// The frozen constants are pinned by VALUE, not only by the source seal: a
/// changed deadline or cap must fail an oracle, not just a hash refresh
/// (pair-2 review). Values verbatim from the frozen data model.
#[test]
fn the_frozen_constants_are_pinned_by_value() {
    assert_eq!(MAX_CURRENT_UNVERIFIED_AGE, Duration::from_secs(900));
    assert_eq!(
        DEFAULT_MAX_COMPLETE_VERIFICATION_PASS,
        Duration::from_secs(720)
    );
    assert_eq!(DEFAULT_MAX_VERIFICATION_BYTES, 17_179_869_184);
    assert_eq!(DEFAULT_MAX_VERIFICATION_ENTRIES, 200_000);
    assert_eq!(RESERVED_VERIFICATION_BYTES_PER_SECOND, 33_554_432);
    assert_eq!(RESERVED_VERIFICATION_ENTRIES_PER_SECOND, 1_000);
}

/// The computed bound is the ceiling arithmetic at the reserved floors, the
/// caps make 712 s the reachable maximum (STRICTLY under the 720 s default —
/// "a ceiling the defaults never reach"), work beyond the caps refuses at
/// admission, and feasibility is bound to its scope — a reservation for one
/// receipt cannot promote another.
#[test]
fn the_work_bound_never_exceeds_the_reachable_default() {
    let receipt = sealed(1);
    let tiny = VerificationWorkBound::for_scope(&receipt, 1, 1).expect("affordable");
    assert_eq!(
        tiny.bound(),
        Duration::from_secs(2),
        "ceil(1/floor) is one second per axis"
    );

    let max = VerificationWorkBound::for_scope(
        &receipt,
        DEFAULT_MAX_VERIFICATION_BYTES,
        DEFAULT_MAX_VERIFICATION_ENTRIES,
    )
    .expect("the caps themselves are affordable");
    assert_eq!(max.bound(), Duration::from_secs(712));
    assert!(max.bound() < DEFAULT_MAX_COMPLETE_VERIFICATION_PASS);

    assert_eq!(
        VerificationWorkBound::for_scope(&receipt, DEFAULT_MAX_VERIFICATION_BYTES + 1, 1)
            .unwrap_err(),
        VerificationRefusal::WorkBeyondCaps
    );
    assert_eq!(
        VerificationWorkBound::for_scope(&receipt, 1, DEFAULT_MAX_VERIFICATION_ENTRIES + 1)
            .unwrap_err(),
        VerificationRefusal::WorkBeyondCaps
    );

    // Feasibility is bound, never bearer: a foreign reservation refuses.
    let this_scope = sealed(2);
    let other_scope = sealed(2);
    assert_eq!(
        RollingVerifier::promote(this_scope, feasibility_for(&other_scope), T0).unwrap_err(),
        VerificationRefusal::FeasibilityNotForThisScope
    );
}

/// Losing the feasibility reservation forces non-Current — it never extends
/// the deadline, and only an authoritative re-scout returns to Current.
#[test]
fn a_lost_reservation_forces_non_current_not_an_extension() {
    let mut verifier = promoted(2);
    assert!(verifier.is_current(MonotonicInstant(T0.0 + 1)));

    verifier.reservation_lost();
    assert!(
        !verifier.is_current(MonotonicInstant(T0.0 + 2)),
        "losing the reservation is non-Current NOW"
    );
    assert_eq!(
        verifier.non_current_cause(),
        Some(NonCurrentCause::ReservationLost)
    );
    assert_eq!(
        verifier
            .acquire_strict(MonotonicInstant(T0.0 + 3))
            .unwrap_err(),
        VerificationRefusal::NonCurrent(NonCurrentCause::ReservationLost)
    );

    // The only way back: an authoritative re-scout with fresh feasibility
    // bound to the fresh receipt.
    let rescouted_at = MonotonicInstant(T0.0 + 10_000);
    let fresh = sealed(2);
    let fresh_feasibility = feasibility_for(&fresh);
    verifier
        .re_scout(fresh, fresh_feasibility, rescouted_at)
        .expect("re-scout accepts its own scope's feasibility");
    assert_eq!(verifier.non_current_cause(), None);
    verifier
        .acquire_strict(MonotonicInstant(rescouted_at.0 + 1))
        .expect("re-scout restores Current");
}

/// A policy-version mismatch refuses the pass AND forces non-Current: no new
/// Current promotion happens before an authoritative re-scout.
#[test]
fn policy_mismatch_forces_rescout_before_new_current() {
    let mut verifier = promoted(2);
    assert_eq!(
        verifier.begin_pass(PolicyVersion(8)).unwrap_err(),
        VerificationRefusal::PolicyMismatch
    );
    assert_eq!(
        verifier.non_current_cause(),
        Some(NonCurrentCause::PolicyMismatch)
    );
    assert_eq!(
        verifier
            .acquire_strict(MonotonicInstant(T0.0 + 1))
            .unwrap_err(),
        VerificationRefusal::NonCurrent(NonCurrentCause::PolicyMismatch)
    );

    // A cross-policy refresh may not smuggle past the mandated re-scout:
    // it refuses without relabeling the latched cause.
    assert_eq!(
        verifier
            .refresh_scope(VerificationScopeReceipt::seal(
                PolicyVersion(8),
                scope_entries(2)
            ))
            .unwrap_err(),
        VerificationRefusal::PolicyMismatch
    );
    assert_eq!(
        verifier.non_current_cause(),
        Some(NonCurrentCause::PolicyMismatch)
    );

    // Re-scout under the new policy is the only path to a new Current.
    let rescouted_at = MonotonicInstant(T0.0 + 5_000);
    let repolicied = VerificationScopeReceipt::seal(PolicyVersion(8), scope_entries(2));
    let repolicied_feasibility = feasibility_for(&repolicied);
    verifier
        .re_scout(repolicied, repolicied_feasibility, rescouted_at)
        .expect("re-scout accepts its own scope's feasibility");
    let mut pass = verifier
        .begin_pass(PolicyVersion(8))
        .expect("the re-scouted policy matches");
    pass.verify_chunk(scope_entries(2)).expect("chunks verify");
    let record = pass.complete().expect("whole scope");
    verifier
        .advance(record, MonotonicInstant(rescouted_at.0 + 1_000))
        .expect("the re-scouted scope promotes and advances");
}
