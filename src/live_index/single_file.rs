//! Single-file admission + publication seam (relocated from `watcher`,
//! task #24) — the ONE canonical path a single repository file takes into the
//! live index: hard-scope/source/gitignore exclusion checks, the same
//! metadata-first scout and admission classification bulk load runs (including
//! secret-content demotion), content-hash-gated parsing, and
//! generation-fenced publication.
//!
//! Compiled under BOTH the server and embed features: the watcher, freshen,
//! and reconciliation paths consume it via `crate::watcher`'s re-export, and
//! the embed facade exposes [`update_file_from_disk`] / [`remove_file`] so an
//! embedder's per-file reindex goes through the identical seam instead of a
//! hand-rolled parse-and-poke (which would bypass admission entirely).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tracing::{debug, trace, warn};

use crate::domain::{
    FileClassification, FileDisposition, LanguageId, MetadataOnlyReason, ScoutDecision,
};
use crate::hash;
use crate::live_index::store::{
    FencedRemoval, IndexedFile, PublicationFence, PublishedGeneration, SharedIndex,
};
use crate::parsing;

/// Result of a single re-index attempt for one file.
#[derive(Debug, PartialEq, Eq)]
pub enum ReindexResult {
    /// Content hash matched existing entry — tree-sitter parse was skipped.
    HashSkip,
    /// File was re-parsed and the index was updated.
    Reindexed,
    /// File classified as Tier 2/3 by the admission gate — NOT parsed/inserted.
    /// Any prior Tier-1 entry was removed and a skip record recorded. The index
    /// remains free of this path's symbols.
    Skipped,
    /// ENOENT observed by `read_and_index`; caller decides whether to retry or treat as confirmed-absent.
    NotFound,
    /// File was not found (ENOENT) — it has been removed from the index.
    Removed,
    /// File could not be read for a reason other than ENOENT.
    ReadError(String),
}

/// Crate-internal result of a single-file publication attempt.
///
/// `ReindexResult` is part of the semver-public embed facade and downstream
/// callers can exhaustively match its six variants. Publication contention is
/// therefore carried on this private result instead of widening that public
/// enum in a patch release.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ReindexOutcome {
    HashSkip,
    Reindexed,
    Skipped,
    NotFound,
    Removed,
    ReadError(String),
    /// The observed change did not reach a publication boundary. Callers must
    /// retry or refuse rather than claim the stale state was reconciled.
    PublicationRejected,
}

impl ReindexOutcome {
    fn into_public_compat(self) -> ReindexResult {
        match self {
            Self::HashSkip => ReindexResult::HashSkip,
            Self::Reindexed => ReindexResult::Reindexed,
            Self::Skipped => ReindexResult::Skipped,
            Self::NotFound => ReindexResult::NotFound,
            Self::Removed => ReindexResult::Removed,
            Self::ReadError(error) => ReindexResult::ReadError(error),
            // Before this patch the public update seam reported a lost
            // publication race as Skipped. Preserve that compatibility only at
            // the frozen embed boundary; in-crate callers retain the typed
            // rejection and must retry or refuse.
            Self::PublicationRejected => ReindexResult::Skipped,
        }
    }
}

pub(crate) struct ReindexReceipt {
    pub outcome: ReindexOutcome,
    /// Publication fence captured immediately before this attempt's
    /// filesystem observation.
    pub observed_at: PublicationFence,
    /// Exact immutable generation returned by the winning publication seam.
    pub published: Option<Arc<PublishedGeneration>>,
    pub snapshot_created: bool,
}

impl ReindexReceipt {
    fn observed(outcome: ReindexOutcome, observed_at: PublicationFence) -> Self {
        Self {
            outcome,
            observed_at,
            published: None,
            snapshot_created: false,
        }
    }

    fn published(
        outcome: ReindexOutcome,
        observed_at: PublicationFence,
        published: Arc<PublishedGeneration>,
    ) -> Self {
        Self {
            outcome,
            observed_at,
            published: Some(published),
            snapshot_created: false,
        }
    }
}

/// Content-hash-gated single-file re-index.
///
/// Reads the file, compares its hash against the existing index entry, and
/// skips the expensive tree-sitter parse when the hash matches.
///
/// # Lock discipline
/// The write lock is **never** held during the tree-sitter parse. The sequence is:
/// 1. Read file bytes (no lock)
/// 2. Acquire read lock → compare hash → drop read lock
/// 3. Parse (no lock)
/// 4. Acquire write lock → update_file → drop write lock
pub(crate) fn maybe_reindex<L>(
    relative_path: &str,
    abs_path: &Path,
    shared: &SharedIndex,
    language: L,
    expected_gen: u64,
) -> ReindexOutcome
where
    L: Into<Option<LanguageId>>,
{
    let language = language.into();
    let first =
        read_and_index_with_receipt(relative_path, abs_path, shared, language, expected_gen);
    if first.observed_at.project_generation != expected_gen {
        return ReindexOutcome::PublicationRejected;
    }
    let mut last_absence_fence = match first.outcome {
        ReindexOutcome::NotFound => first.observed_at,
        other => return other,
    };

    let delays_ms = [50u64, 200, 500];
    for delay_ms in delays_ms {
        std::thread::sleep(std::time::Duration::from_millis(delay_ms));
        let retry =
            read_and_index_with_receipt(relative_path, abs_path, shared, language, expected_gen);
        if retry.observed_at.project_generation != expected_gen {
            return ReindexOutcome::PublicationRejected;
        }
        match retry.outcome {
            ReindexOutcome::NotFound => last_absence_fence = retry.observed_at,
            other => return other,
        }
    }

    finalize_missing_file(
        shared,
        relative_path,
        abs_path,
        expected_gen,
        last_absence_fence,
    )
}

fn finalize_missing_file(
    shared: &SharedIndex,
    relative_path: &str,
    abs_path: &Path,
    expected_gen: u64,
    absence_fence: PublicationFence,
) -> ReindexOutcome {
    if absence_fence.project_generation != expected_gen {
        return ReindexOutcome::PublicationRejected;
    }
    match shared.remove_file_if_absent_at_publication_fence_with_receipt(
        relative_path,
        abs_path,
        absence_fence,
    ) {
        FencedRemoval::Removed(_) => {
            warn!("watcher: file not found after retries, removed from index: {relative_path}");
            ReindexOutcome::Removed
        }
        // Nothing held this path anywhere: the absence is confirmed and there
        // is no removal to publish. Reporting `Removed` (or publishing) here
        // would claim an operation nothing observed (the D14 defect).
        FencedRemoval::NothingHeld => {
            trace!(
                "watcher: file not found and nothing held, no removal to publish: {relative_path}"
            );
            ReindexOutcome::NotFound
        }
        FencedRemoval::Rejected => {
            trace!(
                "watcher: file not found after retries, stale publication rejected remove: {relative_path}"
            );
            ReindexOutcome::PublicationRejected
        }
    }
}

/// Recover the project root by walking up from the absolute event path once per
/// component of the relative path. Both come from the same watcher event (or
/// freshen-on-read call), so the suffix relationship holds by construction;
/// `None` only if the relative path is deeper than the absolute one.
fn project_root_from_paths(abs_path: &Path, relative_path: &str) -> Option<PathBuf> {
    let depth = Path::new(relative_path).components().count();
    abs_path.ancestors().nth(depth).map(|p| p.to_path_buf())
}

fn catalog_terminal_disposition(decision: &ScoutDecision) -> Option<FileDisposition> {
    match decision {
        ScoutDecision::HardSkip { reason } => Some(FileDisposition::HardSkip { reason: *reason }),
        ScoutDecision::MetadataOnly { reason } => Some(FileDisposition::MetadataOnly {
            reason: reason.clone(),
        }),
        ScoutDecision::Ingest { .. } | ScoutDecision::Unavailable { .. } => None,
    }
}

pub(crate) fn read_and_index<L>(
    relative_path: &str,
    abs_path: &Path,
    shared: &SharedIndex,
    language: L,
    expected_gen: u64,
) -> ReindexOutcome
where
    L: Into<Option<LanguageId>>,
{
    read_and_index_with_receipt(relative_path, abs_path, shared, language, expected_gen).outcome
}

fn read_and_index_with_receipt<L>(
    relative_path: &str,
    abs_path: &Path,
    shared: &SharedIndex,
    language: L,
    expected_gen: u64,
) -> ReindexReceipt
where
    L: Into<Option<LanguageId>>,
{
    read_and_index_with_stable_read_receipt(
        relative_path,
        abs_path,
        shared,
        language,
        expected_gen,
        crate::live_index::store::stable_read_file,
    )
}

/// Run one canonical repository-relative path through the same metadata-first
/// admission and publication seam used by watcher events and reconciliation.
pub(crate) fn admit_and_index_single_path(
    relative_path: &str,
    abs_path: &Path,
    shared: &SharedIndex,
    expected_gen: u64,
) -> ReindexOutcome {
    admit_and_index_single_path_with_receipt(relative_path, abs_path, shared, expected_gen).outcome
}

pub(crate) fn admit_and_index_single_path_with_receipt(
    relative_path: &str,
    abs_path: &Path,
    shared: &SharedIndex,
    expected_gen: u64,
) -> ReindexReceipt {
    read_and_index_with_stable_read_receipt(
        relative_path,
        abs_path,
        shared,
        None,
        expected_gen,
        crate::live_index::store::stable_read_file,
    )
}

#[cfg(test)]
pub(crate) fn read_and_index_with_stable_read<L, R>(
    relative_path: &str,
    abs_path: &Path,
    shared: &SharedIndex,
    language: L,
    expected_gen: u64,
    stable_read: R,
) -> ReindexOutcome
where
    L: Into<Option<LanguageId>>,
    R: FnMut(&Path, &crate::domain::FileStamp) -> crate::live_index::store::StableReadOutcome,
{
    read_and_index_with_stable_read_receipt(
        relative_path,
        abs_path,
        shared,
        language,
        expected_gen,
        stable_read,
    )
    .outcome
}

fn read_and_index_with_stable_read_receipt<L, R>(
    relative_path: &str,
    abs_path: &Path,
    shared: &SharedIndex,
    language: L,
    expected_gen: u64,
    mut stable_read: R,
) -> ReindexReceipt
where
    L: Into<Option<LanguageId>>,
    R: FnMut(&Path, &crate::domain::FileStamp) -> crate::live_index::store::StableReadOutcome,
{
    let language = language.into();
    let mut base = shared.published_generation();
    let mut observed_at = PublicationFence::from_published(&base);
    // Keep single-file watcher/freshen paths symmetric with the bulk walk.
    // Both universal name exclusions and placement-derived subtree exclusions
    // run before any metadata/content read from VCS or runtime-state internals.
    let relative = Path::new(relative_path);
    if crate::discovery::path_is_hard_scope_excluded(relative)
        || shared.is_source_excluded(relative)
        || shared.read().is_path_gitignored(relative_path)
    {
        if observed_at.project_generation != expected_gen {
            return ReindexReceipt::observed(ReindexOutcome::PublicationRejected, observed_at);
        }
        return match shared
            .remove_file_at_publication_fence_with_receipt(relative_path, observed_at)
        {
            FencedRemoval::Removed(published) => {
                debug!("watcher: source-scope eviction {relative_path}");
                ReindexReceipt::published(ReindexOutcome::Skipped, observed_at, published)
            }
            // The excluded path was never held: the exclusion is already the
            // published state, so there is no eviction to publish.
            FencedRemoval::NothingHeld => {
                trace!("watcher: source-scope exclusion already reconciled {relative_path}");
                ReindexReceipt::observed(ReindexOutcome::Skipped, observed_at)
            }
            FencedRemoval::Rejected => {
                trace!("watcher: source-scope eviction publication rejected {relative_path}");
                ReindexReceipt::observed(ReindexOutcome::PublicationRejected, observed_at)
            }
        };
    }

    const MAX_PUBLICATION_ATTEMPTS: usize = 4;
    for attempt in 1..=MAX_PUBLICATION_ATTEMPTS {
        base = shared.published_generation();
        observed_at = PublicationFence::from_published(&base);
        // The store mutation CAS protects the health/live index base, whose
        // generation is intentionally distinct from the full publication
        // bundle generation. Bridge- or authority-only publishes can advance
        // the latter while retaining this same live-index base.
        let expected_index_state_generation = base.health.generation;

        // Run the same metadata-first scout as cold load before any whole-file read.
        let mut scouted = match crate::discovery::scout_single_path(relative_path, abs_path) {
            Ok(scouted) => scouted,
            Err(error) => {
                if matches!(
                    std::fs::metadata(abs_path),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound
                ) {
                    return ReindexReceipt::observed(ReindexOutcome::NotFound, observed_at);
                }
                warn!("watcher: failed to scout {relative_path}: {error}");
                return ReindexReceipt::observed(
                    ReindexOutcome::ReadError(error.to_string()),
                    observed_at,
                );
            }
        };
        let mtime_secs = scouted
            .stamp
            .modified_hint
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|duration| duration.as_secs())
            .unwrap_or(0);

        if let Some(disposition) = catalog_terminal_disposition(&scouted.decision) {
            if let Some(published) = shared.publish_terminal_disposition_at_generation(
                relative_path,
                scouted,
                disposition,
                expected_gen,
                expected_index_state_generation,
            ) {
                debug!("watcher: metadata-terminal admission {relative_path}");
                return ReindexReceipt::published(ReindexOutcome::Skipped, observed_at, published);
            }
            if shared.current_project_generation() == expected_gen
                && shared.published_state().generation != expected_index_state_generation
            {
                trace!(attempt, "watcher: retrying metadata-terminal publication");
                continue;
            }
            trace!("watcher: stale metadata-terminal admission rejected: {relative_path}");
            return ReindexReceipt::observed(ReindexOutcome::PublicationRejected, observed_at);
        }
        if let ScoutDecision::Unavailable { stage, kind } = &scouted.decision {
            if *stage == crate::domain::AccessStage::Metadata
                && *kind == crate::domain::AccessErrorKind::NotFound
            {
                return ReindexReceipt::observed(ReindexOutcome::NotFound, observed_at);
            }
            let disposition = FileDisposition::Unreadable {
                stage: *stage,
                kind: *kind,
            };
            if let Some(published) = shared.publish_terminal_disposition_at_generation(
                relative_path,
                scouted,
                disposition,
                expected_gen,
                expected_index_state_generation,
            ) {
                return ReindexReceipt::published(
                    ReindexOutcome::ReadError("single-path scout unavailable".to_string()),
                    observed_at,
                    published,
                );
            }
            if shared.current_project_generation() == expected_gen
                && shared.published_state().generation != expected_index_state_generation
            {
                trace!(attempt, "watcher: retrying unavailable publication");
                continue;
            }
            return ReindexReceipt::observed(ReindexOutcome::PublicationRejected, observed_at);
        }

        // Generated-output placement is metadata policy too; evaluate it before
        // loading content, matching the cold-walk ordering.
        if let Some(root) = project_root_from_paths(abs_path, relative_path)
            && crate::discovery::is_untracked_generated_output_path(&root, relative_path)
        {
            let decision = ScoutDecision::MetadataOnly {
                reason: MetadataOnlyReason::GeneratedOrVendor,
            };
            let disposition = catalog_terminal_disposition(&decision)
                .expect("metadata-only decision must project to a terminal disposition");
            scouted.decision = decision;
            if let Some(published) = shared.publish_terminal_disposition_at_generation(
                relative_path,
                scouted,
                disposition,
                expected_gen,
                expected_index_state_generation,
            ) {
                return ReindexReceipt::published(ReindexOutcome::Skipped, observed_at, published);
            }
            if shared.current_project_generation() == expected_gen
                && shared.published_state().generation != expected_index_state_generation
            {
                trace!(attempt, "watcher: retrying generated-output publication");
                continue;
            }
            return ReindexReceipt::observed(ReindexOutcome::PublicationRejected, observed_at);
        }

        let targets = match &scouted.decision {
            ScoutDecision::Ingest { targets } => *targets,
            _ => unreachable!("terminal scout decisions return before content ingestion"),
        };

        let language = language.or(scouted.language).unwrap_or(LanguageId::Text);

        let bytes = match stable_read(abs_path, &scouted.stamp) {
            crate::live_index::store::StableReadOutcome::Accepted { bytes, .. } => bytes,
            crate::live_index::store::StableReadOutcome::HardSkip { reason } => {
                let decision = ScoutDecision::HardSkip { reason };
                let disposition = catalog_terminal_disposition(&decision)
                    .expect("hard-skip decision must project to a terminal disposition");
                scouted.decision = decision;
                if let Some(published) = shared.publish_terminal_disposition_at_generation(
                    relative_path,
                    scouted,
                    disposition,
                    expected_gen,
                    expected_index_state_generation,
                ) {
                    return ReindexReceipt::published(
                        ReindexOutcome::Skipped,
                        observed_at,
                        published,
                    );
                }
                if shared.current_project_generation() == expected_gen
                    && shared.published_state().generation != expected_index_state_generation
                {
                    trace!(
                        attempt,
                        "watcher: retrying stable-read hard-skip publication"
                    );
                    continue;
                }
                return ReindexReceipt::observed(ReindexOutcome::PublicationRejected, observed_at);
            }
            crate::live_index::store::StableReadOutcome::Unreadable { stage, kind } => {
                let publication = shared.publish_terminal_disposition_at_generation(
                    relative_path,
                    scouted,
                    FileDisposition::Unreadable { stage, kind },
                    expected_gen,
                    expected_index_state_generation,
                );
                if let Some(published) = publication {
                    return ReindexReceipt::published(
                        ReindexOutcome::ReadError("stable read unavailable".to_string()),
                        observed_at,
                        published,
                    );
                }
                if shared.current_project_generation() == expected_gen
                    && shared.published_state().generation != expected_index_state_generation
                {
                    trace!(
                        attempt,
                        "watcher: retrying stable-read unavailable publication"
                    );
                    continue;
                }
                return ReindexReceipt::observed(ReindexOutcome::PublicationRejected, observed_at);
            }
            crate::live_index::store::StableReadOutcome::UnstableDuringRead => {
                let publication = shared.publish_terminal_disposition_at_generation(
                    relative_path,
                    scouted,
                    FileDisposition::UnstableDuringRead,
                    expected_gen,
                    expected_index_state_generation,
                );
                if let Some(published) = publication {
                    return ReindexReceipt::published(
                        ReindexOutcome::ReadError("file changed during stable read".to_string()),
                        observed_at,
                        published,
                    );
                }
                if shared.current_project_generation() == expected_gen
                    && shared.published_state().generation != expected_index_state_generation
                {
                    trace!(attempt, "watcher: retrying unstable-read publication");
                    continue;
                }
                return ReindexReceipt::observed(ReindexOutcome::PublicationRejected, observed_at);
            }
        };

        // The watcher shares the exact stable-content admission boundary used
        // by cold load. Terminal outcomes are published before hashing or parsing,
        // and the owned byte buffer is discarded on every non-admitted path.
        if let crate::knowledge::StableContentAdmission::MetadataOnly(reason) =
            crate::knowledge::classify_stable_content(relative_path, targets, &bytes)
        {
            if let Some(published) = shared.publish_terminal_disposition_at_generation(
                relative_path,
                scouted,
                FileDisposition::MetadataOnly { reason },
                expected_gen,
                expected_index_state_generation,
            ) {
                return ReindexReceipt::published(ReindexOutcome::Skipped, observed_at, published);
            }
            if shared.current_project_generation() == expected_gen
                && shared.published_state().generation != expected_index_state_generation
            {
                trace!(attempt, "watcher: retrying content-policy publication");
                continue;
            }
            return ReindexReceipt::observed(ReindexOutcome::PublicationRejected, observed_at);
        }

        // Compute the hash and compare without holding the publication writer lock.
        let new_hash = hash::digest_hex(&bytes);
        {
            let index = shared.read();
            if let Some(existing) = index.get_file(relative_path)
                && existing.content_hash == new_hash
            {
                drop(index);
                if let Some(published) = shared.publish_hash_skip_at_generation(
                    relative_path,
                    mtime_secs,
                    scouted,
                    targets,
                    expected_gen,
                    expected_index_state_generation,
                ) {
                    debug!("watcher: hash-skip {relative_path}");
                    return ReindexReceipt {
                        outcome: ReindexOutcome::HashSkip,
                        observed_at,
                        published: Some(published),
                        snapshot_created: false,
                    };
                }
                if shared.current_project_generation() == expected_gen
                    && shared.published_state().generation != expected_index_state_generation
                {
                    trace!(attempt, "watcher: retrying hash-skip publication");
                    continue;
                }
                return ReindexReceipt::observed(ReindexOutcome::PublicationRejected, observed_at);
            }
        }

        // Parse outside the lock (Tier-1 only).
        let result = parsing::process_file_with_classification(
            relative_path,
            &bytes,
            language,
            FileClassification::for_indexed_path(relative_path, targets),
        );
        let indexed = IndexedFile::from_parse_result(result, bytes).with_mtime(mtime_secs);

        // Commit every observable lane under one source-base fence and one
        // publication boundary. If another update won while this build was
        // off-lock, retry from disk instead of overwriting that newer bundle.
        if let Some(publication) = shared.publish_indexed_file_at_generation(
            relative_path,
            indexed,
            scouted,
            targets,
            expected_gen,
            expected_index_state_generation,
        ) {
            debug!("watcher: re-indexed {relative_path}");
            return ReindexReceipt {
                outcome: ReindexOutcome::Reindexed,
                observed_at,
                published: Some(publication.published),
                snapshot_created: publication.snapshot_created,
            };
        }
        if shared.current_project_generation() == expected_gen
            && shared.published_state().generation != expected_index_state_generation
        {
            trace!(attempt, "watcher: retrying indexed-file publication");
            continue;
        }
        trace!("watcher: stale indexed-file publication rejected: {relative_path}");
        return ReindexReceipt::observed(ReindexOutcome::PublicationRejected, observed_at);
    }

    warn!(
        "watcher: aborting {relative_path} after {MAX_PUBLICATION_ATTEMPTS} concurrent publications"
    );
    ReindexReceipt::observed(ReindexOutcome::PublicationRejected, observed_at)
}

/// Re-index one repository file FROM DISK through the canonical single-file
/// admission seam (embed facade; task #24 / AAP ask 3).
///
/// `relative_path` is the repo-relative, forward-slash path (backslashes are
/// normalized). The file's current bytes are read from
/// `repo_root/relative_path` with the same stable-read, admission, hash-skip,
/// and fenced-publication behavior the watcher applies — an embedder needs no
/// parsing or store internals, and a file the admission gate demotes is
/// recorded as skipped, never silently parsed.
///
/// No retries, no sleeps: a missing file returns [`ReindexResult::NotFound`]
/// immediately (deadline-friendly for in-VM embedders); callers tracking a
/// deletion should follow up with [`remove_file`].
pub fn update_file_from_disk(
    shared: &SharedIndex,
    repo_root: &Path,
    relative_path: &str,
) -> ReindexResult {
    let relative = relative_path.replace('\\', "/");
    let abs_path = repo_root.join(&relative);
    let expected_gen = shared.current_project_generation();
    let outcome = admit_and_index_single_path(&relative, &abs_path, shared, expected_gen)
        .into_public_compat();
    // V11 observation lane (Feature 020 Slice 4, T029): an admission that
    // MUTATED the index is observed through the isolated candidate pipeline,
    // permit-free, attributed to the current incarnation (this synchronous
    // facade holds no id across time). D1 applies: the LiveIndex mutation
    // above IS the data plane mid-cut; the lane runs the frozen lifecycle
    // semantics beside it. A hash-skip observed no change.
    if matches!(outcome, ReindexResult::Reindexed) {
        crate::live_index::index_lifecycle::activation::project_source_authority(repo_root)
            .observe_admission_active(&relative);
    }
    outcome
}

/// Remove one file from the live index (embed facade; task #24 / AAP ask 3).
///
/// Generation-fenced like the watcher's removals: returns `true` when the
/// removal was APPLIED under the current project generation (including the
/// no-op case where the path was not tracked), `false` only when a concurrent
/// retarget invalidated the generation fence.
pub fn remove_file(shared: &SharedIndex, relative_path: &str) -> bool {
    let relative = relative_path.replace('\\', "/");
    let outcome =
        shared.remove_file_outcome_at_generation(&relative, shared.current_project_generation());
    // V11 observation lane (T029): an applied removal lands as an
    // accumulator invalidation (the dark candidate pipeline carries no
    // removal payload — recorded bridging simplification). Fixture indexes
    // with no bound root have no source authority to observe through.
    // `NothingHeld` observes nothing: no removal happened, and nothing
    // published (the D14 fence) — but the facade contract still reports the
    // fence-held no-op as applied.
    if matches!(outcome, FencedRemoval::Removed(_))
        && let Some(root) = shared.read().indexed_root.clone()
    {
        crate::live_index::index_lifecycle::activation::project_source_authority(&root)
            .observe_removal_active(&relative);
    }
    !matches!(outcome, FencedRemoval::Rejected)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::live_index::LiveIndex;

    /// Task #24: the facade per-file entry points drive the SAME admission
    /// seam — index a new file, see its symbols, update it, remove it — with
    /// no parsing/store imports below the facade.
    #[test]
    fn facade_update_and_remove_round_trip() {
        let dir = tempfile::TempDir::new().expect("root");
        std::fs::write(
            dir.path().join("lib.rs"),
            "pub fn first() {}
",
        )
        .expect("seed");
        let shared = LiveIndex::load(dir.path()).expect("cold load");
        assert_eq!(shared.read().file_count(), 1);

        // New file appears through the seam.
        std::fs::write(
            dir.path().join("extra.rs"),
            "pub fn second() {}
",
        )
        .expect("new file");
        let outcome = update_file_from_disk(&shared, dir.path(), "extra.rs");
        assert!(
            matches!(outcome, ReindexResult::Reindexed),
            "new file must be admitted and parsed, got {outcome:?}"
        );
        assert_eq!(shared.read().file_count(), 2);

        // Unchanged content is hash-skipped, not re-parsed.
        let outcome = update_file_from_disk(&shared, dir.path(), "extra.rs");
        assert!(
            matches!(outcome, ReindexResult::HashSkip),
            "unchanged file must hash-skip, got {outcome:?}"
        );

        // Edited content re-parses.
        std::fs::write(
            dir.path().join("extra.rs"),
            "pub fn renamed() {}
",
        )
        .expect("edit");
        let outcome = update_file_from_disk(&shared, dir.path(), "extra.rs");
        assert!(
            matches!(outcome, ReindexResult::Reindexed),
            "edited file must re-parse, got {outcome:?}"
        );

        // Removal is fenced: applied under the current generation (a second
        // removal is an applied no-op, still fence-true).
        assert!(remove_file(&shared, "extra.rs"), "tracked file removes");
        assert_eq!(shared.read().file_count(), 1);
        assert!(
            remove_file(&shared, "extra.rs"),
            "no-op removal still passes the generation fence"
        );
        assert_eq!(shared.read().file_count(), 1);

        // Missing file reports NotFound without retries.
        let outcome = update_file_from_disk(&shared, dir.path(), "ghost.rs");
        assert!(
            matches!(outcome, ReindexResult::NotFound),
            "missing file must be NotFound, got {outcome:?}"
        );
    }

    /// Feature 020 Slice 4, T029 (observation lane): every facade admission
    /// that MUTATED the index also drives the isolated candidate pipeline to
    /// its commit point, permit-free; a hash-skip observes no change and
    /// commits nothing.
    #[test]
    fn facade_admission_feeds_the_observation_lane() {
        let dir = tempfile::TempDir::new().expect("root");
        std::fs::write(dir.path().join("lib.rs"), "pub fn first() {}\n").expect("seed");
        let shared = LiveIndex::load(dir.path()).expect("cold load");
        let authority =
            crate::live_index::index_lifecycle::activation::project_source_authority(dir.path());
        assert_eq!(authority.committed_observations("extra.rs"), 0);

        std::fs::write(dir.path().join("extra.rs"), "pub fn second() {}\n").expect("new file");
        assert!(matches!(
            update_file_from_disk(&shared, dir.path(), "extra.rs"),
            ReindexResult::Reindexed
        ));
        assert_eq!(
            authority.committed_observations("extra.rs"),
            1,
            "an admitted mutation must reach the candidate commit point"
        );

        assert!(matches!(
            update_file_from_disk(&shared, dir.path(), "extra.rs"),
            ReindexResult::HashSkip
        ));
        assert_eq!(
            authority.committed_observations("extra.rs"),
            1,
            "a hash-skip observed no change and commits nothing"
        );

        std::fs::write(dir.path().join("extra.rs"), "pub fn renamed() {}\n").expect("edit");
        assert!(matches!(
            update_file_from_disk(&shared, dir.path(), "extra.rs"),
            ReindexResult::Reindexed
        ));
        assert_eq!(authority.committed_observations("extra.rs"), 2);

        assert!(remove_file(&shared, "extra.rs"));
        assert_eq!(
            authority.committed_observations("extra.rs"),
            2,
            "a removal rides the accumulator, not the candidate pipeline \
             (dark bridging: no removal payload exists yet)"
        );
    }

    #[test]
    fn reindex_receipt_remains_exact_after_later_publication() {
        let dir = tempfile::TempDir::new().expect("root");
        let path = dir.path().join("lib.rs");
        std::fs::write(&path, "pub fn first() {}\n").expect("seed");
        let shared = LiveIndex::load(dir.path()).expect("cold load");
        let expected_gen = shared.current_project_generation();

        std::fs::write(&path, "pub fn second() {}\n").expect("first edit");
        let second =
            admit_and_index_single_path_with_receipt("lib.rs", &path, &shared, expected_gen);
        assert_eq!(second.outcome, ReindexOutcome::Reindexed);
        assert!(second.snapshot_created);
        let second_published = second.published.expect("second publication receipt");
        let second_fence = PublicationFence::from_published(&second_published);

        let unchanged =
            admit_and_index_single_path_with_receipt("lib.rs", &path, &shared, expected_gen);
        assert_eq!(unchanged.outcome, ReindexOutcome::HashSkip);
        assert!(!unchanged.snapshot_created);
        let unchanged_published = unchanged.published.expect("hash-skip publication receipt");
        assert!(
            unchanged_published
                .live
                .get_file("lib.rs")
                .expect("hash-skip file")
                .symbols
                .iter()
                .any(|symbol| symbol.name == "second")
        );

        std::fs::write(&path, "pub fn third() {}\n").expect("second edit");
        let third =
            admit_and_index_single_path_with_receipt("lib.rs", &path, &shared, expected_gen);
        assert_eq!(third.outcome, ReindexOutcome::Reindexed);
        let third_published = third.published.expect("third publication receipt");
        let third_fence = PublicationFence::from_published(&third_published);
        assert_ne!(second_fence, third_fence);

        let second_file = second_published
            .live
            .get_file("lib.rs")
            .expect("second file");
        assert!(
            second_file
                .symbols
                .iter()
                .any(|symbol| symbol.name == "second")
        );
        assert!(
            !second_file
                .symbols
                .iter()
                .any(|symbol| symbol.name == "third")
        );
        assert!(
            !unchanged_published
                .live
                .get_file("lib.rs")
                .expect("immutable hash-skip file")
                .symbols
                .iter()
                .any(|symbol| symbol.name == "third")
        );
        assert!(
            third_published
                .live
                .get_file("lib.rs")
                .expect("third file")
                .symbols
                .iter()
                .any(|symbol| symbol.name == "third")
        );

        assert!(
            shared
                .take_pre_update_snapshot_for_publication_at_generation(
                    "lib.rs",
                    expected_gen,
                    second_fence,
                )
                .is_none(),
            "the old receipt must not drain the later publication's snapshot"
        );
        let third_baseline = shared
            .take_pre_update_snapshot_for_publication_at_generation(
                "lib.rs",
                expected_gen,
                third_fence,
            )
            .expect("third publication owns the current snapshot");
        assert!(
            third_baseline
                .symbols
                .iter()
                .any(|symbol| symbol.name == "second")
        );
    }

    #[test]
    fn bridge_only_publication_does_not_invalidate_live_index_reindex_cas() {
        let dir = tempfile::TempDir::new().expect("root");
        let path = dir.path().join("lib.rs");
        std::fs::write(&path, "pub fn before() {}\n").expect("seed");
        let shared = LiveIndex::load(dir.path()).expect("cold load");
        let before = shared.published_generation();
        let health_generation = before.health.generation;

        let prepared = shared.prepare_bridge_rebuild();
        assert!(shared.publish_prepared_bridge(prepared));
        let bridge_only = shared.published_generation();
        assert!(
            bridge_only.publication_generation > before.publication_generation,
            "bridge publication must advance the full publication bundle"
        );
        assert_eq!(
            bridge_only.health.generation, health_generation,
            "bridge publication must retain the live-index health base"
        );

        std::fs::write(&path, "pub fn after() {}\n").expect("edit");
        let receipt = admit_and_index_single_path_with_receipt(
            "lib.rs",
            &path,
            &shared,
            shared.current_project_generation(),
        );
        assert_eq!(receipt.outcome, ReindexOutcome::Reindexed);
        assert!(
            receipt
                .published
                .expect("winning reindex publication")
                .live
                .get_file("lib.rs")
                .expect("reindexed file")
                .symbols
                .iter()
                .any(|symbol| symbol.name == "after")
        );
    }

    #[test]
    fn stale_not_found_fence_preserves_recreated_file_and_reports_rejection() {
        let dir = tempfile::TempDir::new().expect("root");
        let path = dir.path().join("lib.rs");
        std::fs::write(&path, "pub fn first() {}\n").expect("seed");
        let shared = LiveIndex::load(dir.path()).expect("cold load");

        std::fs::remove_file(&path).expect("simulate missing observation");
        let absence_fence = shared.publication_fence();

        std::fs::write(&path, "pub fn recreated() {}\n").expect("recreate");
        assert_eq!(
            finalize_missing_file(
                &shared,
                "lib.rs",
                &path,
                shared.current_project_generation(),
                absence_fence,
            ),
            ReindexOutcome::PublicationRejected,
            "a rejected stale removal must not claim the file was removed"
        );
        assert!(
            shared
                .read()
                .get_file("lib.rs")
                .expect("last-valid indexed file remains")
                .symbols
                .iter()
                .any(|symbol| symbol.name == "first")
        );
        assert!(
            std::fs::read_to_string(&path)
                .expect("recreated disk file")
                .contains("recreated")
        );
    }

    #[test]
    fn missing_file_finalization_ignores_unrelated_same_project_publication() {
        let dir = tempfile::TempDir::new().expect("root");
        let a_path = dir.path().join("a.rs");
        let b_path = dir.path().join("b.rs");
        std::fs::write(&a_path, "pub fn a_before() {}\n").expect("seed a");
        std::fs::write(&b_path, "pub fn b_before() {}\n").expect("seed b");
        let shared = LiveIndex::load(dir.path()).expect("cold load");
        let expected_gen = shared.current_project_generation();

        std::fs::remove_file(&a_path).expect("observe a missing");
        let absence_fence = shared.publication_fence();

        std::fs::write(&b_path, "pub fn b_after() {}\n").expect("edit b");
        let b_receipt =
            admit_and_index_single_path_with_receipt("b.rs", &b_path, &shared, expected_gen);
        assert_eq!(b_receipt.outcome, ReindexOutcome::Reindexed);
        let after_b = shared.publication_fence();
        assert_eq!(after_b.project_generation, absence_fence.project_generation);
        assert_ne!(
            after_b.publication_generation, absence_fence.publication_generation,
            "the regression requires an intervening unrelated publication"
        );

        assert_eq!(
            finalize_missing_file(&shared, "a.rs", &a_path, expected_gen, absence_fence,),
            ReindexOutcome::Removed,
            "an unrelated publication must not prevent convergence to disk absence"
        );
        let published = shared.published_generation();
        assert!(published.live.get_file("a.rs").is_none());
        assert!(
            published
                .live
                .get_file("b.rs")
                .expect("unrelated publication survives")
                .symbols
                .iter()
                .any(|symbol| symbol.name == "b_after")
        );
    }

    #[test]
    fn stale_project_generation_cannot_remove_from_rebound_project() {
        let project_a = tempfile::TempDir::new().expect("project A");
        let project_b = tempfile::TempDir::new().expect("project B");
        std::fs::write(project_a.path().join("lib.rs"), "pub fn a() {}\n").unwrap();
        std::fs::write(project_b.path().join("lib.rs"), "pub fn b() {}\n").unwrap();
        let shared = LiveIndex::load(project_a.path()).expect("load A");
        let stale_gen = shared.current_project_generation();
        shared.reload(project_b.path()).expect("rebind to B");
        let rebound_fence = shared.publication_fence();

        assert_eq!(
            finalize_missing_file(
                &shared,
                "lib.rs",
                &project_a.path().join("lib.rs"),
                stale_gen,
                rebound_fence,
            ),
            ReindexOutcome::PublicationRejected
        );
        assert!(
            shared
                .read()
                .get_file("lib.rs")
                .expect("B file survives stale absence finalization")
                .symbols
                .iter()
                .any(|symbol| symbol.name == "b")
        );

        let publication_before = shared.publication_fence();
        let excluded = read_and_index_with_receipt(
            ".git/config",
            &project_b.path().join(".git/config"),
            &shared,
            None::<LanguageId>,
            stale_gen,
        );
        assert_eq!(excluded.outcome, ReindexOutcome::PublicationRejected);
        assert_eq!(shared.publication_fence(), publication_before);
        assert!(shared.read().get_file("lib.rs").is_some());
    }
}
