//! In-process git operations via libgit2.
//!
//! Replaces all `crate::process_util::hidden_command("git")` usage with library calls.
//! Zero child processes, zero console windows, faster execution.

use std::path::Path;

/// Thin wrapper around `git2::Repository`.
pub struct GitRepo {
    repo: git2::Repository,
}

/// A single commit from the log, with the list of files it touched.
#[derive(Debug, Clone)]
pub struct LogEntry {
    /// Short hash (first 7 chars).
    pub hash: String,
    /// ISO-8601 timestamp string.
    pub timestamp: String,
    /// Unix timestamp in seconds.
    pub unix_timestamp: i64,
    /// Author name.
    pub author: String,
    /// First line of commit message.
    pub message: String,
    /// Relative file paths touched by this commit.
    pub files: Vec<String>,
}

impl GitRepo {
    /// Open the repository at the given root path.
    pub fn open(root: &Path) -> Result<Self, String> {
        let repo = git2::Repository::discover(root)
            .map_err(|e| format!("failed to open git repository: {e}"))?;
        Ok(Self { repo })
    }

    /// Return the set of paths tracked by the git index (staged tree), using
    /// `git ls-files` semantics: every entry currently recorded in the index.
    ///
    /// Paths are normalized to forward slashes to match the rest of SymForge's
    /// relative-path convention. This is the authoritative "is this file under
    /// version control?" source — the `ignore` crate has no tracked-files concept,
    /// so it cannot answer this question.
    ///
    /// Returns `Err` when the index cannot be read (e.g. a freshly `git init`-ed
    /// repo with no index yet). Callers treat that as fail-open (no tracked set).
    pub fn tracked_paths(&self) -> Result<Vec<String>, String> {
        let index = self
            .repo
            .index()
            .map_err(|e| format!("cannot read git index: {e}"))?;

        let mut paths: Vec<String> = index
            .iter()
            .filter_map(|entry| String::from_utf8(entry.path).ok())
            .map(|p| p.replace('\\', "/"))
            .collect();
        paths.sort();
        paths.dedup();

        Ok(paths)
    }

    /// Return paths with uncommitted changes (staged + unstaged + untracked).
    ///
    /// Replaces: `git status --porcelain --untracked-files=all`
    pub fn uncommitted_paths(&self) -> Result<Vec<String>, String> {
        let mut opts = git2::StatusOptions::new();
        opts.include_untracked(true).recurse_untracked_dirs(true);

        let statuses = self
            .repo
            .statuses(Some(&mut opts))
            .map_err(|e| format!("git status failed: {e}"))?;

        let paths: Vec<String> = statuses
            .iter()
            .filter(|entry| !entry.status().is_ignored())
            // git2 0.21 changed StatusEntry::path() from Option<&str> to
            // Result<&str, Error> (UTF-8 validation); `.ok()` preserves the
            // prior "skip non-UTF-8 paths" behavior the Option-returning API gave.
            .filter_map(|entry| entry.path().ok().map(|p| p.replace('\\', "/")))
            .collect();

        Ok(paths)
    }

    /// Return untracked working-tree paths only, excluding ignored and staged files.
    pub fn untracked_paths(&self) -> Result<Vec<String>, String> {
        let mut opts = git2::StatusOptions::new();
        opts.include_untracked(true)
            .include_ignored(false)
            .recurse_untracked_dirs(true);

        let statuses = self
            .repo
            .statuses(Some(&mut opts))
            .map_err(|e| format!("git status failed: {e}"))?;

        let mut paths: Vec<String> = statuses
            .iter()
            .filter(|entry| {
                let status = entry.status();
                status.is_wt_new() && !status.is_index_new() && !status.is_ignored()
            })
            // git2 0.21 changed StatusEntry::path() from Option<&str> to
            // Result<&str, Error> (UTF-8 validation); `.ok()` preserves the
            // prior "skip non-UTF-8 paths" behavior the Option-returning API gave.
            .filter_map(|entry| entry.path().ok().map(|p| p.replace('\\', "/")))
            .collect();
        paths.sort();
        paths.dedup();

        Ok(paths)
    }

    /// Return file paths changed between two refs (using merge-base for 3-dot semantics).
    ///
    /// Replaces: `git diff --name-only base...target`
    pub fn changed_paths_between_refs(
        &self,
        base: &str,
        target: &str,
    ) -> Result<Vec<String>, String> {
        let base_obj = self
            .repo
            .revparse_single(base)
            .map_err(|e| format!("cannot resolve ref '{base}': {e}"))?;
        let target_obj = self
            .repo
            .revparse_single(target)
            .map_err(|e| format!("cannot resolve ref '{target}': {e}"))?;

        // Use merge-base for 3-dot diff semantics (matches `git diff base...target`).
        let merge_base_oid = self
            .repo
            .merge_base(base_obj.id(), target_obj.id())
            .map_err(|e| format!("cannot find merge base: {e}"))?;
        let merge_base_tree = self
            .repo
            .find_commit(merge_base_oid)
            .map_err(|e| format!("cannot find merge base commit: {e}"))?
            .tree()
            .map_err(|e| format!("cannot get merge base tree: {e}"))?;

        let target_tree = target_obj
            .peel_to_tree()
            .map_err(|e| format!("cannot peel target to tree: {e}"))?;

        let diff = self
            .repo
            .diff_tree_to_tree(Some(&merge_base_tree), Some(&target_tree), None)
            .map_err(|e| format!("diff failed: {e}"))?;

        Ok(collect_diff_paths(&diff))
    }

    /// Return file paths changed between a ref and the working tree.
    ///
    /// Replaces: `git diff --name-only <ref> --`
    pub fn changed_paths_from_ref(&self, reference: &str) -> Result<Vec<String>, String> {
        let obj = self
            .repo
            .revparse_single(reference)
            .map_err(|e| format!("cannot resolve ref '{reference}': {e}"))?;
        let tree = obj
            .peel_to_tree()
            .map_err(|e| format!("cannot peel to tree: {e}"))?;

        let diff = self
            .repo
            .diff_tree_to_workdir_with_index(Some(&tree), None)
            .map_err(|e| format!("diff failed: {e}"))?;

        Ok(collect_diff_paths(&diff))
    }

    /// Union of ref-range diff + working-tree changes; repo-relative POSIX
    /// paths; deduped (Program 015 S1a, `detect_impact` — see
    /// `specs/015-cbm-capability-ports/planning/sprint-1-quick-wins-spec.md`
    /// § Git merge helper).
    ///
    /// Algorithm:
    /// 1. `since` set -> [`Self::changed_paths_between_refs`]`(since, "HEAD")`,
    ///    or the working-tree variant ([`Self::changed_paths_from_ref`]`("HEAD")`)
    ///    when `since` is the literal sentinel `"WORKTREE"`.
    /// 2. Else `base_branch` set -> three-dot diff vs `HEAD`.
    /// 3. Always merge [`Self::uncommitted_paths`] when `include_untracked`
    ///    (staged + unstaged + untracked).
    /// 4. Dedupe, sort, reject any path escaping the repo root.
    ///
    /// No shell is invoked (this crate uses `git2`, never `crate::process_util::hidden_command("git")`
    /// for production paths — see module docs), so shell-metacharacter
    /// injection does not apply here; an invalid `since`/`base_branch` ref is
    /// still rejected via `git2`'s own ref resolution error.
    pub fn merge_git_changed_paths(
        &self,
        base_branch: Option<&str>,
        since: Option<&str>,
        include_untracked: bool,
    ) -> Result<Vec<String>, String> {
        let since = since.map(str::trim).filter(|s| !s.is_empty());
        let base_branch = base_branch.map(str::trim).filter(|s| !s.is_empty());

        let mut paths: Vec<String> = if let Some(since_ref) = since {
            if since_ref == "WORKTREE" {
                self.changed_paths_from_ref("HEAD")?
            } else {
                self.changed_paths_between_refs(since_ref, "HEAD")?
            }
        } else if let Some(base) = base_branch {
            self.changed_paths_between_refs(base, "HEAD")?
        } else {
            Vec::new()
        };

        if include_untracked {
            paths.extend(self.uncommitted_paths()?);
        }

        paths.retain(|p| is_repo_relative(p));
        paths.sort();
        paths.dedup();
        Ok(paths)
    }

    /// Resolve a ref/revspec to its commit OID, or `None` when it does not
    /// exist. Used by `detect_impact`'s default base resolution to prefer
    /// `origin/main` over a possibly-stale local `main` (dogfood Wave 1 Fix 6).
    pub fn resolve_ref_commit(&self, reference: &str) -> Option<git2::Oid> {
        let obj = self.repo.revparse_single(reference).ok()?;
        obj.peel_to_commit().ok().map(|commit| commit.id())
    }

    /// Ahead/behind commit counts of `local` relative to `upstream`, as
    /// `(ahead, behind)`: `ahead` = commits reachable from `local` but not
    /// `upstream` (unpushed work); `behind` = commits reachable from `upstream`
    /// but not `local` (staleness). Returns `None` when the two share no common
    /// ancestor, where the direction is not a meaningful scalar.
    pub fn ahead_behind(&self, local: git2::Oid, upstream: git2::Oid) -> Option<(usize, usize)> {
        self.repo.merge_base(local, upstream).ok()?;
        self.repo.graph_ahead_behind(local, upstream).ok()
    }

    /// Read file content at a specific git ref. Returns None if the file doesn't exist at that ref.
    ///
    /// Replaces: `git show <ref>:<path>`
    pub fn file_at_ref(&self, reference: &str, path: &str) -> Result<Option<String>, String> {
        let obj = match self.repo.revparse_single(reference) {
            Ok(obj) => obj,
            Err(_) => return Ok(None),
        };
        let tree = match obj.peel_to_tree() {
            Ok(tree) => tree,
            Err(_) => return Ok(None),
        };
        let entry = match tree.get_path(Path::new(path)) {
            Ok(entry) => entry,
            Err(_) => return Ok(None),
        };
        let blob = entry
            .to_object(&self.repo)
            .map_err(|e| format!("cannot read object: {e}"))?;
        let blob = match blob.as_blob() {
            Some(b) => b,
            None => return Ok(None),
        };

        // Skip binary files.
        if blob.is_binary() {
            return Ok(None);
        }

        Ok(String::from_utf8(blob.content().to_vec()).ok())
    }

    /// Read file content from the working tree (on disk). Returns None if the file doesn't exist.
    ///
    /// Used for uncommitted-mode diffs where the target is the current working tree
    /// rather than a git ref.
    pub fn file_from_workdir(&self, path: &str) -> Result<Option<String>, String> {
        let Some(workdir) = self.repo.workdir() else {
            return Err("bare repository has no working directory".to_string());
        };
        let full_path = workdir.join(path);
        if !full_path.is_file() {
            return Ok(None);
        }
        match std::fs::read(&full_path) {
            Ok(bytes) => Ok(String::from_utf8(bytes).ok()),
            Err(e) => Err(format!("cannot read working tree file: {e}")),
        }
    }

    /// Walk the commit log and return entries with file stats.
    ///
    /// Replaces: `git log --format=... --numstat --max-count=N --since=D days ago`
    pub fn log_with_stats(
        &self,
        max_commits: usize,
        since_days: u32,
    ) -> Result<Vec<LogEntry>, String> {
        let mut revwalk = self
            .repo
            .revwalk()
            .map_err(|e| format!("revwalk failed: {e}"))?;

        revwalk
            .push_head()
            .map_err(|e| format!("cannot push HEAD: {e}"))?;
        revwalk
            .set_sorting(git2::Sort::TIME)
            .map_err(|e| format!("cannot set sorting: {e}"))?;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        let cutoff = now - (since_days as i64 * 86400);

        let mut entries = Vec::new();

        for oid_result in revwalk {
            if entries.len() >= max_commits {
                break;
            }

            let oid = oid_result.map_err(|e| format!("revwalk error: {e}"))?;
            let commit = self
                .repo
                .find_commit(oid)
                .map_err(|e| format!("cannot find commit: {e}"))?;

            let commit_time = commit.time().seconds();
            if commit_time < cutoff {
                break; // Commits are sorted by time, so we can stop early.
            }

            let commit_tree = commit
                .tree()
                .map_err(|e| format!("cannot get commit tree: {e}"))?;

            // Diff against first parent (or empty tree for root commits).
            let parent_tree = commit.parent(0).ok().and_then(|p| p.tree().ok());

            let diff = self
                .repo
                .diff_tree_to_tree(parent_tree.as_ref(), Some(&commit_tree), None)
                .map_err(|e| format!("diff failed: {e}"))?;

            let files = collect_diff_paths(&diff);

            let sig = commit.author();
            let hash_full = oid.to_string();
            let hash = if hash_full.len() >= 7 {
                hash_full[..7].to_string()
            } else {
                hash_full
            };

            // Format ISO-8601 timestamp.
            let time = commit.time();
            let timestamp = format_git_timestamp(time.seconds(), time.offset_minutes());

            let message = commit
                .message()
                .unwrap_or("")
                .lines()
                .next()
                .unwrap_or("")
                .to_string();

            entries.push(LogEntry {
                hash,
                timestamp,
                unix_timestamp: commit_time,
                author: sig.name().unwrap_or("unknown").to_string(),
                message,
                files,
            });
        }

        Ok(entries)
    }
}

/// Count commits reachable from `to` but not from `from`, equivalent to
/// `git rev-list --count <from>..<to>`.
///
/// Returns `Ok(None)` when the two refs share no common ancestor (e.g., one
/// branch was rebased onto unrelated history, or an orphan branch was created).
/// In that case the distance is not a meaningful scalar.
pub fn commit_distance(from: &str, to: &str, repo_root: &Path) -> Result<Option<u32>, String> {
    let repo = git2::Repository::discover(repo_root)
        .map_err(|e| format!("failed to open git repository: {e}"))?;
    let from_oid = repo
        .revparse_single(from)
        .map_err(|e| format!("cannot resolve ref '{from}': {e}"))?
        .id();
    let to_oid = repo
        .revparse_single(to)
        .map_err(|e| format!("cannot resolve ref '{to}': {e}"))?
        .id();
    match repo.merge_base(from_oid, to_oid) {
        Ok(_) => {}
        Err(e) if e.code() == git2::ErrorCode::NotFound => return Ok(None),
        Err(e) => return Err(format!("merge_base failed: {e}")),
    }
    // graph_ahead_behind(local, upstream):
    //   ahead  = commits reachable from local not from upstream
    //   behind = commits reachable from upstream not from local
    // For `from..to` (commits in `to` not in `from`) set local=to, upstream=from
    // and read the `ahead` count.
    let (ahead, _behind) = repo
        .graph_ahead_behind(to_oid, from_oid)
        .map_err(|e| format!("graph_ahead_behind failed: {e}"))?;
    Ok(Some(ahead as u32))
}

/// True when `path` is a well-behaved repo-relative path: not absolute, and
/// no `..` component. Defense in depth for [`GitRepo::merge_git_changed_paths`]
/// — `git2` diffs are already tree-relative and cannot escape the repo root,
/// but a malformed or crafted delta path must never leak outside the scope
/// this tool operates on.
fn is_repo_relative(path: &str) -> bool {
    // `Path::is_absolute()` requires a drive letter on Windows, so a
    // leading `/` (git's own path separator) would otherwise slip through.
    if path.starts_with('/') || path.starts_with('\\') {
        return false;
    }
    let p = Path::new(path);
    !p.is_absolute()
        && !p
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
}

/// Collect changed file paths from a git2 diff.
fn collect_diff_paths(diff: &git2::Diff<'_>) -> Vec<String> {
    let mut paths = Vec::new();
    for delta in diff.deltas() {
        if let Some(path) = delta.new_file().path().or_else(|| delta.old_file().path())
            && let Some(s) = path.to_str()
        {
            paths.push(s.replace('\\', "/"));
        }
    }
    paths
}

/// Format a unix timestamp + offset into ISO-8601 string.
///
/// This is hand-rolled to avoid pulling in `chrono` or `time` as a dependency
/// for a single formatting use case. The date conversion delegates to
/// [`days_to_ymd`] which implements the Hinnant civil calendar algorithm.
/// Correctness is covered by unit tests in this module.
fn format_git_timestamp(secs: i64, offset_minutes: i32) -> String {
    let total_offset_secs = (offset_minutes as i64) * 60;
    let adjusted = secs + total_offset_secs;

    // Simple UTC conversion — good enough for display.
    let days_since_epoch = adjusted / 86400;
    let time_of_day = adjusted.rem_euclid(86400);
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    // Approximate date from days since epoch (good enough for display).
    let (year, month, day) = days_to_ymd(days_since_epoch);

    let sign = if offset_minutes >= 0 { '+' } else { '-' };
    let abs_offset = offset_minutes.unsigned_abs();
    let off_h = abs_offset / 60;
    let off_m = abs_offset % 60;

    format!(
        "{year:04}-{month:02}-{day:02}T{hours:02}:{minutes:02}:{seconds:02}{sign}{off_h:02}:{off_m:02}"
    )
}

/// Return the full SHA of HEAD.
///
/// Handles detached HEAD gracefully: when HEAD points directly to a commit
/// rather than a branch tip, the commit SHA is still returned.
///
/// Equivalent of `git rev-parse HEAD`.
pub fn head_sha(repo_root: &Path) -> Result<String, String> {
    let repo = git2::Repository::discover(repo_root)
        .map_err(|e| format!("failed to open git repository: {e}"))?;
    let commit = repo
        .revparse_single("HEAD")
        .map_err(|e| format!("cannot resolve HEAD: {e}"))?
        .peel_to_commit()
        .map_err(|e| format!("cannot peel HEAD to commit: {e}"))?;
    Ok(commit.id().to_string())
}

/// Convert days since Unix epoch to (year, month, day).
///
/// Implements Howard Hinnant's civil calendar algorithm
/// (<https://howardhinnant.github.io/date_algorithms.html>).
/// Hand-rolled to avoid a `chrono`/`time` dependency for this single use case.
/// Correctness is covered by unit tests in this module.
fn days_to_ymd(days: i64) -> (i64, u32, u32) {
    // Civil calendar algorithm from Howard Hinnant.
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Create a temp git repo with a few commits for testing.
    fn make_test_repo() -> (tempfile::TempDir, GitRepo) {
        let dir = tempfile::tempdir().expect("create temp dir");
        let root = dir.path();

        // Use git CLI for repo setup only (not production code).
        let run = |args: &[&str]| {
            crate::process_util::hidden_command("git")
                .args(args)
                .current_dir(root)
                .output()
                .expect("git command");
        };

        run(&["init"]);
        run(&["config", "user.email", "test@test.com"]);
        run(&["config", "user.name", "Test"]);

        fs::write(root.join("file1.rs"), "fn main() {}").unwrap();
        run(&["add", "."]);
        run(&["commit", "-m", "initial"]);

        fs::write(root.join("file2.rs"), "fn helper() {}").unwrap();
        fs::write(root.join("README.md"), "# test").unwrap();
        run(&["add", "."]);
        run(&["commit", "-m", "add files"]);

        let repo = GitRepo::open(root).expect("open repo");
        (dir, repo)
    }

    #[test]
    fn test_open_repo() {
        let (dir, _repo) = make_test_repo();
        assert!(GitRepo::open(dir.path()).is_ok());
    }

    #[test]
    fn test_open_nonexistent_fails() {
        assert!(GitRepo::open(Path::new("/nonexistent/path")).is_err());
    }

    #[test]
    fn test_file_at_ref() {
        let (_dir, repo) = make_test_repo();
        let content = repo.file_at_ref("HEAD", "file1.rs").unwrap();
        assert_eq!(content, Some("fn main() {}".to_string()));
    }

    #[test]
    fn test_file_at_ref_missing_file() {
        let (_dir, repo) = make_test_repo();
        let content = repo.file_at_ref("HEAD", "nonexistent.rs").unwrap();
        assert_eq!(content, None);
    }

    #[test]
    fn test_file_at_ref_previous_commit() {
        let (_dir, repo) = make_test_repo();
        // file2.rs didn't exist in the first commit
        let content = repo.file_at_ref("HEAD~1", "file2.rs").unwrap();
        assert_eq!(content, None);
        // but file1.rs did
        let content = repo.file_at_ref("HEAD~1", "file1.rs").unwrap();
        assert_eq!(content, Some("fn main() {}".to_string()));
    }

    #[test]
    fn test_changed_paths_between_refs() {
        let (_dir, repo) = make_test_repo();
        let paths = repo.changed_paths_between_refs("HEAD~1", "HEAD").unwrap();
        assert!(paths.contains(&"file2.rs".to_string()));
        assert!(paths.contains(&"README.md".to_string()));
        assert!(!paths.contains(&"file1.rs".to_string()));
    }

    #[test]
    fn test_uncommitted_paths_clean() {
        let (_dir, repo) = make_test_repo();
        let paths = repo.uncommitted_paths().unwrap();
        assert!(
            paths.is_empty(),
            "clean repo should have no uncommitted paths"
        );
    }

    #[test]
    fn test_uncommitted_paths_with_changes() {
        let (dir, repo) = make_test_repo();
        fs::write(dir.path().join("new_file.rs"), "fn new() {}").unwrap();
        let paths = repo.uncommitted_paths().unwrap();
        assert!(paths.contains(&"new_file.rs".to_string()));
    }

    #[test]
    fn test_untracked_paths_returns_only_worktree_new_files() {
        let (dir, repo) = make_test_repo();
        fs::write(dir.path().join("file1.rs"), "fn changed() {}").unwrap();
        fs::write(dir.path().join("new_file.rs"), "fn new() {}").unwrap();
        fs::write(dir.path().join("staged_new.rs"), "fn staged() {}").unwrap();
        crate::process_util::hidden_command("git")
            .args(["add", "staged_new.rs"])
            .current_dir(dir.path())
            .output()
            .expect("git add staged file");

        let paths = repo.untracked_paths().unwrap();

        assert_eq!(paths, vec!["new_file.rs".to_string()]);
    }

    #[test]
    fn test_tracked_paths_lists_committed_files() {
        let (dir, repo) = make_test_repo();
        // Add a brand-new untracked file: it must NOT appear in tracked_paths.
        fs::write(dir.path().join("scratch.rs"), "fn scratch() {}").unwrap();

        let tracked = repo.tracked_paths().unwrap();

        assert!(tracked.contains(&"file1.rs".to_string()));
        assert!(tracked.contains(&"file2.rs".to_string()));
        assert!(tracked.contains(&"README.md".to_string()));
        assert!(
            !tracked.contains(&"scratch.rs".to_string()),
            "an untracked working-tree file must not be reported as tracked"
        );
    }

    #[test]
    fn test_tracked_paths_empty_repo_has_no_tracked_files() {
        let dir = tempfile::tempdir().expect("create temp dir");
        crate::process_util::hidden_command("git")
            .arg("init")
            .current_dir(dir.path())
            .output()
            .expect("git init");
        let repo = GitRepo::open(dir.path()).expect("open repo");
        // A fresh repo with no commits and nothing staged has an empty index.
        let tracked = repo.tracked_paths().unwrap();
        assert!(
            tracked.is_empty(),
            "fresh repo should report no tracked paths, got {tracked:?}"
        );
    }

    #[test]
    fn test_log_with_stats() {
        let (_dir, repo) = make_test_repo();
        let entries = repo.log_with_stats(10, 90).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].message, "add files");
        assert!(entries[0].files.contains(&"file2.rs".to_string()));
        assert_eq!(entries[1].message, "initial");
    }

    #[test]
    fn test_log_max_commits() {
        let (_dir, repo) = make_test_repo();
        let entries = repo.log_with_stats(1, 90).unwrap();
        assert_eq!(entries.len(), 1);
    }

    #[test]
    fn test_head_sha_returns_full_sha() {
        let (dir, _repo) = make_test_repo();
        let sha = head_sha(dir.path()).expect("head_sha");
        assert_eq!(sha.len(), 40, "expected 40-char full SHA, got {sha:?}");
        assert!(
            sha.chars().all(|c| c.is_ascii_hexdigit()),
            "SHA should be hex: {sha}"
        );
    }

    #[test]
    fn test_head_sha_matches_rev_parse() {
        let (dir, _repo) = make_test_repo();
        let cli_sha = String::from_utf8(
            crate::process_util::hidden_command("git")
                .args(["rev-parse", "HEAD"])
                .current_dir(dir.path())
                .output()
                .expect("git rev-parse")
                .stdout,
        )
        .unwrap()
        .trim()
        .to_string();
        let ours = head_sha(dir.path()).expect("head_sha");
        assert_eq!(ours, cli_sha);
    }

    #[test]
    fn test_head_sha_detached_head() {
        let (dir, _repo) = make_test_repo();
        // Detach HEAD onto the first commit.
        let output = crate::process_util::hidden_command("git")
            .args(["rev-parse", "HEAD~1"])
            .current_dir(dir.path())
            .output()
            .expect("git rev-parse HEAD~1");
        let first_commit = String::from_utf8(output.stdout).unwrap().trim().to_string();

        crate::process_util::hidden_command("git")
            .args(["checkout", "--detach", &first_commit])
            .current_dir(dir.path())
            .output()
            .expect("git checkout --detach");

        let sha = head_sha(dir.path()).expect("head_sha on detached HEAD");
        assert_eq!(
            sha, first_commit,
            "detached HEAD should return the commit SHA it points at"
        );
    }

    #[test]
    fn test_head_sha_no_commits_errors() {
        let dir = tempfile::tempdir().expect("create temp dir");
        crate::process_util::hidden_command("git")
            .arg("init")
            .current_dir(dir.path())
            .output()
            .expect("git init");
        // Fresh repo with no commits: HEAD points to unborn branch.
        assert!(head_sha(dir.path()).is_err());
    }

    #[test]
    fn test_head_sha_not_a_repo_errors() {
        let dir = tempfile::tempdir().expect("create temp dir");
        assert!(head_sha(dir.path()).is_err());
    }

    #[test]
    fn test_format_git_timestamp() {
        let ts = format_git_timestamp(1710000000, 0);
        assert!(ts.contains("2024"), "timestamp should contain year: {ts}");
        assert!(ts.contains("+00:00"), "UTC offset: {ts}");
    }

    #[test]
    fn test_commit_distance_same_ref() {
        let (dir, _repo) = make_test_repo();
        let result = commit_distance("HEAD", "HEAD", dir.path()).unwrap();
        assert_eq!(result, Some(0));
    }

    #[test]
    fn test_commit_distance_forward() {
        // `make_test_repo` creates two commits. HEAD~1 -> HEAD is 1 commit ahead.
        let (dir, _repo) = make_test_repo();
        let result = commit_distance("HEAD~1", "HEAD", dir.path()).unwrap();
        assert_eq!(result, Some(1));
    }

    #[test]
    fn test_commit_distance_backward() {
        // Going from HEAD to HEAD~1 is 0 (HEAD~1 is an ancestor of HEAD).
        let (dir, _repo) = make_test_repo();
        let result = commit_distance("HEAD", "HEAD~1", dir.path()).unwrap();
        assert_eq!(result, Some(0));
    }

    #[test]
    fn test_commit_distance_invalid_ref() {
        let (dir, _repo) = make_test_repo();
        let result = commit_distance("no_such_ref", "HEAD", dir.path());
        assert!(result.is_err(), "expected error for invalid ref");
    }

    #[test]
    fn test_is_repo_relative() {
        assert!(is_repo_relative("src/main.rs"));
        assert!(is_repo_relative("a.rs"));
        assert!(!is_repo_relative("../secret.txt"));
        assert!(!is_repo_relative("src/../../escape.rs"));
        assert!(!is_repo_relative("/etc/passwd"));
    }

    #[test]
    fn test_merge_git_changed_paths_base_branch() {
        let (_dir, repo) = make_test_repo();
        // make_test_repo's second commit touches file2.rs + README.md.
        let paths = repo
            .merge_git_changed_paths(Some("HEAD~1"), None, false)
            .unwrap();
        assert_eq!(paths, vec!["README.md".to_string(), "file2.rs".to_string()]);
    }

    #[test]
    fn test_merge_git_changed_paths_since_overrides_base_branch() {
        let (_dir, repo) = make_test_repo();
        // `since` wins even when a (bogus) base_branch is also supplied.
        let paths = repo
            .merge_git_changed_paths(Some("no_such_branch"), Some("HEAD~1"), false)
            .unwrap();
        assert_eq!(paths, vec!["README.md".to_string(), "file2.rs".to_string()]);
    }

    #[test]
    fn test_merge_git_changed_paths_worktree_sentinel() {
        let (dir, repo) = make_test_repo();
        fs::write(dir.path().join("file1.rs"), "fn changed() {}").unwrap();
        // WORKTREE diffs tracked changes (HEAD vs working tree) without pulling
        // in untracked files, unlike `include_untracked`.
        let paths = repo
            .merge_git_changed_paths(None, Some("WORKTREE"), false)
            .unwrap();
        assert_eq!(paths, vec!["file1.rs".to_string()]);
    }

    #[test]
    fn test_merge_git_changed_paths_include_untracked_merges() {
        let (dir, repo) = make_test_repo();
        fs::write(dir.path().join("new_file.rs"), "fn new() {}").unwrap();
        let paths = repo.merge_git_changed_paths(None, None, true).unwrap();
        assert_eq!(paths, vec!["new_file.rs".to_string()]);

        let paths_excluded = repo.merge_git_changed_paths(None, None, false).unwrap();
        assert!(
            paths_excluded.is_empty(),
            "include_untracked=false must not merge working-tree changes"
        );
    }

    #[test]
    fn test_merge_git_changed_paths_dedup_sorted() {
        let (dir, repo) = make_test_repo();
        // file2.rs is both part of the HEAD~1..HEAD diff and currently re-modified.
        fs::write(dir.path().join("file2.rs"), "fn helper() { /* changed */ }").unwrap();
        let paths = repo
            .merge_git_changed_paths(Some("HEAD~1"), None, true)
            .unwrap();
        assert_eq!(paths, vec!["README.md".to_string(), "file2.rs".to_string()]);
    }

    #[test]
    fn test_merge_git_changed_paths_invalid_ref_rejected() {
        let (_dir, repo) = make_test_repo();
        let result = repo.merge_git_changed_paths(Some("no_such_ref_at_all"), None, false);
        assert!(
            result.is_err(),
            "an invalid base_branch ref must be rejected"
        );
    }

    #[test]
    fn test_commit_distance_no_common_ancestor() {
        let (dir, _repo) = make_test_repo();
        let root = dir.path();
        let run = |args: &[&str]| {
            crate::process_util::hidden_command("git")
                .args(args)
                .current_dir(root)
                .output()
                .expect("git command");
        };

        // Tag the current tip so we can reference it after switching branches.
        run(&["tag", "original"]);
        // Create an orphan branch (no parents, no shared history).
        run(&["checkout", "--orphan", "orphan_branch"]);
        fs::write(root.join("orphan.rs"), "fn orphan() {}").unwrap();
        run(&["add", "orphan.rs"]);
        run(&["commit", "-m", "orphan commit"]);

        let result = commit_distance("original", "HEAD", root).unwrap();
        assert_eq!(result, None, "no common ancestor should yield None");

        // And the reverse direction too.
        let result = commit_distance("HEAD", "original", root).unwrap();
        assert_eq!(result, None);
    }
}
