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
    /// A render made only to learn what the answer cites; never shown.
    provisional: bool,
}

/// Why no comparison ran; the same text on the MCP and embed lanes (M9).
pub(crate) const NOT_RUN_INDEX_NOT_READY: &str = "the index is not ready";
/// An embedded source that is shutting down compares nothing.
#[cfg_attr(not(feature = "embed"), allow(dead_code))] // read by the embed lane
pub(crate) const NOT_RUN_SHUTTING_DOWN: &str = "the source is shutting down";
/// An embedded source whose root was replaced or cannot be reached.
#[cfg_attr(not(feature = "embed"), allow(dead_code))] // read by the embed lane
pub(crate) const NOT_RUN_ROOT_REPLACED: &str =
    "the repository root was replaced or cannot be reached";
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
            provisional: false,
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
            provisional: false,
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
            provisional: false,
        }
    }

    /// True for the render [`answer_checked`] makes only to learn what an
    /// answer cites: a render may skip work whose result is never shown
    /// (a retrievable budget handle).
    pub(crate) fn is_provisional(&self) -> bool {
        self.provisional
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

/// What an answer claims beyond the rows it cites (spec 036 FR-015).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Claim {
    /// Only its rows: the answer says nothing about files it does not cite.
    Rows,
    /// An empty or complete result over its scope: a file that has just
    /// gained a match is not among the rows, so the whole scope is compared.
    Whole,
}

impl Claim {
    /// `Whole` when `complete`, else `Rows`.
    pub(crate) fn whole_if(complete: bool) -> Self {
        if complete { Self::Whole } else { Self::Rows }
    }
}

/// One run of a query: what it rendered, the indexed files the rendered
/// answer cites, and what it claims.
pub(crate) struct Rendered<T> {
    pub(crate) answer: T,
    pub(crate) cited: Vec<String>,
    pub(crate) claim: Claim,
}

/// The disk an answer is compared with, and the scope of its whole claims.
pub(crate) struct AnswerDisk<'a> {
    pub(crate) shared: &'a SharedIndex,
    /// Read BEFORE `repo_root` was captured, so a retarget in between fails
    /// the generation fence instead of indexing one root's files under
    /// another's generation.
    pub(crate) expected_gen: u64,
    pub(crate) repo_root: &'a std::path::Path,
    pub(crate) authority:
        &'a crate::live_index::index_lifecycle::activation::ProjectSourceAuthority,
    /// False while another writer owns publication (an embedded source that
    /// is refreshing): files that differ are named, not re-indexed.
    pub(crate) may_reindex: bool,
    /// Indexed-path prefix of the query (`None`: every file).
    pub(crate) scope: Option<&'a str>,
}

/// Most files one answer re-indexes on the request thread (round 3 item 6).
/// A parse costs about 1-5 ms per source file (debug build, this repository),
/// so 32 files keep the re-index under roughly 150 ms of an interactive call;
/// after a large branch switch the rest are named with
/// [`NOT_REINDEXED_REFRESHING`] and left to the background reconcile.
pub(crate) const MAX_REINDEX_PER_ANSWER: usize = 32;

/// Cause for a cited file that still differs after the one re-run (D5).
const CHANGED_DURING_ANSWER: &str = "changed again while this answer was prepared; retry";
/// Cause for a file that differs while another writer owns publication.
pub(crate) const NOT_REINDEXED_REFRESHING: &str = "the index is refreshing; retry";
const GENERATION_CHANGED: &str = "the project changed during the check; retry";

/// The indexed files among `paths`, the row paths of the structured result
/// an answer is rendered from (never parsed back out of its text, so a path
/// with spaces, non-ASCII letters or `\` separators is cited like any other).
pub(crate) fn cited_paths<'a>(
    live: &crate::live_index::LiveIndex,
    paths: impl IntoIterator<Item = &'a str>,
) -> Vec<String> {
    let mut cited = std::collections::BTreeSet::new();
    for path in paths {
        let path = path.trim_start_matches("./");
        let spelled = path.replace('\\', "/");
        for candidate in [path, spelled.as_str()] {
            if live.files.contains_key(candidate) {
                cited.insert(candidate.to_string());
                break;
            }
        }
    }
    cited.into_iter().collect()
}

/// Answer a query whose files are compared with the disk (spec 036 FR-015,
/// research D5). `capture` takes the publication to answer from; `render`
/// runs the query over it with an [`AnswerCheck`] and reports what it cites
/// and claims. The cited files, and on a [`Claim::Whole`] every file of the
/// scope up to [`ANSWER_CHECK_FILE_BOUND`], are compared; the files that
/// differ are re-indexed in one publication and the query runs once more. A
/// cited file that differs again is named, never looped on. The final render
/// gets the check of its own files, so its caveats can sit inside its budget.
/// No read guard is held across the re-index: `capture` runs after it.
pub(crate) fn answer_checked<Q, T>(
    disk: Result<AnswerDisk<'_>, &'static str>,
    mut capture: impl FnMut() -> Q,
    mut render: impl FnMut(&Q, &AnswerCheck) -> Rendered<T>,
    mut on_reindexed: impl FnMut(),
) -> (Q, T, AnswerCheck) {
    let disk = match disk {
        Ok(disk) => disk,
        Err(reason) => {
            let check = AnswerCheck::not_run(reason);
            let query = capture();
            let answer = render(&query, &check).answer;
            return (query, answer, check);
        }
    };
    crate::discovery::os_response::with_call_budget(|| {
        let expected_gen = disk.expected_gen;
        // Renders only to learn what the answer cites: no notes, so the
        // first run's budget and claim are the answer's own.
        let provisional = AnswerCheck {
            unconfirmed: Vec::new(),
            uncompared: 0,
            not_run: None,
            observations_lost: false,
            compared: 0,
            provisional: true,
        };
        let mut confinement = Confinement::new(disk.repo_root);
        let mut query = capture();
        let first = render(&query, &provisional);
        let (paths, uncompared) = paths_to_compare(&disk, &first);
        let seen = compare_paths(&disk, &mut confinement, &paths);
        let differing: Vec<String> = seen
            .iter()
            .filter(|(_, seen)| matches!(seen, Seen::Differs))
            .map(|(path, _)| path.clone())
            .collect();
        let mut claim = first.claim;
        let mut check = if differing.is_empty() || !disk.may_reindex {
            build_check(seen, uncompared, NOT_REINDEXED_REFRESHING, &[])
        } else {
            // Round 3 item 6: a bounded batch; the rest is named and left
            // to the background reconcile.
            let mut differing = differing;
            let deferred = differing.split_off(differing.len().min(MAX_REINDEX_PER_ANSWER));
            let (changed, mut failed) = reindex_batch(&disk, expected_gen, differing);
            failed.extend(
                deferred
                    .into_iter()
                    .map(|path| (path, NOT_REINDEXED_REFRESHING.to_string())),
            );
            if changed {
                on_reindexed();
            }
            query = capture();
            let again = render(&query, &provisional);
            claim = again.claim;
            let (paths, uncompared) = paths_to_compare(&disk, &again);
            let seen = compare_paths(&disk, &mut confinement, &paths);
            build_check(seen, uncompared, CHANGED_DURING_ANSWER, &failed)
        };
        // Only an empty or complete claim prints the FR-017 line, and the
        // measurement walks the scout plan, so a partial answer skips it.
        check.observations_lost = claim == Claim::Whole && observations_lost(disk.shared);
        let answer = render(&query, &check).answer;
        (query, answer, check)
    })
}

/// The cited files, plus the rest of the scope on a whole claim, sorted and
/// bounded; the second value counts the in-scope files past the bound.
fn paths_to_compare<T>(disk: &AnswerDisk<'_>, rendered: &Rendered<T>) -> (Vec<String>, usize) {
    paths_to_compare_bounded(disk, rendered, ANSWER_CHECK_FILE_BOUND)
}

fn paths_to_compare_bounded<T>(
    disk: &AnswerDisk<'_>,
    rendered: &Rendered<T>,
    bound: usize,
) -> (Vec<String>, usize) {
    let mut paths: std::collections::BTreeSet<String> = rendered.cited.iter().cloned().collect();
    let mut uncompared = 0;
    if rendered.claim == Claim::Whole {
        // Indexed paths are forward-slash and relative; spell the prefix the same.
        let prefix = disk
            .scope
            .filter(|prefix| !prefix.is_empty())
            .map(|prefix| {
                prefix
                    .replace('\\', "/")
                    .trim_start_matches("./")
                    .to_string()
            });
        let published = disk.shared.published_generation();
        let mut scope: Vec<&String> = published
            .live
            .all_files()
            .map(|(path, _)| path)
            .filter(|path| {
                prefix
                    .as_deref()
                    .is_none_or(|prefix| path.starts_with(prefix))
                    && !paths.contains(*path)
            })
            .collect();
        scope.sort_unstable();
        let room = bound.saturating_sub(paths.len());
        uncompared = scope.len().saturating_sub(room);
        paths.extend(scope.into_iter().take(room).cloned());
    }
    (paths.into_iter().collect(), uncompared)
}

/// What one comparison of a file with the disk observed.
enum Seen {
    Same,
    Differs,
    Unconfirmed(String),
}

/// H2 confinement for many files at once: each distinct parent directory is
/// resolved once per answer, and a directory that resolves outside the
/// repository (a junction or a linked folder) refuses every file beneath it.
/// The leaf itself is refused as a link by the stat in `compare_with_disk`.
struct Confinement {
    root: Option<std::path::PathBuf>,
    inside: std::collections::HashMap<std::path::PathBuf, bool>,
}

impl Confinement {
    fn new(repo_root: &std::path::Path) -> Self {
        Self {
            root: repo_root.canonicalize().ok(),
            inside: std::collections::HashMap::new(),
        }
    }

    fn admits(&mut self, repo_root: &std::path::Path, relative: &str) -> bool {
        let Some(root) = &self.root else {
            // No resolvable root: the per-file resolver decides, as before.
            return !super::read_gate::resolve_repo_path(repo_root, relative)
                .is_err_and(|message| message.contains("outside the repository"));
        };
        let Some(parent) = std::path::Path::new(relative)
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        else {
            return true;
        };
        let parent = parent.to_path_buf();
        if let Some(&inside) = self.inside.get(&parent) {
            return inside;
        }
        // A directory that is gone holds nothing outside; the stat says so.
        let inside = repo_root
            .join(&parent)
            .canonicalize()
            .map_or(true, |resolved| resolved.starts_with(root));
        self.inside.insert(parent, inside);
        inside
    }
}

fn compare_paths(
    disk: &AnswerDisk<'_>,
    confinement: &mut Confinement,
    paths: &[String],
) -> Vec<(String, Seen)> {
    use crate::live_index::single_file::{DiskComparison, compare_with_disk};
    paths
        .iter()
        .map(|path| {
            let seen = if !confinement.admits(disk.repo_root, path) {
                Seen::Unconfirmed(
                    crate::discovery::os_response::OsCause::refused(
                        "resolves outside the repository",
                    )
                    .describe(),
                )
            } else {
                match compare_with_disk(
                    path,
                    &disk.repo_root.join(path),
                    disk.shared,
                    disk.expected_gen,
                ) {
                    DiskComparison::Same => Seen::Same,
                    DiskComparison::GenerationChanged => {
                        Seen::Unconfirmed(GENERATION_CHANGED.to_string())
                    }
                    DiskComparison::Differs => Seen::Differs,
                    DiskComparison::Unconfirmed(cause) => Seen::Unconfirmed(cause.describe()),
                    DiskComparison::UnstampedAbsent => Seen::Unconfirmed(
                        "no modification time was recorded and no file is on disk".to_string(),
                    ),
                }
            };
            (path.clone(), seen)
        })
        .collect()
}

/// The check of one answer's files: `differs_cause` names a file that still
/// differs; `failed` carries the re-index failures of the files compared.
fn build_check(
    seen: Vec<(String, Seen)>,
    uncompared: usize,
    differs_cause: &str,
    failed: &[(String, String)],
) -> AnswerCheck {
    let mut check = AnswerCheck {
        unconfirmed: Vec::new(),
        uncompared,
        not_run: None,
        observations_lost: false,
        compared: 0,
        provisional: false,
    };
    for (path, seen) in seen {
        if let Some((_, cause)) = failed.iter().find(|(failed, _)| *failed == path) {
            check.unconfirmed.push((path, cause.clone()));
            continue;
        }
        match seen {
            Seen::Same => check.compared += 1,
            Seen::Differs => check.unconfirmed.push((path, differs_cause.to_string())),
            Seen::Unconfirmed(cause) => check.unconfirmed.push((path, cause)),
        }
    }
    check
}

/// Re-index `paths` (all differing from the disk) in ONE publication where
/// the canonical admission can be prepared off-lock; a path it cannot settle
/// that way (gone, refused, moved again) takes the single-file seam. Returns
/// whether the publication changed and the files that could not be confirmed.
fn reindex_batch(
    disk: &AnswerDisk<'_>,
    expected_gen: u64,
    paths: Vec<String>,
) -> (bool, Vec<(String, String)>) {
    use crate::live_index::single_file::prepare_snapshot_verify_admission;
    let shared = disk.shared;
    let observer = disk.authority.active_observer();
    let mut changed = false;
    let mut failed = Vec::new();
    let mut batch = Vec::new();
    let mut single = Vec::new();
    {
        let live = shared.read();
        for path in paths {
            let abs_path = disk.repo_root.join(&path);
            // A vanished file waits out an atomic save in the single seam.
            let prepared = abs_path
                .symlink_metadata()
                .is_ok()
                .then(|| {
                    prepare_snapshot_verify_admission(
                        &path,
                        &abs_path,
                        shared,
                        live.files.get(&path),
                    )
                })
                .flatten();
            match prepared {
                Some(prepared) => batch.push(prepared),
                None => single.push(path),
            }
        }
    }
    if !batch.is_empty() {
        let batched: Vec<String> = batch.iter().map(|file| file.path.clone()).collect();
        match shared.publish_snapshot_verify_with_observation(
            expected_gen,
            &[],
            batch,
            None,
            |_, _| false,
            |_, scouted| {
                scouted.absolute_path.as_deref().is_some_and(|path| {
                    std::fs::metadata(path).is_ok_and(|meta| {
                        meta.len() == scouted.stamp.size
                            && meta.modified().ok() == scouted.stamp.modified_hint
                    })
                })
            },
            || true,
        ) {
            None => failed.extend(
                batched
                    .into_iter()
                    .map(|path| (path, GENERATION_CHANGED.to_string())),
            ),
            Some(receipt) => {
                changed |= !receipt.indexed.is_empty()
                    || !receipt.terminal.is_empty()
                    || !receipt.superseded.is_empty();
                for path in &receipt.indexed {
                    if disk.authority.observe_admission(observer, path).is_err() {
                        tracing::debug!("answer check: stale incarnation's admission refused");
                    }
                }
                single.extend(receipt.refused);
            }
        }
    }
    for path in single {
        let abs_path = disk.repo_root.join(&path);
        match freshen_file_if_stale(
            &path,
            &abs_path,
            shared,
            expected_gen,
            disk.authority,
            observer,
        ) {
            FreshenResult::Fresh => {}
            FreshenResult::StaleReindexed | FreshenResult::StaleRemoved => changed = true,
            FreshenResult::GenerationMismatch => failed.push((path, GENERATION_CHANGED.into())),
            FreshenResult::PublicationRejected => {
                failed.push((path, "refresh did not publish; retry".into()));
            }
            FreshenResult::Unconfirmed(cause) => failed.push((path, cause.describe())),
            FreshenResult::UnstampedAbsent => failed.push((
                path,
                "no modification time was recorded and no file is on disk".into(),
            )),
        }
    }
    (changed, failed)
}

/// Fit `notes` and the answer into `max_tokens` (spec 036 item M7): the notes
/// are never cut, so the body is assembled under the budget minus the notes.
/// `assemble` builds the body under the budget it is given.
pub(crate) fn budget_with_notes(
    check: &AnswerCheck,
    claim: Claim,
    max_tokens: Option<u64>,
    assemble: impl FnOnce(Option<u64>) -> String,
) -> String {
    let notes = check.notes(claim == Claim::Whole);
    if notes.is_empty() {
        return assemble(max_tokens);
    }
    let reserve = crate::stel::handler::estimate_tokens_from_bytes(notes.len() as u64 + 2);
    let body = assemble(max_tokens.map(|max| max.saturating_sub(reserve).max(1)));
    // A body the budget cut claims no completeness (review M7).
    let notes = if answer_was_cut(&body) {
        check.notes(false)
    } else {
        notes
    };
    if notes.is_empty() {
        body
    } else {
        format!("{body}\n\n{notes}")
    }
}

/// Close a targeted read with its caveat line, inside `max_tokens` (item 4):
/// when the answer and the line do not fit, the answer is cut to the budget
/// the line leaves, never the line.
pub(crate) fn fit_caveat(body: String, caveat: Option<&str>, max_tokens: Option<u64>) -> String {
    let Some(caveat) = caveat.filter(|caveat| !caveat.is_empty()) else {
        return body;
    };
    let reserve = caveat.len() + 2;
    let body = match max_tokens.filter(|max| *max > 0) {
        Some(max) if body.len() + reserve > (max as usize).saturating_mul(4) => {
            let left = (max as usize).saturating_mul(4).saturating_sub(reserve) / 4;
            super::source::enforce_token_budget(body, Some(left.max(1) as u64))
        }
        _ => body,
    };
    format!("{body}\n\n{caveat}")
}

/// A targeted read's answer closed with the caveats of the files it could
/// not compare with the disk (item 5), on both lanes; a refusal or an error
/// answer is left as it is.
pub(crate) fn close_targeted_read(
    body: String,
    caveats: &[String],
    max_tokens: Option<u64>,
) -> String {
    if caveats.is_empty()
        || super::outcome::is_error_output(&body)
        || super::outcome::is_index_unavailable_output(&body)
    {
        return body;
    }
    fit_caveat(body, Some(&caveats.join("\n")), max_tokens)
}

impl<T> Rendered<T> {
    /// An answer that cites no file and claims nothing beyond itself (a
    /// refusal, a notice, an invalid request).
    pub(crate) fn plain(answer: T) -> Self {
        Self {
            answer,
            cited: Vec::new(),
            claim: Claim::Rows,
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
            provisional: false,
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
        let mut changed = false;
        let (_, _, check) = answer_checked(
            Ok(disk(&shared, dir.path(), &authority)),
            || (),
            |_, _| Rendered {
                answer: (),
                cited: vec!["a.rs".to_string(), "b.rs".to_string()],
                claim: Claim::Rows,
            },
            || changed = true,
        );
        assert!(
            !check.all_confirmed(),
            "control: a.rs was not confirmed: {check:?}"
        );
        assert!(!changed, "an unread file is not a re-index");
    }

    fn disk<'a>(
        shared: &'a SharedIndex,
        root: &'a std::path::Path,
        authority: &'a crate::live_index::index_lifecycle::activation::ProjectSourceAuthority,
    ) -> AnswerDisk<'a> {
        AnswerDisk {
            shared,
            expected_gen: shared.current_project_generation(),
            repo_root: root,
            authority,
            may_reindex: true,
            scope: None,
        }
    }

    /// Item 1a / research D5: a cited file that differs is re-indexed and the
    /// query runs once more; when it differs again the answer names it and
    /// the loop stops (three renders: first, re-run, final).
    #[test]
    fn a_file_that_changes_again_is_named_not_looped_on() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.rs");
        std::fs::write(&path, "fn a() {}\n").unwrap();
        let shared = crate::live_index::LiveIndex::load(dir.path()).unwrap();
        let authority =
            crate::live_index::index_lifecycle::activation::project_source_authority(dir.path());
        std::fs::write(&path, "fn a() { 1 }\n").unwrap();
        let mut renders = 0;
        let (_, _, check) = answer_checked(
            Ok(disk(&shared, dir.path(), &authority)),
            || (),
            |_, _| {
                renders += 1;
                if renders == 2 {
                    // The re-run's file changes once more under it.
                    std::fs::write(&path, "fn a() { 22 }\n").unwrap();
                }
                Rendered {
                    answer: (),
                    cited: vec!["a.rs".to_string()],
                    claim: Claim::Rows,
                }
            },
            || {},
        );
        assert_eq!(renders, 3, "first run, one re-run, final render");
        assert_eq!(
            check.notes(false),
            format!("Not confirmed against disk: a.rs ({CHANGED_DURING_ANSWER})")
        );
        // Control: the first change was re-indexed before the re-run.
        assert!(
            shared
                .read()
                .get_file("a.rs")
                .is_some_and(|file| file.content.as_slice() != b"fn a() {}\n")
        );
    }

    /// Item 1b/1c: only a whole claim compares the rest of the scope, and only
    /// a whole claim past the bound says how many files it did not compare.
    #[test]
    fn only_a_whole_claim_reaches_past_its_rows() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["a.rs", "b.rs", "c.rs"] {
            std::fs::write(dir.path().join(name), "fn f() {}\n").unwrap();
        }
        let shared = crate::live_index::LiveIndex::load(dir.path()).unwrap();
        let authority =
            crate::live_index::index_lifecycle::activation::project_source_authority(dir.path());
        let disk = disk(&shared, dir.path(), &authority);
        let rendered = |claim| Rendered {
            answer: (),
            cited: vec!["a.rs".to_string()],
            claim,
        };
        assert_eq!(
            paths_to_compare_bounded(&disk, &rendered(Claim::Rows), 2),
            (vec!["a.rs".to_string()], 0),
            "a partial answer compares its rows only"
        );
        assert_eq!(
            paths_to_compare_bounded(&disk, &rendered(Claim::Whole), 2),
            (vec!["a.rs".to_string(), "b.rs".to_string()], 1),
            "a whole claim fills the bound and counts the rest"
        );
    }

    /// Review round 3 item 6: one answer re-indexes at most
    /// `MAX_REINDEX_PER_ANSWER` files; the rest are named as left to the
    /// background reconcile, never parsed on the request thread.
    #[test]
    fn one_answer_reindexes_a_bounded_batch() {
        const EXTRA: usize = 3;
        let dir = tempfile::tempdir().unwrap();
        let names: Vec<String> = (0..super::MAX_REINDEX_PER_ANSWER + EXTRA)
            .map(|n| format!("f{n:03}.rs"))
            .collect();
        for name in &names {
            std::fs::write(dir.path().join(name), "fn f() {}\n").unwrap();
        }
        let shared = crate::live_index::LiveIndex::load(dir.path()).unwrap();
        let authority =
            crate::live_index::index_lifecycle::activation::project_source_authority(dir.path());
        for name in &names {
            std::fs::write(dir.path().join(name), "fn f() { 1 }\n").unwrap();
        }
        let (_, _, check) = answer_checked(
            Ok(disk(&shared, dir.path(), &authority)),
            || (),
            |_, _| Rendered {
                answer: (),
                cited: names.clone(),
                claim: Claim::Rows,
            },
            || {},
        );
        let deferred = check
            .unconfirmed
            .iter()
            .filter(|(_, cause)| cause == NOT_REINDEXED_REFRESHING)
            .count();
        assert_eq!(deferred, EXTRA, "{check:?}");
        assert_eq!(check.unconfirmed.len(), EXTRA, "{check:?}");
    }
}
