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

use crate::domain::{FileDisposition, MetadataOnlyReason};
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
/// disk, [`crate::index_lifecycle::guidance::read_gate::refuse_disk_spelling`] refuses a symlink that
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

/// The refusal for a path under VCS or runtime-state internals (`.git`,
/// `.symforge`). Lexical and case-insensitive, so it needs no filesystem call
/// and answers the same whether or not the path exists.
pub(crate) use crate::index_lifecycle::guidance::read_gate::hard_scope_refusal;

/// The on-disk half of the gate's confinement, shared with the embedded lanes.
use crate::index_lifecycle::guidance::read_gate::refuse_disk_spelling;

#[cfg(test)]
use crate::index_lifecycle::guidance::read_gate::file_type_is_disk;
#[cfg(all(test, windows))]
use crate::index_lifecycle::guidance::read_gate::open_for_gate_read;
/// Regular-file reads that cannot block on a FIFO, socket, or device; the
/// shared gate owns them so the embedded lanes read through the same door.
#[cfg(test)]
use crate::index_lifecycle::guidance::read_gate::read_regular_file;
use crate::index_lifecycle::guidance::read_gate::read_regular_file_limited;

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
    // The gate owns the read: it classifies the exact buffer it just read and
    // returns it only on a permit, so no lane can classify one set of bytes and
    // then render another.
    crate::index_lifecycle::guidance::read_gate::worktree_text_with(
        repo,
        relative_path,
        &mut |full_path| disk_read(live, relative_path, full_path, name_lines),
    )
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
    crate::index_lifecycle::guidance::read_gate::refuse_by_policy_with(
        live,
        relative_path,
        &mut crate::protocol::withheld::record_pending_withheld,
    )
}

/// A caller's path as a catalog key: separators forward, no leading `./`,
/// no leading or trailing `/`. Pure string work, no filesystem access.
pub(crate) use crate::index_lifecycle::guidance::read_gate::normalize_requested_path;

/// The unverified-since-restore refusal for a path the caller named, when the
/// snapshot verify withheld it. In-memory only, like every policy refusal
/// here. A sensitive path is left to the path rule, which takes precedence.
pub(crate) use crate::index_lifecycle::guidance::read_gate::unverified_notice;

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
    crate::index_lifecycle::guidance::read_gate::recorded_finding_evidence_from_bytes(
        relative_path,
        &bytes,
        recorded,
    )
}

/// Text for `relative_path` as of `git_ref`, admitted by the shared gate's
/// byte admission (`admit_bytes_with`).
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
    // store at all. The shared gate owns that order and the classification.
    crate::index_lifecycle::guidance::read_gate::admit_git_text_with(
        live,
        repo,
        git_ref,
        relative_path,
        &mut crate::protocol::withheld::record_pending_withheld,
    )
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
    // Secret dismissals need no override here: every publication route
    // classifies with them applied, so a fully dismissed file is recorded
    // admitted, not demoted, and never reaches this refusal.
    crate::index_lifecycle::guidance::read_gate::disk_read_with(
        live,
        relative_path,
        canon_path,
        &mut || {
            if name_lines {
                recorded_refusal_naming_lines(live, relative_path)
            } else {
                None
            }
        },
        &mut crate::protocol::withheld::record_pending_withheld,
    )
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

    #[test]
    fn file_type_is_disk_accepts_remote_disk_and_rejects_pipes() {
        assert!(super::file_type_is_disk(1));
        assert!(super::file_type_is_disk(1 | 0x8000));
        assert!(!super::file_type_is_disk(2));
        assert!(!super::file_type_is_disk(3));
        assert!(!super::file_type_is_disk(3 | 0x8000));
        assert!(!super::file_type_is_disk(0));
    }

    #[test]
    fn read_regular_file_reads_bytes_of_a_regular_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("note.txt");
        std::fs::write(&path, b"hello").expect("write");
        let bytes = super::read_regular_file(&path).expect("regular file");
        assert_eq!(bytes, b"hello");
    }

    /// A listening named pipe is the Windows stand-in for a Unix FIFO.
    /// `CreateFile` connects without waiting for a writer; `ReadFile` would
    /// block until that writer appears. The pre-open `symlink_metadata` check
    /// can refuse without ever opening, so this calls `open_for_gate_read`
    /// directly — the TOCTOU path the metadata check does not cover.
    #[cfg(windows)]
    #[test]
    fn open_for_gate_read_does_not_block_on_a_named_pipe() {
        let pipe = IdleOutboundPipe::listen("open");
        let path = pipe.path.clone();
        let result = finish_within(
            std::time::Duration::from_secs(5),
            "open_for_gate_read",
            move || super::open_for_gate_read(&path),
        );
        let err = result.expect_err("a named pipe is not a regular file");
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput, "{err}");
        assert!(err.to_string().contains("not a regular file"), "{err}");
    }

    /// The full read, including the metadata pre-check, must also return.
    /// Metadata may refuse before the open; either way the call must not block
    /// and must not yield the pipe's bytes.
    #[cfg(windows)]
    #[test]
    fn read_regular_file_does_not_block_on_a_named_pipe() {
        let pipe = IdleOutboundPipe::listen("read");
        let path = pipe.path.clone();
        let result = finish_within(
            std::time::Duration::from_secs(5),
            "read_regular_file",
            move || super::read_regular_file(&path),
        );
        // Any error is a completed refusal. The budget above is the hang check:
        // a synchronous read of this pipe would still be blocked.
        let err = result.expect_err("a named pipe is not admissible content");
        assert!(
            err.to_string().contains("not a regular file") || err.raw_os_error().is_some(),
            "unexpected refusal: {err:?}"
        );
    }

    #[cfg(windows)]
    fn finish_within<T: Send + 'static>(
        budget: std::time::Duration,
        label: &str,
        work: impl FnOnce() -> T + Send + 'static,
    ) -> T {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(work());
        });
        match rx.recv_timeout(budget) {
            Ok(value) => value,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                panic!("{label} blocked for {budget:?}")
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                panic!("{label} ended without a result")
            }
        }
    }

    /// Server end of a byte-mode outbound pipe. No writer ever produces bytes,
    /// so a synchronous client read blocks. Drop closes the instance.
    #[cfg(windows)]
    struct IdleOutboundPipe {
        handle: windows::Win32::Foundation::HANDLE,
        path: std::path::PathBuf,
    }

    #[cfg(windows)]
    impl IdleOutboundPipe {
        #[allow(unsafe_code)]
        fn listen(label: &str) -> Self {
            use std::os::windows::ffi::OsStrExt;
            use windows::Win32::Foundation::HANDLE;
            use windows::Win32::Storage::FileSystem::{
                FILE_FLAG_FIRST_PIPE_INSTANCE, PIPE_ACCESS_OUTBOUND,
            };
            use windows::Win32::System::Pipes::{CreateNamedPipeW, PIPE_REJECT_REMOTE_CLIENTS};

            let path = std::path::PathBuf::from(format!(
                r"\\.\pipe\symforge-read-gate-{}-{label}",
                std::process::id()
            ));
            let wide: Vec<u16> = path
                .as_os_str()
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();
            // SAFETY: `wide` is NUL-terminated and outlives this call. Default
            // security (null attributes) lets this process open the client end.
            // The returned handle is owned by `IdleOutboundPipe`.
            // Byte mode and `PIPE_WAIT` are the zero defaults; only the
            // non-zero mode flag is passed, so a later `| 0` cannot trip
            // clippy. A synchronous client read of this instance blocks.
            let handle: HANDLE = unsafe {
                CreateNamedPipeW(
                    windows::core::PCWSTR(wide.as_ptr()),
                    PIPE_ACCESS_OUTBOUND | FILE_FLAG_FIRST_PIPE_INSTANCE,
                    PIPE_REJECT_REMOTE_CLIENTS,
                    1,
                    4096,
                    4096,
                    0,
                    None,
                )
            };
            assert!(
                !handle.is_invalid(),
                "CreateNamedPipeW({}): {}",
                path.display(),
                std::io::Error::last_os_error()
            );
            Self { handle, path }
        }
    }

    #[cfg(windows)]
    impl Drop for IdleOutboundPipe {
        #[allow(unsafe_code)]
        fn drop(&mut self) {
            use windows::Win32::Foundation::CloseHandle;
            // SAFETY: `handle` came from `CreateNamedPipeW` and is closed once,
            // here. Closing it unblocks a client still stuck in `ReadFile`.
            unsafe { CloseHandle(self.handle) }.ok();
        }
    }
}
