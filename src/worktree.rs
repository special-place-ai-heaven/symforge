//! Worktree-aware path resolution for edit tools.
//!
//! SymForge indexes one repo path and stores absolute file paths in its
//! index. When an agent runs from inside a parallel `git worktree` that
//! shares the same `.git` objects, an edit that resolves against the
//! indexed absolute path silently writes to the *indexed* repo copy
//! instead of the agent's own working tree. The resolver below implements
//! explicit call-time routing through the `working_directory` edit parameter.
//!
//! The edit handlers call [`resolve_target_path`] before writing. When
//! `working_directory` is `None`, behavior is byte-identical to pre-routing
//! releases (write to the indexed absolute path). When `Some`, the path
//! is re-rooted against the working directory and validated against a
//! cached `git worktree list`, refreshed on cache miss.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use parking_lot::Mutex;

use crate::capability::WorktreeRoutingPolicy;
use crate::protocol::edit_hooks::{EditContext, EditHook};

pub const WORKTREE_ROUTING_ENV: &str = "SYMFORGE_WORKTREE_AWARE";

/// A worktree known to be associated with the indexed root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeEntry {
    /// Canonical absolute path to the worktree root (main or linked).
    pub path: PathBuf,
}

/// Outcome of [`resolve_target_path`]: where the edit should land, plus
/// where the index believes the file lives, plus a flag flipping true
/// iff the two differ.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedTarget {
    /// Absolute path the tool should write to.
    pub target_path: PathBuf,
    /// Absolute path from the index (always the indexed-root copy).
    pub indexed_path: PathBuf,
    /// `true` when `target_path != indexed_path`.
    pub rerouted: bool,
}

/// Errors produced by worktree-aware resolution.
///
/// `Display` intentionally leads with the variant name so MCP tool output
/// exposes a stable, grep-friendly error tag (see
/// `tests/worktree_awareness.rs` AC4/AC6). Hints follow as plain prose so
/// callers can act without looking up docs.
#[derive(Debug, thiserror::Error)]
pub enum WorktreeError {
    #[error(
        "WorkingDirectoryNotARecognizedWorktree: working_directory `{}` is not a recognized worktree of `{}` — {hint}",
        cwd.display(), indexed_root.display()
    )]
    WorkingDirectoryNotARecognizedWorktree {
        cwd: PathBuf,
        indexed_root: PathBuf,
        hint: String,
    },

    #[error("TargetFileMissing: target file `{}` does not exist — {hint}", path.display())]
    TargetFileMissing { path: PathBuf, hint: String },

    #[error(
        "PathOutsideIndexedRoot: indexed path `{}` is not under the indexed root `{}`",
        path.display(), indexed_root.display()
    )]
    PathOutsideIndexedRoot {
        path: PathBuf,
        indexed_root: PathBuf,
    },

    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),

    #[error("`git worktree list` failed: {0}")]
    GitWorktreeList(String),

    #[error(
        "WorktreeRoutingDisabledByPolicy: working_directory `{}` requested worktree routing, but worktree routing is disabled by policy — {hint}",
        cwd.display()
    )]
    WorktreeRoutingDisabledByPolicy { cwd: PathBuf, hint: String },
}

/// Canonicalize a path, returning a clean form without the `\\?\` UNC
/// prefix on Windows. Delegates to `dunce::canonicalize` which falls
/// back to `std::fs::canonicalize` on non-Windows.
///
/// The returned path has no trailing separator — `dunce` and the
/// platform canonicalizer both normalize this away.
pub fn canonicalize(path: &Path) -> std::io::Result<PathBuf> {
    dunce::canonicalize(path)
}

/// Best-effort canonicalize. Falls back to the input when the target
/// does not exist — useful for paths produced by symbolic lookups that
/// may name files not yet on disk.
fn canonicalize_or_identity(path: &Path) -> PathBuf {
    canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Cache of known worktrees for one indexed root. Populated on demand
/// by `refresh()`; `lookup()` refreshes once on cache miss before
/// concluding the directory is unknown.
#[derive(Debug, Default)]
pub struct WorktreeCache {
    entries: HashMap<PathBuf, WorktreeEntry>,
    last_refreshed: Option<Instant>,
}

impl WorktreeCache {
    /// Empty cache. The first `lookup` will trigger a refresh.
    pub fn new() -> Self {
        Self::default()
    }

    /// Inject a worktree entry directly. Primarily used by tests that
    /// want to exercise the resolution algorithm without shelling out
    /// to `git`.
    pub fn insert(&mut self, path: PathBuf) {
        self.entries.insert(path.clone(), WorktreeEntry { path });
    }

    /// Number of cached worktree entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the cache holds no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Timestamp of the most recent successful refresh, if any.
    pub fn last_refreshed(&self) -> Option<Instant> {
        self.last_refreshed
    }

    /// Canonicalize `dir` and look it up. On a miss, force one refresh
    /// (cheap — single `git` invocation) and try again.
    ///
    /// Returns `Ok(Some(entry))` when the directory resolves to a
    /// known worktree, `Ok(None)` when the refresh completed cleanly
    /// but the directory is still unknown.
    pub fn lookup(
        &mut self,
        dir: &Path,
        indexed_root: &Path,
    ) -> Result<Option<WorktreeEntry>, WorktreeError> {
        let canon = canonicalize(dir)?;
        if let Some(entry) = self.entries.get(&canon) {
            return Ok(Some(entry.clone()));
        }
        self.refresh(indexed_root)?;
        Ok(self.entries.get(&canon).cloned())
    }

    /// Wipe the cache and repopulate from `git -C <indexed_root>
    /// worktree list --porcelain`. Callers rarely need this directly;
    /// `lookup` calls it on cache miss.
    pub fn refresh(&mut self, indexed_root: &Path) -> Result<(), WorktreeError> {
        let paths = list_worktrees(indexed_root)?;
        self.entries.clear();
        for path in paths {
            self.entries.insert(path.clone(), WorktreeEntry { path });
        }
        self.last_refreshed = Some(Instant::now());
        Ok(())
    }

    /// Read-only view of the cached paths in no particular order.
    /// Useful for diagnostics (e.g. building `health` output).
    pub fn paths(&self) -> impl Iterator<Item = &Path> {
        self.entries.keys().map(|p| p.as_path())
    }
}

/// Re-root an indexed absolute path against an optional working
/// directory, applying the spec §2.2 resolution algorithm.
///
/// * `working_directory = None` or equal to the canonical indexed root
///   → pass through unchanged (`rerouted = false`).
/// * Otherwise the directory must be a known worktree of the indexed
///   root (cache refresh on miss). The target file must exist in that
///   worktree; otherwise `TargetFileMissing` with a hint.
pub fn resolve_target_path(
    indexed_abs: &Path,
    indexed_root: &Path,
    working_directory: Option<&Path>,
    cache: &mut WorktreeCache,
) -> Result<ResolvedTarget, WorktreeError> {
    let Some(raw_wd) = working_directory else {
        return Ok(ResolvedTarget {
            target_path: indexed_abs.to_path_buf(),
            indexed_path: indexed_abs.to_path_buf(),
            rerouted: false,
        });
    };

    let canonical_indexed_abs = canonicalize_or_identity(indexed_abs);
    let canonical_indexed_root = canonicalize(indexed_root)?;

    // Canonicalize fails with NotFound when the supplied working_directory
    // doesn't exist on disk (e.g. a typo or a yet-to-be-created worktree
    // path). Treat that as "not a recognized worktree" rather than a raw IO
    // error — it's the same user-facing condition and the tests
    // (`matrix_cache_refresh_newly_created_worktree_accepted`) assert this.
    let canonical_wd = match canonicalize(raw_wd) {
        Ok(p) => p,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(WorktreeError::WorkingDirectoryNotARecognizedWorktree {
                cwd: raw_wd.to_path_buf(),
                indexed_root: canonical_indexed_root,
                hint: "pass a path that is the root of a `git worktree list` entry, or omit to write to the indexed root".to_string(),
            });
        }
        Err(e) => return Err(WorktreeError::Io(e)),
    };
    if canonical_wd == canonical_indexed_root {
        return Ok(ResolvedTarget {
            target_path: canonical_indexed_abs.clone(),
            indexed_path: canonical_indexed_abs,
            rerouted: false,
        });
    }

    let relative = canonical_indexed_abs
        .strip_prefix(&canonical_indexed_root)
        .map_err(|_| WorktreeError::PathOutsideIndexedRoot {
            path: canonical_indexed_abs.clone(),
            indexed_root: canonical_indexed_root.clone(),
        })?;

    let entry = cache.lookup(&canonical_wd, &canonical_indexed_root)?;
    if entry.is_none() {
        return Err(WorktreeError::WorkingDirectoryNotARecognizedWorktree {
            cwd: canonical_wd,
            indexed_root: canonical_indexed_root,
            hint: "pass a path that is the root of a `git worktree list` entry, or omit to write to the indexed root".to_string(),
        });
    }

    let target_path = canonical_wd.join(relative);
    if !target_path.exists() {
        return Err(WorktreeError::TargetFileMissing {
            path: target_path,
            hint: "the worktree may be at a commit where this file does not yet exist; check `git ls-tree HEAD <relative_path>` in the worktree".to_string(),
        });
    }

    Ok(ResolvedTarget {
        target_path,
        indexed_path: canonical_indexed_abs,
        rerouted: true,
    })
}

/// Shell out to `git -C <indexed_root> worktree list --porcelain` and
/// return the canonicalized root path of every worktree (main + linked).
///
/// We shell out rather than using `git2` because the porcelain format
/// is stable, handles every edge case (bare, detached, locked, prunable)
/// uniformly, and the call only happens on cache miss.
fn list_worktrees(indexed_root: &Path) -> Result<Vec<PathBuf>, WorktreeError> {
    let output = Command::new("git")
        .arg("-C")
        .arg(indexed_root)
        .arg("worktree")
        .arg("list")
        .arg("--porcelain")
        .output()
        .map_err(|e| WorktreeError::GitWorktreeList(format!("spawn failed: {e}")))?;

    if !output.status.success() {
        return Err(WorktreeError::GitWorktreeList(format!(
            "exit {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }

    Ok(parse_porcelain(&String::from_utf8_lossy(&output.stdout)))
}

/// Parse `git worktree list --porcelain`. Each block opens with
/// `worktree <abs-path>`; we take that line, canonicalize it, and skip
/// the rest. Blank lines separate blocks and are ignored.
fn parse_porcelain(input: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for line in input.lines() {
        let Some(rest) = line.strip_prefix("worktree ") else {
            continue;
        };
        let raw = PathBuf::from(rest.trim());
        match canonicalize(&raw) {
            Ok(canon) => out.push(canon),
            Err(_) => out.push(raw),
        }
    }
    out
}

/// [`EditHook`] implementation that reroutes edits into a sibling `git
/// worktree` when the caller supplies `working_directory`. Holds one
/// shared [`WorktreeCache`] so repeated calls in the same session avoid
/// re-shelling out to `git worktree list`.
pub struct WorktreeAwareEditHook {
    cache: Mutex<WorktreeCache>,
}

impl WorktreeAwareEditHook {
    pub fn new() -> Self {
        Self {
            cache: Mutex::new(WorktreeCache::new()),
        }
    }
}

impl Default for WorktreeAwareEditHook {
    fn default() -> Self {
        Self::new()
    }
}

impl EditHook for WorktreeAwareEditHook {
    fn resolve_target_path(&self, ctx: &EditContext) -> Result<ResolvedTarget, String> {
        if let Some(cwd) = ctx.working_directory
            && routing_policy_from_env() == WorktreeRoutingPolicy::Disabled
        {
            return Err(WorktreeError::WorktreeRoutingDisabledByPolicy {
                cwd: cwd.to_path_buf(),
                hint: format!(
                    "unset {WORKTREE_ROUTING_ENV} or set it to `1`/`true`/`on` to allow explicit call-time routing"
                ),
            }
            .to_string());
        }
        let mut cache = self.cache.lock();
        resolve_target_path(
            ctx.indexed_absolute_path,
            ctx.repo_root,
            ctx.working_directory,
            &mut cache,
        )
        .map_err(|e| e.to_string())
    }
}

/// Interpret the transitional worktree-routing env var as policy. Unset means
/// explicit call-time routing is allowed; false/off/disabled values block
/// requested routing before any write.
pub fn routing_policy_from_env() -> WorktreeRoutingPolicy {
    match std::env::var(WORKTREE_ROUTING_ENV) {
        Ok(value) => match value.trim().to_ascii_lowercase().as_str() {
            "" | "1" | "true" | "yes" | "on" | "explicit" | "explicit-call-time"
            | "explicit_call_time" => WorktreeRoutingPolicy::ExplicitCallTime,
            "0" | "false" | "no" | "off" | "disabled" | "disable" => {
                WorktreeRoutingPolicy::Disabled
            }
            _ => WorktreeRoutingPolicy::Disabled,
        },
        Err(std::env::VarError::NotPresent) => WorktreeRoutingPolicy::ExplicitCallTime,
        Err(std::env::VarError::NotUnicode(_)) => WorktreeRoutingPolicy::Disabled,
    }
}

/// Install [`WorktreeAwareEditHook`] on the process-wide edit-hook
/// registry, exactly once. Routing policy is resolved inside the hook so
/// callers can request worktree routing at call time with `working_directory`.
///
/// Safe to call repeatedly — the first caller registers the hook and
/// every subsequent call short-circuits via the internal [`OnceLock`].
pub fn register_if_feature_enabled() {
    static REGISTERED: OnceLock<()> = OnceLock::new();
    REGISTERED.get_or_init(|| {
        crate::protocol::edit_hooks::register(Box::new(WorktreeAwareEditHook::new()));
    });
}

/// Length of the `health`-visible misuse window.
const MISUSE_WINDOW: Duration = Duration::from_secs(3600);

/// Counts edit-tool calls that omitted `working_directory` while the
/// transitional worktree observability knob is on. Exposed through the
/// `health` tool as a rolling "last hour" signal so regressions stay
/// visible after the feature ships.
///
/// The window rolls lazily: [`record_missing_working_directory`] and
/// [`current_window_count`] both reset the counter when the previous
/// window has elapsed before reading or incrementing it.
#[derive(Debug)]
pub struct WorktreeMisuseCounter {
    count: AtomicU64,
    window_start: Mutex<Instant>,
}

impl WorktreeMisuseCounter {
    pub fn new() -> Self {
        Self {
            count: AtomicU64::new(0),
            window_start: Mutex::new(Instant::now()),
        }
    }

    /// Bump the counter, rolling the window first if it has elapsed.
    pub fn record_missing_working_directory(&self) {
        self.maybe_reset_window();
        self.count.fetch_add(1, Ordering::Relaxed);
    }

    /// Read the current window's count, rolling first if it has elapsed.
    pub fn current_window_count(&self) -> u64 {
        self.maybe_reset_window();
        self.count.load(Ordering::Relaxed)
    }

    fn maybe_reset_window(&self) {
        let mut start = self.window_start.lock();
        if start.elapsed() >= MISUSE_WINDOW {
            *start = Instant::now();
            self.count.store(0, Ordering::Relaxed);
        }
    }
}

impl Default for WorktreeMisuseCounter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command;

    fn run_git(args: &[&str], cwd: &Path) {
        let output = Command::new("git")
            .args(args)
            .current_dir(cwd)
            .output()
            .expect("git command");
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    /// Create a repo in `<tmp>/main` with one committed file and a
    /// linked worktree at `<tmp>/wt`. Returns canonicalized roots.
    fn make_repo_with_worktree() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let dir = tempfile::tempdir().expect("tempdir");
        let main_root = dir.path().join("main");
        let worktree_root = dir.path().join("wt");
        fs::create_dir_all(&main_root).unwrap();

        run_git(&["init", "-q"], &main_root);
        run_git(&["config", "user.email", "t@t.test"], &main_root);
        run_git(&["config", "user.name", "t"], &main_root);
        run_git(&["config", "commit.gpgsign", "false"], &main_root);
        fs::create_dir_all(main_root.join("src")).unwrap();
        fs::write(main_root.join("src/file.rs"), "fn main() {}\n").unwrap();
        run_git(&["add", "."], &main_root);
        run_git(&["commit", "-q", "-m", "init"], &main_root);
        run_git(
            &[
                "worktree",
                "add",
                "-q",
                worktree_root.to_str().unwrap(),
                "-b",
                "feature",
            ],
            &main_root,
        );

        let main_canon = canonicalize(&main_root).unwrap();
        let wt_canon = canonicalize(&worktree_root).unwrap();
        (dir, main_canon, wt_canon)
    }

    #[test]
    fn canonicalize_strips_trailing_separator() {
        let tmp = tempfile::tempdir().unwrap();
        let canon = canonicalize(tmp.path()).unwrap();

        let with_sep = {
            let mut s = tmp.path().to_string_lossy().into_owned();
            if !s.ends_with(std::path::MAIN_SEPARATOR) {
                s.push(std::path::MAIN_SEPARATOR);
            }
            canonicalize(Path::new(&s)).unwrap()
        };

        assert_eq!(canon, with_sep);
        let s = canon.to_string_lossy();
        assert!(
            !s.ends_with('/') && !s.ends_with('\\'),
            "canonicalized path kept trailing separator: {s}"
        );
    }

    #[test]
    #[cfg(windows)]
    fn canonicalize_normalizes_windows_slash_direction() {
        let tmp = tempfile::tempdir().unwrap();
        let forward = tmp.path().to_string_lossy().replace('\\', "/");
        let back = tmp.path().to_string_lossy().replace('/', "\\");
        let a = canonicalize(Path::new(&forward)).unwrap();
        let b = canonicalize(Path::new(&back)).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    #[cfg(windows)]
    fn canonicalize_does_not_produce_unc_prefix() {
        let tmp = tempfile::tempdir().unwrap();
        let canon = canonicalize(tmp.path()).unwrap();
        let s = canon.to_string_lossy();
        assert!(
            !s.starts_with(r"\\?\"),
            "dunce should have stripped the UNC prefix, got: {s}"
        );
    }

    #[test]
    fn parse_porcelain_extracts_worktree_paths() {
        let sample = "worktree /tmp/main\nHEAD abc123\nbranch refs/heads/main\n\nworktree /tmp/wt\nHEAD def456\nbranch refs/heads/feature\n";
        let paths = parse_porcelain(sample);
        assert_eq!(paths.len(), 2, "expected two worktrees, got {paths:?}");
    }

    #[test]
    fn parse_porcelain_ignores_blank_and_attribute_lines() {
        let sample = "\nworktree /only/one\nbare\n\n";
        let paths = parse_porcelain(sample);
        assert_eq!(paths.len(), 1);
    }

    #[test]
    fn new_cache_is_empty() {
        let cache = WorktreeCache::new();
        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
        assert!(cache.last_refreshed().is_none());
    }

    #[test]
    fn insert_adds_entry_without_refresh() {
        let mut cache = WorktreeCache::new();
        let tmp = tempfile::tempdir().unwrap();
        let canon = canonicalize(tmp.path()).unwrap();
        cache.insert(canon.clone());
        assert_eq!(cache.len(), 1);
        let paths: Vec<&Path> = cache.paths().collect();
        assert!(paths.contains(&canon.as_path()));
    }

    #[test]
    fn refresh_lists_main_and_linked_worktrees() {
        let (_tmp, main_root, wt_root) = make_repo_with_worktree();
        let mut cache = WorktreeCache::new();
        cache.refresh(&main_root).expect("refresh");
        assert!(cache.last_refreshed().is_some());
        assert_eq!(cache.len(), 2, "expected main + linked worktree");
        let paths: Vec<PathBuf> = cache.paths().map(Path::to_path_buf).collect();
        assert!(paths.contains(&main_root), "missing main in {paths:?}");
        assert!(paths.contains(&wt_root), "missing linked wt in {paths:?}");
    }

    #[test]
    fn lookup_returns_known_worktree() {
        let (_tmp, main_root, wt_root) = make_repo_with_worktree();
        let mut cache = WorktreeCache::new();
        let entry = cache
            .lookup(&wt_root, &main_root)
            .expect("lookup")
            .expect("entry");
        assert_eq!(entry.path, wt_root);
    }

    #[test]
    fn lookup_refreshes_on_miss() {
        let (_tmp, main_root, wt_root) = make_repo_with_worktree();
        let mut cache = WorktreeCache::new();
        assert!(cache.is_empty());
        let entry = cache.lookup(&wt_root, &main_root).expect("lookup");
        assert!(
            entry.is_some(),
            "expected cache to refresh and find worktree"
        );
        assert!(!cache.is_empty(), "refresh should have populated cache");
    }

    #[test]
    fn lookup_returns_none_for_unknown_directory() {
        let (_tmp, main_root, _wt_root) = make_repo_with_worktree();
        let stray = tempfile::tempdir().unwrap();
        let stray_canon = canonicalize(stray.path()).unwrap();
        let mut cache = WorktreeCache::new();
        let entry = cache.lookup(&stray_canon, &main_root).expect("lookup");
        assert!(
            entry.is_none(),
            "unrelated tempdir must not match a known worktree, got {entry:?}"
        );
    }

    #[test]
    fn resolve_returns_indexed_path_when_working_directory_omitted() {
        let (_tmp, main_root, _wt) = make_repo_with_worktree();
        let indexed_abs = main_root.join("src/file.rs");
        let mut cache = WorktreeCache::new();
        let resolved =
            resolve_target_path(&indexed_abs, &main_root, None, &mut cache).expect("resolve");
        assert!(!resolved.rerouted);
        assert_eq!(resolved.target_path, indexed_abs);
        assert_eq!(resolved.target_path, resolved.indexed_path);
    }

    #[test]
    fn resolve_omitted_working_directory_does_not_touch_filesystem() {
        let missing_root =
            std::env::temp_dir().join(format!("symforge-missing-root-{}", std::process::id()));
        let indexed_abs = missing_root.join("src/file.rs");
        let mut cache = WorktreeCache::new();
        let resolved = resolve_target_path(&indexed_abs, &missing_root, None, &mut cache)
            .expect("omitted working_directory should not canonicalize repo root");

        assert!(!resolved.rerouted);
        assert_eq!(resolved.target_path, indexed_abs);
        assert_eq!(resolved.target_path, resolved.indexed_path);
    }

    #[test]
    fn resolve_returns_indexed_path_when_working_directory_is_indexed_root() {
        let (_tmp, main_root, _wt) = make_repo_with_worktree();
        let indexed_abs = main_root.join("src/file.rs");
        let mut cache = WorktreeCache::new();
        let resolved = resolve_target_path(&indexed_abs, &main_root, Some(&main_root), &mut cache)
            .expect("resolve");
        assert!(!resolved.rerouted, "indexed root should not reroute");
    }

    #[test]
    fn resolve_reroutes_to_known_worktree() {
        let (_tmp, main_root, wt_root) = make_repo_with_worktree();
        let indexed_abs = main_root.join("src/file.rs");
        let mut cache = WorktreeCache::new();
        let resolved = resolve_target_path(&indexed_abs, &main_root, Some(&wt_root), &mut cache)
            .expect("resolve");
        assert!(resolved.rerouted);
        assert_eq!(resolved.target_path, wt_root.join("src/file.rs"));
        assert_eq!(resolved.indexed_path, canonicalize(&indexed_abs).unwrap());
    }

    #[test]
    fn resolve_errors_on_unknown_working_directory() {
        let (_tmp, main_root, _wt) = make_repo_with_worktree();
        let stray = tempfile::tempdir().unwrap();
        let indexed_abs = main_root.join("src/file.rs");
        let mut cache = WorktreeCache::new();
        let err = resolve_target_path(&indexed_abs, &main_root, Some(stray.path()), &mut cache)
            .expect_err("should reject unknown dir");
        assert!(
            matches!(
                err,
                WorktreeError::WorkingDirectoryNotARecognizedWorktree { .. }
            ),
            "got {err:?}"
        );
    }

    #[test]
    fn resolve_errors_when_target_file_missing() {
        let (_tmp, main_root, wt_root) = make_repo_with_worktree();
        // file exists in main; remove it in worktree so the reroute target is missing.
        fs::remove_file(wt_root.join("src/file.rs")).unwrap();
        let indexed_abs = main_root.join("src/file.rs");
        let mut cache = WorktreeCache::new();
        let err = resolve_target_path(&indexed_abs, &main_root, Some(&wt_root), &mut cache)
            .expect_err("should report missing target");
        assert!(
            matches!(err, WorktreeError::TargetFileMissing { .. }),
            "got {err:?}"
        );
    }

    #[test]
    fn resolve_errors_when_indexed_path_outside_indexed_root() {
        let (_tmp, main_root, wt_root) = make_repo_with_worktree();
        // Build an "indexed" absolute path that does not sit under main_root.
        let outside = tempfile::tempdir().unwrap();
        let outside_canon = canonicalize(outside.path()).unwrap();
        let stray_indexed = outside_canon.join("file.rs");
        fs::write(&stray_indexed, "x").unwrap();
        let mut cache = WorktreeCache::new();
        let err = resolve_target_path(&stray_indexed, &main_root, Some(&wt_root), &mut cache)
            .expect_err("should reject path outside indexed root");
        assert!(
            matches!(err, WorktreeError::PathOutsideIndexedRoot { .. }),
            "got {err:?}"
        );
    }

    #[test]
    #[cfg(windows)]
    fn resolve_normalizes_windows_slash_variants() {
        let (_tmp, main_root, wt_root) = make_repo_with_worktree();
        let indexed_abs = main_root.join("src/file.rs");
        let wt_forward = PathBuf::from(wt_root.to_string_lossy().replace('\\', "/"));
        let wt_back = PathBuf::from(wt_root.to_string_lossy().replace('/', "\\"));
        let mut cache = WorktreeCache::new();

        let a = resolve_target_path(&indexed_abs, &main_root, Some(&wt_forward), &mut cache)
            .expect("resolve forward");
        let b = resolve_target_path(&indexed_abs, &main_root, Some(&wt_back), &mut cache)
            .expect("resolve back");
        assert_eq!(a, b);
        assert!(a.rerouted);
    }
}
