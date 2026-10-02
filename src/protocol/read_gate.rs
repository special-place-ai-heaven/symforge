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
//!
//! **T044 — the authority CHOICE is explicit.** Serving bytes the generation
//! PUBLISHED and observing what is on disk RIGHT NOW are different claims with
//! different scopes, and before this split the choice between them was implicit
//! in which function a lane reached for: a lane meaning to serve generation
//! content could take an index miss and silently backfill from disk. The two
//! lanes are now named — [`resolve_generation_bytes`], which NEVER touches
//! disk, and [`observe_disk_beneath`], which always does, confined beneath the
//! workspace root — so a caller states which authority its answer rides on.

use std::path::{Component, Path};

use crate::domain::{FileDisposition, IndexTargets, LanguageId, MetadataOnlyReason};
use crate::live_index::LiveIndex;
use crate::protocol::format;

/// What the generation has to say about one path.
#[derive(Debug, PartialEq, Eq)]
pub enum GenerationResolution<'a> {
    /// The bytes the generation PUBLISHED for this path — index-resident,
    /// served with zero disk I/O, exactly as `IndexedFile.content` promises.
    Published(&'a [u8]),
    /// The generation holds no content for this path. A miss, and ONLY a
    /// miss: it is never permission to observe the disk. The caller that
    /// wants current disk bytes says so by calling [`observe_disk_beneath`].
    NotInGeneration,
}

/// Resolve the bytes the generation published for `relative_path`.
///
/// This function cannot read the disk — it has no path to it. A generation
/// miss surfaces as [`GenerationResolution::NotInGeneration`] so the
/// authority choice lands on the caller, in the open.
pub fn resolve_generation_bytes<'a>(
    live: &'a LiveIndex,
    relative_path: &str,
) -> GenerationResolution<'a> {
    match live.get_file(relative_path) {
        Some(file) => GenerationResolution::Published(&file.content),
        None => GenerationResolution::NotInGeneration,
    }
}

/// Deliberately observe `relative_path` on disk RIGHT NOW, confined beneath
/// `workspace_root`, admitted by the same policy and classification as every
/// other content read.
///
/// Confinement refuses BEFORE any read. Lexically, an absolute path, a drive or
/// root prefix, or any `..` component is an escape however it is spelled. On
/// disk, [`crate::protocol::edit::refuse_path_alias`] refuses a symlink that
/// resolves outside the root and any spelling whose resolved name differs.
/// The refusal never carries the escaped content.
// ponytail: resolve-then-read, not open-by-handle — a link swapped in between
// the check and the read is a TOCTOU window; the upgrade path is opening the
// file once and checking the handle's final path.
pub fn observe_disk_beneath(
    live: &LiveIndex,
    workspace_root: &Path,
    relative_path: &str,
) -> Result<Vec<u8>, String> {
    observe_beneath(live, workspace_root, relative_path, true)
}

/// [`observe_disk_beneath`] for a sweep that drops the refusal: a recorded
/// content demotion is refused without the gate's re-read to name its lines.
pub(crate) fn observe_disk_beneath_without_lines(
    live: &LiveIndex,
    workspace_root: &Path,
    relative_path: &str,
) -> Result<Vec<u8>, String> {
    observe_beneath(live, workspace_root, relative_path, false)
}

fn observe_beneath(
    live: &LiveIndex,
    workspace_root: &Path,
    relative_path: &str,
    name_lines: bool,
) -> Result<Vec<u8>, String> {
    let candidate = Path::new(relative_path);
    let escapes = candidate.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    });
    if escapes || candidate.is_absolute() {
        return Err(format!(
            "{relative_path} [error: path escapes the workspace root; a disk \
             observation is confined beneath it]"
        ));
    }
    // `refuse_by_policy` inside the gate matches the caller's spelling only and
    // stays syscall-free; the spelling is judged on the resolved path here.
    refuse_disk_spelling(workspace_root, relative_path)?;
    let full_path = workspace_root.join(candidate);
    disk_read(live, relative_path, &full_path, name_lines)
}

/// The on-disk half of the gate's confinement, shared by both disk-reading
/// entries. It refuses any spelling the shared resolver refuses: an escape
/// through a symlink, or another spelling of a file. It also refuses VCS and
/// runtime-state internals (`.git`, `.symforge`), which the cold walk never
/// reads either.
fn refuse_disk_spelling(root: &Path, relative_path: &str) -> Result<(), String> {
    crate::protocol::edit::refuse_path_alias(root, relative_path)?;
    if let Some(refusal) = hard_scope_refusal(relative_path) {
        return Err(refusal);
    }
    if let Some(rule_id) =
        crate::knowledge::sensitive_path_rule_at(relative_path, &root.join(relative_path))
    {
        return Err(format::content_withheld_by_path_rule(
            relative_path,
            rule_id,
        ));
    }
    Ok(())
}

/// The refusal for a path under VCS or runtime-state internals (`.git`,
/// `.symforge`). Lexical and case-insensitive, so it needs no filesystem call
/// and answers the same whether or not the path exists.
pub(crate) fn hard_scope_refusal(relative_path: &str) -> Option<String> {
    crate::discovery::path_is_hard_scope_excluded(Path::new(relative_path)).then(|| {
        format!(
            "{relative_path} [error: VCS and runtime-state internals are outside \
             source scope; a disk observation never reads them]"
        )
    })
}

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
    worktree_text(live, repo, relative_path, true)
}

/// [`admit_worktree_text`] for a lane that drops the refusal (the `search_text`
/// untracked sweep, `detect_impact` seeding): a recorded content demotion is
/// refused without the gate's re-read to name its lines.
pub(crate) fn admit_worktree_text_without_lines(
    live: &LiveIndex,
    repo: &crate::git::GitRepo,
    relative_path: &str,
) -> Result<Option<String>, String> {
    worktree_text(live, repo, relative_path, false)
}

fn worktree_text(
    live: &LiveIndex,
    repo: &crate::git::GitRepo,
    relative_path: &str,
    name_lines: bool,
) -> Result<Option<String>, String> {
    let Some(workdir) = repo.workdir() else {
        return Err("bare repository has no working directory".to_string());
    };
    let full_path = workdir.join(relative_path);
    if !full_path.is_file() {
        return Ok(None);
    }
    // `is_file` follows links, so a tracked symlink to a file outside the work
    // tree reaches here; the resolved spelling decides before the read.
    refuse_disk_spelling(workdir, relative_path)?;
    // The gate owns the read: it classifies the exact buffer it just read and
    // returns it only on a permit, so no lane can classify one set of bytes and
    // then render another.
    let bytes = disk_read(live, relative_path, &full_path, name_lines)?;
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

/// Policy refusals that need NO bytes: the current path rule and the recorded
/// disposition on the publication that produced the miss.
///
/// Makes no syscall: the answer is a pure function of the path and the
/// manifest, identical for a demoted file that exists and one that does not,
/// so the degradation view, the binary sniff and the sweeps that ask it hold no
/// existence bit. A recorded content demotion therefore names no lines here;
/// [`admit_disk_read`] and the read lanes built on it add them, because their
/// callers render the refusal. The `_without_lines` twins the sweeps use never
/// re-read, since the sweeps drop it.
pub(crate) fn refuse_by_policy(live: &LiveIndex, relative_path: &str) -> Option<String> {
    // Current path rule — no read needed.
    if let Some(rule_id) = crate::knowledge::sensitive_path_rule(relative_path) {
        crate::protocol::withheld::record_pending_withheld(
            crate::protocol::withheld::WithheldMeta::path_rule_only(relative_path, rule_id),
        );
        return Some(format::content_withheld_by_path_rule(
            relative_path,
            rule_id,
        ));
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
                crate::protocol::withheld::record_pending_withheld(
                    crate::protocol::withheld::WithheldMeta::unscanned(relative_path),
                );
                return Some(format::content_withheld_unscanned(relative_path));
            }
            MetadataOnlyReason::SensitivePath { rule_id } => {
                crate::protocol::withheld::record_pending_withheld(
                    crate::protocol::withheld::WithheldMeta::path_rule_only(relative_path, rule_id),
                );
                return Some(format::content_withheld_by_path_rule(
                    relative_path,
                    rule_id,
                ));
            }
            MetadataOnlyReason::SensitiveContent {
                rule_ids,
                finding_count,
            } => {
                return Some(format::content_withheld_by_admission(
                    relative_path,
                    rule_ids,
                    *finding_count,
                    &[],
                ));
            }
            _ => {}
        }
    }
    // Last, so a sensitive path keeps its policy refusal: a restored row the
    // snapshot verify could not reconcile. Its index row is withheld, and
    // disk bytes are not a substitute the verify vouched for.
    unverified_notice(live, relative_path)
}

/// A caller's path as a catalog key: separators forward, no leading `./`,
/// no leading or trailing `/`. Pure string work, no filesystem access.
pub(crate) fn normalize_requested_path(raw: &str) -> String {
    let mut normalized = raw.trim().replace('\\', "/");
    while normalized.starts_with("./") {
        normalized = normalized[2..].to_string();
    }
    normalized.trim_matches('/').to_string()
}

/// The unverified-since-restore refusal for a path the caller named, when the
/// snapshot verify withheld it. In-memory only, like every policy refusal
/// here. A sensitive path is left to the path rule, which takes precedence.
pub(crate) fn unverified_notice(live: &LiveIndex, requested: &str) -> Option<String> {
    let path = normalize_requested_path(requested);
    if crate::knowledge::sensitive_path_rule(&path).is_some() {
        return None;
    }
    live.unverified_since_restore(&path)
        .map(|reason| format::unverified_since_restore(&path, reason))
}

/// The read lane's refusal for a RECORDED content demotion, naming its finding
/// lines from the gate's bounded re-read. Asked only by [`admit_disk_read`],
/// and only once [`refuse_by_policy`] has refused, so the policy answer itself
/// stays syscall-free.
///
/// `None` when that refusal is not a recorded content match (the path rule and
/// a detector failure take precedence exactly as in [`refuse_by_policy`]) or no
/// lines were recovered; the caller then renders the policy refusal unchanged.
fn recorded_refusal_naming_lines(live: &LiveIndex, relative_path: &str) -> Option<String> {
    if crate::knowledge::sensitive_path_rule(relative_path).is_some() {
        return None;
    }
    let Some(FileDisposition::MetadataOnly {
        reason:
            MetadataOnlyReason::SensitiveContent {
                rule_ids,
                finding_count,
            },
    }) = live.capture_file_disposition(relative_path)
    else {
        return None;
    };
    if rule_ids
        .iter()
        .any(|id| id == crate::knowledge::INDETERMINATE_RULE_ID)
    {
        return None;
    }
    let (line_ranges, findings) = recorded_finding_evidence(live, relative_path, rule_ids);
    if !findings.is_empty() {
        crate::protocol::withheld::record_pending_withheld(
            crate::protocol::withheld::WithheldMeta::from_content_findings(
                relative_path,
                &findings,
            ),
        );
    }
    (!line_ranges.is_empty()).then(|| {
        format::content_withheld_by_admission(relative_path, rule_ids, *finding_count, &line_ranges)
    })
}

/// Bytes of a regular file, or an error that did not block.
///
/// FIFO, socket, and device opens block until a peer appears. `symlink_metadata`
/// refuses those before `open`. On Unix the open itself is `O_NONBLOCK`, so a
/// replacement between the check and the open cannot hang the caller either.
/// Symlinks are not followed: `symlink_metadata` sees the link, not its target.
pub(crate) fn read_regular_file(path: &Path) -> std::io::Result<Vec<u8>> {
    read_regular_file_limited(path, None)
}

fn read_regular_file_limited(path: &Path, limit: Option<u64>) -> std::io::Result<Vec<u8>> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "not a regular file",
        ));
    }
    let mut file = open_for_gate_read(path)?;
    if !file.metadata().is_ok_and(|opened| opened.is_file()) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "not a regular file",
        ));
    }
    let mut bytes = Vec::new();
    if let Some(limit) = limit {
        std::io::Read::read_to_end(&mut std::io::Read::take(&mut file, limit), &mut bytes)?;
    } else {
        std::io::Read::read_to_end(&mut file, &mut bytes)?;
    }
    Ok(bytes)
}

fn open_for_gate_read(path: &Path) -> std::io::Result<std::fs::File> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(path)
    }
    #[cfg(not(unix))]
    {
        std::fs::File::open(path)
    }
}

/// Finding lines for a RECORDED content demotion, computed at refusal time.
///
/// The manifest records the verdict (rule ids, count) but not where the
/// findings sit — persisting lines would change the snapshot schema. So the
/// gate re-reads the file ITSELF, scans it, and keeps only the line numbers;
/// the bytes are dropped here and never reach a caller. Lines are returned only
/// when the fresh scan names exactly the recorded rules; if the file moved on,
/// the refusal names the recorded verdict without lines rather than pairing it
/// with positions from different content.
///
/// The read is bounded like every other gate read: a spelling the shared
/// repository-path resolver admits as the file's own name beneath the indexed
/// root, a regular file opened once, and no more than the scan budget read
/// from that one handle. Any doubt yields no lines, and the refusal itself
/// never depends on this read.
fn recorded_finding_evidence(
    live: &LiveIndex,
    relative_path: &str,
    recorded: &[String],
) -> (
    Vec<(u32, u32)>,
    Vec<crate::knowledge::SecretFindingDescriptor>,
) {
    let Some(root) = live.indexed_root.as_deref() else {
        return (Vec::new(), Vec::new());
    };
    // The shared resolver admits only a spelling whose canonical path is,
    // component for component, the spelling itself beneath the root: no link
    // on any component, no alias, no escape.
    let Ok(Some(full_path)) = crate::discovery::resolve_repo_path(root, relative_path) else {
        return (Vec::new(), Vec::new());
    };
    let budget = crate::knowledge::SECRET_SCAN_MAX_BYTES as u64 + 1;
    let Ok(bytes) = read_regular_file_limited(&full_path, Some(budget)) else {
        return (Vec::new(), Vec::new());
    };
    if crate::knowledge::exceeds_scan_limit(bytes.len()) {
        return (Vec::new(), Vec::new());
    }
    match crate::knowledge::scan_secret_bytes(relative_path, &bytes) {
        crate::knowledge::SecretScan::Sensitive {
            rule_ids,
            line_ranges,
            findings,
            ..
        } if rule_ids.len() == recorded.len()
            && rule_ids
                .iter()
                .all(|rule| recorded.iter().any(|seen| seen == rule)) =>
        {
            (line_ranges, findings)
        }
        _ => (Vec::new(), Vec::new()),
    }
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
        // 034 US4: dismiss can clear a recorded SensitiveContent demotion on
        // the read/admit lane (bytes already held — no extra disk open for
        // the source file; dismissal store load is bounded).
        if let Some(root) = live.indexed_root.as_deref() {
            let scan = crate::knowledge::scan_secret_bytes(relative_path, &bytes);
            let filtered = crate::protocol::secret_dismissals::filter_scan_with_dismissals(
                root,
                relative_path,
                &bytes,
                scan,
            );
            if matches!(filtered, crate::knowledge::SecretScan::Clean)
                && crate::knowledge::sensitive_path_rule(relative_path).is_none()
            {
                return Ok(bytes);
            }
        }
        return Err(refusal);
    }
    if let Some(refusal) = classify_admitted_bytes(live, relative_path, &bytes) {
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
    disk_read(live, relative_path, canon_path, true)
}

fn disk_read(
    live: &LiveIndex,
    relative_path: &str,
    canon_path: &Path,
    name_lines: bool,
) -> Result<Vec<u8>, String> {
    // Policy refusals need no bytes, so they run BEFORE the read: a demoted
    // file is never opened for disclosure. Only once refusing, and only for a
    // caller that renders the refusal, does the gate re-read a recorded content
    // demotion, bounded, to name its finding lines; those bytes never leave the
    // gate.
    //
    // Feature 034 US4: a recorded SensitiveContent demotion may be overridden
    // on the READ LANE (not inside refuse_by_policy) when every finding's line
    // content digest is dismissed. That keeps refuse_by_policy syscall-free.
    if let Some(refusal) = refuse_by_policy(live, relative_path) {
        if let Some(bytes) = try_admit_dismissed_sensitive_content(live, relative_path, canon_path)
        {
            return Ok(bytes);
        }
        return Err(if name_lines {
            recorded_refusal_naming_lines(live, relative_path).unwrap_or(refusal)
        } else {
            refusal
        });
    }

    // The one read, and the classification of exactly those bytes. Required
    // even when the manifest is clean or says Indexed: a clean manifest cannot
    // authorize bytes that changed after it was published. Same regular-file
    // rule as the dismissal re-read: a FIFO, socket, or device must not block
    // the gate.
    let bytes = match read_regular_file(canon_path) {
        Ok(bytes) => bytes,
        Err(e) => return Err(format!("{relative_path} [error: could not read file: {e}]")),
    };
    if let Some(refusal) = classify_admitted_bytes(live, relative_path, &bytes) {
        return Err(refusal);
    }
    Ok(bytes)
}

/// Read-lane override for content-digest dismissals (034 US4).
///
/// Returns `Some(bytes)` only when the recorded disposition is SensitiveContent
/// (not path-rule / indeterminate) AND a fresh scan filtered by the dismissal
/// store is Clean. The source is read only as a regular file: a FIFO, socket,
/// or device returns `None` without blocking, and the caller keeps the policy
/// refusal. Never follows a symlink store (loader enforces).
fn try_admit_dismissed_sensitive_content(
    live: &LiveIndex,
    relative_path: &str,
    canon_path: &Path,
) -> Option<Vec<u8>> {
    if crate::knowledge::sensitive_path_rule(relative_path).is_some() {
        return None;
    }
    let Some(FileDisposition::MetadataOnly {
        reason: MetadataOnlyReason::SensitiveContent { rule_ids, .. },
    }) = live.capture_file_disposition(relative_path)
    else {
        return None;
    };
    if rule_ids
        .iter()
        .any(|id| id == crate::knowledge::INDETERMINATE_RULE_ID)
    {
        return None;
    }
    let root = live.indexed_root.as_deref()?;
    let bytes = read_regular_file(canon_path).ok()?;
    if crate::knowledge::exceeds_scan_limit(bytes.len()) {
        return None;
    }
    let scan = crate::knowledge::scan_secret_bytes(relative_path, &bytes);
    let filtered = crate::protocol::secret_dismissals::filter_scan_with_dismissals(
        root,
        relative_path,
        &bytes,
        scan,
    );
    match filtered {
        crate::knowledge::SecretScan::Clean => Some(bytes),
        _ => None,
    }
}

/// Classify bytes the gate is holding. `None` admits them.
fn classify_admitted_bytes(live: &LiveIndex, relative_path: &str, bytes: &[u8]) -> Option<String> {
    // Fail closed on bytes the detector cannot have inspected, and do it HERE so
    // the refusal MESSAGE can be honest. `classify_stable_content` demotes both
    // populations correctly on its own — it collapses the scan-budget refusal
    // into `SensitiveContent`, and since Ruling 4 it encoding-validates the whole
    // buffer on every path — but neither cause is legible in that verdict, so its
    // refusal would name a detector match that never happened. Placed before
    // `classify_stable_content` so a binary buffer is not pointlessly scanned;
    // `detect_lfs_pointer` requires valid UTF-8 under 1 KiB, so no pointer is
    // swallowed here.
    if crate::knowledge::exceeds_scan_limit(bytes.len())
        || crate::knowledge::decode_searchable_text(bytes).is_err()
    {
        crate::protocol::withheld::record_pending_withheld(
            crate::protocol::withheld::WithheldMeta::unscanned(relative_path),
        );
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
    // naming a match.
    // The scan's finding lines are kept beside the verdict for the refusal
    // text; they are never part of the recorded disposition.
    let mut finding_lines = Vec::new();
    let mut finding_descriptors: Vec<crate::knowledge::SecretFindingDescriptor> = Vec::new();
    if let crate::knowledge::StableContentAdmission::MetadataOnly(
        MetadataOnlyReason::SensitiveContent {
            rule_ids,
            finding_count,
        },
    ) = crate::knowledge::classify_stable_content_with(
        relative_path,
        targets,
        bytes,
        |path, bytes| {
            let scan = crate::knowledge::scan_secret_bytes(path, bytes);
            let scan = if let Some(root) = live.indexed_root.as_deref() {
                crate::protocol::secret_dismissals::filter_scan_with_dismissals(
                    root, path, bytes, scan,
                )
            } else {
                scan
            };
            if let crate::knowledge::SecretScan::Sensitive {
                line_ranges,
                findings,
                ..
            } = &scan
            {
                finding_lines.clone_from(line_ranges);
                finding_descriptors.clone_from(findings);
            }
            scan
        },
    ) {
        return Some(
            if rule_ids
                .iter()
                .any(|id| id == crate::knowledge::INDETERMINATE_RULE_ID)
            {
                crate::protocol::withheld::record_pending_withheld(
                    crate::protocol::withheld::WithheldMeta::unscanned(relative_path),
                );
                format::content_withheld_unscanned(relative_path)
            } else {
                if !finding_descriptors.is_empty() {
                    crate::protocol::withheld::record_pending_withheld(
                        crate::protocol::withheld::WithheldMeta::from_content_findings(
                            relative_path,
                            &finding_descriptors,
                        ),
                    );
                }
                format::content_withheld_by_admission(
                    relative_path,
                    &rule_ids,
                    finding_count,
                    &finding_lines,
                )
            },
        );
    }

    // Permit. These are the only bytes any gated lane may render or parse.
    None
}

// ── Frozen seam anchor (C5) ────────────────────────────────────────────────

/// The single admission gate for raw-disk content reads, under its frozen
/// seam name. Every lane that reopens a file from disk routes through the
/// gate; this carrier's one associated operation IS that door.
pub struct ReadGate;

impl ReadGate {
    /// Admit one raw-disk read: policy refusals run before the file is
    /// opened, and the returned bytes are the gate's own read.
    pub fn admit_disk_read(
        live: &LiveIndex,
        relative_path: &str,
        canon_path: &Path,
    ) -> Result<Vec<u8>, String> {
        admit_disk_read(live, relative_path, canon_path)
    }
}

#[cfg(test)]
mod tests {
    /// The recorded-demotion line re-read has exactly one caller: the disk-read
    /// core behind `admit_disk_read`, on its rendering branch. Visibility keeps
    /// it inside this file; this pin keeps it out of every other lane here.
    #[test]
    fn recorded_line_reread_is_called_only_from_the_disk_read_lane() {
        let needle = concat!("recorded_refusal_", "naming_lines(");
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let src = manifest.join("src");
        let mut callers: Vec<(String, String)> = Vec::new();
        let mut stack = vec![src.clone()];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).expect("read source dir") {
                let path = entry.expect("dir entry").path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                    continue;
                }
                let relative = path
                    .strip_prefix(&src)
                    .expect("under src")
                    .to_string_lossy()
                    .replace('\\', "/");
                let contents = std::fs::read_to_string(&path).expect("read source file");
                let mut enclosing = String::new();
                for line in contents.lines() {
                    let trimmed = line.trim_start();
                    if trimmed.starts_with("//") {
                        continue;
                    }
                    for prefix in ["fn ", "pub fn ", "pub(crate) fn "] {
                        if let Some(rest) = trimmed.strip_prefix(prefix) {
                            enclosing = rest.split(['(', '<']).next().unwrap_or("").to_string();
                        }
                    }
                    if line.contains(needle) && !trimmed.contains(&format!("fn {needle}")) {
                        callers.push((relative.clone(), enclosing.clone()));
                    }
                }
            }
        }
        assert_eq!(
            callers,
            vec![("protocol/read_gate.rs".to_string(), "disk_read".to_string())],
            "the line re-read must be reached only through the disk-read lane"
        );
    }
}
