//! Feature 020 V11 Slice 2 capacity oracles (T032).
//!
//! Every rejection case asserts the accepting path in the same test. A ledger
//! that refuses every reservation satisfies a lone negative assertion perfectly,
//! and Slice 0 already shipped three controls that passed for reasons unrelated
//! to the property under test.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

use symforge::live_index::index_lifecycle::capacity::{CapacityRefusal, ProcessCapacityPool};

/// TEST-CAPACITY (T032). The name is pinned by
/// `contracts/lifecycle-oracle-traceability-v11.md` as a `planned_exact` target;
/// do not rename it without amending that contract.
///
/// "Exact conservation" is the arithmetic identity
/// `charged == sum(bytes of live allocations)` at every observable moment, with
/// every charge refunded exactly once. The word that carries the weight is
/// PHYSICAL: the refund is owed when the allocation is actually dropped, not
/// when a caller remembers to release it, because a forgotten release is a
/// permanent leak of process capacity and "the caller should have released" is
/// not an accounting policy.
#[test]
fn capacity_is_conserved_until_physical_drop() {
    let ledger = ProcessCapacityPool::new();
    let root = ledger.root(1_000);

    // Charged on reserve, and the reservation is visible immediately.
    let grant = ledger.reserve(root, 400).expect("headroom exists");
    assert_eq!(ledger.charged(root), 400);
    assert_eq!(ledger.available(root), 600);
    assert_eq!(ledger.outstanding_charges(root), 1);

    // Redeeming does not change the charge: the bytes were already committed.
    let allocation = ledger.redeem(grant).expect("own grant");
    assert_eq!(ledger.charged(root), 400);
    assert_eq!(allocation.bytes(), 400);

    // Dropping WITHOUT releasing must still refund. This is the property the
    // test is named for.
    drop(allocation);
    assert_eq!(
        ledger.charged(root),
        0,
        "a dropped allocation leaked its charge"
    );
    assert_eq!(ledger.available(root), 1_000);
    assert_eq!(ledger.outstanding_charges(root), 0);

    // Explicit release refunds exactly the same way, and reports what it freed.
    let second = ledger
        .redeem(ledger.reserve(root, 250).expect("headroom exists"))
        .expect("own grant");
    assert_eq!(ledger.charged(root), 250);
    assert_eq!(second.release(), 250);
    assert_eq!(ledger.charged(root), 0);

    // And nothing was invented along the way: no refund named a charge the
    // ledger never issued.
    assert_eq!(
        ledger.unknown_refunds(),
        0,
        "a refund named a charge the ledger never issued"
    );
}

/// A grant authorizes exactly one allocation.
///
/// `CapacityGrant` is not `Clone` and `redeem` consumes it, so redeeming twice
/// is unrepresentable rather than merely discouraged. This asserts the
/// accounting consequence: two allocations require two reservations, and the
/// charge reflects both.
#[test]
fn one_grant_backs_exactly_one_allocation() {
    let ledger = ProcessCapacityPool::new();
    let root = ledger.root(1_000);

    let first = ledger
        .redeem(ledger.reserve(root, 100).expect("headroom"))
        .expect("own grant");
    let second = ledger
        .redeem(ledger.reserve(root, 100).expect("headroom"))
        .expect("own grant");
    assert_eq!(
        ledger.charged(root),
        200,
        "two allocations must charge twice"
    );
    assert_eq!(ledger.outstanding_charges(root), 2);
    assert_ne!(
        first.charge(),
        second.charge(),
        "two reservations shared one charge identity"
    );

    drop(first);
    assert_eq!(
        ledger.charged(root),
        100,
        "dropping one allocation refunded the other's charge too"
    );
    drop(second);
    assert_eq!(ledger.charged(root), 0);
}

/// Concurrent reserve/redeem/drop against a tight limit still conserves.
///
/// The ledger mutex makes a data race unrepresentable; this still proves the
/// accounting identity after real threads interleave grants, redemptions,
/// exhaustion refusals, and drops — a sequential loop never did.
#[test]
fn concurrent_reserve_and_drop_conserves_a_tight_limit() {
    let ledger = ProcessCapacityPool::new();
    let limit = 3;
    let root = ledger.root(limit);

    let successes = std::sync::Arc::new(AtomicUsize::new(0));
    let handles: Vec<_> = (0..12)
        .map(|_| {
            let ledger = ledger.clone();
            let successes = successes.clone();
            thread::spawn(move || match ledger.reserve(root, 1) {
                Ok(grant) => {
                    let permit = ledger.redeem(grant).expect("own grant");
                    drop(permit);
                    successes.fetch_add(1, Ordering::Relaxed);
                }
                Err(CapacityRefusal::Exhausted { requested, available }) => {
                    assert_eq!(requested, 1);
                    assert!(available <= limit);
                }
                Err(other) => panic!("unexpected refusal: {other:?}"),
            })
        })
        .collect();

    for handle in handles {
        handle.join().expect("capacity thread");
    }

    assert_eq!(ledger.charged(root), 0, "a concurrent drop leaked a charge");
    assert!(
        successes.load(Ordering::Relaxed) > 0,
        "every reserve failed; the positive control never ran"
    );
    assert_eq!(ledger.available(root), limit);
    assert_eq!(ledger.outstanding_charges(root), 0);
    assert_eq!(
        ledger.unknown_refunds(),
        0,
        "a concurrent refund named a charge the ledger never issued"
    );
}

/// Exhaustion refuses with the numbers, and does not charge on refusal.
#[test]
fn an_exhausted_owner_refuses_without_charging() {
    let ledger = ProcessCapacityPool::new();
    let root = ledger.root(500);
    let held = ledger
        .redeem(ledger.reserve(root, 400).expect("headroom"))
        .expect("own grant");

    // Negative: over-large request is refused with what was actually available.
    assert_eq!(
        ledger
            .reserve(root, 200)
            .expect_err("200 does not fit in 100"),
        CapacityRefusal::Exhausted {
            requested: 200,
            available: 100,
        }
    );
    assert_eq!(
        ledger.charged(root),
        400,
        "a refused reservation charged anyway"
    );
    assert_eq!(ledger.outstanding_charges(root), 1);

    // Positive: a request that fits still succeeds, so the refusal is about the
    // size rather than about the owner being closed.
    let fits = ledger.reserve(root, 100).expect("100 fits exactly");
    assert_eq!(ledger.charged(root), 500);
    drop(ledger.redeem(fits).expect("own grant"));
    drop(held);
    assert_eq!(ledger.charged(root), 0);
}

/// A child owner cannot promise more than its parent can back, and what it is
/// given is charged to the parent immediately.
#[test]
fn a_child_owner_is_backed_by_its_parent() {
    let ledger = ProcessCapacityPool::new();
    let root = ledger.root(1_000);

    // Negative: a child larger than the parent is refused.
    assert_eq!(
        ledger
            .child(root, 1_200)
            .expect_err("a child cannot exceed its parent"),
        CapacityRefusal::ExceedsParent {
            requested: 1_200,
            available: 1_000,
        }
    );
    assert_eq!(
        ledger.charged(root),
        0,
        "a refused child charged the parent"
    );

    // Positive: a child that fits is created, and the parent is charged for the
    // whole promise immediately -- capacity promised to a child is capacity the
    // parent can no longer promise elsewhere.
    let child = ledger.child(root, 600).expect("600 fits in 1000");
    assert_eq!(ledger.charged(root), 600);
    assert_eq!(ledger.available(root), 400);
    assert_eq!(ledger.available(child), 600);

    // A second child may only take what is left.
    assert!(ledger.child(root, 500).is_err());
    let sibling = ledger
        .child(root, 400)
        .expect("400 is exactly what remains");
    assert_eq!(ledger.available(root), 0);
    assert_eq!(ledger.available(sibling), 400);

    // Spending inside the child does not double-charge the parent.
    let inside = ledger
        .redeem(ledger.reserve(child, 500).expect("fits in the child"))
        .expect("own grant");
    assert_eq!(ledger.charged(child), 500);
    assert_eq!(
        ledger.charged(root),
        1_000,
        "spending inside a child re-charged its parent"
    );
    drop(inside);
    assert_eq!(ledger.charged(child), 0);
}

/// Releasing a child returns its whole promise to the parent, but only once the
/// child has actually stopped spending.
#[test]
fn a_child_cannot_be_released_while_it_is_still_spending() {
    let ledger = ProcessCapacityPool::new();
    let root = ledger.root(1_000);
    let child = ledger.child(root, 600).expect("600 fits");
    assert_eq!(ledger.available(root), 400);

    let inside = ledger
        .redeem(ledger.reserve(child, 200).expect("fits in the child"))
        .expect("own grant");

    // Negative: releasing now would refund the parent for capacity the child is
    // still spending, letting the process promise the same bytes twice.
    assert!(
        ledger.release_owner(child).is_err(),
        "a child with outstanding charges was released"
    );
    assert_eq!(
        ledger.available(root),
        400,
        "a refused release refunded the parent anyway"
    );

    // Positive: once the child has drained, the release returns the whole
    // promise -- so the refusal is about the outstanding charge, not a blanket
    // refusal to ever release.
    drop(inside);
    assert_eq!(
        ledger
            .release_owner(child)
            .expect("a drained child releases"),
        600
    );
    assert_eq!(
        ledger.available(root),
        1_000,
        "releasing a child did not return its promise to the parent"
    );
    assert_eq!(ledger.unknown_refunds(), 0);
}

/// A refund naming a charge the ledger never issued refunds nothing and is
/// counted, rather than silently inventing capacity.
#[test]
fn a_refund_for_an_unknown_charge_invents_nothing() {
    let ledger = ProcessCapacityPool::new();
    let root = ledger.root(1_000);
    let allocation = ledger
        .redeem(ledger.reserve(root, 300).expect("headroom"))
        .expect("own grant");

    // Release once: legitimate, refunds the real amount.
    assert_eq!(allocation.release(), 300);
    assert_eq!(ledger.charged(root), 0);
    assert_eq!(ledger.unknown_refunds(), 0);

    // A second allocation on a DIFFERENT ledger cannot refund against this one:
    // its charge is unknown here, so it must refund zero and be counted.
    let other = ProcessCapacityPool::new();
    let other_root = other.root(1_000);
    let foreign = other
        .redeem(other.reserve(other_root, 100).expect("headroom"))
        .expect("own grant");
    drop(foreign);
    assert_eq!(
        ledger.unknown_refunds(),
        0,
        "another ledger's refund reached this one"
    );
    assert_eq!(other.charged(other_root), 0);
}

/// Releasing an owner that still backs children would promise the same bytes
/// twice.
///
/// Found by adversarial review. A child's limit is charged to its parent at
/// `child()` time and recorded in `charged`, never in `outstanding`, so the
/// outstanding-only guard saw an owner with live children as perfectly drained:
/// `release_owner` returned its whole limit to the grandparent while its own
/// children kept spending against a limit nothing backed. The reviewer's
/// sequence promised 1500 bytes beneath a 1000-byte root.
#[test]
fn an_owner_that_still_backs_children_cannot_return_its_promise() {
    let ledger = ProcessCapacityPool::new();
    let root = ledger.root(1_000);
    let a = ledger.child(root, 600).expect("fits under the root");
    let b = ledger.child(a, 500).expect("fits under a");

    // The whole 600 is charged to the root the moment `a` exists.
    assert_eq!(ledger.charged(root), 600);
    assert_eq!(
        ledger.release_owner(a).expect_err("a still backs b"),
        CapacityRefusal::HasChildren { children: 1 }
    );
    // Nothing moved: the root is still backing the promise it made.
    assert_eq!(ledger.charged(root), 600);
    assert_eq!(ledger.available(root), 400);
    // And the over-promise the refusal prevents is still refused.
    assert!(ledger.child(root, 1_000).is_err());

    // Paired positive: release the leaf first, then the parent, and every byte
    // comes back exactly once.
    assert_eq!(ledger.release_owner(b).expect("b backs nothing"), 500);
    assert_eq!(ledger.release_owner(a).expect("a now backs nothing"), 600);
    assert_eq!(ledger.charged(root), 0);
    assert_eq!(ledger.available(root), 1_000);

    // Releasing an owner this pool does not have refunded nothing, and says so
    // rather than reporting a success it never performed.
    assert_eq!(
        ledger.release_owner(a).expect_err("already gone"),
        CapacityRefusal::UnknownOwner
    );
}

/// A pool must not honour another pool's grant.
///
/// Found by adversarial review. `redeem` copied the grant's fields and stamped
/// the redeeming pool onto the permit, so the refund landed here while the
/// issuer kept the charge outstanding forever. `unknown_refunds` did fire — on
/// the pool that had lost nothing, leaving the pool that actually leaked
/// capacity reporting a clean account. That is worse than no detector.
#[test]
fn a_grant_from_another_pool_is_refused_rather_than_honoured() {
    let issuer = ProcessCapacityPool::new();
    let issuer_root = issuer.root(1_000);
    let other = ProcessCapacityPool::new();

    let grant = issuer.reserve(issuer_root, 300).expect("headroom");
    assert_eq!(issuer.charged(issuer_root), 300);

    assert_eq!(
        other.redeem(grant).expect_err("not this pool's grant"),
        CapacityRefusal::ForeignGrant
    );

    // The refusal consumed the grant, so its `Drop` returned the bytes to the
    // pool that charged them. The charge does not survive the refusal and it
    // does not land in the wrong pool either — the two outcomes that would each
    // leave one account describing capacity it does not have.
    assert_eq!(issuer.charged(issuer_root), 0);
    assert_eq!(issuer.outstanding_charges(issuer_root), 0);
    assert_eq!(issuer.available(issuer_root), 1_000);
    assert_eq!(issuer.unknown_refunds(), 0);
    assert_eq!(other.unknown_refunds(), 0);

    // Paired positive: the issuing pool still honours its own grant, and the
    // refund lands where the charge was made.
    let permit = issuer
        .redeem(issuer.reserve(issuer_root, 100).expect("headroom"))
        .expect("own grant");
    assert_eq!(permit.release(), 100);
    assert_eq!(issuer.charged(issuer_root), 0);
    assert_eq!(issuer.unknown_refunds(), 0);
}

/// A grant abandoned before redemption must return its charge.
///
/// Found by adversarial review, which reproduced it by execution. `reserve`
/// charges the row and records the charge as outstanding, and only the PERMIT
/// had a `Drop` — so a grant dropped on any path between reserve and redeem, a
/// `?` or an early return or a panic, leaked those bytes permanently with no
/// refund and no counter, and wedged `release_owner` for that owner forever.
/// The module's stated invariant is that every charged byte is either held by a
/// live allocation or refunded exactly once; a grant is neither.
#[test]
fn a_grant_abandoned_before_redemption_refunds_itself() {
    let pool = ProcessCapacityPool::new();
    let root = pool.root(1_000);

    {
        let grant = pool.reserve(root, 400).expect("headroom exists");
        assert_eq!(grant.bytes(), 400);
        // Charged from the moment it exists, not from redemption.
        assert_eq!(pool.charged(root), 400);
        assert_eq!(pool.outstanding_charges(root), 1);
    }

    // Dropped without redeeming: every byte comes back, exactly once, and the
    // pool records no anomaly because nothing anomalous happened.
    assert_eq!(pool.charged(root), 0);
    assert_eq!(pool.available(root), 1_000);
    assert_eq!(pool.outstanding_charges(root), 0);
    assert_eq!(pool.unknown_refunds(), 0);

    // The owner is releasable rather than wedged.
    let child = pool.child(root, 200).expect("headroom");
    drop(pool.reserve(child, 50).expect("headroom"));
    assert_eq!(pool.release_owner(child).expect("nothing outstanding"), 200);

    // Paired positive: redeeming keeps the charge, and the permit owns the
    // refund from then on, so the grant's own drop must not also refund it.
    let permit = pool
        .redeem(pool.reserve(root, 300).expect("headroom"))
        .expect("own grant");
    assert_eq!(pool.charged(root), 300, "redemption refunded the charge");
    assert_eq!(permit.release(), 300);
    assert_eq!(pool.charged(root), 0);
    assert_eq!(pool.unknown_refunds(), 0);

    // And a grant refused by a foreign pool returns its bytes to the pool that
    // charged them rather than stranding them.
    let other = ProcessCapacityPool::new();
    let grant = pool.reserve(root, 100).expect("headroom");
    assert_eq!(
        other.redeem(grant).expect_err("not this pool's grant"),
        CapacityRefusal::ForeignGrant
    );
    assert_eq!(
        pool.charged(root),
        0,
        "a refused redemption stranded the charge"
    );
    assert_eq!(pool.unknown_refunds(), 0);
    assert_eq!(other.unknown_refunds(), 0);
}

/// TEST-CAPACITY-INTEGRATION (T069, Slice 4). Reserved name, empty of proof.
///
/// The traceability checker requires every `planned_exact` case declared for a
/// file to EXIST once that file exists, and Slice 2 created this file for
/// TEST-CAPACITY. So the name is materialized here at its declared target,
/// carrying nothing it has not observed: the activation cut whose conservation
/// it measures does not exist before T060, so there is no runtime to hold to
/// the identity `retained[d] + candidate[d] <= pregranted[d] + scratch[d] +
/// headroom[d]` across all four surfaces.
///
/// It is RED by construction and kept out of the default suite by `#[ignore]`,
/// the same shape Slice 0 used for its controls. Removing the attribute without
/// writing the body fails loudly rather than reporting a pass; the release
/// runner separately refuses an ignored-only run as execution evidence
/// (`scripts/validate-lifecycle-oracle-traceability.cjs`, `expect_execution`).
#[test]
#[ignore = "Feature 020 planned_not_executed case for TEST-CAPACITY-INTEGRATION; remove this attribute in Slice 4 (T069) when the activation cut can be driven and conservation actually measured"]
fn whole_runtime_capacity_is_conserved_under_activation() {
    panic!(
        "TEST-CAPACITY-INTEGRATION is planned_not_executed: no activation cut exists to \
         measure, so nothing here has observed whole-runtime conservation. T069 owns the body."
    );
}
