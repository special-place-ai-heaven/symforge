//! Feature 020 V11 snapshot migration (Slice 4, T065 — dark).
//!
//! Bounded untrusted-seed restore, complete re-observation, quarantine,
//! preserved rollback, rebuild fallback, `.symforge/v11/` namespace
//! isolation, and the exact frozen-FR-051 team-artifact receipt/refusal
//! matrix (frozen tasks.md T065). Every pre-existing V10 byte is an
//! UNTRUSTED SEED: it may accelerate re-proof, never confer authority, and
//! nothing promotes to Current without complete current-process proof.
//!
//! Dark payload simplifications, in the `runtime.rs` idiom: seeds are
//! (source, stamp) vectors with an opaque note standing in for arbitrary V10
//! bytes, namespaces are in-memory models, and decode/proof are injected
//! closures so oracles can count and refuse. The live
//! `src/live_index/persist.rs` wiring frozen T065 names — the actual
//! `CURRENT_VERSION` bump and on-disk V11 write path — is activation work
//! (T064/T066), because the darkness sweep forbids this module's name in
//! live files; the companion in-scope `.gitattributes` write through the
//! mutation-permit path is likewise a sealed negative here and T064's work
//! there. The authority SEMANTICS — pre-decode capacity, digest quarantine
//! with rollback-preserved originals, all-or-nothing seed proof, V10/V11
//! namespace isolation, secret-canary bytes never entering V11 snapshots,
//! quarantine metadata, receipts, or diagnostics, and the FR-051 four-state
//! disclosure with no inferred shareability — are exact.
//!
//! **Nothing in production calls this module.** Only the Slice 4 oracle
//! suites and this directory do; activation (T064/T066) is the only planned
//! production caller.

use std::collections::BTreeMap;

/// One untrusted V10 seed: version, self-declared payload size (checked
/// BEFORE decode), a digest over the entries, the entries themselves, and
/// an opaque note standing in for arbitrary V10 bytes (possibly
/// secret-bearing — the canary oracle plants one here).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SnapshotSeed {
    /// Deliberately unread by the store: a seed is untrusted regardless of
    /// the version it claims.
    pub version: u32,
    pub declared_len: u64,
    pub root_digest: u64,
    pub entries: Vec<(u64, u64)>,
    pub opaque_note: Vec<u8>,
}

/// The digest the store recomputes over a seed's entries — public so
/// oracles can build valid and corrupted seeds.
pub fn seed_digest(entries: &[(u64, u64)]) -> u64 {
    entries
        .iter()
        .fold(0xcbf2_9ce4_8422_2325_u64, |acc, (source, stamp)| {
            acc.wrapping_mul(0x0100_0000_01b3)
                .wrapping_add(*source)
                .wrapping_mul(0x0100_0000_01b3)
                .wrapping_add(*stamp)
        })
}

/// Why a snapshot operation refused outright.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SnapshotRefusal {
    /// The seed declares more payload than the pre-decode capacity allows;
    /// nothing was decoded.
    SeedBeyondCapacity { declared: u64, limit: u64 },
    /// The seed LIED: its declaration passed, but actual decode consumption
    /// crossed the limit — aborted mid-decode, nothing promoted.
    CapacityExceededMidDecode { consumed: u64, limit: u64 },
    /// The binding class refuses team-artifact export BEFORE any mutation.
    BindingRefusesExport(BindingClass),
    /// A companion `.gitattributes` repository-content write requires the
    /// `SourceMutationPermit` path — activation (T064) work; this module
    /// cannot mint one, by design.
    GitattributesRequiresMutationPermit,
}

/// How a restore ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RestoreOutcome {
    /// Every entry was proven by current-process observation; all promoted.
    Promoted { sources: usize },
    /// At least one entry lacked proof: NOTHING promoted, rebuild required.
    SeedRejected { unproven: usize },
    /// The seed failed integrity: quarantined with its rollback payload
    /// preserved, rebuild required.
    Quarantined { id: u64 },
}

/// Quarantine metadata — digests, counts, and lengths only, never seed
/// byte CONTENT (`declared_digest` echoes the seed's numeric claim, which
/// the mismatch explanation requires).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuarantineMetadata {
    pub id: u64,
    pub declared_digest: u64,
    pub computed_digest: u64,
    pub entry_count: usize,
    pub opaque_len: usize,
}

/// The frozen FR-051 git-visibility states, exactly four.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GitVisibility {
    AlreadyTracked,
    UntrackedVisible,
    IgnoredForceAddRequired,
    // T038 round-1 repair: renamed from `GitVisibilityUnavailable` — the
    // stutter tripped `clippy::enum_variant_names` in the doorless embed
    // clippy lane the same round un-masked. No frozen contract names this
    // Rust identifier (the frozen wire label is persist.rs's separate
    // snake_case string "git_visibility_unavailable", whose identifier is
    // deliberately NOT renamed — see `ArtifactGitVisibility`).
    Unavailable,
}

impl GitVisibility {
    pub const ALL: [GitVisibility; 4] = [
        GitVisibility::AlreadyTracked,
        GitVisibility::UntrackedVisible,
        GitVisibility::IgnoredForceAddRequired,
        GitVisibility::Unavailable,
    ];
}

/// The binding classes FR-051 names; only a normal writable current project
/// may export.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BindingClass {
    NormalWritable,
    ExplicitProtected,
    ReadOnly,
    UserLocalOnly,
    MemoryOnly,
}

/// The export receipt: disclosing exactly one visibility state, placed in
/// the project state dir, and NEVER inferring shareability when git
/// visibility cannot be established.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportReceipt {
    pub visibility: GitVisibility,
}

impl ExportReceipt {
    /// `None` exactly when git visibility is unavailable — shareability is
    /// never inferred without it. The Some(..) booleans for the other three
    /// states are a DARK MODELING CHOICE, not frozen semantics: FR-051
    /// constrains only the None-when-unavailable case, and the oracle
    /// deliberately pins only that.
    pub fn shareability(&self) -> Option<bool> {
        match self.visibility {
            GitVisibility::AlreadyTracked | GitVisibility::UntrackedVisible => Some(true),
            GitVisibility::IgnoredForceAddRequired => Some(false),
            GitVisibility::Unavailable => None,
        }
    }
}

struct QuarantineEntry {
    metadata: QuarantineMetadata,
    /// The untrusted ORIGINAL, preserved byte-intact for one rollback.
    /// Retention is not disclosure: this payload never reaches metadata,
    /// receipts, diagnostics, or the V11 state — and the manual Debug below
    /// redacts it, so no future diagnostics line can leak it by accident.
    rollback_payload: Option<SnapshotSeed>,
}

impl std::fmt::Debug for QuarantineEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("QuarantineEntry")
            .field("metadata", &self.metadata)
            .field(
                "rollback_payload",
                &self
                    .rollback_payload
                    .as_ref()
                    .map(|seed| format!("<retained, {} opaque bytes>", seed.opaque_note.len())),
            )
            .finish()
    }
}

/// The dark `.symforge/v11/` store model: V10 namespace, V11 current state,
/// quarantine, team-artifact persistence, and bounded diagnostics.
#[derive(Debug, Default)]
pub struct SnapshotStore {
    v10: Vec<Vec<u8>>,
    current: BTreeMap<u64, u64>,
    quarantine: Vec<QuarantineEntry>,
    next_quarantine: u64,
    rebuild_required: bool,
    team_artifacts: Vec<usize>,
    diagnostics: Vec<String>,
}

impl SnapshotStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Append raw bytes to the V10 namespace model (a concurrent V10 writer).
    pub fn v10_write(&mut self, bytes: Vec<u8>) {
        self.v10.push(bytes);
    }

    /// The V10 namespace contents, for isolation oracles.
    pub fn v10_namespace(&self) -> Vec<Vec<u8>> {
        self.v10.clone()
    }

    /// Restore from an untrusted seed. `limit` is the PRE-decode capacity:
    /// a seed declaring more refuses before `decode` runs once. `decode` is
    /// the per-entry decoder (injected so oracles count invocations);
    /// `prove` is the current-process re-observation — only a seed whose
    /// EVERY entry proves promotes anything.
    pub fn restore(
        &mut self,
        seed: SnapshotSeed,
        limit: u64,
        mut decode: impl FnMut(&(u64, u64)) -> (u64, u64),
        prove: impl Fn(u64, u64) -> bool,
    ) -> Result<RestoreOutcome, SnapshotRefusal> {
        if seed.declared_len > limit {
            return Err(SnapshotRefusal::SeedBeyondCapacity {
                declared: seed.declared_len,
                limit,
            });
        }
        // The declaration is a fast-path check, never the enforcement: the
        // bound binds ACTUAL decode consumption (model stride: 16 bytes per
        // entry), so a seed that lies small aborts mid-decode.
        let mut decoded: Vec<(u64, u64)> = Vec::new();
        let mut consumed: u64 = 0;
        for entry in &seed.entries {
            consumed += 16;
            if consumed > limit {
                return Err(SnapshotRefusal::CapacityExceededMidDecode { consumed, limit });
            }
            decoded.push(decode(entry));
        }
        let computed = seed_digest(&decoded);
        if computed != seed.root_digest {
            let id = self.next_quarantine;
            self.next_quarantine += 1;
            self.diagnostics.push(format!(
                "seed {id}: digest mismatch, {} entries, opaque {} bytes",
                decoded.len(),
                seed.opaque_note.len()
            ));
            self.quarantine.push(QuarantineEntry {
                metadata: QuarantineMetadata {
                    id,
                    declared_digest: seed.root_digest,
                    computed_digest: computed,
                    entry_count: decoded.len(),
                    opaque_len: seed.opaque_note.len(),
                },
                rollback_payload: Some(seed),
            });
            self.rebuild_required = true;
            return Ok(RestoreOutcome::Quarantined { id });
        }
        let unproven = decoded
            .iter()
            .filter(|(source, stamp)| !prove(*source, *stamp))
            .count();
        if unproven > 0 {
            self.diagnostics.push(format!(
                "seed rejected: {unproven} of {} entries lack current-process proof",
                decoded.len()
            ));
            self.rebuild_required = true;
            return Ok(RestoreOutcome::SeedRejected { unproven });
        }
        let sources = decoded.len();
        self.current = decoded.into_iter().collect();
        self.diagnostics
            .push(format!("seed promoted: {sources} sources"));
        Ok(RestoreOutcome::Promoted { sources })
    }

    /// The promoted V11 current state.
    pub fn current(&self) -> BTreeMap<u64, u64> {
        self.current.clone()
    }

    /// Whether a rejected/quarantined seed left a rebuild obligation.
    pub fn rebuild_required(&self) -> bool {
        self.rebuild_required
    }

    /// Quarantine metadata rows (digests and lengths only, never bytes).
    pub fn quarantine_metadata(&self) -> Vec<QuarantineMetadata> {
        self.quarantine
            .iter()
            .map(|entry| entry.metadata.clone())
            .collect()
    }

    /// Roll a quarantined seed back out, byte-intact — handed out once.
    pub fn rollback(&mut self, id: u64) -> Option<SnapshotSeed> {
        self.quarantine
            .iter_mut()
            .find(|entry| entry.metadata.id == id)
            .and_then(|entry| entry.rollback_payload.take())
    }

    /// Bounded human-readable diagnostics (never seed bytes).
    pub fn diagnostics(&self) -> Vec<String> {
        self.diagnostics.clone()
    }

    /// Export the opt-in team artifact per frozen FR-051: only a normal
    /// writable binding may export (everything else refuses BEFORE any
    /// mutation), and the receipt disclosing the git-visibility state is
    /// persistence-only — no source mutation authority exists here.
    pub fn export_team_artifact(
        &mut self,
        binding: BindingClass,
        visibility: GitVisibility,
        artifact: Vec<u8>,
    ) -> Result<ExportReceipt, SnapshotRefusal> {
        if binding != BindingClass::NormalWritable {
            return Err(SnapshotRefusal::BindingRefusesExport(binding));
        }
        self.team_artifacts.push(artifact.len());
        Ok(ExportReceipt { visibility })
    }

    /// Team artifacts persisted so far (ProjectStateDir placement is the
    /// only placement that exists — user-local redirection is
    /// unrepresentable).
    pub fn team_artifacts(&self) -> usize {
        self.team_artifacts.len()
    }

    /// Sealed negative: a companion `.gitattributes` repository-content
    /// write through snapshot code. Refused unconditionally — that write
    /// belongs to the mutation-permit path (T064).
    pub fn export_with_gitattributes_change(
        &mut self,
        binding: BindingClass,
        visibility: GitVisibility,
    ) -> SnapshotRefusal {
        let _ = (binding, visibility);
        SnapshotRefusal::GitattributesRequiresMutationPermit
    }
}
