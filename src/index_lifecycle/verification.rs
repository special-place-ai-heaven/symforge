//! Feature 020 V11 rolling verification (Slice 4, T062 — dark).
//!
//! Racy-clean entry obligations, scope-discovery deadlines, resumable rolling
//! verification, immutable proof refresh, and the exact frozen-FR-049
//! monotonic overdue predicate: only a complete exact-identity whole-scope
//! `VerificationRecord` bound to its sealed `VerificationScopeReceipt`
//! advances the fixed 15-minute deadline; promotion requires a
//! `VerificationFeasibilityReceipt` whose `VerificationWorkBound` is
//! affordable at the reserved service floors; deadline expiry atomically
//! latches non-Current before any strict lease (frozen tasks.md T062).
//!
//! Dark payload simplifications, in the `runtime.rs` idiom: scope entries are
//! id→stamp pairs, not real catalog derivations, and the clock is an explicit
//! `MonotonicInstant` parameter so oracles drive the deadline exactly. The
//! authority SEMANTICS — sealed scope, exact-identity advancement, the
//! at/after latch ordering, feasibility-loss → non-Current, policy-mismatch →
//! re-scout — are exact; payloads are recorded cut obligations. Deliberately
//! deferred with them: `MAX_SUCCESSOR_VERIFICATION_START` (180 s) and the
//! `successor_start_deadline` scheduling arithmetic, which sit outside
//! T055's case list and land with the live scheduler at the cut.
//!
//! **Nothing in production calls this module.** Only the Slice 4 oracle
//! suites and this directory do; activation (T064/T066) is the only planned
//! production caller.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::VecDeque;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::time::Duration;

/// Frozen defaults (`specs/020-repository-knowledge-index/data-model.md`):
/// the bound computed at the reserved floors caps at 512 + 200 = 712 seconds,
/// so `DEFAULT_MAX_COMPLETE_VERIFICATION_PASS` is a ceiling the defaults
/// never reach.
pub const DEFAULT_MAX_VERIFICATION_BYTES: u64 = 17_179_869_184;
pub const DEFAULT_MAX_VERIFICATION_ENTRIES: u64 = 200_000;
pub const DEFAULT_MAX_COMPLETE_VERIFICATION_PASS: Duration = Duration::from_secs(720);
pub const RESERVED_VERIFICATION_BYTES_PER_SECOND: u64 = 33_554_432;
pub const RESERVED_VERIFICATION_ENTRIES_PER_SECOND: u64 = 1_000;
/// The fixed 15-minute deadline (frozen FR-049).
pub const MAX_CURRENT_UNVERIFIED_AGE: Duration = Duration::from_secs(900);

/// An explicit monotonic instant in milliseconds. Oracles drive it; nothing
/// here reads an ambient clock.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct MonotonicInstant(pub u64);

impl MonotonicInstant {
    pub fn plus(self, delta: Duration) -> Self {
        Self(self.0 + u64::try_from(delta.as_millis()).expect("delta fits in u64 millis"))
    }
}

fn age_millis() -> u64 {
    u64::try_from(MAX_CURRENT_UNVERIFIED_AGE.as_millis()).expect("age fits in u64 millis")
}

/// Scope/policy version an observation was discovered under.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PolicyVersion(pub u64);

/// Identity of one verification scope receipt.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct VerificationScopeReceiptId(pub(crate) u64);

static NEXT_RECEIPT: AtomicU64 = AtomicU64::new(1);

/// Why verification refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VerificationRefusal {
    /// The deadline latched: at/after the fixed age, non-Current is latched
    /// BEFORE any strict acquisition can be served.
    OverdueLatched,
    /// The record's entry set is not the exact sealed scope.
    NotWholeScope {
        missing: usize,
        extra: usize,
        mismatched: usize,
    },
    /// The record or pass is bound to a receipt this verifier no longer
    /// serves (immutable proof refresh fenced it out).
    ProofFenced,
    /// The scope's declared work exceeds the admission caps.
    WorkBeyondCaps,
    /// The pass's policy version does not match the sealed receipt's.
    PolicyMismatch,
    /// The feasibility receipt was reserved for a DIFFERENT scope receipt —
    /// feasibility is bound, never bearer (frozen data model:
    /// `VerificationWorkBound.scope_receipt`).
    FeasibilityNotForThisScope,
    /// The verifier is non-Current; only an authoritative re-scout returns.
    NonCurrent(NonCurrentCause),
    /// An entry drifted to a DIFFERENT stamp during the pass (a same-stamp
    /// rewrite stays racy-clean; a changed stamp dirties the scope).
    ScopeDirty,
}

/// Why a verifier is non-Current.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NonCurrentCause {
    OverdueLatched,
    ReservationLost,
    PolicyMismatch,
}

/// Sealed enumeration of everything one complete whole-declared-scope pass
/// must check. Sealed at construction and never widened or narrowed in
/// place: a scope change produces a NEW receipt with a NEW identity.
#[derive(Debug)]
pub struct VerificationScopeReceipt {
    id: VerificationScopeReceiptId,
    policy: PolicyVersion,
    entries: BTreeMap<u64, u64>,
}

impl VerificationScopeReceipt {
    /// Seal a receipt over discovered scope entries (id → stamp) under one
    /// policy version.
    pub fn seal(policy: PolicyVersion, entries: BTreeMap<u64, u64>) -> Self {
        Self {
            id: VerificationScopeReceiptId(NEXT_RECEIPT.fetch_add(1, Ordering::Relaxed)),
            policy,
            entries,
        }
    }

    pub fn receipt_id(&self) -> VerificationScopeReceiptId {
        self.id
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Cost of the pass the receipt describes. The private constructor computes
/// `bound` from the operands at the reserved service floors; `bound` is never
/// separately settable, so the value the admission gate trusts cannot
/// disagree with the operands it was derived from.
#[derive(Debug)]
pub struct VerificationWorkBound {
    scope_receipt: VerificationScopeReceiptId,
    bound: Duration,
}

impl VerificationWorkBound {
    /// Compute the bound for ONE sealed scope's declared work, refusing work
    /// beyond the frozen admission caps. The bound carries the receipt it
    /// was computed for: feasibility is bound to a scope, never bearer.
    pub fn for_scope(
        receipt: &VerificationScopeReceipt,
        verification_bytes: u64,
        verification_entries: u64,
    ) -> Result<Self, VerificationRefusal> {
        if verification_bytes > DEFAULT_MAX_VERIFICATION_BYTES
            || verification_entries > DEFAULT_MAX_VERIFICATION_ENTRIES
        {
            return Err(VerificationRefusal::WorkBeyondCaps);
        }
        let seconds = verification_bytes.div_ceil(RESERVED_VERIFICATION_BYTES_PER_SECOND)
            + verification_entries.div_ceil(RESERVED_VERIFICATION_ENTRIES_PER_SECOND);
        Ok(Self {
            scope_receipt: receipt.id,
            bound: Duration::from_secs(seconds),
        })
    }

    pub fn bound(&self) -> Duration {
        self.bound
    }
}

/// Capacity actually reserved for the pass. Promotion to `Current` requires
/// one; losing the reservation makes the source non-Current rather than
/// extending the deadline.
#[derive(Debug)]
pub struct VerificationFeasibilityReceipt {
    work: VerificationWorkBound,
}

impl VerificationFeasibilityReceipt {
    pub fn reserve(work: VerificationWorkBound) -> Self {
        Self { work }
    }

    fn scope_receipt(&self) -> VerificationScopeReceiptId {
        self.work.scope_receipt
    }
}

/// A complete exact-identity whole-scope verification result, bound to the
/// sealed receipt that scoped it.
#[derive(Debug)]
pub struct VerificationRecord {
    receipt: VerificationScopeReceiptId,
}

/// One resumable rolling pass over a sealed receipt's scope. Partial
/// progress lives HERE, never in the verifier: nothing short of `complete`
/// can touch a deadline.
#[derive(Debug)]
pub struct RollingPass {
    receipt: VerificationScopeReceiptId,
    scope: BTreeMap<u64, u64>,
    verified: BTreeMap<u64, u64>,
}

impl RollingPass {
    /// Verify a chunk of entry ids (id → observed stamp). Partial progress
    /// accumulates in the pass and NEVER advances any deadline.
    pub fn verify_chunk(
        &mut self,
        observed: BTreeMap<u64, u64>,
    ) -> Result<(), VerificationRefusal> {
        self.verified.extend(observed);
        Ok(())
    }

    /// The entry ids still owed.
    pub fn remaining(&self) -> BTreeSet<u64> {
        self.scope
            .keys()
            .filter(|id| !self.verified.contains_key(id))
            .copied()
            .collect()
    }

    /// Complete the pass into a record — only when the accumulated set is
    /// EXACTLY the sealed scope: nothing missing, nothing extra, every stamp
    /// identical.
    pub fn complete(self) -> Result<VerificationRecord, VerificationRefusal> {
        let missing = self
            .scope
            .keys()
            .filter(|id| !self.verified.contains_key(id))
            .count();
        let extra = self
            .verified
            .keys()
            .filter(|id| !self.scope.contains_key(id))
            .count();
        let mismatched = self
            .scope
            .iter()
            .filter(|(id, stamp)| self.verified.get(id).is_some_and(|seen| seen != *stamp))
            .count();
        if missing + extra + mismatched > 0 {
            return Err(VerificationRefusal::NotWholeScope {
                missing,
                extra,
                mismatched,
            });
        }
        Ok(VerificationRecord {
            receipt: self.receipt,
        })
    }
}

/// Fair scheduler over concurrently rolling passes: strict rotation, so
/// every registered pass makes progress and none starves.
#[derive(Debug, Default)]
pub struct PassScheduler {
    queue: VecDeque<u64>,
}

impl PassScheduler {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a pass identity for scheduling.
    pub fn register(&mut self, pass_id: u64) {
        self.queue.push_back(pass_id);
    }

    /// The next pass to grant a verification slot to; the grantee rotates to
    /// the back of the queue.
    pub fn next_grant(&mut self) -> Option<u64> {
        let granted = self.queue.pop_front()?;
        self.queue.push_back(granted);
        Some(granted)
    }
}

/// The per-source verification machine: owns the sealed receipt, the fixed
/// deadline, the overdue latch, and the only paths between Current and
/// non-Current.
#[derive(Debug)]
pub struct RollingVerifier {
    receipt: VerificationScopeReceipt,
    _feasibility: VerificationFeasibilityReceipt,
    verified_at: MonotonicInstant,
    non_current: Option<NonCurrentCause>,
    scope_dirty: bool,
}

impl RollingVerifier {
    /// Promote to Current at `now`. Requires a feasibility reservation whose
    /// work bound was computed for THIS receipt — affordability is admission,
    /// not advice, and a foreign reservation refuses.
    pub fn promote(
        receipt: VerificationScopeReceipt,
        feasibility: VerificationFeasibilityReceipt,
        now: MonotonicInstant,
    ) -> Result<Self, VerificationRefusal> {
        if feasibility.scope_receipt() != receipt.id {
            return Err(VerificationRefusal::FeasibilityNotForThisScope);
        }
        Ok(Self {
            receipt,
            _feasibility: feasibility,
            verified_at: now,
            non_current: None,
            scope_dirty: false,
        })
    }

    /// Begin a rolling pass over the sealed scope under `policy`. A policy
    /// mismatch refuses AND forces non-Current (without relabeling an
    /// already-latched cause): only an authoritative re-scout may mint a new
    /// Current after that. An OVERDUE-latched verifier may still begin a
    /// pass — frozen FR-049's latch is cleared by exactly a fresh complete
    /// verification, so the path to one must stay open; the other
    /// non-Current causes refuse (no capacity, or an invalidated scope).
    pub fn begin_pass(
        &mut self,
        policy: PolicyVersion,
    ) -> Result<RollingPass, VerificationRefusal> {
        if policy != self.receipt.policy {
            if self.non_current.is_none() {
                self.non_current = Some(NonCurrentCause::PolicyMismatch);
            }
            return Err(VerificationRefusal::PolicyMismatch);
        }
        match self.non_current {
            None | Some(NonCurrentCause::OverdueLatched) => Ok(RollingPass {
                receipt: self.receipt.id,
                scope: self.receipt.entries.clone(),
                verified: BTreeMap::new(),
            }),
            Some(cause) => Err(VerificationRefusal::NonCurrent(cause)),
        }
    }

    /// Whether the source is Current at `now`: never while a non-Current
    /// cause is latched, and only STRICTLY before the fixed deadline — AT
    /// the deadline is overdue.
    pub fn is_current(&self, now: MonotonicInstant) -> bool {
        self.non_current.is_none() && now.0 < self.verified_at.0 + age_millis()
    }

    /// Acquire a strict lease at `now`. At/after the deadline this LATCHES
    /// non-Current first and then refuses — the latch is observable even
    /// though the acquisition never was.
    pub fn acquire_strict(&mut self, now: MonotonicInstant) -> Result<(), VerificationRefusal> {
        match self.non_current {
            Some(NonCurrentCause::OverdueLatched) => Err(VerificationRefusal::OverdueLatched),
            Some(cause) => Err(VerificationRefusal::NonCurrent(cause)),
            None => {
                if now.0 >= self.verified_at.0 + age_millis() {
                    self.non_current = Some(NonCurrentCause::OverdueLatched);
                    return Err(VerificationRefusal::OverdueLatched);
                }
                Ok(())
            }
        }
    }

    /// Advance the deadline with a complete record at `now`. Anything less
    /// than the exact sealed whole scope was already refused at `complete`;
    /// here a stale receipt binding fences and a dirty scope refuses. A
    /// fresh complete record CLEARS an overdue latch — frozen FR-049's
    /// exact clear clause — while the other non-Current causes refuse.
    pub fn advance(
        &mut self,
        record: VerificationRecord,
        now: MonotonicInstant,
    ) -> Result<(), VerificationRefusal> {
        if record.receipt != self.receipt.id {
            return Err(VerificationRefusal::ProofFenced);
        }
        if self.scope_dirty {
            return Err(VerificationRefusal::ScopeDirty);
        }
        match self.non_current {
            None | Some(NonCurrentCause::OverdueLatched) => {
                self.non_current = None;
                self.verified_at = now;
                Ok(())
            }
            Some(cause) => Err(VerificationRefusal::NonCurrent(cause)),
        }
    }

    /// Refresh the proof scope: seals a NEW receipt (new identity) and
    /// fences every pass still bound to the old one. The old receipt is
    /// never mutated; the fresh receipt starts racy-clean. A receipt sealed
    /// under a DIFFERENT policy refuses — a policy change may not smuggle
    /// past the mandated re-scout.
    pub fn refresh_scope(
        &mut self,
        receipt: VerificationScopeReceipt,
    ) -> Result<(), VerificationRefusal> {
        if receipt.policy != self.receipt.policy {
            if self.non_current.is_none() {
                self.non_current = Some(NonCurrentCause::PolicyMismatch);
            }
            return Err(VerificationRefusal::PolicyMismatch);
        }
        self.receipt = receipt;
        self.scope_dirty = false;
        Ok(())
    }

    /// Record that an entry was rewritten during verification with `stamp`.
    /// A SAME-stamp rewrite stays racy-clean; a different stamp — or a
    /// rewrite outside the sealed scope — dirties it.
    pub fn observe_rewrite(&mut self, entry: u64, stamp: u64) {
        match self.receipt.entries.get(&entry) {
            Some(sealed) if *sealed == stamp => {}
            _ => self.scope_dirty = true,
        }
    }

    /// The reservation backing feasibility was lost: the source becomes
    /// non-Current — the deadline is NOT extended and the cause says why.
    pub fn reservation_lost(&mut self) {
        self.non_current = Some(NonCurrentCause::ReservationLost);
    }

    /// The cause if non-Current.
    pub fn non_current_cause(&self) -> Option<NonCurrentCause> {
        self.non_current
    }

    /// The authoritative re-scout: a fresh receipt with fresh feasibility
    /// bound to it. The only path back to Current from a lost reservation or
    /// a policy mismatch (the overdue latch alternatively clears through a
    /// fresh complete verification, per frozen FR-049).
    pub fn re_scout(
        &mut self,
        receipt: VerificationScopeReceipt,
        feasibility: VerificationFeasibilityReceipt,
        now: MonotonicInstant,
    ) -> Result<(), VerificationRefusal> {
        if feasibility.scope_receipt() != receipt.id {
            return Err(VerificationRefusal::FeasibilityNotForThisScope);
        }
        self.receipt = receipt;
        self._feasibility = feasibility;
        self.verified_at = now;
        self.non_current = None;
        self.scope_dirty = false;
        Ok(())
    }
}
