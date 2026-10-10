use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Atomic file write
// ---------------------------------------------------------------------------

/// Write content to a file atomically: write to a unique temp file in the same directory,
/// then rename over the target. Using a `NamedTempFile` in the same directory ensures the
/// rename is within a single filesystem (no cross-device move) and avoids collisions between
/// concurrent callers that would occur with a fixed `.symforge_tmp` extension.
#[derive(Debug, Clone)]
pub(crate) struct AtomicWriteReport {
    pub tee_snapshot: crate::edit_safety::tee::TeeSnapshot,
}

/// Wrap an authority refusal for this module's `io::Result` writers.
fn authority_refused(
    refusal: crate::live_index::index_lifecycle::authority::AuthorityRefusal,
) -> std::io::Error {
    std::io::Error::other(format!("source mutation authority refused: {refusal:?}"))
}

pub(crate) fn atomic_write_file(
    repo_root: &Path,
    project_state_dir: Option<&crate::domain::ProjectStateDir>,
    path: &Path,
    content: &[u8],
) -> std::io::Result<AtomicWriteReport> {
    atomic_write_file_expected(repo_root, project_state_dir, path, content, None)
}

pub(crate) fn atomic_write_file_expected(
    repo_root: &Path,
    project_state_dir: Option<&crate::domain::ProjectStateDir>,
    path: &Path,
    content: &[u8],
    expected_publication: Option<crate::lifecycle_identity::PublicationIdentity>,
) -> std::io::Result<AtomicWriteReport> {
    // The tee snapshot is a ProjectStateDir state write and stays permit-free
    // per the frozen writers-category assertion; only the repository-source
    // byte write below carries mutation authority.
    let tee_snapshot = match project_state_dir {
        Some(state_dir) => crate::edit_safety::tee::Tee::for_repo(repo_root, state_dir)
            .snapshot(path)
            .unwrap_or_else(|err| crate::edit_safety::tee::TeeSnapshot::Warning {
                original_path: path.to_path_buf(),
                message: format!("unexpected tee snapshot error: {err}"),
            }),
        None => crate::edit_safety::tee::TeeSnapshot::Warning {
            original_path: path.to_path_buf(),
            message: "project state unavailable; tee snapshot disabled".to_string(),
        },
    };
    match path.strip_prefix(repo_root) {
        Ok(relative) => {
            // The lease's staged replacement deliberately creates missing
            // parents beneath the confined root (physical_root.rs); this
            // seam preserves the V10 refusal instead — pinned by
            // test_atomic_write_error_path_no_orphan — so an edit-tool
            // write into a nonexistent directory stays an error, checked
            // before any grant is spent.
            match path.parent() {
                None => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "path has no parent directory",
                    ));
                }
                Some(parent) if !parent.exists() => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        "target parent directory does not exist",
                    ));
                }
                Some(_) => {}
            }
            // T064: the repository-source byte write obtains a current
            // SourceMutationPermit BEFORE I/O, writes beneath the permit's
            // own confined lease, and returns to Current only through a
            // fresh publication. A sibling writer holds the source
            // non-Current for the length of one write cycle, so a brief
            // bounded wait stands in for the daemon-level serialization the
            // C4 root commit makes structural.
            let authority =
                crate::live_index::index_lifecycle::activation::project_source_authority(repo_root);
            let mut write = {
                let mut attempts = 0u32;
                loop {
                    let acquisition = match expected_publication {
                        Some(expected) => authority.acquire_write_expected(expected),
                        None => authority.acquire_write(),
                    };
                    match acquisition {
                        Ok(write) => break write,
                        Err(
                            refusal @ crate::live_index::index_lifecycle::authority::AuthorityRefusal::PhaseNotCurrent { .. },
                        ) => {
                            attempts += 1;
                            if attempts >= 80 {
                                // ~2s of sibling-writer patience, then the
                                // refusal surfaces honestly.
                                return Err(authority_refused(refusal));
                            }
                            std::thread::sleep(std::time::Duration::from_millis(25));
                        }
                        Err(refusal) => return Err(authority_refused(refusal)),
                    }
                }
            };
            let receipt = write.write(relative, content).map_err(authority_refused)?;
            write.finish_committed(receipt).map_err(authority_refused)?;
        }
        Err(_) => {
            if expected_publication.is_some() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "expected-publication writes cannot leave their source root",
                ));
            }
            // Resolve-hook reroute outside the project root (worktree lane).
            // Recorded C2 residual: this lane keeps the legacy tempfile
            // write until the per-root authorities of C3/C4 attach to
            // resolved targets (execution map, seal section).
            use std::io::Write;
            let parent = path.parent().ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "path has no parent directory",
                )
            })?;
            let mut tmp = tempfile::NamedTempFile::new_in(parent)?;
            tmp.write_all(content)?;
            tmp.flush()?;
            tmp.as_file().sync_all()?;
            // persist() uses rename(2) on Unix and
            // MoveFileExW(MOVEFILE_REPLACE_EXISTING) on Windows, atomically
            // replacing any existing target file.
            tmp.persist(path).map_err(|e| e.error)?;
        }
    }
    Ok(AtomicWriteReport { tee_snapshot })
}

/// Outcome of a write-time `if_match` guard check (TR-06 / FR-009).
///
/// `Rejected` means the on-disk bytes diverged from the base the splice was
/// computed against AFTER the caller's read, so the guarded apply is refused
/// and NOTHING is written — the divergent on-disk content is left intact.
pub(crate) enum GuardedWriteOutcome {
    /// The write committed; carries the atomic-write report for the response.
    Written(AtomicWriteReport),
    /// The guard rejected the write; the on-disk content is unchanged.
    Rejected,
}

/// Test-only interleave hook for the TR-06 regression test.
///
/// Installed by the concurrent-change test to simulate a writer that lands
/// inside the guarded window: it fires INSIDE [`guarded_atomic_write_file`],
/// strictly BEFORE the write-time on-disk re-read, so the subsequent re-read
/// observes the injected divergence deterministically (no sleep, no extra
/// thread). It is compiled out of release builds.
#[cfg(all(test, feature = "server"))]
pub(crate) use write_interleave::install as install_write_interleave_hook;

/// Process-global registry of per-path write locks (TR-06 / FR-009, design D1).
///
/// Each distinct target file maps to one `Arc<Mutex<()>>`; [`lock_for_path`]
/// hands out the same mutex for every write to that path so the
/// re-read → rename critical section in [`guarded_atomic_write_file`] is
/// serialized PER FILE — unrelated files never contend. `std::sync::Mutex` is
/// deliberate: the guarded write runs in sync / `spawn_blocking` context and
/// holds NO `.await` across the lock, so a tokio mutex would be both wrong
/// (cannot be held across the blocking rename without an async runtime) and
/// unnecessary.
///
/// Memory: the map is never evicted. It is keyed by canonical path, so its
/// size is bounded by the number of distinct files ever written in the
/// process — i.e. the repo's file count. That is acceptable; deliberately not
/// GC'd to keep the lock identity stable for the process lifetime.
static PATH_WRITE_LOCKS: std::sync::OnceLock<
    std::sync::Mutex<std::collections::HashMap<PathBuf, std::sync::Arc<std::sync::Mutex<()>>>>,
> = std::sync::OnceLock::new();

/// Return the process-global write lock for `key`, creating it on first use.
///
/// `key` MUST be a canonicalized path (see [`guarded_atomic_write_file`]) so
/// symlink / relative / `.`-segment variants of the same file all map to a
/// single lock and cannot race each other.
pub(crate) fn lock_for_path(key: &Path) -> std::sync::Arc<std::sync::Mutex<()>> {
    let map =
        PATH_WRITE_LOCKS.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()));
    let mut guard = map.lock().expect("path write-lock map poisoned");
    std::sync::Arc::clone(
        guard
            .entry(key.to_path_buf())
            .or_insert_with(|| std::sync::Arc::new(std::sync::Mutex::new(()))),
    )
}

/// Atomic write with a write-time `if_match` optimistic-concurrency guard
/// (TR-06 / FR-009, design D1).
///
/// `base` is the exact byte image the splice in `new_content` was computed
/// against (the index snapshot, or the rebased worktree target). The entire
/// re-read → rename critical section runs under a process-global per-path
/// mutex ([`lock_for_path`], keyed by the canonical path), so two in-process
/// writers targeting the SAME file are serialized: the second blocks until the
/// first's rename commits, then — if it supplied `if_match` — its re-read sees
/// the first's committed bytes (`on_disk != base`) and the apply is REJECTED
/// with no write, preserving the concurrent change (US3 AC-1).
///
/// The per-path lock is taken for EVERY write through this function, including
/// the `if_match: None` case. That is intentional: if an unguarded write could
/// slip between a guarded writer's re-read and its rename, the guarded writer
/// would still clobber it. The re-read/compare itself stays gated on
/// `if_match.is_some()` (an unguarded write keeps today's last-writer-wins
/// semantics), but the LOCK is unconditional so the critical section is never
/// interleaved by another in-process write to the same path.
///
/// HONESTY / SCOPE: the per-path mutex serializes ALL in-process writes to a
/// given path, so two concurrent same-file applies through SymForge cannot
/// clobber each other. It is NOT an OS-level file lock: a truly external,
/// non-SymForge process writing the file between the re-read and the rename is
/// outside this lock and is not serialized by it. For SymForge's own
/// multi-agent workflow — every writer funnels through the same in-process
/// server — the clobber is closed on every surface (in-process facade, daemon,
/// serve). The residual is the external-editor case only.
pub(crate) fn guarded_atomic_write_file(
    repo_root: &Path,
    project_state_dir: Option<&crate::domain::ProjectStateDir>,
    path: &Path,
    base: &[u8],
    new_content: &[u8],
    if_match: Option<&str>,
) -> std::io::Result<GuardedWriteOutcome> {
    guarded_atomic_write_file_expected(
        repo_root,
        project_state_dir,
        path,
        base,
        new_content,
        if_match,
        None,
    )
}

pub(crate) fn guarded_atomic_write_file_expected(
    repo_root: &Path,
    project_state_dir: Option<&crate::domain::ProjectStateDir>,
    path: &Path,
    base: &[u8],
    new_content: &[u8],
    if_match: Option<&str>,
    expected_publication: Option<crate::lifecycle_identity::PublicationIdentity>,
) -> std::io::Result<GuardedWriteOutcome> {
    // Pin the path once: canonicalize so symlink / relative variants resolve to
    // the same lock key AND so the re-read and the write operate on the same
    // resolved path (mitigates symlink TOCTOU on the re-read). Fall back to the
    // caller's path if canonicalize fails (e.g. parent dir not yet canonical on
    // some platforms); the lock map then keys on the non-canonical path, which
    // is still consistent within the process for that exact path value.
    // Symlink assumption: the canonical key collapses symlink aliases to one
    // lock, so concurrent SymForge writers cannot race through different aliases
    // of the same file.
    let pinned = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());

    // Acquire the per-path lock and HOLD it across BOTH the on-disk re-read and
    // the atomic rename. `_write_lock` lives to the end of the function, so the
    // whole critical section is serialized against any other in-process write to
    // this path. No `.await` exists in this function — the std mutex is correct.
    let lock = lock_for_path(&pinned);
    let _write_lock = lock.lock().expect("per-path write lock poisoned");

    // Deterministic test interleave point: a concurrent writer "lands" here,
    // strictly before the on-disk re-read below (no-op in release). It fires
    // INSIDE the lock by design — the T022 hook simulates a writer that already
    // committed before this writer entered the critical section.
    #[cfg(test)]
    write_interleave::fire();

    if if_match.is_some() {
        // Re-read the bytes actually on disk right now and compare to the
        // base image the splice was computed against. Divergence => a writer
        // changed the file after the caller's read; reject without writing.
        // Re-read via the pinned (canonical) path so we compare the same file
        // we are about to write.
        match std::fs::read(&pinned) {
            Ok(on_disk) => {
                if on_disk.as_slice() != base {
                    return Ok(GuardedWriteOutcome::Rejected);
                }
            }
            Err(err) => return Err(err),
        }
    }

    atomic_write_file_expected(
        repo_root,
        project_state_dir,
        path,
        new_content,
        expected_publication,
    )
    .map(GuardedWriteOutcome::Written)
}

pub(crate) fn format_tee_snapshot_suffix(report: &AtomicWriteReport) -> String {
    report
        .tee_snapshot
        .response_hint()
        .map(|hint| format!("\n{hint}"))
        .unwrap_or_default()
}

#[cfg(test)]
mod write_interleave {
    use std::cell::RefCell;

    type Hook = Box<dyn Fn()>;

    thread_local! {
        static HOOK: RefCell<Option<Hook>> = const { RefCell::new(None) };
    }

    /// RAII guard that uninstalls the hook on drop so tests cannot leak it
    /// across the thread-local into a sibling test on the same thread.
    #[cfg(feature = "server")]
    pub(crate) struct InterleaveGuard;

    #[cfg(feature = "server")]
    impl Drop for InterleaveGuard {
        fn drop(&mut self) {
            HOOK.with(|h| *h.borrow_mut() = None);
        }
    }

    /// Install a callback fired at the next guarded-write interleave point.
    ///
    /// The hook is consumed on first fire (see [`fire`]), so it runs at most
    /// once per `install` — a second guarded write on the same thread does not
    /// re-trigger it. This keeps the T022 interleave deterministic: exactly one
    /// simulated concurrent write lands in the guarded window.
    #[cfg(feature = "server")]
    pub(crate) fn install(hook: impl Fn() + 'static) -> InterleaveGuard {
        HOOK.with(|h| *h.borrow_mut() = Some(Box::new(hook)));
        InterleaveGuard
    }

    /// Fire the installed hook if one is present, consuming it so it fires at
    /// most once. Called from the guarded write path before the on-disk
    /// re-read. `take()` removes the hook before invoking it so a re-entrant or
    /// subsequent guarded write does not fire it again.
    pub(crate) fn fire() {
        let hook = HOOK.with(|h| h.borrow_mut().take());
        if let Some(hook) = hook {
            hook();
        }
    }
}
