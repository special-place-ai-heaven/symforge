//! Process-wide identity minting, shared by the lifecycle runtime and the
//! protocol provenance types.
//!
//! **Why this module exists at all.** `identity_newtype!` and its counter began
//! in `src/index_lifecycle/authority.rs`, and Feature 020 V11's provenance types
//! (`src/protocol/claim_provenance.rs`) need the SAME identities. Two options
//! were rejected:
//!
//!   * Redeclaring the macro under `protocol` would create a SECOND counter, so
//!     two identities minted "fresh" could compare unequal while both claiming
//!     to be unique. Two identity spaces is a worse defect than a name mismatch,
//!     because nothing about it is visible at the call site.
//!   * Importing from `src/index_lifecycle/` would create a
//!     `protocol -> index_lifecycle` call edge. That directory is DARK for the
//!     whole preactivation period, and `index_lifecycle/mod.rs` states its
//!     darkness as "`grep -rn index_lifecycle src/` returns no hit outside it".
//!     T051 proves that property; a protocol import would end it.
//!
//! So the primitives live HERE, under neither tree. `authority.rs` and
//! `claim_provenance.rs` both use them, one counter, no call edge between them.
//!
//! **This module is `pub(crate)`, deliberately.** The frozen public-API census
//! (`derivePublicApiAtoms`, `scripts/validate-lifecycle-oracle-traceability.cjs`)
//! adds one atom per `^\s*pub\s+mod\s+NAME\s*;` line in `src/lib.rs`. A
//! `pub mod lifecycle_identity;` there would add `symforge::lifecycle_identity`
//! and WIDEN the surface that Slice 3 must leave frozen. `pub(crate) mod` does
//! not match that pattern, so it adds nothing. Modules that need to expose one
//! of these types publicly re-export it; a `pub use` outside `src/embed.rs` is
//! not counted either.

use std::num::NonZeroU64;
use std::sync::atomic::{AtomicU64, Ordering};

/// The one counter. Every identity in the process is drawn from it, so an
/// identity minted by the lifecycle runtime can never collide with one minted
/// by a provenance receipt.
static NEXT_IDENTITY: AtomicU64 = AtomicU64::new(1);

pub(crate) fn next_identity() -> NonZeroU64 {
    let raw = NEXT_IDENTITY.fetch_add(1, Ordering::Relaxed);
    NonZeroU64::new(raw).expect("identity counter starts at 1 and only increases")
}

macro_rules! identity_newtype {
    // No PartialOrd/Ord: with a monotonic counter, ordering identities exposes
    // MINT ORDER, an inference channel nothing should read. An earlier draft
    // added Ord so a test could sort identities; the test uses a HashSet now
    // and the derive set matches the original authority.rs newtypes exactly.
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub struct $name(NonZeroU64);

        impl $name {
            /// Mint a fresh never-reused identity.
            pub fn fresh() -> Self {
                Self($crate::lifecycle_identity::next_identity())
            }
        }
    };
}

identity_newtype!(
    /// Identity of a source binding to one physical root.
    BindingIdentity
);
identity_newtype!(
    /// Stable identity of a filesystem observer registration.
    ObserverToken
);
identity_newtype!(
    /// Identity of an in-progress candidate build.
    CandidateIdentity
);
identity_newtype!(
    /// Identity of a promoted generation's authority.
    GenerationIdentity
);
identity_newtype!(
    /// Identity of one publication of source runtime state.
    PublicationIdentity
);
identity_newtype!(
    /// Identity of an untrusted on-disk snapshot seed.
    SnapshotIdentity
);

// ── Feature 020 V11 provenance identities (T043) ───────────────────────────

identity_newtype!(
    /// Identity of one atomic authority: a generation, or a single disk,
    /// worktree-scope, or Git observation.
    AuthorityIdentity
);
identity_newtype!(
    /// Identity of one claim's provenance structure. Caches, CCR, and
    /// persistence key on this, so bounded rendering must never move it.
    ProvenanceIdentity
);
identity_newtype!(
    /// Identity of one normalized operation request.
    OperationIdentity
);
identity_newtype!(
    /// Identity of one immutable ranking snapshot.
    EvaluationIdentity
);
identity_newtype!(
    /// Identity of one sealed worktree traversal.
    WorktreeScanId
);
identity_newtype!(
    /// Identity of the runtime publication that produced a claim.
    ProducingRuntimeIdentity
);

/// A monotonic observation instant.
///
/// Deliberately NOT a wall clock. Absence proofs are scoped to "the instant this
/// was observed", and a wall clock can repeat, jump backwards across a
/// resync, or tie between two observations. A monotonic counter cannot, so
/// `observed_at` orders and compares exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ObservationTime(NonZeroU64);

impl ObservationTime {
    /// Take a fresh observation instant.
    pub fn fresh() -> Self {
        Self(next_identity())
    }
}

/// Monotonic epoch of a filesystem observer registration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ObserverEpoch(u64);

impl ObserverEpoch {
    /// The epoch an observer occupies before it has seen any invalidation.
    pub const fn initial() -> Self {
        Self(0)
    }

    /// The next epoch. Never rewinds.
    pub const fn advanced(self) -> Self {
        Self(self.0 + 1)
    }

    /// Diagnostic value. Callers must not derive permission from it.
    pub const fn raw(self) -> u64 {
        self.0
    }
}

/// Position in an observer's invalidation stream. Seals the interval a
/// worktree-scope observation may speak for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InvalidationSequence(u64);

impl InvalidationSequence {
    /// The sequence before any invalidation has been recorded.
    pub const fn initial() -> Self {
        Self(0)
    }

    /// The next position. Never rewinds.
    pub const fn advanced(self) -> Self {
        Self(self.0 + 1)
    }

    /// Diagnostic value. Callers must not derive permission from it.
    pub const fn raw(self) -> u64 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_counter_serves_every_identity_kind() {
        // The property this module exists for: identities minted through
        // DIFFERENT newtypes still come from one space, so their raw values
        // never collide. A second counter under `protocol` would break this
        // silently, which is why the macro is not duplicated there.
        let a = GenerationIdentity::fresh().0;
        let b = AuthorityIdentity::fresh().0;
        let c = OperationIdentity::fresh().0;
        let d = ObservationTime::fresh().0;

        let mut raws = vec![a, b, c, d];
        raws.sort_unstable();
        raws.dedup();
        assert_eq!(raws.len(), 4, "one shared counter must not repeat a value");
    }

    #[test]
    fn a_fresh_identity_never_repeats_within_its_own_kind() {
        let first = GenerationIdentity::fresh();
        let second = GenerationIdentity::fresh();
        assert_ne!(first, second);
    }

    #[test]
    fn monotonic_positions_never_rewind() {
        let epoch = ObserverEpoch::initial();
        assert!(epoch.advanced() > epoch);
        let sequence = InvalidationSequence::initial();
        assert!(sequence.advanced() > sequence);
    }
}
