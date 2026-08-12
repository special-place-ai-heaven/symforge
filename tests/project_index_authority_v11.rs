//! Feature 020 V11 Slice 1 authority oracles (T022).
//!
//! Every rejection case in this file also asserts that the *accepting* path
//! still works, in the same test. A guard that refuses everything is not a
//! guard, and Slice 0 already produced three controls that passed for reasons
//! unrelated to the property under test. Pairing the negative with the positive
//! is what makes each negative load-bearing.

use std::sync::Arc;

use symforge::index_lifecycle::authority::{
    AuthorityRefusal, BindingAuthority, CandidateAuthority, CurrentPublication, GenerationIdentity,
    MutationGrantInput, ObserverToken, PhaseName, Provenance, PublicationIdentity,
    SnapshotIdentity, SourceRuntime,
};
use symforge::index_lifecycle::mutation::{
    NoSideEffectProof, PermitDrainSignal, SourceMutationPermit, Termination,
};
use symforge::index_lifecycle::physical_root::PhysicalRootLease;
use symforge::index_lifecycle::transition::{self, TransitionKind, TransitionStep};

/// A source that is `Current` on a fresh root, plus the lease that root is on.
fn current_source() -> (SourceRuntime, Arc<PhysicalRootLease>, PublicationIdentity) {
    let lease = Arc::new(PhysicalRootLease::take(std::env::temp_dir()));
    let binding = BindingAuthority::bind(lease.identity());
    let publication = CurrentPublication::promote(binding, ObserverToken::fresh());
    let identity = publication.publication();
    (SourceRuntime::current(publication), lease, identity)
}

#[test]
fn grant_requires_the_exact_live_current_publication() {
    let (mut runtime, _lease, live) = current_source();

    // Negative: a publication identity that was never live is refused.
    let stranger = PublicationIdentity::fresh();
    let refusal = runtime
        .request_mutation_grant(MutationGrantInput::LiveCurrent(stranger))
        .expect_err("a publication that is not live must not grant");
    assert_eq!(
        refusal,
        AuthorityRefusal::PublicationIdentityMismatch {
            presented: stranger,
            live,
        }
    );
    assert_eq!(
        runtime.mutation_epoch().get(),
        0,
        "refusal advanced the epoch"
    );
    assert_eq!(runtime.permits_issued(), 0, "refusal recorded a permit");
    assert_eq!(
        runtime.phase(),
        PhaseName::Current,
        "refusal moved the phase"
    );

    // Positive: the exact live publication grants, so the negative above is
    // rejecting provenance rather than rejecting everything.
    let grant = runtime
        .request_mutation_grant(MutationGrantInput::LiveCurrent(live))
        .expect("the exact live Current publication must grant");
    assert_eq!(grant.authority().publication(), live);
    assert_eq!(runtime.permits_issued(), 1);
}

#[test]
fn grant_provenance_matrix_accepts_only_a_live_current_publication() {
    // Non-Current phases refuse regardless of what is presented, and leave no
    // epoch or permit trace behind.
    let non_current: Vec<(PhaseName, SourceRuntime)> = vec![
        (PhaseName::Loading, SourceRuntime::loading()),
        (
            PhaseName::Refreshing,
            SourceRuntime::refreshing(
                BindingAuthority::bind(PhysicalRootLease::take(std::env::temp_dir()).identity()),
                GenerationIdentity::fresh(),
            ),
        ),
        (
            PhaseName::Blocked,
            SourceRuntime::blocked(Some(GenerationIdentity::fresh())),
        ),
        (
            PhaseName::Stopping,
            SourceRuntime::stopping(Some(GenerationIdentity::fresh())),
        ),
    ];

    for (expected_phase, mut runtime) in non_current {
        let refusal = runtime
            .request_mutation_grant(MutationGrantInput::LiveCurrent(PublicationIdentity::fresh()))
            .expect_err("a non-Current source must not grant a mutation");
        assert_eq!(
            refusal,
            AuthorityRefusal::PhaseNotCurrent {
                phase: expected_phase
            }
        );
        assert_eq!(runtime.mutation_epoch().get(), 0);
        assert_eq!(runtime.permits_issued(), 0);
        assert_eq!(runtime.phase(), expected_phase);
    }

    // Non-Current *provenances* refuse even while the source is genuinely
    // Current, so the refusal is about what was presented, not about the phase.
    let lease = Arc::new(PhysicalRootLease::take(std::env::temp_dir()));
    let binding = BindingAuthority::bind(lease.identity());
    let candidate = CandidateAuthority::open(binding.clone(), ObserverToken::fresh());

    let inputs = vec![
        (
            Provenance::Candidate,
            MutationGrantInput::Candidate(candidate.identity()),
        ),
        (
            Provenance::Snapshot,
            MutationGrantInput::Snapshot(SnapshotIdentity::fresh()),
        ),
        (
            Provenance::RetainedGeneration,
            MutationGrantInput::RetainedGeneration(GenerationIdentity::fresh()),
        ),
        (
            Provenance::StalePublication,
            MutationGrantInput::StalePublication(PublicationIdentity::fresh()),
        ),
    ];

    for (expected, input) in inputs {
        let publication = CurrentPublication::promote(binding.clone(), ObserverToken::fresh());
        let live = publication.publication();
        let mut runtime = SourceRuntime::current(publication);

        let refusal = runtime
            .request_mutation_grant(input)
            .expect_err("only a live Current publication may grant");
        assert_eq!(
            refusal,
            AuthorityRefusal::ProvenanceNotLiveCurrent {
                provenance: expected
            }
        );
        assert_eq!(runtime.mutation_epoch().get(), 0);
        assert_eq!(runtime.permits_issued(), 0);
        assert_eq!(runtime.phase(), PhaseName::Current);

        // Same source, same instant: the live publication does grant.
        runtime
            .request_mutation_grant(MutationGrantInput::LiveCurrent(live))
            .expect("the live Current publication must still grant");
    }
}

#[test]
fn strict_queryability_is_closed_over_retained_generations() {
    // Loading retains nothing; only Current is queryable.
    let loading = SourceRuntime::loading();
    assert_eq!(loading.retained_generation(), None);
    assert_eq!(loading.live_publication(), None);

    // Refreshing retains exactly one, and it is not queryable.
    let lease = PhysicalRootLease::take(std::env::temp_dir());
    let binding = BindingAuthority::bind(lease.identity());
    let retained = GenerationIdentity::fresh();
    let refreshing = SourceRuntime::refreshing(binding.clone(), retained);
    assert_eq!(refreshing.retained_generation(), Some(retained));
    assert_eq!(refreshing.retained_binding(), Some(&binding));
    assert_eq!(
        refreshing.live_publication(),
        None,
        "a retained generation must not be queryable"
    );

    // Blocked and Stopping may retain zero or one.
    assert_eq!(SourceRuntime::blocked(None).retained_generation(), None);
    let accounted = GenerationIdentity::fresh();
    assert_eq!(
        SourceRuntime::stopping(Some(accounted)).retained_generation(),
        Some(accounted)
    );
    assert_eq!(
        SourceRuntime::stopping(Some(accounted)).live_publication(),
        None
    );
}

#[test]
fn granting_publishes_non_current_before_the_permit_exists() {
    let (mut runtime, lease, live) = current_source();

    let grant = runtime
        .request_mutation_grant(MutationGrantInput::LiveCurrent(live))
        .expect("live Current must grant");

    // The source is already non-queryable at the moment the grant exists, and
    // the generation it retains is exactly the one it stopped serving.
    assert_eq!(runtime.phase(), PhaseName::Refreshing);
    assert_eq!(
        runtime.retained_generation(),
        Some(grant.authority().generation())
    );
    assert_eq!(runtime.live_publication(), None);
    assert_eq!(runtime.mutation_epoch().get(), 1);
    assert_eq!(
        grant.published_non_current().epoch(),
        runtime.mutation_epoch()
    );

    let drain = Arc::new(PermitDrainSignal::new());
    let published = grant.published_non_current().publication();
    let mut permit = SourceMutationPermit::grant(grant, Arc::clone(&lease), Arc::clone(&drain))
        .expect("a grant matching its lease must produce a permit");

    // The permit carries the exact publication that made the source
    // non-queryable, rather than merely asserting that one happened.
    assert_eq!(permit.published_non_current().publication(), published);
    assert_eq!(runtime.published_identity(), Some(published));

    permit
        .start_side_effect()
        .expect("a permit whose source published non-Current may act");
}

#[test]
fn a_grant_cannot_be_paired_with_a_lease_on_another_root() {
    let (mut runtime, lease_a, live) = current_source();
    let lease_b = Arc::new(PhysicalRootLease::take(std::env::temp_dir()));

    let grant = runtime
        .request_mutation_grant(MutationGrantInput::LiveCurrent(live))
        .expect("live Current must grant");

    // Negative: the authority names root A, so pairing it with root B refuses.
    let refusal = SourceMutationPermit::grant(
        grant,
        Arc::clone(&lease_b),
        Arc::new(PermitDrainSignal::new()),
    )
    .expect_err("an authority for root A must not pair with a lease on root B");
    assert_eq!(refusal, AuthorityRefusal::WholeAuthorityMismatch);

    // Positive: the same shape with the matching lease succeeds, so the refusal
    // above is validating the pairing rather than refusing all pairings.
    let (mut runtime, lease, live) = current_source();
    let grant = runtime
        .request_mutation_grant(MutationGrantInput::LiveCurrent(live))
        .expect("live Current must grant");
    SourceMutationPermit::grant(
        grant,
        Arc::clone(&lease),
        Arc::new(PermitDrainSignal::new()),
    )
    .expect("an authority paired with its own lease must produce a permit");
    drop(lease_a);
}

#[test]
fn a_permit_is_terminal_once_it_ends() {
    let (mut runtime, lease, live) = current_source();
    let grant = runtime
        .request_mutation_grant(MutationGrantInput::LiveCurrent(live))
        .expect("live Current must grant");
    let drain = Arc::new(PermitDrainSignal::new());
    let mut permit = SourceMutationPermit::grant(grant, Arc::clone(&lease), Arc::clone(&drain))
        .expect("grant must produce a permit");

    // Positive: the declared terminal path works once.
    let ticket = permit
        .no_side_effect(NoSideEffectProof::observed())
        .expect("a granted permit may terminate with no side effect");
    assert_eq!(ticket.termination(), Termination::NoSideEffect);
    assert!(permit.is_terminal());
    assert_eq!(drain.termination(), Some(Termination::NoSideEffect));

    // Negative: it cannot be driven again afterwards.
    assert_eq!(
        permit
            .start_side_effect()
            .expect_err("a terminal permit must refuse"),
        AuthorityRefusal::PermitAlreadyTerminal
    );
    assert_eq!(
        permit
            .no_side_effect(NoSideEffectProof::observed())
            .expect_err("a terminal permit must refuse a second termination"),
        AuthorityRefusal::PermitAlreadyTerminal
    );
}

#[test]
fn dropping_a_permit_reports_drained_rather_than_stranding_the_source() {
    let (mut runtime, lease, live) = current_source();
    let grant = runtime
        .request_mutation_grant(MutationGrantInput::LiveCurrent(live))
        .expect("live Current must grant");
    let drain = Arc::new(PermitDrainSignal::new());
    let permit = SourceMutationPermit::grant(grant, Arc::clone(&lease), Arc::clone(&drain))
        .expect("grant must produce a permit");

    assert!(!drain.has_ended(), "a live permit must not report ended");
    drop(permit);
    assert!(drain.has_ended(), "a dropped permit must report ended");
    assert_eq!(drain.termination(), Some(Termination::Drained));
}

#[test]
fn a_root_a_permit_cannot_write_after_root_b_is_installed() {
    let (mut runtime, lease_a, live) = current_source();
    let grant = runtime
        .request_mutation_grant(MutationGrantInput::LiveCurrent(live))
        .expect("live Current must grant");
    let drain = Arc::new(PermitDrainSignal::new());
    let mut permit = SourceMutationPermit::grant(grant, Arc::clone(&lease_a), Arc::clone(&drain))
        .expect("grant must produce a permit");

    // Positive: while root A is installed, the permit may act.
    permit
        .start_side_effect()
        .expect("a permit on the installed root may act");
    permit
        .no_side_effect(NoSideEffectProof::observed())
        .expect("permit terminates");

    // Install root B through the writer-validated transition.
    let lease_b = Arc::new(PhysicalRootLease::take(std::env::temp_dir()));
    let receipt = transition::apply(
        &mut runtime,
        TransitionKind::PhysicalRootReplacement,
        &lease_a,
        BindingAuthority::bind(lease_b.identity()),
        ObserverToken::fresh(),
        Some(&drain),
    )
    .expect("a drained source may install a new root");
    assert_eq!(
        receipt.steps(),
        &[
            TransitionStep::Freeze,
            TransitionStep::Drain,
            TransitionStep::Install
        ],
        "install must not precede freeze and drain"
    );

    // Negative: a permit still pinned to root A can no longer act.
    let live_b = runtime
        .live_publication()
        .expect("the source is Current on root B")
        .publication();
    let grant_b = runtime
        .request_mutation_grant(MutationGrantInput::LiveCurrent(live_b))
        .expect("root B grants");
    let stale = SourceMutationPermit::grant(
        grant_b,
        Arc::clone(&lease_a),
        Arc::new(PermitDrainSignal::new()),
    )
    .expect_err("a lease on the replaced root must not accept root B's authority");
    assert_eq!(stale, AuthorityRefusal::WholeAuthorityMismatch);
    assert!(!lease_a.is_live(), "installing root B must revoke root A");
    assert!(lease_b.is_live(), "root B must be installed");
}

#[test]
fn the_non_current_proof_names_the_publication_the_source_actually_stored() {
    let (mut runtime, _lease, live) = current_source();
    let grant = runtime
        .request_mutation_grant(MutationGrantInput::LiveCurrent(live))
        .expect("live Current must grant");

    // The proof must not name a publication identity that nothing published.
    assert_eq!(
        runtime.published_identity(),
        Some(grant.published_non_current().publication()),
        "the proof named a publication the source never stored"
    );
    assert_ne!(
        grant.published_non_current().publication(),
        live,
        "freezing must publish a new identity, not reuse the Current one"
    );
}

#[test]
fn the_mutation_epoch_never_rewinds_across_a_transition() {
    let (mut runtime, lease_a, live) = current_source();

    // Burn an epoch by granting and draining a permit.
    let grant = runtime
        .request_mutation_grant(MutationGrantInput::LiveCurrent(live))
        .expect("live Current must grant");
    let drain = Arc::new(PermitDrainSignal::new());
    drop(
        SourceMutationPermit::grant(grant, Arc::clone(&lease_a), Arc::clone(&drain))
            .expect("grant must produce a permit"),
    );
    let before = runtime.mutation_epoch();
    let permits_before = runtime.permits_issued();
    assert!(before.get() >= 1);

    let lease_b = Arc::new(PhysicalRootLease::take(std::env::temp_dir()));
    transition::apply(
        &mut runtime,
        TransitionKind::Rebind,
        &lease_a,
        BindingAuthority::bind(lease_b.identity()),
        ObserverToken::fresh(),
        Some(&drain),
    )
    .expect("a drained source may transition");

    // A transition that reset the epoch would let a stale authority compare
    // equal to a later one.
    assert!(
        runtime.mutation_epoch() > before,
        "epoch rewound across a transition: {:?} -> {:?}",
        before,
        runtime.mutation_epoch()
    );
    assert_eq!(
        runtime.permits_issued(),
        permits_before,
        "a transition must not discard the permit record"
    );
}

#[test]
fn a_transition_refuses_to_install_over_a_live_permit() {
    let (mut runtime, lease_a, live) = current_source();
    let grant = runtime
        .request_mutation_grant(MutationGrantInput::LiveCurrent(live))
        .expect("live Current must grant");
    let drain = Arc::new(PermitDrainSignal::new());
    let permit = SourceMutationPermit::grant(grant, Arc::clone(&lease_a), Arc::clone(&drain))
        .expect("grant must produce a permit");

    // Negative: the permit is outstanding, so Install must not happen.
    let lease_b = Arc::new(PhysicalRootLease::take(std::env::temp_dir()));
    let refusal = transition::apply(
        &mut runtime,
        TransitionKind::Rebind,
        &lease_a,
        BindingAuthority::bind(lease_b.identity()),
        ObserverToken::fresh(),
        Some(&drain),
    )
    .expect_err("a transition must not install over a live permit");
    assert_eq!(refusal, AuthorityRefusal::OutstandingPermit);
    assert!(lease_a.is_live(), "a refused transition must not revoke");

    // Positive: once the permit ends, the same transition proceeds.
    drop(permit);
    transition::apply(
        &mut runtime,
        TransitionKind::Rebind,
        &lease_a,
        BindingAuthority::bind(lease_b.identity()),
        ObserverToken::fresh(),
        Some(&drain),
    )
    .expect("a drained source may transition");
    assert!(!lease_a.is_live());
}
