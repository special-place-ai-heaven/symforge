//! The single admission/disclosure gate for raw-disk content reads.
//!
//! Every protocol lane that reopens a repository file from disk — rather than
//! serving bytes already held in the in-memory index — routes its read through
//! [`admit_disk_read`]. The gate owns the read: it classifies the exact buffer
//! it just read and hands that buffer back only on a permit verdict, so no
//! caller can classify one set of bytes and then render another.

use std::path::Path;

use crate::domain::{FileDisposition, IndexTargets, LanguageId, MetadataOnlyReason};
use crate::live_index::LiveIndex;
use crate::protocol::format;

/// Working-tree text for `relative_path`, admitted by [`admit_disk_read`].
///
/// The gated replacement for `GitRepo::file_from_workdir`, which reads the
/// working tree with only a containment check. Three protocol lanes shared that
/// ungated read and each disclosed a security-demoted file: the `search_text`
/// untracked sweep (an anchored regex over the content recovers it character by
/// character), `diff_symbols` in uncommitted mode, and `detect_impact` seeding
/// from `WORKTREE` (both disclose symbol names and signatures).
///
/// The return shape deliberately MIRRORS `file_from_workdir` so refusal stays
/// distinguishable from absence at every call site:
///   * `Ok(None)` — not a regular file, or not valid UTF-8 (today's behaviour);
///   * `Ok(Some(text))` — admitted content, the only bytes a lane may use;
///   * `Err(message)` — REFUSED, carrying the caller-ready refusal. A lane that
///     collapses this to "absent" is fail-closed and safe; a lane that renders
///     a verdict about the file must say it was withheld rather than imply the
///     file is empty.
pub(crate) fn admit_worktree_text(
    live: &LiveIndex,
    repo: &crate::git::GitRepo,
    relative_path: &str,
) -> Result<Option<String>, String> {
    let Some(workdir) = repo.workdir() else {
        return Err("bare repository has no working directory".to_string());
    };
    let full_path = workdir.join(relative_path);
    if !full_path.is_file() {
        return Ok(None);
    }
    // The gate owns the read: it classifies the exact buffer it just read and
    // returns it only on a permit, so no lane can classify one set of bytes and
    // then render another.
    let bytes = admit_disk_read(live, relative_path, &full_path)?;
    Ok(String::from_utf8(bytes).ok())
}

/// Predict — WITHOUT reading any bytes — whether [`admit_disk_read`] would
/// refuse `relative_path`, from the same signals the gate checks before its
/// read: the current path rule, the recorded disposition on the live
/// publication, and (when known) the size against the scan limit.
///
/// Used to key advice text ("Use get_file_content for raw reads") on the
/// gate's actual verdict, so no message ever points the caller at a read the
/// gate is certain to refuse. Conservative by construction: it cannot see
/// content that changed after publication, so a `false` here is advice, not
/// authorization — the gate itself still decides on the exact bytes.
pub(crate) fn disk_read_would_refuse(
    live: &LiveIndex,
    relative_path: &str,
    size: Option<u64>,
) -> bool {
    if crate::knowledge::sensitive_path_rule(relative_path).is_some() {
        return true;
    }
    if let Some(FileDisposition::MetadataOnly {
        reason:
            MetadataOnlyReason::SensitivePath { .. } | MetadataOnlyReason::SensitiveContent { .. },
    }) = live.capture_file_disposition(relative_path)
    {
        return true;
    }
    size.is_some_and(|size| crate::knowledge::exceeds_scan_limit(size as usize))
}

/// Read `canon_path` and return its bytes only if the file is admissible for
/// content disclosure.
///
/// `live` must be the SAME publication snapshot that produced the caller's
/// "not in the index" verdict; a second `self.index.read()` is a different
/// snapshot and would let the manifest and the index-miss disagree.
///
/// `relative_path` is the repo-relative path the caller was asked for, already
/// normalized by `normalize_exact_path`. It is used for the path rule, the
/// manifest lookup, and target derivation — never re-joined to read from.
///
/// Returns `Err` with the caller-ready refusal or IO message; the caller
/// returns it verbatim.
pub(crate) fn admit_disk_read(
    live: &LiveIndex,
    relative_path: &str,
    canon_path: &Path,
) -> Result<Vec<u8>, String> {
    // Current path rule — no read needed.
    if crate::knowledge::sensitive_path_rule(relative_path).is_some() {
        return Err(format::content_withheld_by_admission(relative_path));
    }

    // Recorded disposition on the publication that produced the miss — no read
    // needed. A missing entry is not authorization: it means the manifest has
    // nothing to say, and the current-bytes classification below still applies.
    if let Some(FileDisposition::MetadataOnly { reason }) =
        live.capture_file_disposition(relative_path)
    {
        match reason {
            // A recorded content demotion carrying the reserved indeterminate id
            // is a detector FAILURE, not a match: reindexing cannot change it.
            MetadataOnlyReason::SensitiveContent { rule_ids, .. }
                if rule_ids
                    .iter()
                    .any(|id| id == crate::knowledge::INDETERMINATE_RULE_ID) =>
            {
                return Err(format::content_withheld_unscanned(relative_path));
            }
            MetadataOnlyReason::SensitivePath { .. }
            | MetadataOnlyReason::SensitiveContent { .. } => {
                return Err(format::content_withheld_by_admission(relative_path));
            }
            _ => {}
        }
    }

    // The one read, and the classification of exactly those bytes. Required
    // even when the manifest is clean or says Indexed: a clean manifest cannot
    // authorize bytes that changed after it was published.
    let bytes = match std::fs::read(canon_path) {
        Ok(bytes) => bytes,
        Err(e) => return Err(format!("{relative_path} [error: could not read file: {e}]")),
    };
    // Fail closed on bytes the detector cannot have inspected, and do it HERE so
    // the refusal MESSAGE can be honest. `classify_stable_content` demotes both
    // populations correctly on its own — it collapses the scan-budget refusal
    // into `SensitiveContent`, and since Ruling 4 it encoding-validates the whole
    // buffer on every path — but neither cause is legible in that verdict, so its
    // refusal would advise a reindex that cannot help. Placed before
    // `classify_stable_content` so a binary buffer is not pointlessly scanned;
    // `detect_lfs_pointer` requires valid UTF-8 under 1 KiB, so no pointer is
    // swallowed here.
    if crate::knowledge::exceeds_scan_limit(bytes.len())
        || crate::knowledge::decode_searchable_text(&bytes).is_err()
    {
        return Err(format::content_withheld_unscanned(relative_path));
    }
    let language = Path::new(relative_path)
        .extension()
        .and_then(|extension| extension.to_str())
        .and_then(LanguageId::from_extension);
    let targets = IndexTargets::for_path(relative_path, language.as_ref());
    // Only the two security variants deny. Every other `MetadataOnlyReason`
    // (binary, encoding, LFS, path collision, oversized, …) keeps today's
    // behavior — this gate takes no position on them. The resource-limit and
    // encoding cases are already decided above, so the remaining `Indeterminate`
    // failures here are the global ones (policy compilation, internal), which
    // `classify_stable_content` maps to `SensitiveContent` carrying the reserved
    // indeterminate id — a detector failure, so the honest message, not the one
    // advising a reindex.
    if let crate::knowledge::StableContentAdmission::MetadataOnly(
        MetadataOnlyReason::SensitiveContent { rule_ids, .. },
    ) = crate::knowledge::classify_stable_content(relative_path, targets, &bytes)
    {
        return Err(
            if rule_ids
                .iter()
                .any(|id| id == crate::knowledge::INDETERMINATE_RULE_ID)
            {
                format::content_withheld_unscanned(relative_path)
            } else {
                format::content_withheld_by_admission(relative_path)
            },
        );
    }

    // Permit. These are the only bytes any gated lane may render or parse.
    Ok(bytes)
}
