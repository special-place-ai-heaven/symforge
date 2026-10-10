//! Native secret-remediation preview/apply over an admitted embedded source.

use std::path::Path;
use std::path::PathBuf;
use std::sync::atomic::Ordering;

use crate::edit_safety::batch_commit::{
    EmbeddedBatchIo, StagedImage, commit_staged_locked, with_staged_locks,
};
use crate::edit_safety::secret_remediation::{self as shared_stage, StageError};
use crate::embed::parity::replay::{
    OutcomeKind, ReplayKey, ReplayOutcome, ReplayRecord, ReplayState, ReplayStore,
    RequestFingerprint, ReserveOutcome,
};

use crate::embed::parity::host::{OperationControl, OperationStop};
use crate::embed::parity::remediation::{
    SecretActionAvailability, SecretActionUnavailable, SecretApplyAuthority, SecretApplyStatus,
    SecretExternalTool, SecretFileGuard, SecretFindingSummary, SecretFindingsClaim,
    SecretRemediationAction, SecretRemediationApplied, SecretRemediationError,
    SecretRemediationPreview, SecretRemediationRefusalKind, SecretRemediationRequest,
    SecretRemediationScope, SecretScanLimits,
};
use crate::knowledge::SecretSpansScan;
use crate::knowledge::secret_remediation::{self, SelectedFile, SelectionRefusal};

use super::activation::ProjectSourceAuthority;
use super::embedded::EmbeddedSourceHandle;
use super::physical_root::read_regular_beneath_root;

const MAX_SCOPE_PATHS: usize = 100_000;
const MAX_REMEDIATION_FILE_BYTES: usize = 2 * 1024 * 1024;

#[cfg(all(test, unix))]
mod encrypt_input_tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;
    use std::time::Duration;

    #[test]
    fn encrypt_process_receives_selected_bytes_after_path_changes() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let path = "config.json";
        let selected_bytes = b"{\"source\":\"selected\"}\n".to_vec();
        std::fs::write(root.join(path), b"{\"source\":\"later\"}\n").unwrap();
        let fake = root.join("fake-sops.sh");
        std::fs::write(
            &fake,
            b"#!/bin/sh\nout=''\ninput=''\nwhile [ \"$#\" -gt 0 ]; do\n  case \"$1\" in\n    encrypt) ;;\n    --output) shift; out=$1 ;;\n    --age|--input-type|--output-type|--filename-override) shift ;;\n    --*) ;;\n    *) input=$1 ;;\n  esac\n  shift\ndone\nif [ -n \"$input\" ]; then /bin/cat \"$input\" > \"$out\"; else /bin/cat > \"$out\"; fi\n",
        )
        .unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o700)).unwrap();
        let tool = SecretExternalTool::new(
            fake,
            "age1qqqqqqqqqqqqqq".to_string(),
            Duration::from_secs(5),
        )
        .unwrap();
        let authority = SecretApplyAuthority::for_source_root(
            root.to_path_buf(),
            "test-scope".to_string(),
            Arc::new(AtomicBool::new(false)),
            SecretScanLimits::default(),
            None,
            Some(tool),
        )
        .unwrap();
        let staged = stage_encrypt(
            root,
            vec![SelectedFile {
                path: path.to_string(),
                original: selected_bytes.clone(),
                findings: Vec::new(),
            }],
            &authority,
        )
        .unwrap();
        assert_eq!(staged[0].replacement, selected_bytes);
    }
}

#[cfg(test)]
mod real_sops_tests {
    use super::*;
    use std::process::Stdio;
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;
    use std::time::Duration;

    #[test]
    fn real_sops_encrypts_only_admitted_selected_bytes_when_configured() {
        let Some(binary) = std::env::var_os("SYMFORGE_TEST_SOPS_BIN") else {
            return;
        };
        let Some(recipient) = std::env::var_os("SYMFORGE_TEST_AGE_RECIPIENT") else {
            return;
        };
        let Some(identity_file) = std::env::var_os("SYMFORGE_TEST_AGE_IDENTITY_FILE") else {
            return;
        };
        let recipient = recipient.to_string_lossy().into_owned();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let path = "config.json";
        let selected_bytes = b"{\"marker\":\"selected\"}\n".to_vec();
        let later_bytes = b"{\"marker\":\"later\"}\n";
        std::fs::write(root.join(path), later_bytes).unwrap();
        let tool =
            SecretExternalTool::new(PathBuf::from(binary), recipient, Duration::from_secs(20))
                .expect("admitted SOPS descriptor");
        let authority = SecretApplyAuthority::for_source_root(
            root.to_path_buf(),
            "live-sops-test".to_string(),
            Arc::new(AtomicBool::new(false)),
            SecretScanLimits::default(),
            None,
            Some(tool),
        )
        .unwrap();
        let staged = stage_encrypt(
            root,
            vec![SelectedFile {
                path: path.to_string(),
                original: selected_bytes,
                findings: Vec::new(),
            }],
            &authority,
        )
        .expect("SOPS encrypted selected bytes");
        assert!(std::fs::read(root.join(path)).is_ok_and(|bytes| bytes.as_slice() == later_bytes));
        let ciphertext = root.join("ciphertext.json");
        std::fs::write(&ciphertext, &staged[0].replacement).unwrap();
        let output = crate::process_util::hidden_command(
            std::env::var_os("SYMFORGE_TEST_SOPS_BIN").unwrap(),
        )
        .env_clear()
        .env("SOPS_AGE_KEY_FILE", identity_file)
        .args(["decrypt", "--input-type", "json", "--output-type", "json"])
        .arg(&ciphertext)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .expect("SOPS decrypt child started");
        assert!(output.status.success(), "SOPS decrypt refused ciphertext");
        let plaintext: serde_json::Value =
            serde_json::from_slice(&output.stdout).expect("SOPS decrypted JSON");
        assert!(
            plaintext["marker"] == "selected",
            "encrypted later path bytes"
        );
    }
}

fn refusal(kind: SecretRemediationRefusalKind) -> SecretRemediationError {
    SecretRemediationError::Refused(kind)
}

fn check_control(control: Option<&OperationControl>) -> Result<(), SecretRemediationError> {
    if let Some(control) = control {
        control.check().map_err(|stop| {
            refusal(match stop {
                OperationStop::Cancelled => SecretRemediationRefusalKind::Cancelled,
                OperationStop::DeadlineExceeded => SecretRemediationRefusalKind::DeadlineExceeded,
            })
        })?;
    }
    Ok(())
}

fn selection_refusal(error: SelectionRefusal) -> SecretRemediationError {
    use SecretRemediationRefusalKind as Kind;
    refusal(match error {
        SelectionRefusal::EmptyFindings
        | SelectionRefusal::DuplicateFinding
        | SelectionRefusal::TooManyFindings => Kind::InvalidRequest,
        SelectionRefusal::ScanIndeterminate => Kind::ScanIndeterminate,
        SelectionRefusal::MissingFinding => Kind::FindingNotFound,
        SelectionRefusal::UndismissableFinding => Kind::RuleCannotBeDismissed,
        SelectionRefusal::MissingLine => Kind::ScanIndeterminate,
        SelectionRefusal::UnsupportedEncryptFormat => Kind::InvalidRequest,
        SelectionRefusal::ResourceLimit => Kind::ResourceLimit,
        SelectionRefusal::Cancelled => Kind::Cancelled,
        SelectionRefusal::DeadlineExceeded => Kind::DeadlineExceeded,
    })
}

fn stage_refusal(error: StageError) -> SecretRemediationError {
    match error {
        StageError::InvalidRequest => refusal(SecretRemediationRefusalKind::InvalidRequest),
        StageError::WriteConflict => refusal(SecretRemediationRefusalKind::WriteConflict),
        StageError::SourceUnavailable => refusal(SecretRemediationRefusalKind::SourceUnavailable),
        StageError::ResourceLimit => refusal(SecretRemediationRefusalKind::ResourceLimit),
        StageError::Selection(error) => selection_refusal(error),
    }
}

fn scoped_paths(
    root: &Path,
    scope: &SecretRemediationScope,
) -> Result<Vec<String>, SecretRemediationError> {
    let mut paths = match scope {
        SecretRemediationScope::Paths(paths) => {
            if paths.is_empty() || paths.len() > MAX_SCOPE_PATHS {
                return Err(refusal(SecretRemediationRefusalKind::InvalidScope));
            }
            for path in paths {
                if crate::discovery::resolve_repo_path(root, path)
                    .map_err(|_| refusal(SecretRemediationRefusalKind::InvalidScope))?
                    .is_none()
                {
                    return Err(refusal(SecretRemediationRefusalKind::InvalidScope));
                }
            }
            paths.clone()
        }
        SecretRemediationScope::Repo => crate::discovery::discover_all_files(root)
            .map_err(|_| refusal(SecretRemediationRefusalKind::ResourceLimit))?
            .into_iter()
            .map(|entry| entry.relative_path)
            .collect(),
    };
    paths.sort();
    paths.dedup();
    if paths.len() > MAX_SCOPE_PATHS {
        return Err(refusal(SecretRemediationRefusalKind::ResourceLimit));
    }
    Ok(paths)
}

struct ScanMeter<'a> {
    remaining_bytes: u64,
    control: Option<&'a OperationControl>,
}

impl<'a> ScanMeter<'a> {
    fn new(limits: SecretScanLimits, control: Option<&'a OperationControl>) -> Self {
        Self {
            remaining_bytes: limits.max_total_bytes,
            control,
        }
    }

    fn read(&mut self, root: &Path, path: &str) -> Result<Option<Vec<u8>>, SelectionRefusal> {
        if let Some(control) = self.control {
            control.check().map_err(|stop| match stop {
                OperationStop::Cancelled => SelectionRefusal::Cancelled,
                OperationStop::DeadlineExceeded => SelectionRefusal::DeadlineExceeded,
            })?;
        }
        let bytes = read_regular_beneath_root(
            root,
            Path::new(path),
            crate::knowledge::SECRET_SCAN_MAX_BYTES,
        )
        .map_err(|_| SelectionRefusal::ScanIndeterminate)?;
        if let Some(ref bytes) = bytes {
            self.remaining_bytes = self
                .remaining_bytes
                .checked_sub(bytes.len() as u64)
                .ok_or(SelectionRefusal::ResourceLimit)?;
        }
        Ok(bytes)
    }
}

fn select(
    root: &Path,
    paths: &[String],
    finding_ids: &[String],
    meter: &mut ScanMeter<'_>,
) -> Result<Vec<SelectedFile>, SecretRemediationError> {
    secret_remediation::select_requested_checked(paths, finding_ids, |path| meter.read(root, path))
        .map_err(selection_refusal)
}

fn stage_image(
    root: &Path,
    path: &str,
    original: Option<Vec<u8>>,
    replacement: Vec<u8>,
    owner_only: bool,
) -> StagedImage {
    StagedImage {
        relative: PathBuf::from(path),
        absolute: root.join(path),
        original,
        replacement,
        owner_only,
    }
}

fn optional_file(root: &Path, path: &str) -> Result<Option<Vec<u8>>, SecretRemediationError> {
    read_regular_beneath_root(root, Path::new(path), MAX_REMEDIATION_FILE_BYTES)
        .map_err(|_| refusal(SecretRemediationRefusalKind::SourceUnavailable))
}

fn stage_externalize(
    root: &Path,
    selected: Vec<SelectedFile>,
) -> Result<Vec<StagedImage>, SecretRemediationError> {
    shared_stage::stage_externalize(root, selected, |path| {
        optional_file(root, path).map_err(|_| StageError::SourceUnavailable)
    })
    .map_err(stage_refusal)
}

fn stage_dismiss(
    root: &Path,
    selected: &[SelectedFile],
) -> Result<Vec<StagedImage>, SecretRemediationError> {
    shared_stage::stage_dismiss(root, selected, |path| {
        optional_file(root, path).map_err(|_| StageError::SourceUnavailable)
    })
    .map_err(stage_refusal)
}

fn encrypt_file(
    path: &str,
    selected_bytes: &[u8],
    tool: &SecretExternalTool,
    authority: &SecretApplyAuthority,
) -> Result<Vec<u8>, SecretRemediationError> {
    let timeout = authority.control.as_ref().map_or(tool.timeout, |control| {
        tool.timeout.min(control.remaining())
    });
    shared_stage::encrypt_selected_bytes(
        path,
        selected_bytes,
        &tool.binary,
        &tool.recipient,
        timeout,
        || {
            if authority.cancel.load(Ordering::Acquire) {
                return Some(shared_stage::EncryptError::Cancelled);
            }
            match authority
                .control
                .as_ref()
                .and_then(|control| control.check().err())
            {
                Some(OperationStop::Cancelled) => Some(shared_stage::EncryptError::Cancelled),
                Some(OperationStop::DeadlineExceeded) => {
                    Some(shared_stage::EncryptError::DeadlineExceeded)
                }
                None => None,
            }
        },
    )
    .map_err(|error| {
        let kind = match error {
            shared_stage::EncryptError::InvalidFormat => {
                SecretRemediationRefusalKind::InvalidRequest
            }
            shared_stage::EncryptError::Unavailable => {
                SecretRemediationRefusalKind::ExternalToolUnavailable
            }
            shared_stage::EncryptError::ResourceLimit => {
                SecretRemediationRefusalKind::ResourceLimit
            }
            shared_stage::EncryptError::Cancelled => SecretRemediationRefusalKind::Cancelled,
            shared_stage::EncryptError::DeadlineExceeded => {
                SecretRemediationRefusalKind::DeadlineExceeded
            }
            shared_stage::EncryptError::SourceUnavailable => {
                SecretRemediationRefusalKind::SourceUnavailable
            }
        };
        refusal(kind)
    })
}

fn stage_encrypt(
    root: &Path,
    selected: Vec<SelectedFile>,
    authority: &SecretApplyAuthority,
) -> Result<Vec<StagedImage>, SecretRemediationError> {
    let tool = authority
        .external_tool
        .as_ref()
        .ok_or_else(|| refusal(SecretRemediationRefusalKind::ExternalToolUnavailable))?;
    secret_remediation::ensure_encrypt_formats(&selected).map_err(selection_refusal)?;
    let mut images = Vec::new();
    for file in selected {
        let encrypted = encrypt_file(&file.path, &file.original, tool, authority)?;
        if secret_remediation::contains_selected_plaintext(&encrypted, &file) {
            return Err(refusal(SecretRemediationRefusalKind::ScanIndeterminate));
        }
        images.push(stage_image(
            root,
            &file.path,
            Some(file.original),
            encrypted,
            false,
        ));
    }
    Ok(images)
}

fn post_image_manifest(
    authority: &ProjectSourceAuthority,
    paths: &[String],
) -> Result<Vec<u8>, SecretRemediationError> {
    let mut images = Vec::new();
    for path in paths {
        let bytes = authority
            .read_regular_beneath_anchor(Path::new(path), MAX_REMEDIATION_FILE_BYTES)
            .map_err(|_| refusal(SecretRemediationRefusalKind::SourceUnavailable))?
            .ok_or_else(|| refusal(SecretRemediationRefusalKind::WriteConflict))?;
        images.push((path, crate::hash::digest_hex(&bytes)));
    }
    serde_json::to_vec(&images)
        .map_err(|_| refusal(SecretRemediationRefusalKind::ReplayUnavailable))
}

fn expected_manifest(
    staged: &[StagedImage],
    paths: &[String],
) -> Result<Vec<u8>, SecretRemediationError> {
    let mut images = Vec::new();
    for path in paths {
        let image = staged
            .iter()
            .find(|image| image.relative == Path::new(path))
            .ok_or_else(|| refusal(SecretRemediationRefusalKind::InvalidRequest))?;
        images.push((path, crate::hash::digest_hex(&image.replacement)));
    }
    serde_json::to_vec(&images)
        .map_err(|_| refusal(SecretRemediationRefusalKind::ReplayUnavailable))
}

fn source_rescan_status(
    authority: &ProjectSourceAuthority,
    files: &[SecretFileGuard],
    action: SecretRemediationAction,
) -> SecretApplyStatus {
    let dismissals = if action == SecretRemediationAction::Dismiss {
        match authority.read_regular_beneath_anchor(
            Path::new(crate::knowledge::secret_dismissals::DISMISSAL_STORE_REL),
            crate::knowledge::secret_dismissals::DISMISSAL_STORE_MAX_BYTES as usize,
        ) {
            Ok(Some(bytes)) => {
                match crate::knowledge::secret_dismissals::parse_dismissals_bytes(&bytes) {
                    Ok(records) => Some(records),
                    Err(_) => return SecretApplyStatus::Indeterminate,
                }
            }
            _ => return SecretApplyStatus::Indeterminate,
        }
    } else {
        None
    };
    let mut sensitive = false;
    for file in files {
        let Ok(Some(bytes)) = authority.read_regular_beneath_anchor(
            Path::new(&file.path),
            crate::knowledge::SECRET_SCAN_MAX_BYTES,
        ) else {
            return SecretApplyStatus::Indeterminate;
        };
        let scan = if action == SecretRemediationAction::Dismiss {
            crate::knowledge::secret_dismissals::scan_with_records(
                &file.path,
                &bytes,
                dismissals.as_deref().unwrap_or_default(),
            )
        } else {
            crate::knowledge::scan_secret_bytes(&file.path, &bytes)
        };
        match scan {
            crate::knowledge::SecretScan::Clean => {}
            crate::knowledge::SecretScan::Sensitive { .. } => sensitive = true,
            crate::knowledge::SecretScan::Indeterminate { .. } => {
                return SecretApplyStatus::Indeterminate;
            }
        }
    }
    if sensitive {
        SecretApplyStatus::StillSensitive
    } else {
        SecretApplyStatus::Clean
    }
}

fn request_fingerprint(
    preview: &SecretRemediationPreview,
    authority: &SecretApplyAuthority,
) -> Result<RequestFingerprint, SecretRemediationError> {
    let tool_identity = authority.external_tool.as_ref().map(|tool| {
        crate::hash::digest_hex(format!("{}\0{}", tool.binary.display(), tool.recipient).as_bytes())
    });
    let arguments = serde_json::json!({
        "schema": "symforge.embed.secret-remediation.v1",
        "request": preview.request,
        "publication_identity": preview.publication_identity,
        "source_version": preview.source_version,
        "files": preview.files,
        "would_write": preview.would_write,
        "external_tool_identity": tool_identity,
    });
    RequestFingerprint::for_json("secret_remediate", &arguments)
        .map_err(|_| refusal(SecretRemediationRefusalKind::ReplayUnavailable))
}

impl EmbeddedSourceHandle {
    /// Apply a previously captured redacted preview under host source authority.
    /// A completed replay verifies every current post-image before returning.
    pub fn apply_secret_remediation(
        &self,
        preview: &SecretRemediationPreview,
        authority: &SecretApplyAuthority,
        operation_key: &str,
    ) -> Result<SecretRemediationApplied, SecretRemediationError> {
        let (bound_root, state_dir) = self
            .bound_replay_placement()
            .ok_or_else(|| refusal(SecretRemediationRefusalKind::SourceUnavailable))?;
        if authority.root != bound_root {
            return Err(refusal(SecretRemediationRefusalKind::WriteAuthorityRefused));
        }
        if authority.cancel.load(Ordering::Acquire) {
            return Err(refusal(SecretRemediationRefusalKind::Cancelled));
        }
        check_control(authority.control.as_ref())?;
        let state_dir = state_dir
            .as_deref()
            .ok_or_else(|| refusal(SecretRemediationRefusalKind::ReplayUnavailable))?;
        let key = ReplayKey::new(operation_key)
            .map_err(|_| refusal(SecretRemediationRefusalKind::ReplayUnavailable))?;
        let bound_authority = self
            .bound_replay_authority()
            .ok_or_else(|| refusal(SecretRemediationRefusalKind::SourceUnavailable))?;
        let root_anchor_key = bound_authority
            .physical_root_stable_key()
            .ok_or_else(|| refusal(SecretRemediationRefusalKind::SourceUnavailable))?;
        let replay =
            ReplayStore::open_bound(&bound_root, state_dir, &authority.scope, root_anchor_key)
                .map_err(|_| refusal(SecretRemediationRefusalKind::ReplayUnavailable))?;
        let fingerprint = request_fingerprint(preview, authority)?;
        let completed = |record: ReplayRecord| {
            if record.request != fingerprint
                || record.state != ReplayState::Completed
                || record
                    .outcome
                    .as_ref()
                    .is_none_or(|outcome| outcome.kind != OutcomeKind::Applied)
            {
                return Err(refusal(SecretRemediationRefusalKind::ReplayConflict));
            }
            let manifest = post_image_manifest(&bound_authority, &preview.would_write)?;
            if !record.matches_post_image(&manifest) {
                return Err(refusal(SecretRemediationRefusalKind::ReplayConflict));
            }
            Ok(SecretRemediationApplied {
                status: source_rescan_status(
                    &bound_authority,
                    &preview.files,
                    preview.request.action,
                ),
                written: preview.would_write.clone(),
                replayed: true,
                refresh_ticket_identity: None,
            })
        };
        if let Some(record) = replay
            .inspect(&key)
            .map_err(|_| refusal(SecretRemediationRefusalKind::ReplayConflict))?
        {
            return completed(record);
        }
        let snapshot = self.capture_query_snapshot(b"apply-secret-remediation")?;
        if snapshot.root != bound_root || snapshot.state_dir.as_deref() != Some(state_dir) {
            return Err(refusal(SecretRemediationRefusalKind::SourceUnavailable));
        }
        let lease = match replay
            .reserve(&key, &fingerprint)
            .map_err(|_| refusal(SecretRemediationRefusalKind::ReplayConflict))?
        {
            ReserveOutcome::Acquired(lease) => lease,
            ReserveOutcome::Existing(record) => return completed(record),
        };
        let prepare = || -> Result<Vec<StagedImage>, SecretRemediationError> {
            if snapshot.serving_publication_identity != preview.publication_identity
                || snapshot.source_version != preview.source_version
            {
                return Err(refusal(SecretRemediationRefusalKind::StalePublication));
            }
            let paths = scoped_paths(&snapshot.root, &preview.request.scope)?;
            if paths.len() > authority.scan_limits.max_files as usize
                || preview.request.finding_ids.len() > authority.scan_limits.max_findings as usize
            {
                return Err(refusal(SecretRemediationRefusalKind::ResourceLimit));
            }
            let mut meter = ScanMeter::new(authority.scan_limits, authority.control.as_ref());
            for guard in &preview.files {
                let current = meter
                    .read(&snapshot.root, &guard.path)
                    .map_err(selection_refusal)?
                    .ok_or_else(|| refusal(SecretRemediationRefusalKind::StaleContent))?;
                if crate::hash::digest_hex(&current) != guard.content_hash {
                    return Err(refusal(SecretRemediationRefusalKind::StaleContent));
                }
            }
            let selected = select(
                &snapshot.root,
                &paths,
                &preview.request.finding_ids,
                &mut meter,
            )?;
            if selected.len() != preview.files.len()
                || selected.iter().zip(&preview.files).any(|(file, guard)| {
                    file.path != guard.path
                        || crate::hash::digest_hex(&file.original) != guard.content_hash
                })
            {
                return Err(refusal(SecretRemediationRefusalKind::StaleContent));
            }
            let staged = match preview.request.action {
                SecretRemediationAction::Externalize => {
                    stage_externalize(&snapshot.root, selected)?
                }
                SecretRemediationAction::Encrypt => {
                    stage_encrypt(&snapshot.root, selected, authority)?
                }
                SecretRemediationAction::Dismiss => stage_dismiss(&snapshot.root, &selected)?,
            };
            let mut paths = staged
                .iter()
                .map(|image| image.relative.to_string_lossy().replace('\\', "/"))
                .collect::<Vec<_>>();
            paths.sort();
            if paths != preview.would_write {
                return Err(refusal(SecretRemediationRefusalKind::InvalidRequest));
            }
            Ok(staged)
        };
        let staged = match prepare() {
            Ok(staged) => staged,
            Err(error) => {
                let _ = replay.release_not_started(&lease);
                return Err(error);
            }
        };
        if authority.cancel.load(Ordering::Acquire) {
            let _ = replay.release_not_started(&lease);
            return Err(refusal(SecretRemediationRefusalKind::Cancelled));
        }
        if let Err(error) = check_control(authority.control.as_ref()) {
            let _ = replay.release_not_started(&lease);
            return Err(error);
        }
        let mut started = false;
        let committed = with_staged_locks(&staged, |order| {
            let source_authority =
                crate::live_index::index_lifecycle::activation::project_source_authority(
                    &snapshot.root,
                );
            let write = source_authority
                .acquire_write_expected(snapshot.authority_publication)
                .map_err(|_| refusal(SecretRemediationRefusalKind::StalePublication))?;
            for guard in &preview.files {
                let observed = write
                    .read_regular_beneath(
                        Path::new(&guard.path),
                        crate::knowledge::SECRET_SCAN_MAX_BYTES,
                    )
                    .map_err(|_| refusal(SecretRemediationRefusalKind::SourceUnavailable))?
                    .ok_or_else(|| refusal(SecretRemediationRefusalKind::StaleContent))?;
                if crate::hash::digest_hex(&observed) != guard.content_hash {
                    return Err(refusal(SecretRemediationRefusalKind::StaleContent));
                }
            }
            let mut io = EmbeddedBatchIo::new(write);
            if authority.cancel.load(Ordering::Acquire)
                || check_control(authority.control.as_ref()).is_err()
            {
                let _ = io.finish();
                return Err(refusal(SecretRemediationRefusalKind::Cancelled));
            }
            if replay.mark_started(&lease).is_err() {
                let _ = io.finish();
                return Err(refusal(SecretRemediationRefusalKind::ReplayUnavailable));
            }
            started = true;
            match commit_staged_locked(&staged, order, &mut io, Some(&authority.cancel)) {
                Ok(_) => io
                    .finish()
                    .map_err(|_| refusal(SecretRemediationRefusalKind::WriteUncertain)),
                Err(_) => {
                    let _ = io.finish();
                    Err(refusal(SecretRemediationRefusalKind::WriteUncertain))
                }
            }
        });
        match committed {
            Ok(Ok(())) => {}
            _ => {
                if started {
                    let _ = replay.mark_uncertain(&lease);
                    return Err(refusal(SecretRemediationRefusalKind::WriteUncertain));
                }
                let _ = replay.release_not_started(&lease);
                return Err(refusal(SecretRemediationRefusalKind::WriteConflict));
            }
        }
        if authority.cancel.load(Ordering::Acquire) {
            let _ = replay.mark_uncertain(&lease);
            return Err(refusal(SecretRemediationRefusalKind::WriteUncertain));
        }
        if check_control(authority.control.as_ref()).is_err() {
            let _ = replay.mark_uncertain(&lease);
            return Err(refusal(SecretRemediationRefusalKind::WriteUncertain));
        }
        let refresh = self.request_refresh().map_err(|_| {
            let _ = replay.mark_uncertain(&lease);
            refusal(SecretRemediationRefusalKind::WriteUncertain)
        })?;
        let manifest =
            post_image_manifest(&snapshot.authority, &preview.would_write).map_err(|_| {
                let _ = replay.mark_uncertain(&lease);
                refusal(SecretRemediationRefusalKind::WriteUncertain)
            })?;
        if expected_manifest(&staged, &preview.would_write)
            .ok()
            .as_deref()
            != Some(manifest.as_slice())
        {
            let _ = replay.mark_uncertain(&lease);
            return Err(refusal(SecretRemediationRefusalKind::WriteUncertain));
        }
        let outcome = ReplayOutcome::from_response_and_post_image(
            OutcomeKind::Applied,
            b"secret_remediation_applied",
            &manifest,
        )
        .map_err(|_| {
            let _ = replay.mark_uncertain(&lease);
            refusal(SecretRemediationRefusalKind::WriteUncertain)
        })?;
        replay.complete(&lease, &outcome).map_err(|_| {
            let _ = replay.mark_uncertain(&lease);
            refusal(SecretRemediationRefusalKind::WriteUncertain)
        })?;
        Ok(SecretRemediationApplied {
            status: source_rescan_status(
                &snapshot.authority,
                &preview.files,
                preview.request.action,
            ),
            written: preview.would_write.clone(),
            replayed: false,
            refresh_ticket_identity: Some(refresh.ticket_identity().to_owned()),
        })
    }

    /// Discover bounded, redacted finding IDs under one current source capture.
    pub fn inspect_secret_findings(
        &self,
        scope: &SecretRemediationScope,
        limits: SecretScanLimits,
        control: Option<&OperationControl>,
        external_tool: Option<&SecretExternalTool>,
    ) -> Result<SecretFindingsClaim, SecretRemediationError> {
        if !limits.valid() {
            return Err(refusal(SecretRemediationRefusalKind::InvalidRequest));
        }
        check_control(control)?;
        let snapshot = self.capture_query_snapshot(b"inspect-secret-findings")?;
        let paths = scoped_paths(&snapshot.root, scope)?;
        if paths.len() > limits.max_files as usize {
            return Err(refusal(SecretRemediationRefusalKind::ResourceLimit));
        }
        let mut meter = ScanMeter::new(limits, control);
        let mut findings = Vec::new();
        for path in &paths {
            let bytes = meter
                .read(&snapshot.root, path)
                .map_err(selection_refusal)?
                .ok_or_else(|| refusal(SecretRemediationRefusalKind::ScanIndeterminate))?;
            let hash = crate::hash::digest_hex(&bytes);
            let spans = match crate::knowledge::scan_secret_spans(path, &bytes) {
                SecretSpansScan::Spans(spans) => spans,
                SecretSpansScan::Indeterminate { .. } => {
                    return Err(refusal(SecretRemediationRefusalKind::ScanIndeterminate));
                }
            };
            for span in spans {
                if findings.len() == limits.max_findings as usize {
                    return Err(refusal(SecretRemediationRefusalKind::ResourceLimit));
                }
                let encrypt_unavailable = if !secret_remediation::encrypt_format_supported(path) {
                    Some(SecretActionUnavailable::UnsupportedFormat)
                } else if external_tool.is_none() {
                    Some(SecretActionUnavailable::ExternalToolUnavailable)
                } else {
                    None
                };
                findings.push(SecretFindingSummary {
                    id: secret_remediation::mint_finding_id(
                        path,
                        span.rule_id,
                        span.line_start,
                        span.line_end,
                        &span.shape,
                    ),
                    path: path.clone(),
                    content_hash: hash.clone(),
                    rule_id: span.rule_id.to_owned(),
                    line_start: span.line_start,
                    line_end: span.line_end,
                    shape: span.shape,
                    actions: vec![
                        SecretActionAvailability {
                            action: SecretRemediationAction::Externalize,
                            unavailable: None,
                        },
                        SecretActionAvailability {
                            action: SecretRemediationAction::Encrypt,
                            unavailable: encrypt_unavailable,
                        },
                        SecretActionAvailability {
                            action: SecretRemediationAction::Dismiss,
                            unavailable: if crate::knowledge::secret_dismissals::rule_is_dismissable(
                                span.rule_id,
                            ) {
                                None
                            } else {
                                Some(SecretActionUnavailable::RuleCannotBeDismissed)
                            },
                        },
                    ],
                });
            }
        }
        let current = self.capture_query_snapshot(b"inspect-secret-findings-after")?;
        if current.serving_publication_identity != snapshot.serving_publication_identity {
            return Err(refusal(SecretRemediationRefusalKind::StalePublication));
        }
        Ok(SecretFindingsClaim {
            publication_identity: snapshot.serving_publication_identity,
            source_version: snapshot.source_version,
            findings,
            scanned_files: paths.len() as u64,
        })
    }

    /// Build a redacted plan bound to the current admitted source publication.
    pub fn preview_secret_remediation(
        &self,
        request: &SecretRemediationRequest,
        limits: SecretScanLimits,
        control: Option<&OperationControl>,
    ) -> Result<SecretRemediationPreview, SecretRemediationError> {
        if !limits.valid() {
            return Err(refusal(SecretRemediationRefusalKind::InvalidRequest));
        }
        check_control(control)?;
        let snapshot = self.capture_query_snapshot(b"preview-secret-remediation")?;
        let paths = scoped_paths(&snapshot.root, &request.scope)?;
        if paths.len() > limits.max_files as usize
            || request.finding_ids.len() > limits.max_findings as usize
        {
            return Err(refusal(SecretRemediationRefusalKind::ResourceLimit));
        }
        let mut meter = ScanMeter::new(limits, control);
        let selected = select(&snapshot.root, &paths, &request.finding_ids, &mut meter)?;
        let files = selected
            .iter()
            .map(|file| SecretFileGuard {
                path: file.path.clone(),
                content_hash: crate::hash::digest_hex(&file.original),
            })
            .collect::<Vec<_>>();
        let (mut would_write, masked_summary) = match request.action {
            SecretRemediationAction::Externalize => {
                let plans = secret_remediation::plan_externalize(selected);
                let mut written = plans
                    .iter()
                    .map(|plan| plan.path.clone())
                    .collect::<Vec<_>>();
                written.extend([".env".to_owned(), ".gitignore".to_owned()]);
                let summary = plans
                    .iter()
                    .map(|plan| plan.masked_diff.as_str())
                    .collect::<String>();
                (written, summary)
            }
            SecretRemediationAction::Encrypt => {
                secret_remediation::ensure_encrypt_formats(&selected).map_err(selection_refusal)?;
                let names = selected.iter().map(|file| file.path.clone()).collect();
                (
                    names,
                    "SOPS encryption of selected files; no values shown".to_owned(),
                )
            }
            SecretRemediationAction::Dismiss => {
                secret_remediation::plan_dismiss(&selected).map_err(selection_refusal)?;
                (
                    vec![crate::knowledge::secret_dismissals::DISMISSAL_STORE_REL.to_owned()],
                    format!(
                        "{} content-digest dismissal(s), no values shown",
                        request.finding_ids.len()
                    ),
                )
            }
        };
        would_write.sort();
        would_write.dedup();
        check_control(control)?;
        let current = self.capture_query_snapshot(b"preview-secret-remediation-after")?;
        if current.serving_publication_identity != snapshot.serving_publication_identity {
            return Err(refusal(SecretRemediationRefusalKind::StalePublication));
        }
        Ok(SecretRemediationPreview {
            request: request.clone(),
            publication_identity: snapshot.serving_publication_identity,
            source_version: snapshot.source_version,
            files,
            would_write,
            masked_summary,
        })
    }
}
