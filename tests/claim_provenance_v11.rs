#![cfg(feature = "server")]
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
    ObservationLease, ObservationTime, OperationKind, OperationReceipt, OutputCoverage,
    PhysicalRootLease, RetryAdvice, SourceRefusalKind,
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

    let identities: std::collections::HashSet<_> =
        authorities.iter().map(|a| a.identity()).collect();
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
        an_operation(OperationKind::SearchText),
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
    let refusal =
        ClaimProvenance::derivation(an_operation(OperationKind::SearchSymbols), Vec::new())
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
            ClaimProvenance::derivation(an_operation(OperationKind::SearchSymbols), inputs)
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
        an_operation(OperationKind::SearchText),
        vec![lease.selection_receipt("project-a")],
        Vec::new(),
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
        an_operation(OperationKind::SearchText),
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
        Vec::new(),
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
        an_operation(OperationKind::SearchText),
        vec![lease.selection_receipt("project-a")],
        vec![(
            "project-a".to_string(),
            lease.admit_generation().expect("gen"),
        )],
        Vec::new(),
    )
    .expect("an exact selection-to-generation bijection is admitted");

    assert_eq!(provenance.kind_name(), "SelectedAggregate");
    assert_eq!(provenance.authority_count(), 1);
}

#[test]
fn a_comparison_across_two_roots_is_refused_rather_than_composed() {
    // The derivation path had this oracle; the comparison path did not, so the
    // comparison's own root gate could be deleted without failing any test —
    // a mutation-surviving gap the audit found.
    let left = a_lease("root-a");
    let right = a_lease("root-b");
    let refusal = ClaimProvenance::comparison(
        an_operation(OperationKind::SearchText),
        ComparisonRelation::SameContent,
        a_generation(&left),
        a_generation(&right),
    )
    .expect_err("two roots must not compose into one comparison");

    assert_eq!(refusal.kind(), SourceRefusalKind::SourceUnavailable);
}

#[test]
fn a_generation_authority_never_proves_repository_absence() {
    // The no-widening loop covers the three OBSERVATION kinds; Generation
    // legitimately proves generation-scoped absence, so it cannot join that
    // loop — but its repository claim still needs pinning, which nothing did.
    let lease = a_lease("root-a");
    let generation = lease.admit_generation().expect("complete generation");

    assert!(
        generation.proves_generation_absence(),
        "GREEN-CONTROL: one captured generation does prove generation-scoped absence"
    );
    assert!(
        !generation.proves_repository_absence(),
        "one captured generation is not the repository"
    );
}

#[test]
fn a_selected_aggregate_refuses_a_forged_duplicate_capture() {
    // "Missing, extra, FORGED, or uncaptured inputs refuse". Two captures
    // under one key are a forgery, and BTreeMap::from_iter would COLLAPSE the
    // duplicate silently — the second entry vanishing rather than refusing.
    let lease = a_lease("root-a");
    let refusal = ClaimProvenance::selected_aggregate(
        an_operation(OperationKind::SearchText),
        vec![
            lease.selection_receipt("project-a"),
            lease.selection_receipt("project-b"),
        ],
        vec![
            (
                "project-a".to_string(),
                lease.admit_generation().expect("gen"),
            ),
            (
                "project-a".to_string(),
                lease.admit_generation().expect("gen"),
            ),
        ],
        Vec::new(),
    )
    .expect_err("a duplicate capture key is forged input and must refuse");

    assert_eq!(refusal.kind(), SourceRefusalKind::InvalidSelection);
}

#[test]
fn a_selected_aggregate_names_every_authority_it_retains() {
    // authorities() yielded NOTHING for SelectedAggregate while
    // authority_count() counted its generations — an aggregate that could not
    // enumerate its own evidence. Both now come from one source of truth.
    let lease = a_lease("root-a");
    let provenance = ClaimProvenance::selected_aggregate(
        an_operation(OperationKind::SearchText),
        vec![lease.selection_receipt("project-a")],
        vec![(
            "project-a".to_string(),
            lease.admit_generation().expect("gen"),
        )],
        vec![a_generation(&lease)],
    )
    .expect("bijection plus an additional authority is admitted");

    assert_eq!(
        provenance.authority_count(),
        2,
        "one captured generation plus one additional authority"
    );
    assert_eq!(
        provenance.authorities().count(),
        provenance.authority_count(),
        "the enumeration and the count can never disagree"
    );
}

#[test]
fn a_selected_aggregate_refuses_an_empty_selection() {
    // Global absence from zero selections would be a claim about everything
    // derived from nothing. No lease is needed: the refusal happens before any
    // evidence is examined.
    let refusal = ClaimProvenance::selected_aggregate(
        an_operation(OperationKind::SearchText),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    )
    .expect_err("an empty selection proves nothing and must refuse");

    assert_eq!(refusal.kind(), SourceRefusalKind::InvalidSelection);
}

#[test]
fn a_selected_aggregate_refuses_a_foreign_root_authority() {
    // Everything the aggregate retains composes into one claim, so an
    // additional authority from another root is the same defect as a
    // mixed-root derivation.
    let lease = a_lease("root-a");
    let foreign = a_lease("root-b");
    let refusal = ClaimProvenance::selected_aggregate(
        an_operation(OperationKind::SearchText),
        vec![lease.selection_receipt("project-a")],
        vec![(
            "project-a".to_string(),
            lease.admit_generation().expect("gen"),
        )],
        vec![a_generation(&foreign)],
    )
    .expect_err("a foreign-root additional authority must not compose");

    assert_eq!(refusal.kind(), SourceRefusalKind::SourceUnavailable);
}

// ── Claim contexts: acquisition through the closed contract ────────────────

#[test]
fn a_context_refuses_an_empty_acquisition() {
    use symforge::protocol::format::claim_provenance::acquire_claim_context;

    let refusal = acquire_claim_context(an_operation(OperationKind::SearchText), Vec::new())
        .expect_err("a context with no inputs proves nothing and must refuse");

    assert_eq!(refusal.kind(), SourceRefusalKind::InvalidSelection);
}

#[test]
fn a_generation_structured_operation_requires_a_current_lease_per_input() {
    use symforge::protocol::format::claim_provenance::acquire_claim_context;

    let lease = a_lease("root-a");

    // Without the lease: refused, and the advice names the event that fixes it.
    let bare = lease.context_input("project-a", "repo-1", None);
    let refusal =
        acquire_claim_context(an_operation(OperationKind::SearchText), vec![bare.clone()])
            .expect_err("a search without a Current lease must refuse");
    assert_eq!(refusal.kind(), SourceRefusalKind::AdmissionUnavailable);

    // GREEN-CONTROL: with the lease, the same acquisition is admitted, so the
    // refusal above is about the MISSING lease and not about search contexts.
    let current = lease.current_query_lease().expect("current lease");
    let held = lease.context_input("project-a", "repo-1", Some(current));
    let context = acquire_claim_context(an_operation(OperationKind::SearchText), vec![held])
        .expect("a search with a Current lease per input is admitted");
    assert_eq!(context.inputs().len(), 1);

    // A NON-generation-structured operation may omit the lease entirely.
    let observation = acquire_claim_context(an_operation(OperationKind::RefreshSource), vec![bare])
        .expect("a pure lifecycle operation omits Current");
    assert!(!observation.permitted_relationships().requires_current());
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
    // The evidence a refusal names must be evidence that was EXAMINED — the
    // first draft minted a fresh identity inside refuse itself, fabricating
    // evidence that existed nowhere, and the oracle blessed it by asserting
    // only is_some. The audit caught both halves.
    let examined = lease
        .observe_missing_path("src/gone.rs", ObservationTime::fresh())
        .expect("missing path");

    let mut seen = 0usize;
    for operation_kind in OperationKind::ALL {
        for kind in [
            SourceRefusalKind::AdmissionUnavailable,
            SourceRefusalKind::InvalidSelection,
            SourceRefusalKind::SelectionUnavailable,
            SourceRefusalKind::SourceUnavailable,
        ] {
            for advice in RetryAdvice::ALL {
                let operation = an_operation(operation_kind);
                let refusal = lease.refuse(operation, kind, advice, Some(examined.identity()));

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
                assert_eq!(
                    refusal.evidence_identity(),
                    Some(examined.identity()),
                    "the refusal names the EXACT evidence that was examined"
                );
                seen += 1;
            }
        }
    }

    assert_eq!(
        seen,
        7 * 4 * 4,
        "control: the full operations x kinds x advices Cartesian was exercised"
    );
}

// ── Evaluation provenance accompanies observable ordering// ── Evaluation provenance accompanies observable ordering ─────────────────

#[test]
fn an_ordered_result_carries_evaluation_provenance_and_an_unordered_one_does_not() {
    let lease = a_lease("root-a");

    let ranked = Claim::single_ranked(
        an_operation(OperationKind::SearchText),
        a_generation(&lease),
        vec!["a", "b"],
        EvaluationProvenance::for_test(),
    );
    assert!(
        ranked.evaluation().is_some(),
        "order is observable, so the ranking that produced it must be attributable"
    );

    let unordered = Claim::single(
        an_operation(OperationKind::SearchText),
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
        matches!(truncated, OutputCoverage::Truncated(_)),
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

    let OutputCoverage::Truncated(breaches) = truncated else {
        panic!("expected a truncated coverage");
    };
    assert_eq!(
        breaches.breaches().len(),
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
        an_operation(OperationKind::SearchText),
        a_generation(&lease),
        (),
    );
    let before = claim.provenance().identity();
    let rendered = claim.render_bounded(truncated.clone());

    assert_eq!(
        rendered.provenance().identity(),
        before,
        "bounded rendering must not move the provenance identity that caches, CCR,          and persistence key on"
    );
    // The other half, which makes this oracle falsifiable: the coverage is
    // RETAINED on the claim. The first draft discarded the argument, so the
    // identity assertion above could never fail under any code change.
    assert_eq!(
        rendered.rendered_coverage(),
        Some(&truncated),
        "the bounded render retains the coverage it was given"
    );
}

// ── Retrieval voice and the frozen voice model ────────────────────────────

#[test]
fn the_knowledge_voice_filter_selects_the_default_set_and_no_history() {
    // "Never selects consistency" is STRUCTURAL: the frozen KnowledgeVoice
    // enum has no consistency variant at all, so no runtime value could
    // violate it. What CAN be asserted is the frozen default selection:
    // Current, Intent, NeedsReview, Unknown — including the
    // current-implementation voice the first draft dropped — and never
    // HistoryOnly or Suppressed. The first draft instead invented a
    // Consistency variant and validated its own invention; the audit caught it.
    use symforge::protocol::format::claim_provenance::{KnowledgeVoice, KnowledgeVoiceFilter};

    let selectable = KnowledgeVoiceFilter::selectable_voices();
    assert_eq!(
        selectable,
        vec![
            KnowledgeVoice::Current,
            KnowledgeVoice::Intent,
            KnowledgeVoice::NeedsReview,
            KnowledgeVoice::Unknown,
        ],
        "the default selection is exactly the four frozen default voices"
    );
    assert!(
        !selectable.iter().any(|voice| matches!(
            voice,
            KnowledgeVoice::HistoryOnly | KnowledgeVoice::Suppressed
        )),
        "history voices never enter the default selection"
    );
}
