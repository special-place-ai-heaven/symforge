//! Feature 020 V11 Slice 2 registry oracles (T030).
//!
//! Every rejection asserts the accepting path in the same test. Slice 0 shipped
//! three controls that passed for reasons unrelated to the property under test,
//! and a registry that refuses everything satisfies a lone negative perfectly.

use symforge::live_index::index_lifecycle::authority::BindingAuthority;
use symforge::live_index::index_lifecycle::physical_root::PhysicalRootLease;
use symforge::live_index::index_lifecycle::registry::{
    ProjectKey, ProjectRegistry, RegistryRefusal, RootProtection, StatePlacement,
};

fn binding() -> BindingAuthority {
    BindingAuthority::bind(PhysicalRootLease::take(std::env::temp_dir()).identity())
}

/// TEST-REGISTRY (T030). The name is pinned by
/// `contracts/lifecycle-oracle-traceability-v11.md` as a `planned_exact` target;
/// do not rename it without amending that contract.
///
/// SC-019: an authorized protected root must reach `PendingProjectAdmission`
/// while selecting a placement that writes nothing beneath the source root.
/// Unauthorized access is refused, and so is a placement that would write there
/// even WITH authorization — authorization permits indexing, not writing.
#[test]
fn protected_membership_and_state_placement() {
    let registry = ProjectRegistry::new();

    // Negative: a protected root with no authorization is refused outright.
    let key = ProjectKey::new("protected-unauthorized");
    assert_eq!(
        registry
            .admit(
                key.clone(),
                binding(),
                RootProtection::Protected,
                false,
                StatePlacement::MemoryOnly,
            )
            .expect_err("an unauthorized protected root must be refused"),
        RegistryRefusal::ProtectedWithoutAuthorization
    );
    assert!(
        registry.pending_joiners(&key).is_none(),
        "a refused admission left a pending entry behind"
    );

    // Negative: authorization is not permission to write beneath the root.
    // Refusing rather than silently relocating is the point: a caller that asked
    // for project-local state must learn its request was not honoured.
    let key = ProjectKey::new("protected-project-local");
    assert_eq!(
        registry
            .admit(
                key.clone(),
                binding(),
                RootProtection::Protected,
                true,
                StatePlacement::ProjectLocal,
            )
            .expect_err("state must not be placed beneath a protected root"),
        RegistryRefusal::ProtectedWithoutAuthorization
    );

    // Positive: authorized, with a placement that writes nothing beneath the
    // source root, reaches pending admission.
    for placement in [StatePlacement::UserLocal, StatePlacement::MemoryOnly] {
        let key = ProjectKey::new(format!("protected-{placement:?}"));
        let slot = registry
            .admit(
                key.clone(),
                binding(),
                RootProtection::Protected,
                true,
                placement,
            )
            .expect("an authorized protected root with off-root state is admitted");
        assert_eq!(registry.pending_joiners(&key), Some(1));

        // Slice 2 never constructs Current: a pending admission is not queryable.
        assert_eq!(
            registry.live(&key).expect_err("pending is not live"),
            RegistryRefusal::StillPending
        );

        let live = registry.install(&key, None).expect("pending installs");
        assert_eq!(live.slot(), slot);
        assert_eq!(live.placement(), placement);
        assert!(registry.is_current(&key, slot));
    }

    // A normal root is unaffected by any of the above.
    let normal = ProjectKey::new("normal");
    registry
        .admit(
            normal.clone(),
            binding(),
            RootProtection::Normal,
            false,
            StatePlacement::ProjectLocal,
        )
        .expect("a normal root admits with project-local state");
    let live = registry.install(&normal, None).expect("installs");
    assert_eq!(live.placement(), StatePlacement::ProjectLocal);
}

/// Concurrent opens of one key join a single admission.
#[test]
fn concurrent_opens_join_one_admission() {
    let registry = ProjectRegistry::new();
    let key = ProjectKey::new("joined");

    let first = registry
        .admit(
            key.clone(),
            binding(),
            RootProtection::Normal,
            false,
            StatePlacement::ProjectLocal,
        )
        .expect("first open admits");
    let second = registry
        .admit(
            key.clone(),
            binding(),
            RootProtection::Normal,
            false,
            StatePlacement::ProjectLocal,
        )
        .expect("second open joins");

    assert_eq!(
        first, second,
        "two opens of one key produced two admissions"
    );
    assert_eq!(registry.pending_joiners(&key), Some(2));

    // A different key is genuinely separate, so the join above is about identity
    // rather than the registry collapsing everything into one slot.
    let other = ProjectKey::new("separate");
    let third = registry
        .admit(
            other,
            binding(),
            RootProtection::Normal,
            false,
            StatePlacement::ProjectLocal,
        )
        .expect("a different key admits separately");
    assert_ne!(first, third);
}

/// A stopped slot REFUSES to serve, rather than relying on the holder to ask.
///
/// This is the property the tombstone exists for. Today's production registry
/// hands out `Arc` clones that outlive map membership with nothing revoking
/// them, so a holder obtained before removal keeps operating on a project the
/// registry believes is gone. Rust cannot take the `Arc` back — so instead the
/// slot refuses every authority-conferring read once stopped, and a holder that
/// never thinks to check is refused anyway.
#[test]
fn a_stopped_slot_refuses_to_serve_a_holder_that_never_asked() {
    let registry = ProjectRegistry::new();
    let key = ProjectKey::new("stopped");
    registry
        .admit(
            key.clone(),
            binding(),
            RootProtection::Normal,
            false,
            StatePlacement::ProjectLocal,
        )
        .expect("admits");
    let held = registry.install(&key, None).expect("installs");
    let slot = held.slot();

    // Positive: while live, the handle serves.
    assert!(held.is_live());
    held.binding().expect("a live slot hands out its binding");
    held.capacity_owner()
        .expect("a live slot reports its owner");
    assert!(registry.is_current(&key, slot));

    let stopped = registry.stop(&key).expect("a live slot stops");
    assert_eq!(stopped, slot);

    // Negative: the SAME handle, still held, now refuses. No cooperation
    // required from the holder.
    assert!(!held.is_live());
    assert_eq!(
        held.binding()
            .expect_err("a stopped slot must refuse its binding"),
        RegistryRefusal::Tombstoned { slot }
    );
    assert_eq!(
        held.capacity_owner()
            .expect_err("a stopped slot must refuse its capacity owner"),
        RegistryRefusal::Tombstoned { slot }
    );

    // Diagnostics stay readable so an operator can see WHICH slot went stale.
    assert_eq!(held.slot(), slot);
    assert_eq!(held.key(), &key);
}

/// A retired identity is never revived, including by reopening the same key.
#[test]
fn reopening_a_key_mints_a_new_identity_and_never_revives_the_old_one() {
    let registry = ProjectRegistry::new();
    let key = ProjectKey::new("reopened");

    registry
        .admit(
            key.clone(),
            binding(),
            RootProtection::Normal,
            false,
            StatePlacement::ProjectLocal,
        )
        .expect("admits");
    let first = registry.install(&key, None).expect("installs");
    let first_slot = first.slot();
    registry.stop(&key).expect("stops");

    assert!(registry.is_tombstoned(first_slot));
    assert!(
        !registry.is_current(&key, first_slot),
        "a retired identity still reported as current"
    );

    // Reopening the same key produces a DIFFERENT identity.
    registry
        .admit(
            key.clone(),
            binding(),
            RootProtection::Normal,
            false,
            StatePlacement::ProjectLocal,
        )
        .expect("reopens");
    let second = registry.install(&key, None).expect("installs again");
    assert_ne!(
        second.slot(),
        first_slot,
        "a reopened key reused a retired identity"
    );
    assert!(second.is_live());
    assert!(registry.is_current(&key, second.slot()));

    // And the old identity stays retired even though its key is live again.
    assert!(
        !registry.is_current(&key, first_slot),
        "reopening a key revived a retired identity"
    );
    assert_eq!(
        first.binding().expect_err("the old handle stays refused"),
        RegistryRefusal::Tombstoned { slot: first_slot }
    );
}

/// A cancelled admission retires its identity and installs nothing.
#[test]
fn a_cancelled_admission_installs_nothing() {
    let registry = ProjectRegistry::new();
    let key = ProjectKey::new("cancelled");
    let slot = registry
        .admit(
            key.clone(),
            binding(),
            RootProtection::Normal,
            false,
            StatePlacement::ProjectLocal,
        )
        .expect("admits");

    assert_eq!(registry.cancel(&key).expect("cancels"), slot);
    assert!(registry.is_tombstoned(slot));
    assert_eq!(
        registry
            .install(&key, None)
            .expect_err("nothing to install"),
        RegistryRefusal::NotAdmitted
    );
    assert_eq!(
        registry.live(&key).expect_err("nothing is live"),
        RegistryRefusal::NotAdmitted
    );

    // Positive: a fresh admission after the cancel works, with a new identity.
    let reopened = registry
        .admit(
            key.clone(),
            binding(),
            RootProtection::Normal,
            false,
            StatePlacement::ProjectLocal,
        )
        .expect("admits again");
    assert_ne!(reopened, slot);
    registry.install(&key, None).expect("installs");
    assert_eq!(registry.tombstone_count(), 1);
}
