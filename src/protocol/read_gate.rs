//! The single admission/disclosure gate for repository CONTENT reads.
//!
//! Every protocol lane that fetches repository bytes from outside the in-memory
//! index routes through this module. There are two such object stores, not one:
//!
//!   * the working tree, via [`admit_worktree_text`] / [`admit_disk_read`];
//!   * git objects, via [`admit_git_text`].
//!
//! The gate OWNS the fetch in both cases. It classifies the exact buffer it
//! just obtained and hands that buffer back only on a permit verdict, so no
//! caller can classify one set of bytes and then render another. A lane that
//! fetched its own bytes and promised to classify them afterwards would be
//! indistinguishable from an ungated read, which is why the fetch lives here.
//!
//! This doc previously described the gate as disk-only, and that omission was
//! load-bearing: `diff_symbols` gated its working-tree read while the two
//! `file_at_ref` reads beside it stayed ungated, disclosing a demoted file's
//! symbol names and signatures out of git objects. Policy and classification
//! are shared by both lanes precisely so the next store added cannot repeat it.

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
/// Policy refusals that need NO bytes: the current path rule and the recorded
/// disposition on the publication that produced the miss.
///
/// Split out so both the disk lane and the git-object lane consult exactly the
/// same policy, and so the disk lane can still refuse WITHOUT reading the file.
fn refuse_by_policy(live: &LiveIndex, relative_path: &str) -> Option<String> {
    // Current path rule — no read needed.
    if crate::knowledge::sensitive_path_rule(relative_path).is_some() {
        return Some(format::content_withheld_by_admission(relative_path));
    }

    // Recorded disposition on the publication that produced the miss — no read
    // needed. A missing entry is not authorization: it means the manifest has
    // nothing to say, and the current-bytes classification still applies.
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
                return Some(format::content_withheld_unscanned(relative_path));
            }
            MetadataOnlyReason::SensitivePath { .. }
            | MetadataOnlyReason::SensitiveContent { .. } => {
                return Some(format::content_withheld_by_admission(relative_path));
            }
            _ => {}
        }
    }
    None
}

/// Admit bytes the caller ALREADY HOLDS — a git blob, not a disk read.
///
/// The disk lane and the git-object lane differ in exactly one step: where the
/// bytes come from. Policy (path rule, recorded disposition) and content
/// classification are identical, so they live here and both lanes share them.
///
/// This exists because `diff_symbols` gated its working-tree read and left the
/// two `file_at_ref` reads beside it ungated, which disclosed a demoted file's
/// symbol names and signatures out of git objects. A lane that holds repository
/// bytes must admit them, whatever object store they came from.
pub(crate) fn admit_bytes(
    live: &LiveIndex,
    relative_path: &str,
    bytes: Vec<u8>,
) -> Result<Vec<u8>, String> {
    if let Some(refusal) = refuse_by_policy(live, relative_path) {
        return Err(refusal);
    }
    if let Some(refusal) = classify_admitted_bytes(relative_path, &bytes) {
        return Err(refusal);
    }
    Ok(bytes)
}

/// Text for `relative_path` as of `git_ref`, admitted by [`admit_bytes`].
///
/// The gated replacement for a bare `GitRepo::file_at_ref` in a disclosure
/// lane. Return shape MIRRORS `file_at_ref` so refusal stays distinguishable
/// from absence at every call site:
///   * `Ok(None)` — absent at that ref, binary, or not valid UTF-8;
///   * `Ok(Some(text))` — admitted content, the only bytes a lane may use;
///   * `Err(message)` — REFUSED, carrying the caller-ready refusal.
pub(crate) fn admit_git_text(
    live: &LiveIndex,
    repo: &crate::git::GitRepo,
    git_ref: &str,
    relative_path: &str,
) -> Result<Option<String>, String> {
    // Policy first: a path-ruled file is refused without touching the object
    // store at all.
    if let Some(refusal) = refuse_by_policy(live, relative_path) {
        return Err(refusal);
    }
    let Some(text) = repo.file_at_ref(git_ref, relative_path)? else {
        return Ok(None);
    };
    let admitted = admit_bytes(live, relative_path, text.into_bytes())?;
    Ok(String::from_utf8(admitted).ok())
}

pub(crate) fn admit_disk_read(
    live: &LiveIndex,
    relative_path: &str,
    canon_path: &Path,
) -> Result<Vec<u8>, String> {
    // Policy refusals need no bytes, so they run BEFORE the read: a demoted
    // file is never opened.
    if let Some(refusal) = refuse_by_policy(live, relative_path) {
        return Err(refusal);
    }

    // The one read, and the classification of exactly those bytes. Required
    // even when the manifest is clean or says Indexed: a clean manifest cannot
    // authorize bytes that changed after it was published.
    let bytes = match std::fs::read(canon_path) {
        Ok(bytes) => bytes,
        Err(e) => return Err(format!("{relative_path} [error: could not read file: {e}]")),
    };
    if let Some(refusal) = classify_admitted_bytes(relative_path, &bytes) {
        return Err(refusal);
    }
    Ok(bytes)
}

/// Classify bytes the gate is holding. `None` admits them.
fn classify_admitted_bytes(relative_path: &str, bytes: &[u8]) -> Option<String> {
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
        || crate::knowledge::decode_searchable_text(bytes).is_err()
    {
        return Some(format::content_withheld_unscanned(relative_path));
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
    ) = crate::knowledge::classify_stable_content(relative_path, targets, bytes)
    {
        return Some(
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
    None
}
