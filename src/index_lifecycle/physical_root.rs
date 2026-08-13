//! Owning physical-root lease and beneath-confined destructive I/O (T025).
//!
//! A lease owns one physical root and holds a **directory capability** for it
//! (`cap_std::fs::Dir`). Every path a permit touches is opened RELATIVE to that
//! handle, so confinement is enforced by the operating system at open time
//! rather than by a check that can go stale between looking and acting.
//!
//! This replaces an earlier design that resolved each component with
//! `symlink_metadata` and then opened the path separately. That left a
//! check-then-open window: a component swapped to a link after the check was
//! followed. The window is now closed rather than documented — `cap-std` opens
//! each component with no-follow semantics (`openat` with `O_NOFOLLOW` on Unix,
//! reparse-point-aware `NtCreateFile` on Windows) and refuses to traverse out of
//! the directory it was given, so a link planted inside root A cannot redirect a
//! write to root B whether it was planted before, during, or after the call.
//!
//! The userspace gate in front of that open is `Metadata::is_symlink()`. On
//! Windows that is the name-surrogate bit (`reparse_tag & 0x20000000`), so a
//! directory junction is refused the same way as a symlink. It is not "every
//! reparse point": a tag without that bit is not a `LinkComponent`.
//!
//! Absolute paths and `..` are refused before they reach the handle, so the
//! refusal names what was wrong instead of surfacing an opaque OS error.
//!
//! Replacement is two-phase: stage the content under an unpredictable temporary
//! name created with `create_new` (which refuses to open anything already
//! occupying the name), then rename it over the target. The target therefore
//! keeps its previous bytes until a complete replacement exists, and an
//! abandoned stage removes its own temporary.

use std::io::Write as _;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use cap_std::ambient_authority;
use cap_std::fs::Dir;

use super::authority::AuthorityRefusal;

static NEXT_ROOT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// Distinguishes concurrent replacements of the same target within one process.
static NEXT_TEMP: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// How many temporary names to try before refusing.
const MAX_TEMP_ATTEMPTS: u32 = 16;

/// Identity of one installed physical root. Never reused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PhysicalRootIdentity(std::num::NonZeroU64);

impl PhysicalRootIdentity {
    /// Mint a fresh never-reused physical-root identity.
    pub fn fresh() -> Self {
        let raw = NEXT_ROOT.fetch_add(1, Ordering::Relaxed);
        Self(std::num::NonZeroU64::new(raw).expect("root counter starts at 1"))
    }
}

/// Why a path could not be resolved beneath a lease's root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RootRefusal {
    /// The lease was revoked because its root was replaced.
    LeaseRevoked,
    /// The path was absolute, or escaped the root with `..` or a prefix.
    EscapesRoot {
        /// The offending relative path.
        requested: PathBuf,
    },
    /// A component is a symlink or a name-surrogate reparse point, which is
    /// never followed. On Windows that is the name-surrogate bit
    /// (`reparse_tag & 0x20000000`): junctions (`IO_REPARSE_TAG_MOUNT_POINT`)
    /// count, while a reparse tag without that bit does not.
    LinkComponent {
        /// The component that is a link.
        component: PathBuf,
    },
    /// The path could not be inspected.
    Unreadable {
        /// The path that could not be inspected.
        path: PathBuf,
        /// The OS error message.
        message: String,
    },
}

impl From<RootRefusal> for AuthorityRefusal {
    fn from(_: RootRefusal) -> Self {
        AuthorityRefusal::PhysicalRootReplaced
    }
}

/// An owning lease on one physical root.
///
/// The lease is the only thing that can resolve a path for a mutation. When the
/// root is replaced, the lease is revoked and every subsequent resolution fails,
/// which is what stops a root-A permit from writing after root B is installed.
#[derive(Debug)]
pub struct PhysicalRootLease {
    identity: PhysicalRootIdentity,
    root: PathBuf,
    /// The directory capability every operation goes through.
    ///
    /// `None` when the root could not be opened. A lease with no capability
    /// refuses everything rather than silently falling back to path-based I/O,
    /// which is the fallback that would reintroduce the escape this closes.
    dir: Option<Dir>,
    /// Shared with anything holding authority derived from this lease, so a
    /// revocation reaches a staged replacement that is between its two steps.
    revoked: Arc<AtomicBool>,
}

impl PhysicalRootLease {
    /// Take a lease on `root` under a fresh identity.
    ///
    /// Opening the directory here is what makes the confinement real: from this
    /// point every path is resolved relative to the handle, so the root cannot
    /// be swapped underneath the lease.
    pub fn take(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        let dir = Dir::open_ambient_dir(&root, ambient_authority()).ok();
        Self {
            identity: PhysicalRootIdentity::fresh(),
            root,
            dir,
            revoked: Arc::new(AtomicBool::new(false)),
        }
    }

    /// The directory capability, if the lease is live and the root opened.
    fn capability(&self) -> Result<&Dir, RootRefusal> {
        if !self.is_live() {
            return Err(RootRefusal::LeaseRevoked);
        }
        self.dir.as_ref().ok_or_else(|| RootRefusal::Unreadable {
            path: self.root.clone(),
            message: "the leased root could not be opened as a directory capability".to_owned(),
        })
    }

    /// This lease's root identity.
    pub fn identity(&self) -> PhysicalRootIdentity {
        self.identity
    }

    /// The root path this lease owns.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Whether the lease is still installed.
    pub fn is_live(&self) -> bool {
        !self.revoked.load(Ordering::Acquire)
    }

    /// Revoke the lease. Idempotent.
    pub fn revoke(&self) {
        self.revoked.store(true, Ordering::Release);
    }

    /// The liveness this lease shares with authority derived from it.
    fn revocation(&self) -> &Arc<AtomicBool> {
        &self.revoked
    }

    /// Resolve `relative` beneath this lease's root without following links.
    ///
    /// Returns the final parent directory and the leaf name, which is the pair a
    /// handle-relative implementation would return.
    pub fn resolve_beneath(&self, relative: &Path) -> Result<ResolvedTarget, RootRefusal> {
        let dir = self.capability()?;

        // Reject absolute paths and parent traversal BEFORE the handle sees
        // them, so the refusal names what was wrong instead of surfacing an
        // opaque OS error. `cap-std` would refuse these too; this is about the
        // quality of the diagnosis, not the strength of the guard.
        let mut components = Vec::new();
        for component in relative.components() {
            match component {
                Component::Normal(part) => components.push(part.to_os_string()),
                Component::CurDir => {}
                Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                    return Err(RootRefusal::EscapesRoot {
                        requested: relative.to_path_buf(),
                    });
                }
            }
        }

        let Some((leaf, parents)) = components.split_last() else {
            return Err(RootRefusal::EscapesRoot {
                requested: relative.to_path_buf(),
            });
        };

        // Walk the ancestors THROUGH THE CAPABILITY. Every lookup is relative to
        // the leased directory, so a component that is a link cannot be followed
        // out of the root even if it is swapped between this check and the open
        // that follows: the open is handle-relative too.
        let mut walked = PathBuf::new();
        let mut parent = self.root.clone();
        for part in parents {
            walked.push(part);
            parent.push(part);
            match dir.symlink_metadata(&walked) {
                Ok(metadata) if metadata.is_symlink() => {
                    return Err(RootRefusal::LinkComponent { component: parent });
                }
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(RootRefusal::Unreadable {
                        path: parent,
                        message: error.to_string(),
                    });
                }
            }
        }

        let mut relative_path = walked.clone();
        relative_path.push(leaf);

        Ok(ResolvedTarget {
            parent,
            leaf: leaf.clone(),
            relative: relative_path,
        })
    }

    /// Refuse a leaf that is itself a link, through the capability.
    fn refuse_link_relative(&self, relative: &Path) -> Result<(), RootRefusal> {
        let dir = self.capability()?;
        match dir.symlink_metadata(relative) {
            Ok(metadata) if metadata.is_symlink() => Err(RootRefusal::LinkComponent {
                component: self.root.join(relative),
            }),
            Ok(_) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(RootRefusal::Unreadable {
                path: self.root.join(relative),
                message: error.to_string(),
            }),
        }
    }
}

/// A path resolved beneath a lease: its final parent and leaf name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedTarget {
    parent: PathBuf,
    leaf: std::ffi::OsString,
    /// The path as the directory capability sees it. Every operation uses this;
    /// the absolute forms above are for diagnostics and receipts.
    relative: PathBuf,
}

impl ResolvedTarget {
    /// The final parent directory.
    pub fn parent(&self) -> &Path {
        &self.parent
    }

    /// The leaf name beneath that parent.
    pub fn leaf(&self) -> &std::ffi::OsStr {
        &self.leaf
    }

    /// The full resolved path, for diagnostics and receipts.
    pub fn path(&self) -> PathBuf {
        self.parent.join(&self.leaf)
    }

    /// The path relative to the leased directory capability.
    pub fn relative(&self) -> &Path {
        &self.relative
    }
}

/// One observable step of a destructive replacement, in the order it happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplacementStep {
    /// The replacement content was written to a temporary beneath the same parent.
    TempCreated,
    /// The temporary replaced the target.
    Replaced,
}

/// What a replacement actually did, recorded as it happened rather than asserted
/// afterwards.
///
/// The receipt names the lease that produced it. Without that, a receipt is just
/// a value a caller can hand to any permit, and a permit pinned to root A can
/// report success for a write that landed under root B.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteReceipt {
    steps: Vec<ReplacementStep>,
    target: PathBuf,
    lease: PhysicalRootIdentity,
}

impl WriteReceipt {
    /// The ordered steps that were observed.
    pub fn steps(&self) -> &[ReplacementStep] {
        &self.steps
    }

    /// The path that was replaced.
    pub fn target(&self) -> &Path {
        &self.target
    }

    /// The lease that actually performed this write.
    pub fn lease(&self) -> PhysicalRootIdentity {
        self.lease
    }
}

/// Replace `relative`'s contents beneath `lease`, temp-first.
///
/// The temporary is created beneath the target's own resolved parent and only
/// then renamed over the target, so the target is never removed or truncated
/// before its replacement exists.
pub fn replace_beneath(
    lease: &PhysicalRootLease,
    relative: &Path,
    contents: &[u8],
) -> Result<WriteReceipt, RootRefusal> {
    stage_replacement(lease, relative, contents)?.commit()
}

/// Stage a replacement without committing it.
///
/// Splitting the write in two is not a testing affordance bolted on: it makes
/// the ordering OBSERVABLE. An oracle can stage, look at the filesystem, and see
/// for itself that the temporary exists while the target still holds its
/// original bytes -- which is the actual claim. Asserting on a receipt's own
/// step list only ever proved that the receipt records what the receipt records;
/// a build that renamed first while pushing the labels in order would have
/// passed. Reviewer grok-4-5 found exactly that hole.
pub fn stage_replacement(
    lease: &PhysicalRootLease,
    relative: &Path,
    contents: &[u8],
) -> Result<StagedReplacement, RootRefusal> {
    let target = lease.resolve_beneath(relative)?;
    lease.refuse_link_relative(target.relative())?;
    let dir = lease.capability()?;
    // Cloned BEFORE anything is created. Cloning after the temporary was written
    // meant a failure here returned with the temporary already on disk and no
    // `Drop` guard yet in existence to remove it, contradicting this module's own
    // claim that an abandoned stage removes its own temporary.
    let staged_dir = dir.try_clone().map_err(|error| RootRefusal::Unreadable {
        path: lease.root().to_path_buf(),
        message: error.to_string(),
    })?;

    let mut steps = Vec::new();

    if let Some(parent) = target.relative().parent()
        && !parent.as_os_str().is_empty()
    {
        dir.create_dir_all(parent)
            .map_err(|error| RootRefusal::Unreadable {
                path: target.parent().to_path_buf(),
                message: error.to_string(),
            })?;
    }

    // `create_new` refuses to open anything already occupying the name,
    // including a symlink, and the open is handle-relative so it cannot escape
    // the leased directory. The unpredictable suffix removes the plant target in
    // the first place: a predictable temp name written through ambient `fs` was
    // a deterministic escape that needed no race at all.
    let mut temp_relative = PathBuf::new();
    let mut file = None;
    for attempt in 0..MAX_TEMP_ATTEMPTS {
        let mut name = target.leaf().to_os_string();
        name.push(format!(
            ".symforge-tmp-{}-{}-{attempt}",
            std::process::id(),
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        let candidate = match target.relative().parent() {
            Some(parent) if !parent.as_os_str().is_empty() => parent.join(&name),
            _ => PathBuf::from(&name),
        };
        let mut options = cap_std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        match dir.open_with(&candidate, &options) {
            Ok(handle) => {
                temp_relative = candidate;
                file = Some(handle);
                break;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(RootRefusal::Unreadable {
                    path: lease.root().join(&candidate),
                    message: error.to_string(),
                });
            }
        }
    }
    let Some(mut handle) = file else {
        return Err(RootRefusal::Unreadable {
            path: target.parent().to_path_buf(),
            message: "no unused temporary name was available beneath the leased root".to_owned(),
        });
    };

    let written = handle.write_all(contents).and_then(|()| handle.sync_all());
    drop(handle);
    if let Err(error) = written {
        let _ = dir.remove_file(&temp_relative);
        return Err(RootRefusal::Unreadable {
            path: lease.root().join(&temp_relative),
            message: error.to_string(),
        });
    }
    steps.push(ReplacementStep::TempCreated);

    let temp_path = lease.root().join(&temp_relative);
    Ok(StagedReplacement {
        temp_relative,
        temp_path,
        target_relative: target.relative().to_path_buf(),
        target: target.path(),
        lease: lease.identity(),
        revoked: Arc::clone(lease.revocation()),
        dir: staged_dir,
        steps,
    })
}

/// A replacement whose content is on disk but which has not replaced anything.
///
/// Dropping one without committing removes the temporary: an abandoned stage
/// must not leave litter beneath the leased root.
#[derive(Debug)]
pub struct StagedReplacement {
    temp_relative: PathBuf,
    temp_path: PathBuf,
    target_relative: PathBuf,
    target: PathBuf,
    lease: PhysicalRootIdentity,
    /// The originating lease's liveness, shared rather than copied.
    revoked: Arc<AtomicBool>,
    dir: Dir,
    steps: Vec<ReplacementStep>,
}

impl StagedReplacement {
    /// Where the staged content currently lives.
    pub fn temp_path(&self) -> &Path {
        &self.temp_path
    }

    /// The path this will replace when committed.
    pub fn target(&self) -> &Path {
        &self.target
    }

    /// Replace the target with the staged content.
    ///
    /// Re-checks the originating lease. `transition::apply` revokes the outgoing
    /// lease "so no surviving permit can resolve a path under the replaced root",
    /// and splitting the write into two steps opened a window that ordering was
    /// written to close: a stage taken before the install committed happily after
    /// it, and the resulting receipt named the revoked lease, so
    /// `SourceMutationPermit::commit` attested a write performed under authority
    /// that had been withdrawn. `Drop` removes the temporary on refusal.
    pub fn commit(mut self) -> Result<WriteReceipt, RootRefusal> {
        if self.revoked.load(Ordering::Acquire) {
            return Err(RootRefusal::LeaseRevoked);
        }
        if let Err(error) = self
            .dir
            .rename(&self.temp_relative, &self.dir, &self.target_relative)
        {
            let _ = self.dir.remove_file(&self.temp_relative);
            self.steps.clear();
            return Err(RootRefusal::Unreadable {
                path: self.target.clone(),
                message: error.to_string(),
            });
        }
        let receipt = WriteReceipt {
            steps: {
                let mut steps = std::mem::take(&mut self.steps);
                steps.push(ReplacementStep::Replaced);
                steps
            },
            target: self.target.clone(),
            lease: self.lease,
        };
        // The temporary no longer exists under its old name; forget it so `Drop`
        // does not try to remove the file we just renamed into place.
        self.temp_relative = PathBuf::new();
        self.temp_path = PathBuf::new();
        Ok(receipt)
    }
}

impl Drop for StagedReplacement {
    fn drop(&mut self) {
        if !self.temp_relative.as_os_str().is_empty() {
            let _ = self.dir.remove_file(&self.temp_relative);
        }
    }
}
