//! Owning physical-root lease and beneath-confined destructive I/O (T025).
//!
//! A lease owns one physical root. Every path a permit touches is resolved
//! component-by-component beneath that root, refusing any symlink or reparse
//! point rather than following it.
//!
//! **What this module does NOT claim.** Confinement is *not* closed. Each
//! component is checked with `symlink_metadata` and then opened separately, so a
//! component swapped to a link between the check and the open is followed. An
//! earlier version of this comment said a mutation authorized for root A "can
//! never reach root B through a link planted inside A"; that asserted more than
//! the code observes, and the honest statement is narrower: **link metadata is
//! refused at check time, and the check-then-open window is open.**
//!
//! The one escape that needed no race at all is closed. The replacement
//! temporary used to carry a predictable name and be written with `fs::write`,
//! which follows links, so a link planted at that path any time beforehand
//! redirected the write outside the root deterministically. The temporary is now
//! created with `create_new`, which refuses to open anything that already
//! exists — link included — under an unpredictable name.
//!
//! Closing the remaining window needs handle-relative I/O (`openat`, or
//! `NtCreateFile` with `FILE_FLAG_OPEN_REPARSE_POINT`), i.e. a `cap-std`-style
//! `Dir` handle. That is a dependency decision deliberately not taken inside this
//! slice; `resolve_beneath` returns a final parent plus leaf name, which is the
//! shape that upgrade slots into.

use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

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
    /// A component is a symlink or reparse point, which is never followed.
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
    revoked: AtomicBool,
}

impl PhysicalRootLease {
    /// Take a lease on `root` under a fresh identity.
    pub fn take(root: impl Into<PathBuf>) -> Self {
        Self {
            identity: PhysicalRootIdentity::fresh(),
            root: root.into(),
            revoked: AtomicBool::new(false),
        }
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

    /// Resolve `relative` beneath this lease's root without following links.
    ///
    /// Returns the final parent directory and the leaf name, which is the pair a
    /// handle-relative implementation would return.
    pub fn resolve_beneath(&self, relative: &Path) -> Result<ResolvedTarget, RootRefusal> {
        if !self.is_live() {
            return Err(RootRefusal::LeaseRevoked);
        }

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

        let mut parent = self.root.clone();
        for part in parents {
            parent.push(part);
            self.refuse_link(&parent)?;
        }

        Ok(ResolvedTarget {
            parent,
            leaf: leaf.clone(),
        })
    }

    /// Refuse a component that is a symlink or reparse point. A missing
    /// component is not a link, so it is permitted: creation is the caller's
    /// business, escape is not.
    fn refuse_link(&self, path: &Path) -> Result<(), RootRefusal> {
        match std::fs::symlink_metadata(path) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink()
                    || crate::paths::metadata_is_reparse_point(&metadata)
                {
                    Err(RootRefusal::LinkComponent {
                        component: path.to_path_buf(),
                    })
                } else {
                    Ok(())
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(RootRefusal::Unreadable {
                path: path.to_path_buf(),
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

    /// The full resolved path.
    pub fn path(&self) -> PathBuf {
        self.parent.join(&self.leaf)
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
    lease.refuse_link(&target.path())?;

    let mut steps = Vec::new();

    if let Some(parent) = target.path().parent() {
        std::fs::create_dir_all(parent).map_err(|error| RootRefusal::Unreadable {
            path: parent.to_path_buf(),
            message: error.to_string(),
        })?;
    }

    // The temporary is created with `create_new`, which fails if ANYTHING
    // already occupies the name -- including a symlink or reparse point. This is
    // not a refinement of the link check: a predictable temp name written with
    // `fs::write` follows a link that was planted long before the mutation, so
    // the escape needs no race to win and no TOCTOU window to exploit. Refusing
    // to create over an existing name closes it outright, and the unpredictable
    // suffix removes the plant target in the first place.
    let mut temp_path = PathBuf::new();
    let mut file = None;
    for attempt in 0..MAX_TEMP_ATTEMPTS {
        let mut name = target.leaf().to_os_string();
        name.push(format!(
            ".symforge-tmp-{}-{}-{attempt}",
            std::process::id(),
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        let candidate = target.parent().join(&name);
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(handle) => {
                temp_path = candidate;
                file = Some(handle);
                break;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(RootRefusal::Unreadable {
                    path: candidate,
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

    let written = std::io::Write::write_all(&mut handle, contents).and_then(|()| handle.sync_all());
    drop(handle);
    if let Err(error) = written {
        let _ = std::fs::remove_file(&temp_path);
        return Err(RootRefusal::Unreadable {
            path: temp_path,
            message: error.to_string(),
        });
    }
    steps.push(ReplacementStep::TempCreated);

    Ok(StagedReplacement {
        temp_path,
        target: target.path(),
        lease: lease.identity(),
        steps,
    })
}

/// A replacement whose content is on disk but which has not replaced anything.
///
/// Dropping one without committing removes the temporary: an abandoned stage
/// must not leave litter beneath the leased root.
#[derive(Debug)]
pub struct StagedReplacement {
    temp_path: PathBuf,
    target: PathBuf,
    lease: PhysicalRootIdentity,
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
    pub fn commit(mut self) -> Result<WriteReceipt, RootRefusal> {
        if let Err(error) = std::fs::rename(&self.temp_path, &self.target) {
            let _ = std::fs::remove_file(&self.temp_path);
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
        self.temp_path = PathBuf::new();
        Ok(receipt)
    }
}

impl Drop for StagedReplacement {
    fn drop(&mut self) {
        if !self.temp_path.as_os_str().is_empty() {
            let _ = std::fs::remove_file(&self.temp_path);
        }
    }
}
