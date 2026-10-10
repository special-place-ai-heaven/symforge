//! Synchronous exact-path freshen for targeted retrieval, shared by the MCP
//! read handlers and the embedded query lanes.

use std::path::PathBuf;

use crate::live_index::single_file::{FreshenResult, freshen_file_if_stale};
use crate::live_index::store::SharedIndex;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TargetedFreshenRefusal {
    ProjectGenerationChanged,
    PublicationRejected,
}

impl TargetedFreshenRefusal {
    pub(crate) fn message(self, relative_path: &str) -> String {
        match self {
            Self::ProjectGenerationChanged => format!(
                "Index refresh interrupted: project changed while refreshing '{relative_path}'; retry the read."
            ),
            Self::PublicationRejected => format!(
                "Index refresh interrupted: refresh for '{relative_path}' did not publish; retry the read."
            ),
        }
    }

    pub(crate) fn permits_authoritative_disk_fallback(self) -> bool {
        matches!(self, Self::PublicationRejected)
    }
}

pub(crate) fn classify_targeted_freshen_result(
    result: FreshenResult,
) -> Result<bool, TargetedFreshenRefusal> {
    match result {
        FreshenResult::Fresh => Ok(false),
        FreshenResult::StaleReindexed | FreshenResult::StaleRemoved => Ok(true),
        FreshenResult::GenerationMismatch => Err(TargetedFreshenRefusal::ProjectGenerationChanged),
        FreshenResult::PublicationRejected => Err(TargetedFreshenRefusal::PublicationRejected),
    }
}

pub(crate) fn safe_repo_path_for_freshen(
    repo_root: &std::path::Path,
    relative_path: &str,
) -> Result<PathBuf, String> {
    let relative = std::path::Path::new(relative_path);
    if relative.is_absolute()
        || relative
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err(format!("path '{relative_path}' is outside the repository"));
    }
    match super::read_gate::resolve_repo_path(repo_root, relative_path)? {
        Some(path) => Ok(path),
        // Only a spelling with nothing on disk falls back, so the freshen lane
        // can confirm a deletion. Every refusal and every other I/O error
        // propagates instead of becoming a path the lane would follow.
        None => {
            let canon_root = repo_root
                .canonicalize()
                .map_err(|e| format!("cannot resolve repo root: {e}"))?;
            Ok(canon_root.join(relative))
        }
    }
}

/// Freshen one exact repository path before a targeted read: re-index it when
/// its on-disk mtime differs from the publication's, or remove it when it is
/// gone. `Ok(true)` means the publication changed. A path the freshen lane
/// cannot resolve is left to the read itself, exactly as before.
///
/// `expected_gen` must be read BEFORE `repo_root` is captured: a retarget in
/// between then fails the generation fence instead of indexing one root's
/// file under another's generation. The observation lane attributes the
/// change to the incarnation active at call time (the C3b
/// synchronous-facade ruling).
pub(crate) fn freshen_exact_path(
    shared: &SharedIndex,
    expected_gen: u64,
    repo_root: &std::path::Path,
    authority: &crate::live_index::index_lifecycle::activation::ProjectSourceAuthority,
    relative_path: &str,
) -> Result<bool, TargetedFreshenRefusal> {
    let Ok(abs_path) = safe_repo_path_for_freshen(repo_root, relative_path) else {
        return Ok(false);
    };
    let observer = authority.active_observer();
    classify_targeted_freshen_result(freshen_file_if_stale(
        relative_path,
        &abs_path,
        shared,
        expected_gen,
        authority,
        observer,
    ))
}
