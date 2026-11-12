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
        // A targeted read leaves an unconfirmed file to the read itself, as it
        // did before the comparison classified its failure.
        FreshenResult::Unconfirmed(_) | FreshenResult::UnstampedAbsent => Ok(false),
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

/// Most files one answer compares with the disk before it is given (spec 036
/// FR-015). Measured 2026-11-11 on this repository, Windows, warm cache
/// (`answer_check_cost_on_this_repository`): a bare stat is 15-20 us per file
/// (1,678 files in 25-34 ms); a whole empty `search_text` with the check took
/// 99 ms once each file's content was confirmed, and 785 ms on the first call,
/// which hashes every file once. 5,000 files keeps the steady-state check near
/// 200 ms. Beyond the bound the answer states how many files it did not
/// compare and makes no completeness claim.
pub(crate) const ANSWER_CHECK_FILE_BOUND: usize = 5_000;

/// What one answer observed about its files against the disk. Built only by
/// [`check_answer_scope`] (or [`AnswerCheck::not_run`]), so an envelope cannot
/// claim a comparison that did not happen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AnswerCheck {
    /// Files that could not be confirmed, with the classified cause.
    unconfirmed: Vec<(String, String)>,
    /// In-scope files past [`ANSWER_CHECK_FILE_BOUND`], not compared.
    uncompared: usize,
    /// Why no comparison ran at all, when none did.
    not_run: Option<&'static str>,
    /// Measured at check time: files the index has never seen may exist.
    observations_lost: bool,
    /// Files this call compared with the disk and confirmed.
    compared: usize,
}

/// Why no comparison ran; the same text on the MCP and embed lanes (M9).
pub(crate) const NOT_RUN_INDEX_NOT_READY: &str = "the index is not ready";
/// A server bound to no repository root has no disk to compare with.
pub(crate) const NOT_RUN_NO_ROOT: &str = "no repository root is bound";
/// At most this many unconfirmed files are named; the rest are counted (M6).
const MAX_NAMED_UNCONFIRMED: usize = 5;

/// True when `text` was cut by a token budget, so it makes no complete claim
/// (review M7). Both lanes read the same markers.
pub(crate) fn answer_was_cut(text: &str) -> bool {
    text.contains("budget-limited (was:")
        || text.contains(super::source::CANONICAL_TRUNCATION_MARKER)
        || text.contains("[adaptive verbosity:")
}

/// The FR-017 line for an empty or complete claim while observations are lost.
pub(crate) const LOST_OBSERVATIONS_LINE: &str =
    "Files created since the last scan may be missing; run index_folder to rescan.";

impl AnswerCheck {
    /// No file was compared, for `reason`; every claim stays qualified.
    pub(crate) fn not_run(reason: &'static str) -> Self {
        Self {
            unconfirmed: Vec::new(),
            uncompared: 0,
            not_run: Some(reason),
            observations_lost: false,
            compared: 0,
        }
    }

    #[cfg(test)]
    pub(crate) fn confirmed_for_tests() -> Self {
        Self {
            unconfirmed: Vec::new(),
            uncompared: 0,
            not_run: None,
            observations_lost: false,
            compared: 1,
        }
    }

    #[cfg(test)]
    pub(crate) fn unconfirmed_for_tests(path: &str, cause: &str) -> Self {
        Self {
            unconfirmed: vec![(path.to_string(), cause.to_string())],
            uncompared: 0,
            not_run: None,
            observations_lost: false,
            compared: 0,
        }
    }

    /// In-scope files past [`ANSWER_CHECK_FILE_BOUND`], not compared.
    pub(crate) fn uncompared(&self) -> usize {
        self.uncompared
    }

    /// What the banner says this call observed: the files it confirmed.
    pub(crate) fn confirmed_label(&self) -> String {
        match self.compared {
            1 => "1 file confirmed against disk".to_string(),
            n => format!("{n} files confirmed against disk"),
        }
    }

    /// Every file the answer may draw on was confirmed against the disk.
    pub(crate) fn all_confirmed(&self) -> bool {
        self.unconfirmed.is_empty() && self.uncompared == 0 && self.not_run.is_none()
    }

    /// The caveat lines of contracts/tool-outcome.md (Freshness), one per
    /// affected file; on an empty or complete claim, also the FR-017 line
    /// when observations were lost. Empty when nothing applies.
    pub(crate) fn notes(&self, claims_complete: bool) -> String {
        let mut lines: Vec<String> = self
            .unconfirmed
            .iter()
            .take(MAX_NAMED_UNCONFIRMED)
            .map(|(path, cause)| format!("Not confirmed against disk: {path} ({cause})"))
            .collect();
        if self.unconfirmed.len() > MAX_NAMED_UNCONFIRMED {
            lines.push(format!(
                "Not confirmed against disk: +{} more files",
                self.unconfirmed.len() - MAX_NAMED_UNCONFIRMED
            ));
        }
        if let Some(reason) = self.not_run {
            lines.push(format!(
                "Not confirmed against disk: no file was compared ({reason})"
            ));
        }
        if self.uncompared > 0 {
            lines.push(format!(
                "Not confirmed against disk: {} files in scope beyond the {ANSWER_CHECK_FILE_BOUND}-file check bound; this answer may be incomplete.",
                self.uncompared
            ));
        }
        if claims_complete && self.observations_lost {
            lines.push(LOST_OBSERVATIONS_LINE.to_string());
        }
        lines.join("\n")
    }

    /// Append [`Self::notes`] to `output` on its own lines.
    pub(crate) fn append_notes(&self, output: &mut String, claims_complete: bool) {
        let notes = self.notes(claims_complete);
        if !notes.is_empty() {
            output.push_str("\n\n");
            output.push_str(&notes);
        }
    }
}

/// True when the index may have missed file events, so files it has never
/// seen may exist (FR-017). An unreadable or unstable record is about a file
/// the index knows and is no lost observation (FR-018): a degraded scout counts
/// only when one of its issues names no scouted entry (an unreadable
/// directory, a traversal stop), because only then can a file be unseen.
fn observations_lost(shared: &SharedIndex) -> bool {
    use crate::domain::{FreshnessReason, FreshnessStatus, ScoutIssueKind};
    let unseen_by_scout = || {
        shared.scout_plan().is_none_or(|plan| {
            let scouted: std::collections::HashSet<&str> = plan
                .entries
                .iter()
                .map(|entry| entry.path.public_id.as_str())
                .collect();
            plan.issues.iter().any(|issue| {
                matches!(issue.kind, ScoutIssueKind::TraversalCircuitBreaker)
                    || issue
                        .path_id
                        .as_deref()
                        .is_none_or(|id| !scouted.contains(id))
            })
        })
    };
    match shared.freshness_status().as_ref() {
        FreshnessStatus::Current => false,
        FreshnessStatus::Verifying => true,
        FreshnessStatus::Degraded { reason_codes, .. } => {
            reason_codes.iter().any(|reason| match reason {
                FreshnessReason::WatcherUnavailable
                | FreshnessReason::SnapshotVerificationFailed
                | FreshnessReason::CatalogEntryCapacityExceeded
                | FreshnessReason::CatalogMetadataCapacityExceeded => true,
                FreshnessReason::ReconciliationPending => unseen_by_scout(),
                FreshnessReason::ObservationFailed | FreshnessReason::DerivedPublicationPending => {
                    false
                }
            })
        }
    }
}

/// Compare every indexed file under `path_prefix` (all files when `None`)
/// with the disk, re-indexing those that differ, BEFORE the caller captures
/// the publication it answers from (spec 036 FR-015, research D5). Each file
/// takes and releases the index write lock on its own; no read guard is held.
/// All files share one transient-retry budget (FR-020). Returns what was
/// observed and whether the publication changed.
pub(crate) fn check_answer_scope(
    shared: &SharedIndex,
    expected_gen: u64,
    repo_root: &std::path::Path,
    authority: &crate::live_index::index_lifecycle::activation::ProjectSourceAuthority,
    path_prefix: Option<&str>,
) -> (AnswerCheck, bool) {
    // Indexed paths are forward-slash and relative; spell the prefix the same.
    let prefix = path_prefix.map(|prefix| {
        let prefix = prefix.replace('\\', "/");
        prefix.trim_start_matches("./").to_string()
    });
    let mut paths: Vec<String> = shared
        .published_generation()
        .live
        .all_files()
        .map(|(path, _)| path.clone())
        .filter(|path| {
            prefix
                .as_deref()
                .is_none_or(|prefix| path.starts_with(prefix))
        })
        .collect();
    paths.sort_unstable();
    let uncompared = paths.len().saturating_sub(ANSWER_CHECK_FILE_BOUND);
    paths.truncate(ANSWER_CHECK_FILE_BOUND);
    let observer = authority.active_observer();
    let mut check = AnswerCheck {
        unconfirmed: Vec::new(),
        uncompared,
        not_run: None,
        observations_lost: false,
        compared: 0,
    };
    let mut changed = false;
    crate::discovery::os_response::with_call_budget(|| {
        for path in paths {
            let abs_path = repo_root.join(&path);
            match freshen_file_if_stale(&path, &abs_path, shared, expected_gen, authority, observer)
            {
                FreshenResult::Fresh => check.compared += 1,
                FreshenResult::StaleReindexed | FreshenResult::StaleRemoved => {
                    changed = true;
                    check.compared += 1;
                }
                // The project changed under the check: every later file
                // would fail the same fence, so stop and say so once (M6).
                FreshenResult::GenerationMismatch => {
                    check.unconfirmed.push((
                        path,
                        "the project changed during the check; retry".to_string(),
                    ));
                    break;
                }
                FreshenResult::PublicationRejected => check
                    .unconfirmed
                    .push((path, "refresh did not publish; retry".to_string())),
                // Nothing was re-indexed, so nothing is published (H5).
                FreshenResult::Unconfirmed(cause) => {
                    check.unconfirmed.push((path, cause.describe()));
                }
                FreshenResult::UnstampedAbsent => check.unconfirmed.push((
                    path,
                    "no modification time was recorded and no file is on disk".to_string(),
                )),
            }
        }
    });
    check.observations_lost = observations_lost(shared);
    (check, changed)
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
) -> Result<TargetedFreshen, TargetedFreshenRefusal> {
    let Ok(abs_path) = safe_repo_path_for_freshen(repo_root, relative_path) else {
        return Ok(TargetedFreshen::default());
    };
    let observer = authority.active_observer();
    let result = freshen_file_if_stale(
        relative_path,
        &abs_path,
        shared,
        expected_gen,
        authority,
        observer,
    );
    // Review M14: an unconfirmed file is still served from the index, so the
    // answer must name it and the cause.
    let unconfirmed = match &result {
        FreshenResult::Unconfirmed(cause) => Some(format!(
            "Not confirmed against disk: {relative_path} ({})",
            cause.describe()
        )),
        _ => None,
    };
    Ok(TargetedFreshen {
        changed: classify_targeted_freshen_result(result)?,
        unconfirmed,
    })
}

/// What a targeted freshen observed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct TargetedFreshen {
    /// The publication changed.
    pub(crate) changed: bool,
    /// The caveat line when the file could not be compared with the disk.
    pub(crate) unconfirmed: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Review M6: at most five unconfirmed files are named, then a count.
    #[test]
    fn caveat_lines_are_capped() {
        let check = AnswerCheck {
            unconfirmed: (0..7)
                .map(|n| {
                    (
                        format!("src/f{n}.rs"),
                        "in use by another process".to_string(),
                    )
                })
                .collect(),
            uncompared: 0,
            not_run: None,
            observations_lost: false,
            compared: 0,
        };
        let notes = check.notes(false);
        assert_eq!(notes.lines().count(), 6, "{notes}");
        assert!(
            notes.ends_with("Not confirmed against disk: +2 more files"),
            "{notes}"
        );
        // Control: five or fewer are all named.
        let five = AnswerCheck {
            unconfirmed: check.unconfirmed[..5].to_vec(),
            ..check.clone()
        };
        assert_eq!(five.notes(false).lines().count(), 5);
    }

    /// Review H5: a file that could not be read changes nothing, so the embed
    /// lane must not publish for it.
    #[test]
    fn an_unconfirmed_file_is_not_a_change() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("a.rs"),
            "fn a() {}
",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("b.rs"),
            "fn b() {}
",
        )
        .unwrap();
        let shared = crate::live_index::LiveIndex::load(dir.path()).unwrap();
        let authority =
            crate::live_index::index_lifecycle::activation::project_source_authority(dir.path());
        #[cfg(windows)]
        let _block = {
            use std::os::windows::fs::OpenOptionsExt;
            std::fs::OpenOptions::new()
                .read(true)
                .share_mode(0)
                .open(dir.path().join("a.rs"))
                .unwrap()
        };
        #[cfg(not(windows))]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(dir.path().join("a.rs"), std::fs::Permissions::from_mode(0))
                .unwrap();
            if std::fs::read(dir.path().join("a.rs")).is_ok() {
                return; // root reads through mode 000
            }
        }
        let generation = shared.current_project_generation();
        let (check, changed) =
            check_answer_scope(&shared, generation, dir.path(), &authority, None);
        assert!(
            !check.all_confirmed(),
            "control: a.rs was not confirmed: {check:?}"
        );
        assert!(!changed, "an unread file is not a re-index");
    }
}
