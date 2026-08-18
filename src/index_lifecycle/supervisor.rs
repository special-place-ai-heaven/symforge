//! Feature 020 V11 per-source supervisor (Slice 4, T059 — dark).
//!
//! The supervisor owns LOADING for exactly one source: attempt ownership,
//! cancellation, attempt accounting, classified failure, and retry triggers.
//! Attempt history is DIAGNOSTIC data. It is structurally separate from
//! committed dispositions and deliberately cannot carry a manifest digest, a
//! completeness certificate, or query authority (`data-model.md:794-800` in
//! the frozen 020 tree): the two ledgers this type exposes are the seam the
//! T056 health split (`committed_generation_and_attempt_health_are_separate`)
//! will later project.
//!
//! **Nothing in production calls this module.** The darkness property is about
//! call edges: the only callers are `candidate.rs` (same directory) and the
//! Slice 4 oracle suites under `tests/`. Activation (T066/T064) is the only
//! planned production caller.

use std::collections::BTreeSet;
use std::sync::Arc;
use std::sync::Mutex;

/// Closed classification of a failed load/build attempt — exactly the causes
/// the closed promotion matrix refuses on (frozen tasks.md T053: `Unreadable`,
/// `UnstableDuringRead`, `AbortedCircuitBreaker`, `ParseStatus::Failed`,
/// unknown ordering, truncated required derivations, `PartialParse`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClassifiedFailure {
    Unreadable,
    UnstableDuringRead,
    AbortedCircuitBreaker,
    ParseFailed,
    UnknownOrdering,
    TruncatedRequiredDerivation,
    PartialParse,
}

impl ClassifiedFailure {
    /// Every member of the closed matrix, for exhaustive oracle sweeps.
    pub const ALL: [ClassifiedFailure; 7] = [
        ClassifiedFailure::Unreadable,
        ClassifiedFailure::UnstableDuringRead,
        ClassifiedFailure::AbortedCircuitBreaker,
        ClassifiedFailure::ParseFailed,
        ClassifiedFailure::UnknownOrdering,
        ClassifiedFailure::TruncatedRequiredDerivation,
        ClassifiedFailure::PartialParse,
    ];
}

/// Terminal disposition of one attempt in the diagnostics ledger.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttemptDisposition {
    /// The attempt's candidate reached the single commit point and published.
    Committed,
    /// Discarded with a classified cause; nothing published.
    Discarded(ClassifiedFailure),
    /// Cancelled by its owner before any terminal outcome.
    Cancelled,
    /// A retry trigger minted a successor; this attempt can never commit.
    Superseded,
    /// The build panicked; the candidate was discarded whole.
    Panicked,
}

/// Identity of one attempt within its supervisor's sequence.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AttemptId(pub(crate) u64);

/// One diagnostics row (the ledger keeps the newest [`MAX_ATTEMPT_RECORDS`]).
/// Deliberately carries no manifest digest, certificate, or query authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AttemptRecord {
    pub id: AttemptId,
    pub disposition: AttemptDisposition,
}

/// The diagnostics ledger keeps at most this many newest terminal rows. The
/// frozen data model requires bounded diagnostics; the attempt-id SETS below
/// are compact but unbounded, and bounding them is a recorded activation
/// obligation (they guard replay of still-live candidate tokens, so they
/// cannot be pruned before the cut defines candidate lifetime).
const MAX_ATTEMPT_RECORDS: usize = 64;

/// Shared interior. Locked for every transition, and every terminal row goes
/// through [`SupervisorState::terminal_row`], so one attempt can terminal-end
/// exactly once: replaying an attempt's token can never make the committed
/// ledger and the diagnostics ledger disagree.
#[derive(Debug, Default)]
pub(crate) struct SupervisorState {
    next_attempt: u64,
    /// Non-terminal attempts, in mint order (last = most recent).
    live: Vec<AttemptId>,
    superseded: BTreeSet<AttemptId>,
    terminal: BTreeSet<AttemptId>,
    records: Vec<AttemptRecord>,
    committed: u64,
}

impl SupervisorState {
    fn mint(&mut self) -> AttemptId {
        let id = AttemptId(self.next_attempt);
        self.next_attempt += 1;
        self.live.push(id);
        id
    }

    /// Terminal-end `id` with `disposition` — exactly once. Returns whether
    /// this call was the one that ended it; a second terminal event for the
    /// same attempt records nothing.
    fn terminal_row(&mut self, id: AttemptId, disposition: AttemptDisposition) -> bool {
        if !self.terminal.insert(id) {
            return false;
        }
        self.live.retain(|live| *live != id);
        self.records.push(AttemptRecord { id, disposition });
        if self.records.len() > MAX_ATTEMPT_RECORDS {
            self.records.remove(0);
        }
        true
    }

    pub(crate) fn is_superseded(&self, id: AttemptId) -> bool {
        self.superseded.contains(&id)
    }

    pub(crate) fn is_terminal(&self, id: AttemptId) -> bool {
        self.terminal.contains(&id)
    }

    pub(crate) fn record_commit(&mut self, id: AttemptId) {
        if self.terminal_row(id, AttemptDisposition::Committed) {
            self.committed += 1;
        }
    }

    pub(crate) fn record_discard(&mut self, id: AttemptId, cause: ClassifiedFailure) {
        self.terminal_row(id, AttemptDisposition::Discarded(cause));
    }

    pub(crate) fn record_panic(&mut self, id: AttemptId) {
        self.terminal_row(id, AttemptDisposition::Panicked);
    }
}

/// Per-source supervisor: loader ownership, cancellation, attempt accounting,
/// classified failure, retry triggers (T059).
#[derive(Debug)]
pub struct SourceSupervisor {
    pub(crate) state: Arc<Mutex<SupervisorState>>,
}

/// Ownership token for one in-flight attempt. Only a non-superseded attempt
/// can commit; a retry trigger supersedes every live predecessor.
#[derive(Debug)]
pub struct LoadAttempt {
    pub(crate) id: AttemptId,
    pub(crate) state: Arc<Mutex<SupervisorState>>,
}

impl SourceSupervisor {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(SupervisorState::default())),
        }
    }

    /// Begin a fresh attempt, taking loader ownership for it.
    pub fn begin_attempt(&self) -> LoadAttempt {
        let id = self.state.lock().expect("supervisor lock").mint();
        LoadAttempt {
            id,
            state: Arc::clone(&self.state),
        }
    }

    /// Record a classified failure for the source's newest live attempt and
    /// mint its superseding successor — the retry trigger. Every OTHER live
    /// attempt is superseded too: the source is being re-observed, so every
    /// in-flight build is stale. With no live attempt the cause has no row to
    /// land on and rides the successor's future terminal row instead.
    pub fn retry_trigger(&self, cause: ClassifiedFailure) -> LoadAttempt {
        let mut state = self.state.lock().expect("supervisor lock");
        if let Some(newest) = state.live.last().copied() {
            state.superseded.insert(newest);
            state.record_discard(newest, cause);
        }
        for stale in std::mem::take(&mut state.live) {
            state.superseded.insert(stale);
            state.terminal_row(stale, AttemptDisposition::Superseded);
        }
        let id = state.mint();
        drop(state);
        LoadAttempt {
            id,
            state: Arc::clone(&self.state),
        }
    }

    /// The bounded diagnostics ledger: every terminal attempt, in order.
    pub fn attempt_records(&self) -> Vec<AttemptRecord> {
        self.state.lock().expect("supervisor lock").records.clone()
    }

    /// The committed ledger: count of attempts whose candidate actually
    /// published. Separate from diagnostics by construction.
    pub fn committed_generations(&self) -> u64 {
        self.state.lock().expect("supervisor lock").committed
    }
}

impl Default for SourceSupervisor {
    fn default() -> Self {
        Self::new()
    }
}

impl LoadAttempt {
    pub fn id(&self) -> AttemptId {
        self.id
    }

    /// Whether a retry trigger has superseded this attempt.
    pub fn is_superseded(&self) -> bool {
        self.state
            .lock()
            .expect("supervisor lock")
            .is_superseded(self.id)
    }

    /// Cancel this attempt: recorded in diagnostics, never committed. A
    /// cancel after the attempt already terminal-ended records nothing.
    pub fn cancel(self) {
        self.state
            .lock()
            .expect("supervisor lock")
            .terminal_row(self.id, AttemptDisposition::Cancelled);
    }
}
