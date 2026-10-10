//! Embedded structural edits over pinned publications and the shared MCP byte
//! planner. Write authorization and durable replay are separate apply gates.

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use crate::edit_safety::atomic_write::{GuardedWriteOutcome, guarded_atomic_write_file_expected};
use crate::edit_safety::preview::{MAX_PREVIEW_BYTES, SafeDiff, render_safe_change};
use crate::edit_safety::structural::{
    PreparedReplace, WithinSelectionError, apply_splice, prepare_replace, select_edit_within_body,
};
use crate::embed::parity::edit::{
    DeleteRequest, EditApplyAuthority, EditBody, EditError, EditErrorKind, EditGuard, EditPlan,
    EditTarget, EditWithinPreview, EditWithinRequest, InsertPosition, InsertRequest,
    ReplaceApplied, ReplacePreview, ReplaceRequest, StructuralApplied, StructuralPreview,
};
use crate::embed::parity::replay::{
    OutcomeKind, ReplayKey, ReplayOutcome, ReplayState, ReplayStore, RequestFingerprint,
    ReserveOutcome,
};
use crate::hash::digest_hex;
use crate::knowledge::{StableContentAdmission, classify_stable_content_for_root};
use crate::live_index::disambiguation::{SymbolSelectorMatch, resolve_symbol_selector};
use crate::live_index::store::IndexedFile;

use super::embed_edit_body::{self as answer, EditBodyParts, SingleParts, StaleQuery};
use super::embed_query::EmbeddedQuerySnapshot;
use super::embedded::EmbeddedSourceHandle;
use super::guidance::edit_body::{self as shared, EditSafetyMode, EditWriteSemantics};

/// MCP's evidence anchor for an edit: `path:line`, one-based.
fn anchor(guard: &EditGuard) -> String {
    format!("{}:{}", guard.path, guard.symbol_line)
}

fn position_label(position: InsertPosition) -> &'static str {
    match position {
        InsertPosition::Before => "before",
        InsertPosition::After => "after",
    }
}

/// MCP's dry-run answer: envelope, summary and trust suffix.
fn preview_body(
    snapshot: &EmbeddedQuerySnapshot,
    guard: &EditGuard,
    safety: EditSafetyMode,
    summary: String,
) -> EditBody {
    EditBody::new(EditBodyParts::Single(Box::new(SingleParts {
        safety,
        semantics: EditWriteSemantics::DryRunNoWrites,
        anchor: anchor(guard),
        path: guard.path.clone(),
        summary,
        stale: None,
        stale_warnings: String::new(),
        tee: None,
        tee_before_reroute: false,
        trust: answer::trust_suffix(snapshot),
        impact: None,
    })))
}

/// What a committed single edit reports beyond its summary.
struct Committed<'a> {
    safety: EditSafetyMode,
    summary: String,
    stale: Option<StaleQuery>,
    tee: crate::edit_safety::tee::TeeSnapshot,
    tee_before_reroute: bool,
    new_content: &'a [u8],
}

/// MCP's answer for a committed single edit. The stale warnings and the
/// impact footer read the index with the post-image substituted, as MCP
/// reads its index after `reindex_after_write`.
fn applied_body(
    snapshot: &EmbeddedQuerySnapshot,
    guard: &EditGuard,
    committed: Committed<'_>,
) -> EditBody {
    let after = answer::post_edit_index(snapshot, &[(&guard.path, committed.new_content)]);
    let stale_warnings = committed
        .stale
        .as_ref()
        .map(|query| query.warnings(&after))
        .unwrap_or_default();
    EditBody::new(EditBodyParts::Single(Box::new(SingleParts {
        safety: committed.safety,
        semantics: EditWriteSemantics::AtomicWriteAndReindex,
        anchor: anchor(guard),
        path: guard.path.clone(),
        summary: committed.summary,
        stale: committed.stale,
        stale_warnings,
        tee: Some(committed.tee),
        tee_before_reroute: committed.tee_before_reroute,
        trust: answer::trust_suffix(snapshot),
        impact: Some(answer::impact_footer(snapshot, &after, &guard.path)),
    })))
}

/// The answer a lane gives once its write committed: built from the
/// snapshot it planned against, the prepared bytes and the tee hint.
type DescribeApplied<'a> = Box<
    dyn FnOnce(
            &EmbeddedQuerySnapshot,
            &PreparedReplace,
            crate::edit_safety::tee::TeeSnapshot,
        ) -> EditBody
        + 'a,
>;

pub(super) fn admitted(snapshot: &EmbeddedQuerySnapshot, file: &IndexedFile) -> bool {
    let targets = crate::domain::IndexTargets::for_path(&file.relative_path, Some(&file.language));
    matches!(
        classify_stable_content_for_root(
            &snapshot.root,
            &file.relative_path,
            targets,
            &file.content,
        ),
        StableContentAdmission::Admitted
    )
}

fn selected_symbol<'a>(
    file: &'a IndexedFile,
    name: &str,
    kind: Option<&str>,
    line: Option<u32>,
) -> Result<&'a crate::domain::SymbolRecord, EditErrorKind> {
    match resolve_symbol_selector(file, name, kind, line) {
        SymbolSelectorMatch::Selected(_, symbol) => Ok(symbol),
        SymbolSelectorMatch::NotFound => Err(EditErrorKind::SymbolNotFound),
        SymbolSelectorMatch::Ambiguous(lines) => Err(EditErrorKind::AmbiguousSymbol {
            candidate_lines: lines
                .into_iter()
                .map(|line| line.saturating_add(1))
                .collect(),
        }),
    }
}

pub(super) fn validate_guard<'a>(
    snapshot: &'a EmbeddedQuerySnapshot,
    guard: &EditGuard,
) -> Result<(&'a IndexedFile, &'a crate::domain::SymbolRecord, PathBuf), EditErrorKind> {
    if snapshot.root != guard.root
        || snapshot.source_version != guard.source_version
        || snapshot.publication_identity != guard.publication_identity
        || snapshot.serving_publication_identity != guard.serving_publication_identity
        || snapshot.generation.publication_generation != guard.publication_generation
        || snapshot.generation.content_generation != guard.content_generation
        || snapshot.authority_publication != guard.authority_publication
    {
        return Err(EditErrorKind::StaleGeneration);
    }
    let path = crate::discovery::resolve_repo_path(&snapshot.root, &guard.path)
        .map_err(|_| EditErrorKind::InvalidPath)?
        .ok_or(EditErrorKind::FileNotAdmitted)?;
    let file = snapshot
        .generation
        .live
        .get_file(&guard.path)
        .ok_or(EditErrorKind::FileNotAdmitted)?;
    if !admitted(snapshot, file) {
        return Err(EditErrorKind::UnsafeContent);
    }
    if digest_hex(&file.content) != guard.content_hash {
        return Err(EditErrorKind::StaleContent);
    }
    let symbol = selected_symbol(
        file,
        &guard.name,
        Some(&guard.kind),
        Some(guard.symbol_line),
    )?;
    let (start, end) = symbol.byte_range;
    if start > end || end as usize > file.content.len() || symbol.effective_start() > start {
        return Err(EditErrorKind::StaleContent);
    }
    let body = file
        .content
        .get(start as usize..end as usize)
        .ok_or(EditErrorKind::StaleContent)?;
    if digest_hex(body) != guard.symbol_hash {
        return Err(EditErrorKind::StaleContent);
    }
    Ok((file, symbol, path))
}

fn prepare(
    snapshot: &EmbeddedQuerySnapshot,
    request: &ReplaceRequest,
) -> Result<(PreparedReplace, PathBuf), EditErrorKind> {
    let (file, symbol, path) = validate_guard(snapshot, &request.guard)?;
    if crate::parsing::config_extractors::edit_capability_for_language(&file.language).is_some_and(
        |capability| {
            !matches!(
                capability,
                crate::parsing::config_extractors::EditCapability::StructuralEditSafe
            )
        },
    ) {
        return Err(EditErrorKind::InvalidReplacement);
    }
    let prepared = prepare_replace(&file.content, symbol, &request.new_body)
        .map_err(|_| EditErrorKind::StaleContent)?;
    let targets = crate::domain::IndexTargets::for_path(&file.relative_path, Some(&file.language));
    if !matches!(
        classify_stable_content_for_root(
            &snapshot.root,
            &file.relative_path,
            targets,
            &prepared.new_content,
        ),
        StableContentAdmission::Admitted
    ) {
        return Err(EditErrorKind::UnsafeContent);
    }
    Ok((prepared, path))
}

fn preview_diff(
    snapshot: &EmbeddedQuerySnapshot,
    path: &str,
    new_content: &[u8],
) -> Result<SafeDiff, EditError> {
    let file = snapshot
        .generation
        .live
        .get_file(path)
        .ok_or(EditError::Edit(EditErrorKind::FileNotAdmitted))?;
    render_safe_change(path, &file.content, new_content, MAX_PREVIEW_BYTES)
        .map_err(|_| EditError::Edit(EditErrorKind::UnsafeContent))
}

fn prepare_structural(
    snapshot: &EmbeddedQuerySnapshot,
    guard: &EditGuard,
    operation: StructuralOperation<'_>,
) -> Result<(PreparedReplace, PathBuf), EditErrorKind> {
    let (file, symbol, path) = validate_guard(snapshot, guard)?;
    let required = match operation {
        StructuralOperation::Insert(_, _) | StructuralOperation::Delete => {
            crate::parsing::config_extractors::EditCapability::StructuralEditSafe
        }
    };
    if crate::parsing::config_extractors::edit_capability_for_language(&file.language)
        .is_some_and(|capability| capability != required)
    {
        return Err(EditErrorKind::InvalidReplacement);
    }
    let new_content = match operation {
        StructuralOperation::Insert(InsertPosition::Before, content) => {
            crate::edit_safety::structural::build_insert_before(
                &file.content,
                symbol,
                content,
                crate::edit_safety::structural::detect_line_ending(&file.content),
            )
        }
        StructuralOperation::Insert(InsertPosition::After, content) => {
            crate::edit_safety::structural::build_insert_after(
                &file.content,
                symbol,
                content,
                crate::edit_safety::structural::detect_line_ending(&file.content),
            )
        }
        StructuralOperation::Delete => crate::edit_safety::structural::build_delete(
            &file.content,
            symbol,
            crate::edit_safety::structural::detect_line_ending(&file.content),
        ),
    };
    let targets = crate::domain::IndexTargets::for_path(&file.relative_path, Some(&file.language));
    if !matches!(
        classify_stable_content_for_root(
            &snapshot.root,
            &file.relative_path,
            targets,
            &new_content,
        ),
        StableContentAdmission::Admitted
    ) {
        return Err(EditErrorKind::UnsafeContent);
    }
    let (old_bytes, inserted_bytes) = if new_content.len() >= file.content.len() {
        (0, new_content.len() - file.content.len())
    } else {
        (file.content.len() - new_content.len(), 0)
    };
    Ok((
        PreparedReplace {
            new_content,
            old_bytes,
            inserted_bytes,
        },
        path,
    ))
}

#[derive(Clone, Copy)]
enum StructuralOperation<'a> {
    Insert(InsertPosition, &'a str),
    Delete,
}

fn prepare_within(
    snapshot: &EmbeddedQuerySnapshot,
    request: &EditWithinRequest,
) -> Result<(PreparedReplace, PathBuf, usize, usize), EditErrorKind> {
    let (file, symbol, path) = validate_guard(snapshot, &request.guard)?;
    if crate::parsing::config_extractors::edit_capability_for_language(&file.language).is_some_and(
        |capability| {
            matches!(
                capability,
                crate::parsing::config_extractors::EditCapability::IndexOnly
            )
        },
    ) {
        return Err(EditErrorKind::InvalidReplacement);
    }
    let selection = select_edit_within_body(
        &file.content,
        symbol,
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
        WithinSelectionError::ConflictingTargeting => EditErrorKind::ConflictingTargeting,
        WithinSelectionError::OccurrenceOutOfRange { requested, total } => {
            EditErrorKind::OccurrenceOutOfRange { requested, total }
        }
        WithinSelectionError::NotFound => EditErrorKind::TextNotFound,
    })?;
    let old_bytes = (symbol.byte_range.1 - symbol.effective_start()) as usize;
    let inserted_bytes = selection.new_body.len();
    let new_content = apply_splice(
        &file.content,
        (symbol.effective_start(), symbol.byte_range.1),
        selection.new_body.as_bytes(),
    );
    let targets = crate::domain::IndexTargets::for_path(&file.relative_path, Some(&file.language));
    if !matches!(
        classify_stable_content_for_root(
            &snapshot.root,
            &file.relative_path,
            targets,
            &new_content,
        ),
        StableContentAdmission::Admitted
    ) {
        return Err(EditErrorKind::UnsafeContent);
    }
    Ok((
        PreparedReplace {
            new_content,
            old_bytes,
            inserted_bytes,
        },
        path,
        selection.count,
        selection.untargeted_extra,
    ))
}

impl EmbeddedSourceHandle {
    /// Resolve one symbol with MCP's selector and mint an exact source guard.
    pub fn edit_plan(&self, target: &EditTarget) -> Result<EditPlan, EditError> {
        let normalized = format!(
            "edit-plan:{}:{}:{:?}:{:?}",
            target.path, target.name, target.kind, target.symbol_line
        );
        let snapshot = self.capture_query_snapshot(normalized.as_bytes())?;
        let _path = crate::discovery::resolve_repo_path(&snapshot.root, &target.path)
            .map_err(|_| EditError::Edit(EditErrorKind::InvalidPath))?
            .ok_or(EditError::Edit(EditErrorKind::FileNotAdmitted))?;
        let file = snapshot
            .generation
            .live
            .get_file(&target.path)
            .ok_or(EditError::Edit(EditErrorKind::FileNotAdmitted))?;
        if !admitted(&snapshot, file) {
            return Err(EditError::Edit(EditErrorKind::UnsafeContent));
        }
        let symbol = selected_symbol(
            file,
            &target.name,
            target.kind.as_deref(),
            target.symbol_line,
        )
        .map_err(EditError::Edit)?;
        let (start, end) = symbol.byte_range;
        let body = file
            .content
            .get(start as usize..end as usize)
            .ok_or(EditError::Edit(EditErrorKind::StaleContent))?;
        let guard = EditGuard {
            root: snapshot.root,
            path: target.path.clone(),
            name: symbol.name.clone(),
            kind: symbol.kind.to_string(),
            symbol_line: symbol.line_range.0.saturating_add(1),
            source_version: snapshot.source_version,
            publication_identity: snapshot.publication_identity,
            serving_publication_identity: snapshot.serving_publication_identity,
            publication_generation: snapshot.generation.publication_generation,
            content_generation: snapshot.generation.content_generation,
            authority_publication: snapshot.authority_publication,
            content_hash: digest_hex(&file.content),
            symbol_hash: digest_hex(body),
            selector: target.name.clone(),
        };
        let reference_count = snapshot
            .generation
            .live
            .find_references_for_name(&symbol.name, None, false)
            .len();
        Ok(EditPlan {
            guard,
            byte_range: symbol.byte_range,
            line_range: (
                symbol.line_range.0.saturating_add(1),
                symbol.line_range.1.saturating_add(1),
            ),
            reference_count,
        })
    }

    /// Compute the exact MCP replacement bytes without touching source or replay state.
    pub fn preview_replace(&self, request: &ReplaceRequest) -> Result<ReplacePreview, EditError> {
        let snapshot = self.capture_query_snapshot(b"preview-replace-symbol-body")?;
        let (prepared, _) = prepare(&snapshot, request).map_err(EditError::Edit)?;
        let diff = preview_diff(&snapshot, &request.guard.path, &prepared.new_content)?;
        let (_, symbol, _) = validate_guard(&snapshot, &request.guard).map_err(EditError::Edit)?;
        let body = preview_body(
            &snapshot,
            &request.guard,
            EditSafetyMode::StructuralEditSafe,
            shared::dry_run_replace_summary(
                &request.guard.selector,
                &request.guard.path,
                (symbol.byte_range.1 - symbol.byte_range.0) as usize,
                request.new_body.len(),
            ),
        );
        Ok(ReplacePreview {
            path: request.guard.path.clone(),
            source_version: snapshot.source_version,
            publication_identity: snapshot.publication_identity,
            old_file_hash: request.guard.content_hash.clone(),
            proposed_file_hash: digest_hex(&prepared.new_content),
            old_bytes: prepared.old_bytes,
            inserted_bytes: prepared.inserted_bytes,
            rendered: diff.rendered,
            truncated: diff.truncated,
            redacted: diff.redacted,
            body,
        })
    }

    pub fn preview_insert(&self, request: &InsertRequest) -> Result<StructuralPreview, EditError> {
        let snapshot = self.capture_query_snapshot(b"preview-insert-symbol")?;
        let (prepared, _) = prepare_structural(
            &snapshot,
            &request.guard,
            StructuralOperation::Insert(request.position, &request.content),
        )
        .map_err(EditError::Edit)?;
        let diff = preview_diff(&snapshot, &request.guard.path, &prepared.new_content)?;
        let body = preview_body(
            &snapshot,
            &request.guard,
            EditSafetyMode::StructuralEditSafe,
            shared::dry_run_insert_summary(
                position_label(request.position),
                &request.guard.selector,
                &request.guard.path,
                request.content.len(),
            ),
        );
        Ok(StructuralPreview {
            path: request.guard.path.clone(),
            source_version: snapshot.source_version,
            publication_identity: snapshot.publication_identity,
            old_file_hash: request.guard.content_hash.clone(),
            proposed_file_hash: digest_hex(&prepared.new_content),
            old_bytes: prepared.old_bytes,
            inserted_bytes: prepared.inserted_bytes,
            rendered: diff.rendered,
            truncated: diff.truncated,
            redacted: diff.redacted,
            body,
        })
    }

    pub fn apply_insert(
        &self,
        request: &InsertRequest,
        authority: &EditApplyAuthority,
        operation_key: &str,
    ) -> Result<StructuralApplied, EditError> {
        let operation = match request.position {
            InsertPosition::Before => "embed_insert_before_symbol_v1",
            InsertPosition::After => "embed_insert_after_symbol_v1",
        };
        self.apply_mutation(
            &request.guard,
            authority,
            operation_key,
            (
                operation,
                "insert_symbol",
                Some(("inserted_hash", digest_hex(request.content.as_bytes()))),
            ),
            |snapshot| {
                prepare_structural(
                    snapshot,
                    &request.guard,
                    StructuralOperation::Insert(request.position, &request.content),
                )
            },
            Box::new(|snapshot, prepared, tee| {
                applied_body(
                    snapshot,
                    &request.guard,
                    Committed {
                        safety: EditSafetyMode::StructuralEditSafe,
                        summary: shared::format_insert(
                            &request.guard.path,
                            &request.guard.selector,
                            position_label(request.position),
                            request.content.len(),
                        ),
                        stale: None,
                        tee,
                        tee_before_reroute: false,
                        new_content: &prepared.new_content,
                    },
                )
            }),
        )
    }

    pub fn preview_delete(&self, request: &DeleteRequest) -> Result<StructuralPreview, EditError> {
        let snapshot = self.capture_query_snapshot(b"preview-delete-symbol")?;
        let (prepared, _) =
            prepare_structural(&snapshot, &request.guard, StructuralOperation::Delete)
                .map_err(EditError::Edit)?;
        let diff = preview_diff(&snapshot, &request.guard.path, &prepared.new_content)?;
        let (_, symbol, _) = validate_guard(&snapshot, &request.guard).map_err(EditError::Edit)?;
        let body = preview_body(
            &snapshot,
            &request.guard,
            EditSafetyMode::StructuralEditSafe,
            shared::dry_run_delete_summary(
                &request.guard.selector,
                &request.guard.path,
                (symbol.byte_range.1 - symbol.byte_range.0) as usize,
            ),
        );
        Ok(StructuralPreview {
            path: request.guard.path.clone(),
            source_version: snapshot.source_version,
            publication_identity: snapshot.publication_identity,
            old_file_hash: request.guard.content_hash.clone(),
            proposed_file_hash: digest_hex(&prepared.new_content),
            old_bytes: prepared.old_bytes,
            inserted_bytes: prepared.inserted_bytes,
            rendered: diff.rendered,
            truncated: diff.truncated,
            redacted: diff.redacted,
            body,
        })
    }

    pub fn apply_delete(
        &self,
        request: &DeleteRequest,
        authority: &EditApplyAuthority,
        operation_key: &str,
    ) -> Result<StructuralApplied, EditError> {
        self.apply_mutation(
            &request.guard,
            authority,
            operation_key,
            ("embed_delete_symbol_v1", "delete_symbol", None),
            |snapshot| prepare_structural(snapshot, &request.guard, StructuralOperation::Delete),
            Box::new(|snapshot, prepared, tee| {
                let deleted = validate_guard(snapshot, &request.guard)
                    .map(|(_, symbol, _)| (symbol.byte_range.1 - symbol.byte_range.0) as usize)
                    .unwrap_or_default();
                applied_body(
                    snapshot,
                    &request.guard,
                    Committed {
                        safety: EditSafetyMode::StructuralEditSafe,
                        summary: shared::format_delete(
                            &request.guard.path,
                            &request.guard.selector,
                            &request.guard.kind,
                            deleted,
                        ),
                        stale: None,
                        tee,
                        tee_before_reroute: false,
                        new_content: &prepared.new_content,
                    },
                )
            }),
        )
    }

    pub fn preview_edit_within(
        &self,
        request: &EditWithinRequest,
    ) -> Result<EditWithinPreview, EditError> {
        let snapshot = self.capture_query_snapshot(b"preview-edit-within-symbol")?;
        let (prepared, _, replacement_count, untargeted_extra) =
            prepare_within(&snapshot, request).map_err(EditError::Edit)?;
        let diff = preview_diff(&snapshot, &request.guard.path, &prepared.new_content)?;
        let body = preview_body(
            &snapshot,
            &request.guard,
            EditSafetyMode::TextEditSafe,
            shared::edit_within_summary(
                true,
                &request.guard.path,
                &request.guard.selector,
                replacement_count,
                0,
                0,
                untargeted_extra,
            ),
        );
        Ok(EditWithinPreview {
            change: StructuralPreview {
                path: request.guard.path.clone(),
                source_version: snapshot.source_version,
                publication_identity: snapshot.publication_identity,
                old_file_hash: request.guard.content_hash.clone(),
                proposed_file_hash: digest_hex(&prepared.new_content),
                old_bytes: prepared.old_bytes,
                inserted_bytes: prepared.inserted_bytes,
                rendered: diff.rendered,
                truncated: diff.truncated,
                redacted: diff.redacted,
                body,
            },
            replacement_count,
            untargeted_extra,
        })
    }

    pub fn apply_edit_within(
        &self,
        request: &EditWithinRequest,
        authority: &EditApplyAuthority,
        operation_key: &str,
    ) -> Result<StructuralApplied, EditError> {
        let payload = serde_json::json!({
            "old_hash": digest_hex(request.old_text.as_bytes()),
            "new_hash": digest_hex(request.new_text.as_bytes()),
            "replace_all": request.replace_all,
            "occurrence": request.occurrence,
            "near_line": request.near_line,
        });
        let payload_hash = digest_hex(
            &serde_json::to_vec(&payload)
                .map_err(|_| EditError::Edit(EditErrorKind::InvalidReplacement))?,
        );
        self.apply_mutation(
            &request.guard,
            authority,
            operation_key,
            (
                "embed_edit_within_symbol_v1",
                "edit_within_symbol",
                Some(("within_request_hash", payload_hash)),
            ),
            |snapshot| {
                prepare_within(snapshot, request).map(|(prepared, path, _, _)| (prepared, path))
            },
            Box::new(|snapshot, prepared, tee| {
                let (count, extra) = prepare_within(snapshot, request)
                    .map(|(_, _, count, extra)| (count, extra))
                    .unwrap_or_default();
                applied_body(
                    snapshot,
                    &request.guard,
                    Committed {
                        safety: EditSafetyMode::TextEditSafe,
                        summary: shared::edit_within_summary(
                            false,
                            &request.guard.path,
                            &request.guard.selector,
                            count,
                            prepared.old_bytes,
                            prepared.inserted_bytes,
                            extra,
                        ),
                        stale: None,
                        tee,
                        tee_before_reroute: false,
                        new_content: &prepared.new_content,
                    },
                )
            }),
        )
    }

    /// Apply a planned replacement through the MCP byte planner and guarded
    /// writer. A host-issued authority and durable operation key are required.
    pub fn apply_replace(
        &self,
        request: &ReplaceRequest,
        authority: &EditApplyAuthority,
        operation_key: &str,
    ) -> Result<ReplaceApplied, EditError> {
        let applied = self.apply_mutation(
            &request.guard,
            authority,
            operation_key,
            (
                "embed_replace_symbol_body_v1",
                "replace_symbol_body",
                Some(("replacement_hash", digest_hex(request.new_body.as_bytes()))),
            ),
            |snapshot| prepare(snapshot, request),
            Box::new(|snapshot, prepared, tee| {
                let stale =
                    validate_guard(snapshot, &request.guard)
                        .ok()
                        .map(|(file, symbol, _)| StaleQuery {
                            path: request.guard.path.clone(),
                            name: request.guard.selector.clone(),
                            old_signature: shared::extract_signature(
                                &file.content,
                                symbol.byte_range,
                            ),
                            new_signature: request
                                .new_body
                                .lines()
                                .next()
                                .unwrap_or("")
                                .to_string(),
                            parent_type: super::guidance::file_impact::find_parent_impl_type(
                                file, symbol,
                            ),
                            language: file.language,
                        });
                applied_body(
                    snapshot,
                    &request.guard,
                    Committed {
                        safety: EditSafetyMode::StructuralEditSafe,
                        summary: shared::format_replace(
                            &request.guard.path,
                            &request.guard.selector,
                            &request.guard.kind,
                            prepared.old_bytes,
                            prepared.inserted_bytes,
                        ),
                        stale,
                        tee,
                        tee_before_reroute: true,
                        new_content: &prepared.new_content,
                    },
                )
            }),
        )?;
        Ok(ReplaceApplied {
            path: applied.path,
            post_image_hash: applied.post_image_hash,
            replayed: applied.replayed,
            refresh_ticket_identity: applied.refresh_ticket_identity,
            body: applied.body,
        })
    }

    fn apply_mutation(
        &self,
        guard: &EditGuard,
        authority: &EditApplyAuthority,
        operation_key: &str,
        // The replay operation name, the MCP tool this lane answers as, and
        // the hashed payload the replay request binds.
        (operation, tool, payload_hash): (
            &'static str,
            &'static str,
            Option<(&'static str, String)>,
        ),
        prepare: impl FnOnce(
            &EmbeddedQuerySnapshot,
        ) -> Result<(PreparedReplace, PathBuf), EditErrorKind>,
        describe: DescribeApplied<'_>,
    ) -> Result<StructuralApplied, EditError> {
        if authority.root != guard.root {
            return Err(EditError::Edit(EditErrorKind::WriteAuthorityRefused));
        }
        let snapshot = self.capture_query_snapshot(operation.as_bytes())?;
        if authority.root != snapshot.root {
            return Err(EditError::Edit(EditErrorKind::WriteAuthorityRefused));
        }
        let state_dir = snapshot
            .state_dir
            .as_deref()
            .ok_or(EditError::Edit(EditErrorKind::ReplayUnavailable))?;
        if !state_dir.is_absolute() {
            return Err(EditError::Edit(EditErrorKind::ReplayUnavailable));
        }
        crate::discovery::resolve_repo_path(&snapshot.root, &guard.path)
            .map_err(|_| EditError::Edit(EditErrorKind::InvalidPath))?
            .ok_or(EditError::Edit(EditErrorKind::FileNotAdmitted))?;
        let key = ReplayKey::new(operation_key)
            .map_err(|_| EditError::Edit(EditErrorKind::ReplayUnavailable))?;
        let mut arguments = serde_json::json!({
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
        });
        if let Some((key, hash)) = payload_hash {
            arguments[key] = serde_json::Value::String(hash);
        }
        let fingerprint = RequestFingerprint::for_json(operation, &arguments)
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
                let current = snapshot
                    .authority
                    .read_regular_beneath_anchor(
                        Path::new(&guard.path),
                        crate::knowledge::SECRET_SCAN_MAX_BYTES,
                    )
                    .map_err(|_| EditError::Edit(EditErrorKind::ReplayUnavailable))?
                    .ok_or(EditError::Edit(EditErrorKind::ReplayUnavailable))?;
                if !record.matches_post_image(&current) {
                    return Err(EditError::Edit(EditErrorKind::ReplayConflict));
                }
                return Ok(StructuralApplied {
                    path: guard.path.clone(),
                    post_image_hash: digest_hex(&current),
                    replayed: true,
                    refresh_ticket_identity: None,
                    body: EditBody::new(EditBodyParts::Replayed {
                        tool,
                        path: guard.path.clone(),
                        post_image_hash: digest_hex(&current),
                    }),
                });
            }
        };
        let (prepared, path) = match prepare(&snapshot) {
            Ok(prepared) => prepared,
            Err(error) => {
                replay
                    .release_not_started(&lease)
                    .map_err(|_| EditError::Edit(EditErrorKind::ReplayUnavailable))?;
                return Err(EditError::Edit(error));
            }
        };
        if authority.cancel.load(Ordering::Acquire) {
            replay
                .release_not_started(&lease)
                .map_err(|_| EditError::Edit(EditErrorKind::ReplayUnavailable))?;
            return Err(EditError::Edit(EditErrorKind::Cancelled));
        }
        let original = match snapshot.generation.live.get_file(&guard.path) {
            Some(file) => &file.content,
            None => {
                let _ = replay.release_not_started(&lease);
                return Err(EditError::Edit(EditErrorKind::FileNotAdmitted));
            }
        };
        if replay.mark_started(&lease).is_err() {
            let _ = replay.release_not_started(&lease);
            return Err(EditError::Edit(EditErrorKind::ReplayUnavailable));
        }
        if authority.cancel.load(Ordering::Acquire) {
            let _ = replay.mark_uncertain(&lease);
            return Err(EditError::Edit(EditErrorKind::WriteUncertain));
        }
        let project_state = crate::domain::ProjectStateDir::new(state_dir.to_path_buf());
        let write = guarded_atomic_write_file_expected(
            &snapshot.root,
            Some(&project_state),
            &path,
            original,
            &prepared.new_content,
            Some(&guard.content_hash),
            Some(guard.authority_publication),
        );
        match write {
            Ok(GuardedWriteOutcome::Rejected) => {
                let rejected = match ReplayOutcome::from_response_and_post_image(
                    OutcomeKind::Rejected,
                    b"write_conflict",
                    original,
                ) {
                    Ok(outcome) => outcome,
                    Err(_) => {
                        let _ = replay.mark_uncertain(&lease);
                        return Err(EditError::Edit(EditErrorKind::WriteUncertain));
                    }
                };
                if replay.fail(&lease, &rejected).is_err() {
                    let _ = replay.mark_uncertain(&lease);
                    return Err(EditError::Edit(EditErrorKind::WriteUncertain));
                }
                Err(EditError::Edit(EditErrorKind::WriteConflict))
            }
            Ok(GuardedWriteOutcome::Written(report)) => {
                let refresh = self.request_refresh();
                if authority.cancel.load(Ordering::Acquire) || refresh.is_err() {
                    let _ = replay.mark_uncertain(&lease);
                    return Err(EditError::Edit(EditErrorKind::WriteUncertain));
                }
                let refresh = refresh.expect("checked successful refresh request");
                let outcome = match ReplayOutcome::from_response_and_post_image(
                    OutcomeKind::Applied,
                    b"applied",
                    &prepared.new_content,
                ) {
                    Ok(outcome) => outcome,
                    Err(_) => {
                        let _ = replay.mark_uncertain(&lease);
                        return Err(EditError::Edit(EditErrorKind::WriteUncertain));
                    }
                };
                if replay.complete(&lease, &outcome).is_err() {
                    let _ = replay.mark_uncertain(&lease);
                    return Err(EditError::Edit(EditErrorKind::WriteUncertain));
                }
                snapshot.record_commitment(&[PathBuf::from(&guard.path)]);
                let body = describe(&snapshot, &prepared, report.tee_snapshot);
                Ok(StructuralApplied {
                    path: guard.path.clone(),
                    post_image_hash: digest_hex(&prepared.new_content),
                    replayed: false,
                    refresh_ticket_identity: Some(refresh.ticket_identity().to_owned()),
                    body,
                })
            }
            Err(_) => {
                let _ = replay.mark_uncertain(&lease);
                Err(EditError::Edit(EditErrorKind::WriteUncertain))
            }
        }
    }
}
