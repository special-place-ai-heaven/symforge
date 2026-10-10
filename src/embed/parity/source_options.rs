//! Trusted, Rust-only selection of one embedded source's local state.
//! These options are never decoded from a room request.

use std::path::{Path, PathBuf};

use crate::domain::{
    ProjectStateDir, RootBinding, SourceAccessMode, StatePlacement, UserLocalPlacementReason,
};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum EmbeddedStateSelection {
    #[default]
    Automatic,
    ProjectLocal,
    /// Existing, host-protected base. Each source receives `projects/<root_id>`.
    UserLocal {
        base_directory: PathBuf,
    },
    MemoryOnly,
}

/// MCP `index_folder` parity for an embedded open. `add` has no option:
/// additive multi-source opens are explicit federation
/// (`federation::query_sources`) over separately opened sources.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EmbeddedOpenOptions {
    pub state: EmbeddedStateSelection,
    /// MCP `allow_protected_root`: direct authority to index the exact
    /// protected root in read/index-only mode. Never inherited.
    pub allow_protected_root: bool,
    /// MCP `index_folder` reset (`SYMFORGE_INDEX_FOLDER_RESET=1`): delete the
    /// persisted snapshot scope before opening so the source loads fresh.
    /// The outcome is reported by `EmbeddedSourceHandle::open_reset_receipt`.
    pub reset_snapshot_state: bool,
    /// Existing, absolute, host-protected directory for the state MCP keeps
    /// in process control state, beneath its `embed-host` child: `index_folder`
    /// idempotency records, and the edit-safety project-config trust store
    /// (`embed-host/edit-safety/trust.json`) whose verdict the edit answers
    /// report. `None` refuses idempotency keys as persistence-unavailable, as
    /// MCP does, and reports the trust store unavailable, as MCP does when it
    /// has no user-local control directory.
    pub replay_control_directory: Option<PathBuf>,
    /// Whether an edit refuses when the project's `.symforge` config is
    /// untrusted. The host chooses; the library never reads
    /// `SYMFORGE_PROJECT_CONFIG_TRUST_MODE` from the process environment.
    pub project_config_trust_mode: ProjectConfigTrustMode,
}

/// MCP's project-config trust mode, chosen by the embedding host.
/// `LogOnly` (the default, as in MCP) applies the edit and carries the
/// warning suffix; `Enforce` refuses the edit before any write.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ProjectConfigTrustMode {
    #[default]
    LogOnly,
    Enforce,
}

/// Outcome of an `index_folder`-style snapshot reset: the shared
/// `persist::reset_snapshot_state` scope and its observed file counts.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SnapshotResetReceipt {
    pub scope: String,
    pub removed: u64,
    pub missing: u64,
}

impl SnapshotResetReceipt {
    pub(crate) fn from_report(report: &crate::live_index::persist::SnapshotResetReport) -> Self {
        Self {
            scope: crate::live_index::persist::SNAPSHOT_RESET_SCOPE_LABEL.to_owned(),
            removed: report.removed_count() as u64,
            missing: report.missing_count() as u64,
        }
    }
}

/// Host-only bounds and placement for disposable Git read artifacts.
///
/// The directory must already exist, be protected by the host, and be disjoint
/// from source roots. MemoryOnly sources may use it without creating durable
/// SymForge state. This type is deliberately not deserializable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitPreparationOptions {
    pub(crate) workspace: PathBuf,
    pub(crate) max_bytes: u64,
    pub(crate) max_files: u64,
    pub(crate) metadata_directories: Vec<PathBuf>,
}

impl GitPreparationOptions {
    pub fn new(workspace: PathBuf, max_bytes: u64, max_files: u64) -> Self {
        Self {
            workspace,
            max_bytes,
            max_files,
            metadata_directories: Vec::new(),
        }
    }

    /// Explicitly admit a Git/common/include directory outside the source root.
    /// The preparation checks and retains its physical identity before use.
    pub fn with_metadata_directory(mut self, directory: PathBuf) -> Self {
        self.metadata_directories.push(directory);
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub enum GitPreparationRefusalKind {
    InvalidOptions,
    SourceUnavailable,
    NotRepository,
    MetadataNotAdmitted,
    UnsafeMetadata,
    InvalidRepository,
    CapacityExceeded,
    Cancelled,
    DeadlineExceeded,
    StateUnavailable,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct GitPreparationRefusal {
    pub(crate) kind: GitPreparationRefusalKind,
}

impl GitPreparationRefusal {
    pub fn kind(&self) -> GitPreparationRefusalKind {
        self.kind
    }
    pub fn kind_name(&self) -> &'static str {
        match self.kind {
            GitPreparationRefusalKind::InvalidOptions => "InvalidOptions",
            GitPreparationRefusalKind::SourceUnavailable => "SourceUnavailable",
            GitPreparationRefusalKind::NotRepository => "NotRepository",
            GitPreparationRefusalKind::MetadataNotAdmitted => "MetadataNotAdmitted",
            GitPreparationRefusalKind::UnsafeMetadata => "UnsafeMetadata",
            GitPreparationRefusalKind::InvalidRepository => "InvalidRepository",
            GitPreparationRefusalKind::CapacityExceeded => "CapacityExceeded",
            GitPreparationRefusalKind::Cancelled => "Cancelled",
            GitPreparationRefusalKind::DeadlineExceeded => "DeadlineExceeded",
            GitPreparationRefusalKind::StateUnavailable => "StateUnavailable",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct GitPreparationClaim {
    pub(crate) view_identity: String,
    pub(crate) serving_publication_identity: String,
    pub(crate) canonical_argument_hash: String,
    pub(crate) copied_bytes: u64,
    pub(crate) copied_files: u64,
    pub(crate) reused_files: u64,
}

impl GitPreparationClaim {
    pub fn view_identity(&self) -> &str {
        &self.view_identity
    }
    pub fn serving_publication_identity(&self) -> &str {
        &self.serving_publication_identity
    }
    pub fn canonical_argument_hash(&self) -> &str {
        &self.canonical_argument_hash
    }
    pub fn copied_bytes(&self) -> u64 {
        self.copied_bytes
    }
    pub fn copied_files(&self) -> u64 {
        self.copied_files
    }
    pub fn reused_files(&self) -> u64 {
        self.reused_files
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StateSelectionError {
    InvalidSelection,
    Unavailable,
}

pub(crate) fn select_state_placement(
    binding: &RootBinding,
    options: &EmbeddedOpenOptions,
) -> Result<StatePlacement, StateSelectionError> {
    match &options.state {
        EmbeddedStateSelection::Automatic => Ok(crate::discovery::resolve_state_placement(binding)),
        EmbeddedStateSelection::MemoryOnly => Ok(StatePlacement::MemoryOnly {
            failures: Vec::new(),
        }),
        EmbeddedStateSelection::ProjectLocal => {
            if binding.access_mode == SourceAccessMode::ExplicitProtected {
                return Err(StateSelectionError::InvalidSelection);
            }
            let directory = binding.canonical_root.join(".symforge");
            prepare_child(&directory)?;
            Ok(StatePlacement::ProjectLocal {
                directory: ProjectStateDir::new(directory),
            })
        }
        EmbeddedStateSelection::UserLocal { base_directory } => {
            if !base_directory.is_absolute() || !base_directory.is_dir() {
                return Err(StateSelectionError::InvalidSelection);
            }
            let mut components: Vec<_> = base_directory.ancestors().collect();
            components.reverse();
            for component in components {
                let metadata = std::fs::symlink_metadata(component)
                    .map_err(|_| StateSelectionError::InvalidSelection)?;
                if crate::paths::state_directory_metadata_is_unsafe(&metadata) {
                    return Err(StateSelectionError::InvalidSelection);
                }
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mode = std::fs::metadata(base_directory)
                    .map_err(|_| StateSelectionError::InvalidSelection)?
                    .permissions()
                    .mode();
                if mode & 0o077 != 0 {
                    return Err(StateSelectionError::InvalidSelection);
                }
            }
            let projects = base_directory.join("projects");
            prepare_child(&projects)?;
            let directory = projects.join(&binding.root_id.0);
            prepare_child(&directory)?;
            Ok(StatePlacement::UserLocal {
                directory: ProjectStateDir::new(directory),
                root_id: binding.root_id.clone(),
                reason: if binding.access_mode == SourceAccessMode::ExplicitProtected {
                    UserLocalPlacementReason::ExplicitProtected
                } else {
                    UserLocalPlacementReason::HostSelected
                },
            })
        }
    }
}

fn prepare_child(path: &Path) -> Result<(), StateSelectionError> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) => {
            if crate::paths::state_directory_metadata_is_unsafe(&metadata) {
                return Err(StateSelectionError::InvalidSelection);
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if metadata.permissions().mode() & 0o077 != 0 {
                    return Err(StateSelectionError::InvalidSelection);
                }
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            #[cfg(unix)]
            let created = {
                use std::os::unix::fs::DirBuilderExt;
                std::fs::DirBuilder::new().mode(0o700).create(path)
            };
            #[cfg(not(unix))]
            let created = std::fs::create_dir(path);
            if let Err(error) = created
                && error.kind() != std::io::ErrorKind::AlreadyExists
            {
                return Err(StateSelectionError::Unavailable);
            }
            let metadata =
                std::fs::symlink_metadata(path).map_err(|_| StateSelectionError::Unavailable)?;
            if crate::paths::state_directory_metadata_is_unsafe(&metadata) {
                return Err(StateSelectionError::InvalidSelection);
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if metadata.permissions().mode() & 0o077 != 0 {
                    return Err(StateSelectionError::InvalidSelection);
                }
            }
        }
        Err(_) => return Err(StateSelectionError::Unavailable),
    }
    Ok(())
}
