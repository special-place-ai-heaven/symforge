//! Git temporal intelligence — enriches the index with git history metadata.
//!
//! Computes per-file churn scores (exponential-decay weighted), ownership
//! distribution, co-change coupling (Jaccard coefficient), and repo-wide
//! hotspot summaries using libgit2 via [`crate::git::GitRepo`].
//!
//! Design principles:
//! - In-process git access: uses libgit2 (via git2 crate) — no child
//!   processes, no console windows, faster execution.
//! - Bounded: max 500 commits OR 90 days, whichever is smaller.
//! - Exponential decay: half-life of 14 days so recent activity dominates.
//! - Rank-normalized churn: percentile position across all tracked files
//!   (0.0 = coldest, 1.0 = hottest in repo) — meaningful regardless of
//!   absolute activity level.
//! - Jaccard co-change: `|A∩B| / |A∪B|` filters out high-frequency noise
//!   files (lock files, CI configs) that appear in many unrelated commits.
//! - Mega-commit filter: commits touching >50 files are excluded from
//!   co-change analysis to avoid pollution from bulk reformats/merges.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use super::store::SharedIndex;

// ── Background computation ──────────────────────────────────────────────

/// Spawn a background task that computes the git temporal index and swaps
/// it into the shared handle. Non-blocking — returns immediately.
///
/// Call after `LiveIndex::load()` or `SharedIndexHandle::reload()` completes.
pub fn spawn_git_temporal_computation(index: SharedIndex, repo_root: PathBuf, expected_gen: u64) {
    // Guard: only spawn if a tokio runtime is available (not the case in some sync tests).
    if tokio::runtime::Handle::try_current().is_err() {
        return;
    }

    // If data is already Ready, keep serving it while we recompute in the background.
    let was_ready = index.git_temporal().state == GitTemporalState::Ready;

    if !was_ready
        && !index.update_git_temporal_at_generation(
            GitTemporalIndex {
                state: GitTemporalState::Computing,
                ..GitTemporalIndex::pending()
            },
            expected_gen,
        )
    {
        tracing::trace!(
            expected_gen,
            "stale git temporal computing-state publication skipped"
        );
    }

    tokio::spawn(async move {
        // Run the computation on a blocking thread (it uses libgit2 which does I/O).
        let result =
            tokio::task::spawn_blocking(move || GitTemporalIndex::compute(&repo_root)).await;

        match result {
            Ok(temporal) => {
                let files = temporal.files.len();
                let commits = temporal.stats.total_commits_analyzed;
                let duration_ms = temporal.stats.compute_duration.as_millis() as u64;
                if index.update_git_temporal_at_generation(temporal, expected_gen) {
                    tracing::info!(files, commits, duration_ms, "git temporal index computed");
                } else {
                    tracing::trace!(
                        expected_gen,
                        files,
                        commits,
                        duration_ms,
                        "stale git temporal publication skipped"
                    );
                }
            }
            Err(error) => {
                tracing::warn!("git temporal computation panicked: {error}");
                if !was_ready {
                    let unavailable =
                        GitTemporalIndex::unavailable(format!("computation panicked: {error}"));
                    if !index.update_git_temporal_at_generation(unavailable, expected_gen) {
                        tracing::trace!(
                            expected_gen,
                            "stale git temporal panic-state publication skipped"
                        );
                    }
                }
            }
        }
    });
}

// ── Configuration constants ─────────────────────────────────────────────

const MAX_COMMITS: u32 = 500;
const WINDOW_DAYS: u32 = 90;
/// Exponential decay half-life in days. A commit 14 days ago has half the
/// weight of one today; 28 days ago has a quarter, etc.
const HALF_LIFE_DAYS: f64 = 14.0;
/// Maximum co-changed files shown per file.
const CO_CHANGE_CAP_PER_FILE: usize = 5;
/// Top hotspots in the repo-wide stats.
const HOTSPOT_CAP: usize = 10;
/// Top coupled pairs in the repo-wide stats.
const COUPLED_PAIRS_CAP: usize = 10;
/// Minimum shared commits before a co-change pair is considered.
const MIN_SHARED_COMMITS: u32 = 2;
/// Minimum Jaccard coefficient to keep a co-change entry.
const MIN_JACCARD: f32 = 0.15;
/// Weak co-change candidates are surfaced only when strong matches are absent.
/// This keeps the main signal high-trust while still avoiding a blank wall for
/// files with some history but no qualifying strong pair.
const WEAK_MIN_JACCARD: f32 = 0.05;
/// Maximum low-confidence co-changed files shown per file.
const WEAK_CO_CHANGE_CAP_PER_FILE: usize = 3;
/// Maximum contributors shown per file.
const CONTRIBUTOR_CAP: usize = 5;
/// Commits touching more files than this are excluded from co-change
/// analysis (likely merges, formatting runs, bulk renames).
const MEGA_COMMIT_THRESHOLD: usize = 50;

// ── Public data types ───────────────────────────────────────────────────

/// Per-file temporal metadata derived from git history.
#[derive(Debug, Clone)]
pub struct GitFileHistory {
    /// Total commits touching this file within the analysis window.
    pub commit_count: u32,
    /// Recency-weighted churn score, rank-normalized to 0.0–1.0 across the
    /// entire repo. Uses exponential decay with a 14-day half-life so
    /// recent commits dominate. Rank-normalized means the hottest file in
    /// the repo is always ~1.0, the coldest ~0.0.
    pub churn_score: f32,
    /// Most recent commit touching this file.
    pub last_commit: CommitSummary,
    /// Ownership distribution — who actually maintains this file, sorted by
    /// commit share descending. Capped at top 5 contributors.
    pub contributors: Vec<ContributorShare>,
    /// Files that co-change with this one, ranked by Jaccard coupling
    /// strength. Capped at top 5.
    pub co_changes: Vec<CoChangeEntry>,
    /// Lower-confidence co-change candidates that missed the strong threshold
    /// but still have some evidence. Only shown when strong matches are absent.
    pub weak_co_changes: Vec<CoChangeEntry>,
}

/// Summary of a single git commit (cheap to clone, display-ready).
#[derive(Debug, Clone)]
pub struct CommitSummary {
    /// Short hash (7 chars).
    pub hash: String,
    /// ISO 8601 author date for display.
    pub timestamp: String,
    /// Author name.
    pub author: String,
    /// First line of commit message, truncated to 72 chars.
    pub message_head: String,
    /// Days ago from the time of computation (for relative time display).
    pub days_ago: f64,
}

/// One contributor's share of a file's commit history.
#[derive(Debug, Clone)]
pub struct ContributorShare {
    pub author: String,
    pub commit_count: u32,
    /// Percentage of total commits to this file (0.0–100.0).
    pub percentage: f32,
}

/// One co-change relationship for a file.
#[derive(Debug, Clone)]
pub struct CoChangeEntry {
    /// Path of the co-changed file.
    pub path: String,
    /// Jaccard coefficient: `|shared_commits| / |union_commits|`, 0.0–1.0.
    pub coupling_score: f32,
    /// Raw number of commits where both files changed together.
    pub shared_commits: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CoChangeStrength {
    Strong,
    Weak,
}

fn classify_co_change_pair(
    shared: u32,
    count_a: u32,
    count_b: u32,
) -> Option<(f32, CoChangeStrength)> {
    if shared == 0 {
        return None;
    }
    let union = count_a + count_b - shared;
    if union == 0 {
        return None;
    }
    let jaccard = shared as f32 / union as f32;
    if shared >= MIN_SHARED_COMMITS && jaccard >= MIN_JACCARD {
        Some((jaccard, CoChangeStrength::Strong))
    } else if jaccard >= WEAK_MIN_JACCARD {
        Some((jaccard, CoChangeStrength::Weak))
    } else {
        None
    }
}

fn sort_and_cap_co_changes(entries: &mut Vec<CoChangeEntry>, cap: usize) {
    entries.sort_by(|a, b| {
        b.coupling_score
            .partial_cmp(&a.coupling_score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.shared_commits.cmp(&a.shared_commits))
            .then_with(|| a.path.cmp(&b.path))
    });
    entries.truncate(cap);
}

/// Repo-wide temporal summary for health reports.
#[derive(Debug, Clone)]
pub struct GitTemporalStats {
    /// Total commits analyzed in this computation.
    pub total_commits_analyzed: u32,
    /// Analysis window in days (from config, currently 90).
    pub analysis_window_days: u32,
    /// Top hotspot files by churn score.
    pub hotspots: Vec<(String, f32)>,
    /// Top coupled file pairs by Jaccard coefficient.
    pub most_coupled: Vec<(String, String, f32)>,
    /// Wall-clock time when computation completed.
    pub computed_at: SystemTime,
    /// Time spent computing the temporal index.
    pub compute_duration: Duration,
}

/// The full temporal index — a side-table that lives parallel to the
/// main `LiveIndex` on `SharedIndexHandle`.
#[derive(Debug, Clone)]
pub struct GitTemporalIndex {
    /// Per-file temporal metadata, keyed by relative path (forward-slash
    /// normalized, same key space as `LiveIndex::files`).
    pub files: HashMap<String, GitFileHistory>,
    /// Repo-wide summary statistics.
    pub stats: GitTemporalStats,
    /// Current state of the temporal index.
    pub state: GitTemporalState,
}

/// Lifecycle state of the temporal index.
#[derive(Debug, Clone, PartialEq)]
pub enum GitTemporalState {
    /// Not yet computed (initial state).
    Pending,
    /// Background computation is in progress.
    Computing,
    /// Computation completed — data is available.
    Ready,
    /// Git is unavailable or the directory is not a git repo.
    Unavailable(String),
}

// ── Intermediate parsing types (private) ────────────────────────────────

#[derive(Debug)]
struct ParsedCommit {
    hash: String,
    timestamp: String,
    author: String,
    message: String,
    /// Days before computation time (0.0 = today).
    days_ago: f64,
    /// Relative file paths touched by this commit.
    files: Vec<String>,
}

// ── Rendering helpers (public) ──────────────────────────────────────────

/// Render a 10-character visual churn bar: `██████░░░░`
pub fn churn_bar(score: f32) -> String {
    let clamped = score.clamp(0.0, 1.0);
    let filled = (clamped * 10.0).round() as usize;
    let empty = 10_usize.saturating_sub(filled);
    format!("{}{}", "█".repeat(filled), "░".repeat(empty))
}

/// Human-readable churn label for a normalized score.
pub fn churn_label(score: f32) -> &'static str {
    if score >= 0.8 {
        "critical"
    } else if score >= 0.6 {
        "hot"
    } else if score >= 0.4 {
        "warm"
    } else if score >= 0.2 {
        "cool"
    } else {
        "frozen"
    }
}

/// Format days-ago as a compact relative time string: "3d ago", "2w ago", etc.
pub fn relative_time(days_ago: f64) -> String {
    if days_ago < 0.0 {
        return "just now".to_string();
    }
    if days_ago < 1.0 {
        return "today".to_string();
    }
    if days_ago < 7.0 {
        return format!("{}d ago", days_ago.round() as u32);
    }
    if days_ago < 30.0 {
        return format!("{}w ago", (days_ago / 7.0).round() as u32);
    }
    format!("{}mo ago", (days_ago / 30.0).round() as u32)
}

// ── Core implementation ─────────────────────────────────────────────────

impl GitTemporalIndex {
    /// Construct a pending (empty) temporal index.
    pub fn pending() -> Self {
        Self {
            files: HashMap::new(),
            stats: GitTemporalStats {
                total_commits_analyzed: 0,
                analysis_window_days: WINDOW_DAYS,
                hotspots: Vec::new(),
                most_coupled: Vec::new(),
                computed_at: SystemTime::now(),
                compute_duration: Duration::ZERO,
            },
            state: GitTemporalState::Pending,
        }
    }

    /// Construct an unavailable temporal index with a reason.
    pub fn unavailable(reason: String) -> Self {
        Self {
            state: GitTemporalState::Unavailable(reason),
            ..Self::pending()
        }
    }

    /// Compute the full temporal index from git history.
    ///
    /// Uses libgit2 via [`crate::git::GitRepo`] to walk the commit log
    /// and build per-file metrics, co-change relationships, and repo-wide
    /// stats. Designed to run on a blocking thread.
    pub fn compute(repo_root: &Path) -> Self {
        let start = Instant::now();

        let commits = match load_commits(repo_root) {
            Ok(c) => c,
            Err(reason) => return Self::unavailable(reason),
        };
        if commits.is_empty() {
            return Self {
                files: HashMap::new(),
                stats: GitTemporalStats {
                    total_commits_analyzed: 0,
                    analysis_window_days: WINDOW_DAYS,
                    hotspots: Vec::new(),
                    most_coupled: Vec::new(),
                    computed_at: SystemTime::now(),
                    compute_duration: start.elapsed(),
                },
                state: GitTemporalState::Ready,
            };
        }

        let total_commits = commits.len() as u32;
        let decay_lambda = (2.0_f64).ln() / HALF_LIFE_DAYS;

        // ── Phase 1: Per-file aggregation ───────────────────────────────

        // file -> list of commit indices (for co-change Jaccard denominators)
        let mut file_commit_indices: HashMap<String, Vec<usize>> = HashMap::new();
        // file -> author -> commit count
        let mut file_authors: HashMap<String, HashMap<String, u32>> = HashMap::new();
        // file -> index of most recent commit (smallest days_ago)
        let mut file_last_commit_idx: HashMap<String, usize> = HashMap::new();
        // file -> sum of decay-weighted commit scores
        let mut file_raw_churn: HashMap<String, f64> = HashMap::new();

        for (idx, commit) in commits.iter().enumerate() {
            let weight = (-decay_lambda * commit.days_ago).exp();

            // Dedup file paths per commit to avoid inflating Jaccard denominators
            // (renames or merges can list the same file twice in a single commit).
            let unique_files: HashSet<&String> = commit.files.iter().collect();
            for file_path in unique_files {
                file_commit_indices
                    .entry(file_path.clone())
                    .or_default()
                    .push(idx);

                *file_authors
                    .entry(file_path.clone())
                    .or_default()
                    .entry(commit.author.clone())
                    .or_insert(0) += 1;

                file_last_commit_idx
                    .entry(file_path.clone())
                    .and_modify(|existing| {
                        if commit.days_ago < commits[*existing].days_ago {
                            *existing = idx;
                        }
                    })
                    .or_insert(idx);

                *file_raw_churn.entry(file_path.clone()).or_insert(0.0) += weight;
            }
        }

        // ── Phase 2: Rank-normalize churn scores ────────────────────────

        let mut churn_entries: Vec<(String, f64)> = file_raw_churn.into_iter().collect();
        churn_entries.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

        let file_count = churn_entries.len();
        let mut normalized_churn: HashMap<String, f32> = HashMap::with_capacity(file_count);
        for (rank, (path, _raw)) in churn_entries.iter().enumerate() {
            let score = if file_count <= 1 {
                if churn_entries[0].1 > 0.0 { 1.0 } else { 0.0 }
            } else {
                rank as f32 / (file_count - 1) as f32
            };
            normalized_churn.insert(path.clone(), score);
        }

        // ── Phase 3: Co-change matrix (Jaccard) ────────────────────────

        // Count how many commits each pair of files shares.
        let mut pair_counts: HashMap<(String, String), u32> = HashMap::new();

        for commit in &commits {
            let mut sorted_files: Vec<&str> = commit.files.iter().map(|s| s.as_str()).collect();
            sorted_files.sort_unstable();
            sorted_files.dedup();

            // Skip mega-commits (merges, bulk reformats, etc.)
            if sorted_files.len() > MEGA_COMMIT_THRESHOLD {
                continue;
            }

            for i in 0..sorted_files.len() {
                for j in (i + 1)..sorted_files.len() {
                    // Canonical ordering: alphabetically smaller path first.
                    let key = (sorted_files[i].to_string(), sorted_files[j].to_string());
                    *pair_counts.entry(key).or_insert(0) += 1;
                }
            }
        }

        // Compute Jaccard for each qualifying pair.
        let mut file_co_changes: HashMap<String, Vec<CoChangeEntry>> = HashMap::new();
        let mut weak_file_co_changes: HashMap<String, Vec<CoChangeEntry>> = HashMap::new();

        for ((file_a, file_b), shared) in &pair_counts {
            let count_a = file_commit_indices
                .get(file_a)
                .map(|v| v.len() as u32)
                .unwrap_or(0);
            let count_b = file_commit_indices
                .get(file_b)
                .map(|v| v.len() as u32)
                .unwrap_or(0);
            let Some((jaccard, strength)) = classify_co_change_pair(*shared, count_a, count_b)
            else {
                continue;
            };

            let target = match strength {
                CoChangeStrength::Strong => &mut file_co_changes,
                CoChangeStrength::Weak => &mut weak_file_co_changes,
            };

            // Bidirectional: A sees B, B sees A.
            target
                .entry(file_a.clone())
                .or_default()
                .push(CoChangeEntry {
                    path: file_b.clone(),
                    coupling_score: jaccard,
                    shared_commits: *shared,
                });
            target
                .entry(file_b.clone())
                .or_default()
                .push(CoChangeEntry {
                    path: file_a.clone(),
                    coupling_score: jaccard,
                    shared_commits: *shared,
                });
        }

        // Sort by coupling strength descending and cap per file.
        for entries in file_co_changes.values_mut() {
            sort_and_cap_co_changes(entries, CO_CHANGE_CAP_PER_FILE);
        }
        for entries in weak_file_co_changes.values_mut() {
            sort_and_cap_co_changes(entries, WEAK_CO_CHANGE_CAP_PER_FILE);
        }

        // ── Phase 4: Assemble GitFileHistory per file ───────────────────

        let mut files: HashMap<String, GitFileHistory> = HashMap::with_capacity(file_count);

        for (path, _commit_indices) in &file_commit_indices {
            let commit_count = _commit_indices.len() as u32;
            let churn_score = normalized_churn.get(path).copied().unwrap_or(0.0);

            // Last commit
            let last_idx = file_last_commit_idx.get(path).copied().unwrap_or(0);
            let last = &commits[last_idx];
            let last_commit = CommitSummary {
                hash: last.hash.clone(),
                timestamp: last.timestamp.clone(),
                author: last.author.clone(),
                message_head: truncate_message(&last.message, 72),
                days_ago: last.days_ago,
            };

            // Contributors
            let contributors = file_authors
                .get(path)
                .map(|authors| {
                    let total = authors.values().sum::<u32>() as f32;
                    let mut shares: Vec<ContributorShare> = authors
                        .iter()
                        .map(|(author, count)| ContributorShare {
                            author: author.clone(),
                            commit_count: *count,
                            percentage: (*count as f32 / total) * 100.0,
                        })
                        .collect();
                    shares.sort_by(|a, b| {
                        b.percentage
                            .partial_cmp(&a.percentage)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });
                    shares.truncate(CONTRIBUTOR_CAP);
                    shares
                })
                .unwrap_or_default();

            let co_changes = file_co_changes.remove(path).unwrap_or_default();
            let weak_co_changes = weak_file_co_changes.remove(path).unwrap_or_default();

            files.insert(
                path.clone(),
                GitFileHistory {
                    commit_count,
                    churn_score,
                    last_commit,
                    contributors,
                    co_changes,
                    weak_co_changes,
                },
            );
        }

        // ── Phase 5: Repo-wide stats ────────────────────────────────────

        let mut hotspots: Vec<(String, f32)> = files
            .iter()
            .map(|(path, h)| (path.clone(), h.churn_score))
            .collect();
        hotspots.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        hotspots.truncate(HOTSPOT_CAP);

        let mut most_coupled: Vec<(String, String, f32)> = pair_counts
            .iter()
            .filter_map(|((a, b), shared)| {
                let count_a = file_commit_indices
                    .get(a)
                    .map(|v| v.len() as u32)
                    .unwrap_or(0);
                let count_b = file_commit_indices
                    .get(b)
                    .map(|v| v.len() as u32)
                    .unwrap_or(0);
                match classify_co_change_pair(*shared, count_a, count_b) {
                    Some((jaccard, CoChangeStrength::Strong)) => {
                        Some((a.clone(), b.clone(), jaccard))
                    }
                    _ => None,
                }
            })
            .collect();
        most_coupled.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
        most_coupled.truncate(COUPLED_PAIRS_CAP);

        Self {
            files,
            stats: GitTemporalStats {
                total_commits_analyzed: total_commits,
                analysis_window_days: WINDOW_DAYS,
                hotspots,
                most_coupled,
                computed_at: SystemTime::now(),
                compute_duration: start.elapsed(),
            },
            state: GitTemporalState::Ready,
        }
    }
}

// ── Git log via libgit2 ─────────────────────────────────────────────────

/// Load commits from git history using libgit2 (no child processes).
fn load_commits(repo_root: &Path) -> Result<Vec<ParsedCommit>, String> {
    use crate::git::GitRepo;

    let repo = GitRepo::open(repo_root)?;
    let entries = repo.log_with_stats(MAX_COMMITS as usize, WINDOW_DAYS)?;

    let now = SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as f64;

    Ok(entries
        .into_iter()
        .map(|e| {
            let days_ago = (now - e.unix_timestamp as f64) / 86400.0;
            ParsedCommit {
                hash: e.hash,
                timestamp: e.timestamp,
                author: e.author,
                message: e.message,
                days_ago: days_ago.max(0.0),
                files: e.files,
            }
        })
        .collect())
}

/// Truncate a message to `max_len` characters, appending "..." if truncated.
fn truncate_message(msg: &str, max_len: usize) -> String {
    if msg.chars().count() <= max_len {
        msg.to_string()
    } else {
        let truncated: String = msg.chars().take(max_len.saturating_sub(3)).collect();
        format!("{truncated}...")
    }
}

// ── Tests ───────────────────────────────────────────────────────────────

// ── Test-only parsing infrastructure ────────────────────────────────────
//
// These were the original CLI-based parsing functions. They are retained
// solely to support the existing unit tests that feed synthetic `git log`
// strings into `compute_from_log`. Compiled only in test builds.

#[cfg(test)]
const COMMIT_DELIMITER: &str = "SYMFORGE_GIT_TEMPORAL_DELIM";

#[cfg(test)]
fn parse_git_log(raw: &str, now_unix: u64) -> Vec<ParsedCommit> {
    let mut commits: Vec<ParsedCommit> = Vec::new();
    let mut current: Option<ParsedCommitBuilder> = None;

    for line in raw.lines() {
        let line = line.trim_end();

        if line == COMMIT_DELIMITER {
            if let Some(builder) = current.take()
                && let Some(commit) = builder.build()
            {
                commits.push(commit);
            }
            current = Some(ParsedCommitBuilder::new());
            continue;
        }

        let Some(builder) = current.as_mut() else {
            continue;
        };

        if let Some(rest) = line.strip_prefix("H:") {
            builder.hash = rest.to_string();
        } else if let Some(rest) = line.strip_prefix("U:") {
            if let Ok(unix_ts) = rest.parse::<u64>() {
                builder.unix_timestamp = Some(unix_ts);
                builder.days_ago = (now_unix as f64 - unix_ts as f64) / 86400.0;
            }
        } else if let Some(rest) = line.strip_prefix("D:") {
            builder.timestamp = rest.to_string();
        } else if let Some(rest) = line.strip_prefix("A:") {
            builder.author = rest.to_string();
        } else if let Some(rest) = line.strip_prefix("M:") {
            builder.message = rest.to_string();
        } else if !line.is_empty()
            && let Some(path) = parse_numstat_line(line)
        {
            builder.files.push(normalize_git_path(&path));
        }
    }

    if let Some(builder) = current.take()
        && let Some(commit) = builder.build()
    {
        commits.push(commit);
    }

    commits
}

#[cfg(test)]
fn parse_numstat_line(line: &str) -> Option<String> {
    let mut parts = line.splitn(3, '\t');
    let added = parts.next()?;
    let _removed = parts.next()?;
    let path = parts.next()?;

    if added == "-" {
        return None;
    }
    if !added.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if path.is_empty() {
        return None;
    }

    Some(path.to_string())
}

#[cfg(test)]
fn normalize_git_path(path: &str) -> String {
    path.replace('\\', "/")
}

#[cfg(test)]
#[derive(Debug, Default)]
struct ParsedCommitBuilder {
    hash: String,
    timestamp: String,
    author: String,
    message: String,
    unix_timestamp: Option<u64>,
    days_ago: f64,
    files: Vec<String>,
}

#[cfg(test)]
impl ParsedCommitBuilder {
    fn new() -> Self {
        Self::default()
    }

    fn build(self) -> Option<ParsedCommit> {
        if self.hash.is_empty() || self.files.is_empty() {
            return None;
        }
        Some(ParsedCommit {
            hash: self.hash,
            timestamp: self.timestamp,
            author: self.author,
            message: self.message,
            days_ago: self.days_ago,
            files: self.files,
        })
    }
}

// ── Test-only helper (separate impl block to keep test infra out of prod) ──

#[cfg(test)]
impl GitTemporalIndex {
    /// Compute from a pre-built log string (skips the `git log` subprocess).
    fn compute_from_log(raw_log: &str, now_unix: u64) -> Self {
        let start = Instant::now();
        let commits = parse_git_log(raw_log, now_unix);
        // Re-use the same computation logic — just inline the post-parse path.
        // We duplicate a bit to avoid making the `run_git_log` call.
        Self::compute_from_parsed(commits, start)
    }

    fn compute_from_parsed(commits: Vec<ParsedCommit>, start: Instant) -> Self {
        if commits.is_empty() {
            return Self {
                files: HashMap::new(),
                stats: GitTemporalStats {
                    total_commits_analyzed: 0,
                    analysis_window_days: WINDOW_DAYS,
                    hotspots: Vec::new(),
                    most_coupled: Vec::new(),
                    computed_at: SystemTime::now(),
                    compute_duration: start.elapsed(),
                },
                state: GitTemporalState::Ready,
            };
        }

        let total_commits = commits.len() as u32;
        let decay_lambda = (2.0_f64).ln() / HALF_LIFE_DAYS;

        let mut file_commit_indices: HashMap<String, Vec<usize>> = HashMap::new();
        let mut file_authors: HashMap<String, HashMap<String, u32>> = HashMap::new();
        let mut file_last_commit_idx: HashMap<String, usize> = HashMap::new();
        let mut file_raw_churn: HashMap<String, f64> = HashMap::new();

        for (idx, commit) in commits.iter().enumerate() {
            let weight = (-decay_lambda * commit.days_ago).exp();
            for file_path in &commit.files {
                file_commit_indices
                    .entry(file_path.clone())
                    .or_default()
                    .push(idx);
                *file_authors
                    .entry(file_path.clone())
                    .or_default()
                    .entry(commit.author.clone())
                    .or_insert(0) += 1;
                file_last_commit_idx
                    .entry(file_path.clone())
                    .and_modify(|existing| {
                        if commit.days_ago < commits[*existing].days_ago {
                            *existing = idx;
                        }
                    })
                    .or_insert(idx);
                *file_raw_churn.entry(file_path.clone()).or_insert(0.0) += weight;
            }
        }

        let mut churn_entries: Vec<(String, f64)> = file_raw_churn.into_iter().collect();
        churn_entries.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        let file_count = churn_entries.len();
        let mut normalized_churn: HashMap<String, f32> = HashMap::with_capacity(file_count);
        for (rank, (path, _)) in churn_entries.iter().enumerate() {
            let score = if file_count <= 1 {
                if churn_entries[0].1 > 0.0 { 1.0 } else { 0.0 }
            } else {
                rank as f32 / (file_count - 1) as f32
            };
            normalized_churn.insert(path.clone(), score);
        }

        let mut pair_counts: HashMap<(String, String), u32> = HashMap::new();
        for commit in &commits {
            let mut sorted_files: Vec<&str> = commit.files.iter().map(|s| s.as_str()).collect();
            sorted_files.sort_unstable();
            sorted_files.dedup();
            if sorted_files.len() > MEGA_COMMIT_THRESHOLD {
                continue;
            }
            for i in 0..sorted_files.len() {
                for j in (i + 1)..sorted_files.len() {
                    let key = (sorted_files[i].to_string(), sorted_files[j].to_string());
                    *pair_counts.entry(key).or_insert(0) += 1;
                }
            }
        }

        let mut file_co_changes: HashMap<String, Vec<CoChangeEntry>> = HashMap::new();
        let mut weak_file_co_changes: HashMap<String, Vec<CoChangeEntry>> = HashMap::new();
        for ((file_a, file_b), shared) in &pair_counts {
            let count_a = file_commit_indices
                .get(file_a)
                .map(|v| v.len() as u32)
                .unwrap_or(0);
            let count_b = file_commit_indices
                .get(file_b)
                .map(|v| v.len() as u32)
                .unwrap_or(0);
            let Some((jaccard, strength)) = classify_co_change_pair(*shared, count_a, count_b)
            else {
                continue;
            };
            let target = match strength {
                CoChangeStrength::Strong => &mut file_co_changes,
                CoChangeStrength::Weak => &mut weak_file_co_changes,
            };
            target
                .entry(file_a.clone())
                .or_default()
                .push(CoChangeEntry {
                    path: file_b.clone(),
                    coupling_score: jaccard,
                    shared_commits: *shared,
                });
            target
                .entry(file_b.clone())
                .or_default()
                .push(CoChangeEntry {
                    path: file_a.clone(),
                    coupling_score: jaccard,
                    shared_commits: *shared,
                });
        }
        for entries in file_co_changes.values_mut() {
            sort_and_cap_co_changes(entries, CO_CHANGE_CAP_PER_FILE);
        }
        for entries in weak_file_co_changes.values_mut() {
            sort_and_cap_co_changes(entries, WEAK_CO_CHANGE_CAP_PER_FILE);
        }

        let mut files: HashMap<String, GitFileHistory> = HashMap::with_capacity(file_count);
        for (path, commit_indices) in &file_commit_indices {
            let commit_count = commit_indices.len() as u32;
            let churn_score = normalized_churn.get(path).copied().unwrap_or(0.0);
            let last_idx = file_last_commit_idx.get(path).copied().unwrap_or(0);
            let last = &commits[last_idx];
            let last_commit = CommitSummary {
                hash: last.hash.clone(),
                timestamp: last.timestamp.clone(),
                author: last.author.clone(),
                message_head: truncate_message(&last.message, 72),
                days_ago: last.days_ago,
            };
            let contributors = file_authors
                .get(path)
                .map(|authors| {
                    let total = authors.values().sum::<u32>() as f32;
                    let mut shares: Vec<ContributorShare> = authors
                        .iter()
                        .map(|(author, count)| ContributorShare {
                            author: author.clone(),
                            commit_count: *count,
                            percentage: (*count as f32 / total) * 100.0,
                        })
                        .collect();
                    shares.sort_by(|a, b| {
                        b.percentage
                            .partial_cmp(&a.percentage)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });
                    shares.truncate(CONTRIBUTOR_CAP);
                    shares
                })
                .unwrap_or_default();
            let co_changes = file_co_changes.remove(path).unwrap_or_default();
            let weak_co_changes = weak_file_co_changes.remove(path).unwrap_or_default();
            files.insert(
                path.clone(),
                GitFileHistory {
                    commit_count,
                    churn_score,
                    last_commit,
                    contributors,
                    co_changes,
                    weak_co_changes,
                },
            );
        }

        let mut hotspots: Vec<(String, f32)> = files
            .iter()
            .map(|(p, h)| (p.clone(), h.churn_score))
            .collect();
        hotspots.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        hotspots.truncate(HOTSPOT_CAP);

        let mut most_coupled: Vec<(String, String, f32)> = pair_counts
            .iter()
            .filter_map(|((a, b), shared)| {
                let ca = file_commit_indices
                    .get(a)
                    .map(|v| v.len() as u32)
                    .unwrap_or(0);
                let cb = file_commit_indices
                    .get(b)
                    .map(|v| v.len() as u32)
                    .unwrap_or(0);
                match classify_co_change_pair(*shared, ca, cb) {
                    Some((j, CoChangeStrength::Strong)) => Some((a.clone(), b.clone(), j)),
                    _ => None,
                }
            })
            .collect();
        most_coupled.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
        most_coupled.truncate(COUPLED_PAIRS_CAP);

        Self {
            files,
            stats: GitTemporalStats {
                total_commits_analyzed: total_commits,
                analysis_window_days: WINDOW_DAYS,
                hotspots,
                most_coupled,
                computed_at: SystemTime::now(),
                compute_duration: start.elapsed(),
            },
            state: GitTemporalState::Ready,
        }
    }
}

#[cfg(test)]
mod tests;
