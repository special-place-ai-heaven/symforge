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
    /// Set while this thread is inside a finalizer, so a close attempted from
    /// within one can refuse rather than wait on itself.
    static FINALIZING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// The internal registration an embedder's process holds.
#[derive(Debug, Default)]
pub struct EmbeddedRegistration {
    open: std::sync::Mutex<HashMap<ProjectKey, EmbeddedIdentity>>,
    shutdown: AtomicBool,
}

impl EmbeddedRegistration {
    /// A registration with nothing open.
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Open `key`, yielding the sole handle for it.
    ///
    /// Refuses if a handle is already live for this key. Handing out a second
    /// handle would let one close while the other still believes it holds an
    /// open source.
    pub fn open(
        self: &Arc<Self>,
        key: ProjectKey,
    ) -> Result<EmbeddedSourceHandle, EmbedRefusal> {
        let mut open = self.open.lock().expect("embedded registration mutex");
        if let Some(held_by) = open.get(&key) {
            return Err(EmbedRefusal::SourceAlreadyOpen { held_by: *held_by });
        }
        let identity = EmbeddedIdentity::fresh();
        open.insert(key.clone(), identity);
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

    /// Whether the final owner has closed and the registration has shut down.
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
    registration: Arc<EmbeddedRegistration>,
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
        if FINALIZING.with(std::cell::Cell::get) {
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

    /// Run `finalizer` with self-wait detection armed.
    ///
    /// A finalizer that tries to close this source is refused rather than
    /// deadlocked. The flag is cleared even if the finalizer panics, so one bad
    /// finalizer cannot poison every later close on this thread.
    pub fn finalize<R>(&self, finalizer: impl FnOnce() -> R) -> R {
        struct Guard;
        impl Drop for Guard {
            fn drop(&mut self) {
                FINALIZING.with(|flag| flag.set(false));
            }
        }
        FINALIZING.with(|flag| flag.set(true));
        let _guard = Guard;
        finalizer()
    }
}

impl Drop for EmbeddedSourceHandle {
    fn drop(&mut self) {
        // Coalesce with an explicit close. A handle dropped without closing must
        // still release the source, or the registration would believe a source
        // is open that nothing holds.
        if !self.closed.swap(true, Ordering::AcqRel) {
            self.registration.close_one(&self.key, self.identity);
        }
    }
}
