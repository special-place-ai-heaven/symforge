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

#[cfg(unix)]
fn sync_directory_beneath_capability(dir: &Dir) -> std::io::Result<()> {
    // cap-std may retain its directory capability as O_PATH; fsync on that
    // descriptor fails with EBADF. Open a readable directory descriptor through
    // the same capability, so this never resolves the mutable root spelling.
    let mut options = cap_std::fs::OpenOptions::new();
    options.read(true);
    dir.open_with(".", &options)?.sync_all()
}

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
    /// Stable bytes for a durable project binding. The two fields come from the
    /// opened directory object, never from its mutable path spelling.
    pub(crate) fn stable_key(self) -> [u8; 16] {
        let mut key = [0_u8; 16];
        key[..8].copy_from_slice(&self.dev.to_le_bytes());
        key[8..].copy_from_slice(&self.ino.to_le_bytes());
        key
    }

    /// Observe the directory object currently installed at `path`.
    pub fn observe(path: &Path) -> Option<Self> {
        observe_physical_root_anchor(path)
    }

    /// Identify the directory object already opened by the capability. A
    /// second path lookup here would accept a replacement at the same spelling.
    fn observe_opened(dir: &Dir) -> Option<Self> {
        observe_opened_physical_root_anchor(dir)
    }
}

#[cfg(unix)]
fn observe_opened_physical_root_anchor(dir: &Dir) -> Option<PhysicalRootAnchor> {
    use cap_std::fs::MetadataExt as _;

    let metadata = dir.dir_metadata().ok()?;
    Some(PhysicalRootAnchor {
        dev: metadata.dev(),
        ino: metadata.ino(),
    })
}

#[cfg(windows)]
#[allow(unsafe_code)]
fn observe_opened_physical_root_anchor(dir: &Dir) -> Option<PhysicalRootAnchor> {
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle,
    };

    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    // SAFETY: the Dir owns this live handle, and `info` is the API's correctly
    // sized output buffer. The Dir remains alive through the call.
    unsafe { GetFileInformationByHandle(HANDLE(dir.as_raw_handle()), &mut info) }.ok()?;
    Some(PhysicalRootAnchor {
        dev: u64::from(info.dwVolumeSerialNumber),
        ino: (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow),
    })
}

#[cfg(not(any(unix, windows)))]
fn observe_opened_physical_root_anchor(_dir: &Dir) -> Option<PhysicalRootAnchor> {
    None
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
    /// Physical identity of the directory handle this lease actually opened.
    pub(crate) fn opened_stable_key(&self) -> Option<[u8; 16]> {
        PhysicalRootAnchor::observe_opened(self.dir.as_ref()?).map(PhysicalRootAnchor::stable_key)
    }

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

    /// Read-only lease that refuses a root whose opened directory object is
    /// different from the authority's admitted object.
    pub(crate) fn take_matching_anchor(
        root: &Path,
        expected: Option<PhysicalRootAnchor>,
    ) -> Result<Self, RootRefusal> {
        let lease = Self::take(root);
        let dir = lease.capability()?;
        if expected.is_none() || PhysicalRootAnchor::observe_opened(dir) != expected {
            return Err(RootRefusal::LeaseRevoked);
        }
        Ok(lease)
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

    /// Reopen a parked state directory only when it is still the admitted
    /// physical directory. The returned capability stays open through one
    /// complete state operation; path spelling is not used for its effects.
    pub(crate) fn reopened_matching_stable_key(
        &self,
        expected: [u8; 16],
    ) -> Result<Self, RootRefusal> {
        let opened = self.reopened();
        if opened.opened_stable_key() != Some(expected) {
            return Err(RootRefusal::LeaseRevoked);
        }
        Ok(opened)
    }

    /// An owned second handle to this lease's own opened directory: same
    /// identity, same revocation, and the same directory object, with no path
    /// re-resolution in between.
    #[cfg(feature = "embed")]
    pub(crate) fn duplicated(&self) -> Result<Self, RootRefusal> {
        let dir = self
            .capability()?
            .try_clone()
            .map_err(|error| RootRefusal::Unreadable {
                path: self.root.clone(),
                message: error.to_string(),
            })?;
        Ok(Self {
            identity: self.identity,
            root: self.root.clone(),
            dir: Some(dir),
            revoked: Arc::clone(&self.revoked),
        })
    }

    /// Open a child directory through this lease, sharing its revocation.
    pub(crate) fn child_directory(
        &self,
        relative: &Path,
        create: bool,
    ) -> Result<Self, RootRefusal> {
        let target = self.resolve_beneath(relative)?;
        self.refuse_link_relative(target.relative())?;
        let parent = self.capability()?;
        if create {
            parent
                .create_dir_all(target.relative())
                .map_err(|error| RootRefusal::Unreadable {
                    path: target.path(),
                    message: error.to_string(),
                })?;
        }
        self.refuse_link_relative(target.relative())?;
        let dir = parent
            .open_dir(target.relative())
            .map_err(|error| RootRefusal::Unreadable {
                path: target.path(),
                message: error.to_string(),
            })?;
        Ok(Self {
            identity: self.identity,
            root: target.path(),
            dir: Some(dir),
            revoked: Arc::clone(&self.revoked),
        })
    }

    pub(crate) fn directory_capability(&self) -> Result<&Dir, RootRefusal> {
        self.capability()
    }

    #[cfg(unix)]
    pub(crate) fn sync_directory(&self) -> Result<(), RootRefusal> {
        sync_directory_beneath_capability(self.capability()?).map_err(|error| {
            RootRefusal::Unreadable {
                path: self.root.clone(),
                message: error.to_string(),
            }
        })
    }

    #[cfg(windows)]
    pub(crate) fn sync_directory(&self) -> Result<(), RootRefusal> {
        self.capability().map(|_| ())
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

pub(crate) fn replace_owner_only_beneath(
    lease: &PhysicalRootLease,
    relative: &Path,
    contents: &[u8],
) -> Result<WriteReceipt, RootRefusal> {
    stage_replacement_with_policy(lease, relative, contents, true)?.commit()
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

/// Verify a staged preimage through the pinned root, including a target that
/// did not exist when the batch was planned. Missing parents count as absent;
/// a link at any existing component is refused before the read.
pub(crate) fn matches_optional_image_beneath(
    lease: &PhysicalRootLease,
    relative: &Path,
    expected: Option<&[u8]>,
) -> Result<bool, RootRefusal> {
    let target = lease.resolve_beneath(relative)?;
    lease.refuse_link_relative(target.relative())?;
    let dir = lease.capability()?;
    match dir.read(target.relative()) {
        Ok(observed) => Ok(expected.is_some_and(|bytes| observed == bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(expected.is_none()),
        Err(error) => Err(RootRefusal::Unreadable {
            path: target.path(),
            message: error.to_string(),
        }),
    }
}

/// Read a bounded regular file through the same path confinement as writes.
/// `None` means absent; links, special files, and oversized files refuse.
pub(crate) fn open_regular_beneath(
    lease: &PhysicalRootLease,
    relative: &Path,
) -> Result<Option<(cap_std::fs::File, PathBuf)>, RootRefusal> {
    let target = lease.resolve_beneath(relative)?;
    lease.refuse_link_relative(target.relative())?;
    let dir = lease.capability()?;
    let metadata = match dir.symlink_metadata(target.relative()) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(RootRefusal::Unreadable {
                path: target.path(),
                message: error.to_string(),
            });
        }
    };
    if !metadata.is_file() {
        return Err(RootRefusal::Unreadable {
            path: target.path(),
            message: "not a regular file".to_string(),
        });
    }
    let mut options = cap_std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use cap_std::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW);
    }
    let file = match dir.open_with(target.relative(), &options) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(RootRefusal::Unreadable {
                path: target.path(),
                message: error.to_string(),
            });
        }
    };
    if !file.metadata().is_ok_and(|metadata| metadata.is_file()) {
        return Err(RootRefusal::Unreadable {
            path: target.path(),
            message: "not a regular file".to_string(),
        });
    }
    Ok(Some((file, target.path())))
}

/// Nofollow metadata for a final entry, including a directory or special file.
/// A missing entry is distinct from an unsafe traversal or an unreadable entry.
#[cfg(any(feature = "server", feature = "embed"))]
pub(crate) struct AnchoredEntryMetadata {
    pub len: u64,
    pub modified_secs: u64,
}

#[cfg(any(feature = "server", feature = "embed"))]
pub(crate) fn entry_metadata_beneath(
    lease: &PhysicalRootLease,
    relative: &Path,
) -> Result<Option<AnchoredEntryMetadata>, RootRefusal> {
    use std::time::UNIX_EPOCH;

    let target = lease.resolve_beneath(relative)?;
    lease.refuse_link_relative(target.relative())?;
    let metadata = match lease.capability()?.symlink_metadata(target.relative()) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(RootRefusal::Unreadable {
                path: target.path(),
                message: error.to_string(),
            });
        }
    };
    if metadata.is_symlink() {
        return Err(RootRefusal::LinkComponent {
            component: target.path(),
        });
    }
    let modified = metadata
        .modified()
        .map_err(|error| RootRefusal::Unreadable {
            path: target.path(),
            message: error.to_string(),
        })?;
    Ok(Some(AnchoredEntryMetadata {
        len: metadata.len(),
        modified_secs: modified
            .into_std()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    }))
}

/// Confirm each component uses the exact name returned by the admitted
/// directory handle. On case-insensitive filesystems a non-exact alias may
/// otherwise open bytes that the index admitted under a different name.
#[cfg(any(feature = "server", feature = "embed"))]
pub(crate) fn exact_spelling_beneath(
    lease: &PhysicalRootLease,
    relative: &Path,
) -> Result<bool, RootRefusal> {
    let target = lease.resolve_beneath(relative)?;
    exact_spelling_from_dir(
        lease.capability()?,
        target.relative().components(),
        &target.path(),
    )
}

#[cfg(any(feature = "server", feature = "embed"))]
fn exact_spelling_from_dir(
    directory: &Dir,
    mut components: std::path::Components<'_>,
    target: &Path,
) -> Result<bool, RootRefusal> {
    let Some(Component::Normal(name)) = components.next() else {
        return Err(RootRefusal::EscapesRoot {
            requested: target.to_path_buf(),
        });
    };
    let mut exact = false;
    for entry in directory
        .read_dir(".")
        .map_err(|error| RootRefusal::Unreadable {
            path: target.to_path_buf(),
            message: error.to_string(),
        })?
    {
        if entry
            .map_err(|error| RootRefusal::Unreadable {
                path: target.to_path_buf(),
                message: error.to_string(),
            })?
            .file_name()
            .as_os_str()
            == name
        {
            exact = true;
            break;
        }
    }
    if !exact {
        return match directory.symlink_metadata(name) {
            Ok(_) => Err(RootRefusal::Unreadable {
                path: target.to_path_buf(),
                message: "path spelling differs from the on-disk name".to_owned(),
            }),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(RootRefusal::Unreadable {
                path: target.to_path_buf(),
                message: error.to_string(),
            }),
        };
    }
    if components.clone().next().is_none() {
        return Ok(true);
    }
    let metadata = directory
        .symlink_metadata(name)
        .map_err(|error| RootRefusal::Unreadable {
            path: target.to_path_buf(),
            message: error.to_string(),
        })?;
    if metadata.is_symlink() {
        return Err(RootRefusal::LinkComponent {
            component: target.to_path_buf(),
        });
    }
    if !metadata.is_dir() {
        return Err(RootRefusal::Unreadable {
            path: target.to_path_buf(),
            message: "path component is not a directory".to_owned(),
        });
    }
    let child = directory
        .open_dir(name)
        .map_err(|error| RootRefusal::Unreadable {
            path: target.to_path_buf(),
            message: error.to_string(),
        })?;
    exact_spelling_from_dir(&child, components, target)
}

/// A stable observation from one no-follow regular-file handle. No file bytes
/// are retained when `bytes` is `None`.
#[cfg(any(feature = "server", feature = "embed"))]
pub(crate) struct AnchoredRegularObservation {
    pub metadata: std::fs::Metadata,
    pub len: u64,
    pub bytes: Option<Vec<u8>>,
}

/// Observe metadata, and optionally bounded bytes, from the same admitted
/// file handle. A disappeared file is distinct from an unsafe/unreadable one.
#[cfg(any(feature = "server", feature = "embed"))]
pub(crate) fn observe_regular_beneath(
    lease: &PhysicalRootLease,
    relative: &Path,
    read_limit: Option<usize>,
) -> Result<Option<AnchoredRegularObservation>, RootRefusal> {
    use std::io::Read;

    let Some((file, path)) = open_regular_beneath(lease, relative)? else {
        return Ok(None);
    };
    let mut file = file.into_std();
    let before = file.metadata().map_err(|error| RootRefusal::Unreadable {
        path: path.clone(),
        message: error.to_string(),
    })?;
    let modified = before.modified().map_err(|error| RootRefusal::Unreadable {
        path: path.clone(),
        message: error.to_string(),
    })?;
    let bytes = if let Some(max_bytes) = read_limit {
        let limit = max_bytes
            .checked_add(1)
            .ok_or_else(|| RootRefusal::Unreadable {
                path: path.clone(),
                message: "bounded read limit overflow".to_owned(),
            })?;
        if before.len() > max_bytes as u64 {
            return Err(RootRefusal::Unreadable {
                path,
                message: "regular file exceeds bounded read limit".to_owned(),
            });
        }
        let mut bytes = Vec::new();
        Read::by_ref(&mut file)
            .take(limit as u64)
            .read_to_end(&mut bytes)
            .map_err(|error| RootRefusal::Unreadable {
                path: path.clone(),
                message: error.to_string(),
            })?;
        if bytes.len() > max_bytes {
            return Err(RootRefusal::Unreadable {
                path,
                message: "regular file changed during bounded read".to_owned(),
            });
        }
        Some(bytes)
    } else {
        None
    };
    let after = file.metadata().map_err(|error| RootRefusal::Unreadable {
        path: path.clone(),
        message: error.to_string(),
    })?;
    if before.len() != after.len() || after.modified().ok() != Some(modified) {
        return Err(RootRefusal::Unreadable {
            path,
            message: "regular file changed during observation".to_owned(),
        });
    }
    Ok(Some(AnchoredRegularObservation {
        metadata: before.clone(),
        len: before.len(),
        bytes,
    }))
}

/// Read only a bounded prefix from one no-follow file handle. A file larger
/// than the prefix remains eligible for metadata-first discovery probing.
#[cfg(any(feature = "server", feature = "embed"))]
pub(crate) fn probe_regular_beneath(
    lease: &PhysicalRootLease,
    relative: &Path,
    prefix_limit: usize,
) -> Result<Option<(std::fs::Metadata, Vec<u8>)>, RootRefusal> {
    use std::io::Read;

    if prefix_limit > 1_048_576 {
        return Err(RootRefusal::Unreadable {
            path: lease.root().join(relative),
            message: "source probe exceeds bounded prefix limit".to_owned(),
        });
    }
    let Some((file, path)) = open_regular_beneath(lease, relative)? else {
        return Ok(None);
    };
    let mut file = file.into_std();
    let before = file.metadata().map_err(|error| RootRefusal::Unreadable {
        path: path.clone(),
        message: error.to_string(),
    })?;
    let before_modified = before.modified().map_err(|error| RootRefusal::Unreadable {
        path: path.clone(),
        message: error.to_string(),
    })?;
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take(prefix_limit as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| RootRefusal::Unreadable {
            path: path.clone(),
            message: error.to_string(),
        })?;
    let after = file.metadata().map_err(|error| RootRefusal::Unreadable {
        path: path.clone(),
        message: error.to_string(),
    })?;
    let after_modified = after.modified().map_err(|error| RootRefusal::Unreadable {
        path: path.clone(),
        message: error.to_string(),
    })?;
    if before.len() != after.len() || before_modified != after_modified {
        return Err(RootRefusal::Unreadable {
            path,
            message: "regular file changed during prefix probe".to_owned(),
        });
    }
    Ok(Some((before, bytes)))
}

#[cfg(any(feature = "server", feature = "embed"))]
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum AnchoredEntryKind {
    Regular,
    Directory,
    Link,
    Other,
}

#[cfg(any(feature = "server", feature = "embed"))]
pub(crate) struct AnchoredDirectoryEntry {
    pub relative: PathBuf,
    pub kind: AnchoredEntryKind,
}

/// List one directory through the admitted root, bounded to one chunk. The
/// caller applies ignore policy and recurses with another short authority read.
#[cfg(any(feature = "server", feature = "embed"))]
pub(crate) fn list_directory_beneath(
    lease: &PhysicalRootLease,
    relative: &Path,
    max_entries: usize,
) -> Result<Option<Vec<AnchoredDirectoryEntry>>, RootRefusal> {
    let directory = if relative == Path::new(".") {
        lease.directory_capability()?
    } else {
        let root = lease.directory_capability()?;
        match root.symlink_metadata(relative) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Ok(metadata) if metadata.is_symlink() => {
                return Err(RootRefusal::LinkComponent {
                    component: lease.root().join(relative),
                });
            }
            Ok(metadata) if metadata.is_dir() => {}
            Ok(_) => {
                return Err(RootRefusal::Unreadable {
                    path: lease.root().join(relative),
                    message: "not a directory".to_owned(),
                });
            }
            Err(error) => {
                return Err(RootRefusal::Unreadable {
                    path: lease.root().join(relative),
                    message: error.to_string(),
                });
            }
        }
        // The owned child keeps the opened directory stable throughout this
        // chunk even if the path spelling changes concurrently.
        return list_opened_directory(
            &lease.child_directory(relative, false)?,
            relative,
            max_entries,
        )
        .map(Some);
    };
    list_entries_from_dir(directory, lease.root(), relative, max_entries).map(Some)
}

#[cfg(any(feature = "server", feature = "embed"))]
fn list_opened_directory(
    opened: &PhysicalRootLease,
    relative: &Path,
    max_entries: usize,
) -> Result<Vec<AnchoredDirectoryEntry>, RootRefusal> {
    list_entries_from_dir(
        opened.directory_capability()?,
        opened.root(),
        relative,
        max_entries,
    )
}

#[cfg(any(feature = "server", feature = "embed"))]
fn list_entries_from_dir(
    directory: &Dir,
    root: &Path,
    relative: &Path,
    max_entries: usize,
) -> Result<Vec<AnchoredDirectoryEntry>, RootRefusal> {
    let mut entries = Vec::new();
    for entry in directory
        .read_dir(".")
        .map_err(|error| RootRefusal::Unreadable {
            path: root.to_path_buf(),
            message: error.to_string(),
        })?
    {
        let entry = entry.map_err(|error| RootRefusal::Unreadable {
            path: root.to_path_buf(),
            message: error.to_string(),
        })?;
        let name = entry.file_name();
        let metadata =
            directory
                .symlink_metadata(&name)
                .map_err(|error| RootRefusal::Unreadable {
                    path: root.join(&name),
                    message: error.to_string(),
                })?;
        let kind = if metadata.is_symlink() {
            AnchoredEntryKind::Link
        } else if metadata.is_dir() {
            AnchoredEntryKind::Directory
        } else if metadata.is_file() {
            AnchoredEntryKind::Regular
        } else {
            AnchoredEntryKind::Other
        };
        entries.push(AnchoredDirectoryEntry {
            relative: if relative == Path::new(".") {
                PathBuf::from(name)
            } else {
                relative.join(name)
            },
            kind,
        });
        if entries.len() > max_entries {
            return Err(RootRefusal::Unreadable {
                path: root.to_path_buf(),
                message: "directory enumeration exceeds its bounded chunk".to_owned(),
            });
        }
    }
    entries.sort_by(|left, right| left.relative.cmp(&right.relative));
    Ok(entries)
}

pub(crate) fn read_regular_beneath(
    lease: &PhysicalRootLease,
    relative: &Path,
    max_bytes: usize,
) -> Result<Option<Vec<u8>>, RootRefusal> {
    use std::io::Read;

    let Some((file, path)) = open_regular_beneath(lease, relative)? else {
        return Ok(None);
    };
    let length = file
        .metadata()
        .map_err(|error| RootRefusal::Unreadable {
            path: path.clone(),
            message: error.to_string(),
        })?
        .len();
    if length > max_bytes as u64 {
        return Err(RootRefusal::Unreadable {
            path,
            message: "regular file exceeds byte limit".to_string(),
        });
    }
    let mut bytes = Vec::new();
    file.take((max_bytes as u64).saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|error| RootRefusal::Unreadable {
            path: path.clone(),
            message: error.to_string(),
        })?;
    if bytes.len() > max_bytes {
        return Err(RootRefusal::Unreadable {
            path,
            message: "regular file exceeds byte limit".to_string(),
        });
    }
    Ok(Some(bytes))
}

/// Hash an opened regular file without buffering its whole contents. Replay
/// postimage checks may cover files larger than the bounded content-read cap.
#[cfg(feature = "embed")]
pub(crate) fn digest_regular_beneath(
    lease: &PhysicalRootLease,
    relative: &Path,
) -> Result<Option<String>, RootRefusal> {
    use sha2::{Digest as _, Sha256};
    use std::fmt::Write as _;
    use std::io::Read as _;

    let Some((mut file, path)) = open_regular_beneath(lease, relative)? else {
        return Ok(None);
    };
    let expected_len = file
        .metadata()
        .map_err(|error| RootRefusal::Unreadable {
            path: path.clone(),
            message: error.to_string(),
        })?
        .len();
    let mut hasher = Sha256::new();
    let mut observed_len = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| RootRefusal::Unreadable {
                path: path.clone(),
                message: error.to_string(),
            })?;
        if count == 0 {
            break;
        }
        observed_len =
            observed_len
                .checked_add(count as u64)
                .ok_or_else(|| RootRefusal::Unreadable {
                    path: path.clone(),
                    message: "regular file length changed during hash".to_string(),
                })?;
        if observed_len > expected_len {
            return Err(RootRefusal::Unreadable {
                path,
                message: "regular file length changed during hash".to_string(),
            });
        }
        hasher.update(&buffer[..count]);
    }
    let final_len = file
        .metadata()
        .map_err(|error| RootRefusal::Unreadable {
            path: path.clone(),
            message: error.to_string(),
        })?
        .len();
    if observed_len != expected_len || final_len != expected_len {
        return Err(RootRefusal::Unreadable {
            path,
            message: "regular file length changed during hash".to_string(),
        });
    }
    let mut digest = String::with_capacity(64);
    for byte in hasher.finalize() {
        write!(digest, "{byte:02x}").expect("hex writes to String");
    }
    Ok(Some(digest))
}

/// Preview-only capability read. Apply callers use the permit's pinned lease.
pub(crate) fn read_regular_beneath_root(
    root: &Path,
    relative: &Path,
    max_bytes: usize,
) -> Result<Option<Vec<u8>>, RootRefusal> {
    let lease = PhysicalRootLease::take(root);
    read_regular_beneath(&lease, relative, max_bytes)
}

/// Read-only regular-file size through the same root capability and no-link
/// checks as a bounded source read. This opens the file but never reads bytes,
/// so aggregate estimates can account for files beyond the content scan cap.
#[cfg(all(test, feature = "embed"))]
pub(crate) fn regular_file_size_beneath_root(
    root: &Path,
    relative: &Path,
) -> Result<Option<u64>, RootRefusal> {
    let lease = PhysicalRootLease::take(root);
    regular_file_size_beneath(&lease, relative)
}

/// Read-only metadata from an already pinned root capability.
#[cfg(feature = "embed")]
pub(crate) fn regular_file_size_beneath(
    lease: &PhysicalRootLease,
    relative: &Path,
) -> Result<Option<u64>, RootRefusal> {
    let Some((file, path)) = open_regular_beneath(lease, relative)? else {
        return Ok(None);
    };
    let opened = file.metadata().map_err(|error| RootRefusal::Unreadable {
        path,
        message: error.to_string(),
    })?;
    Ok(Some(opened.len()))
}

/// Complete a post-rename recovery sync through the admitted root, including
/// the containing directory on Unix. This never reopens the root by spelling.
pub(crate) fn sync_root_file_beneath(
    lease: &PhysicalRootLease,
    relative: &Path,
) -> Result<(), RootRefusal> {
    let target = lease.resolve_beneath(relative)?;
    if target
        .relative()
        .parent()
        .is_some_and(|parent| !parent.as_os_str().is_empty())
    {
        return Err(RootRefusal::EscapesRoot {
            requested: relative.to_path_buf(),
        });
    }
    let Some((file, path)) = open_regular_beneath(lease, relative)? else {
        return Err(RootRefusal::Unreadable {
            path: target.path(),
            message: "committed source file is absent".to_owned(),
        });
    };
    file.sync_all().map_err(|error| RootRefusal::Unreadable {
        path,
        message: error.to_string(),
    })?;
    #[cfg(unix)]
    sync_directory_beneath_capability(lease.capability()?).map_err(|error| {
        RootRefusal::Unreadable {
            path: lease.root().to_path_buf(),
            message: error.to_string(),
        }
    })?;
    #[cfg(not(any(unix, windows)))]
    return Err(RootRefusal::Unreadable {
        path: lease.root().to_path_buf(),
        message: "parent durability unsupported".to_owned(),
    });
    Ok(())
}

#[cfg(unix)]
fn verify_owner_only(file: &cap_std::fs::File) -> std::io::Result<()> {
    use cap_std::fs::PermissionsExt;
    let mode = file.metadata()?.permissions().mode() & 0o777;
    if mode == 0o600 {
        Ok(())
    } else {
        Err(std::io::Error::other("owner-only mode was not preserved"))
    }
}

#[cfg(unix)]
fn protect_owner_only(file: &cap_std::fs::File) -> std::io::Result<()> {
    // OpenOptions::mode sets this at create_new, before any source bytes exist.
    verify_owner_only(file)
}

#[cfg(windows)]
#[allow(unsafe_code)]
mod owner_only_windows {
    use std::ffi::c_void;
    use std::io;
    use std::mem::size_of;
    use std::os::windows::io::AsRawHandle;

    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::Security::{
        ACCESS_ALLOWED_ACE, ACL, ACL_REVISION, ACL_SIZE_INFORMATION, AclSizeInformation,
        AddAccessAllowedAce, DACL_SECURITY_INFORMATION, EqualSid, GetAce, GetAclInformation,
        GetKernelObjectSecurity, GetLengthSid, GetSecurityDescriptorControl,
        GetSecurityDescriptorDacl, GetTokenInformation, InitializeAcl,
        InitializeSecurityDescriptor, PROTECTED_DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR,
        PSID, SE_DACL_PROTECTED, SECURITY_DESCRIPTOR, SetKernelObjectSecurity,
        SetSecurityDescriptorControl, SetSecurityDescriptorDacl, TOKEN_QUERY, TOKEN_USER,
        TokenUser,
    };
    use windows::Win32::Storage::FileSystem::FILE_ALL_ACCESS;
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    fn io_error(error: windows::core::Error) -> io::Error {
        io::Error::other(error)
    }

    fn current_user_sid() -> io::Result<(Vec<u64>, PSID)> {
        let mut token = HANDLE::default();
        // SAFETY: out handle is valid; it is closed after the bounded token query.
        unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) }
            .map_err(io_error)?;
        let result = (|| {
            let mut len = 0_u32;
            // SAFETY: a null buffer is the documented size probe.
            let _ = unsafe { GetTokenInformation(token, TokenUser, None, 0, &mut len) };
            if len < size_of::<TOKEN_USER>() as u32 || len > 64 * 1024 {
                return Err(io::Error::other("invalid current-user token size"));
            }
            let mut storage = vec![0_u64; (len as usize).div_ceil(8)];
            // SAFETY: storage is aligned and has at least the probed byte length.
            unsafe {
                GetTokenInformation(
                    token,
                    TokenUser,
                    Some(storage.as_mut_ptr().cast()),
                    len,
                    &mut len,
                )
            }
            .map_err(io_error)?;
            // SAFETY: the successful TokenUser query initialized TOKEN_USER in storage.
            let sid = unsafe { (*(storage.as_ptr().cast::<TOKEN_USER>())).User.Sid };
            if sid.0.is_null() {
                return Err(io::Error::other("current-user SID is absent"));
            }
            Ok((storage, sid))
        })();
        // SAFETY: token is owned here and never used after this close.
        let _ = unsafe { CloseHandle(token) };
        result
    }

    fn handle(file: &cap_std::fs::File) -> HANDLE {
        HANDLE(file.as_raw_handle())
    }

    pub(super) fn protect(file: &cap_std::fs::File) -> io::Result<()> {
        let (_token_storage, sid) = current_user_sid()?;
        // SAFETY: GetLengthSid receives a SID returned by a successful TokenUser query.
        let sid_len = unsafe { GetLengthSid(sid) as usize };
        if sid_len == 0 || sid_len > 64 * 1024 {
            return Err(io::Error::other("invalid current-user SID length"));
        }
        let acl_len =
            size_of::<ACL>() + size_of::<ACCESS_ALLOWED_ACE>() - size_of::<u32>() + sid_len;
        let mut acl_storage = vec![0_u64; acl_len.div_ceil(8)];
        let acl = acl_storage.as_mut_ptr().cast::<ACL>();
        let mut descriptor = SECURITY_DESCRIPTOR::default();
        let descriptor_ptr =
            PSECURITY_DESCRIPTOR((&mut descriptor as *mut SECURITY_DESCRIPTOR).cast());
        // SAFETY: all buffers are aligned, sufficiently sized, and live through SetKernelObjectSecurity.
        unsafe {
            InitializeAcl(acl, acl_len as u32, ACL_REVISION).map_err(io_error)?;
            AddAccessAllowedAce(acl, ACL_REVISION, FILE_ALL_ACCESS.0, sid).map_err(io_error)?;
            InitializeSecurityDescriptor(descriptor_ptr, 1).map_err(io_error)?;
            SetSecurityDescriptorDacl(descriptor_ptr, true, Some(acl), false).map_err(io_error)?;
            SetSecurityDescriptorControl(descriptor_ptr, SE_DACL_PROTECTED, SE_DACL_PROTECTED)
                .map_err(io_error)?;
            SetKernelObjectSecurity(
                handle(file),
                DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                descriptor_ptr,
            )
            .map_err(io_error)?;
        }
        verify(file)
    }

    pub(super) fn verify(file: &cap_std::fs::File) -> io::Result<()> {
        let (_token_storage, sid) = current_user_sid()?;
        let mut len = 0_u32;
        // SAFETY: the null descriptor is a documented size probe on a live handle.
        let _ = unsafe {
            GetKernelObjectSecurity(handle(file), DACL_SECURITY_INFORMATION.0, None, 0, &mut len)
        };
        if len < size_of::<SECURITY_DESCRIPTOR>() as u32 || len > 64 * 1024 {
            return Err(io::Error::other("invalid file security descriptor size"));
        }
        let mut storage = vec![0_u64; (len as usize).div_ceil(8)];
        let descriptor = PSECURITY_DESCRIPTOR(storage.as_mut_ptr().cast());
        // SAFETY: the descriptor buffer is aligned and sized from the preceding kernel query.
        unsafe {
            GetKernelObjectSecurity(
                handle(file),
                DACL_SECURITY_INFORMATION.0,
                Some(descriptor),
                len,
                &mut len,
            )
            .map_err(io_error)?;
        }
        let mut control = Default::default();
        let mut revision = 0_u32;
        let mut present = Default::default();
        let mut defaulted = Default::default();
        let mut acl = std::ptr::null_mut();
        // SAFETY: queried self-relative descriptor remains alive for both inspectors.
        unsafe {
            GetSecurityDescriptorControl(descriptor, &mut control, &mut revision)
                .map_err(io_error)?;
            GetSecurityDescriptorDacl(descriptor, &mut present, &mut acl, &mut defaulted)
                .map_err(io_error)?;
        }
        if control & SE_DACL_PROTECTED.0 == 0 || !present.as_bool() || acl.is_null() {
            return Err(io::Error::other("file DACL is not private"));
        }
        let mut info = ACL_SIZE_INFORMATION::default();
        // SAFETY: ACL pointer belongs to the queried descriptor and info has the documented size.
        unsafe {
            GetAclInformation(
                acl,
                (&mut info as *mut ACL_SIZE_INFORMATION).cast::<c_void>(),
                size_of::<ACL_SIZE_INFORMATION>() as u32,
                AclSizeInformation,
            )
            .map_err(io_error)?;
        }
        if info.AceCount != 1 {
            return Err(io::Error::other("file DACL has unexpected entries"));
        }
        let mut ace_ptr = std::ptr::null_mut();
        // SAFETY: one ACE was reported, and the ACL remains alive.
        unsafe { GetAce(acl, 0, &mut ace_ptr) }.map_err(io_error)?;
        // SAFETY: GetAce returns a valid ACCESS_ALLOWED_ACE when AceType is zero.
        let ace = unsafe { &*(ace_ptr.cast::<ACCESS_ALLOWED_ACE>()) };
        if ace.Header.AceType != 0 || ace.Mask != FILE_ALL_ACCESS.0 {
            return Err(io::Error::other("file DACL has unexpected rights"));
        }
        let ace_sid = PSID((&ace.SidStart as *const u32).cast_mut().cast());
        // SAFETY: both SIDs remain valid for this comparison.
        unsafe { EqualSid(ace_sid, sid) }.map_err(io_error)?;
        Ok(())
    }
}

#[cfg(windows)]
fn protect_owner_only(file: &cap_std::fs::File) -> std::io::Result<()> {
    owner_only_windows::protect(file)
}

#[cfg(windows)]
fn verify_owner_only(file: &cap_std::fs::File) -> std::io::Result<()> {
    owner_only_windows::verify(file)
}

#[cfg(not(any(unix, windows)))]
fn protect_owner_only(_file: &cap_std::fs::File) -> std::io::Result<()> {
    Err(std::io::Error::other(
        "owner-only publication is unavailable",
    ))
}

#[cfg(not(any(unix, windows)))]
fn verify_owner_only(_file: &cap_std::fs::File) -> std::io::Result<()> {
    Err(std::io::Error::other(
        "owner-only publication is unavailable",
    ))
}

/// The durability boundaries exposed by the curation policy writer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DurableReplaceStage {
    TempWritten,
    TempSynced,
    Replaced,
    ParentSynced,
}

pub(crate) enum DurableReplaceError<E> {
    Root(RootRefusal),
    Stage(E),
    ImageMismatch,
}

/// Durably replace one root-level source file through the permit's opened root.
/// The callback preserves the curation writer's ordered failpoints. It may
/// modify the still-private temporary after `TempWritten` for corruption tests.
pub(crate) fn durable_replace_root_file_beneath<E>(
    lease: &PhysicalRootLease,
    relative: &Path,
    contents: &[u8],
    prefix: &str,
    mut stage: impl FnMut(DurableReplaceStage, &mut cap_std::fs::File) -> Result<(), E>,
) -> Result<(), DurableReplaceError<E>> {
    use std::io::{Read as _, Seek as _, SeekFrom};

    let target = lease
        .resolve_beneath(relative)
        .map_err(DurableReplaceError::Root)?;
    if target
        .relative()
        .parent()
        .is_some_and(|parent| !parent.as_os_str().is_empty())
    {
        return Err(DurableReplaceError::Root(RootRefusal::EscapesRoot {
            requested: relative.to_path_buf(),
        }));
    }
    lease
        .refuse_link_relative(target.relative())
        .map_err(DurableReplaceError::Root)?;
    let dir = lease.capability().map_err(DurableReplaceError::Root)?;
    let unreadable = |path: PathBuf, error: std::io::Error| {
        DurableReplaceError::Root(RootRefusal::Unreadable {
            path,
            message: error.to_string(),
        })
    };

    struct TempCleanup<'a> {
        dir: &'a Dir,
        relative: PathBuf,
        armed: bool,
    }
    impl Drop for TempCleanup<'_> {
        fn drop(&mut self) {
            if self.armed {
                let _ = self.dir.remove_file(&self.relative);
            }
        }
    }

    let mut opened = None;
    for attempt in 0..MAX_TEMP_ATTEMPTS {
        let candidate = PathBuf::from(format!(
            "{prefix}{}-{}-{attempt}",
            std::process::id(),
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        let mut options = cap_std::fs::OpenOptions::new();
        options.read(true).write(true).create_new(true);
        #[cfg(windows)]
        {
            use cap_std::fs::OpenOptionsExt;
            use windows::Win32::Storage::FileSystem::{
                DELETE, FILE_FLAG_WRITE_THROUGH, FILE_GENERIC_READ, FILE_GENERIC_WRITE,
                FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
            };
            options.access_mode((FILE_GENERIC_READ | FILE_GENERIC_WRITE | DELETE).0);
            options.share_mode((FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE).0);
            options.custom_flags(FILE_FLAG_WRITE_THROUGH.0);
        }
        match dir.open_with(&candidate, &options) {
            Ok(file) => {
                opened = Some((candidate, file));
                break;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(unreadable(lease.root().join(candidate), error)),
        }
    }
    let (temp_relative, mut file) = opened.ok_or_else(|| {
        DurableReplaceError::Root(RootRefusal::Unreadable {
            path: lease.root().to_path_buf(),
            message: "no unused curation temporary name was available".to_owned(),
        })
    })?;
    let mut cleanup = TempCleanup {
        dir,
        relative: temp_relative.clone(),
        armed: true,
    };
    let temp_path = lease.root().join(&temp_relative);
    file.write_all(contents)
        .and_then(|()| file.flush())
        .map_err(|error| unreadable(temp_path.clone(), error))?;
    stage(DurableReplaceStage::TempWritten, &mut file).map_err(DurableReplaceError::Stage)?;
    file.seek(SeekFrom::Start(0))
        .map_err(|error| unreadable(temp_path.clone(), error))?;
    let mut observed = Vec::new();
    (&mut file)
        .take((contents.len() as u64).saturating_add(1))
        .read_to_end(&mut observed)
        .map_err(|error| unreadable(temp_path.clone(), error))?;
    if observed != contents {
        return Err(DurableReplaceError::ImageMismatch);
    }
    file.sync_all()
        .map_err(|error| unreadable(lease.root().join(&temp_relative), error))?;
    stage(DurableReplaceStage::TempSynced, &mut file).map_err(DurableReplaceError::Stage)?;
    lease
        .refuse_link_relative(target.relative())
        .map_err(DurableReplaceError::Root)?;
    if !lease.is_live() {
        return Err(DurableReplaceError::Root(RootRefusal::LeaseRevoked));
    }
    #[cfg(windows)]
    windows_write_through_replace_beneath(dir, &file, target.relative())
        .map_err(|error| unreadable(target.path(), error))?;
    #[cfg(not(windows))]
    dir.rename(&temp_relative, dir, target.relative())
        .map_err(|error| unreadable(target.path(), error))?;
    cleanup.armed = false;
    stage(DurableReplaceStage::Replaced, &mut file).map_err(DurableReplaceError::Stage)?;
    file.sync_all()
        .map_err(|error| unreadable(target.path(), error))?;
    #[cfg(unix)]
    sync_directory_beneath_capability(dir)
        .map_err(|error| unreadable(lease.root().to_path_buf(), error))?;
    #[cfg(not(any(unix, windows)))]
    return Err(DurableReplaceError::Root(RootRefusal::Unreadable {
        path: lease.root().to_path_buf(),
        message: "parent durability unsupported".to_owned(),
    }));
    stage(DurableReplaceStage::ParentSynced, &mut file).map_err(DurableReplaceError::Stage)
}

#[cfg(windows)]
#[allow(unsafe_code)]
fn windows_write_through_replace_beneath(
    dir: &Dir,
    file: &cap_std::fs::File,
    target: &Path,
) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::io::AsRawHandle;
    use windows::Wdk::Storage::FileSystem::{
        FILE_RENAME_INFORMATION, FileRenameInformation, NtSetInformationFile,
    };
    use windows::Win32::Foundation::{HANDLE, RtlNtStatusToDosError};
    use windows::Win32::System::IO::IO_STATUS_BLOCK;

    let name = target.as_os_str().encode_wide().collect::<Vec<_>>();
    let length = std::mem::offset_of!(FILE_RENAME_INFORMATION, FileName) + name.len() * 2;
    let mut storage = vec![0_u64; length.div_ceil(8)];
    let info = storage.as_mut_ptr().cast::<FILE_RENAME_INFORMATION>();
    // SAFETY: the aligned buffer has room for the header and every UTF-16 code
    // unit; both handles remain open through the rename call. `target` is a
    // single validated leaf relative to the retained root directory handle.
    unsafe {
        (*info).Anonymous.ReplaceIfExists = true;
        (*info).RootDirectory = HANDLE(dir.as_raw_handle());
        (*info).FileNameLength = (name.len() * 2) as u32;
        std::ptr::copy_nonoverlapping(name.as_ptr(), (*info).FileName.as_mut_ptr(), name.len());
        let mut io_status = IO_STATUS_BLOCK::default();
        let status = NtSetInformationFile(
            HANDLE(file.as_raw_handle()),
            &mut io_status,
            info.cast(),
            length as u32,
            FileRenameInformation,
        );
        if status.0 < 0 {
            Err(std::io::Error::from_raw_os_error(
                RtlNtStatusToDosError(status) as i32,
            ))
        } else {
            Ok(())
        }
    }
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
    stage_replacement_with_policy(lease, relative, contents, false)
}

fn stage_replacement_with_policy(
    lease: &PhysicalRootLease,
    relative: &Path,
    contents: &[u8],
    owner_only: bool,
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
        #[cfg(unix)]
        if owner_only {
            use cap_std::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        #[cfg(windows)]
        if owner_only {
            use cap_std::fs::OpenOptionsExt;
            use windows::Win32::Storage::FileSystem::{
                FILE_GENERIC_WRITE, READ_CONTROL, WRITE_DAC,
            };
            options.access_mode((FILE_GENERIC_WRITE | READ_CONTROL | WRITE_DAC).0);
            options.share_mode(0);
        }
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

    if owner_only && let Err(error) = protect_owner_only(&handle) {
        drop(handle);
        let _ = dir.remove_file(&temp_relative);
        return Err(RootRefusal::Unreadable {
            path: lease.root().join(&temp_relative),
            message: error.to_string(),
        });
    }

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
        owner_only,
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
    owner_only: bool,
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
        if self.owner_only {
            let verified = self
                .dir
                .open(&self.target_relative)
                .and_then(|file| verify_owner_only(&file));
            if let Err(error) = verified {
                return Err(RootRefusal::Unreadable {
                    path: self.target.clone(),
                    message: error.to_string(),
                });
            }
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

#[cfg(test)]
mod capability_read_tests {
    use super::*;

    #[test]
    fn bounded_read_distinguishes_absent_regular_and_invalid_targets() {
        let root = tempfile::tempdir().expect("temporary root");
        std::fs::write(root.path().join("present"), b"source bytes").expect("source");
        std::fs::create_dir(root.path().join("directory")).expect("directory");
        assert_eq!(
            read_regular_beneath_root(root.path(), Path::new("missing/nested"), 64)
                .expect("absent target"),
            None
        );
        assert_eq!(
            read_regular_beneath_root(root.path(), Path::new("present"), 64).expect("regular file"),
            Some(b"source bytes".to_vec())
        );
        assert!(read_regular_beneath_root(root.path(), Path::new("present"), 4).is_err());
        assert!(read_regular_beneath_root(root.path(), Path::new("directory"), 64).is_err());
        assert!(read_regular_beneath_root(root.path(), Path::new("../escape"), 64).is_err());
    }

    #[cfg(feature = "embed")]
    #[test]
    fn regular_size_observes_large_file_without_reading_its_content() {
        let root = tempfile::tempdir().expect("temporary root");
        let path = root.path().join("large");
        std::fs::File::create(&path)
            .expect("create sparse file")
            .set_len(4 * 1024 * 1024 + 1)
            .expect("size sparse file");
        assert_eq!(
            regular_file_size_beneath_root(root.path(), Path::new("large"))
                .expect("size regular file"),
            Some(4 * 1024 * 1024 + 1)
        );
        assert_eq!(
            regular_file_size_beneath_root(root.path(), Path::new("missing")).expect("absent file"),
            None
        );
        assert!(regular_file_size_beneath_root(root.path(), Path::new("../escape")).is_err());
    }

    #[cfg(feature = "embed")]
    #[test]
    fn anchored_prefix_probe_accepts_a_large_file_without_reading_past_limit() {
        let root = tempfile::tempdir().expect("temporary root");
        let path = root.path().join("large");
        std::fs::write(&path, vec![b'x'; 2 * 1024 * 1024]).expect("large source");
        let lease = PhysicalRootLease::take(root.path());

        let (metadata, prefix) = probe_regular_beneath(&lease, Path::new("large"), 1024)
            .expect("bounded prefix probe")
            .expect("regular file");
        assert_eq!(metadata.len(), 2 * 1024 * 1024);
        assert_eq!(prefix, vec![b'x'; 1024]);
        assert!(probe_regular_beneath(&lease, Path::new("../escape"), 1024).is_err());
    }

    #[cfg(feature = "embed")]
    #[test]
    fn anchored_directory_listing_preserves_relative_names_and_bounds() {
        let root = tempfile::tempdir().expect("temporary root");
        std::fs::create_dir(root.path().join("nested")).expect("nested directory");
        std::fs::write(root.path().join("nested/file.rs"), b"fn example() {}")
            .expect("nested source");
        let lease = PhysicalRootLease::take(root.path());

        let entries = list_directory_beneath(&lease, Path::new("."), 1)
            .expect("root listing")
            .expect("root directory");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].relative, Path::new("nested"));
        assert!(entries[0].kind == AnchoredEntryKind::Directory);

        let nested = list_directory_beneath(&lease, Path::new("nested"), 1)
            .expect("nested listing")
            .expect("nested directory");
        assert_eq!(nested[0].relative, Path::new("nested/file.rs"));
        assert!(nested[0].kind == AnchoredEntryKind::Regular);
        assert!(list_directory_beneath(&lease, Path::new("nested"), 0).is_err());
    }

    #[cfg(feature = "embed")]
    #[test]
    fn streamed_digest_covers_large_regular_file_and_refuses_invalid_targets() {
        let root = tempfile::tempdir().expect("temporary root");
        let bytes = vec![b'x'; 4 * 1024 * 1024 + 17];
        std::fs::write(root.path().join("large"), &bytes).expect("large source");
        std::fs::create_dir(root.path().join("directory")).expect("directory");
        let lease = PhysicalRootLease::take(root.path());
        assert_eq!(
            digest_regular_beneath(&lease, Path::new("large")).expect("stream digest"),
            Some(crate::hash::digest_hex(&bytes))
        );
        assert_eq!(
            digest_regular_beneath(&lease, Path::new("missing")).expect("absent target"),
            None
        );
        assert!(digest_regular_beneath(&lease, Path::new("directory")).is_err());
        assert!(digest_regular_beneath(&lease, Path::new("../escape")).is_err());
    }
}

#[cfg(all(test, any(unix, windows)))]
mod owner_only_tests {
    use super::*;

    #[test]
    fn staged_sensitive_image_is_private_before_publication_and_after_rename() {
        let root = tempfile::tempdir().expect("temporary root");
        let lease = PhysicalRootLease::take(root.path());
        let target = Path::new(".env");
        let stage = stage_replacement_with_policy(&lease, target, b"fixture", true)
            .expect("owner-only stage");
        assert!(!root.path().join(target).exists());
        let temp = stage.temp_path().to_path_buf();
        let staged_file = std::fs::File::open(&temp).expect("staged private image");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                staged_file.metadata().unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        #[cfg(windows)]
        {
            let opened = lease
                .capability()
                .unwrap()
                .open(stage.temp_relative.as_path())
                .unwrap();
            verify_owner_only(&opened).expect("staged protected DACL");
        }
        drop(staged_file);
        stage.commit().expect("published owner-only image");
        assert!(!temp.exists());
        assert_eq!(std::fs::read(root.path().join(target)).unwrap(), b"fixture");
        let published = lease.capability().unwrap().open(target).unwrap();
        verify_owner_only(&published).expect("published private image");
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
