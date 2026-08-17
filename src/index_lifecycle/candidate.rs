//! Feature 020 V11 isolated candidate pipeline (Slice 4, T060 — dark).
//!
//! Capacity-reserved isolated full and delta candidates with complete artifact
//! certificates and ONE runtime-store commit point. `CatalogPath`
//! native/opaque identity is preserved through candidate, manifest, and
//! promotion without lossy reconstruction; a prepared source delta
//! exact-validates only its changed source token and no-allocation patches the
//! LATEST whole project root so unrelated newer membership/source siblings
//! survive; same-source drift retries or aborts; numeric epochs never
//! authorize publication (frozen tasks.md T060).
//!
//! Dark payload simplifications, in the `runtime.rs` idiom: artifact digests
//! are stamps, not real derivations; the sealed `RequiredArtifactSet` compiler
//! is a fixed closed set. The authority SEMANTICS — isolation, the single
//! commit point, completeness, supersession, drift, sibling survival — are
//! exact; the payloads are recorded Slice 4/release obligations.
//!
//! **Nothing in production calls this module.** Only the Slice 4 oracle
//! suites and this directory do; activation (T064/T066) is the only planned
//! production caller.

use std::collections::BTreeMap;
use std::collections::HashMap;
use std::panic::AssertUnwindSafe;
use std::panic::catch_unwind;
use std::sync::Arc;
use std::sync::Mutex;

use crate::domain::index::CatalogPath;
use crate::domain::index::MetadataOnlyReason;

use super::capacity::CapacityPermit;
use super::capacity::CapacityRefusal;
use super::capacity::OwnerIdentity;
use super::capacity::ProcessCapacityPool;
use super::supervisor::AttemptId;
use super::supervisor::ClassifiedFailure;
use super::supervisor::LoadAttempt;
use super::supervisor::SupervisorState;

/// Identity of one source within its project's artifact root.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SourceId(pub u64);

/// Content stamp for one source at observation time — the "changed source
/// token" a delta exact-validates against. A token compares by value; it
/// carries no authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceContentToken(pub u64);

/// The closed required artifact set (dark stand-in for the sealed
/// `RequiredArtifactSet` compiler: only the set's IDENTITY crosses the seam;
/// callers cannot choose a subset or construct completeness).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ArtifactKind {
    Catalog,
    Symbols,
    References,
    Outline,
}

impl ArtifactKind {
    pub const REQUIRED: [ArtifactKind; 4] = [
        ArtifactKind::Catalog,
        ArtifactKind::Symbols,
        ArtifactKind::References,
        ArtifactKind::Outline,
    ];
}

/// A capability claim. Deliberately inert at the promotion gate: capability
/// certificates cannot authorize partial promotion, and this type exists so
/// the oracle can prove that refusal against a real value.
#[derive(Clone, Copy, Debug)]
pub struct CapabilityCertificate {
    _private: (),
}

impl CapabilityCertificate {
    pub fn for_test() -> Self {
        Self { _private: () }
    }
}

/// What one source looked like when the candidate observed it.
#[derive(Clone, Debug)]
pub enum SourceObservation {
    /// Textual content admitted for derivation.
    Content {
        path: CatalogPath,
        token: SourceContentToken,
        bytes: u64,
    },
    /// Catalog-only admission: present in the manifest, excluded from every
    /// content derivation, never content-probed.
    MetadataOnly {
        path: CatalogPath,
        reason: MetadataOnlyReason,
    },
    /// The observation itself failed with a classified cause.
    Failed(ClassifiedFailure),
}

/// One source's input to a candidate build.
#[derive(Clone, Debug)]
pub struct CandidateSource {
    pub id: SourceId,
    pub observation: SourceObservation,
}

/// Disposition of one manifest entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EntryDisposition {
    Indexed { content_stamp: u64 },
    MetadataOnly { reason: MetadataOnlyReason },
}

/// One promoted manifest row. The `path` is the EXACT `CatalogPath` the
/// observation carried — never reconstructed, never lossily respelled.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManifestEntry {
    pub path: CatalogPath,
    pub disposition: EntryDisposition,
}

/// The candidate's manifest: total over its observed sources.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CandidateManifest {
    pub entries: Vec<ManifestEntry>,
}

/// Per-source promoted artifacts (dark payloads).
#[derive(Debug)]
pub struct SourceArtifacts {
    pub token: SourceContentToken,
    pub manifest: CandidateManifest,
    pub artifacts: BTreeMap<ArtifactKind, u64>,
}

/// One published whole-project artifact snapshot. A delta patch replaces
/// exactly one source's `Arc`; every sibling's `Arc` survives identical.
#[derive(Debug, Default)]
pub struct ProjectArtifacts {
    pub sources: HashMap<SourceId, Arc<SourceArtifacts>>,
}

/// The LATEST whole project artifact root — the one runtime store the single
/// commit point publishes into.
#[derive(Debug, Default)]
pub struct ProjectArtifactRoot {
    pub(crate) inner: Mutex<Arc<ProjectArtifacts>>,
}

impl ProjectArtifactRoot {
    pub fn empty() -> Self {
        Self::default()
    }

    /// The current publication. Holders keep reading their snapshot after any
    /// later commit: publish happens before prune.
    pub fn load(&self) -> Arc<ProjectArtifacts> {
        Arc::clone(&self.inner.lock().expect("artifact root lock"))
    }

    /// Sealed negative: a bare numeric epoch is never publication authority.
    /// Nothing is published, unconditionally.
    pub fn publish_claiming_epoch_only(&self, epoch: u64) -> PromotionRefusal {
        let _ = epoch;
        PromotionRefusal::EpochIsNotAuthority
    }
}

/// Why a candidate refused to promote.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PromotionRefusal {
    /// A closed-matrix cause observed during build.
    Failure(ClassifiedFailure),
    /// The sealed required artifact set is not completely certified.
    IncompleteRequiredArtifacts { missing: Vec<ArtifactKind> },
    /// A capability certificate was offered in place of completeness.
    CapabilityCannotAuthorize,
    /// A retry trigger superseded the owning attempt.
    Superseded,
    /// The delta's changed source token no longer matches the live root.
    SameSourceDrift {
        expected: Option<SourceContentToken>,
        found: Option<SourceContentToken>,
    },
    /// A bare numeric epoch was presented as publication authority.
    EpochIsNotAuthority,
    /// The build panicked; the candidate was discarded whole.
    Panicked,
}

#[derive(Debug)]
struct DeltaSpec {
    changed: SourceId,
    expected: Option<SourceContentToken>,
}

#[derive(Debug)]
enum BuildOutcome {
    Ready {
        built: HashMap<SourceId, Arc<SourceArtifacts>>,
        delta: Option<DeltaSpec>,
    },
    Classified(ClassifiedFailure),
    Panicked,
}

/// A capacity-reserved isolated candidate: nothing it does is observable in
/// the project root until `commit`, its single runtime-store commit point.
/// The capacity permit is held for the candidate's whole life; dropping the
/// candidate (any terminal path) refunds the charge.
#[derive(Debug)]
pub struct IsolatedCandidate {
    outcome: BuildOutcome,
    attempt: AttemptId,
    supervisor: Arc<Mutex<SupervisorState>>,
    _permit: CapacityPermit,
}

fn reserved_bytes(sources: &[CandidateSource]) -> u64 {
    sources
        .iter()
        .map(|source| match &source.observation {
            SourceObservation::Content { bytes, .. } => *bytes,
            _ => 0,
        })
        .sum::<u64>()
        .max(1)
}

/// Build every source's artifacts in ISOLATION. A `Failed` observation
/// classifies the whole candidate; a panicking derivation discards it whole.
fn build_sources(
    sources: &[CandidateSource],
    derive: &mut dyn FnMut(&CandidateSource) -> u64,
) -> BuildOutcome {
    let mut built = HashMap::new();
    for source in sources {
        match &source.observation {
            SourceObservation::Failed(cause) => return BuildOutcome::Classified(*cause),
            SourceObservation::Content { path, token, .. } => {
                let stamp = match catch_unwind(AssertUnwindSafe(|| derive(source))) {
                    Ok(stamp) => stamp,
                    Err(_) => return BuildOutcome::Panicked,
                };
                let mut artifacts = BTreeMap::new();
                for (offset, kind) in ArtifactKind::REQUIRED.into_iter().enumerate() {
                    artifacts.insert(kind, stamp.wrapping_add(offset as u64));
                }
                built.insert(
                    source.id,
                    Arc::new(SourceArtifacts {
                        token: *token,
                        manifest: CandidateManifest {
                            entries: vec![ManifestEntry {
                                path: path.clone(),
                                disposition: EntryDisposition::Indexed {
                                    content_stamp: stamp,
                                },
                            }],
                        },
                        artifacts,
                    }),
                );
            }
            SourceObservation::MetadataOnly { path, reason } => {
                built.insert(
                    source.id,
                    Arc::new(SourceArtifacts {
                        token: SourceContentToken(0),
                        manifest: CandidateManifest {
                            entries: vec![ManifestEntry {
                                path: path.clone(),
                                disposition: EntryDisposition::MetadataOnly {
                                    reason: reason.clone(),
                                },
                            }],
                        },
                        artifacts: BTreeMap::new(),
                    }),
                );
            }
        }
    }
    BuildOutcome::Ready { built, delta: None }
}

impl IsolatedCandidate {
    /// Prepare a FULL candidate over `sources`, deriving content artifacts
    /// through `derive` (injected so oracles can observe probe counts and
    /// inject panics). Capacity is reserved before any work.
    pub fn prepare_full(
        pool: &Arc<ProcessCapacityPool>,
        owner: OwnerIdentity,
        attempt: &LoadAttempt,
        sources: Vec<CandidateSource>,
        mut derive: impl FnMut(&CandidateSource) -> u64,
    ) -> Result<Self, CapacityRefusal> {
        let permit = pool.redeem(pool.reserve(owner, reserved_bytes(&sources))?)?;
        let outcome = build_sources(&sources, &mut derive);
        Ok(Self {
            outcome,
            attempt: attempt.id,
            supervisor: Arc::clone(&attempt.state),
            _permit: permit,
        })
    }

    /// Prepare a DELTA candidate for exactly one changed source, valid only
    /// against `expected` — the changed source token it exact-validates
    /// (`None` expects the source to be new membership).
    pub fn prepare_delta(
        pool: &Arc<ProcessCapacityPool>,
        owner: OwnerIdentity,
        attempt: &LoadAttempt,
        changed: CandidateSource,
        expected: Option<SourceContentToken>,
        mut derive: impl FnMut(&CandidateSource) -> u64,
    ) -> Result<Self, CapacityRefusal> {
        let changed_id = changed.id;
        let sources = vec![changed];
        let permit = pool.redeem(pool.reserve(owner, reserved_bytes(&sources))?)?;
        let outcome = match build_sources(&sources, &mut derive) {
            BuildOutcome::Ready { built, .. } => BuildOutcome::Ready {
                built,
                delta: Some(DeltaSpec {
                    changed: changed_id,
                    expected,
                }),
            },
            terminal => terminal,
        };
        Ok(Self {
            outcome,
            attempt: attempt.id,
            supervisor: Arc::clone(&attempt.state),
            _permit: permit,
        })
    }

    /// Whether the build reached a terminal classified failure.
    pub fn classified_failure(&self) -> Option<ClassifiedFailure> {
        match &self.outcome {
            BuildOutcome::Classified(cause) => Some(*cause),
            _ => None,
        }
    }

    /// The ONE runtime-store commit point. A full candidate replaces the
    /// root; a delta candidate exact-validates its changed source token and
    /// patches the LATEST root — cloning only the `Arc` table, never sibling
    /// contents, so unrelated and newer siblings survive with their `Arc`s
    /// intact. The swap publishes BEFORE the prior generation is pruned:
    /// holders of the old `Arc` keep reading it.
    pub fn commit(
        self,
        root: &ProjectArtifactRoot,
    ) -> Result<Arc<ProjectArtifacts>, PromotionRefusal> {
        let mut supervisor = self.supervisor.lock().expect("supervisor lock");
        if supervisor.is_superseded(self.attempt) {
            return Err(PromotionRefusal::Superseded);
        }
        match self.outcome {
            BuildOutcome::Classified(cause) => {
                supervisor.record_discard(self.attempt, cause);
                Err(PromotionRefusal::Failure(cause))
            }
            BuildOutcome::Panicked => {
                supervisor.record_panic(self.attempt);
                Err(PromotionRefusal::Panicked)
            }
            BuildOutcome::Ready { built, delta } => {
                let mut inner = root.inner.lock().expect("artifact root lock");
                let next = match delta {
                    None => Arc::new(ProjectArtifacts { sources: built }),
                    Some(spec) => {
                        let found = inner.sources.get(&spec.changed).map(|arts| arts.token);
                        if found != spec.expected {
                            return Err(PromotionRefusal::SameSourceDrift {
                                expected: spec.expected,
                                found,
                            });
                        }
                        let changed = built
                            .get(&spec.changed)
                            .expect("a delta candidate builds its changed source");
                        let mut sources = inner.sources.clone();
                        sources.insert(spec.changed, Arc::clone(changed));
                        Arc::new(ProjectArtifacts { sources })
                    }
                };
                *inner = Arc::clone(&next);
                drop(inner);
                supervisor.record_commit(self.attempt);
                Ok(next)
            }
        }
    }

    /// Sealed negative: promotion of a partially-certified candidate on the
    /// strength of a capability certificate. Refused unconditionally —
    /// completeness comes from the sealed required set, never from a caller's
    /// capability.
    pub fn promote_partial_with_capability(
        self,
        root: &ProjectArtifactRoot,
        certificate: CapabilityCertificate,
    ) -> PromotionRefusal {
        let _ = (root, certificate);
        PromotionRefusal::CapabilityCannotAuthorize
    }
}
