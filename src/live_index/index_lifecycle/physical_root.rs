//! Owning physical-root lease and beneath-confined destructive I/O (T025).
//!
//! A lease owns one physical root. Every path a permit touches is resolved
//! component-by-component beneath that root, refusing any symlink or reparse
//! point rather than following it, so a mutation authorized for root A can never
//! reach root B through a link planted inside A.
//!
//! ponytail: confinement is checked per component with `symlink_metadata` before
//! the open, which leaves a TOCTOU window between check and open. Closing it
//! needs handle-relative I/O (`openat`/`NtCreateFile` with
//! `FILE_FLAG_OPEN_REPARSE_POINT`), i.e. a `cap-std`-style `Dir` handle. That is
//! a dependency decision, deliberately not taken inside this slice; the seam
//! below (`resolve_beneath` returning a final-parent handle plus leaf name) is
//! the shape that upgrade would slot into.

use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use super::authority::AuthorityRefusal;

static NEXT_ROOT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteReceipt {
    steps: Vec<ReplacementStep>,
    target: PathBuf,
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
    let target = lease.resolve_beneath(relative)?;
    lease.refuse_link(&target.path())?;

    let mut steps = Vec::new();

    let mut temp_name = target.leaf().to_os_string();
    temp_name.push(format!(".symforge-tmp-{}", std::process::id()));
    let temp_path = target.parent().join(&temp_name);

    if let Some(parent) = temp_path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| RootRefusal::Unreadable {
            path: parent.to_path_buf(),
            message: error.to_string(),
        })?;
    }
    std::fs::write(&temp_path, contents).map_err(|error| RootRefusal::Unreadable {
        path: temp_path.clone(),
        message: error.to_string(),
    })?;
    steps.push(ReplacementStep::TempCreated);

    std::fs::rename(&temp_path, target.path()).map_err(|error| RootRefusal::Unreadable {
        path: target.path(),
        message: error.to_string(),
    })?;
    steps.push(ReplacementStep::Replaced);

    Ok(WriteReceipt {
        steps,
        target: target.path(),
    })
}
