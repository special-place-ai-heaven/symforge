//! Embedded multi-file structural edit over the MCP splice functions and the
//! shared staged commit/rollback kernel. One host grant, replay key, and source
//! mutation permit cover the entire batch.

use std::collections::{BTreeMap, HashMap};
use std::ops::Range;
use std::path::PathBuf;
use std::sync::atomic::Ordering;

use crate::domain::SymbolRecord;
use crate::edit_safety::batch_commit::{
    BatchAbort, BatchIo, EmbeddedBatchIo, StagedImage, commit_staged_locked, with_staged_locks,
};
use crate::edit_safety::preview::render_safe_batch;
use crate::edit_safety::rename::{RenamePlanInput, build_rename_plan};
use crate::edit_safety::structural::{
    SpliceFootprint, WithinSelectionError, apply_splice, build_delete, build_insert_after,
    build_insert_before, cleanup_invalidates_splice, delete_cleanup_ranges, delete_source_range,
    detect_line_ending, insert_after_position, insert_before_position, prepare_replace,
    replacement_splice_range, select_edit_within_body, splice_footprints_overlap,
};
use crate::embed::parity::edit::{
    BatchEditAction, BatchEditApplied, BatchEditPreview, BatchEditRequest, BatchFilePreview,
    BatchInsertRequest, BatchRenamePlan, BatchRenamePreview, BatchRenameRequest,
    EditApplyAuthority, EditError, EditErrorKind, EditTarget, InsertRequest, RoutedBatchApplied,
    RoutedBatchPart,
};
use crate::embed::parity::replay::{
    OutcomeKind, ReplayKey, ReplayOutcome, ReplayState, ReplayStore, RequestFingerprint,
    ReserveOutcome,
};
use crate::hash::digest_hex;
use crate::knowledge::{StableContentAdmission, classify_stable_content_for_root};

use super::embed_mutation::{admitted, validate_guard};
use super::embed_query::EmbeddedQuerySnapshot;
use super::embedded::EmbeddedSourceHandle;

struct ResolvedAction<'a> {
    symbol: SymbolRecord,
    action: &'a BatchEditAction,
    absolute: PathBuf,
}

fn stage_batch(
    snapshot: &EmbeddedQuerySnapshot,
    request: &BatchEditRequest,
) -> Result<Vec<StagedImage>, EditErrorKind> {
    if request.actions.is_empty() || request.actions.len() > 100 {
        return Err(EditErrorKind::InvalidReplacement);
    }
    let mut by_file = BTreeMap::<String, Vec<ResolvedAction<'_>>>::new();
    for action in &request.actions {
        let (file, symbol, absolute) = validate_guard(snapshot, action.guard())?;
        if let Some(capability) =
            crate::parsing::config_extractors::edit_capability_for_language(&file.language)
        {
            let allowed = match action {
                BatchEditAction::EditWithin(_) => !matches!(
                    capability,
                    crate::parsing::config_extractors::EditCapability::IndexOnly
                ),
                _ => matches!(
                    capability,
                    crate::parsing::config_extractors::EditCapability::StructuralEditSafe
                ),
            };
            if !allowed {
                return Err(EditErrorKind::InvalidReplacement);
            }
        }
        by_file
            .entry(action.guard().path.clone())
            .or_default()
            .push(ResolvedAction {
                symbol: symbol.clone(),
                action,
                absolute,
            });
    }
    let mut staged = Vec::with_capacity(by_file.len());
    for (path, mut actions) in by_file {
        for left in 0..actions.len() {
            for right in (left + 1)..actions.len() {
                let a = &actions[left].symbol;
                let b = &actions[right].symbol;
                if a.effective_start() < b.byte_range.1 && b.effective_start() < a.byte_range.1 {
                    return Err(EditErrorKind::WriteConflict);
                }
            }
        }
        actions.sort_by(|left, right| {
            right
                .symbol
                .effective_start()
                .cmp(&left.symbol.effective_start())
        });
        let file = snapshot
            .generation
            .live
            .get_file(&path)
            .ok_or(EditErrorKind::FileNotAdmitted)?;
        let line_ending = detect_line_ending(&file.content);
        let footprints = actions
            .iter()
            .map(|resolved| {
                Ok(match resolved.action {
                    BatchEditAction::Insert(request) => {
                        SpliceFootprint::Insert(match request.position {
                            crate::embed::parity::edit::InsertPosition::Before => {
                                insert_before_position(&file.content, &resolved.symbol)
                            }
                            crate::embed::parity::edit::InsertPosition::After => {
                                insert_after_position(&file.content, &resolved.symbol)
                            }
                        })
                    }
                    BatchEditAction::Delete(_) => {
                        let (start, end) =
                            delete_source_range(&file.content, &resolved.symbol, line_ending);
                        SpliceFootprint::Bytes(start, end)
                    }
                    BatchEditAction::Replace(request) => {
                        let (start, end) = replacement_splice_range(
                            &file.content,
                            &resolved.symbol,
                            &request.new_body,
                        )
                        .map_err(|_| EditErrorKind::StaleContent)?;
                        SpliceFootprint::Bytes(start, end)
                    }
                    BatchEditAction::EditWithin(_) => SpliceFootprint::Bytes(
                        resolved.symbol.effective_start(),
                        resolved.symbol.byte_range.1,
                    ),
                })
            })
            .collect::<Result<Vec<_>, EditErrorKind>>()?;
        for left in 0..footprints.len() {
            for right in (left + 1)..footprints.len() {
                if splice_footprints_overlap(footprints[left], footprints[right]) {
                    return Err(EditErrorKind::WriteConflict);
                }
            }
        }
        for (index, resolved) in actions.iter().enumerate() {
            if !matches!(resolved.action, BatchEditAction::Delete(_)) {
                continue;
            }
            let cleanup = delete_cleanup_ranges(&file.content, &resolved.symbol, line_ending);
            if footprints.iter().enumerate().any(|(other, footprint)| {
                other != index && cleanup_invalidates_splice(&cleanup, *footprint)
            }) {
                return Err(EditErrorKind::WriteConflict);
            }
        }
        let mut content = file.content.clone();
        for resolved in &actions {
            content = match resolved.action {
                BatchEditAction::Replace(request) => {
                    prepare_replace(&content, &resolved.symbol, &request.new_body)
                        .map_err(|_| EditErrorKind::StaleContent)?
                        .new_content
                }
                BatchEditAction::Insert(request) => {
                    let line_ending = detect_line_ending(&content);
                    match request.position {
                        crate::embed::parity::edit::InsertPosition::Before => build_insert_before(
                            &content,
                            &resolved.symbol,
                            &request.content,
                            line_ending,
                        ),
                        crate::embed::parity::edit::InsertPosition::After => build_insert_after(
                            &content,
                            &resolved.symbol,
                            &request.content,
                            line_ending,
                        ),
                    }
                }
                BatchEditAction::Delete(_) => {
                    build_delete(&content, &resolved.symbol, detect_line_ending(&content))
                }
                BatchEditAction::EditWithin(request) => {
                    let selection = select_edit_within_body(
                        &content,
                        &resolved.symbol,
                        &request.old_text,
                        &request.new_text,
                        request.replace_all,
                        request.occurrence,
                        request.near_line,
                    )
                    .map_err(|error| match error {
                        WithinSelectionError::InvalidSpan | WithinSelectionError::InvalidUtf8 => {
                            EditErrorKind::StaleContent
                        }
                        WithinSelectionError::ConflictingTargeting => {
                            EditErrorKind::ConflictingTargeting
                        }
                        WithinSelectionError::OccurrenceOutOfRange { requested, total } => {
                            EditErrorKind::OccurrenceOutOfRange { requested, total }
                        }
                        WithinSelectionError::NotFound => EditErrorKind::TextNotFound,
                    })?;
                    apply_splice(
                        &content,
                        (
                            resolved.symbol.effective_start(),
                            resolved.symbol.byte_range.1,
                        ),
                        selection.new_body.as_bytes(),
                    )
                }
            };
        }
        let targets =
            crate::domain::IndexTargets::for_path(&file.relative_path, Some(&file.language));
        if !matches!(
            classify_stable_content_for_root(
                &snapshot.root,
                &file.relative_path,
                targets,
                &content,
            ),
            StableContentAdmission::Admitted
        ) {
            return Err(EditErrorKind::UnsafeContent);
        }
        staged.push(StagedImage {
            relative: PathBuf::from(path),
            absolute: actions[0].absolute.clone(),
            original: Some(file.content.clone()),
            replacement: content,
            owner_only: false,
        });
    }
    Ok(staged)
}

fn rename_plan(
    snapshot: &EmbeddedQuerySnapshot,
    guard: &crate::embed::parity::edit::EditGuard,
    code_only: bool,
) -> Result<crate::edit_safety::rename::RenamePlan, EditErrorKind> {
    let (file, symbol, _) = validate_guard(snapshot, guard)?;
    build_rename_plan(
        &snapshot.generation.live,
        file,
        symbol,
        &RenamePlanInput {
            path: guard.path.clone(),
            name: guard.name.clone(),
            code_only,
        },
    )
    .map_err(|_| EditErrorKind::StaleContent)
}

fn stage_rename(
    snapshot: &EmbeddedQuerySnapshot,
    request: &BatchRenameRequest,
) -> Result<(Vec<StagedImage>, crate::edit_safety::rename::RenamePlan), EditErrorKind> {
    if request.new_name.is_empty()
        || request.new_name.len() > crate::domain::index::METADATA_ONLY_CODE_BYTES as usize
    {
        return Err(EditErrorKind::InvalidReplacement);
    }
    let plan = rename_plan(snapshot, &request.plan.guard, request.plan.code_only)?;
    let by_file = plan
        .by_file
        .iter()
        .map(|(path, ranges)| (path.clone(), ranges.clone()))
        .collect::<BTreeMap<_, _>>();
    let paths = by_file.keys().cloned().collect::<Vec<_>>();
    let sites = by_file.values().map(Vec::len).sum::<usize>();
    if paths != request.plan.affected_paths
        || sites != request.plan.confident_sites
        || plan.uncertain_lines.len() != request.plan.uncertain_sites
    {
        return Err(EditErrorKind::WriteConflict);
    }
    let mut staged = Vec::with_capacity(paths.len());
    for (path, ranges) in by_file {
        let file = snapshot
            .generation
            .live
            .get_file(&path)
            .ok_or(EditErrorKind::FileNotAdmitted)?;
        if !admitted(snapshot, file) {
            return Err(EditErrorKind::UnsafeContent);
        }
        let absolute = crate::discovery::resolve_repo_path(&snapshot.root, &path)
            .map_err(|_| EditErrorKind::InvalidPath)?
            .ok_or(EditErrorKind::FileNotAdmitted)?;
        let mut replacement = file.content.clone();
        for range in ranges {
            replacement = apply_splice(&replacement, range, request.new_name.as_bytes());
        }
        let targets =
            crate::domain::IndexTargets::for_path(&file.relative_path, Some(&file.language));
        if !matches!(
            classify_stable_content_for_root(
                &snapshot.root,
                &file.relative_path,
                targets,
                &replacement,
            ),
            StableContentAdmission::Admitted
        ) {
            return Err(EditErrorKind::UnsafeContent);
        }
        staged.push(StagedImage {
            relative: PathBuf::from(path),
            absolute,
            original: Some(file.content.clone()),
            replacement,
            owner_only: false,
        });
    }
    Ok((staged, plan))
}

fn manifest(
    staged: &[StagedImage],
    authority: &super::activation::ProjectSourceAuthority,
) -> Result<Vec<u8>, EditErrorKind> {
    serde_json::to_vec(&manifest_entries(staged, authority)?)
        .map_err(|_| EditErrorKind::ReplayUnavailable)
}

fn manifest_entries(
    staged: &[StagedImage],
    authority: &super::activation::ProjectSourceAuthority,
) -> Result<Vec<(String, String)>, EditErrorKind> {
    let mut entries = Vec::with_capacity(staged.len());
    for image in staged {
        let digest = crate::idempotency::post_image_digest_beneath(authority, &image.relative)
            .ok_or(EditErrorKind::WriteUncertain)?
            .ok_or(EditErrorKind::WriteUncertain)?;
        entries.push((image.relative.to_string_lossy().to_string(), digest));
    }
    Ok(entries)
}

type PathManifest = (Vec<u8>, Vec<(String, String)>);

fn current_manifest_for_paths(
    root: &std::path::Path,
    paths: &[String],
    authority: &super::activation::ProjectSourceAuthority,
) -> Result<PathManifest, EditErrorKind> {
    if paths.is_empty() {
        return Err(EditErrorKind::ReplayConflict);
    }
    let mut files = BTreeMap::new();
    for path in paths {
        let absolute = crate::discovery::resolve_repo_path(root, path)
            .map_err(|_| EditErrorKind::ReplayConflict)?
            .ok_or(EditErrorKind::ReplayConflict)?;
        if files.insert(path.clone(), absolute).is_some() {
            return Err(EditErrorKind::ReplayConflict);
        }
    }
    let images = files
        .into_iter()
        .map(|(path, absolute)| StagedImage {
            relative: PathBuf::from(path),
            absolute,
            original: None,
            replacement: Vec::new(),
            owner_only: false,
        })
        .collect::<Vec<_>>();
    with_staged_locks(&images, |_| {
        let mut entries = Vec::with_capacity(images.len());
        for image in &images {
            let digest = crate::idempotency::post_image_digest_beneath(authority, &image.relative)
                .ok_or(EditErrorKind::ReplayConflict)?
                .ok_or(EditErrorKind::ReplayConflict)?;
            entries.push((image.relative.to_string_lossy().to_string(), digest));
        }
        let manifest =
            serde_json::to_vec(&entries).map_err(|_| EditErrorKind::ReplayUnavailable)?;
        Ok((manifest, entries))
    })
    .map_err(|_| EditErrorKind::ReplayConflict)?
}

fn edit_paths(
    root: &std::path::Path,
    request: &BatchEditRequest,
) -> Result<Vec<String>, EditErrorKind> {
    if request.actions.is_empty() || request.actions.len() > 100 {
        return Err(EditErrorKind::InvalidReplacement);
    }
    let mut paths = std::collections::BTreeSet::new();
    for action in &request.actions {
        let guard = action.guard();
        if guard.root != root {
            return Err(EditErrorKind::InvalidPath);
        }
        paths.insert(guard.path.clone());
    }
    Ok(paths.into_iter().collect())
}

fn request_fingerprint(
    request: &BatchEditRequest,
    authority: &EditApplyAuthority,
) -> Result<RequestFingerprint, EditErrorKind> {
    RequestFingerprint::for_json(
        "embed_batch_edit_v1",
        &serde_json::json!({"scope": authority.scope, "actions": action_rows(request)?}),
    )
    .map_err(|_| EditErrorKind::ReplayUnavailable)
}

fn action_rows(request: &BatchEditRequest) -> Result<Vec<serde_json::Value>, EditErrorKind> {
    let mut actions = Vec::with_capacity(request.actions.len());
    for action in &request.actions {
        let guard = action.guard();
        let (operation, payload) = match action {
            BatchEditAction::Replace(request) => {
                ("replace", digest_hex(request.new_body.as_bytes()))
            }
            BatchEditAction::Insert(request) => (
                match request.position {
                    crate::embed::parity::edit::InsertPosition::Before => "insert_before",
                    crate::embed::parity::edit::InsertPosition::After => "insert_after",
                },
                digest_hex(request.content.as_bytes()),
            ),
            BatchEditAction::Delete(_) => ("delete", String::new()),
            BatchEditAction::EditWithin(request) => {
                let args = serde_json::json!({
                    "old_hash": digest_hex(request.old_text.as_bytes()),
                    "new_hash": digest_hex(request.new_text.as_bytes()),
                    "replace_all": request.replace_all,
                    "occurrence": request.occurrence,
                    "near_line": request.near_line,
                });
                (
                    "edit_within",
                    digest_hex(
                        &serde_json::to_vec(&args).map_err(|_| EditErrorKind::ReplayUnavailable)?,
                    ),
                )
            }
        };
        actions.push(serde_json::json!({
            "path": guard.path,
            "name": guard.name,
            "kind": guard.kind,
            "symbol_line": guard.symbol_line,
            "source_version": guard.source_version,
            "publication_identity": guard.publication_identity,
            "serving_publication_identity": guard.serving_publication_identity,
            "publication_generation": guard.publication_generation,
            "content_generation": guard.content_generation,
            "content_hash": guard.content_hash,
            "symbol_hash": guard.symbol_hash,
            "operation": operation,
            "payload_hash": payload,
        }));
    }
    Ok(actions)
}

fn rename_fingerprint(
    request: &BatchRenameRequest,
    authority: &EditApplyAuthority,
) -> Result<RequestFingerprint, EditErrorKind> {
    let guard = &request.plan.guard;
    RequestFingerprint::for_json(
        "embed_batch_rename_v1",
        &serde_json::json!({
            "scope": authority.scope,
            "path": guard.path,
            "name": guard.name,
            "kind": guard.kind,
            "symbol_line": guard.symbol_line,
            "source_version": guard.source_version,
            "publication_identity": guard.publication_identity,
            "serving_publication_identity": guard.serving_publication_identity,
            "publication_generation": guard.publication_generation,
            "content_generation": guard.content_generation,
            "content_hash": guard.content_hash,
            "symbol_hash": guard.symbol_hash,
            "new_name_hash": digest_hex(request.new_name.as_bytes()),
            "code_only": request.plan.code_only,
            "affected_paths": request.plan.affected_paths,
            "confident_sites": request.plan.confident_sites,
            "uncertain_sites": request.plan.uncertain_sites,
        }),
    )
    .map_err(|_| EditErrorKind::ReplayUnavailable)
}

impl EmbeddedSourceHandle {
    pub fn plan_batch_rename(
        &self,
        target: &EditTarget,
        code_only: bool,
    ) -> Result<BatchRenamePlan, EditError> {
        let edit_plan = self.edit_plan(target)?;
        let snapshot = self.capture_query_snapshot(b"plan-batch-rename")?;
        let plan = rename_plan(&snapshot, &edit_plan.guard, code_only).map_err(EditError::Edit)?;
        let by_file = plan.by_file.into_iter().collect::<BTreeMap<_, _>>();
        Ok(BatchRenamePlan {
            guard: edit_plan.guard,
            code_only,
            affected_paths: by_file.keys().cloned().collect(),
            confident_sites: by_file.values().map(Vec::len).sum(),
            uncertain_sites: plan.uncertain_lines.len(),
        })
    }

    pub fn preview_batch_rename(
        &self,
        request: &BatchRenameRequest,
    ) -> Result<BatchRenamePreview, EditError> {
        let snapshot = self.capture_query_snapshot(b"preview-batch-rename")?;
        let (staged, plan) = stage_rename(&snapshot, request).map_err(EditError::Edit)?;
        let mut rendered = crate::edit_safety::rename::render_rename_preview(
            &snapshot.generation.live,
            &plan,
            &request.plan.guard.name,
            &request.new_name,
        );
        const MAX_PREVIEW_BYTES: usize = 1_048_576;
        let truncated = rendered.len() > MAX_PREVIEW_BYTES;
        if truncated {
            let mut boundary = MAX_PREVIEW_BYTES;
            while !rendered.is_char_boundary(boundary) {
                boundary -= 1;
            }
            rendered.truncate(boundary);
            rendered.push_str("\n… preview truncated; hashes cover the complete staged rename");
        }
        if crate::knowledge::guard_query(&rendered).is_err() {
            return Err(EditError::Edit(EditErrorKind::UnsafeContent));
        }
        let diff = render_safe_batch(staged.iter().map(|image| {
            (
                image.relative.to_str().expect("indexed UTF-8 path"),
                image.original.as_deref().expect("indexed source"),
                image.replacement.as_slice(),
            )
        }))
        .map_err(|_| EditError::Edit(EditErrorKind::UnsafeContent))?;
        let changes = BatchEditPreview {
            files: staged
                .iter()
                .map(|image| BatchFilePreview {
                    path: image.relative.to_string_lossy().to_string(),
                    old_file_hash: digest_hex(image.original.as_deref().expect("indexed source")),
                    proposed_file_hash: digest_hex(&image.replacement),
                })
                .collect(),
            action_count: request.plan.confident_sites,
            publication_identity: snapshot.publication_identity,
            source_version: snapshot.source_version,
            rendered: diff.rendered,
            truncated: diff.truncated,
            redacted: diff.redacted,
        };
        Ok(BatchRenamePreview {
            changes,
            rendered,
            truncated,
        })
    }

    pub fn apply_batch_rename(
        &self,
        request: &BatchRenameRequest,
        authority: &EditApplyAuthority,
        operation_key: &str,
    ) -> Result<BatchEditApplied, EditError> {
        let snapshot = self.capture_query_snapshot(b"apply-batch-rename")?;
        if request.plan.guard.root != snapshot.root || request.plan.affected_paths.is_empty() {
            return Err(EditError::Edit(EditErrorKind::InvalidPath));
        }
        let fingerprint = rename_fingerprint(request, authority).map_err(EditError::Edit)?;
        self.apply_staged_batch(
            snapshot,
            authority,
            operation_key,
            fingerprint,
            request.plan.affected_paths.clone(),
            |snapshot| stage_rename(snapshot, request).map(|(staged, _)| staged),
        )
    }

    pub fn preview_batch_insert(
        &self,
        request: &BatchInsertRequest,
    ) -> Result<BatchEditPreview, EditError> {
        self.preview_batch_edit(&batch_insert_as_edits(request).map_err(EditError::Edit)?)
    }

    pub fn apply_batch_insert(
        &self,
        request: &BatchInsertRequest,
        authority: &EditApplyAuthority,
        operation_key: &str,
    ) -> Result<BatchEditApplied, EditError> {
        self.apply_batch_edit(
            &batch_insert_as_edits(request).map_err(EditError::Edit)?,
            authority,
            operation_key,
        )
    }

    pub fn preview_batch_edit(
        &self,
        request: &BatchEditRequest,
    ) -> Result<BatchEditPreview, EditError> {
        let snapshot = self.capture_query_snapshot(b"preview-batch-edit")?;
        let staged = stage_batch(&snapshot, request).map_err(EditError::Edit)?;
        let diff = render_safe_batch(staged.iter().map(|image| {
            (
                image.relative.to_str().expect("indexed UTF-8 path"),
                image.original.as_deref().expect("indexed source"),
                image.replacement.as_slice(),
            )
        }))
        .map_err(|_| EditError::Edit(EditErrorKind::UnsafeContent))?;
        Ok(BatchEditPreview {
            files: staged
                .iter()
                .map(|image| BatchFilePreview {
                    path: image.relative.to_string_lossy().to_string(),
                    old_file_hash: digest_hex(image.original.as_deref().expect("indexed source")),
                    proposed_file_hash: digest_hex(&image.replacement),
                })
                .collect(),
            action_count: request.actions.len(),
            publication_identity: snapshot.publication_identity,
            source_version: snapshot.source_version,
            rendered: diff.rendered,
            truncated: diff.truncated,
            redacted: diff.redacted,
        })
    }

    pub fn apply_batch_edit(
        &self,
        request: &BatchEditRequest,
        authority: &EditApplyAuthority,
        operation_key: &str,
    ) -> Result<BatchEditApplied, EditError> {
        let snapshot = self.capture_query_snapshot(b"apply-batch-edit")?;
        let replay_paths = edit_paths(&snapshot.root, request).map_err(EditError::Edit)?;
        let fingerprint = request_fingerprint(request, authority).map_err(EditError::Edit)?;
        self.apply_staged_batch(
            snapshot,
            authority,
            operation_key,
            fingerprint,
            replay_paths,
            |snapshot| stage_batch(snapshot, request),
        )
    }

    fn apply_staged_batch(
        &self,
        snapshot: EmbeddedQuerySnapshot,
        authority: &EditApplyAuthority,
        operation_key: &str,
        fingerprint: RequestFingerprint,
        replay_paths: Vec<String>,
        stage: impl FnOnce(&EmbeddedQuerySnapshot) -> Result<Vec<StagedImage>, EditErrorKind>,
    ) -> Result<BatchEditApplied, EditError> {
        if authority.root != snapshot.root || authority.cancel.load(Ordering::Acquire) {
            return Err(EditError::Edit(EditErrorKind::WriteAuthorityRefused));
        }
        let state_dir = snapshot
            .state_dir
            .as_deref()
            .ok_or(EditError::Edit(EditErrorKind::ReplayUnavailable))?;
        let key = ReplayKey::new(operation_key)
            .map_err(|_| EditError::Edit(EditErrorKind::ReplayUnavailable))?;
        let anchor = snapshot
            .authority
            .physical_root_stable_key()
            .ok_or(EditError::Edit(EditErrorKind::ReplayUnavailable))?;
        let replay = ReplayStore::open_bound(&snapshot.root, state_dir, &authority.scope, anchor)
            .map_err(|_| EditError::Edit(EditErrorKind::ReplayUnavailable))?;
        let lease = match replay
            .reserve(&key, &fingerprint)
            .map_err(|_| EditError::Edit(EditErrorKind::ReplayConflict))?
        {
            ReserveOutcome::Acquired(lease) => lease,
            ReserveOutcome::Existing(record) => {
                if record.state != ReplayState::Completed
                    || record
                        .outcome
                        .as_ref()
                        .is_none_or(|outcome| outcome.kind != OutcomeKind::Applied)
                {
                    return Err(EditError::Edit(EditErrorKind::ReplayConflict));
                }
                let (current, files) =
                    current_manifest_for_paths(&snapshot.root, &replay_paths, &snapshot.authority)
                        .map_err(EditError::Edit)?;
                if !record.matches_post_image(&current) {
                    return Err(EditError::Edit(EditErrorKind::ReplayConflict));
                }
                return Ok(BatchEditApplied {
                    files,
                    replayed: true,
                    refresh_ticket_identity: None,
                });
            }
        };
        let staged = match stage(&snapshot) {
            Ok(staged) => staged,
            Err(error) => {
                let _ = replay.release_not_started(&lease);
                return Err(EditError::Edit(error));
            }
        };
        if authority.cancel.load(Ordering::Acquire) {
            let _ = replay.release_not_started(&lease);
            return Err(EditError::Edit(EditErrorKind::Cancelled));
        }
        if replay.mark_started(&lease).is_err() {
            let _ = replay.release_not_started(&lease);
            return Err(EditError::Edit(EditErrorKind::ReplayUnavailable));
        }
        with_staged_locks(&staged, |order| {
            let source_authority = &snapshot.authority;
            let write =
                match source_authority.acquire_write_expected(snapshot.authority_publication) {
                    Ok(write) => write,
                    Err(_) => {
                        let _ = replay.mark_uncertain(&lease);
                        return Err(EditError::Edit(EditErrorKind::WriteUncertain));
                    }
                };
            let mut io = EmbeddedBatchIo::new(write);
            let committed = commit_staged_locked(&staged, order, &mut io, Some(&authority.cancel));
            match committed {
                Ok(_) => {
                    if io.finish().is_err() {
                        let _ = replay.mark_uncertain(&lease);
                        return Err(EditError::Edit(EditErrorKind::WriteUncertain));
                    }
                    let refresh = self.request_refresh();
                    if authority.cancel.load(Ordering::Acquire) || refresh.is_err() {
                        let _ = replay.mark_uncertain(&lease);
                        return Err(EditError::Edit(EditErrorKind::WriteUncertain));
                    }
                    let post = manifest(&staged, &snapshot.authority).map_err(|_| {
                        let _ = replay.mark_uncertain(&lease);
                        EditError::Edit(EditErrorKind::WriteUncertain)
                    })?;
                    let outcome = ReplayOutcome::from_response_and_post_image(
                        OutcomeKind::Applied,
                        b"batch_applied",
                        &post,
                    )
                    .map_err(|_| {
                        let _ = replay.mark_uncertain(&lease);
                        EditError::Edit(EditErrorKind::WriteUncertain)
                    })?;
                    replay.complete(&lease, &outcome).map_err(|_| {
                        let _ = replay.mark_uncertain(&lease);
                        EditError::Edit(EditErrorKind::WriteUncertain)
                    })?;
                    snapshot.record_commitment(
                        &staged
                            .iter()
                            .map(|image| image.relative.clone())
                            .collect::<Vec<_>>(),
                    );
                    Ok(BatchEditApplied {
                        files: staged
                            .iter()
                            .map(|image| {
                                (
                                    image.relative.to_string_lossy().to_string(),
                                    digest_hex(&image.replacement),
                                )
                            })
                            .collect(),
                        replayed: false,
                        refresh_ticket_identity: Some(
                            refresh
                                .expect("checked refresh")
                                .ticket_identity()
                                .to_owned(),
                        ),
                    })
                }
                Err(abort) => {
                    let no_source_write = abort.no_source_write();
                    let uncertain = abort.rollback_uncertain();
                    if uncertain || io.finish().is_err() {
                        let _ = replay.mark_uncertain(&lease);
                        return Err(EditError::Edit(EditErrorKind::WriteUncertain));
                    }
                    let _ = self.request_refresh();
                    let current = manifest(&staged, &snapshot.authority).map_err(|_| {
                        let _ = replay.mark_uncertain(&lease);
                        EditError::Edit(EditErrorKind::WriteUncertain)
                    })?;
                    let outcome = ReplayOutcome::from_response_and_post_image(
                        OutcomeKind::Rejected,
                        b"batch_rejected",
                        &current,
                    )
                    .map_err(|_| {
                        let _ = replay.mark_uncertain(&lease);
                        EditError::Edit(EditErrorKind::WriteUncertain)
                    })?;
                    replay.fail(&lease, &outcome).map_err(|_| {
                        let _ = replay.mark_uncertain(&lease);
                        EditError::Edit(EditErrorKind::WriteUncertain)
                    })?;
                    Err(EditError::Edit(if no_source_write {
                        EditErrorKind::WriteConflict
                    } else {
                        match abort {
                            BatchAbort::Cancelled { .. } => EditErrorKind::Cancelled,
                            _ => EditErrorKind::WriteConflict,
                        }
                    }))
                }
            }
        })
        .map_err(|_| {
            let _ = replay.mark_uncertain(&lease);
            EditError::Edit(EditErrorKind::WriteUncertain)
        })?
    }
}

/// The shared kernel's I/O over several admitted sources: every staged image
/// is checked and written through the write authority of its own source.
struct RoutedBatchIo {
    ios: Vec<EmbeddedBatchIo>,
    owner: HashMap<PathBuf, usize>,
}

impl RoutedBatchIo {
    fn io(&mut self, image: &StagedImage) -> Result<&mut EmbeddedBatchIo, String> {
        let index = *self
            .owner
            .get(&image.absolute)
            .ok_or_else(|| "batch_image_unrouted".to_string())?;
        Ok(&mut self.ios[index])
    }

    /// Release every source's write authority; one failure leaves the batch
    /// uncertain, but every authority is still released.
    fn finish(self) -> Result<(), String> {
        let mut result = Ok(());
        for io in self.ios {
            if let Err(error) = io.finish() {
                result = Err(error);
            }
        }
        result
    }
}

impl BatchIo for RoutedBatchIo {
    type Report = ();

    fn matches(&mut self, image: &StagedImage, expected: Option<&[u8]>) -> Result<bool, String> {
        self.io(image)?.matches(image, expected)
    }

    fn write(&mut self, image: &StagedImage, bytes: &[u8]) -> Result<Self::Report, String> {
        self.io(image)?.write(image, bytes)
    }
}

/// One replay manifest across sources: each part's `(path, digest)` entries
/// under its root, in request order.
type RoutedManifest = Vec<(String, Vec<(String, String)>)>;

fn routed_manifest(entries: &[(String, Vec<(String, String)>)]) -> Result<Vec<u8>, EditErrorKind> {
    serde_json::to_vec(entries).map_err(|_| EditErrorKind::ReplayUnavailable)
}

impl EmbeddedSourceHandle {
    /// MCP `batch_edit` whose per-action `working_directory` routes files into
    /// different admitted worktrees. Like MCP's `execute_batch_edit`, every
    /// part is staged first and committed by one `commit_staged` run: all
    /// targets in all sources are locked in canonical order and every
    /// pre-image is verified before any write; a failed write rolls back
    /// across every source. Each source writes through its own write
    /// authority. The replay record lives in this (bound) source's store under
    /// `authority`, as MCP's lives in the indexed project's; its post-image
    /// covers every part and a retry verifies each part through that part's
    /// own source authority.
    pub fn apply_routed_batch_edit(
        &self,
        authority: &EditApplyAuthority,
        parts: &[RoutedBatchPart<'_>],
        operation_key: &str,
    ) -> Result<RoutedBatchApplied, EditError> {
        let bound = self.capture_query_snapshot(b"apply-routed-batch-edit")?;
        if authority.root != bound.root || authority.cancel.load(Ordering::Acquire) {
            return Err(EditError::Edit(EditErrorKind::WriteAuthorityRefused));
        }
        let action_count: usize = parts.iter().map(|part| part.request.actions.len()).sum();
        if parts.is_empty() || action_count == 0 || action_count > 100 {
            return Err(EditError::Edit(EditErrorKind::InvalidReplacement));
        }
        let mut snapshots: Vec<EmbeddedQuerySnapshot> = Vec::with_capacity(parts.len());
        let mut replay_paths = Vec::with_capacity(parts.len());
        let mut rows = Vec::with_capacity(parts.len());
        for part in parts {
            let snapshot = part
                .handle
                .capture_query_snapshot(b"apply-routed-batch-part")?;
            if part.authority.root != snapshot.root || part.authority.cancel.load(Ordering::Acquire)
            {
                return Err(EditError::Edit(EditErrorKind::WriteAuthorityRefused));
            }
            if snapshot.root != bound.root
                && !super::embed_route::same_repository(&bound, &snapshot)
            {
                return Err(EditError::Edit(EditErrorKind::WorkingDirectoryNotAWorktree));
            }
            if snapshots.iter().any(|other| other.root == snapshot.root) {
                return Err(EditError::Edit(EditErrorKind::InvalidPath));
            }
            replay_paths.push(edit_paths(&snapshot.root, &part.request).map_err(EditError::Edit)?);
            rows.push(serde_json::json!({
                "root": snapshot.root.to_string_lossy(),
                "scope": part.authority.scope,
                "actions": action_rows(&part.request).map_err(EditError::Edit)?,
            }));
            snapshots.push(snapshot);
        }
        let fingerprint = RequestFingerprint::for_json(
            "embed_routed_batch_edit_v1",
            &serde_json::json!({"scope": authority.scope, "parts": rows}),
        )
        .map_err(|_| EditError::Edit(EditErrorKind::ReplayUnavailable))?;
        let state_dir = bound
            .state_dir
            .as_deref()
            .ok_or(EditError::Edit(EditErrorKind::ReplayUnavailable))?;
        let key = ReplayKey::new(operation_key)
            .map_err(|_| EditError::Edit(EditErrorKind::ReplayUnavailable))?;
        let anchor = bound
            .authority
            .physical_root_stable_key()
            .ok_or(EditError::Edit(EditErrorKind::ReplayUnavailable))?;
        let replay = ReplayStore::open_bound(&bound.root, state_dir, &authority.scope, anchor)
            .map_err(|_| EditError::Edit(EditErrorKind::ReplayUnavailable))?;
        let root_key =
            |snapshot: &EmbeddedQuerySnapshot| snapshot.root.to_string_lossy().into_owned();
        let lease = match replay
            .reserve(&key, &fingerprint)
            .map_err(|_| EditError::Edit(EditErrorKind::ReplayConflict))?
        {
            ReserveOutcome::Acquired(lease) => lease,
            ReserveOutcome::Existing(record) => {
                if record.state != ReplayState::Completed
                    || record
                        .outcome
                        .as_ref()
                        .is_none_or(|outcome| outcome.kind != OutcomeKind::Applied)
                {
                    return Err(EditError::Edit(EditErrorKind::ReplayConflict));
                }
                let mut entries = Vec::with_capacity(snapshots.len());
                for (snapshot, paths) in snapshots.iter().zip(&replay_paths) {
                    let (_, files) =
                        current_manifest_for_paths(&snapshot.root, paths, &snapshot.authority)
                            .map_err(EditError::Edit)?;
                    entries.push((root_key(snapshot), files));
                }
                if !record.matches_post_image(&routed_manifest(&entries).map_err(EditError::Edit)?)
                {
                    return Err(EditError::Edit(EditErrorKind::ReplayConflict));
                }
                return Ok(RoutedBatchApplied {
                    parts: entries
                        .into_iter()
                        .map(|(_, files)| BatchEditApplied {
                            files,
                            replayed: true,
                            refresh_ticket_identity: None,
                        })
                        .collect(),
                    replayed: true,
                });
            }
        };
        let mut staged = Vec::new();
        let mut ranges: Vec<Range<usize>> = Vec::with_capacity(parts.len());
        let mut owner = HashMap::new();
        for (index, (part, snapshot)) in parts.iter().zip(&snapshots).enumerate() {
            let images = match stage_batch(snapshot, &part.request) {
                Ok(images) => images,
                Err(error) => {
                    let _ = replay.release_not_started(&lease);
                    return Err(EditError::Edit(error));
                }
            };
            let start = staged.len();
            for image in images {
                owner.insert(image.absolute.clone(), index);
                staged.push(image);
            }
            ranges.push(start..staged.len());
        }
        if authority.cancel.load(Ordering::Acquire) {
            let _ = replay.release_not_started(&lease);
            return Err(EditError::Edit(EditErrorKind::Cancelled));
        }
        if replay.mark_started(&lease).is_err() {
            let _ = replay.release_not_started(&lease);
            return Err(EditError::Edit(EditErrorKind::ReplayUnavailable));
        }
        let uncertain = || {
            let _ = replay.mark_uncertain(&lease);
            EditError::Edit(EditErrorKind::WriteUncertain)
        };
        let current_entries = || -> Result<RoutedManifest, EditError> {
            snapshots
                .iter()
                .zip(&ranges)
                .map(|(snapshot, range)| {
                    manifest_entries(&staged[range.clone()], &snapshot.authority)
                        .map(|files| (root_key(snapshot), files))
                        .map_err(|_| uncertain())
                })
                .collect()
        };
        with_staged_locks(&staged, |order| {
            // Each source's mutation permit after the path locks, as a single
            // source takes its one permit; sources in root order.
            let mut by_root: Vec<usize> = (0..snapshots.len()).collect();
            by_root.sort_by(|left, right| snapshots[*left].root.cmp(&snapshots[*right].root));
            let mut writes: Vec<Option<EmbeddedBatchIo>> =
                (0..snapshots.len()).map(|_| None).collect();
            for index in by_root {
                let snapshot = &snapshots[index];
                match snapshot
                    .authority
                    .acquire_write_expected(snapshot.authority_publication)
                {
                    Ok(write) => writes[index] = Some(EmbeddedBatchIo::new(write)),
                    Err(_) => {
                        for io in writes.into_iter().flatten() {
                            let _ = io.finish();
                        }
                        return Err(uncertain());
                    }
                }
            }
            let mut io = RoutedBatchIo {
                ios: writes.into_iter().flatten().collect(),
                owner,
            };
            match commit_staged_locked(&staged, order, &mut io, Some(&authority.cancel)) {
                Ok(_) => {
                    if io.finish().is_err() {
                        return Err(uncertain());
                    }
                    let mut tickets = Vec::with_capacity(parts.len());
                    for part in parts {
                        match part.handle.request_refresh() {
                            Ok(ticket) => tickets.push(ticket.ticket_identity().to_owned()),
                            Err(_) => return Err(uncertain()),
                        }
                    }
                    if authority.cancel.load(Ordering::Acquire) {
                        return Err(uncertain());
                    }
                    let post = current_entries()?;
                    let outcome = ReplayOutcome::from_response_and_post_image(
                        OutcomeKind::Applied,
                        b"routed_batch_applied",
                        &routed_manifest(&post).map_err(|_| uncertain())?,
                    )
                    .map_err(|_| uncertain())?;
                    replay.complete(&lease, &outcome).map_err(|_| uncertain())?;
                    for (snapshot, range) in snapshots.iter().zip(&ranges) {
                        snapshot.record_commitment(
                            &staged[range.clone()]
                                .iter()
                                .map(|image| image.relative.clone())
                                .collect::<Vec<_>>(),
                        );
                    }
                    Ok(RoutedBatchApplied {
                        parts: ranges
                            .iter()
                            .zip(tickets)
                            .map(|(range, ticket)| BatchEditApplied {
                                files: staged[range.clone()]
                                    .iter()
                                    .map(|image| {
                                        (
                                            image.relative.to_string_lossy().to_string(),
                                            digest_hex(&image.replacement),
                                        )
                                    })
                                    .collect(),
                                replayed: false,
                                refresh_ticket_identity: Some(ticket),
                            })
                            .collect(),
                        replayed: false,
                    })
                }
                Err(abort) => {
                    let no_source_write = abort.no_source_write();
                    if abort.rollback_uncertain() || io.finish().is_err() {
                        return Err(uncertain());
                    }
                    for part in parts {
                        let _ = part.handle.request_refresh();
                    }
                    let current = current_entries()?;
                    let outcome = ReplayOutcome::from_response_and_post_image(
                        OutcomeKind::Rejected,
                        b"routed_batch_rejected",
                        &routed_manifest(&current).map_err(|_| uncertain())?,
                    )
                    .map_err(|_| uncertain())?;
                    replay.fail(&lease, &outcome).map_err(|_| uncertain())?;
                    Err(EditError::Edit(if no_source_write {
                        EditErrorKind::WriteConflict
                    } else {
                        match abort {
                            BatchAbort::Cancelled { .. } => EditErrorKind::Cancelled,
                            _ => EditErrorKind::WriteConflict,
                        }
                    }))
                }
            }
        })
        .map_err(|_| uncertain())?
    }

    /// [`Self::apply_routed_batch_edit`] for MCP `batch_insert` with per-target
    /// `working_directory`: each part's insert becomes its edit actions, as
    /// `apply_batch_insert` does for one source.
    pub fn apply_routed_batch_insert(
        &self,
        authority: &EditApplyAuthority,
        parts: &[RoutedBatchPart<'_, BatchInsertRequest>],
        operation_key: &str,
    ) -> Result<RoutedBatchApplied, EditError> {
        let parts = parts
            .iter()
            .map(|part| {
                Ok(RoutedBatchPart {
                    handle: part.handle,
                    authority: part.authority,
                    request: batch_insert_as_edits(&part.request)?,
                })
            })
            .collect::<Result<Vec<_>, EditErrorKind>>()
            .map_err(EditError::Edit)?;
        self.apply_routed_batch_edit(authority, &parts, operation_key)
    }
}
fn batch_insert_as_edits(request: &BatchInsertRequest) -> Result<BatchEditRequest, EditErrorKind> {
    if request.targets.is_empty()
        || request.targets.len() > 100
        || request.content.len() > crate::domain::index::METADATA_ONLY_CODE_BYTES as usize
    {
        return Err(EditErrorKind::InvalidReplacement);
    }
    Ok(BatchEditRequest {
        actions: request
            .targets
            .iter()
            .map(|guard| {
                BatchEditAction::Insert(InsertRequest {
                    guard: guard.clone(),
                    position: request.position,
                    content: request.content.clone(),
                })
            })
            .collect(),
    })
}
