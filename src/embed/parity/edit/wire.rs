//! Serializable edit facts for room hosts. Write grants are never wire data.

use std::path::Path;

use serde::{Deserialize, Serialize};

use super::{
    BatchEditApplied, BatchEditPreview, BatchRenamePreview, EditGuard, EditTarget,
    EditWithinPreview, InsertPosition, ReplaceApplied, ReplacePreview, StructuralApplied,
    StructuralPreview,
};
use crate::lifecycle_identity::PublicationIdentity;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WireEditGuard {
    pub path: String,
    pub name: String,
    pub kind: String,
    pub symbol_line: u32,
    pub source_version: u64,
    pub publication_identity: String,
    pub serving_publication_identity: String,
    pub publication_generation: u64,
    pub content_generation: u64,
    pub content_hash: String,
    pub symbol_hash: String,
}

impl From<&EditGuard> for WireEditGuard {
    fn from(guard: &EditGuard) -> Self {
        Self {
            path: guard.path.clone(),
            name: guard.name.clone(),
            kind: guard.kind.clone(),
            symbol_line: guard.symbol_line,
            source_version: guard.source_version,
            publication_identity: guard.publication_identity.clone(),
            serving_publication_identity: guard.serving_publication_identity.clone(),
            publication_generation: guard.publication_generation,
            content_generation: guard.content_generation,
            content_hash: guard.content_hash.clone(),
            symbol_hash: guard.symbol_hash.clone(),
        }
    }
}

impl WireEditGuard {
    pub(crate) fn bind(self, root: &Path, authority_publication: PublicationIdentity) -> EditGuard {
        EditGuard {
            root: root.to_path_buf(),
            path: self.path,
            name: self.name,
            kind: self.kind,
            symbol_line: self.symbol_line,
            source_version: self.source_version,
            publication_identity: self.publication_identity,
            serving_publication_identity: self.serving_publication_identity,
            publication_generation: self.publication_generation,
            content_generation: self.content_generation,
            authority_publication,
            content_hash: self.content_hash,
            symbol_hash: self.symbol_hash,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum WireBatchEditAction {
    Replace {
        guard: WireEditGuard,
        new_body: String,
    },
    Insert {
        guard: WireEditGuard,
        position: InsertPosition,
        content: String,
    },
    Delete {
        guard: WireEditGuard,
    },
    EditWithin {
        guard: WireEditGuard,
        old_text: String,
        new_text: String,
        replace_all: bool,
        occurrence: Option<u32>,
        near_line: Option<u32>,
    },
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WireBatchRenamePlan {
    pub guard: WireEditGuard,
    pub code_only: bool,
    pub affected_paths: Vec<String>,
    pub confident_sites: usize,
    pub uncertain_sites: usize,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum WireEditRequest {
    Plan {
        target: EditTarget,
    },
    PlanBatchRename {
        target: EditTarget,
        code_only: bool,
    },
    PreviewReplace {
        guard: WireEditGuard,
        new_body: String,
    },
    ApplyReplace {
        guard: WireEditGuard,
        new_body: String,
    },
    PreviewInsert {
        guard: WireEditGuard,
        position: InsertPosition,
        content: String,
    },
    ApplyInsert {
        guard: WireEditGuard,
        position: InsertPosition,
        content: String,
    },
    PreviewDelete {
        guard: WireEditGuard,
    },
    ApplyDelete {
        guard: WireEditGuard,
    },
    PreviewEditWithin {
        guard: WireEditGuard,
        old_text: String,
        new_text: String,
        replace_all: bool,
        occurrence: Option<u32>,
        near_line: Option<u32>,
    },
    ApplyEditWithin {
        guard: WireEditGuard,
        old_text: String,
        new_text: String,
        replace_all: bool,
        occurrence: Option<u32>,
        near_line: Option<u32>,
    },
    PreviewBatchEdit {
        actions: Vec<WireBatchEditAction>,
    },
    ApplyBatchEdit {
        actions: Vec<WireBatchEditAction>,
    },
    PreviewBatchInsert {
        targets: Vec<WireEditGuard>,
        position: InsertPosition,
        content: String,
    },
    ApplyBatchInsert {
        targets: Vec<WireEditGuard>,
        position: InsertPosition,
        content: String,
    },
    PreviewBatchRename {
        plan: WireBatchRenamePlan,
        new_name: String,
    },
    ApplyBatchRename {
        plan: WireBatchRenamePlan,
        new_name: String,
    },
}

impl WireEditRequest {
    pub fn is_apply(&self) -> bool {
        match self {
            Self::ApplyReplace { .. }
            | Self::ApplyInsert { .. }
            | Self::ApplyDelete { .. }
            | Self::ApplyEditWithin { .. }
            | Self::ApplyBatchEdit { .. }
            | Self::ApplyBatchInsert { .. }
            | Self::ApplyBatchRename { .. } => true,
            Self::Plan { .. }
            | Self::PlanBatchRename { .. }
            | Self::PreviewReplace { .. }
            | Self::PreviewInsert { .. }
            | Self::PreviewDelete { .. }
            | Self::PreviewEditWithin { .. }
            | Self::PreviewBatchEdit { .. }
            | Self::PreviewBatchInsert { .. }
            | Self::PreviewBatchRename { .. } => false,
        }
    }
}

#[derive(Serialize)]
pub struct WireEditPlan {
    pub guard: WireEditGuard,
    pub byte_range: (u32, u32),
    pub line_range: (u32, u32),
    pub reference_count: usize,
}

#[derive(Serialize)]
#[serde(tag = "kind", content = "result", rename_all = "snake_case")]
pub enum WireEditReply {
    Plan(WireEditPlan),
    BatchRenamePlan(WireBatchRenamePlan),
    ReplacePreview(ReplacePreview),
    StructuralPreview(StructuralPreview),
    EditWithinPreview(EditWithinPreview),
    BatchEditPreview(BatchEditPreview),
    BatchRenamePreview(BatchRenamePreview),
    ReplaceApplied(ReplaceApplied),
    StructuralApplied(StructuralApplied),
    BatchEditApplied(BatchEditApplied),
}
