//! Feature 020 V11 Slice 2 registry oracles (T030).
//!
//! Every rejection asserts the accepting path in the same test. Slice 0 shipped
//! three controls that passed for reasons unrelated to the property under test,
//! and a registry that refuses everything satisfies a lone negative perfectly.

use symforge::live_index::index_lifecycle::authority::BindingAuthority;
use symforge::live_index::index_lifecycle::physical_root::PhysicalRootLease;
use symforge::live_index::index_lifecycle::registry::{
    ProjectKey, ProjectRegistry, RegistryRefusal, RootProtection, SlotIdentity, StatePlacement,
};

fn binding() -> BindingAuthority {
    BindingAuthority::bind(
        PhysicalRootLease::take(tempfile::tempdir().expect("root").keep()).identity(),
    )
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

/// T039: refusal and cancellation cannot construct `Current`, leak a slot,
/// double-refund, or release memory something still holds.
///
/// These are one test because they are one property — that a path which did not
/// complete leaves the process exactly as it found it. Splitting them would let
/// each pass while the combination still drifted.
#[test]
fn a_refused_or_cancelled_admission_leaves_nothing_behind() {
    use symforge::live_index::index_lifecycle::adapters::{self, AdapterRefusal};
    use symforge::live_index::index_lifecycle::process_runtime::{
        ProcessIndexRuntime, SurfaceKind,
    };

    let runtime = ProcessIndexRuntime::incarnate(10_000);
    let registry = ProjectRegistry::new();
    let owner = runtime
        .attach(SurfaceKind::Daemon, 4_000)
        .expect("the daemon surface attaches");
    let charged_before = runtime.ledger().charged(owner);
    let available_before = runtime.available();

    // A refused plan charges nothing and admits nothing.
    let refused = adapters::plan_admission(
        &runtime,
        SurfaceKind::Daemon,
        ProjectKey::new("protected"),
        RootProtection::Protected,
        false,
        StatePlacement::MemoryOnly,
    )
    .expect_err("an unauthorized protected root must be refused");
    assert_eq!(
        refused,
        AdapterRefusal::Registry(RegistryRefusal::ProtectedWithoutAuthorization)
    );
    assert_eq!(runtime.ledger().charged(owner), charged_before);
    assert_eq!(runtime.available(), available_before);
    assert_eq!(registry.tombstone_count(), 0);

    // A cancelled admission retires its identity, installs nothing, and never
    // becomes queryable.
    let key = ProjectKey::new("cancelled-clean");
    registry
        .admit(
            key.clone(),
            binding(),
            RootProtection::Normal,
            false,
            StatePlacement::ProjectLocal,
        )
        .expect("admits");
    let slot = registry.cancel(&key).expect("cancels");
    assert!(registry.is_tombstoned(slot));
    assert!(
        !registry.is_current(&key, slot),
        "a cancelled admission reported as current"
    );
    assert_eq!(
        registry.live(&key).expect_err("nothing is live"),
        RegistryRefusal::NotAdmitted
    );
    assert_eq!(runtime.ledger().charged(owner), charged_before);

    // A surface holding a live allocation cannot be detached: detaching would
    // return capacity to the process that the surface is still using.
    let held = runtime
        .ledger()
        .redeem(runtime.ledger().reserve(owner, 1_000).expect("headroom"))
        .expect("the runtime's own pool issued it");
    assert!(
        runtime.detach(SurfaceKind::Daemon).is_err(),
        "a surface with live allocations was detached"
    );
    assert_eq!(
        runtime.available(),
        available_before,
        "a refused detach returned capacity anyway"
    );

    // Positive: once drained, the detach returns exactly the promise, once.
    drop(held);
    let returned = runtime
        .detach(SurfaceKind::Daemon)
        .expect("a drained surface detaches");
    assert_eq!(returned, 4_000);
    assert_eq!(runtime.available(), 10_000);

    // And detaching twice does not return it a second time.
    assert!(
        runtime.detach(SurfaceKind::Daemon).is_err(),
        "a surface was detached twice"
    );
    assert_eq!(
        runtime.available(),
        10_000,
        "a second detach invented capacity"
    );
    assert_eq!(runtime.ledger().unknown_refunds(), 0);
}

/// Slice 2 must never construct or claim lifecycle `Current`.
///
/// The spec states this as a constraint on the slice, so it is asserted rather
/// than assumed: every path that could plausibly yield a queryable generation is
/// exercised and none does.
#[test]
fn slice_two_never_constructs_a_queryable_generation() {
    let registry = ProjectRegistry::new();
    let key = ProjectKey::new("never-current");

    registry
        .admit(
            key.clone(),
            binding(),
            RootProtection::Normal,
            false,
            StatePlacement::ProjectLocal,
        )
        .expect("admits");

    // Pending is not live.
    assert_eq!(
        registry.live(&key).expect_err("pending is not queryable"),
        RegistryRefusal::StillPending
    );

    // Installed yields a slot, and a slot is not a generation: it carries a
    // binding and a capacity owner, and there is no way to ask it for anything
    // queryable.
    let live = registry.install(&key, None).expect("installs");
    live.binding().expect("a live slot has a binding");
    assert_eq!(
        live.capacity_owner()
            .expect("a live slot reports its owner"),
        None,
        "no capacity owner was attached, so none should be reported"
    );

    // Stopping revokes it; nothing anywhere became queryable.
    registry.stop(&key).expect("stops");
    assert!(!live.is_live());
}

/// Concurrent opens of one key join a single admission.
///
/// Both opens name the SAME physical root, because that is what two opens of one
/// project are. Each mints its own `BindingAuthority` — `bind` gives a fresh
/// identity per call — so this also pins that a join compares roots rather than
/// binding identities. The earlier version handed the two opens two different
/// tempdirs, i.e. two different projects under one key, and passed.
#[test]
fn concurrent_opens_join_one_admission() {
    let registry = ProjectRegistry::new();
    let key = ProjectKey::new("joined");
    let root = PhysicalRootLease::take(tempfile::tempdir().expect("root").keep()).identity();

    let first = registry
        .admit(
            key.clone(),
            BindingAuthority::bind(root),
            RootProtection::Normal,
            false,
            StatePlacement::ProjectLocal,
        )
        .expect("first open admits");
    let second = registry
        .admit(
            key.clone(),
            BindingAuthority::bind(root),
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

/// A refusal must not consume what it refused to act on.
///
/// Found by adversarial review: all three transitions matched their expected
/// occupancy with `let Some(Occupancy::X(..)) = state.keys.remove(key) else`,
/// and `remove` runs before the pattern is tested. Refusing therefore dropped
/// the entry — a live slot evicted from the map with no revocation and no
/// tombstone, its handle still serving, which is the exact defect this module
/// exists to prevent. The comment above the first one even claimed a restore
/// that no line performed.
///
/// Each refusal below is paired with the accepting call that proves the
/// occupancy really did survive it.
#[test]
fn a_refused_transition_leaves_the_occupancy_it_refused_intact() {
    let registry = ProjectRegistry::new();

    // A live slot must survive install() and cancel(), which expect pending.
    let live_key = ProjectKey::new("live");
    registry
        .admit(
            live_key.clone(),
            binding(),
            RootProtection::Normal,
            false,
            StatePlacement::ProjectLocal,
        )
        .expect("ordinary admission");
    let live = registry.install(&live_key, None).expect("install pending");

    assert_eq!(
        registry.install(&live_key, None).expect_err("not pending"),
        RegistryRefusal::NotAdmitted
    );
    assert_eq!(
        registry.cancel(&live_key).expect_err("not pending"),
        RegistryRefusal::NotAdmitted
    );

    // Still live, still current, still handing out its binding, not tombstoned.
    assert!(live.is_live(), "a refused install evicted the live slot");
    assert!(registry.is_current(&live_key, live.slot()));
    assert!(live.binding().is_ok());

    // A pending admission must survive stop(), which expects live.
    let pending_key = ProjectKey::new("pending");
    let pending_slot = registry
        .admit(
            pending_key.clone(),
            binding(),
            RootProtection::Normal,
            false,
            StatePlacement::ProjectLocal,
        )
        .expect("ordinary admission");
    assert_eq!(
        registry.stop(&pending_key).expect_err("not live"),
        RegistryRefusal::NotAdmitted
    );

    // The accepting case: the admission survived, so it still installs, and
    // under the identity every joiner was already given.
    let installed = registry
        .install(&pending_key, None)
        .expect("a refused stop destroyed the pending admission");
    assert_eq!(installed.slot(), pending_slot);

    // Paired positives: each transition still works on the occupancy it expects.
    assert!(registry.stop(&live_key).is_ok());
    let cancel_key = ProjectKey::new("cancel");
    registry
        .admit(
            cancel_key.clone(),
            binding(),
            RootProtection::Normal,
            false,
            StatePlacement::ProjectLocal,
        )
        .expect("ordinary admission");
    assert!(registry.cancel(&cancel_key).is_ok());
}

/// Joining an admission must be joining the admission you asked for.
///
/// Found by adversarial review. Both join branches dropped the caller's binding,
/// protection and placement and returned `Ok`, so a caller that admitted a key
/// as protected with user-local state joined an occupancy admitted as ordinary
/// with project-local state and was told it succeeded — state written beneath a
/// root the second caller had declared protected, reported as success. That is
/// this repository's named recurring shape: reporting that a request was
/// honoured without observing that it was.
#[test]
fn a_join_that_disagrees_with_the_occupancy_is_refused() {
    let registry = ProjectRegistry::new();
    let key = ProjectKey::new("joined");
    let first = binding();

    let slot = registry
        .admit(
            key.clone(),
            first.clone(),
            RootProtection::Normal,
            false,
            StatePlacement::ProjectLocal,
        )
        .expect("ordinary admission");

    // A different placement for the same key is refused, not silently joined.
    assert_eq!(
        registry
            .admit(
                key.clone(),
                first.clone(),
                RootProtection::Normal,
                false,
                StatePlacement::UserLocal,
            )
            .expect_err("placements disagree"),
        RegistryRefusal::PlacementMismatch {
            joined: StatePlacement::ProjectLocal,
            presented: StatePlacement::UserLocal,
        }
    );

    // So is a different physical root: one key cannot be two roots. Compared on
    // the ROOT rather than the binding identity, because `bind` mints a fresh
    // identity per call and two legitimate concurrent opens of one path hold
    // different identities for the same root — comparing identities would refuse
    // the single-flight join this registry exists to provide, which is exactly
    // what `concurrent_opens_join_one_admission` caught.
    let other = binding();
    assert_eq!(
        registry
            .admit(
                key.clone(),
                other.clone(),
                RootProtection::Normal,
                false,
                StatePlacement::ProjectLocal,
            )
            .expect_err("roots disagree"),
        RegistryRefusal::RootMismatch {
            joined: first.physical_root(),
            presented: other.physical_root(),
        }
    );

    // Paired positive for that distinction: a DIFFERENT binding on the SAME root
    // joins, because it is the same project.
    let same_root = BindingAuthority::bind(first.physical_root());
    assert_ne!(same_root.identity(), first.identity());
    assert_eq!(
        registry
            .admit(
                key.clone(),
                same_root,
                RootProtection::Normal,
                false,
                StatePlacement::ProjectLocal,
            )
            .expect("a fresh binding on the same root joins"),
        slot
    );

    // Paired positive: an identical request still joins, and joins the SAME
    // admission — the refusals above are about disagreement, not about joining.
    assert_eq!(
        registry
            .admit(
                key.clone(),
                first.clone(),
                RootProtection::Normal,
                false,
                StatePlacement::ProjectLocal,
            )
            .expect("an identical request joins"),
        slot
    );
    assert_eq!(registry.pending_joiners(&key), Some(3));

    // And the same holds once installed, rather than only while pending.
    let live = registry.install(&key, None).expect("install pending");
    let other_root = other.physical_root();
    assert_eq!(
        registry
            .admit(
                key.clone(),
                other,
                RootProtection::Normal,
                false,
                StatePlacement::ProjectLocal,
            )
            .expect_err("roots disagree"),
        RegistryRefusal::RootMismatch {
            joined: first.physical_root(),
            presented: other_root,
        }
    );
    assert!(live.is_live());
}

/// An executed plan must execute the plan.
///
/// Found by adversarial review. `execute_plan` forwarded only the key and the
/// placement; `protection` and `authorized` were supplied fresh by the caller
/// and `plan.owner()` was never used at all — so the admission performed could
/// differ from the admission planned, and the capacity owner the whole of T034
/// exists to determine was computed and dropped. T038's value is that the
/// decision made now is the decision Slice 4 must make under real traffic.
#[test]
fn an_executed_plan_uses_the_decision_the_plan_recorded() {
    use symforge::live_index::index_lifecycle::adapters;
    use symforge::live_index::index_lifecycle::process_runtime::{
        ProcessIndexRuntime, SurfaceKind,
    };

    let runtime = ProcessIndexRuntime::incarnate(10_000);
    let owner = runtime
        .attach(SurfaceKind::Daemon, 4_000)
        .expect("the daemon surface attaches");
    let registry = ProjectRegistry::new();

    // An authorized protected root may index but must not place state beneath
    // itself, so the plan records UserLocal and touches nothing.
    let plan = adapters::plan_admission(
        &runtime,
        SurfaceKind::Daemon,
        ProjectKey::new("protected-planned"),
        RootProtection::Protected,
        true,
        StatePlacement::UserLocal,
    )
    .expect("an authorized protected root with user-local state plans");
    assert_eq!(plan.placement(), StatePlacement::UserLocal);
    assert_eq!(plan.protection(), RootProtection::Protected);
    assert!(plan.authorized());
    assert!(!plan.touches_source_root());
    assert_eq!(plan.owner(), owner);

    // Executing it returns the owner the plan chose, and the registry sees the
    // protection the plan recorded rather than whatever the caller says now.
    let (slot, charged_to) =
        adapters::execute_plan(&registry, &plan, binding()).expect("the plan executes");
    assert_eq!(
        charged_to, owner,
        "the plan's owner did not survive execution"
    );
    assert_eq!(
        registry.pending_joiners(plan.key()),
        Some(1),
        "the admission the plan named was not the admission performed"
    );
    let live = registry
        .install(plan.key(), Some(charged_to))
        .expect("install");
    assert_eq!(live.slot(), slot);
    assert_eq!(live.placement(), StatePlacement::UserLocal);
    assert_eq!(live.capacity_owner().expect("live slot"), Some(owner));
}

/// Two real threads racing one key must reach one admission.
///
/// Found by adversarial review: not one oracle in this slice spawned a thread,
/// so `concurrent_opens_join_one_admission` proved a sequential property under a
/// concurrent name, and a build that deleted the `Mutex` and used a bare
/// `HashMap` would have passed every one of them. The lock is the mechanism; a
/// test that never overlaps two calls does not pin that it stays.
#[test]
fn overlapping_admits_of_one_key_mint_one_identity() {
    use std::sync::Barrier;

    let registry = ProjectRegistry::new();
    let key = ProjectKey::new("raced");
    let root = PhysicalRootLease::take(tempfile::tempdir().expect("root").keep()).identity();

    // A barrier so both threads are inside `admit` at the same time rather than
    // merely dispatched at the same time.
    let gate = std::sync::Arc::new(Barrier::new(8));
    let slots: Vec<SlotIdentity> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..8)
            .map(|_| {
                let registry = std::sync::Arc::clone(&registry);
                let key = key.clone();
                let gate = std::sync::Arc::clone(&gate);
                scope.spawn(move || {
                    gate.wait();
                    registry
                        .admit(
                            key,
                            BindingAuthority::bind(root),
                            RootProtection::Normal,
                            false,
                            StatePlacement::ProjectLocal,
                        )
                        .expect("every racing open joins")
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().expect("thread"))
            .collect()
    });

    // One identity for all eight, and the registry counted all eight joiners.
    assert!(
        slots.windows(2).all(|pair| pair[0] == pair[1]),
        "racing opens of one key minted more than one identity: {slots:?}"
    );
    assert_eq!(registry.pending_joiners(&key), Some(8));

    // And it installs once, under that identity.
    let live = registry.install(&key, None).expect("install");
    assert_eq!(live.slot(), slots[0]);
}

/// D17 regression window: a stopped occupancy refuses BOTH `install` and
/// `live`, and re-admitting is what resolves it.
///
/// `admit_project_with_outcome` recovers from `install` refusing `NotAdmitted`
/// by asking `live`, on the reading that a concurrent opener must have
/// installed the slot. That reading has a third case. An admit that JOINS an
/// already-live occupancy holds no pending claim, so when the last session
/// closes and `stop` removes the live occupancy in that window, `install`
/// refuses because nothing is pending and `live` refuses because nothing is
/// live. Under contention that propagated out of `open_project_session`, which
/// D17 forbids.
///
/// This pins the shape rather than the race: the double refusal is reachable
/// deterministically, and a fresh admit clears it. The accepting control is in
/// the same test because a registry that refuses everything would satisfy the
/// negative half perfectly.
#[test]
fn a_stopped_occupancy_refuses_install_and_live_until_readmitted() {
    let registry = ProjectRegistry::new();
    let key = ProjectKey::new("d17-torn-down-occupancy");

    registry
        .admit(
            key.clone(),
            binding(),
            RootProtection::Normal,
            false,
            StatePlacement::MemoryOnly,
        )
        .expect("first admission is pending");
    let live = registry.install(&key, None).expect("pending installs");
    let first_slot = live.slot();
    drop(live);

    registry.stop(&key).expect("the live occupancy stops");

    // The window itself: neither pending nor live, so BOTH doors refuse.
    assert_eq!(
        registry
            .install(&key, None)
            .expect_err("a stopped occupancy has nothing pending to install"),
        RegistryRefusal::NotAdmitted
    );
    assert_eq!(
        registry
            .live(&key)
            .expect_err("a stopped occupancy has nothing live to hand back"),
        RegistryRefusal::NotAdmitted
    );

    // The accepting control, and the strategy the caller relies on: a torn-down
    // occupancy is a lost race, not a refusal, so re-admitting succeeds and
    // yields a DIFFERENT slot identity than the one that was stopped.
    registry
        .admit(
            key.clone(),
            binding(),
            RootProtection::Normal,
            false,
            StatePlacement::MemoryOnly,
        )
        .expect("re-admission after a stop is accepted");
    let readmitted = registry
        .install(&key, None)
        .expect("the re-admitted occupancy installs");
    assert_ne!(
        readmitted.slot(),
        first_slot,
        "a re-admission must mint a new slot identity, not resurrect the stopped one"
    );
}

/// D17 adjacent regression: after the live occupancy is stopped, a different
/// opener can recreate Pending before the original opener's `live` fallback.
/// That `StillPending` answer is also a retryable lost race.
#[test]
fn a_reappeared_pending_occupancy_is_joined_by_readmission() {
    let registry = ProjectRegistry::new();
    let key = ProjectKey::new("d17-reappeared-pending");
    let root = binding();

    registry
        .admit(
            key.clone(),
            root.clone(),
            RootProtection::Normal,
            false,
            StatePlacement::MemoryOnly,
        )
        .expect("first admission is pending");
    let live = registry.install(&key, None).expect("pending installs");
    let first_slot = live.slot();
    drop(live);

    registry.stop(&key).expect("the live occupancy stops");
    assert_eq!(
        registry
            .install(&key, None)
            .expect_err("the original opener has no pending claim"),
        RegistryRefusal::NotAdmitted
    );

    let pending = registry
        .admit(
            key.clone(),
            root.clone(),
            RootProtection::Normal,
            false,
            StatePlacement::MemoryOnly,
        )
        .expect("another opener recreates pending");
    assert_ne!(
        pending, first_slot,
        "the recreated pending admission must not resurrect the stopped slot"
    );
    assert_eq!(
        registry
            .live(&key)
            .expect_err("the recreated occupancy is not live yet"),
        RegistryRefusal::StillPending
    );

    let joined = registry
        .admit(
            key.clone(),
            root.clone(),
            RootProtection::Normal,
            false,
            StatePlacement::MemoryOnly,
        )
        .expect("re-admission joins the recreated pending occupancy");
    assert_eq!(
        joined, pending,
        "retrying must join the new pending occupancy"
    );
    assert_eq!(registry.pending_joiners(&key), Some(2));

    let installed = registry
        .install(&key, None)
        .expect("the recreated pending occupancy installs");
    assert_eq!(installed.slot(), pending);
}
