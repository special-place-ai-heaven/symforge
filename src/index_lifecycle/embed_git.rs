//! Disposable, host-prepared Git views built from original-root capabilities.
//! libgit2 reads only this protected immutable artifact, never source spellings.

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use super::embed_query::EmbeddedQuerySnapshot;
use super::physical_root::{self, AnchoredEntryKind, PhysicalRootLease};
use crate::embed::parity::host::{OperationControl, OperationStop};
use crate::embed::parity::source_options::{
    GitPreparationClaim, GitPreparationOptions, GitPreparationRefusal,
    GitPreparationRefusalKind as Refusal,
};

pub(super) struct PreparedGitView {
    artifact: tempfile::TempDir,
    identity: String,
    argument_hash: String,
    source_binding: String,
    inputs: Vec<CapturedFile>,
    source_key: [u8; 16],
    copied_bytes: u64,
}

impl PreparedGitView {
    pub(super) fn repository(&self) -> Result<git2::Repository, GitPreparationRefusal> {
        // Opt-in flag supplied by the pinned local libgit2 patch. The embed
        // dependency feature must enforce that patched library at link time.
        const LOCAL_CONFIG_ONLY: u32 = 1 << 5;
        let repository = git2::Repository::open_ext(
            self.artifact.path().join("repo/.git"),
            git2::RepositoryOpenFlags::NO_SEARCH
                | git2::RepositoryOpenFlags::NO_DOTGIT
                | git2::RepositoryOpenFlags::from_bits_retain(LOCAL_CONFIG_ONLY),
            &[] as &[&std::ffi::OsStr],
        )
        .map_err(|_| refusal(Refusal::InvalidRepository))?;
        Ok(repository)
    }
}

struct CapturedFile {
    lease: Arc<PhysicalRootLease>,
    relative: PathBuf,
    destination: PathBuf,
    bytes: Vec<u8>,
    executable: bool,
}

struct Capture<'a> {
    options: &'a GitPreparationOptions,
    control: &'a OperationControl,
    grants: Vec<Arc<PhysicalRootLease>>,
    files: BTreeMap<PathBuf, CapturedFile>,
    bytes: u64,
    entries: u64,
}

fn refusal(kind: Refusal) -> GitPreparationRefusal {
    GitPreparationRefusal { kind }
}

fn check(control: &OperationControl) -> Result<(), GitPreparationRefusal> {
    control.check().map_err(|stop| {
        refusal(match stop {
            OperationStop::Cancelled => Refusal::Cancelled,
            OperationStop::DeadlineExceeded => Refusal::DeadlineExceeded,
        })
    })
}

fn validate_directory(path: &Path) -> Result<(), GitPreparationRefusal> {
    if !path.is_absolute() {
        return Err(refusal(Refusal::InvalidOptions));
    }
    for ancestor in path.ancestors() {
        let metadata =
            std::fs::symlink_metadata(ancestor).map_err(|_| refusal(Refusal::InvalidOptions))?;
        if !metadata.is_dir() || crate::paths::state_directory_metadata_is_unsafe(&metadata) {
            return Err(refusal(Refusal::InvalidOptions));
        }
    }
    Ok(())
}

fn normalize(path: &Path) -> Result<PathBuf, GitPreparationRefusal> {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !result.pop() {
                    return Err(refusal(Refusal::MetadataNotAdmitted));
                }
            }
            other => result.push(other.as_os_str()),
        }
    }
    if !result.is_absolute() {
        return Err(refusal(Refusal::MetadataNotAdmitted));
    }
    Ok(result)
}

impl Capture<'_> {
    fn charge_entry(&mut self) -> Result<(), GitPreparationRefusal> {
        check(self.control)?;
        self.entries = self
            .entries
            .checked_add(1)
            .ok_or_else(|| refusal(Refusal::CapacityExceeded))?;
        if self.entries > self.options.max_files {
            return Err(refusal(Refusal::CapacityExceeded));
        }
        Ok(())
    }

    fn read(
        &mut self,
        lease: &Arc<PhysicalRootLease>,
        relative: &Path,
        destination: &Path,
    ) -> Result<Option<Vec<u8>>, GitPreparationRefusal> {
        check(self.control)?;
        let remaining = self.options.max_bytes.saturating_sub(self.bytes);
        let metadata = physical_root::observe_regular_beneath(lease, relative, None)
            .map_err(|_| refusal(Refusal::UnsafeMetadata))?;
        let Some(metadata) = metadata else {
            return Ok(None);
        };
        if metadata.len > remaining {
            return Err(refusal(Refusal::CapacityExceeded));
        }
        let max = usize::try_from(remaining).map_err(|_| refusal(Refusal::InvalidOptions))?;
        let observed = physical_root::observe_regular_beneath(lease, relative, Some(max))
            .map_err(|_| refusal(Refusal::UnsafeMetadata))?
            .ok_or_else(|| refusal(Refusal::SourceUnavailable))?;
        let bytes = observed
            .bytes
            .ok_or_else(|| refusal(Refusal::UnsafeMetadata))?;
        #[cfg(unix)]
        let executable = {
            use std::os::unix::fs::PermissionsExt;
            observed.metadata.permissions().mode() & 0o111 != 0
        };
        #[cfg(not(unix))]
        let executable = false;
        self.charge_entry()?;
        self.bytes = self
            .bytes
            .checked_add(bytes.len() as u64)
            .ok_or_else(|| refusal(Refusal::CapacityExceeded))?;
        self.files.insert(
            destination.to_path_buf(),
            CapturedFile {
                lease: Arc::clone(lease),
                relative: relative.to_path_buf(),
                destination: destination.to_path_buf(),
                bytes: bytes.clone(),
                executable,
            },
        );
        Ok(Some(bytes))
    }

    fn tree(
        &mut self,
        lease: &Arc<PhysicalRootLease>,
        relative: &Path,
        destination: &Path,
        worktree: bool,
    ) -> Result<(), GitPreparationRefusal> {
        check(self.control)?;
        if relative.components().count() > 128 {
            return Err(refusal(Refusal::CapacityExceeded));
        }
        let remaining = self.options.max_files.saturating_sub(self.entries);
        let entries = physical_root::list_directory_beneath(
            lease,
            relative,
            usize::try_from(remaining).map_err(|_| refusal(Refusal::InvalidOptions))?,
        )
        .map_err(|_| refusal(Refusal::CapacityExceeded))?
        .ok_or_else(|| refusal(Refusal::SourceUnavailable))?;
        for entry in entries {
            let name = entry
                .relative
                .file_name()
                .ok_or_else(|| refusal(Refusal::UnsafeMetadata))?;
            if worktree && (name == ".git" || name == ".symforge") {
                continue;
            }
            let target = destination.join(name);
            match entry.kind {
                AnchoredEntryKind::Directory => {
                    self.charge_entry()?;
                    self.tree(lease, &entry.relative, &target, worktree)?;
                }
                AnchoredEntryKind::Regular => {
                    self.read(lease, &entry.relative, &target)?
                        .ok_or_else(|| refusal(Refusal::SourceUnavailable))?;
                }
                AnchoredEntryKind::Link | AnchoredEntryKind::Other => {
                    return Err(refusal(Refusal::UnsafeMetadata));
                }
            }
        }
        Ok(())
    }

    fn admitted_file(
        &mut self,
        absolute: &Path,
        destination: &Path,
    ) -> Result<Option<Vec<u8>>, GitPreparationRefusal> {
        let absolute = normalize(absolute)?;
        let (lease, relative) = self
            .grants
            .iter()
            .filter_map(|lease| {
                absolute
                    .strip_prefix(lease.root())
                    .ok()
                    .map(|relative| (Arc::clone(lease), relative.to_path_buf()))
            })
            .max_by_key(|(lease, _)| lease.root().components().count())
            .ok_or_else(|| refusal(Refusal::MetadataNotAdmitted))?;
        self.read(&lease, &relative, destination)
    }
}

pub(super) fn prepare(
    snapshot: &EmbeddedQuerySnapshot,
    options: &GitPreparationOptions,
    control: &OperationControl,
    previous: Option<&Arc<PreparedGitView>>,
) -> Result<(Arc<PreparedGitView>, GitPreparationClaim), GitPreparationRefusal> {
    check(control)?;
    if options.max_bytes == 0 || options.max_files == 0 || options.max_bytes >= usize::MAX as u64 {
        return Err(refusal(Refusal::InvalidOptions));
    }
    validate_directory(&options.workspace)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if std::fs::metadata(&options.workspace)
            .map_err(|_| refusal(Refusal::InvalidOptions))?
            .permissions()
            .mode()
            & 0o077
            != 0
        {
            return Err(refusal(Refusal::InvalidOptions));
        }
    }
    let workspace =
        std::fs::canonicalize(&options.workspace).map_err(|_| refusal(Refusal::InvalidOptions))?;
    if workspace.starts_with(&snapshot.root) || snapshot.root.starts_with(&workspace) {
        return Err(refusal(Refusal::InvalidOptions));
    }
    let workspace_lease = PhysicalRootLease::take(&workspace);
    let workspace_key = workspace_lease
        .opened_stable_key()
        .ok_or_else(|| refusal(Refusal::StateUnavailable))?;
    let source = Arc::new(
        snapshot
            .authority
            .with_source_anchor_read(|root| root.child_directory(Path::new("."), false))
            .map_err(|_| refusal(Refusal::SourceUnavailable))?,
    );
    let source_key = source
        .opened_stable_key()
        .ok_or_else(|| refusal(Refusal::SourceUnavailable))?;
    let mut grants = vec![Arc::clone(&source)];
    for path in &options.metadata_directories {
        validate_directory(path)?;
        let path = std::fs::canonicalize(path).map_err(|_| refusal(Refusal::InvalidOptions))?;
        if path.starts_with(&workspace) || workspace.starts_with(&path) {
            return Err(refusal(Refusal::InvalidOptions));
        }
        let lease = Arc::new(PhysicalRootLease::take(path));
        if lease.opened_stable_key().is_none() {
            return Err(refusal(Refusal::MetadataNotAdmitted));
        }
        grants.push(lease);
    }
    let git = Arc::new(
        source
            .child_directory(Path::new(".git"), false)
            .map_err(|_| refusal(Refusal::NotRepository))?,
    );
    grants.push(Arc::clone(&git));
    let mut capture = Capture {
        options,
        control,
        grants,
        files: BTreeMap::new(),
        bytes: 0,
        entries: 0,
    };
    capture.tree(&git, Path::new(""), Path::new("repo/.git"), false)?;
    capture.tree(&source, Path::new(""), Path::new("repo"), true)?;
    // Neither libgit2 nor the config parser sees source paths. A gitfile,
    // common-dir or object alternate needs a separately admitted graph before
    // it can be rewritten into this owned view.
    for path in [
        "repo/.git/commondir",
        "repo/.git/objects/info/alternates",
        "repo/.git/config.worktree",
    ] {
        if capture.files.contains_key(Path::new(path)) {
            return Err(refusal(Refusal::MetadataNotAdmitted));
        }
    }
    let config_path = Path::new("repo/.git/config");
    let config_bytes = capture
        .files
        .get(config_path)
        .map(|row| row.bytes.clone())
        .ok_or_else(|| refusal(Refusal::InvalidRepository))?;
    let entries = super::embed_git_config::parse(&config_bytes, options.max_files as usize)
        .map_err(|error| {
            refusal(match error {
                super::embed_git_config::ConfigRefusal::Invalid => Refusal::InvalidRepository,
                super::embed_git_config::ConfigRefusal::CapacityExceeded => {
                    Refusal::CapacityExceeded
                }
            })
        })?;
    let mut config_entries = Vec::new();
    for entry in entries {
        if entry.name == "include.path"
            || (entry.name.starts_with("includeif.") && entry.name.ends_with(".path"))
        {
            return Err(refusal(Refusal::MetadataNotAdmitted));
        }
        if matches!(
            entry.name.as_str(),
            "core.attributesfile" | "core.excludesfile"
        ) {
            let value = entry
                .value
                .as_deref()
                .ok_or_else(|| refusal(Refusal::InvalidRepository))?;
            if value.starts_with('~') {
                return Err(refusal(Refusal::MetadataNotAdmitted));
            }
            let absolute = if Path::new(value).is_absolute() {
                PathBuf::from(value)
            } else {
                git.root().join(value)
            };
            let target = PathBuf::from(format!("aux/{}", config_entries.len()));
            if capture.admitted_file(&absolute, &target)?.is_some() {
                config_entries.push((entry.name, Some(target), None));
            }
        } else if !matches!(entry.name.as_str(), "core.worktree" | "core.bare") {
            config_entries.push((entry.name, None, entry.value));
        }
    }
    check(control)?;
    let argument_bytes = serde_json::to_vec(&(
        "symforge.embed.git-preparation.v1",
        &snapshot.binding_identity,
        &workspace,
        workspace_key,
        options.max_bytes,
        options.max_files,
        &options.metadata_directories,
    ))
    .expect("Git preparation arguments serialize");
    let argument_hash = crate::hash::digest_hex(&argument_bytes);
    let fingerprint: Vec<_> = capture
        .files
        .values()
        .map(|row| {
            (
                row.lease.opened_stable_key(),
                &row.relative,
                &row.destination,
                crate::hash::digest_hex(&row.bytes),
                row.executable,
            )
        })
        .collect();
    let identity = crate::hash::digest_hex(
        &serde_json::to_vec(&(
            "symforge.embed.git-view.v1",
            &snapshot.binding_identity,
            source_key,
            &argument_hash,
            fingerprint,
        ))
        .expect("Git view manifest serializes"),
    );
    if let Some(previous) = previous.filter(|previous| {
        previous.identity == identity
            && previous.argument_hash == argument_hash
            && previous.source_binding == snapshot.binding_identity
            && previous.source_key == source_key
            && previous.copied_bytes <= options.max_bytes
            && previous.inputs.len() == capture.files.len()
            && previous.artifact.path().join("repo/.git").is_dir()
    }) {
        let claim = GitPreparationClaim {
            view_identity: identity,
            serving_publication_identity: snapshot.serving_publication_identity.clone(),
            canonical_argument_hash: argument_hash,
            copied_bytes: 0,
            copied_files: 0,
            reused_files: capture.files.len() as u64,
        };
        return Ok((Arc::clone(previous), claim));
    }
    let artifact = tempfile::Builder::new()
        .prefix("symforge-git-")
        .tempdir_in(&workspace)
        .map_err(|_| refusal(Refusal::StateUnavailable))?;
    for row in capture.files.values() {
        check(control)?;
        if row.destination == config_path {
            continue;
        }
        let destination = artifact.path().join(&row.destination);
        std::fs::create_dir_all(
            destination
                .parent()
                .ok_or_else(|| refusal(Refusal::StateUnavailable))?,
        )
        .map_err(|_| refusal(Refusal::StateUnavailable))?;
        std::fs::write(&destination, &row.bytes).map_err(|_| refusal(Refusal::StateUnavailable))?;
        #[cfg(unix)]
        if row.executable {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(destination, std::fs::Permissions::from_mode(0o700))
                .map_err(|_| refusal(Refusal::StateUnavailable))?;
        }
    }
    let destination = artifact.path().join(config_path);
    std::fs::create_dir_all(
        destination
            .parent()
            .ok_or_else(|| refusal(Refusal::StateUnavailable))?,
    )
    .map_err(|_| refusal(Refusal::StateUnavailable))?;
    std::fs::write(&destination, []).map_err(|_| refusal(Refusal::StateUnavailable))?;
    let mut config =
        git2::Config::open(&destination).map_err(|_| refusal(Refusal::InvalidRepository))?;
    for (name, path, value) in config_entries {
        let value = path
            .map(|path| artifact.path().join(path).to_string_lossy().into_owned())
            .or(value)
            .unwrap_or_else(|| "true".to_owned());
        config
            .set_multivar(&name, "a^", &value)
            .map_err(|_| refusal(Refusal::InvalidRepository))?;
    }
    config
        .set_bool("core.bare", false)
        .map_err(|_| refusal(Refusal::InvalidRepository))?;
    // This setting applies only to the disposable copy, never the original.
    config
        .set_str("core.worktree", "..")
        .map_err(|_| refusal(Refusal::InvalidRepository))?;
    drop(config);
    check(control)?;
    snapshot
        .authority
        .verify_physical_root_anchor()
        .map_err(|_| refusal(Refusal::SourceUnavailable))?;
    workspace_lease
        .reopened_matching_stable_key(workspace_key)
        .map_err(|_| refusal(Refusal::StateUnavailable))?;
    let claim = GitPreparationClaim {
        view_identity: identity.clone(),
        serving_publication_identity: snapshot.serving_publication_identity.clone(),
        canonical_argument_hash: argument_hash.clone(),
        copied_bytes: capture.bytes,
        copied_files: capture.files.len() as u64,
        reused_files: 0,
    };
    let view = Arc::new(PreparedGitView {
        artifact,
        identity,
        argument_hash,
        source_binding: snapshot.binding_identity.clone(),
        inputs: capture.files.into_values().collect(),
        source_key,
        copied_bytes: capture.bytes,
    });
    view.repository()?;
    Ok((view, claim))
}
