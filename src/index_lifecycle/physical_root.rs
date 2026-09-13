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

    /// Stable stored form for cross-handle admission comparison.
    pub fn as_u64(self) -> u64 {
        self.0.get()
    }

    /// Rehydrate from a stored admission identity. Zero means unset.
    pub fn from_stored(raw: u64) -> Option<Self> {
        std::num::NonZeroU64::new(raw).map(Self)
    }
}

/// Observed physical object at one path. Detects same-path replacement (ABA)
/// without rekeying the path convergence map.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PhysicalRootAnchor {
    dev: u64,
    ino: u64,
}

impl PhysicalRootAnchor {
    /// Observe the directory object currently installed at `path`.
    pub fn observe(path: &Path) -> Option<Self> {
        observe_physical_root_anchor(path)
    }
}

#[cfg(unix)]
fn observe_physical_root_anchor(path: &Path) -> Option<PhysicalRootAnchor> {
    use std::os::unix::fs::MetadataExt;

    let metadata = std::fs::metadata(path).ok()?;
    Some(PhysicalRootAnchor {
        dev: metadata.dev(),
        ino: metadata.ino(),
    })
}

/// Stable Windows identity via `GetFileInformationByHandle`: the same
/// volume-serial/file-index pair std's unstable `windows_by_handle` methods
/// would expose. The path is opened as a DIRECTORY-capable handle with
/// attribute-only access, so observation never conflicts with concurrent
/// watchers, editors, or indexers.
///
/// `unsafe` here is FFI-only; the crate-level `unsafe_code = "deny"` is opted
/// out per-item, matching `cli/update.rs`'s native-Win32 precedent. Each call
/// carries its own SAFETY justification.
#[cfg(windows)]
#[allow(unsafe_code)]
fn observe_physical_root_anchor(path: &Path) -> Option<PhysicalRootAnchor> {
    use std::os::windows::fs::OpenOptionsExt;
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, FILE_FLAG_BACKUP_SEMANTICS, FILE_READ_ATTRIBUTES,
        FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, GetFileInformationByHandle,
    };

    // Attribute-only access (`FILE_READ_ATTRIBUTES`), fully shared, and
    // `FILE_FLAG_BACKUP_SEMANTICS` because a directory cannot be opened
    // without it. The handle stays owned by `opened`: closing it here as well
    // would double-close a handle value the kernel may already have handed to
    // another object.
    let opened = std::fs::OpenOptions::new()
        .access_mode(FILE_READ_ATTRIBUTES.0)
        .share_mode((FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE).0)
        .attributes(FILE_FLAG_BACKUP_SEMANTICS.0)
        .open(path)
        .ok()?;

    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    // SAFETY: `opened` owns a live kernel handle for the duration of the call
    // and `info` is the correctly sized out-buffer the API fills.
    let queried = unsafe { GetFileInformationByHandle(HANDLE(opened.as_raw_handle()), &mut info) };
    queried.ok()?;

    Some(PhysicalRootAnchor {
        dev: u64::from(info.dwVolumeSerialNumber),
        ino: (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow),
    })
}

#[cfg(not(any(unix, windows)))]
fn observe_physical_root_anchor(_path: &Path) -> Option<PhysicalRootAnchor> {
    None
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

    /// A DORMANT copy of this lease: same identity, same shared revocation, no
    /// OS handle.
    ///
    /// A root with no outstanding permit must stay user-movable — on Windows
    /// an open directory handle blocks renaming (and, pre-POSIX-semantics,
    /// deleting) the root, so an authority that idled with the capability open
    /// held every repository it ever wrote hostage (found by the C3b curation
    /// foreign-root oracles). The confinement claim in the module doc protects
    /// a WRITE's resolution, and a dormant lease can resolve nothing; the
    /// handle therefore exists only for the duration of a permit cycle
    /// ([`Self::reopened`] to begin one). Between cycles, root-swap detection
    /// belongs to the observation lane's baseline latches, not to a handle
    /// nothing is resolving through.
    pub fn parked(&self) -> Self {
        Self {
            identity: self.identity,
            root: self.root.clone(),
            dir: None,
            revoked: Arc::clone(&self.revoked),
        }
    }

    /// Reopen a dormant lease's capability for one permit cycle, preserving
    /// identity and revocation. From this open until the cycle's terminal
    /// transition, every resolution is handle-relative exactly as the module
    /// doc claims. A root that vanished while dormant yields a capability-less
    /// lease that refuses every resolution rather than falling back to
    /// path-based I/O.
    pub fn reopened(&self) -> Self {
        Self {
            identity: self.identity,
            root: self.root.clone(),
            dir: Dir::open_ambient_dir(&self.root, ambient_authority()).ok(),
            revoked: Arc::clone(&self.revoked),
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
    /// A DELEGATED replacement's post-image was read back through the lease's
    /// capability and matched the authorized bytes exactly. The lease did not
    /// perform the write, so no temp/replace pair is claimed — this step
    /// records the one thing the lease actually observed.
    DelegatedVerified,
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

/// Verify a DELEGATED replacement beneath `lease`: the caller ran its own
/// durability protocol against `relative` (a lane whose staged fsync/failpoint
/// protocol is contract-pinned and cannot be replaced by [`replace_beneath`]);
/// the lease re-reads the target through its capability and mints a receipt
/// only when the bytes it observes are exactly the authorized post-image.
///
/// `Ok(None)` is a mismatch: the lease observed bytes it did not authorize, so
/// nothing attests the write and the caller's only honest terminal is the
/// drop-recovery lane. The traversal/link guards refuse exactly as they do for
/// a lease-performed write.
pub fn verify_replacement_beneath(
    lease: &PhysicalRootLease,
    relative: &Path,
    expected: &[u8],
) -> Result<Option<WriteReceipt>, RootRefusal> {
    let target = lease.resolve_beneath(relative)?;
    lease.refuse_link_relative(target.relative())?;
    let dir = lease.capability()?;
    let observed = dir
        .read(target.relative())
        .map_err(|error| RootRefusal::Unreadable {
            path: target.path(),
            message: error.to_string(),
        })?;
    if observed != expected {
        return Ok(None);
    }
    Ok(Some(WriteReceipt {
        steps: vec![ReplacementStep::DelegatedVerified],
        target: target.path(),
        lease: lease.identity(),
    }))
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

/// Behavior tests for [`PhysicalRootAnchor::observe`], written against the
/// real filesystem: every assertion here pins an observable property of
/// object identity, and each is expected to go red under the mutation that
/// removes the property it defends.
#[cfg(all(test, any(unix, windows)))]
mod anchor_tests {
    use super::PhysicalRootAnchor;
    use std::fs;

    fn temp_root() -> tempfile::TempDir {
        tempfile::tempdir().expect("temporary root directory")
    }

    /// Two distinct directory objects at two distinct paths must never share
    /// an identity. (Same volume here, so this exercises the index half as
    /// the discriminator, exactly as same-path replacement would.)
    #[test]
    fn distinct_directories_yield_distinct_anchors() {
        let root = temp_root();
        let a = root.path().join("a");
        let b = root.path().join("b");
        fs::create_dir(&a).unwrap();
        fs::create_dir(&b).unwrap();

        let anchor_a = PhysicalRootAnchor::observe(&a).expect("anchor for a");
        let anchor_b = PhysicalRootAnchor::observe(&b).expect("anchor for b");

        assert_ne!(
            anchor_a, anchor_b,
            "distinct directories at distinct paths must not share an anchor"
        );
    }

    /// The same directory observed twice must yield the same anchor, or every
    /// re-observation would look like a replacement.
    #[test]
    fn same_directory_observed_twice_yields_same_anchor() {
        let root = temp_root();
        let a = root.path().join("a");
        fs::create_dir(&a).unwrap();

        let first = PhysicalRootAnchor::observe(&a).expect("first observation");
        let second = PhysicalRootAnchor::observe(&a).expect("second observation");

        assert_eq!(
            first, second,
            "a stable directory must not change identity between observations"
        );
    }

    /// THE property the anchor type exists for: a directory replaced by a
    /// different directory at the same path must be noticed. This is exactly
    /// the test that goes red if a platform implementation degrades to
    /// `None` — the compile-clean shortcut that would silently drop ABA
    /// detection.
    #[test]
    fn directory_replaced_at_same_path_yields_new_anchor() {
        let root = temp_root();
        let a = root.path().join("a");
        // Non-empty, so nothing can quietly rename over it; replacement goes
        // through an explicit remove.
        fs::create_dir_all(a.join("occupied")).unwrap();
        let b = root.path().join("b");
        fs::create_dir(&b).unwrap();

        let before = PhysicalRootAnchor::observe(&a).expect("anchor before replacement");

        fs::remove_dir_all(&a).unwrap();
        fs::rename(&b, &a).unwrap();

        let after = PhysicalRootAnchor::observe(&a).expect("anchor after replacement");

        assert_ne!(
            before, after,
            "a different directory installed at the same path must change the anchor (ABA)"
        );
    }

    /// An absent path observes to `None` — no error, no panic. The
    /// `Option`-returning contract every caller already handles.
    #[test]
    fn missing_path_yields_none() {
        let root = temp_root();
        let missing = root.path().join("does-not-exist");

        assert_eq!(
            PhysicalRootAnchor::observe(&missing),
            None,
            "a nonexistent path must observe to None, not error or panic"
        );
    }

    /// A plain FILE at the observed path yields `Some`: the kernel identity
    /// (volume serial + file index) is well defined for files, and the unix
    /// branch already returns `Some` for a file via `fs::metadata` dev/ino.
    /// Decided for cross-platform consistency: only an ABSENT object is
    /// `None`; `observe` merely happens to be called on directory roots.
    #[test]
    fn file_yields_some_stable_anchor() {
        let root = temp_root();
        let f = root.path().join("file.txt");
        fs::write(&f, b"payload").unwrap();

        let first = PhysicalRootAnchor::observe(&f).expect("file must observe to Some");
        let second = PhysicalRootAnchor::observe(&f).expect("re-observation of file");

        assert_eq!(
            first, second,
            "file identity must be stable across observations"
        );
    }
}
