//! Feature 020 V11 Slice 4 snapshot-migration oracles (T057).
//!
//! RED-first for the pair T057+T065: authored and observed failing against
//! `todo!()` seams before any machinery existed. Every rejection case asserts
//! the accepting path in the same test — a store that quarantines everything
//! satisfies a lone rejection assertion perfectly.

use std::cell::Cell;
use std::collections::BTreeMap;

use symforge::live_index::index_lifecycle::snapshot::{
    BindingClass, GitVisibility, RestoreOutcome, SnapshotRefusal, SnapshotSeed, SnapshotStore,
    seed_digest,
};

// ── fixtures ───────────────────────────────────────────────────────────────

/// The runtime secret canary the frozen T057 case list forbids from ever
/// entering snapshots, quarantine metadata, receipts, or diagnostics.
const SECRET_CANARY: &[u8] = b"CANARY-9f2c-DO-NOT-PERSIST";

fn healthy_seed(count: u64) -> SnapshotSeed {
    let entries: Vec<(u64, u64)> = (1..=count).map(|id| (id, 100 + id)).collect();
    SnapshotSeed {
        version: 10,
        declared_len: count * 16,
        root_digest: seed_digest(&entries),
        entries,
        opaque_note: b"ordinary v10 residue".to_vec(),
    }
}

fn identity_decode(entry: &(u64, u64)) -> (u64, u64) {
    *entry
}

fn contains_canary(haystack: &str) -> bool {
    haystack.contains(std::str::from_utf8(SECRET_CANARY).unwrap())
}

// ── the frozen-named oracle ────────────────────────────────────────────────

/// TEST-SNAPSHOT (T057). The name is pinned by
/// `contracts/lifecycle-oracle-traceability-v11.md` as a `planned_exact`
/// target; do not rename it without amending that contract.
///
/// A seed is a HINT, never authority: only complete current-process proof
/// promotes anything. One unproven entry rejects the WHOLE seed — nothing
/// partial, nothing stale, and the rebuild fallback is latched. The
/// positive control proves an all-proven seed promotes everything.
#[test]
fn snapshot_seed_requires_complete_current_proof() {
    // One unproven entry: nothing promotes.
    let mut store = SnapshotStore::new();
    let outcome = store
        .restore(healthy_seed(3), 1_024, identity_decode, |source, _| {
            source != 2
        })
        .expect("an in-capacity seed is admitted to proof");
    assert_eq!(outcome, RestoreOutcome::SeedRejected { unproven: 1 });
    assert!(
        store.current().is_empty(),
        "a partially proven seed promoted something"
    );
    assert!(
        store.rebuild_required(),
        "a rejected seed must latch the rebuild fallback"
    );

    // Positive control: complete proof promotes all.
    let mut proven = SnapshotStore::new();
    let outcome = proven
        .restore(healthy_seed(3), 1_024, identity_decode, |_, _| true)
        .expect("an in-capacity seed is admitted to proof");
    assert_eq!(outcome, RestoreOutcome::Promoted { sources: 3 });
    assert_eq!(
        proven.current(),
        BTreeMap::from([(1, 101), (2, 102), (3, 103)]),
        "a completely proven seed promotes exactly its entries"
    );
    assert!(!proven.rebuild_required());
}

// ── pre-decode capacity ────────────────────────────────────────────────────

/// The capacity check happens BEFORE decoding: a seed declaring more than
/// the limit refuses with zero decode calls — untrusted bytes never get to
/// spend our cycles first.
#[test]
fn pre_decode_capacity_refuses_before_any_decode() {
    let mut store = SnapshotStore::new();
    let decodes = Cell::new(0_u32);

    let mut oversized = healthy_seed(2);
    oversized.declared_len = 1_000_000;
    assert_eq!(
        store
            .restore(
                oversized,
                1_024,
                |entry| {
                    decodes.set(decodes.get() + 1);
                    *entry
                },
                |_, _| true,
            )
            .unwrap_err(),
        SnapshotRefusal::SeedBeyondCapacity {
            declared: 1_000_000,
            limit: 1_024,
        }
    );
    assert_eq!(decodes.get(), 0, "an over-capacity seed was decoded anyway");

    // Positive control: within capacity, every entry decodes exactly once.
    store
        .restore(
            healthy_seed(2),
            1_024,
            |entry| {
                decodes.set(decodes.get() + 1);
                *entry
            },
            |_, _| true,
        )
        .expect("within capacity");
    assert_eq!(decodes.get(), 2);
}

// ── quarantine and rollback ────────────────────────────────────────────────

/// A root-digest mismatch quarantines the seed with its rollback payload
/// preserved byte-intact, latches the rebuild fallback, and promotes
/// nothing.
#[test]
fn root_digest_mismatch_quarantines_with_rollback_preserved() {
    let mut store = SnapshotStore::new();
    let mut corrupt = healthy_seed(3);
    corrupt.root_digest ^= 0xdead_beef;
    let original = corrupt.clone();

    let outcome = store
        .restore(corrupt, 1_024, identity_decode, |_, _| true)
        .expect("integrity failures quarantine, they do not refuse");
    let RestoreOutcome::Quarantined { id } = outcome else {
        panic!("a digest mismatch must quarantine, got {outcome:?}");
    };
    assert!(store.current().is_empty());
    assert!(store.rebuild_required());

    let metadata = store.quarantine_metadata();
    assert_eq!(metadata.len(), 1);
    assert_eq!(metadata[0].entry_count, 3);
    assert_ne!(metadata[0].declared_digest, metadata[0].computed_digest);

    // Rollback: the original seed comes back byte-intact, exactly once.
    assert_eq!(store.rollback(id), Some(original));
    assert_eq!(
        store.rollback(id),
        None,
        "a rollback payload is handed out once"
    );
}

// ── namespace isolation and concurrent writers ─────────────────────────────

/// The V11 store never touches the V10 namespace: restores and quarantines
/// leave every V10 byte identical, and a V10 writer landing DURING the V11
/// era never becomes Current without proof.
#[test]
fn the_v11_namespace_is_isolated_and_v10_writers_cannot_reach_current() {
    let mut store = SnapshotStore::new();
    store.v10_write(b"legacy snapshot one".to_vec());
    let before = store.v10_namespace();

    store
        .restore(healthy_seed(2), 1_024, identity_decode, |_, _| true)
        .expect("restore succeeds");
    let mut corrupt = healthy_seed(1);
    corrupt.root_digest ^= 1;
    store
        .restore(corrupt, 1_024, identity_decode, |_, _| true)
        .expect("quarantine path");

    assert_eq!(
        store.v10_namespace(),
        before,
        "V11 restore or quarantine wrote into the V10 namespace"
    );

    // A concurrent V10 writer during the V11 era: recorded in ITS namespace,
    // absent from Current.
    store.v10_write(b"late v10 writer".to_vec());
    assert_eq!(store.v10_namespace().len(), 2);
    assert_eq!(
        store.current(),
        BTreeMap::from([(1, 101), (2, 102)]),
        "an unproven late V10 write leaked into Current"
    );
}

// ── the secret canary ──────────────────────────────────────────────────────

/// Runtime secret-canary bytes never enter V11 snapshots, quarantine
/// metadata, receipts, or diagnostics — while the quarantined ORIGINAL is
/// still preserved intact for rollback, which is exactly the difference
/// between retention and disclosure.
#[test]
fn secret_canary_bytes_never_persist_in_v11_surfaces() {
    let mut store = SnapshotStore::new();
    let mut secret_bearing = healthy_seed(2);
    secret_bearing.opaque_note = SECRET_CANARY.to_vec();
    secret_bearing.root_digest ^= 7; // force the quarantine path too
    let original = secret_bearing.clone();

    let RestoreOutcome::Quarantined { id } = store
        .restore(secret_bearing, 1_024, identity_decode, |_, _| true)
        .expect("quarantine path")
    else {
        panic!("expected quarantine");
    };

    let metadata_view = format!("{:?}", store.quarantine_metadata());
    assert!(
        !contains_canary(&metadata_view),
        "the canary reached quarantine metadata"
    );
    assert!(
        metadata_view.contains("opaque_len"),
        "metadata must still describe the opaque payload by size"
    );
    let diagnostics_view = store.diagnostics().join("\n");
    assert!(
        !contains_canary(&diagnostics_view),
        "the canary reached diagnostics"
    );
    let current_view = format!("{:?}", store.current());
    assert!(
        !contains_canary(&current_view),
        "the canary reached the V11 state"
    );

    let receipt = store
        .export_team_artifact(
            BindingClass::NormalWritable,
            GitVisibility::AlreadyTracked,
            b"artifact".to_vec(),
        )
        .expect("a normal writable binding exports");
    assert!(
        !contains_canary(&format!("{receipt:?}")),
        "the canary reached a receipt"
    );

    // Retention is not disclosure: rollback still returns the original.
    assert_eq!(store.rollback(id), Some(original));
}

// ── FR-051: the team-artifact matrix ───────────────────────────────────────

/// The export receipt discloses EXACTLY one of the four frozen git
/// visibility states, and shareability is never inferred when git
/// visibility cannot be established.
#[test]
fn team_artifact_export_discloses_exactly_one_visibility_state() {
    let mut store = SnapshotStore::new();
    for (offset, visibility) in GitVisibility::ALL.into_iter().enumerate() {
        let receipt = store
            .export_team_artifact(
                BindingClass::NormalWritable,
                visibility,
                vec![0_u8; offset + 1],
            )
            .expect("a normal writable binding exports");
        assert_eq!(
            receipt.visibility, visibility,
            "the receipt must disclose its exact state"
        );
        match visibility {
            GitVisibility::GitVisibilityUnavailable => assert_eq!(
                receipt.shareability(),
                None,
                "shareability must never be inferred without git visibility"
            ),
            _ => assert!(receipt.shareability().is_some()),
        }
    }
    assert_eq!(store.team_artifacts(), 4);
}

/// Explicit-protected, read-only, user-local-only, and memory-only bindings
/// refuse BEFORE any artifact mutation — and the `.gitattributes` companion
/// write is a sealed negative: it belongs to the mutation-permit path.
#[test]
fn non_writable_bindings_refuse_before_any_mutation() {
    let mut store = SnapshotStore::new();
    for binding in [
        BindingClass::ExplicitProtected,
        BindingClass::ReadOnly,
        BindingClass::UserLocalOnly,
        BindingClass::MemoryOnly,
    ] {
        assert_eq!(
            store
                .export_team_artifact(binding, GitVisibility::AlreadyTracked, b"a".to_vec())
                .unwrap_err(),
            SnapshotRefusal::BindingRefusesExport(binding)
        );
    }
    assert_eq!(
        store.team_artifacts(),
        0,
        "a refused export persisted anyway"
    );

    // Sealed negative: the companion .gitattributes write is permit-path
    // work, refused here unconditionally — even for a writable binding.
    assert_eq!(
        store.export_with_gitattributes_change(
            BindingClass::NormalWritable,
            GitVisibility::IgnoredForceAddRequired,
        ),
        SnapshotRefusal::GitattributesRequiresMutationPermit
    );

    // Positive control: the writable binding still exports the artifact
    // itself, persistence-only.
    store
        .export_team_artifact(
            BindingClass::NormalWritable,
            GitVisibility::UntrackedVisible,
            b"artifact".to_vec(),
        )
        .expect("persistence-only export works");
    assert_eq!(store.team_artifacts(), 1);
}
