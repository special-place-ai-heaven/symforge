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

use tracing::{debug, trace, warn};

use crate::domain::{
    FileClassification, FileDisposition, LanguageId, MetadataOnlyReason, ScoutDecision,
};
use crate::hash;
use crate::live_index::store::{IndexedFile, SharedIndex};
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
) -> ReindexResult
where
    L: Into<Option<LanguageId>>,
{
    let language = language.into();
    match read_and_index(
        relative_path,
        abs_path,
        shared,
        language.clone(),
        expected_gen,
    ) {
        ReindexResult::NotFound => {}
        other => return other,
    }

    let delays_ms = [50u64, 200, 500];
    for delay_ms in delays_ms {
        std::thread::sleep(std::time::Duration::from_millis(delay_ms));
        match read_and_index(
            relative_path,
            abs_path,
            shared,
            language.clone(),
            expected_gen,
        ) {
            ReindexResult::NotFound => continue,
            other => return other,
        }
    }

    if shared.remove_file_at_generation(relative_path, expected_gen) {
        warn!("watcher: file not found after retries, removed from index: {relative_path}");
    } else {
        trace!(
            "watcher: file not found after retries, stale generation rejected remove: {relative_path}"
        );
    }
    ReindexResult::Removed
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
) -> ReindexResult
where
    L: Into<Option<LanguageId>>,
{
    read_and_index_with_stable_read(
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
) -> ReindexResult {
    read_and_index(relative_path, abs_path, shared, None, expected_gen)
}

pub(crate) fn read_and_index_with_stable_read<L, R>(
    relative_path: &str,
    abs_path: &Path,
    shared: &SharedIndex,
    language: L,
    expected_gen: u64,
    mut stable_read: R,
) -> ReindexResult
where
    L: Into<Option<LanguageId>>,
    R: FnMut(&Path, &crate::domain::FileStamp) -> crate::live_index::store::StableReadOutcome,
{
    let language = language.into();
    // Keep single-file watcher/freshen paths symmetric with the bulk walk.
    // Both universal name exclusions and placement-derived subtree exclusions
    // run before any metadata/content read from VCS or runtime-state internals.
    let relative = Path::new(relative_path);
    if crate::discovery::path_is_hard_scope_excluded(relative)
        || shared.is_source_excluded(relative)
        || shared.read().is_path_gitignored(relative_path)
    {
        let removed = shared.remove_file_at_generation(relative_path, expected_gen);
        if removed {
            debug!("watcher: source-scope eviction {relative_path}");
        } else {
            trace!("watcher: source-scope skip (no prior record) {relative_path}");
        }
        return ReindexResult::Skipped;
    }

    const MAX_PUBLICATION_ATTEMPTS: usize = 4;
    for attempt in 1..=MAX_PUBLICATION_ATTEMPTS {
        let base_publication_gen = shared.published_state().generation;

        // Run the same metadata-first scout as cold load before any whole-file read.
        let mut scouted = match crate::discovery::scout_single_path(relative_path, abs_path) {
            Ok(scouted) => scouted,
            Err(error) => {
                if matches!(
                    std::fs::metadata(abs_path),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound
                ) {
                    return ReindexResult::NotFound;
                }
                warn!("watcher: failed to scout {relative_path}: {error}");
                return ReindexResult::ReadError(error.to_string());
            }
        };
        let mtime_secs = scouted
            .stamp
            .modified_hint
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|duration| duration.as_secs())
            .unwrap_or(0);

        if let Some(disposition) = catalog_terminal_disposition(&scouted.decision) {
            if shared.publish_terminal_disposition_at_generation(
                relative_path,
                scouted,
                disposition,
                expected_gen,
                base_publication_gen,
            ) {
                debug!("watcher: metadata-terminal admission {relative_path}");
                return ReindexResult::Skipped;
            }
            if shared.current_project_generation() == expected_gen
                && shared.published_state().generation != base_publication_gen
            {
                trace!(attempt, "watcher: retrying metadata-terminal publication");
                continue;
            }
            trace!("watcher: stale metadata-terminal admission rejected: {relative_path}");
            return ReindexResult::Skipped;
        }
        if let ScoutDecision::Unavailable { stage, kind } = &scouted.decision {
            if *stage == crate::domain::AccessStage::Metadata
                && *kind == crate::domain::AccessErrorKind::NotFound
            {
                return ReindexResult::NotFound;
            }
            let disposition = FileDisposition::Unreadable {
                stage: *stage,
                kind: *kind,
            };
            if shared.publish_terminal_disposition_at_generation(
                relative_path,
                scouted,
                disposition,
                expected_gen,
                base_publication_gen,
            ) {
                return ReindexResult::ReadError("single-path scout unavailable".to_string());
            }
            if shared.current_project_generation() == expected_gen
                && shared.published_state().generation != base_publication_gen
            {
                trace!(attempt, "watcher: retrying unavailable publication");
                continue;
            }
            return ReindexResult::ReadError("single-path scout unavailable".to_string());
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
            if shared.publish_terminal_disposition_at_generation(
                relative_path,
                scouted,
                disposition,
                expected_gen,
                base_publication_gen,
            ) {
                return ReindexResult::Skipped;
            }
            if shared.current_project_generation() == expected_gen
                && shared.published_state().generation != base_publication_gen
            {
                trace!(attempt, "watcher: retrying generated-output publication");
                continue;
            }
            return ReindexResult::Skipped;
        }

        let targets = match &scouted.decision {
            ScoutDecision::Ingest { targets } => *targets,
            _ => unreachable!("terminal scout decisions return before content ingestion"),
        };

        let language = language
            .clone()
            .or_else(|| scouted.language.clone())
            .unwrap_or(LanguageId::Text);

        let bytes = match stable_read(abs_path, &scouted.stamp) {
            crate::live_index::store::StableReadOutcome::Accepted { bytes, .. } => bytes,
            crate::live_index::store::StableReadOutcome::HardSkip { reason } => {
                let decision = ScoutDecision::HardSkip { reason };
                let disposition = catalog_terminal_disposition(&decision)
                    .expect("hard-skip decision must project to a terminal disposition");
                scouted.decision = decision;
                if shared.publish_terminal_disposition_at_generation(
                    relative_path,
                    scouted,
                    disposition,
                    expected_gen,
                    base_publication_gen,
                ) {
                    return ReindexResult::Skipped;
                }
                if shared.current_project_generation() == expected_gen
                    && shared.published_state().generation != base_publication_gen
                {
                    trace!(
                        attempt,
                        "watcher: retrying stable-read hard-skip publication"
                    );
                    continue;
                }
                return ReindexResult::Skipped;
            }
            crate::live_index::store::StableReadOutcome::Unreadable { stage, kind } => {
                if !shared.publish_terminal_disposition_at_generation(
                    relative_path,
                    scouted,
                    FileDisposition::Unreadable { stage, kind },
                    expected_gen,
                    base_publication_gen,
                ) && shared.current_project_generation() == expected_gen
                    && shared.published_state().generation != base_publication_gen
                {
                    trace!(
                        attempt,
                        "watcher: retrying stable-read unavailable publication"
                    );
                    continue;
                }
                return ReindexResult::ReadError("stable read unavailable".to_string());
            }
            crate::live_index::store::StableReadOutcome::UnstableDuringRead => {
                if !shared.publish_terminal_disposition_at_generation(
                    relative_path,
                    scouted,
                    FileDisposition::UnstableDuringRead,
                    expected_gen,
                    base_publication_gen,
                ) && shared.current_project_generation() == expected_gen
                    && shared.published_state().generation != base_publication_gen
                {
                    trace!(attempt, "watcher: retrying unstable-read publication");
                    continue;
                }
                return ReindexResult::ReadError("file changed during stable read".to_string());
            }
        };

        // The watcher shares the exact stable-content admission boundary used
        // by cold load. Terminal outcomes are published before hashing or parsing,
        // and the owned byte buffer is discarded on every non-admitted path.
        if let crate::knowledge::StableContentAdmission::MetadataOnly(reason) =
            crate::knowledge::classify_stable_content(relative_path, targets, &bytes)
        {
            if shared.publish_terminal_disposition_at_generation(
                relative_path,
                scouted,
                FileDisposition::MetadataOnly { reason },
                expected_gen,
                base_publication_gen,
            ) {
                return ReindexResult::Skipped;
            }
            if shared.current_project_generation() == expected_gen
                && shared.published_state().generation != base_publication_gen
            {
                trace!(attempt, "watcher: retrying content-policy publication");
                continue;
            }
            return ReindexResult::Skipped;
        }

        // Compute the hash and compare without holding the publication writer lock.
        let new_hash = hash::digest_hex(&bytes);
        {
            let index = shared.read();
            if let Some(existing) = index.get_file(relative_path)
                && existing.content_hash == new_hash
            {
                drop(index);
                if shared.publish_hash_skip_at_generation(
                    relative_path,
                    mtime_secs,
                    scouted,
                    targets,
                    expected_gen,
                    base_publication_gen,
                ) {
                    debug!("watcher: hash-skip {relative_path}");
                    return ReindexResult::HashSkip;
                }
                if shared.current_project_generation() == expected_gen
                    && shared.published_state().generation != base_publication_gen
                {
                    trace!(attempt, "watcher: retrying hash-skip publication");
                    continue;
                }
                return ReindexResult::Skipped;
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
        if shared.publish_indexed_file_at_generation(
            relative_path,
            indexed,
            scouted,
            targets,
            expected_gen,
            base_publication_gen,
        ) {
            debug!("watcher: re-indexed {relative_path}");
            return ReindexResult::Reindexed;
        }
        if shared.current_project_generation() == expected_gen
            && shared.published_state().generation != base_publication_gen
        {
            trace!(attempt, "watcher: retrying indexed-file publication");
            continue;
        }
        trace!("watcher: stale indexed-file publication rejected: {relative_path}");
        return ReindexResult::Skipped;
    }

    warn!(
        "watcher: aborting {relative_path} after {MAX_PUBLICATION_ATTEMPTS} concurrent publications"
    );
    ReindexResult::Skipped
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
    admit_and_index_single_path(&relative, &abs_path, shared, expected_gen)
}

/// Remove one file from the live index (embed facade; task #24 / AAP ask 3).
///
/// Generation-fenced like the watcher's removals: returns `true` when the
/// removal was APPLIED under the current project generation (including the
/// no-op case where the path was not tracked), `false` only when a concurrent
/// retarget invalidated the generation fence.
pub fn remove_file(shared: &SharedIndex, relative_path: &str) -> bool {
    let relative = relative_path.replace('\\', "/");
    shared.remove_file_at_generation(&relative, shared.current_project_generation())
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
}
