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

// ── T044: the authority CHOICE is explicit, and generation never reads disk ─

#[test]
fn generation_resolution_never_reads_unmatched_disk_bytes() {
    // T042's first clause, on T044's seam. A generation miss is a MISS — it
    // surfaces to the caller, who must then CHOOSE disk observation by name.
    // Before the split, the choice was implicit in which function a lane
    // reached for, which is how a lane meaning to serve published bytes could
    // silently backfill from disk.
    use symforge::protocol::format::claim_provenance::{
        GenerationResolution, observe_disk_beneath, resolve_generation_bytes,
    };

    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        dir.path().join("indexed.rs"),
        "pub fn published_anchor() {}
",
    )
    .expect("write indexed fixture");
    let shared = symforge::live_index::LiveIndex::load(dir.path()).expect("load index");
    let live = shared.read();

    // AFTER the load: the indexed file changes on disk, and a new file appears
    // that the generation never saw. Both are exactly what generation
    // resolution must not observe.
    std::fs::write(
        dir.path().join("indexed.rs"),
        "pub fn disk_only_anchor_a() {}
",
    )
    .expect("rewrite on disk");
    std::fs::write(
        dir.path().join("diskonly.rs"),
        "pub fn disk_only_anchor_b() {}
",
    )
    .expect("write unindexed fixture");

    // The generation serves the bytes it PUBLISHED, not the current disk.
    let GenerationResolution::Published(bytes) = resolve_generation_bytes(&live, "indexed.rs")
    else {
        panic!("an indexed file resolves from the generation");
    };
    let text = std::str::from_utf8(bytes).expect("published bytes are utf8");
    assert!(
        text.contains("published_anchor"),
        "generation resolution serves the published bytes"
    );
    assert!(
        !text.contains("disk_only_anchor_a"),
        "generation resolution must not observe the disk rewrite"
    );

    // The miss is a miss — never the disk bytes.
    assert!(
        matches!(
            resolve_generation_bytes(&live, "diskonly.rs"),
            GenerationResolution::NotInGeneration
        ),
        "a generation miss surfaces as a miss, not as disk content"
    );

    // GREEN-CONTROL: the disk-observation lane EXISTS and serves the current
    // bytes when chosen by name — the split is a choice, not a wall.
    let observed = observe_disk_beneath(&live, dir.path(), "diskonly.rs")
        .expect("a deliberate confined observation is admitted");
    assert!(
        std::str::from_utf8(&observed)
            .expect("observed bytes are utf8")
            .contains("disk_only_anchor_b"),
        "disk observation serves what is on disk right now"
    );
}

#[test]
fn a_disk_observation_is_confined_beneath_its_root() {
    use symforge::protocol::format::claim_provenance::observe_disk_beneath;

    let outside = tempfile::tempdir().expect("outside dir");
    let canary = ["escape", "-", "canary", "-", "bytes"].concat();
    std::fs::write(outside.path().join("secret.txt"), &canary).expect("write outside fixture");

    let root = tempfile::TempDir::new_in(outside.path()).expect("nested root");
    let shared = symforge::live_index::LiveIndex::load(root.path()).expect("load index");
    let live = shared.read();

    // A traversal component refuses BEFORE any read: the refusal must not
    // echo the escaped content.
    let refused = observe_disk_beneath(&live, root.path(), "../secret.txt")
        .expect_err("a path that escapes the root must refuse");
    assert!(
        !refused.contains(canary.as_str()),
        "the refusal must not carry the escaped bytes"
    );

    // An absolute path is an escape however it is spelled.
    let absolute = outside.path().join("secret.txt");
    let refused = observe_disk_beneath(&live, root.path(), absolute.to_str().expect("utf8 path"))
        .expect_err("an absolute path must refuse");
    assert!(!refused.contains(canary.as_str()));

    // GREEN-CONTROL: a plain relative path beneath the root is admitted.
    std::fs::write(root.path().join("inside.txt"), "inside-bytes").expect("write inside fixture");
    let observed =
        observe_disk_beneath(&live, root.path(), "inside.txt").expect("a beneath path is admitted");
    assert_eq!(observed, b"inside-bytes");
}

// ── Context acquisition: rebinds refuse rather than composing roots ────────

#[test]
fn a_rebind_between_input_acquisitions_is_refused() {
    use symforge::protocol::format::claim_provenance::acquire_claim_context;

    // Two inputs captured under two different roots is what a rebind between
    // acquisitions looks like from the context's side. CloseSource acts on
    // exactly one source, so the closed contract permits no cross-source
    // relation and the acquisition must refuse rather than compose.
    let before = a_binding_on("root-a");
    let after = a_binding_on("root-b");
    let refusal = acquire_claim_context(
        OperationReceipt::for_test(OperationKind::CloseSource),
        vec![
            before.context_input("project-a", "repo-1", None),
            after.context_input("project-a", "repo-1", None),
        ],
    )
    .expect_err("a root drift between acquisitions is a rebind and must refuse");

    assert_eq!(refusal.kind(), SourceRefusalKind::SourceUnavailable);
}

#[test]
fn a_cross_source_search_admits_two_roots_deliberately() {
    use symforge::protocol::format::claim_provenance::acquire_claim_context;

    // GREEN-CONTROL for the rebind refusal: search is the closed contract's
    // explicit cross-source relation, so two roots compose HERE and only here.
    let left = a_binding_on("root-a");
    let right = a_binding_on("root-b");
    let context = acquire_claim_context(
        OperationReceipt::for_test(OperationKind::SearchText),
        vec![
            left.context_input(
                "project-a",
                "repo-1",
                Some(left.current_query_lease().expect("lease")),
            ),
            right.context_input(
                "project-b",
                "repo-2",
                Some(right.current_query_lease().expect("lease")),
            ),
        ],
    )
    .expect("the contract explicitly permits a cross-source search");

    assert!(context.permitted_relationships().cross_source_permitted());
    assert_eq!(context.inputs().len(), 2);
}

#[test]
fn a_returned_context_retains_what_it_captured() {
    use symforge::protocol::format::claim_provenance::acquire_claim_context;

    // "A rebind after the complete context is returned does not trigger a
    // trailing live-state check; claims derived wholly from its retained
    // authorities remain valid." The retained half is falsifiable today: the
    // context reports exactly the roots and sources captured at acquisition.
    let lease = a_binding_on("root-a");
    let context = acquire_claim_context(
        OperationReceipt::for_test(OperationKind::CloseSource),
        vec![lease.context_input("project-a", "repo-1", None)],
    )
    .expect("one source, one root");

    let input = &context.inputs()[0];
    assert_eq!(input.root(), "root-a");
    assert_eq!(input.project_source(), "project-a");
    assert_eq!(input.repository_id(), "repo-1");
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
