//! Internal embedded registration and sole-handle ownership (T037).
//!
//! **Production-unreachable.** Nothing in `src/` calls this module; the V10
//! public embed lane in `crate::embed` is untouched and keeps its own semantics.
//! "Unreachable" here means exactly that there is no production call path, which
//! is checkable by grep and is what the spec's own Slice 2 test asserts — not a
//! visibility trick. The types are `pub` because the contract-pinned oracles live
//! in `tests/`, which is an external crate.
//!
//! **Spawns nothing.** The public embed contract promises no implicit background
//! machinery, and `live_index` is not feature-gated, so this module compiles into
//! an embedder's binary. A finalizer here is a value that runs on the closing
//! thread, never a thread this module started.
//!
//! The ownership rule is *sole handle*: one open source has exactly one handle,
//! and that handle is the only thing that can close it. Two handles to one source
//! would let one close while the other still believes it holds an open source —
//! the shape where a caller reads from something already torn down.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use super::registry::ProjectKey;

/// Identity of one embedded open. Never reused, including across reopen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EmbeddedIdentity(std::num::NonZeroU64);

static NEXT_EMBEDDED: AtomicU64 = AtomicU64::new(1);

impl EmbeddedIdentity {
    /// Mint a fresh never-reused embedded identity.
    pub fn fresh() -> Self {
        let raw = NEXT_EMBEDDED.fetch_add(1, Ordering::Relaxed);
        Self(std::num::NonZeroU64::new(raw).expect("embedded counter starts at 1"))
    }
}

/// Why an embedded request was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmbedRefusal {
    /// This source already has a live handle; there is only ever one.
    SourceAlreadyOpen {
        /// The identity currently holding it.
        held_by: EmbeddedIdentity,
    },
    /// Closing here would wait on the thread doing the closing.
    ///
    /// Returned instead of deadlocking. A finalizer that closes the source it is
    /// finalizing is waiting for itself, and blocking would hang the embedder's
    /// thread with no diagnosis.
    WouldSelfWait,
    /// The handle has already closed.
    AlreadyClosed,
}

/// What a close actually did.
///
/// Records whether THIS call performed the shutdown or joined one already done,
/// so a caller cannot mistake a coalesced close for having been the one that
/// tore the source down.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloseReceipt {
    identity: EmbeddedIdentity,
    performed_shutdown: bool,
    was_final_owner: bool,
}

impl CloseReceipt {
    /// The source that was closed.
    pub fn identity(&self) -> EmbeddedIdentity {
        self.identity
    }

    /// Whether this call performed the shutdown, rather than joining one that
    /// had already happened.
    pub fn performed_shutdown(&self) -> bool {
        self.performed_shutdown
    }

    /// Whether this was the last open source in the registration.
    pub fn was_final_owner(&self) -> bool {
        self.was_final_owner
    }
}

thread_local! {
    /// Which source this thread is finalizing, if any, so a close attempted
    /// from inside that source's own finalizer can refuse rather than wait on
    /// itself.
    ///
    /// The identity is the whole point. A bare `bool` refused ANY close made
    /// from inside ANY finalizer, so `a.finalize(|| b.close())` reported that
    /// closing `b` would wait on itself — a diagnosis naming something that did
    /// not happen, while `b` was left open.
    static FINALIZING: std::cell::Cell<Option<EmbeddedIdentity>> =
        const { std::cell::Cell::new(None) };
}

/// Mints embedded source handles, and is the only thing that can.
///
/// Named by the frozen `ORACLE-EMBED-FOUNDATION` seam. Construction of a handle
/// is private to this type, which is what makes "one incarnation owns at most
/// one handle" enforceable rather than advisory.
#[derive(Debug, Default)]
pub struct EmbeddedSourceFactory {
    open: std::sync::Mutex<HashMap<ProjectKey, EmbeddedIdentity>>,
    shutdown: AtomicBool,
}

impl EmbeddedSourceFactory {
    /// A registration with nothing open.
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Open `key`, yielding the sole handle for it.
    ///
    /// Refuses if a handle is already live for this key. Handing out a second
    /// handle would let one close while the other still believes it holds an
    /// open source.
    pub fn open(self: &Arc<Self>, key: ProjectKey) -> Result<EmbeddedSourceHandle, EmbedRefusal> {
        let mut open = self.open.lock().expect("embedded registration mutex");
        if let Some(held_by) = open.get(&key) {
            return Err(EmbedRefusal::SourceAlreadyOpen { held_by: *held_by });
        }
        let identity = EmbeddedIdentity::fresh();
        open.insert(key.clone(), identity);
        // The factory is serving again, so it is no longer shut down. The flag
        // latched before, which made `has_shut_down()` report `true` while
        // `open_count()` was 1 — a claim about a past moment presented as
        // present state. Cleared under the same lock that records the open, so
        // the two can never disagree.
        self.shutdown.store(false, Ordering::Release);
        Ok(EmbeddedSourceHandle {
            identity,
            key,
            registration: Arc::clone(self),
            closed: AtomicBool::new(false),
        })
    }

    /// How many sources are open.
    pub fn open_count(&self) -> usize {
        self.open.lock().expect("embedded registration mutex").len()
    }

    /// Whether the factory is currently shut down: the final owner closed and
    /// nothing has been opened since.
    pub fn has_shut_down(&self) -> bool {
        self.shutdown.load(Ordering::Acquire)
    }

    /// Close one source. Returns whether this call performed the shutdown.
    fn close_one(&self, key: &ProjectKey, identity: EmbeddedIdentity) -> (bool, bool) {
        let mut open = self.open.lock().expect("embedded registration mutex");
        let performed = open.get(key) == Some(&identity) && open.remove(key).is_some();
        let final_owner = performed && open.is_empty();
        if final_owner {
            self.shutdown.store(true, Ordering::Release);
        }
        (performed, final_owner)
    }
}

/// The sole handle to one embedded source.
///
/// Not `Clone`: sole ownership is the invariant, and a clonable handle would
/// mean two owners could each believe they hold the source.
#[derive(Debug)]
pub struct EmbeddedSourceHandle {
    identity: EmbeddedIdentity,
    key: ProjectKey,
    registration: Arc<EmbeddedSourceFactory>,
    closed: AtomicBool,
}

impl EmbeddedSourceHandle {
    /// This open's identity.
    pub fn identity(&self) -> EmbeddedIdentity {
        self.identity
    }

    /// The key it holds open.
    pub fn key(&self) -> &ProjectKey {
        &self.key
    }

    /// Whether this handle is still open.
    pub fn is_open(&self) -> bool {
        !self.closed.load(Ordering::Acquire)
    }

    /// Close the source.
    ///
    /// Coalesces with `Drop`: whichever runs first performs the shutdown, and
    /// the other reports that it joined rather than performed it. Refuses with
    /// [`EmbedRefusal::WouldSelfWait`] when called from inside a finalizer,
    /// because waiting there is waiting on the calling thread itself.
    pub fn close(&self) -> Result<CloseReceipt, EmbedRefusal> {
        if FINALIZING.with(std::cell::Cell::get) == Some(self.identity) {
            return Err(EmbedRefusal::WouldSelfWait);
        }
        if self.closed.swap(true, Ordering::AcqRel) {
            return Err(EmbedRefusal::AlreadyClosed);
        }
        let (performed, final_owner) = self.registration.close_one(&self.key, self.identity);
        Ok(CloseReceipt {
            identity: self.identity,
            performed_shutdown: performed,
            was_final_owner: final_owner,
        })
    }

    /// V11 (T047, E1 ruling): begin the close INFALLIBLY. The Slice 2 `close`
    /// refuses a self-wait AT CLOSE; the V11 contract relocates that guard to
    /// the WAIT — beginning a close is always legal, and only waiting on your
    /// own close from inside the finalizer refuses. An already-closed source
    /// yields a receipt that joined rather than performed, same as `Drop`
    /// coalescing.
    pub fn begin_close(&self) -> SourceCloseReceipt {
        let performed = if self.closed.swap(true, Ordering::AcqRel) {
            false
        } else {
            let (performed, _final_owner) = self.registration.close_one(&self.key, self.identity);
            performed
        };
        SourceCloseReceipt {
            identity: self.identity,
            performed_shutdown: performed,
        }
    }

    /// Fixture probe for the relocated guard: arms the finalizer for THIS
    /// source and attempts to wait on its own close receipt from inside it —
    /// which must refuse with [`ReceiptWaitError::WouldSelfWait`] rather than
    /// deadlock.
    pub fn self_wait_probe_for_test(&self) -> Result<SourceCloseReport, ReceiptWaitError> {
        let receipt = self.begin_close();
        self.finalize(|| receipt.wait_for_test())
    }

    /// Run `finalizer` with self-wait detection armed for THIS source.
    ///
    /// A finalizer that tries to close this source is refused rather than
    /// deadlocked; a finalizer that closes some other source is none of this
    /// source's business and proceeds. The previous value is restored even if
    /// the finalizer panics, so one bad finalizer cannot poison every later
    /// close on this thread, and nesting two finalizers cannot leave the outer
    /// one disarmed.
    pub fn finalize<R>(&self, finalizer: impl FnOnce() -> R) -> R {
        struct Guard(Option<EmbeddedIdentity>);
        impl Drop for Guard {
            fn drop(&mut self) {
                FINALIZING.with(|flag| flag.set(self.0));
            }
        }
        let _guard = Guard(FINALIZING.with(|flag| flag.replace(Some(self.identity))));
        finalizer()
    }
}

/// Waiting on a receipt can refuse; the wait is where the self-wait guard
/// lives in V11, so the error is typed rather than a deadlock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReceiptWaitError {
    /// The wait was attempted from inside this source's own finalizer:
    /// waiting there is waiting on the calling thread itself.
    WouldSelfWait,
}

/// Receipt for a V11 `begin_close`. Nothing is spawned in the dark modules,
/// so the wait completes immediately — but it still owns the self-wait guard.
#[derive(Debug)]
pub struct SourceCloseReceipt {
    identity: EmbeddedIdentity,
    performed_shutdown: bool,
}

impl SourceCloseReceipt {
    /// Wait for the close to finalize. Refuses a self-wait; completes
    /// immediately otherwise, because the close performed synchronously and
    /// only observed completions may be reported.
    pub fn wait_for_test(&self) -> Result<SourceCloseReport, ReceiptWaitError> {
        if FINALIZING.with(std::cell::Cell::get) == Some(self.identity) {
            return Err(ReceiptWaitError::WouldSelfWait);
        }
        Ok(SourceCloseReport {
            finalized: true,
            performed_shutdown: self.performed_shutdown,
        })
    }
}

/// What the close wait OBSERVED.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceCloseReport {
    finalized: bool,
    performed_shutdown: bool,
}

impl SourceCloseReport {
    pub fn finalized(&self) -> bool {
        self.finalized
    }

    pub fn performed_shutdown(&self) -> bool {
        self.performed_shutdown
    }
}

impl Drop for EmbeddedSourceHandle {
    fn drop(&mut self) {
        // Coalesce with an explicit close. A handle dropped without closing must
        // still release the source, or the registration would believe a source
        // is open that nothing holds.
        //
        // `Drop` deliberately does NOT consult `FINALIZING`. The self-wait
        // hazard `close` refuses is about WAITING, and `close_one` takes a
        // mutex and returns — it never waits on the finalizer. Refusing here
        // would be worse than the hazard: a `Drop` cannot report a refusal, so
        // the source would simply stay open forever with nothing holding it.
        // This is stated rather than left as an inconsistency between the two
        // paths, which is how it read before: `a.finalize(|| drop(b))` was
        // permitted while `a.finalize(|| b.close())` was refused.
        if !self.closed.swap(true, Ordering::AcqRel) {
            self.registration.close_one(&self.key, self.identity);
        }
    }
}
