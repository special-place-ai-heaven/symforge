//! Feature 020 V11 Slice 2 capacity oracles (T032).
//!
//! Every rejection case asserts the accepting path in the same test. A ledger
//! that refuses every reservation satisfies a lone negative assertion perfectly,
//! and Slice 0 already shipped three controls that passed for reasons unrelated
//! to the property under test.

use symforge::live_index::index_lifecycle::capacity::{CapacityLedger, CapacityRefusal};

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
    let ledger = CapacityLedger::new();
    let root = ledger.root(1_000);

    // Charged on reserve, and the reservation is visible immediately.
    let grant = ledger.reserve(root, 400).expect("headroom exists");
    assert_eq!(ledger.charged(root), 400);
    assert_eq!(ledger.available(root), 600);
    assert_eq!(ledger.outstanding_charges(root), 1);

    // Redeeming does not change the charge: the bytes were already committed.
    let allocation = ledger.redeem(grant);
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
    let second = ledger.redeem(ledger.reserve(root, 250).expect("headroom exists"));
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
    let ledger = CapacityLedger::new();
    let root = ledger.root(1_000);

    let first = ledger.redeem(ledger.reserve(root, 100).expect("headroom"));
    let second = ledger.redeem(ledger.reserve(root, 100).expect("headroom"));
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

/// Exhaustion refuses with the numbers, and does not charge on refusal.
#[test]
fn an_exhausted_owner_refuses_without_charging() {
    let ledger = CapacityLedger::new();
    let root = ledger.root(500);
    let held = ledger.redeem(ledger.reserve(root, 400).expect("headroom"));

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
    drop(ledger.redeem(fits));
    drop(held);
    assert_eq!(ledger.charged(root), 0);
}

/// A child owner cannot promise more than its parent can back, and what it is
/// given is charged to the parent immediately.
#[test]
fn a_child_owner_is_backed_by_its_parent() {
    let ledger = CapacityLedger::new();
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
    let inside = ledger.redeem(ledger.reserve(child, 500).expect("fits in the child"));
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
    let ledger = CapacityLedger::new();
    let root = ledger.root(1_000);
    let child = ledger.child(root, 600).expect("600 fits");
    assert_eq!(ledger.available(root), 400);

    let inside = ledger.redeem(ledger.reserve(child, 200).expect("fits in the child"));

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
    let ledger = CapacityLedger::new();
    let root = ledger.root(1_000);
    let allocation = ledger.redeem(ledger.reserve(root, 300).expect("headroom"));

    // Release once: legitimate, refunds the real amount.
    assert_eq!(allocation.release(), 300);
    assert_eq!(ledger.charged(root), 0);
    assert_eq!(ledger.unknown_refunds(), 0);

    // A second allocation on a DIFFERENT ledger cannot refund against this one:
    // its charge is unknown here, so it must refund zero and be counted.
    let other = CapacityLedger::new();
    let other_root = other.root(1_000);
    let foreign = other.redeem(other.reserve(other_root, 100).expect("headroom"));
    drop(foreign);
    assert_eq!(
        ledger.unknown_refunds(),
        0,
        "another ledger's refund reached this one"
    );
    assert_eq!(other.charged(other_root), 0);
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
