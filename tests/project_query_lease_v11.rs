//! Feature 020 V11 Slice 4 strict-query-lease oracles (T056).
//!
//! RED-first for the pair T056+T063: authored and observed failing against
//! `todo!()` seams before any machinery existed. Every rejection case asserts
//! the accepting path in the same test — a table that refuses everything
//! satisfies a lone negative perfectly.

use std::collections::BTreeMap;

use symforge::live_index::index_lifecycle::candidate::{
    IsolatedCandidate, ProjectArtifactRoot, SourceId,
};
use symforge::live_index::index_lifecycle::capacity::ProcessCapacityPool;
use symforge::live_index::index_lifecycle::query::{
    HealthSurface, OutputCoverage, ProjectQueryTable, QueryOutcome, SelectedAggregate,
    SelectionRefusal, acquire_multi_project,
};
use symforge::live_index::index_lifecycle::supervisor::{ClassifiedFailure, SourceSupervisor};

// ── fixtures ───────────────────────────────────────────────────────────────

/// A table with sources 1..=count published Current at generation 10+id.
fn current_table(count: u64) -> ProjectQueryTable {
    let mut table = ProjectQueryTable::new();
    for id in 1..=count {
        table.publish_current(SourceId(id), 10 + id);
    }
    table
}

fn select_all(count: u64) -> SelectedAggregate {
    SelectedAggregate::of((1..=count).map(|id| (id, 10 + id)))
}

// ── the frozen-named oracles ───────────────────────────────────────────────

/// TEST-QUERY (T056). The name is pinned by
/// `contracts/lifecycle-oracle-traceability-v11.md` as a `planned_exact`
/// target; do not rename it without amending that contract.
///
/// A strict selection is ATOMIC — one capture of exactly the selected
/// sources — and COMPLETE — the captured set is a bijection with the
/// selection. One non-Current source turns the whole answer into a typed
/// refusal naming it: never a partial answer, never a no-match. The capture
/// is immune to publications that land after acquisition.
#[test]
fn strict_selection_is_atomic_and_complete() {
    let mut table = current_table(3);

    // The accepting path: exact bijection, captured atomically.
    let lease = table
        .acquire_strict(&select_all(3))
        .expect("all selected sources are Current");
    let captured = lease.captured();
    assert_eq!(
        captured,
        BTreeMap::from([(SourceId(1), 11), (SourceId(2), 12), (SourceId(3), 13),]),
        "the capture must be the exact selected bijection"
    );

    // Atomicity: a publication AFTER acquisition does not reach into the
    // open lease's capture, and the completed lease renders the captured
    // truth.
    table.publish_current(SourceId(2), 99);
    assert_eq!(
        lease.captured()[&SourceId(2)],
        12,
        "the capture moved after acquisition"
    );

    // One non-Current source: the whole selection refuses, naming it.
    let mut poisoned = current_table(3);
    poisoned.mark_non_current(SourceId(2));
    assert_eq!(
        poisoned.acquire_strict(&select_all(3)).unwrap_err(),
        SelectionRefusal::NotCurrent(SourceId(2)),
        "a non-Current member must poison the whole selection with a TYPED refusal"
    );
}

/// TEST-HEALTH (T056). The name is pinned by
/// `contracts/lifecycle-oracle-traceability-v11.md` as a `planned_exact`
/// target; do not rename it without amending that contract.
///
/// Committed generations and bounded attempts are SEPARATE ledgers, and all
/// four health surfaces report them as two separate numbers: attempt
/// diagnostics can never masquerade as committed dispositions, on any
/// surface. (Committed rows also appear in the bounded diagnostics ledger —
/// separate, not disjoint.)
#[test]
fn committed_generation_and_attempt_health_are_separate() {
    let table = current_table(1);
    let supervisor = SourceSupervisor::new();

    // Two committed generations...
    for _ in 0..2 {
        let attempt = supervisor.begin_attempt();
        let root = ProjectArtifactRoot::empty();
        let pool = ProcessCapacityPool::new();
        let owner = pool.root(1_000);
        IsolatedCandidate::prepare_full(&pool, owner, &attempt, Vec::new(), |_| 0)
            .expect("capacity headroom exists")
            .commit(&root)
            .expect("an empty full candidate publishes");
    }
    // ...two classified discards and one cancellation. The successor each
    // retry trigger mints (and the final trigger's still-live successor)
    // are not terminal rows: five terminal attempts total, two committed.
    supervisor.begin_attempt();
    supervisor.retry_trigger(ClassifiedFailure::Unreadable);
    supervisor.retry_trigger(ClassifiedFailure::ParseFailed);
    supervisor.begin_attempt().cancel();

    for surface in HealthSurface::ALL {
        let projection = table.health(surface, &supervisor);
        assert_eq!(
            projection.committed_generations, 2,
            "{surface:?} must report the committed ledger exactly"
        );
        assert_eq!(
            projection.bounded_attempts, 5,
            "{surface:?} must report the attempt ledger exactly"
        );
    }
}

// ── selection shape rejections ─────────────────────────────────────────────

/// Empty, missing, duplicate, and generation-mismatched `SelectedAggregate`s
/// each reject with their exact typed cause — and the corrected selection
/// acquires.
#[test]
fn empty_missing_duplicate_and_mismatched_selections_reject() {
    let table = current_table(2);

    assert_eq!(
        table
            .acquire_strict(&SelectedAggregate::default())
            .unwrap_err(),
        SelectionRefusal::EmptySelection
    );
    assert_eq!(
        table
            .acquire_strict(&SelectedAggregate::of([(1, 11), (9, 19)]))
            .unwrap_err(),
        SelectionRefusal::MissingSource(SourceId(9))
    );
    assert_eq!(
        table
            .acquire_strict(&SelectedAggregate::of([(1, 11), (1, 11)]))
            .unwrap_err(),
        SelectionRefusal::DuplicateSource(SourceId(1))
    );
    assert_eq!(
        table
            .acquire_strict(&SelectedAggregate::of([(1, 11), (2, 55)]))
            .unwrap_err(),
        SelectionRefusal::MismatchedGeneration {
            source: SourceId(2),
            expected: 55,
            found: 12,
        }
    );

    // Positive control: the corrected selection acquires.
    table
        .acquire_strict(&select_all(2))
        .expect("the exact selection acquires");
}

/// `NoMatch` exists ONLY behind a completed all-Current lease: the refusal
/// path can never produce it, and an absent needle through a legitimate
/// lease does.
#[test]
fn no_match_requires_an_all_current_selection() {
    let mut table = current_table(2);

    let lease = table.acquire_strict(&select_all(2)).expect("all Current");
    let completed = lease.finalize(&table).expect("nothing drifted");
    assert_eq!(
        completed.query(424_242),
        QueryOutcome::NoMatch,
        "an absent needle through a legitimate lease is a NO-MATCH"
    );
    assert!(matches!(completed.query(11), QueryOutcome::Matches(_)));

    table.mark_non_current(SourceId(1));
    let refusal = table.acquire_strict(&select_all(2)).unwrap_err();
    assert_eq!(
        refusal,
        SelectionRefusal::NotCurrent(SourceId(1)),
        "a poisoned selection is a refusal — the no-match door never opens"
    );
}

// ── finalization fences ────────────────────────────────────────────────────

/// A lease finalizes only against the world it captured: a republication
/// stales it, a retarget fences it, and an undrifted lease completes.
#[test]
fn stale_finalization_and_retarget_races_reject() {
    // Stale: the source republished while the lease was open.
    let mut table = current_table(2);
    let lease = table.acquire_strict(&select_all(2)).expect("all Current");
    table.publish_current(SourceId(2), 99);
    assert_eq!(
        lease.finalize(&table).unwrap_err(),
        SelectionRefusal::StaleAtFinalization(SourceId(2))
    );

    // Retarget: the project moved out from under the lease.
    let mut table = current_table(2);
    let lease = table.acquire_strict(&select_all(2)).expect("all Current");
    table.retarget();
    assert_eq!(
        lease.finalize(&table).unwrap_err(),
        SelectionRefusal::RetargetedDuringLease
    );

    // Positive control: no drift, no retarget — the lease completes.
    let table = current_table(2);
    let lease = table.acquire_strict(&select_all(2)).expect("all Current");
    lease
        .finalize(&table)
        .expect("an undrifted lease completes");
}

/// Post-lease rendering may add `OutputCoverage::Truncated` ONLY after a
/// complete strict lease — and truncation changes coverage and length,
/// never identity. (Rendering before completion is unrepresentable: only
/// `CompletedLease` has `render`.)
#[test]
fn post_lease_truncation_never_changes_identity() {
    let table = current_table(3);
    let completed = table
        .acquire_strict(&select_all(3))
        .expect("all Current")
        .finalize(&table)
        .expect("nothing drifted");

    let full = completed.render(usize::MAX);
    let truncated = completed.render(1);
    assert_eq!(full.coverage, OutputCoverage::Full);
    assert_eq!(truncated.coverage, OutputCoverage::Truncated);
    assert!(truncated.body_len < full.body_len);
    assert_eq!(
        full.identity, truncated.identity,
        "truncation changed source-truth or cache identity"
    );
}

// ── protected roots (frozen SC-019) ────────────────────────────────────────

/// A real committed candidate root — the promotion evidence fixture.
fn promotion_evidence()
-> std::sync::Arc<symforge::live_index::index_lifecycle::candidate::ProjectArtifacts> {
    let pool = ProcessCapacityPool::new();
    let owner = pool.root(1_000);
    let supervisor = SourceSupervisor::new();
    let attempt = supervisor.begin_attempt();
    let root = ProjectArtifactRoot::empty();
    IsolatedCandidate::prepare_full(&pool, owner, &attempt, Vec::new(), |_| 0)
        .expect("capacity headroom exists")
        .commit(&root)
        .expect("the candidate publishes")
}

/// A protected root reaches `Current` only through full candidate
/// promotion, with ZERO state/durability-probe I/O below the source root —
/// and a bare publication refuses.
#[test]
fn protected_roots_reach_current_only_via_full_promotion() {
    let mut table = ProjectQueryTable::new();
    table.declare_protected(SourceId(7));

    assert_eq!(
        table.publish_without_promotion(SourceId(7), 1).unwrap_err(),
        SelectionRefusal::ProtectedRootRequiresPromotion(SourceId(7)),
        "a protected root must refuse bare publication"
    );

    let promoted = promotion_evidence();
    let mut probes = 0_u32;
    table
        .publish_protected_from_promotion(SourceId(7), &promoted, || probes += 1)
        .expect("full promotion is the legitimate path");
    assert_eq!(probes, 0, "SC-019: zero probe I/O below the source root");

    // And the protected source now serves strict leases.
    table
        .acquire_strict(&SelectedAggregate::of([(7, 1)]))
        .expect("the promoted protected root is Current");
}

/// The bare-publication door CONSULTS the roster (pair-3 review W3): a
/// non-protected source publishes bare and is leasable; a protected one
/// refuses and nothing changes. A refusal that fires regardless of state
/// would prove nothing about the state.
#[test]
fn the_bare_publication_door_consults_the_protection_roster() {
    let mut table = ProjectQueryTable::new();

    table
        .publish_without_promotion(SourceId(1), 5)
        .expect("non-protected sources may publish bare");
    table
        .acquire_strict(&SelectedAggregate::of([(1, 5)]))
        .expect("the bare publication is Current");

    table.declare_protected(SourceId(7));
    assert_eq!(
        table.publish_without_promotion(SourceId(7), 1).unwrap_err(),
        SelectionRefusal::ProtectedRootRequiresPromotion(SourceId(7))
    );
    assert_eq!(
        table
            .acquire_strict(&SelectedAggregate::of([(7, 1)]))
            .unwrap_err(),
        SelectionRefusal::MissingSource(SourceId(7)),
        "the refused publication must have changed nothing"
    );
}

/// Declaring protection DEMOTES any existing bare Current state: a newly
/// protected root re-earns `Current` through promotion, and the ordering
/// hole (publish first, declare later) is closed (pair-3 review W3).
#[test]
fn declaring_protection_demotes_any_bare_current_state() {
    let mut table = ProjectQueryTable::new();
    table.publish_current(SourceId(3), 5);
    table.declare_protected(SourceId(3));
    assert_eq!(
        table
            .acquire_strict(&SelectedAggregate::of([(3, 5)]))
            .unwrap_err(),
        SelectionRefusal::NotCurrent(SourceId(3)),
        "declaration must demote bare Current state"
    );

    // Promotion re-earns Current — at a generation ABOVE the demoted one.
    let promoted = promotion_evidence();
    table
        .publish_protected_from_promotion(SourceId(3), &promoted, || ())
        .expect("promotion is the path back");
    table
        .acquire_strict(&SelectedAggregate::of([(3, 6)]))
        .expect("the promoted generation continues the sequence, never reuses it");
}

/// The caller-bug door is pinned: bare `publish_current` on a protected
/// root is unrepresentable, loudly.
#[test]
#[should_panic(expected = "protected roots publish only through full candidate promotion")]
fn bare_publish_current_on_a_protected_root_is_unrepresentable() {
    let mut table = ProjectQueryTable::new();
    table.declare_protected(SourceId(9));
    table.publish_current(SourceId(9), 1);
}

/// Machinery-owned promotion generations NEVER reuse a stamp across an
/// invalidation (pair-3 review W1): the ABA that would let a lease spanning
/// an invalidation finalize clean is unrepresentable.
#[test]
fn promotion_generations_never_reuse_across_invalidation() {
    let mut table = ProjectQueryTable::new();
    table.declare_protected(SourceId(7));
    let promoted = promotion_evidence();
    table
        .publish_protected_from_promotion(SourceId(7), &promoted, || ())
        .expect("first promotion");

    let lease = table
        .acquire_strict(&SelectedAggregate::of([(7, 1)]))
        .expect("generation 1 is Current");

    table.mark_non_current(SourceId(7));
    table
        .publish_protected_from_promotion(SourceId(7), &promoted, || ())
        .expect("re-promotion after invalidation");

    assert_eq!(
        lease.finalize(&table).unwrap_err(),
        SelectionRefusal::StaleAtFinalization(SourceId(7)),
        "a lease that spanned an invalidation must never finalize clean"
    );
    table
        .acquire_strict(&SelectedAggregate::of([(7, 2)]))
        .expect("the re-promotion continues the generation sequence");
}

/// A lease binds to the table that issued it (pair-3 review W2): a
/// same-shaped foreign table cannot finalize it — the stale/retarget fences
/// cannot be driven around by handing back a lookalike world.
#[test]
fn a_lease_binds_to_its_issuing_table() {
    let issuing = current_table(2);
    let lookalike = current_table(2);

    let lease = issuing.acquire_strict(&select_all(2)).expect("all Current");
    assert_eq!(
        lease.finalize(&lookalike).unwrap_err(),
        SelectionRefusal::ForeignTable,
        "a lookalike table must not finalize another table's lease"
    );

    // Positive control: the issuing table still finalizes its own lease.
    issuing
        .acquire_strict(&select_all(2))
        .expect("all Current")
        .finalize(&issuing)
        .expect("the issuing table finalizes its own lease");
}

// ── multi-project selections ───────────────────────────────────────────────

/// An exact multi-project selection is all-or-nothing: one refusal anywhere
/// poisons the whole; all-Current everywhere leases everywhere.
#[test]
fn multi_project_selections_are_all_or_nothing() {
    let alpha = current_table(1);
    let mut beta = current_table(1);
    beta.mark_non_current(SourceId(1));

    assert_eq!(
        acquire_multi_project(&[(&alpha, &select_all(1)), (&beta, &select_all(1))]).unwrap_err(),
        SelectionRefusal::NotCurrent(SourceId(1)),
        "one project's refusal must poison the whole multi-selection"
    );

    let gamma = current_table(1);
    let leases = acquire_multi_project(&[(&alpha, &select_all(1)), (&gamma, &select_all(1))])
        .expect("all projects all Current");
    assert_eq!(leases.len(), 2, "one lease per project, atomically");
}

// ── ranking and transport ──────────────────────────────────────────────────

/// Ranking snapshots are SEPARATE from content identity: two leases over
/// identical content carry distinct ranking snapshots, and the rendered
/// identity does not move with them.
#[test]
fn ranking_snapshots_are_separate_from_content_identity() {
    let table = current_table(2);
    let first = table.acquire_strict(&select_all(2)).expect("all Current");
    let second = table.acquire_strict(&select_all(2)).expect("all Current");
    assert_ne!(
        first.ranking_snapshot(),
        second.ranking_snapshot(),
        "each lease owns its own ranking snapshot"
    );

    let first_render = first
        .finalize(&table)
        .expect("undrifted")
        .render(usize::MAX);
    let second_render = second
        .finalize(&table)
        .expect("undrifted")
        .render(usize::MAX);
    assert_eq!(
        first_render.identity, second_render.identity,
        "content identity must not move with the ranking snapshot"
    );
}

/// Every refusal maps onto exactly one stable transport code, and the codes
/// are pairwise distinct — typed on the wire, not prose.
#[test]
fn source_refusals_map_totally_onto_transport_codes() {
    let refusals = [
        SelectionRefusal::EmptySelection,
        SelectionRefusal::MissingSource(SourceId(1)),
        SelectionRefusal::DuplicateSource(SourceId(1)),
        SelectionRefusal::MismatchedGeneration {
            source: SourceId(1),
            expected: 1,
            found: 2,
        },
        SelectionRefusal::NotCurrent(SourceId(1)),
        SelectionRefusal::StaleAtFinalization(SourceId(1)),
        SelectionRefusal::RetargetedDuringLease,
        SelectionRefusal::ForeignTable,
        SelectionRefusal::ProtectedRootRequiresPromotion(SourceId(1)),
    ];
    let codes: Vec<&'static str> = refusals
        .iter()
        .map(SelectionRefusal::transport_code)
        .collect();
    for (index, code) in codes.iter().enumerate() {
        assert!(!code.is_empty());
        assert!(
            !codes[index + 1..].contains(code),
            "two refusals share the transport code {code}"
        );
    }
}
