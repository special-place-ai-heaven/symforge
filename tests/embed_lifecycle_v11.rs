//! Feature 020 V11 Slice 2 embedded-source oracles (T031).
//!
//! Every refusal asserts the accepting path in the same test.

use symforge::live_index::index_lifecycle::embedded::{EmbedRefusal, EmbeddedSourceFactory};
use symforge::live_index::index_lifecycle::registry::ProjectKey;

/// TEST-EMBED-FOUNDATION (T031). The name is pinned by
/// `contracts/lifecycle-oracle-traceability-v11.md` as a `planned_exact` target;
/// do not rename it without amending that contract.
///
/// One handle, one close authority, and `close` coalesces with `Drop`. These are
/// one property: if two handles could exist, either could close while the other
/// still believed it held an open source; and if `close` and `Drop` did not
/// coalesce, a handle closed explicitly and then dropped would release twice, or
/// one dropped without closing would leak the source forever.
#[test]
fn one_handle_close_and_drop_coalesce() {
    let registration = EmbeddedSourceFactory::new();
    let key = ProjectKey::new("embedded");

    let handle = registration.open(key.clone()).expect("first open succeeds");
    assert!(handle.is_open());
    assert_eq!(registration.open_count(), 1);

    // Negative: a second handle to the SAME source is refused, naming who holds it.
    assert_eq!(
        registration
            .open(key.clone())
            .expect_err("a second handle to one source must be refused"),
        EmbedRefusal::SourceAlreadyOpen {
            held_by: handle.identity()
        }
    );

    // Positive: a DIFFERENT source opens fine, so the refusal is about sole
    // ownership rather than the registration being closed.
    let other = registration
        .open(ProjectKey::new("other"))
        .expect("a different source opens");
    assert_eq!(registration.open_count(), 2);
    assert_ne!(other.identity(), handle.identity());

    // Closing performs the shutdown exactly once, and reports that it did.
    let receipt = handle.close().expect("an open handle closes");
    assert_eq!(receipt.identity(), handle.identity());
    assert!(
        receipt.performed_shutdown(),
        "the first close did not perform the shutdown"
    );
    assert!(
        !receipt.was_final_owner(),
        "one source still open, so this was not the final owner"
    );
    assert!(!handle.is_open());
    assert_eq!(registration.open_count(), 1);

    // A second close reports that it closed nothing, rather than pretending.
    assert_eq!(
        handle.close().expect_err("a closed handle refuses"),
        EmbedRefusal::AlreadyClosed
    );

    // Dropping the already-closed handle must not release the source a second
    // time -- the key is free, and re-opening it is what proves that.
    drop(handle);
    let reopened = registration
        .open(key)
        .expect("the closed source can be reopened");
    assert_eq!(registration.open_count(), 2);

    // Dropping WITHOUT closing releases too: coalescing runs both ways.
    drop(reopened);
    assert_eq!(
        registration.open_count(),
        1,
        "a handle dropped without closing leaked its source"
    );

    // The final owner's departure shuts the registration down.
    assert!(!registration.has_shut_down());
    let final_receipt = other.close().expect("the last handle closes");
    assert!(
        final_receipt.was_final_owner(),
        "the last close was not reported as the final owner"
    );
    assert!(registration.has_shut_down());
    assert_eq!(registration.open_count(), 0);
}

/// Closing from inside a finalizer would wait on the calling thread, so it is
/// refused rather than deadlocked.
#[test]
fn closing_from_within_a_finalizer_refuses_instead_of_waiting_on_itself() {
    let registration = EmbeddedSourceFactory::new();
    let handle = registration
        .open(ProjectKey::new("self-wait"))
        .expect("opens");

    // Negative: inside the finalizer, close refuses.
    let refusal = handle.finalize(|| {
        handle
            .close()
            .expect_err("close must refuse in a finalizer")
    });
    assert_eq!(refusal, EmbedRefusal::WouldSelfWait);
    assert!(handle.is_open(), "a refused close closed the source anyway");

    // Positive: outside the finalizer the same call succeeds, so the refusal is
    // about re-entrancy rather than the handle being unusable.
    let receipt = handle.close().expect("close succeeds outside a finalizer");
    assert!(receipt.performed_shutdown());
}

/// A panicking finalizer must not poison later closes on the same thread.
#[test]
fn a_panicking_finalizer_does_not_wedge_later_closes() {
    let registration = EmbeddedSourceFactory::new();
    let handle = registration
        .open(ProjectKey::new("panicking"))
        .expect("opens");

    let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        handle.finalize(|| panic!("finalizer blew up"));
    }));
    assert!(panicked.is_err(), "the finalizer was expected to panic");

    // The self-wait flag must have been cleared on unwind, or every later close
    // on this thread would refuse forever.
    let receipt = handle
        .close()
        .expect("a close after a panicking finalizer must still work");
    assert!(receipt.performed_shutdown());
    assert!(registration.has_shut_down());
}

/// Reopening a released key mints a new identity; the old one is never revived.
#[test]
fn a_reopened_source_is_a_new_identity() {
    let registration = EmbeddedSourceFactory::new();
    let key = ProjectKey::new("reopened");

    let first = registration.open(key.clone()).expect("opens");
    let first_identity = first.identity();
    first.close().expect("closes");

    let second = registration.open(key).expect("reopens");
    assert_ne!(
        second.identity(),
        first_identity,
        "a reopened source reused its predecessor's identity"
    );
}
