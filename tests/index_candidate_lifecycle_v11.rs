//! Feature 020 V11 Slice 4 candidate-pipeline oracles (T053).
//!
//! RED-first for the pair T053+T059+T060: these tests were authored and
//! observed failing against `todo!()` seams before any machinery existed.
//! Every rejection case asserts the accepting path in the same test — a
//! pipeline that refuses everything satisfies a lone negative perfectly.

use std::ffi::OsString;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use symforge::domain::index::{CatalogPath, MetadataOnlyReason};
use symforge::live_index::index_lifecycle::candidate::{
    CandidateSource, CapabilityCertificate, EntryDisposition, IsolatedCandidate,
    ProjectArtifactRoot, ProjectArtifacts, PromotionRefusal, SourceContentToken, SourceId,
    SourceObservation,
};
use symforge::live_index::index_lifecycle::capacity::ProcessCapacityPool;
use symforge::live_index::index_lifecycle::supervisor::{
    AttemptDisposition, ClassifiedFailure, SourceSupervisor,
};

// ── fixtures ───────────────────────────────────────────────────────────────

fn utf8_path(rel: &str) -> CatalogPath {
    CatalogPath {
        public_id: format!("pid-utf8-{rel}"),
        normalized_utf8: Some(rel.to_string()),
    }
}

fn content_source(id: u64, rel: &str, token: u64) -> CandidateSource {
    CandidateSource {
        id: SourceId(id),
        observation: SourceObservation::Content {
            path: utf8_path(rel),
            token: SourceContentToken(token),
            bytes: 64,
        },
    }
}

fn failed_source(id: u64, cause: ClassifiedFailure) -> CandidateSource {
    CandidateSource {
        id: SourceId(id),
        observation: SourceObservation::Failed(cause),
    }
}

/// Two native identities that COLLIDE under lossy display conversion, the
/// same shape the frozen pattern base
/// (`discovery::tests::metadata_first_scout::non_utf8_path_is_opaque_catalog_only_without_lossy_collision`)
/// proves at the scout seam.
fn colliding_native_pair() -> (OsString, OsString) {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStringExt;
        let mut first: Vec<u16> = "src/we".encode_utf16().collect();
        let mut second = first.clone();
        first.push(0xD800);
        second.push(0xD801);
        let tail: Vec<u16> = "ird.rs".encode_utf16().collect();
        first.extend(&tail);
        second.extend(&tail);
        (OsString::from_wide(&first), OsString::from_wide(&second))
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        let mut first = b"src/we".to_vec();
        let mut second = first.clone();
        first.push(0xFF);
        second.push(0xFE);
        first.extend_from_slice(b"ird.rs");
        second.extend_from_slice(b"ird.rs");
        (OsString::from_vec(first), OsString::from_vec(second))
    }
}

/// A stable public identity per native spelling, distinct whenever the native
/// encoding units differ — the property the scout's minting guarantees and
/// the pipeline must PRESERVE, which is what this suite proves.
fn public_id_for(native: &std::ffi::OsStr) -> String {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        let units: Vec<u16> = native.encode_wide().collect();
        format!("pid-native-{units:x?}")
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        format!("pid-native-{:x?}", native.as_bytes())
    }
}

fn opaque_source(id: u64, native: &std::ffi::OsStr) -> CandidateSource {
    CandidateSource {
        id: SourceId(id),
        observation: SourceObservation::MetadataOnly {
            path: CatalogPath {
                public_id: public_id_for(native),
                normalized_utf8: None,
            },
            reason: MetadataOnlyReason::UnsupportedPathEncoding,
        },
    }
}

fn stamp_derive(source: &CandidateSource) -> u64 {
    match &source.observation {
        SourceObservation::Content { token, .. } => token.0.wrapping_mul(31),
        other => panic!("derive called for a non-content observation: {other:?}"),
    }
}

/// Commit a healthy full candidate over `sources`; panics on any refusal.
fn commit_full(
    pool: &Arc<ProcessCapacityPool>,
    owner: symforge::live_index::index_lifecycle::capacity::OwnerIdentity,
    supervisor: &SourceSupervisor,
    root: &ProjectArtifactRoot,
    sources: Vec<CandidateSource>,
) -> Arc<ProjectArtifacts> {
    let attempt = supervisor.begin_attempt();
    let candidate = IsolatedCandidate::prepare_full(pool, owner, &attempt, sources, stamp_derive)
        .expect("capacity headroom exists");
    candidate.commit(root).expect("healthy candidate promotes")
}

// ── the closed promotion matrix ────────────────────────────────────────────

/// TEST-CANDIDATE (T053). The name is pinned by
/// `contracts/lifecycle-oracle-traceability-v11.md` as a `planned_exact`
/// target; do not rename it without amending that contract.
///
/// The matrix is CLOSED: `Unreadable`, `UnstableDuringRead`,
/// `AbortedCircuitBreaker`, `ParseFailed` (`ParseStatus::Failed`),
/// `UnknownOrdering`, `TruncatedRequiredDerivation`, and `PartialParse` each
/// block promotion — only their own candidate's, never a sibling's — and a
/// blocked candidate leaves no trace: the root is untouched, the capacity
/// charge is refunded, and the discard is accounted with its exact cause.
#[test]
fn closed_candidate_promotion_matrix() {
    let pool = ProcessCapacityPool::new();
    let owner = pool.root(1_000_000);

    for cause in ClassifiedFailure::ALL {
        let supervisor = SourceSupervisor::new();
        let root = ProjectArtifactRoot::empty();
        let baseline = commit_full(
            &pool,
            owner,
            &supervisor,
            &root,
            vec![content_source(1, "a.rs", 10)],
        );

        let attempt = supervisor.begin_attempt();
        let candidate = IsolatedCandidate::prepare_full(
            &pool,
            owner,
            &attempt,
            vec![content_source(1, "a.rs", 11), failed_source(2, cause)],
            stamp_derive,
        )
        .expect("capacity headroom exists");
        assert_eq!(
            candidate.classified_failure(),
            Some(cause),
            "the build must classify its terminal cause"
        );

        let refusal = candidate
            .commit(&root)
            .expect_err("a classified candidate must not promote");
        assert_eq!(refusal, PromotionRefusal::Failure(cause));
        assert!(
            Arc::ptr_eq(&root.load(), &baseline),
            "a refused candidate mutated the published root ({cause:?})"
        );
        assert_eq!(
            pool.charged(owner),
            0,
            "a refused candidate leaked its capacity charge ({cause:?})"
        );
        assert!(
            supervisor
                .attempt_records()
                .iter()
                .any(|record| record.disposition == AttemptDisposition::Discarded(cause)),
            "the discard was not accounted with its cause ({cause:?})"
        );
        assert_eq!(
            supervisor.committed_generations(),
            1,
            "only the baseline committed"
        );
    }
}

/// TEST-OPAQUE-PATH (T053). The name is pinned by
/// `contracts/lifecycle-oracle-traceability-v11.md` as a `planned_exact`
/// target; do not rename it without amending that contract.
///
/// Two native identities whose lossy displays COLLIDE must survive candidate,
/// manifest, and promotion as DISTINCT stable identities: catalog-only, zero
/// content probes, and no lossy spelling persisted anywhere in the promoted
/// manifest. The scout half of the chain is the frozen pattern base in
/// `src/discovery/mod.rs`; this oracle owns the pipeline half.
#[test]
fn opaque_non_utf8_path_identity_is_lossless() {
    let (first, second) = colliding_native_pair();
    assert_eq!(
        first.to_string_lossy(),
        second.to_string_lossy(),
        "fixture must prove two native identities collide under lossy conversion"
    );
    assert_ne!(
        first, second,
        "the native identities themselves are distinct"
    );
    let lossy = first.to_string_lossy().into_owned();

    let pool = ProcessCapacityPool::new();
    let owner = pool.root(1_000_000);
    let supervisor = SourceSupervisor::new();
    let root = ProjectArtifactRoot::empty();

    let opaque_a = opaque_source(1, &first);
    let opaque_b = opaque_source(2, &second);
    let expected_a = match &opaque_a.observation {
        SourceObservation::MetadataOnly { path, .. } => path.clone(),
        _ => unreachable!(),
    };
    let expected_b = match &opaque_b.observation {
        SourceObservation::MetadataOnly { path, .. } => path.clone(),
        _ => unreachable!(),
    };
    assert_ne!(expected_a.public_id, expected_b.public_id);

    let probes = AtomicUsize::new(0);
    let attempt = supervisor.begin_attempt();
    let candidate = IsolatedCandidate::prepare_full(
        &pool,
        owner,
        &attempt,
        vec![opaque_a, opaque_b, content_source(3, "normal.rs", 7)],
        |source| {
            probes.fetch_add(1, Ordering::SeqCst);
            stamp_derive(source)
        },
    )
    .expect("capacity headroom exists");
    let published = candidate
        .commit(&root)
        .expect("opaque paths must remain catalogable");

    assert_eq!(
        probes.load(Ordering::SeqCst),
        1,
        "a catalog-only entry was content-probed"
    );

    for (id, expected) in [(SourceId(1), &expected_a), (SourceId(2), &expected_b)] {
        let artifacts = published
            .sources
            .get(&id)
            .unwrap_or_else(|| panic!("opaque source {id:?} missing from the promoted root"));
        let entry = artifacts
            .manifest
            .entries
            .iter()
            .find(|entry| entry.path == *expected)
            .unwrap_or_else(|| panic!("opaque source {id:?} lost its exact identity"));
        assert_eq!(
            entry.disposition,
            EntryDisposition::MetadataOnly {
                reason: MetadataOnlyReason::UnsupportedPathEncoding
            }
        );
        assert!(
            entry.path.normalized_utf8.is_none(),
            "an opaque path grew a UTF-8 spelling"
        );
        assert_ne!(
            entry.path.public_id, lossy,
            "the promoted manifest persisted the lossy spelling as an identity"
        );
    }

    let promoted_a = &published.sources[&SourceId(1)].manifest;
    let promoted_b = &published.sources[&SourceId(2)].manifest;
    assert_ne!(
        promoted_a.entries[0].path.public_id, promoted_b.entries[0].path.public_id,
        "two colliding native identities merged during promotion"
    );
}

// ── isolation and publish-before-prune ─────────────────────────────────────

/// A building candidate is invisible: the published root is byte-identical
/// until the single commit point, including WHILE derivation runs.
#[test]
fn isolated_build_leaves_the_published_root_untouched() {
    let pool = ProcessCapacityPool::new();
    let owner = pool.root(1_000_000);
    let supervisor = SourceSupervisor::new();
    let root = ProjectArtifactRoot::empty();
    let baseline = commit_full(
        &pool,
        owner,
        &supervisor,
        &root,
        vec![content_source(1, "a.rs", 1)],
    );

    let attempt = supervisor.begin_attempt();
    let candidate = IsolatedCandidate::prepare_full(
        &pool,
        owner,
        &attempt,
        vec![content_source(1, "a.rs", 2), content_source(2, "b.rs", 1)],
        |source| {
            assert!(
                Arc::ptr_eq(&root.load(), &baseline),
                "the published root changed while a candidate was still building"
            );
            stamp_derive(source)
        },
    )
    .expect("capacity headroom exists");

    assert!(
        Arc::ptr_eq(&root.load(), &baseline),
        "a prepared, uncommitted candidate is already observable"
    );

    let published = candidate.commit(&root).expect("healthy candidate promotes");
    assert!(Arc::ptr_eq(&root.load(), &published));
    assert!(!Arc::ptr_eq(&published, &baseline));
}

/// Publication precedes pruning: a holder of the prior snapshot keeps reading
/// exactly what it captured, while new loads see the successor.
#[test]
fn publish_before_prune_keeps_prior_snapshots_readable() {
    let pool = ProcessCapacityPool::new();
    let owner = pool.root(1_000_000);
    let supervisor = SourceSupervisor::new();
    let root = ProjectArtifactRoot::empty();

    let prior = commit_full(
        &pool,
        owner,
        &supervisor,
        &root,
        vec![content_source(1, "a.rs", 1)],
    );
    let successor = commit_full(
        &pool,
        owner,
        &supervisor,
        &root,
        vec![content_source(1, "a.rs", 2), content_source(2, "b.rs", 1)],
    );

    assert!(Arc::ptr_eq(&root.load(), &successor));
    assert_eq!(
        prior.sources.len(),
        1,
        "the prior snapshot was pruned in place"
    );
    assert_eq!(
        prior.sources[&SourceId(1)].token,
        SourceContentToken(1),
        "the prior snapshot's content changed under its holder"
    );
    assert_eq!(successor.sources[&SourceId(1)].token, SourceContentToken(2));
}

// ── supervisor: supersession, cancellation, accounting ─────────────────────

/// A retry trigger supersedes the older attempt: its candidate can never
/// commit, the failure is accounted with its cause, and the successor commits.
#[test]
fn retry_supersession_blocks_the_older_attempt() {
    let pool = ProcessCapacityPool::new();
    let owner = pool.root(1_000_000);
    let supervisor = SourceSupervisor::new();
    let root = ProjectArtifactRoot::empty();

    let stale = supervisor.begin_attempt();
    let stale_candidate = IsolatedCandidate::prepare_full(
        &pool,
        owner,
        &stale,
        vec![content_source(1, "a.rs", 1)],
        stamp_derive,
    )
    .expect("capacity headroom exists");

    let fresh = supervisor.retry_trigger(ClassifiedFailure::UnstableDuringRead);
    assert!(
        stale.is_superseded(),
        "the retry trigger must supersede the live attempt"
    );
    assert!(!fresh.is_superseded());

    let refusal = stale_candidate
        .commit(&root)
        .expect_err("a superseded attempt's candidate must not commit");
    assert_eq!(refusal, PromotionRefusal::Superseded);
    assert_eq!(
        root.load().sources.len(),
        0,
        "a superseded candidate published anyway"
    );

    let fresh_candidate = IsolatedCandidate::prepare_full(
        &pool,
        owner,
        &fresh,
        vec![content_source(1, "a.rs", 2)],
        stamp_derive,
    )
    .expect("capacity headroom exists");
    fresh_candidate
        .commit(&root)
        .expect("the successor attempt commits");

    let records = supervisor.attempt_records();
    assert!(
        records.iter().any(|record| record.disposition
            == AttemptDisposition::Discarded(ClassifiedFailure::UnstableDuringRead)),
        "the retried failure lost its classified cause: {records:?}"
    );
    assert_eq!(supervisor.committed_generations(), 1);
}

/// Cancellation lands in the diagnostics ledger and never in the committed
/// ledger — the two are separate by construction.
#[test]
fn cancellation_is_accounted_and_never_committed() {
    let pool = ProcessCapacityPool::new();
    let owner = pool.root(1_000_000);
    let supervisor = SourceSupervisor::new();
    let root = ProjectArtifactRoot::empty();

    supervisor.begin_attempt().cancel();
    assert_eq!(supervisor.committed_generations(), 0);
    assert!(
        supervisor
            .attempt_records()
            .iter()
            .any(|record| record.disposition == AttemptDisposition::Cancelled),
        "the cancellation was not accounted"
    );

    commit_full(
        &pool,
        owner,
        &supervisor,
        &root,
        vec![content_source(1, "a.rs", 1)],
    );
    assert_eq!(supervisor.committed_generations(), 1);
    let records = supervisor.attempt_records();
    assert!(
        records
            .iter()
            .any(|record| record.disposition == AttemptDisposition::Committed),
        "the commit was not accounted: {records:?}"
    );
}

// ── metadata-terminal exclusions ───────────────────────────────────────────

/// Every metadata-terminal reason stays cataloged and stays EXCLUDED from
/// content derivation — the exclusion set is complete over the closed reason
/// list, and a content sibling in the same candidate still derives.
#[test]
fn metadata_terminal_exclusions_remain_complete() {
    let reasons: Vec<MetadataOnlyReason> = vec![
        MetadataOnlyReason::Lockfile,
        MetadataOnlyReason::Binary,
        MetadataOnlyReason::OversizedData,
        MetadataOnlyReason::GeneratedOrVendor,
        MetadataOnlyReason::SensitivePath {
            rule_id: "RULE-PATH".into(),
        },
        MetadataOnlyReason::SensitiveContent {
            rule_ids: vec!["RULE-CONTENT".into()],
            finding_count: 1,
        },
        MetadataOnlyReason::LfsPointer {
            declared_oid: None,
            declared_size: None,
        },
        MetadataOnlyReason::UnsupportedPathEncoding,
        MetadataOnlyReason::PathMetadataTooLarge,
        MetadataOnlyReason::UnsupportedTextEncoding,
    ];

    let pool = ProcessCapacityPool::new();
    let owner = pool.root(1_000_000);
    let supervisor = SourceSupervisor::new();
    let root = ProjectArtifactRoot::empty();

    let mut sources = Vec::new();
    for (offset, reason) in reasons.iter().enumerate() {
        let id = offset as u64 + 10;
        sources.push(CandidateSource {
            id: SourceId(id),
            observation: SourceObservation::MetadataOnly {
                path: utf8_path(&format!("meta-{id}.bin")),
                reason: reason.clone(),
            },
        });
    }
    sources.push(content_source(1, "content.rs", 5));

    let probes = AtomicUsize::new(0);
    let attempt = supervisor.begin_attempt();
    let candidate = IsolatedCandidate::prepare_full(&pool, owner, &attempt, sources, |source| {
        probes.fetch_add(1, Ordering::SeqCst);
        stamp_derive(source)
    })
    .expect("capacity headroom exists");
    let published = candidate
        .commit(&root)
        .expect("metadata-terminal entries must not block");

    assert_eq!(
        probes.load(Ordering::SeqCst),
        1,
        "a metadata-terminal entry was content-probed"
    );
    for (offset, reason) in reasons.iter().enumerate() {
        let id = SourceId(offset as u64 + 10);
        let artifacts = published
            .sources
            .get(&id)
            .unwrap_or_else(|| panic!("metadata-terminal source {id:?} fell out of the catalog"));
        assert_eq!(
            artifacts.manifest.entries[0].disposition,
            EntryDisposition::MetadataOnly {
                reason: reason.clone()
            },
            "the exclusion reason was not preserved for {id:?}"
        );
        assert!(
            artifacts.artifacts.is_empty(),
            "a metadata-terminal source grew content artifacts ({reason:?})"
        );
    }
    assert!(
        !published.sources[&SourceId(1)].artifacts.is_empty(),
        "the content sibling must still derive"
    );
}

// ── completeness and capability ────────────────────────────────────────────

/// A capability certificate is not completeness: offering one in place of a
/// complete required artifact set refuses, publishes nothing — and the same
/// candidate content promotes through the honest commit path.
#[test]
fn capability_certificates_cannot_authorize_partial_promotion() {
    let pool = ProcessCapacityPool::new();
    let owner = pool.root(1_000_000);
    let supervisor = SourceSupervisor::new();
    let root = ProjectArtifactRoot::empty();

    let attempt = supervisor.begin_attempt();
    let candidate = IsolatedCandidate::prepare_full(
        &pool,
        owner,
        &attempt,
        vec![content_source(1, "a.rs", 1)],
        stamp_derive,
    )
    .expect("capacity headroom exists");

    let refusal =
        candidate.promote_partial_with_capability(&root, CapabilityCertificate::for_test());
    assert_eq!(refusal, PromotionRefusal::CapabilityCannotAuthorize);
    assert_eq!(
        root.load().sources.len(),
        0,
        "a capability claim published state"
    );

    // Positive control: the identical content promotes through commit().
    commit_full(
        &pool,
        owner,
        &supervisor,
        &root,
        vec![content_source(1, "a.rs", 1)],
    );
    assert_eq!(root.load().sources.len(), 1);
}

/// A panicking derivation discards the candidate WHOLE: nothing publishes,
/// the capacity charge is refunded, and the panic is accounted.
#[test]
fn failed_and_panicked_candidates_are_discarded() {
    let pool = ProcessCapacityPool::new();
    let owner = pool.root(1_000_000);
    let supervisor = SourceSupervisor::new();
    let root = ProjectArtifactRoot::empty();
    let baseline = commit_full(
        &pool,
        owner,
        &supervisor,
        &root,
        vec![content_source(1, "a.rs", 1)],
    );

    let attempt = supervisor.begin_attempt();
    let candidate = IsolatedCandidate::prepare_full(
        &pool,
        owner,
        &attempt,
        vec![content_source(1, "a.rs", 2), content_source(2, "b.rs", 1)],
        |source| match &source.observation {
            SourceObservation::Content { token, .. } if token.0 == 1 => {
                panic!("injected derivation panic")
            }
            _ => stamp_derive(source),
        },
    )
    .expect("capacity headroom exists");

    let refusal = candidate
        .commit(&root)
        .expect_err("a panicked candidate must not promote");
    assert_eq!(refusal, PromotionRefusal::Panicked);
    assert!(
        Arc::ptr_eq(&root.load(), &baseline),
        "a panicked candidate published state"
    );
    assert_eq!(
        pool.charged(owner),
        0,
        "a panicked candidate leaked its capacity charge"
    );
    assert!(
        supervisor
            .attempt_records()
            .iter()
            .any(|record| record.disposition == AttemptDisposition::Panicked),
        "the panic was not accounted"
    );
    assert_eq!(
        supervisor.committed_generations(),
        1,
        "only the baseline committed"
    );
}

// ── deltas ─────────────────────────────────────────────────────────────────

/// A prepared delta exact-validates ONLY its changed source token and patches
/// the LATEST whole project root: membership and siblings that arrived after
/// it was prepared survive with their `Arc`s intact.
#[test]
fn delta_exact_validates_only_its_changed_token_and_spares_siblings() {
    let pool = ProcessCapacityPool::new();
    let owner = pool.root(1_000_000);
    let supervisor = SourceSupervisor::new();
    let root = ProjectArtifactRoot::empty();

    commit_full(
        &pool,
        owner,
        &supervisor,
        &root,
        vec![
            content_source(1, "x.rs", 1),
            content_source(2, "y.rs", 1),
            content_source(3, "z.rs", 1),
        ],
    );

    // Prepare the delta for Y against the CURRENT root...
    let delta_attempt = supervisor.begin_attempt();
    let delta = IsolatedCandidate::prepare_delta(
        &pool,
        owner,
        &delta_attempt,
        content_source(2, "y.rs", 2),
        Some(SourceContentToken(1)),
        stamp_derive,
    )
    .expect("capacity headroom exists");

    // ...then let NEWER membership land first: W arrives via its own delta.
    let w_attempt = supervisor.begin_attempt();
    let w_delta = IsolatedCandidate::prepare_delta(
        &pool,
        owner,
        &w_attempt,
        content_source(4, "w.rs", 1),
        None,
        stamp_derive,
    )
    .expect("capacity headroom exists");
    let with_w = w_delta
        .commit(&root)
        .expect("adding a new source is a valid delta");
    let x_before = Arc::clone(&with_w.sources[&SourceId(1)]);
    let z_before = Arc::clone(&with_w.sources[&SourceId(3)]);
    let w_before = Arc::clone(&with_w.sources[&SourceId(4)]);

    // The Y delta still commits: only Y's token is validated, and it patches
    // the latest root, not the one it was prepared against.
    let patched = delta
        .commit(&root)
        .expect("the delta's own token still matches");
    assert_eq!(patched.sources[&SourceId(2)].token, SourceContentToken(2));
    assert_eq!(
        patched.sources.len(),
        4,
        "newer membership was lost by the patch"
    );
    assert!(
        Arc::ptr_eq(&patched.sources[&SourceId(1)], &x_before),
        "an unrelated sibling was reallocated"
    );
    assert!(
        Arc::ptr_eq(&patched.sources[&SourceId(3)], &z_before),
        "an unrelated sibling was reallocated"
    );
    assert!(
        Arc::ptr_eq(&patched.sources[&SourceId(4)], &w_before),
        "a newer sibling was reallocated"
    );
}

/// Same-source drift refuses the stale delta — and a delta re-prepared
/// against the drifted token commits, which is the retry the contract names.
#[test]
fn same_source_drift_retries_or_aborts() {
    let pool = ProcessCapacityPool::new();
    let owner = pool.root(1_000_000);
    let supervisor = SourceSupervisor::new();
    let root = ProjectArtifactRoot::empty();

    commit_full(
        &pool,
        owner,
        &supervisor,
        &root,
        vec![content_source(1, "a.rs", 1)],
    );

    let stale_attempt = supervisor.begin_attempt();
    let stale_delta = IsolatedCandidate::prepare_delta(
        &pool,
        owner,
        &stale_attempt,
        content_source(1, "a.rs", 9),
        Some(SourceContentToken(1)),
        stamp_derive,
    )
    .expect("capacity headroom exists");

    // The source drifts to token 5 before the stale delta commits.
    let drift_attempt = supervisor.begin_attempt();
    IsolatedCandidate::prepare_delta(
        &pool,
        owner,
        &drift_attempt,
        content_source(1, "a.rs", 5),
        Some(SourceContentToken(1)),
        stamp_derive,
    )
    .expect("capacity headroom exists")
    .commit(&root)
    .expect("the drifting update itself is valid");

    let refusal = stale_delta
        .commit(&root)
        .expect_err("a drifted delta must not commit");
    assert_eq!(
        refusal,
        PromotionRefusal::SameSourceDrift {
            expected: Some(SourceContentToken(1)),
            found: Some(SourceContentToken(5)),
        }
    );
    assert_eq!(
        root.load().sources[&SourceId(1)].token,
        SourceContentToken(5)
    );

    // The retry: re-prepared against the drifted token, it commits.
    let retry_attempt = supervisor.begin_attempt();
    IsolatedCandidate::prepare_delta(
        &pool,
        owner,
        &retry_attempt,
        content_source(1, "a.rs", 9),
        Some(SourceContentToken(5)),
        stamp_derive,
    )
    .expect("capacity headroom exists")
    .commit(&root)
    .expect("the re-prepared delta commits");
    assert_eq!(
        root.load().sources[&SourceId(1)].token,
        SourceContentToken(9)
    );
}

// ── epochs ─────────────────────────────────────────────────────────────────

/// A numeric epoch is bookkeeping, never authority: presenting one without a
/// candidate refuses and publishes nothing.
#[test]
fn numeric_epochs_never_authorize_publication() {
    let root = ProjectArtifactRoot::empty();
    let before = root.load();
    assert_eq!(
        root.publish_claiming_epoch_only(u64::MAX),
        PromotionRefusal::EpochIsNotAuthority
    );
    assert!(
        Arc::ptr_eq(&root.load(), &before),
        "an epoch claim mutated the published root"
    );
}
