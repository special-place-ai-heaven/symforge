//! Shared current/worktree/ref selection and coverage for knowledge tools.

use super::review_contract::KnowledgeSourceScope;
use crate::domain::{CoverageStatus, SourceLocation};
use crate::live_index::{PublishedGeneration, PublishedSourceSet};
use std::sync::Arc;

/// Select the published generations a source scope addresses, deterministically.
///
/// `current` is the single current-worktree lane. `local_refs`/`worktrees`
/// filter the captured set by source location, excluding the current lane.
/// `all` lists the current lane first (it ranks ahead of a divergent ref but
/// never hides it), then every other lane in `SourceId` order.
pub(crate) fn select_scoped_sources(
    source_set: &PublishedSourceSet,
    scope: KnowledgeSourceScope,
) -> Vec<Arc<PublishedGeneration>> {
    let current_id = &source_set.current_source_id;
    let is_git_ref = |generation: &PublishedGeneration| {
        matches!(
            generation.source.as_deref().map(|source| &source.location),
            Some(SourceLocation::GitRef { .. })
        )
    };
    let is_worktree = |generation: &PublishedGeneration| {
        matches!(
            generation.source.as_deref().map(|source| &source.location),
            Some(SourceLocation::WorkingTree { .. })
        )
    };
    match scope {
        KnowledgeSourceScope::Current => source_set
            .sources
            .get(current_id)
            .map(Arc::clone)
            .into_iter()
            .collect(),
        KnowledgeSourceScope::LocalRefs => source_set
            .sources
            .iter()
            .filter(|(id, generation)| *id != current_id && is_git_ref(generation))
            .map(|(_, generation)| Arc::clone(generation))
            .collect(),
        KnowledgeSourceScope::Worktrees => source_set
            .sources
            .iter()
            .filter(|(id, generation)| *id != current_id && is_worktree(generation))
            .map(|(_, generation)| Arc::clone(generation))
            .collect(),
        KnowledgeSourceScope::All => {
            let mut selected: Vec<Arc<PublishedGeneration>> = source_set
                .sources
                .get(current_id)
                .map(Arc::clone)
                .into_iter()
                .collect();
            selected.extend(
                source_set
                    .sources
                    .iter()
                    .filter(|(id, _)| *id != current_id)
                    .map(|(_, generation)| Arc::clone(generation)),
            );
            selected
        }
    }
}

/// Manifest-declared coverage for one source, defaulting to `Degraded` when no
/// manifest is published so a composed response never claims a false Complete.
pub(crate) fn source_coverage(generation: &PublishedGeneration) -> CoverageStatus {
    generation
        .manifest
        .as_deref()
        .map(|manifest| manifest.coverage)
        .unwrap_or(CoverageStatus::Degraded)
}

/// Overall coverage of a composed response equals the worst included source
/// (L-R06): a single degraded source degrades the whole envelope.
pub(crate) fn worst_source_coverage(selected: &[Arc<PublishedGeneration>]) -> CoverageStatus {
    if selected
        .iter()
        .any(|generation| source_coverage(generation) == CoverageStatus::Degraded)
    {
        CoverageStatus::Degraded
    } else {
        CoverageStatus::Complete
    }
}

pub(crate) fn source_scope_label(scope: KnowledgeSourceScope) -> &'static str {
    match scope {
        KnowledgeSourceScope::Current => "current",
        KnowledgeSourceScope::Worktrees => "worktrees",
        KnowledgeSourceScope::LocalRefs => "local_refs",
        KnowledgeSourceScope::All => "all",
    }
}
