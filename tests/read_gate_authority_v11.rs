#![cfg(feature = "server")]
//! Feature 020 V11, T042 — cross-authority proof scoping.
//!
//! RED until T043 exists. Every test here pins ONE frozen rule: an authority
//! proves exactly what it observed, and no local negative can be widened into a
//! generation-wide or repository-wide claim.
//!
//! `data-model.md:1874-1881` states the property this file enforces:
//! "`DiskObservationReceipt::PathMissing` is path-local at `observed_at` under
//! the retained parent; `GitObservationReceipt::NotInTree` is tree-local for its
//! exact immutable `tree_id`; and a complete worktree-scope receipt may support
//! absence only inside its declared scope and start/end interval. Generation
//! `NotInGeneration` is limited to one complete captured generation.
//! Repository/global no-match or absence is legal only through
//! `SelectedAggregate` ... None of the local negative receipts can be widened
//! into generation or global absence."
//!
//! API surface follows `contracts/public-api-v11.json` `introduced_v11_atoms`,
//! NOT the `data-model.md` prose, because the atoms are what
//! `expected_graph.activation_rule` refuses on. See D9 in
//! `docs/reviews/SLICE3-RECON-FINDINGS-v11.md`. Refusals are matched on
//! `kind()`; the four kind names are identical in both documents.

use symforge::protocol::format::claim_provenance::{
    AtomicAuthority, Claim, ClaimInput, DiskObservationReceipt, GenerationAuthority,
    GitObservationReceipt, ObservationTime, OperationKind, OperationReceipt, SourceRefusal,
    SourceRefusalKind, WorktreeObservationScope, WorktreeScopeObservationReceipt,
};
use symforge::protocol::format::claim_provenance::{ObservationLease, PhysicalRootLease};

// ── Fixtures ───────────────────────────────────────────────────────────────
// Local to this file, following the Slice 2 oracle files: each pins its own
// fixtures rather than sharing a support crate, so a fixture change cannot
// silently retarget an unrelated oracle.

fn observed_now() -> ObservationTime {
    ObservationTime::fresh()
}

/// A lease that owns one physical root, which is what every observation
/// receipt requires before it can be constructed at all.
fn a_binding_on(root: &str) -> ObservationLease {
    ObservationLease::for_test_root(PhysicalRootLease::for_test_root(root))
}

fn a_second_root(root: &str) -> ObservationLease {
    a_binding_on(root)
}

fn a_missing_path(
    lease: &ObservationLease,
    path: &str,
    observed_at: ObservationTime,
) -> DiskObservationReceipt {
    lease
        .observe_missing_path(path, observed_at)
        .expect("a lease that owns its root may record a missing path")
}

fn a_complete_worktree_scan(
    lease: &ObservationLease,
    scope: &str,
) -> WorktreeScopeObservationReceipt {
    lease
        .complete_scope_scan(WorktreeObservationScope::beneath(scope))
        .expect("a complete traversal seals a scope receipt")
}

fn a_git_tree_miss(tree_id: &str, path: &str) -> GitObservationReceipt {
    GitObservationReceipt::not_in_tree(tree_id, path)
}

fn an_admitted_generation(lease: &ObservationLease) -> GenerationAuthority {
    lease
        .admit_generation()
        .expect("a lease that owns its root may capture a complete generation")
}

fn an_operation() -> OperationReceipt {
    OperationReceipt::for_test(OperationKind::SearchText)
}

/// Derive one claim from two leases, which is the seam where root
/// compatibility is decided.
fn derive_across(
    left: &ObservationLease,
    right: &ObservationLease,
) -> Result<Claim<()>, SourceRefusal> {
    Claim::derive(
        an_operation(),
        [
            ClaimInput::Authority(AtomicAuthority::Generation(an_admitted_generation(left))),
            ClaimInput::Authority(AtomicAuthority::Generation(an_admitted_generation(right))),
        ],
        (),
    )
}

// ── Disk observation: path-local, and only at its own instant ───────────────

#[test]
fn a_missing_path_proves_only_that_path_at_that_moment() {
    let binding = a_binding_on("root-a");
    let observed_at = observed_now();
    let receipt = a_missing_path(&binding, "src/gone.rs", observed_at);

    assert!(
        receipt.proves_path_local_absence(),
        "a PathMissing receipt must prove the absence it actually observed"
    );
    assert_eq!(
        receipt.observed_at(),
        observed_at,
        "the proof is pinned to the instant of observation, not to read time"
    );
    assert_eq!(
        receipt.path(),
        "src/gone.rs",
        "the proof names the exact path it observed"
    );
}

#[test]
fn a_missing_path_never_proves_generation_absence() {
    let binding = a_binding_on("root-a");
    let receipt = a_missing_path(&binding, "src/gone.rs", observed_now());

    assert!(
        !receipt.proves_generation_absence(),
        "a path-local miss must not widen into 'absent from the generation'"
    );
    assert!(
        !receipt.proves_repository_absence(),
        "a path-local miss must not widen into 'absent from the repository'"
    );
}

// ── Worktree scope: sealed scope and sealed interval ────────────────────────

#[test]
fn a_complete_worktree_scan_proves_absence_only_inside_its_own_scope() {
    let binding = a_binding_on("root-a");
    let scan = a_complete_worktree_scan(&binding, "src/");

    assert!(
        scan.proves_absence_within_scope("src/gone.rs"),
        "a COMPLETE scan proves absence inside the scope it sealed"
    );
    assert!(
        !scan.proves_absence_within_scope("docs/gone.md"),
        "a scan of src/ proves nothing about docs/"
    );
    assert!(
        !scan.proves_generation_absence(),
        "a sealed scope is not a generation"
    );
    assert!(
        !scan.proves_repository_absence(),
        "a sealed scope is not the repository"
    );
}

#[test]
fn a_worktree_scan_proves_nothing_outside_its_observation_interval() {
    let binding = a_binding_on("root-a");
    let scan = a_complete_worktree_scan(&binding, "src/");
    let cut = scan.observation_cut();

    assert!(
        !scan.proves_absence_after(cut.end_seq()),
        "absence is claimed only up to the sealed end of the interval"
    );
    assert!(
        !scan.proves_absence_before(cut.start_seq()),
        "absence is claimed only from the sealed start of the interval"
    );
}

// ── Git observation: tree-local, for one immutable tree ─────────────────────

#[test]
fn a_git_tree_miss_proves_absence_only_for_that_exact_tree() {
    let miss = a_git_tree_miss("tree-1", "src/gone.rs");

    assert!(
        miss.proves_absence_in_tree("tree-1"),
        "NotInTree proves non-membership of the tree it names"
    );
    assert!(
        !miss.proves_absence_in_tree("tree-2"),
        "a different tree is a different immutable object and is not covered"
    );
    assert!(
        !miss.proves_generation_absence(),
        "a git tree is not the generation"
    );
    assert!(
        !miss.proves_repository_absence(),
        "one tree is not the whole repository across all history"
    );
}

// ── The closed property, stated once over every local negative ─────────────

#[test]
fn no_local_negative_receipt_can_be_widened_to_repository_absence() {
    let binding = a_binding_on("root-a");
    let locals: Vec<AtomicAuthority> = vec![
        AtomicAuthority::DiskObservation(a_missing_path(&binding, "src/gone.rs", observed_now())),
        AtomicAuthority::WorktreeScopeObservation(a_complete_worktree_scan(&binding, "src/")),
        AtomicAuthority::GitObservation(a_git_tree_miss("tree-1", "src/gone.rs")),
    ];

    for authority in &locals {
        assert!(
            !authority.proves_repository_absence(),
            "{} must not prove repository-wide absence",
            authority.kind_name()
        );
        assert!(
            !authority.proves_generation_absence(),
            "{} must not prove generation-wide absence",
            authority.kind_name()
        );
    }

    // Anti-vacuity: the loop really did examine three distinct authorities.
    let mut kinds: Vec<&'static str> = locals.iter().map(|a| a.kind_name()).collect();
    kinds.sort_unstable();
    kinds.dedup();
    assert_eq!(
        kinds.len(),
        3,
        "control: three DISTINCT local negative authorities were checked"
    );
}

// ── Root compatibility ─────────────────────────────────────────────────────

#[test]
fn a_derivation_across_two_roots_is_refused_rather_than_composed() {
    let left = a_binding_on("root-a");
    let right = a_second_root("root-b");
    let refusal = derive_across(&left, &right)
        .expect_err("inputs bound to different physical roots must not compose");

    assert_eq!(
        refusal.kind(),
        SourceRefusalKind::SourceUnavailable,
        "a mixed-root derivation is refused as unavailable, never silently joined"
    );
}

#[test]
fn a_derivation_within_one_root_is_admitted() {
    // GREEN-CONTROL for the rule above: the refusal is about root MISMATCH,
    // not about derivation being refused in general.
    let binding = a_binding_on("root-a");
    let claim = derive_across(&binding, &binding).expect("one root must compose");

    assert_eq!(
        claim.provenance().authority_count(),
        2,
        "both inputs are retained in the provenance, not collapsed"
    );
}

// ── A failed pure observation refuses; it does not invalidate the source ───

#[test]
fn a_failed_observation_refuses_without_disturbing_the_current_generation() {
    let binding = a_binding_on("root-a");
    let generation = an_admitted_generation(&binding);
    let before = generation.identity();

    let refusal = a_missing_path(&binding, "src/gone.rs", observed_now())
        .into_failed_read(an_operation())
        .expect_err("a failed pure observation returns a typed refusal");

    assert_eq!(
        refusal.kind(),
        SourceRefusalKind::SourceUnavailable,
        "the failure is reported as typed refusal rather than as absence"
    );
    assert_eq!(
        generation.identity(),
        before,
        "a pure observation failure must not invalidate the Current generation; \
         only the observer seam may do that, on its own independent evidence"
    );
}

// ── The one authority that MAY prove global absence ────────────────────────

#[test]
fn only_a_selected_aggregate_may_prove_repository_wide_absence() {
    let binding = a_binding_on("root-a");
    let aggregate = an_admitted_generation(&binding).into_selected_aggregate();

    assert!(
        aggregate.proves_repository_absence(),
        "GREEN-CONTROL: SelectedAggregate is the ONE constructor for global absence, \
         so the negative assertions above are about scope and not about a type that \
         can never prove anything"
    );
}
