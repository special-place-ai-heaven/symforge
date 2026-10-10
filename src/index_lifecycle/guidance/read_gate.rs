//! Transport-independent admission policy over exact byte buffers.
use super::read_admission_format as format;
use super::withheld::WithheldMeta;
use crate::domain::{FileDisposition, IndexTargets, LanguageId, MetadataOnlyReason};
use crate::live_index::LiveIndex;
use std::path::Path;

pub(crate) fn hard_scope_refusal(relative_path: &str) -> Option<String> {
    crate::discovery::path_is_hard_scope_excluded(Path::new(relative_path)).then(|| {
        format!(
            "{relative_path} [error: VCS and runtime-state internals are outside \
             source scope; a disk observation never reads them]"
        )
    })
}

pub(crate) fn normalize_requested_path(raw: &str) -> String {
    let mut normalized = raw.trim().replace('\\', "/");
    while normalized.starts_with("./") {
        normalized = normalized[2..].to_string();
    }
    normalized.trim_matches('/').to_string()
}

pub(crate) fn unverified_notice(live: &LiveIndex, requested: &str) -> Option<String> {
    let path = normalize_requested_path(requested);
    if crate::knowledge::sensitive_path_rule(&path).is_some() {
        return None;
    }
    live.unverified_since_restore(&path)
        .map(|reason| format::unverified_since_restore(&path, reason))
}

/// Predict — WITHOUT reading any bytes — whether `admit_disk_read` would
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

pub(crate) fn refuse_by_policy_with(
    live: &LiveIndex,
    relative_path: &str,
    record: &mut dyn FnMut(WithheldMeta),
) -> Option<String> {
    // Current path rule — no read needed.
    if let Some(rule_id) = crate::knowledge::sensitive_path_rule(relative_path) {
        record(WithheldMeta::path_rule_only(relative_path, rule_id));
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
                record(WithheldMeta::unscanned(relative_path));
                return Some(format::content_withheld_unscanned(relative_path));
            }
            MetadataOnlyReason::SensitivePath { rule_id } => {
                record(WithheldMeta::path_rule_only(relative_path, rule_id));
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

pub(crate) fn classify_admitted_bytes_with(
    live: &LiveIndex,
    relative_path: &str,
    bytes: &[u8],
    record: &mut dyn FnMut(WithheldMeta),
) -> Option<String> {
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
        record(WithheldMeta::unscanned(relative_path));
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
            let scan = match live.indexed_root.as_deref() {
                Some(root) => {
                    crate::knowledge::secret_dismissals::scan_with_dismissals(root, path, bytes)
                }
                None => crate::knowledge::scan_secret_bytes(path, bytes),
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
                record(WithheldMeta::unscanned(relative_path));
                format::content_withheld_unscanned(relative_path)
            } else {
                if !finding_descriptors.is_empty() {
                    record(WithheldMeta::from_content_findings(
                        relative_path,
                        &finding_descriptors,
                    ));
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

/// Match bounded current bytes to the recorded rule set before adding actionable
/// positions to a refusal. A changed verdict never weakens the recorded refusal.
pub(crate) fn recorded_finding_evidence_from_bytes(
    relative_path: &str,
    bytes: &[u8],
    recorded: &[String],
) -> (
    Vec<(u32, u32)>,
    Vec<crate::knowledge::SecretFindingDescriptor>,
) {
    if crate::knowledge::exceeds_scan_limit(bytes.len()) {
        return (Vec::new(), Vec::new());
    }
    match crate::knowledge::scan_secret_bytes(relative_path, bytes) {
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

// ── Repository object stores: the gated fetches shared by MCP and embed ──────
//
// The MCP read gate (`protocol::read_gate`) and the embedded query lanes both
// reach git objects and working-tree bytes through these functions. Each
// caller supplies only its `record` sink for withheld metadata; policy,
// confinement, the read itself and the classification of exactly those bytes
// live here, once.

/// Leading text of the refusal for a spelling that reaches a file whose
/// on-disk name is different. Callers match on it to surface the hint.
pub(crate) const PATH_SPELLING_DIFFERS: &str = "path spelling differs from the on-disk name";

/// [`crate::discovery::resolve_repo_path`] with its refusal rendered as the
/// caller-facing message. `Ok(None)` means nothing exists at that spelling.
pub(crate) fn resolve_repo_path(
    repo_root: &Path,
    relative_path: &str,
) -> Result<Option<std::path::PathBuf>, String> {
    use crate::discovery::PathRefusal;
    crate::discovery::resolve_repo_path(repo_root, relative_path).map_err(|refusal| match refusal {
        PathRefusal::OutsideRoot => format!("path '{relative_path}' is outside the repository"),
        PathRefusal::Unresolvable(message) => message,
        PathRefusal::WindowsAlias => format!(
            "path '{relative_path}' is an alias spelling on Windows (a ':' stream \
             suffix or a trailing dot or space); use the file's own name"
        ),
        // The refusal the on-disk name gets from the read gate.
        PathRefusal::CredentialAlias(rule_id) => {
            format::content_withheld_by_path_rule(relative_path, rule_id)
        }
        PathRefusal::SpellingDiffers(None) => PATH_SPELLING_DIFFERS.to_string(),
        PathRefusal::SpellingDiffers(Some(canonical)) => {
            format!("{PATH_SPELLING_DIFFERS}; retry with `{canonical}`")
        }
    })
}

/// The on-disk half of the gate's confinement, shared by every disk-reading
/// entry. It refuses any spelling the shared resolver refuses: an escape
/// through a symlink, or another spelling of a file. It also refuses VCS and
/// runtime-state internals (`.git`, `.symforge`), which the cold walk never
/// reads either.
pub(crate) fn refuse_disk_spelling(root: &Path, relative_path: &str) -> Result<(), String> {
    resolve_repo_path(root, relative_path)?;
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

/// Bytes of a regular file, or an error that did not block.
///
/// FIFO, socket, and device opens block until a peer appears. `symlink_metadata`
/// refuses those before `open`. On Unix the open itself is `O_NONBLOCK`, so a
/// replacement between the check and the open cannot hang the caller either.
/// On Windows `is_file` is true for a named pipe and for a character device
/// (`CON`, `COM1`, …). `CreateFile` on a listening pipe returns immediately —
/// `ReadFile` is what blocks — and `open_for_gate_read` refuses a non-disk
/// handle before that read. Symlinks are not followed: `symlink_metadata` sees
/// the link, not its target.
pub(crate) fn read_regular_file(path: &Path) -> std::io::Result<Vec<u8>> {
    read_regular_file_limited(path, None)
}

pub(crate) fn read_regular_file_limited(
    path: &Path,
    limit: Option<u64>,
) -> std::io::Result<Vec<u8>> {
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

pub(crate) fn open_for_gate_read(path: &Path) -> std::io::Result<std::fs::File> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(path)
    }
    #[cfg(windows)]
    {
        // `FILE_FLAG_OVERLAPPED` would make a later `std::fs::File` read fail
        // (`ERROR_INVALID_PARAMETER`); std does not drive overlapped I/O. The
        // open itself does not wait for a pipe peer — classify the handle and
        // refuse anything that is not a disk file before the caller reads.
        let file = std::fs::File::open(path)?;
        if !windows_handle_is_disk_file(&file) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "not a regular file",
            ));
        }
        Ok(file)
    }
    #[cfg(not(any(unix, windows)))]
    {
        std::fs::File::open(path)
    }
}

/// `GetFileType` of an opened handle, ignoring `FILE_TYPE_REMOTE`.
///
/// Fail closed: `FILE_TYPE_UNKNOWN` (the failure return) is not a disk file.
/// A remote disk file is `FILE_TYPE_DISK | FILE_TYPE_REMOTE` and still passes.
#[cfg(windows)]
#[allow(unsafe_code)]
fn windows_handle_is_disk_file(file: &std::fs::File) -> bool {
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Storage::FileSystem::GetFileType;

    // SAFETY: `file` owns a live kernel handle for the duration of the call.
    // `GetFileType` only classifies that handle; it does not read bytes or wait
    // for a pipe peer.
    let kind = unsafe { GetFileType(HANDLE(file.as_raw_handle())) };
    file_type_is_disk(kind.0)
}

/// Disk file per Win32 `GetFileType`. `FILE_TYPE_REMOTE` (0x8000) is a flag
/// or'd onto the type, so a remote disk file is `1 | 0x8000`, not `1`.
/// `FILE_TYPE_UNKNOWN` (0), pipes (3), and character devices (2) fail closed.
///
/// The numeric values are the Win32 constants. The predicate stays out of the
/// `windows` crate so a Linux test can lock the mask.
#[cfg(any(windows, test))]
pub(crate) fn file_type_is_disk(kind: u32) -> bool {
    const FILE_TYPE_DISK: u32 = 1;
    const FILE_TYPE_REMOTE: u32 = 0x8000;
    kind & !FILE_TYPE_REMOTE == FILE_TYPE_DISK
}

/// Read `canon_path` and return its bytes only if the file is admissible for
/// content disclosure. Policy refusals need no bytes, so they run BEFORE the
/// read; `name_lines` lets a caller that renders the refusal replace it with
/// one naming the recorded finding lines. The classification then runs on
/// exactly the bytes read here, which are the only bytes returned.
pub(crate) fn disk_read_with(
    live: &LiveIndex,
    relative_path: &str,
    canon_path: &Path,
    name_lines: &mut dyn FnMut() -> Option<String>,
    record: &mut dyn FnMut(WithheldMeta),
) -> Result<Vec<u8>, String> {
    if let Some(refusal) = refuse_by_policy_with(live, relative_path, record) {
        return Err(name_lines().unwrap_or(refusal));
    }
    // Required even when the manifest is clean or says Indexed: a clean
    // manifest cannot authorize bytes that changed after it was published.
    // Regular files only: a FIFO, socket, or device must not block the gate.
    let bytes = match read_regular_file(canon_path) {
        Ok(bytes) => bytes,
        Err(e) => return Err(format!("{relative_path} [error: could not read file: {e}]")),
    };
    if let Some(refusal) = classify_admitted_bytes_with(live, relative_path, &bytes, record) {
        return Err(refusal);
    }
    Ok(bytes)
}

/// Working-tree text for `relative_path`. `read` is the caller's admitted disk
/// read of the confined full path (normally [`disk_read_with`]).
///
/// Return shape mirrors `GitRepo::file_from_workdir` so refusal stays
/// distinguishable from absence:
///   * `Ok(None)` — not a regular file, or not valid UTF-8;
///   * `Ok(Some(text))` — admitted content, the only bytes a lane may use;
///   * `Err(message)` — REFUSED, carrying the caller-ready refusal.
pub(crate) fn worktree_text_with(
    repo: &crate::git::GitRepo,
    relative_path: &str,
    read: &mut dyn FnMut(&Path) -> Result<Vec<u8>, String>,
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
    let bytes = read(&full_path)?;
    Ok(String::from_utf8(bytes).ok())
}

/// Admit bytes the caller ALREADY HOLDS — a git blob, not a disk read. Policy
/// and content classification are identical to the disk lane.
///
/// This exists because `diff_symbols` gated its working-tree read and left the
/// two `file_at_ref` reads beside it ungated, which disclosed a demoted file's
/// symbol names and signatures out of git objects. A lane that holds repository
/// bytes must admit them, whatever object store they came from.
pub(crate) fn admit_bytes_with(
    live: &LiveIndex,
    relative_path: &str,
    bytes: Vec<u8>,
    record: &mut dyn FnMut(WithheldMeta),
) -> Result<Vec<u8>, String> {
    if let Some(refusal) = refuse_by_policy_with(live, relative_path, record) {
        return Err(refusal);
    }
    if let Some(refusal) = classify_admitted_bytes_with(live, relative_path, &bytes, record) {
        return Err(refusal);
    }
    Ok(bytes)
}

/// Text for `relative_path` as of `git_ref`, admitted by [`admit_bytes_with`].
///
/// Return shape mirrors `GitRepo::file_at_ref`:
///   * `Ok(None)` — absent at that ref, binary, or not valid UTF-8;
///   * `Ok(Some(text))` — admitted content, the only bytes a lane may use;
///   * `Err(message)` — REFUSED, carrying the caller-ready refusal.
pub(crate) fn admit_git_text_with(
    live: &LiveIndex,
    repo: &crate::git::GitRepo,
    git_ref: &str,
    relative_path: &str,
    record: &mut dyn FnMut(WithheldMeta),
) -> Result<Option<String>, String> {
    // Policy first: a path-ruled file is refused without touching the object
    // store at all.
    if let Some(refusal) = refuse_by_policy_with(live, relative_path, record) {
        return Err(refusal);
    }
    let Some(text) = repo.file_at_ref(git_ref, relative_path)? else {
        return Ok(None);
    };
    let admitted = admit_bytes_with(live, relative_path, text.into_bytes(), record)?;
    Ok(String::from_utf8(admitted).ok())
}
