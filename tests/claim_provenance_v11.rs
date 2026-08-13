//! Feature 020 V11, T041 — claim attribution and the `OperationContractV1`
//! Cartesian negatives.
//!
//! RED until T043 exists.
//!
//! API surface follows `contracts/public-api-v11.json` `introduced_v11_atoms`,
//! NOT the `data-model.md` prose. The two frozen documents disagree: the data
//! model spells a pub-field `Claim` with `producing_publication` and a
//! four-variant `SourceRefusal` enum, while the atoms fix
//! `Claim::producing_runtime_identity` and an opaque `SourceRefusal` with
//! `kind` / `operation` / `retry` / `evidence_identity`, plus the
//! `SourceRefusalKind` and `RetryAdvice` types. The atoms win because
//! `expected_graph.activation_rule` refuses activation on a missing or extra
//! atom, while no checker compares the prose to code. See D9 in
//! `docs/reviews/SLICE3-RECON-FINDINGS-v11.md`.
//!
//! The four refusal KIND NAMES are identical across both documents, so these
//! tests match on `kind()` and never destructure fields.

use symforge::live_index::knowledge_bridge::DerivedLimitKind;
use symforge::protocol::format::claim_provenance::{
    AtomicAuthority, Claim, ClaimInput, ClaimProvenance, ComparisonRelation, EvaluationProvenance,
    ObservationLease, OperationKind, OperationReceipt, OutputCoverage, PhysicalRootLease,
    RetryAdvice, SourceRefusalKind,
};

// ── Fixtures ───────────────────────────────────────────────────────────────

fn a_lease(root: &str) -> ObservationLease {
    ObservationLease::for_test_root(PhysicalRootLease::for_test_root(root))
}

fn an_operation(kind: OperationKind) -> OperationReceipt {
    OperationReceipt::for_test(kind)
}

fn a_generation(lease: &ObservationLease) -> AtomicAuthority {
    AtomicAuthority::Generation(lease.admit_generation().expect("complete generation"))
}

/// Every atomic authority kind, once. The Cartesian tests iterate this so a new
/// authority kind cannot be added without being covered.
fn every_authority_kind(lease: &ObservationLease) -> Vec<AtomicAuthority> {
    vec![
        a_generation(lease),
        AtomicAuthority::DiskObservation(
            lease
                .observe_missing_path(
                    "src/gone.rs",
                    symforge::protocol::format::claim_provenance::ObservationTime::fresh(),
                )
                .expect("missing path"),
        ),
        AtomicAuthority::WorktreeScopeObservation(
            lease
                .complete_scope_scan(
                    symforge::protocol::format::claim_provenance::WorktreeObservationScope::beneath(
                        "src/",
                    ),
                )
                .expect("complete scan"),
        ),
        AtomicAuthority::GitObservation(
            symforge::protocol::format::claim_provenance::GitObservationReceipt::not_in_tree(
                "tree-1",
                "src/gone.rs",
            ),
        ),
    ]
}

// ── Every authority kind is self-describing and distinctly identified ──────

#[test]
fn every_authority_kind_reports_its_own_name_and_a_distinct_identity() {
    let lease = a_lease("root-a");
    let authorities = every_authority_kind(&lease);

    let mut names: Vec<&'static str> = authorities.iter().map(|a| a.kind_name()).collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(
        names.len(),
        4,
        "all four authority kinds must be distinguishable by name: {names:?}"
    );

    let mut identities: Vec<_> = authorities.iter().map(|a| a.identity()).collect();
    identities.sort_unstable();
    identities.dedup();
    assert_eq!(
        identities.len(),
        4,
        "each authority carries its own identity; none may share one"
    );
}

// ── Provenance shape: cardinality is enforced, not advisory ────────────────

#[test]
fn a_comparison_admits_exactly_two_authorities() {
    let lease = a_lease("root-a");
    let provenance = ClaimProvenance::comparison(
        an_operation(OperationKind::Comparison),
        ComparisonRelation::SameContent,
        a_generation(&lease),
        a_generation(&lease),
    )
    .expect("two authorities is the comparison arity");

    assert_eq!(provenance.authority_count(), 2);
    assert_eq!(provenance.kind_name(), "Comparison");
    assert_eq!(
        provenance.authorities().count(),
        2,
        "both sides stay retained and enumerable"
    );
}

#[test]
fn a_derivation_refuses_an_empty_input_set() {
    let refusal = ClaimProvenance::derivation(an_operation(OperationKind::Derivation), Vec::new())
        .expect_err("a derivation with no inputs proves nothing and must refuse");

    assert_eq!(refusal.kind(), SourceRefusalKind::InvalidSelection);
}

#[test]
fn a_derivation_is_n_ary_above_one() {
    let lease = a_lease("root-a");
    for arity in 1..=4 {
        let inputs: Vec<ClaimInput> = (0..arity)
            .map(|_| ClaimInput::Authority(a_generation(&lease)))
            .collect();
        let provenance =
            ClaimProvenance::derivation(an_operation(OperationKind::Derivation), inputs)
                .unwrap_or_else(|_| panic!("arity {arity} must be admitted"));
        assert_eq!(
            provenance.authority_count(),
            arity,
            "a derivation retains every input it was given"
        );
    }
}

#[test]
fn a_selected_aggregate_refuses_a_selection_without_its_generation() {
    let lease = a_lease("root-a");
    let refusal = ClaimProvenance::selected_aggregate(
        an_operation(OperationKind::SelectedAggregate),
        vec![lease.selection_receipt("project-a")],
        Vec::new(),
    )
    .expect_err("a selection with no matching captured generation breaks the bijection");

    assert_eq!(refusal.kind(), SourceRefusalKind::SelectionUnavailable);
}

#[test]
fn a_selected_aggregate_refuses_an_extra_unselected_generation() {
    // The OTHER arm of the bijection. "Missing, extra, forged, or uncaptured
    // inputs refuse" — data-model.md:1893. The missing-generation test above
    // exercises containment; this one exercises the length guard, which is the
    // only thing that catches a captured generation nobody selected. Found by
    // mutation: disabling the length check alone survived the original suite.
    let lease = a_lease("root-a");
    let refusal = ClaimProvenance::selected_aggregate(
        an_operation(OperationKind::SelectedAggregate),
        vec![lease.selection_receipt("project-a")],
        vec![
            (
                "project-a".to_string(),
                lease.admit_generation().expect("gen"),
            ),
            (
                "project-b".to_string(),
                lease.admit_generation().expect("gen"),
            ),
        ],
    )
    .expect_err("an extra captured generation breaks the exact bijection");

    assert_eq!(refusal.kind(), SourceRefusalKind::SelectionUnavailable);
}

#[test]
fn a_selected_aggregate_admits_an_exact_bijection() {
    // GREEN-CONTROL: the refusal above is about the MISSING generation, not
    // about SelectedAggregate being unconstructible.
    let lease = a_lease("root-a");
    let provenance = ClaimProvenance::selected_aggregate(
        an_operation(OperationKind::SelectedAggregate),
        vec![lease.selection_receipt("project-a")],
        vec![(
            "project-a".to_string(),
            lease.admit_generation().expect("gen"),
        )],
    )
    .expect("an exact selection-to-generation bijection is admitted");

    assert_eq!(provenance.kind_name(), "SelectedAggregate");
    assert_eq!(provenance.authority_count(), 1);
}

// ── The OperationContractV1 Cartesian ──────────────────────────────────────

/// The name is pinned by the frozen traceability catalog:
/// `contracts/lifecycle-oracle-traceability-v11.md` binds `TEST-PROVENANCE` to
/// `tests/claim_provenance_v11.rs::operation_contract_cartesian_matrix`
/// (CMD-PROVENANCE, owner T041, introduced_slice 3). The checker activates the
/// pin the moment this FILE exists, so the test carries the contract-pinned
/// name, not an invented one — and since the pinned name says OPERATION
/// contract, the operation kind is an axis of the matrix, not a constant.
#[test]
fn operation_contract_cartesian_matrix() {
    let lease = a_lease("root-a");
    let operations = [
        OperationKind::Retrieval,
        OperationKind::Comparison,
        OperationKind::Derivation,
        OperationKind::SelectedAggregate,
    ];
    let kinds = [
        SourceRefusalKind::AdmissionUnavailable,
        SourceRefusalKind::InvalidSelection,
        SourceRefusalKind::SelectionUnavailable,
        SourceRefusalKind::SourceUnavailable,
    ];
    let advices = [
        RetryAdvice::Never,
        RetryAdvice::AfterRebind,
        RetryAdvice::AfterRefresh,
    ];

    let mut seen = 0usize;
    for operation_kind in operations {
        for kind in kinds {
            for advice in advices {
                let operation = an_operation(operation_kind);
                let refusal = lease.refuse(operation, kind, advice);

                assert_eq!(
                    refusal.kind(),
                    kind,
                    "the refusal reports the kind it was built with"
                );
                assert_eq!(
                    refusal.retry(),
                    advice,
                    "retry advice survives the round trip"
                );
                assert_eq!(
                    refusal.operation().operation_kind(),
                    operation_kind,
                    "every refusal names the operation that produced it"
                );
                assert_eq!(
                    refusal.operation().schema_version(),
                    OperationReceipt::SCHEMA_VERSION,
                    "the receipt rides the one V11 schema"
                );
                assert!(
                    refusal.evidence_identity().is_some(),
                    "a refusal carries the identity of the evidence it refused on"
                );
                seen += 1;
            }
        }
    }

    assert_eq!(
        seen, 48,
        "control: the full 4x4x3 operation-contract Cartesian was exercised"
    );
}

// ── Evaluation provenance accompanies observable ordering ─────────────────

#[test]
fn an_ordered_result_carries_evaluation_provenance_and_an_unordered_one_does_not() {
    let lease = a_lease("root-a");

    let ranked = Claim::single_ranked(
        an_operation(OperationKind::Retrieval),
        a_generation(&lease),
        vec!["a", "b"],
        EvaluationProvenance::for_test(),
    );
    assert!(
        ranked.evaluation().is_some(),
        "order is observable, so the ranking that produced it must be attributable"
    );

    let unordered = Claim::single(
        an_operation(OperationKind::Retrieval),
        a_generation(&lease),
        (),
    );
    assert!(
        unordered.evaluation().is_none(),
        "no observable order means no ranking to attribute"
    );
}

// ── OutputCoverage::Truncated is post-lease only ──────────────────────────

#[test]
fn truncated_coverage_requires_a_completed_strict_lease() {
    let lease = a_lease("root-a");
    let render = lease
        .completed_render_authority()
        .expect("a completed strict lease may bound its own rendering");

    let truncated = render.truncate(vec![(DerivedLimitKind::Cards, 3)]);
    assert!(
        matches!(truncated, OutputCoverage::Truncated { .. }),
        "bounded rendering after a completed lease may report truncation"
    );
}

#[test]
fn truncation_uses_the_live_limit_kinds_not_a_second_enum() {
    // D3: `data-model.md` lists SIX DerivedLimitKind variants; production ships
    // EIGHT and records the extra two. T043 reuses the live type rather than
    // transcribing a lossy copy, so the two extra kinds must be expressible.
    let lease = a_lease("root-a");
    let render = lease.completed_render_authority().expect("completed lease");

    let truncated = render.truncate(vec![
        (DerivedLimitKind::OwnershipSelectors, 1),
        (DerivedLimitKind::AmbiguousSamples, 2),
    ]);

    let OutputCoverage::Truncated { breaches } = truncated else {
        panic!("expected a truncated coverage");
    };
    assert_eq!(
        breaches.len(),
        2,
        "both production-only limit kinds survive into the breach record"
    );
}

#[test]
fn truncated_coverage_never_enters_a_claim_identity() {
    let lease = a_lease("root-a");
    let render = lease.completed_render_authority().expect("completed lease");
    let truncated = render.truncate(vec![(DerivedLimitKind::Output, 1)]);

    let claim = Claim::single(
        an_operation(OperationKind::Retrieval),
        a_generation(&lease),
        (),
    );
    let before = claim.provenance().identity();
    let rendered = claim.render_bounded(truncated);

    assert_eq!(
        rendered.provenance().identity(),
        before,
        "bounded rendering does not change generation completeness, so it must not \
         move the provenance identity that caches, CCR, and persistence key on"
    );
}

// ── Retrieval voice never selects consistency ─────────────────────────────

#[test]
fn the_knowledge_voice_filter_never_selects_consistency() {
    let selectable =
        symforge::protocol::format::claim_provenance::KnowledgeVoiceFilter::selectable_voices();

    assert!(
        !selectable.iter().any(|voice| voice.is_consistency()),
        "retrieval voice must never select consistency; that is authority hygiene, \
         not a retrieval filter"
    );
    assert!(
        !selectable.is_empty(),
        "control: the filter does select SOMETHING, so the assertion above is not vacuous"
    );
}
