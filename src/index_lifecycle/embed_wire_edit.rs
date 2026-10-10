//! Host-facing serialized edit requests, materialized only under a bound source.

use crate::embed::parity::edit::{
    BatchEditAction, BatchEditRequest, BatchInsertRequest, BatchRenamePlan, BatchRenameRequest,
    DeleteRequest, EditApplyAuthority, EditError, EditErrorKind, EditGuard, EditWithinRequest,
    InsertRequest, ReplaceRequest, WireBatchEditAction, WireBatchRenamePlan, WireEditGuard,
    WireEditPlan, WireEditReply, WireEditRequest,
};
use crate::lifecycle_identity::PublicationIdentity;
use std::path::PathBuf;

use super::embedded::EmbeddedSourceHandle;

fn bound_guard(
    wire: &WireEditGuard,
    root: &PathBuf,
    publication: PublicationIdentity,
) -> EditGuard {
    wire.clone().bind(root, publication)
}

fn bound_actions(
    actions: &[WireBatchEditAction],
    root: &PathBuf,
    publication: PublicationIdentity,
) -> BatchEditRequest {
    BatchEditRequest {
        actions: actions
            .iter()
            .map(|action| match action {
                WireBatchEditAction::Replace { guard, new_body } => {
                    BatchEditAction::Replace(ReplaceRequest {
                        guard: bound_guard(guard, root, publication),
                        new_body: new_body.clone(),
                    })
                }
                WireBatchEditAction::Insert {
                    guard,
                    position,
                    content,
                } => BatchEditAction::Insert(InsertRequest {
                    guard: bound_guard(guard, root, publication),
                    position: *position,
                    content: content.clone(),
                }),
                WireBatchEditAction::Delete { guard } => BatchEditAction::Delete(DeleteRequest {
                    guard: bound_guard(guard, root, publication),
                }),
                WireBatchEditAction::EditWithin {
                    guard,
                    old_text,
                    new_text,
                    replace_all,
                    occurrence,
                    near_line,
                } => BatchEditAction::EditWithin(EditWithinRequest {
                    guard: bound_guard(guard, root, publication),
                    old_text: old_text.clone(),
                    new_text: new_text.clone(),
                    replace_all: *replace_all,
                    occurrence: *occurrence,
                    near_line: *near_line,
                }),
            })
            .collect(),
    }
}

fn bound_rename(
    wire: &WireBatchRenamePlan,
    root: &PathBuf,
    publication: PublicationIdentity,
) -> BatchRenamePlan {
    BatchRenamePlan {
        guard: bound_guard(&wire.guard, root, publication),
        code_only: wire.code_only,
        affected_paths: wire.affected_paths.clone(),
        confident_sites: wire.confident_sites,
        uncertain_sites: wire.uncertain_sites,
    }
}

impl EmbeddedSourceHandle {
    /// Read-only planning and dry runs. An apply variant never reaches the engine here.
    pub fn preview_wire_edit(&self, wire: &WireEditRequest) -> Result<WireEditReply, EditError> {
        if wire.is_apply() {
            return Err(EditError::Edit(EditErrorKind::WriteAuthorityRefused));
        }
        let snapshot = self.capture_query_snapshot(b"preview-wire-edit")?;
        let root = &snapshot.root;
        let publication = snapshot.authority_publication;
        match wire {
            WireEditRequest::Plan { target } => {
                let plan = self.edit_plan(target)?;
                Ok(WireEditReply::Plan(WireEditPlan {
                    guard: WireEditGuard::from(&plan.guard),
                    byte_range: plan.byte_range,
                    line_range: plan.line_range,
                    reference_count: plan.reference_count,
                }))
            }
            WireEditRequest::PlanBatchRename { target, code_only } => {
                let plan = self.plan_batch_rename(target, *code_only)?;
                Ok(WireEditReply::BatchRenamePlan(WireBatchRenamePlan {
                    guard: WireEditGuard::from(&plan.guard),
                    code_only: plan.code_only,
                    affected_paths: plan.affected_paths,
                    confident_sites: plan.confident_sites,
                    uncertain_sites: plan.uncertain_sites,
                }))
            }
            WireEditRequest::PreviewReplace { guard, new_body } => self
                .preview_replace(&ReplaceRequest {
                    guard: bound_guard(guard, root, publication),
                    new_body: new_body.clone(),
                })
                .map(WireEditReply::ReplacePreview),
            WireEditRequest::PreviewInsert {
                guard,
                position,
                content,
            } => self
                .preview_insert(&InsertRequest {
                    guard: bound_guard(guard, root, publication),
                    position: *position,
                    content: content.clone(),
                })
                .map(WireEditReply::StructuralPreview),
            WireEditRequest::PreviewDelete { guard } => self
                .preview_delete(&DeleteRequest {
                    guard: bound_guard(guard, root, publication),
                })
                .map(WireEditReply::StructuralPreview),
            WireEditRequest::PreviewEditWithin {
                guard,
                old_text,
                new_text,
                replace_all,
                occurrence,
                near_line,
            } => self
                .preview_edit_within(&EditWithinRequest {
                    guard: bound_guard(guard, root, publication),
                    old_text: old_text.clone(),
                    new_text: new_text.clone(),
                    replace_all: *replace_all,
                    occurrence: *occurrence,
                    near_line: *near_line,
                })
                .map(WireEditReply::EditWithinPreview),
            WireEditRequest::PreviewBatchEdit { actions } => self
                .preview_batch_edit(&bound_actions(actions, root, publication))
                .map(WireEditReply::BatchEditPreview),
            WireEditRequest::PreviewBatchInsert {
                targets,
                position,
                content,
            } => self
                .preview_batch_insert(&BatchInsertRequest {
                    targets: targets
                        .iter()
                        .map(|guard| bound_guard(guard, root, publication))
                        .collect(),
                    position: *position,
                    content: content.clone(),
                })
                .map(WireEditReply::BatchEditPreview),
            WireEditRequest::PreviewBatchRename { plan, new_name } => self
                .preview_batch_rename(&BatchRenameRequest {
                    plan: bound_rename(plan, root, publication),
                    new_name: new_name.clone(),
                })
                .map(WireEditReply::BatchRenamePreview),
            _ => Err(EditError::Edit(EditErrorKind::InvalidReplacement)),
        }
    }

    /// Host-only apply. The wire carries source facts, never the write grant.
    pub fn apply_wire_edit(
        &self,
        wire: &WireEditRequest,
        authority: &EditApplyAuthority,
        operation_key: &str,
    ) -> Result<WireEditReply, EditError> {
        if !wire.is_apply() {
            return Err(EditError::Edit(EditErrorKind::InvalidReplacement));
        }
        let snapshot = self.capture_query_snapshot(b"apply-wire-edit")?;
        let root = &snapshot.root;
        let publication = snapshot.authority_publication;
        match wire {
            WireEditRequest::ApplyReplace { guard, new_body } => self
                .apply_replace(
                    &ReplaceRequest {
                        guard: bound_guard(guard, root, publication),
                        new_body: new_body.clone(),
                    },
                    authority,
                    operation_key,
                )
                .map(WireEditReply::ReplaceApplied),
            WireEditRequest::ApplyInsert {
                guard,
                position,
                content,
            } => self
                .apply_insert(
                    &InsertRequest {
                        guard: bound_guard(guard, root, publication),
                        position: *position,
                        content: content.clone(),
                    },
                    authority,
                    operation_key,
                )
                .map(WireEditReply::StructuralApplied),
            WireEditRequest::ApplyDelete { guard } => self
                .apply_delete(
                    &DeleteRequest {
                        guard: bound_guard(guard, root, publication),
                    },
                    authority,
                    operation_key,
                )
                .map(WireEditReply::StructuralApplied),
            WireEditRequest::ApplyEditWithin {
                guard,
                old_text,
                new_text,
                replace_all,
                occurrence,
                near_line,
            } => self
                .apply_edit_within(
                    &EditWithinRequest {
                        guard: bound_guard(guard, root, publication),
                        old_text: old_text.clone(),
                        new_text: new_text.clone(),
                        replace_all: *replace_all,
                        occurrence: *occurrence,
                        near_line: *near_line,
                    },
                    authority,
                    operation_key,
                )
                .map(WireEditReply::StructuralApplied),
            WireEditRequest::ApplyBatchEdit { actions } => self
                .apply_batch_edit(
                    &bound_actions(actions, root, publication),
                    authority,
                    operation_key,
                )
                .map(WireEditReply::BatchEditApplied),
            WireEditRequest::ApplyBatchInsert {
                targets,
                position,
                content,
            } => self
                .apply_batch_insert(
                    &BatchInsertRequest {
                        targets: targets
                            .iter()
                            .map(|guard| bound_guard(guard, root, publication))
                            .collect(),
                        position: *position,
                        content: content.clone(),
                    },
                    authority,
                    operation_key,
                )
                .map(WireEditReply::BatchEditApplied),
            WireEditRequest::ApplyBatchRename { plan, new_name } => self
                .apply_batch_rename(
                    &BatchRenameRequest {
                        plan: bound_rename(plan, root, publication),
                        new_name: new_name.clone(),
                    },
                    authority,
                    operation_key,
                )
                .map(WireEditReply::BatchEditApplied),
            _ => Err(EditError::Edit(EditErrorKind::InvalidReplacement)),
        }
    }
}
