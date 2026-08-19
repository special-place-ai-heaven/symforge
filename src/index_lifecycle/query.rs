//! Feature 020 V11 strict query leases (Slice 4, T063 — dark).
//!
//! Project/single-source strict leases, exact multi-project selections,
//! separate ranking snapshots, sealed completed-lease render authority with
//! post-lease `OutputCoverage`, `SourceRefusal` transport mapping, and the
//! committed-generation-versus-bounded-attempt health projections (frozen
//! tasks.md T063). Success and no-match require an exact all-`Current`
//! selection; anything else is a typed refusal, never a silently degraded
//! answer (frozen tasks.md T056).
//!
//! Dark payload simplifications, in the `runtime.rs` idiom: source content
//! is a generation stamp, rendering is a length-bounded stand-in, promotion
//! evidence is the committed candidate root (its content is not yet
//! consulted), and the four health surfaces are modeled INSIDE this module —
//! the live `health_view.rs`/`src/protocol/` wiring frozen T063 names is
//! activation work (T064/T066), because the darkness sweep forbids this
//! module's name in live files. The authority SEMANTICS — atomic
//! exact-bijection capture, refusal-not-no-match, stale/retarget
//! finalization fences, sealed render authority whose truncation cannot
//! change identity, protected roots reaching Current only through full
//! candidate promotion, and the two-ledger health split — are exact. The
//! promotion-evidence PAYLOAD is not: any committed candidate root counts
//! as evidence today, and binding it to the specific source is a recorded
//! cut obligation.
//!
//! **Nothing in production calls this module.** Only the Slice 4 oracle
//! suites and this directory do; activation (T064/T066) is the only planned
//! production caller.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;

use super::candidate::ProjectArtifacts;
use super::candidate::SourceId;
use super::supervisor::SourceSupervisor;

/// Why a strict selection refused. Every variant maps onto exactly one
/// stable transport code — refusals are typed on the wire, not prose.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SelectionRefusal {
    /// The selection names no sources.
    EmptySelection,
    /// The selection names a source the project does not have.
    MissingSource(SourceId),
    /// The selection names one source more than once.
    DuplicateSource(SourceId),
    /// The selection's expected generation does not match the source.
    MismatchedGeneration {
        source: SourceId,
        expected: u64,
        found: u64,
    },
    /// A selected source is not `Current`; the answer is this refusal,
    /// never a no-match.
    NotCurrent(SourceId),
    /// The captured generations drifted before finalization.
    StaleAtFinalization(SourceId),
    /// The project was retargeted while the lease was open.
    RetargetedDuringLease,
    /// The lease was finalized against a table that did not issue it.
    ForeignTable,
    /// A protected root may reach `Current` only through full candidate
    /// promotion.
    ProtectedRootRequiresPromotion(SourceId),
}

impl SelectionRefusal {
    /// The stable transport code for this refusal.
    pub fn transport_code(&self) -> &'static str {
        match self {
            SelectionRefusal::EmptySelection => "selection_empty",
            SelectionRefusal::MissingSource(_) => "selection_missing_source",
            SelectionRefusal::DuplicateSource(_) => "selection_duplicate_source",
            SelectionRefusal::MismatchedGeneration { .. } => "selection_generation_mismatch",
            SelectionRefusal::NotCurrent(_) => "source_not_current",
            SelectionRefusal::StaleAtFinalization(_) => "lease_stale_at_finalization",
            SelectionRefusal::RetargetedDuringLease => "lease_retargeted",
            SelectionRefusal::ForeignTable => "lease_foreign_table",
            SelectionRefusal::ProtectedRootRequiresPromotion(_) => {
                "protected_root_requires_promotion"
            }
        }
    }
}

/// Whether a rendering covered the whole answer. Truncation may appear only
/// AFTER a complete strict lease, and never changes identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputCoverage {
    Full,
    Truncated,
}

/// The identity a rendering carries: source truth and the cache/CCR
/// identity surrogate. Truncation must leave every field identical.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderIdentity {
    pub source_truth: BTreeMap<SourceId, u64>,
    pub cache_identity: u64,
}

/// One rendered answer from a completed lease.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rendering {
    pub coverage: OutputCoverage,
    pub identity: RenderIdentity,
    pub body_len: usize,
}

/// Outcome of a query through a completed lease. `NoMatch` is reachable
/// ONLY here — through an exact all-`Current` selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueryOutcome {
    Matches(usize),
    NoMatch,
}

/// One health surface. All four project the SAME two ledgers as two
/// separate numbers; none may sum, swap, or substitute them. (Committed
/// rows also appear in the bounded diagnostics ledger, so the ledgers are
/// separate, not disjoint — and committed may exceed the bounded row
/// count.)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HealthSurface {
    Health,
    HealthCompact,
    Status,
    HealthResource,
}

impl HealthSurface {
    pub const ALL: [HealthSurface; 4] = [
        HealthSurface::Health,
        HealthSurface::HealthCompact,
        HealthSurface::Status,
        HealthSurface::HealthResource,
    ];
}

/// The committed-versus-attempt split, projected: two disjoint ledgers as
/// two fields, never one number.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HealthProjection {
    pub committed_generations: u64,
    pub bounded_attempts: u64,
}

/// The caller's requested source set with expected generations — the
/// `SelectedAggregate` a strict lease must capture in exact bijection.
#[derive(Clone, Debug, Default)]
pub struct SelectedAggregate {
    pub sources: Vec<(SourceId, u64)>,
}

impl SelectedAggregate {
    pub fn of(sources: impl IntoIterator<Item = (u64, u64)>) -> Self {
        Self {
            sources: sources
                .into_iter()
                .map(|(id, generation)| (SourceId(id), generation))
                .collect(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SourceState {
    Current { generation: u64 },
    NonCurrent { last_generation: u64 },
}

static NEXT_RANKING_SNAPSHOT: AtomicU64 = AtomicU64::new(1);
static NEXT_TABLE_IDENTITY: AtomicU64 = AtomicU64::new(1);

/// The dark per-project query table: source states, protection roster,
/// target epoch, and its own never-reused identity — a lease binds to the
/// table that issued it, so a lookalike world cannot finalize it.
#[derive(Debug)]
pub struct ProjectQueryTable {
    identity: u64,
    sources: BTreeMap<SourceId, SourceState>,
    protected: BTreeSet<SourceId>,
    target_epoch: u64,
}

impl Default for ProjectQueryTable {
    fn default() -> Self {
        Self {
            identity: NEXT_TABLE_IDENTITY.fetch_add(1, Ordering::Relaxed),
            sources: BTreeMap::new(),
            protected: BTreeSet::new(),
            target_epoch: 0,
        }
    }
}

/// An open (not yet finalized) strict lease: the atomic capture.
#[derive(Debug)]
pub struct StrictSelectionLease {
    table_identity: u64,
    captured: BTreeMap<SourceId, u64>,
    ranking_snapshot: u64,
    target_epoch: u64,
}

/// A COMPLETED lease: the sealed render authority. Rendering exists only
/// here — truncation before completion is unrepresentable.
#[derive(Debug)]
pub struct CompletedLease {
    captured: BTreeMap<SourceId, u64>,
}

impl ProjectQueryTable {
    pub fn new() -> Self {
        Self::default()
    }

    /// Publish `source` as `Current` at `generation`. Protected roots have
    /// no bare-publication path at all — that door is the sealed negative
    /// below, so reaching here with one is a caller bug, not a refusal.
    pub fn publish_current(&mut self, source: SourceId, generation: u64) {
        assert!(
            !self.protected.contains(&source),
            "protected roots publish only through full candidate promotion"
        );
        self.sources
            .insert(source, SourceState::Current { generation });
    }

    /// Mark `source` non-Current, preserving the last generation so the
    /// machinery-owned allocator can never reuse a stamp across the
    /// invalidation.
    pub fn mark_non_current(&mut self, source: SourceId) {
        let last_generation = match self.sources.get(&source) {
            Some(SourceState::Current { generation }) => *generation,
            Some(SourceState::NonCurrent { last_generation }) => *last_generation,
            None => 0,
        };
        self.sources
            .insert(source, SourceState::NonCurrent { last_generation });
    }

    /// Declare `source` a PROTECTED root: `Current` only through full
    /// candidate promotion. Any existing bare `Current` state is DEMOTED —
    /// a newly protected root re-earns `Current` through promotion, so the
    /// publish-first-declare-later ordering hole does not exist.
    pub fn declare_protected(&mut self, source: SourceId) {
        self.protected.insert(source);
        if let Some(SourceState::Current { generation }) = self.sources.get(&source) {
            let last_generation = *generation;
            self.sources
                .insert(source, SourceState::NonCurrent { last_generation });
        }
    }

    /// The bare-publication door: legitimate for non-protected sources
    /// (caller-supplied generation, like `publish_current`), refused for
    /// protected roots — and the refusal changes nothing.
    pub fn publish_without_promotion(
        &mut self,
        source: SourceId,
        generation: u64,
    ) -> Result<(), SelectionRefusal> {
        if self.protected.contains(&source) {
            return Err(SelectionRefusal::ProtectedRootRequiresPromotion(source));
        }
        self.sources
            .insert(source, SourceState::Current { generation });
        Ok(())
    }

    /// Publish a protected root from a full candidate promotion. `probe` is
    /// the below-root state/durability probe the installation must NEVER
    /// call (frozen SC-019: zero probe I/O below the source root). The
    /// promotion evidence is the committed candidate root; its content is a
    /// recorded dark simplification — the publication generation is the
    /// source's own fresh sequence, starting at 1.
    pub fn publish_protected_from_promotion(
        &mut self,
        source: SourceId,
        promotion: &ProjectArtifacts,
        probe: impl FnMut(),
    ) -> Result<(), SelectionRefusal> {
        let _ = promotion;
        let _ = probe;
        let next = match self.sources.get(&source) {
            Some(SourceState::Current { generation }) => generation + 1,
            Some(SourceState::NonCurrent { last_generation }) => last_generation + 1,
            None => 1,
        };
        self.sources
            .insert(source, SourceState::Current { generation: next });
        Ok(())
    }

    /// Retarget the project: every open lease is fenced.
    pub fn retarget(&mut self) {
        self.target_epoch += 1;
    }

    /// Acquire a strict lease over `selection`: an ATOMIC capture of exactly
    /// the selected sources, all `Current`, in exact bijection — or a typed
    /// refusal, never a no-match.
    pub fn acquire_strict(
        &self,
        selection: &SelectedAggregate,
    ) -> Result<StrictSelectionLease, SelectionRefusal> {
        if selection.sources.is_empty() {
            return Err(SelectionRefusal::EmptySelection);
        }
        let mut captured = BTreeMap::new();
        for (source, expected) in &selection.sources {
            if captured.contains_key(source) {
                return Err(SelectionRefusal::DuplicateSource(*source));
            }
            match self.sources.get(source) {
                None => return Err(SelectionRefusal::MissingSource(*source)),
                Some(SourceState::NonCurrent { .. }) => {
                    return Err(SelectionRefusal::NotCurrent(*source));
                }
                Some(SourceState::Current { generation }) => {
                    if generation != expected {
                        return Err(SelectionRefusal::MismatchedGeneration {
                            source: *source,
                            expected: *expected,
                            found: *generation,
                        });
                    }
                    captured.insert(*source, *generation);
                }
            }
        }
        Ok(StrictSelectionLease {
            table_identity: self.identity,
            captured,
            ranking_snapshot: NEXT_RANKING_SNAPSHOT.fetch_add(1, Ordering::Relaxed),
            target_epoch: self.target_epoch,
        })
    }

    /// Project the health split for `surface` from `supervisor`'s two
    /// ledgers. All four surfaces report the same two numbers AS two
    /// numbers — none may sum, swap, or substitute them.
    pub fn health(
        &self,
        surface: HealthSurface,
        supervisor: &SourceSupervisor,
    ) -> HealthProjection {
        let _ = surface;
        HealthProjection {
            committed_generations: supervisor.committed_generations(),
            bounded_attempts: supervisor.attempt_records().len() as u64,
        }
    }
}

/// Acquire strict leases across SEVERAL projects, all-or-nothing: one
/// refusal anywhere poisons the whole multi-selection.
pub fn acquire_multi_project(
    selections: &[(&ProjectQueryTable, &SelectedAggregate)],
) -> Result<Vec<StrictSelectionLease>, SelectionRefusal> {
    let mut leases = Vec::with_capacity(selections.len());
    for (table, selection) in selections {
        leases.push(table.acquire_strict(selection)?);
    }
    Ok(leases)
}

fn cache_identity_of(captured: &BTreeMap<SourceId, u64>) -> u64 {
    captured
        .iter()
        .fold(0xcbf2_9ce4_8422_2325_u64, |acc, (source, generation)| {
            acc.wrapping_mul(0x0100_0000_01b3)
                .wrapping_add(source.0)
                .wrapping_mul(0x0100_0000_01b3)
                .wrapping_add(*generation)
        })
}

impl StrictSelectionLease {
    /// The captured (source → generation) snapshot — the exact bijection.
    pub fn captured(&self) -> BTreeMap<SourceId, u64> {
        self.captured.clone()
    }

    /// This lease's ranking snapshot identity — separate from content
    /// identity by construction.
    pub fn ranking_snapshot(&self) -> u64 {
        self.ranking_snapshot
    }

    /// Finalize against the table: refuses if the project retargeted or any
    /// captured generation went stale while the lease was open.
    pub fn finalize(self, table: &ProjectQueryTable) -> Result<CompletedLease, SelectionRefusal> {
        if table.identity != self.table_identity {
            return Err(SelectionRefusal::ForeignTable);
        }
        if table.target_epoch != self.target_epoch {
            return Err(SelectionRefusal::RetargetedDuringLease);
        }
        for (source, generation) in &self.captured {
            match table.sources.get(source) {
                Some(SourceState::Current { generation: live }) if live == generation => {}
                _ => return Err(SelectionRefusal::StaleAtFinalization(*source)),
            }
        }
        Ok(CompletedLease {
            captured: self.captured,
        })
    }
}

impl CompletedLease {
    /// Query through the sealed authority. `NoMatch` here is the ONLY
    /// legitimate no-match in the model.
    pub fn query(&self, needle: u64) -> QueryOutcome {
        let matches = self
            .captured
            .values()
            .filter(|generation| **generation == needle)
            .count();
        if matches == 0 {
            QueryOutcome::NoMatch
        } else {
            QueryOutcome::Matches(matches)
        }
    }

    /// Render the answer bounded to `max_len`. Truncation may change
    /// `coverage` and `body_len` — never `identity`.
    pub fn render(&self, max_len: usize) -> Rendering {
        let full_body: String = self
            .captured
            .iter()
            .map(|(source, generation)| format!("{}:{generation};", source.0))
            .collect();
        let coverage = if max_len >= full_body.len() {
            OutputCoverage::Full
        } else {
            OutputCoverage::Truncated
        };
        Rendering {
            coverage,
            identity: RenderIdentity {
                source_truth: self.captured.clone(),
                cache_identity: cache_identity_of(&self.captured),
            },
            body_len: full_body.len().min(max_len),
        }
    }
}

// ── Frozen seam anchors (C5) ───────────────────────────────────────────────

/// SEAM-KNOWLEDGE / SEAM-QUERY / SEAM-SURFACE anchor: the strict
/// per-project query lease — atomic exact-bijection selection over the
/// requested sources.
pub type ProjectQueryLease = StrictSelectionLease;

/// SEAM-QUERY anchor: the caller's requested source set with expected
/// generations — what a strict lease must capture exactly.
pub type QuerySelection = SelectedAggregate;

/// SEAM-HEALTH anchor: the committed-versus-attempt split, projected — two
/// disjoint ledgers as two fields, never one number.
pub type RuntimeHealthObservation = HealthProjection;

/// SEAM-STATE anchor: whether a checkpoint may be seeded from the queried
/// publication. The frozen FR-051/T065 rule makes a COMPLETE `Current`
/// publication the seed precondition; this projects that judgment at the
/// query seam instead of letting a checkpoint caller infer it from raw
/// state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CheckpointAvailability {
    /// Every selected source is `Current` — the seed precondition holds.
    pub current_complete: bool,
}
