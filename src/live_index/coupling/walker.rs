//! Cold-build + incremental delta walker.
//!
//! Walks bounded git history via `git2` directly so `cfg.reference_ts`
//! is honoured uniformly for both commit selection (window cutoff) and
//! temporal decay. The store carries a commit-scoped ledger alongside
//! the aggregate table so deltas can subtract commits that fall out of
//! the bounded window as new ones enter — producing exact equivalence
//! to a fresh cold-build regardless of how many times delta is applied.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use anyhow::{Context, Result, anyhow};

use super::AnchorKey;
use super::decay_factor;
use super::store::{CouplingRow, CouplingStore, LedgerEdgeRow};

pub const DEFAULT_MAX_COMMITS: usize = 500;
pub const DEFAULT_WINDOW_DAYS: u32 = 3 * 365;
pub const DEFAULT_HALF_LIFE_DAYS: u32 = 30;
pub const DEFAULT_MAX_FILES_PER_COMMIT: usize = 200;

#[derive(Debug, Clone)]
pub struct WalkerConfig {
    pub max_commits: usize,
    pub window_days: u32,
    pub half_life_days: u32,
    pub max_files_per_commit: usize,
    /// Reference timestamp for temporal decay AND commit cutoff.
    pub reference_ts: i64,
    /// When true, additionally parse new-side blobs and emit
    /// symbol-level ledger entries alongside file-level.
    pub include_symbols: bool,
}

impl WalkerConfig {
    pub fn with_now(now_ts: i64) -> Self {
        Self {
            max_commits: DEFAULT_MAX_COMMITS,
            window_days: DEFAULT_WINDOW_DAYS,
            half_life_days: DEFAULT_HALF_LIFE_DAYS,
            max_files_per_commit: DEFAULT_MAX_FILES_PER_COMMIT,
            reference_ts: now_ts,
            include_symbols: false,
        }
    }

    pub fn system_now() -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        Self::with_now(now)
    }

    fn half_life_secs(&self) -> i64 {
        self.half_life_days as i64 * 86400
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct WalkerStats {
    pub commits_scanned: usize,
    pub commits_with_pairs: usize,
    pub commits_skipped_large: usize,
    pub unique_edges: usize,
    pub rows_written: usize,
}

/// Outcome of `apply_head_delta`.
#[derive(Debug, Clone)]
pub enum DeltaOutcome {
    /// HEAD hasn't moved and reference_ts hasn't advanced since the last
    /// build; no SQL executed.
    NoOp { head_oid: String },
    /// Delta applied: some combination of rescale, subtract, and add.
    Applied {
        new_head: Option<String>,
        incoming_commits: usize,
        outgoing_commits: usize,
        rescale_factor: f64,
    },
}

/// Set of currently git-tracked paths (`git ls-files` semantics), normalized
/// to forward slashes — the liveness source fed to the cold-build dead-path
/// eviction. This is a SUPERSET of the live INDEX file set the read path
/// actually gates on (`index.files`), so pruning against it is conservative:
/// a tracked-but-unindexed file is kept, never wrongly evicted, while pairs
/// referencing files deleted from HEAD (the actual bloat) are removed.
///
/// The walker's entry points (`run_init`, `start_lazy_prepare`) only carry
/// `repo_root`, not the live `SharedIndex`, so the index file set is not in
/// scope here; `git ls-files` is the closest read-path-honest approximation
/// reachable from this layer. Returns `None` on any failure (no repo, fresh
/// `git init` with no index, empty tracked set), which the caller treats as
/// fail-open: no eviction rather than a wrongly-emptied store.
fn live_tracked_paths(repo_root: &Path) -> Option<HashSet<String>> {
    let repo = crate::git::GitRepo::open(repo_root).ok()?;
    let paths = repo.tracked_paths().ok()?;
    if paths.is_empty() {
        return None;
    }
    Some(paths.into_iter().collect())
}

/// Cold-build the coupling store from a bounded slice of git history.
/// Atomic rebuild — purges existing rows and ledger, writes fresh state, then
/// evicts dead-path pairs in the same transaction before the post-build VACUUM
/// (see [`CouplingStore::commit_cold_build`]).
pub fn cold_build(
    store: &CouplingStore,
    repo_root: &Path,
    cfg: &WalkerConfig,
) -> Result<WalkerStats> {
    let (entries, head_oid) = compute_window(repo_root, cfg)?;

    let skipped_large = entries.iter().filter(|e| e.skipped_large).count();
    let commits_with_pairs = entries.iter().filter(|e| e.commits_active).count();

    let (rows, ledger, active) = build_cold_inputs(&entries, cfg);

    let stats = WalkerStats {
        commits_scanned: commits_with_pairs + skipped_large,
        commits_with_pairs,
        commits_skipped_large: skipped_large,
        unique_edges: rows.len() / 2,
        rows_written: rows.len(),
    };

    let live_paths = live_tracked_paths(repo_root);

    store
        .commit_cold_build(
            &rows,
            &active,
            &ledger,
            head_oid.as_deref(),
            cfg.reference_ts,
            live_paths.as_ref(),
        )
        .context("committing cold build")?;
    Ok(stats)
}

/// Incrementally update the coupling graph to reflect the bounded window
/// rooted at the current HEAD. Diffs the old active commit set (from the
/// ledger) against the newly-computed window, rescales existing rows
/// from `last_reference_ts` to `cfg.reference_ts`, and runs
/// subtract+add in a single SQL transaction.
///
/// Produces exact equivalence to a fresh `cold_build(cfg)` regardless of
/// how many deltas have been applied previously — commits that fall out
/// of the bounded window are removed along with their contributions.
pub fn apply_head_delta(
    store: &CouplingStore,
    repo_root: &Path,
    cfg: &WalkerConfig,
) -> Result<DeltaOutcome> {
    let old_head = store.last_head()?;
    let old_ref_ts = store.last_reference_ts()?;

    let (entries, new_head) = compute_window(repo_root, cfg)?;

    // NoOp fast path: nothing moved.
    if new_head == old_head
        && old_ref_ts == Some(cfg.reference_ts)
        && let Some(ref h) = new_head
    {
        return Ok(DeltaOutcome::NoOp {
            head_oid: h.clone(),
        });
    }

    let old_active = store.active_commit_oids()?;
    let new_active: HashMap<String, i64> = entries
        .iter()
        .filter(|e| e.commits_active)
        .map(|e| (e.commit_oid.clone(), e.commit_ts))
        .collect();

    let old_oids: HashSet<&String> = old_active.keys().collect();
    let new_oids: HashSet<&String> = new_active.keys().collect();

    let outgoing: Vec<String> = old_oids
        .difference(&new_oids)
        .map(|s| (*s).clone())
        .collect();

    let incoming_set: HashSet<&String> = new_oids.difference(&old_oids).copied().collect();
    let incoming_commits: Vec<(String, i64)> = incoming_set
        .iter()
        .map(|oid| ((*oid).clone(), new_active[*oid]))
        .collect();
    let incoming_ledger: Vec<LedgerEdgeRow> = entries
        .iter()
        .filter(|e| incoming_set.contains(&e.commit_oid))
        .flat_map(|e| e.edges.iter().cloned())
        .collect();

    let half_life_secs = cfg.half_life_secs();
    let rescale_factor = match old_ref_ts {
        Some(old_ref) => decay_factor(cfg.reference_ts - old_ref, half_life_secs),
        None => 1.0,
    };

    store
        .commit_delta(
            &incoming_commits,
            &incoming_ledger,
            &outgoing,
            new_head.as_deref(),
            old_ref_ts,
            cfg.reference_ts,
            half_life_secs,
        )
        .context("committing coupling delta")?;

    Ok(DeltaOutcome::Applied {
        new_head,
        incoming_commits: incoming_commits.len(),
        outgoing_commits: outgoing.len(),
        rescale_factor,
    })
}

/// A commit within the bounded window. `edges` may be empty if the
/// commit contributed nothing (no pairs produced). `skipped_large` is
/// true when the commit exceeded `max_files_per_commit` and was skipped.
/// `commits_active` is true when this commit's OID belongs in the active
/// set (currently: commits with at least one emitted pair).
struct WindowEntry {
    commit_oid: String,
    commit_ts: i64,
    edges: Vec<LedgerEdgeRow>,
    commits_active: bool,
    skipped_large: bool,
}

/// Walk the bounded history once and return the active commit set with
/// their per-commit ledger edges. Shared by cold-build and delta so both
/// apply the same commit-selection and pair-emission rules.
fn compute_window(
    repo_root: &Path,
    cfg: &WalkerConfig,
) -> Result<(Vec<WindowEntry>, Option<String>)> {
    let repo =
        git2::Repository::open(repo_root).map_err(|e| anyhow!("git2 open {:?}: {e}", repo_root))?;

    let head_oid = match repo.head().ok().and_then(|h| h.target()) {
        Some(oid) => Some(oid.to_string()),
        None => return Ok((Vec::new(), None)),
    };

    let mut revwalk = repo.revwalk().map_err(|e| anyhow!("revwalk init: {e}"))?;
    revwalk.push_head().map_err(|e| anyhow!("push_head: {e}"))?;
    revwalk
        .set_sorting(git2::Sort::TIME)
        .map_err(|e| anyhow!("set_sorting: {e}"))?;

    let cutoff = cfg.reference_ts - (cfg.window_days as i64) * 86400;
    let mut out: Vec<WindowEntry> = Vec::new();

    for (walked, oid_result) in revwalk.enumerate() {
        if walked >= cfg.max_commits {
            break;
        }
        let oid = oid_result.map_err(|e| anyhow!("revwalk: {e}"))?;
        let commit = repo
            .find_commit(oid)
            .map_err(|e| anyhow!("find_commit: {e}"))?;
        let commit_ts = commit.time().seconds();
        if commit_ts < cutoff {
            break;
        }

        let commit_tree = commit.tree().map_err(|e| anyhow!("tree: {e}"))?;
        let parent_tree = commit.parent(0).ok().and_then(|p| p.tree().ok());

        let mut diff_opts = git2::DiffOptions::new();
        diff_opts.context_lines(0);
        let diff = repo
            .diff_tree_to_tree(
                parent_tree.as_ref(),
                Some(&commit_tree),
                Some(&mut diff_opts),
            )
            .map_err(|e| anyhow!("diff_tree_to_tree: {e}"))?;

        let file_hunks = collect_file_hunks(&diff)?;

        if file_hunks.len() > cfg.max_files_per_commit {
            out.push(WindowEntry {
                commit_oid: oid.to_string(),
                commit_ts,
                edges: Vec::new(),
                commits_active: false,
                skipped_large: true,
            });
            continue;
        }

        let mut file_anchors: Vec<String> = file_hunks
            .iter()
            .map(|fh| AnchorKey::file(&fh.path).as_str().to_string())
            .collect();
        file_anchors.sort();
        file_anchors.dedup();

        let mut symbol_anchors: Vec<String> = Vec::new();
        if cfg.include_symbols {
            for fh in &file_hunks {
                let Some(ext) = extension_of(&fh.path) else {
                    continue;
                };
                let Some(lang) = crate::domain::LanguageId::from_extension(ext) else {
                    continue;
                };
                if !language_supports_parsing(&lang) {
                    continue;
                }
                let Some(new_oid) = fh.new_oid else {
                    continue;
                };
                let Ok(blob) = repo.find_blob(new_oid) else {
                    continue;
                };
                if blob.is_binary() {
                    continue;
                }
                let Ok(content) = std::str::from_utf8(blob.content()) else {
                    continue;
                };
                let is_tsx = crate::domain::LanguageId::is_tsx_path(&fh.path);
                for (name, kind) in resolve_symbol_names(content, &lang, is_tsx, &fh.hunks) {
                    symbol_anchors.push(
                        AnchorKey::symbol(&fh.path, &name, kind)
                            .as_str()
                            .to_string(),
                    );
                }
            }
            symbol_anchors.sort();
            symbol_anchors.dedup();
        }

        let has_file_pairs = file_anchors.len() >= 2;
        let has_symbol_pairs = symbol_anchors.len() >= 2;
        if !has_file_pairs && !has_symbol_pairs {
            continue;
        }

        let commit_oid = oid.to_string();
        let mut edges: Vec<LedgerEdgeRow> = Vec::new();
        if has_file_pairs {
            let base = size_weight(file_anchors.len());
            emit_ledger(&file_anchors, &commit_oid, commit_ts, base, &mut edges);
        }
        if has_symbol_pairs {
            let base = size_weight(symbol_anchors.len());
            emit_ledger(&symbol_anchors, &commit_oid, commit_ts, base, &mut edges);
        }
        out.push(WindowEntry {
            commit_oid,
            commit_ts,
            edges,
            commits_active: true,
            skipped_large: false,
        });
    }

    Ok((out, head_oid))
}

fn emit_ledger(
    anchors: &[String],
    commit_oid: &str,
    commit_ts: i64,
    base_weight: f64,
    out: &mut Vec<LedgerEdgeRow>,
) {
    for i in 0..anchors.len() {
        for j in 0..anchors.len() {
            if i == j {
                continue;
            }
            out.push(LedgerEdgeRow {
                commit_oid: commit_oid.to_string(),
                anchor_key: anchors[i].clone(),
                partner_key: anchors[j].clone(),
                shared_inc: 1,
                base_weight,
                commit_ts,
            });
        }
    }
}

/// Flatten window entries into the tuple of inputs `commit_cold_build`
/// expects. Aggregates each pair across all active commits, applying the
/// reference-time decay.
fn build_cold_inputs(
    entries: &[WindowEntry],
    cfg: &WalkerConfig,
) -> (Vec<CouplingRow>, Vec<LedgerEdgeRow>, Vec<(String, i64)>) {
    let half_life_secs = cfg.half_life_secs();
    let mut agg: HashMap<(String, String), (u32, f64, i64)> = HashMap::new();
    let mut ledger: Vec<LedgerEdgeRow> = Vec::new();
    let mut active: Vec<(String, i64)> = Vec::new();

    for entry in entries {
        if !entry.commits_active {
            continue;
        }
        active.push((entry.commit_oid.clone(), entry.commit_ts));
        for edge in &entry.edges {
            let decay = decay_factor(cfg.reference_ts - edge.commit_ts, half_life_secs);
            let contribution = edge.base_weight * decay;
            let slot = agg
                .entry((edge.anchor_key.clone(), edge.partner_key.clone()))
                .or_insert((0, 0.0, i64::MIN));
            slot.0 += edge.shared_inc;
            slot.1 += contribution;
            if slot.2 < edge.commit_ts {
                slot.2 = edge.commit_ts;
            }
            ledger.push(edge.clone());
        }
    }

    let rows: Vec<CouplingRow> = agg
        .into_iter()
        .map(|((anchor, partner), (shared, score, ts))| CouplingRow {
            anchor: AnchorKey::from_raw(anchor),
            partner: AnchorKey::from_raw(partner),
            shared_commits: shared,
            weighted_score: score,
            last_commit_ts: ts,
        })
        .collect();

    (rows, ledger, active)
}

struct FileHunks {
    path: String,
    new_oid: Option<git2::Oid>,
    hunks: Vec<(u32, u32, u32, u32)>,
}

fn collect_file_hunks(diff: &git2::Diff) -> Result<Vec<FileHunks>> {
    let mut out: Vec<FileHunks> = Vec::new();
    for delta_idx in 0..diff.deltas().len() {
        let Some(delta) = diff.get_delta(delta_idx) else {
            continue;
        };
        let Some(path) = delta
            .new_file()
            .path()
            .or_else(|| delta.old_file().path())
            .and_then(|p| p.to_str())
            .map(|s| s.to_string())
        else {
            continue;
        };
        if path.is_empty() {
            continue;
        }
        let new_id = delta.new_file().id();
        let new_oid = if new_id.is_zero() { None } else { Some(new_id) };

        let patch_opt =
            git2::Patch::from_diff(diff, delta_idx).map_err(|e| anyhow!("patch from_diff: {e}"))?;
        let mut hunks = Vec::new();
        if let Some(patch) = patch_opt {
            for h in 0..patch.num_hunks() {
                if let Ok((hunk, _)) = patch.hunk(h) {
                    hunks.push((
                        hunk.old_start(),
                        hunk.old_lines(),
                        hunk.new_start(),
                        hunk.new_lines(),
                    ));
                }
            }
        }
        out.push(FileHunks {
            path,
            new_oid,
            hunks,
        });
    }
    Ok(out)
}

fn extension_of(path: &str) -> Option<&str> {
    path.rsplit_once('.').map(|(_, ext)| ext)
}

fn language_supports_parsing(lang: &crate::domain::LanguageId) -> bool {
    use crate::domain::LanguageId::*;
    !matches!(lang, Json | Toml | Yaml | Markdown | Env)
}

fn resolve_symbol_names(
    source: &str,
    language: &crate::domain::LanguageId,
    is_tsx: bool,
    hunks: &[(u32, u32, u32, u32)],
) -> Vec<(String, &'static str)> {
    let (symbols, _has_error, _diag, _refs, _alias) =
        match crate::parsing::parse_source(source, language, is_tsx) {
            Ok(v) => v,
            Err(_) => return Vec::new(),
        };

    let mut seen: HashSet<(String, &'static str)> = HashSet::new();
    for (_old_start, _old_lines, new_start, new_lines) in hunks {
        if *new_lines == 0 {
            continue;
        }
        let start_0 = new_start.saturating_sub(1);
        let end_0 = new_start
            .saturating_add(*new_lines)
            .saturating_sub(1)
            .saturating_sub(1);
        for line in start_0..=end_0.max(start_0) {
            if let Some(idx) = crate::domain::find_enclosing_symbol(&symbols, line) {
                let sym = &symbols[idx as usize];
                seen.insert((sym.name.clone(), symbol_kind_str(&sym.kind)));
            }
        }
    }
    seen.into_iter().collect()
}

fn symbol_kind_str(kind: &crate::domain::SymbolKind) -> &'static str {
    use crate::domain::SymbolKind::*;
    match kind {
        Function => "fn",
        Method => "method",
        Class => "class",
        Struct => "struct",
        Enum => "enum",
        Interface => "interface",
        Module => "mod",
        Constant => "const",
        Variable => "var",
        Type => "type",
        Trait => "trait",
        Impl => "impl",
        Other => "other",
        Key => "key",
        Section => "section",
        MacroGenerated => "macro-generated",
    }
}

fn size_weight(anchor_count: usize) -> f64 {
    if anchor_count <= 1 {
        return 0.0;
    }
    1.0 / ((anchor_count + 1) as f64).log2()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    mod git_test_helpers {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/git/test_helpers.rs"
        ));
    }

    struct TestRepo {
        tmp: tempfile::TempDir,
        repo: git2::Repository,
    }

    impl TestRepo {
        fn init() -> Self {
            let tmp = tempfile::tempdir().unwrap();
            let repo = git2::Repository::init(tmp.path()).unwrap();
            let mut cfg = repo.config().unwrap();
            cfg.set_str("user.name", "Test").unwrap();
            cfg.set_str("user.email", "t@example.com").unwrap();
            Self { tmp, repo }
        }

        fn path(&self) -> PathBuf {
            self.tmp.path().to_path_buf()
        }

        fn commit(&self, files: &[(&str, &str)], ts: i64, msg: &str) -> git2::Oid {
            for (rel, content) in files {
                let full = self.tmp.path().join(rel);
                if let Some(parent) = full.parent() {
                    fs::create_dir_all(parent).unwrap();
                }
                fs::write(&full, content).unwrap();
            }
            let mut index = self.repo.index().unwrap();
            for (rel, _) in files {
                index.add_path(Path::new(rel)).unwrap();
            }
            index.write().unwrap();
            let tree_oid = index.write_tree().unwrap();
            let tree = self.repo.find_tree(tree_oid).unwrap();
            let sig =
                git2::Signature::new("Test", "t@example.com", &git2::Time::new(ts, 0)).unwrap();
            let parent_oid = self.repo.head().ok().and_then(|h| h.target());
            let parents: Vec<git2::Commit> = parent_oid
                .and_then(|oid| self.repo.find_commit(oid).ok())
                .into_iter()
                .collect();
            let parent_refs: Vec<&git2::Commit> = parents.iter().collect();
            git_test_helpers::commit_head_with_retry(
                &self.repo,
                &sig,
                &sig,
                msg,
                &tree,
                &parent_refs,
            )
        }

        /// Commit that removes `removals` from the git index (and worktree),
        /// optionally also writing `add` files. Used to exercise dead-path
        /// eviction: after this commit the removed paths are no longer tracked.
        fn commit_removing(
            &self,
            add: &[(&str, &str)],
            removals: &[&str],
            ts: i64,
            msg: &str,
        ) -> git2::Oid {
            for (rel, content) in add {
                let full = self.tmp.path().join(rel);
                if let Some(parent) = full.parent() {
                    fs::create_dir_all(parent).unwrap();
                }
                fs::write(&full, content).unwrap();
            }
            let mut index = self.repo.index().unwrap();
            for (rel, _) in add {
                index.add_path(Path::new(rel)).unwrap();
            }
            for rel in removals {
                index.remove_path(Path::new(rel)).unwrap();
                let _ = fs::remove_file(self.tmp.path().join(rel));
            }
            index.write().unwrap();
            let tree_oid = index.write_tree().unwrap();
            let tree = self.repo.find_tree(tree_oid).unwrap();
            let sig =
                git2::Signature::new("Test", "t@example.com", &git2::Time::new(ts, 0)).unwrap();
            let parent_oid = self.repo.head().ok().and_then(|h| h.target());
            let parents: Vec<git2::Commit> = parent_oid
                .and_then(|oid| self.repo.find_commit(oid).ok())
                .into_iter()
                .collect();
            let parent_refs: Vec<&git2::Commit> = parents.iter().collect();
            git_test_helpers::commit_head_with_retry(
                &self.repo,
                &sig,
                &sig,
                msg,
                &tree,
                &parent_refs,
            )
        }
    }

    fn now_sec() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
    }

    fn test_cfg(now_ts: i64) -> WalkerConfig {
        WalkerConfig::with_now(now_ts)
    }

    // ---------- cold_build basics ----------

    #[test]
    fn cold_build_on_empty_repo_returns_zero_stats() {
        let tmp = tempfile::tempdir().unwrap();
        git2::Repository::init(tmp.path()).unwrap();
        let store = CouplingStore::open_in_memory().unwrap();
        let stats = cold_build(&store, tmp.path(), &test_cfg(now_sec())).unwrap();
        assert_eq!(stats, WalkerStats::default());
    }

    #[test]
    fn cold_build_single_file_commit_emits_no_pairs() {
        let repo = TestRepo::init();
        repo.commit(&[("a.txt", "hello")], now_sec() - 3600, "seed");
        let store = CouplingStore::open_in_memory().unwrap();
        cold_build(&store, &repo.path(), &test_cfg(now_sec())).unwrap();
        assert!(
            store
                .query(&AnchorKey::file("a.txt"), 10)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn cold_build_emits_both_directions_for_two_file_commit() {
        let repo = TestRepo::init();
        repo.commit(
            &[("src/a.rs", "fn a() {}"), ("src/b.rs", "fn b() {}")],
            now_sec() - 3600,
            "seed",
        );
        let store = CouplingStore::open_in_memory().unwrap();
        cold_build(&store, &repo.path(), &test_cfg(now_sec())).unwrap();

        let from_a = store.query(&AnchorKey::file("src/a.rs"), 10).unwrap();
        let from_b = store.query(&AnchorKey::file("src/b.rs"), 10).unwrap();
        assert_eq!(from_a.len(), 1);
        assert_eq!(from_b.len(), 1);
    }

    #[test]
    fn cold_build_aggregates_shared_commits() {
        let repo = TestRepo::init();
        let now = now_sec();
        repo.commit(&[("a.rs", "v1"), ("b.rs", "v1")], now - 86400 * 10, "c1");
        repo.commit(&[("a.rs", "v2"), ("b.rs", "v2")], now - 86400 * 5, "c2");
        repo.commit(&[("a.rs", "v3"), ("b.rs", "v3")], now - 86400, "c3");

        let store = CouplingStore::open_in_memory().unwrap();
        cold_build(&store, &repo.path(), &test_cfg(now)).unwrap();
        let row = store.query(&AnchorKey::file("a.rs"), 10).unwrap();
        assert_eq!(row[0].shared_commits, 3);
        assert_eq!(row[0].last_commit_ts, now - 86400);
    }

    #[test]
    fn cold_build_skips_large_commits() {
        let repo = TestRepo::init();
        let now = now_sec();
        repo.commit(
            &[
                ("f1.rs", "x"),
                ("f2.rs", "x"),
                ("f3.rs", "x"),
                ("f4.rs", "x"),
            ],
            now - 3600,
            "big",
        );
        let store = CouplingStore::open_in_memory().unwrap();
        let mut cfg = test_cfg(now);
        cfg.max_files_per_commit = 3;
        let stats = cold_build(&store, &repo.path(), &cfg).unwrap();
        assert_eq!(stats.commits_skipped_large, 1);
        assert_eq!(stats.unique_edges, 0);
    }

    #[test]
    fn cold_build_records_head_reference_ts_and_cold_built_at() {
        let repo = TestRepo::init();
        let oid = repo.commit(&[("a.rs", "x"), ("b.rs", "y")], now_sec() - 3600, "seed");
        let store = CouplingStore::open_in_memory().unwrap();
        let cfg = test_cfg(now_sec());
        cold_build(&store, &repo.path(), &cfg).unwrap();
        assert_eq!(store.last_head().unwrap().unwrap(), oid.to_string());
        assert_eq!(store.last_reference_ts().unwrap(), Some(cfg.reference_ts));
        assert_eq!(store.cold_built_at().unwrap(), Some(cfg.reference_ts));
    }

    #[test]
    fn cold_build_weighted_score_matches_formula() {
        let repo = TestRepo::init();
        let now = 2_000_000_000i64;
        let commit_ts = now - 86400 * 5;
        repo.commit(&[("a.rs", "x"), ("b.rs", "y")], commit_ts, "seed");
        let store = CouplingStore::open_in_memory().unwrap();
        let mut cfg = test_cfg(now);
        cfg.half_life_days = 30;
        cfg.window_days = 365_000;
        cold_build(&store, &repo.path(), &cfg).unwrap();
        let got = store
            .query(&AnchorKey::file("a.rs"), 1)
            .unwrap()
            .first()
            .unwrap()
            .weighted_score;
        let expected_size = 1.0 / 3.0f64.log2();
        let expected_time = (-(86400.0 * 5.0) * std::f64::consts::LN_2 / (86400.0 * 30.0)).exp();
        assert!((got - expected_size * expected_time).abs() < 1e-9);
    }

    #[test]
    fn cold_build_is_idempotent() {
        let repo = TestRepo::init();
        let now = now_sec();
        repo.commit(&[("a.rs", "x"), ("b.rs", "y")], now - 3600, "c1");
        let store = CouplingStore::open_in_memory().unwrap();
        cold_build(&store, &repo.path(), &test_cfg(now)).unwrap();
        let first = store.query(&AnchorKey::file("a.rs"), 10).unwrap();
        cold_build(&store, &repo.path(), &test_cfg(now)).unwrap();
        let second = store.query(&AnchorKey::file("a.rs"), 10).unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn cold_build_purges_on_no_head() {
        let store = CouplingStore::open_in_memory().unwrap();
        let now = now_sec();
        let headed = TestRepo::init();
        headed.commit(&[("a.rs", "x"), ("b.rs", "y")], now - 3600, "seed");
        cold_build(&store, &headed.path(), &test_cfg(now)).unwrap();
        assert_eq!(store.query(&AnchorKey::file("a.rs"), 10).unwrap().len(), 1);
        let empty = tempfile::tempdir().unwrap();
        git2::Repository::init(empty.path()).unwrap();
        cold_build(&store, empty.path(), &test_cfg(now)).unwrap();
        assert!(
            store
                .query(&AnchorKey::file("a.rs"), 10)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn no_head_cold_build_clears_last_head_meta() {
        // Regression: previously, commit_cold_build / commit_delta only wrote
        // META_LAST_HEAD when new_head was Some, leaving stale values.
        // A subsequent NoOp fast-path could then falsely match on a
        // head that had actually been invalidated.
        let store = CouplingStore::open_in_memory().unwrap();
        let now = now_sec();

        let headed = TestRepo::init();
        headed.commit(&[("a.rs", "x"), ("b.rs", "y")], now - 3600, "seed");
        cold_build(&store, &headed.path(), &test_cfg(now)).unwrap();
        assert!(store.last_head().unwrap().is_some());

        let empty = tempfile::tempdir().unwrap();
        git2::Repository::init(empty.path()).unwrap();
        cold_build(&store, empty.path(), &test_cfg(now)).unwrap();
        assert_eq!(
            store.last_head().unwrap(),
            None,
            "no-HEAD cold build must clear META_LAST_HEAD"
        );
    }

    #[test]
    fn delta_does_not_noop_after_head_lost_and_restored_to_same_oid() {
        // Critical regression. Sequence:
        //   1. cold-build on repo-A -> store has last_head=H_A, tables populated.
        //   2. apply_head_delta on empty repo -> purge, last_head cleared.
        //   3. apply_head_delta on repo-A again (same HEAD oid, same reference_ts).
        //      MUST rebuild from the (now empty) ledger instead of NoOp-ing.
        let store = CouplingStore::open_in_memory().unwrap();
        let now = now_sec();

        let repo_a = TestRepo::init();
        repo_a.commit(&[("a.rs", "x"), ("b.rs", "y")], now - 3600, "seed");
        cold_build(&store, &repo_a.path(), &test_cfg(now)).unwrap();
        assert_eq!(store.query(&AnchorKey::file("a.rs"), 10).unwrap().len(), 1);

        // Step 2: simulate HEAD loss.
        let empty = tempfile::tempdir().unwrap();
        git2::Repository::init(empty.path()).unwrap();
        apply_head_delta(&store, empty.path(), &test_cfg(now)).unwrap();
        assert_eq!(store.last_head().unwrap(), None);
        assert!(
            store
                .query(&AnchorKey::file("a.rs"), 10)
                .unwrap()
                .is_empty(),
            "purge must have happened"
        );

        // Step 3: HEAD restored to the same repo / same oid. If NoOp were
        // incorrectly taken, the store would remain empty.
        match apply_head_delta(&store, &repo_a.path(), &test_cfg(now)).unwrap() {
            DeltaOutcome::NoOp { .. } => {
                panic!("must not NoOp after HEAD was lost and restored")
            }
            DeltaOutcome::Applied {
                incoming_commits,
                outgoing_commits,
                ..
            } => {
                assert_eq!(incoming_commits, 1);
                assert_eq!(outgoing_commits, 0);
            }
        }

        let restored = store.query(&AnchorKey::file("a.rs"), 10).unwrap();
        assert_eq!(
            restored.len(),
            1,
            "delta must have rebuilt the pair from ledger"
        );
    }

    #[test]
    fn delta_with_no_head_repo_clears_last_head_meta() {
        // Delta path: starting from a populated store, apply_head_delta on a
        // no-HEAD repo must clear META_LAST_HEAD too.
        let store = CouplingStore::open_in_memory().unwrap();
        let now = now_sec();

        let headed = TestRepo::init();
        headed.commit(&[("a.rs", "x"), ("b.rs", "y")], now - 3600, "seed");
        cold_build(&store, &headed.path(), &test_cfg(now)).unwrap();
        assert!(store.last_head().unwrap().is_some());

        let empty = tempfile::tempdir().unwrap();
        git2::Repository::init(empty.path()).unwrap();
        apply_head_delta(&store, empty.path(), &test_cfg(now)).unwrap();
        assert_eq!(store.last_head().unwrap(), None);
        assert!(
            store
                .query(&AnchorKey::file("a.rs"), 10)
                .unwrap()
                .is_empty()
        );
    }

    // ---------- symbol-level ----------

    #[test]
    fn cold_build_with_symbols_emits_symbol_pairs() {
        let repo = TestRepo::init();
        let now = now_sec();
        repo.commit(
            &[
                ("foo.rs", "fn alpha() { let x = 1; }\n"),
                ("bar.rs", "fn beta() { let y = 1; }\n"),
            ],
            now - 3600,
            "seed",
        );
        let store = CouplingStore::open_in_memory().unwrap();
        let mut cfg = test_cfg(now);
        cfg.include_symbols = true;
        cold_build(&store, &repo.path(), &cfg).unwrap();

        let sym = store
            .query(&AnchorKey::symbol("foo.rs", "alpha", "fn"), 10)
            .unwrap();
        assert_eq!(sym.len(), 1);
        assert_eq!(sym[0].partner, AnchorKey::symbol("bar.rs", "beta", "fn"));
    }

    #[test]
    fn cold_build_with_symbols_emits_intra_file_pairs_for_single_file_commit() {
        let repo = TestRepo::init();
        let now = now_sec();
        repo.commit(
            &[(
                "solo.rs",
                "fn alpha() { let x = 1; }\nfn beta() { let y = 1; }\n",
            )],
            now - 3600,
            "seed",
        );
        let store = CouplingStore::open_in_memory().unwrap();
        let mut cfg = test_cfg(now);
        cfg.include_symbols = true;
        cold_build(&store, &repo.path(), &cfg).unwrap();

        let alpha = store
            .query(&AnchorKey::symbol("solo.rs", "alpha", "fn"), 10)
            .unwrap();
        assert_eq!(alpha.len(), 1);
        assert_eq!(alpha[0].partner, AnchorKey::symbol("solo.rs", "beta", "fn"));
    }

    #[test]
    fn cold_build_with_symbols_skips_config_languages_but_keeps_files() {
        let repo = TestRepo::init();
        let now = now_sec();
        repo.commit(
            &[("config.json", "{\"a\":1}"), ("manifest.toml", "[x]\na=1")],
            now - 3600,
            "seed",
        );
        let store = CouplingStore::open_in_memory().unwrap();
        let mut cfg = test_cfg(now);
        cfg.include_symbols = true;
        cold_build(&store, &repo.path(), &cfg).unwrap();
        assert_eq!(
            store
                .query(&AnchorKey::file("config.json"), 10)
                .unwrap()
                .len(),
            1
        );
    }

    // ---------- dead-path eviction at cold build ----------

    /// Build a coupling store via the full `cold_build` path against a repo
    /// where one file was deleted, and assert that every pair / ledger edge /
    /// orphaned commit referencing the deleted path is gone, while the live
    /// pair is byte-identical to an unpruned control filtered to live pairs.
    #[test]
    fn cold_build_evicts_pairs_for_deleted_paths() {
        let repo = TestRepo::init();
        let now_ts = 2_000_000_000i64;
        let mut cfg = test_cfg(now_ts);
        cfg.window_days = 365_000;

        // c1: a, b, dead all change together -> 3 files -> all three pairs.
        repo.commit(
            &[("a.rs", "1"), ("b.rs", "1"), ("dead.rs", "1")],
            now_ts - 86400 * 10,
            "c1",
        );
        // c2: a, b change again AND dead.rs is removed from the index.
        repo.commit_removing(
            &[("a.rs", "2"), ("b.rs", "2")],
            &["dead.rs"],
            now_ts - 86400,
            "c2",
        );

        // Pruned store via the production cold_build path (reads git ls-files).
        let pruned = CouplingStore::open_in_memory().unwrap();
        cold_build(&pruned, &repo.path(), &cfg).unwrap();

        // Unpruned control: same window inputs, committed with live_paths=None.
        let control = CouplingStore::open_in_memory().unwrap();
        let (entries, head_oid) = compute_window(&repo.path(), &cfg).unwrap();
        let (rows, ledger, active) = build_cold_inputs(&entries, &cfg);
        control
            .commit_cold_build(
                &rows,
                &active,
                &ledger,
                head_oid.as_deref(),
                cfg.reference_ts,
                None,
            )
            .unwrap();

        // dead.rs must have no surviving pairs in the pruned store.
        assert!(
            pruned
                .query(&AnchorKey::file("dead.rs"), 100)
                .unwrap()
                .is_empty(),
            "deleted path must have no surviving partners"
        );
        // ...but the control (unpruned) must still carry them.
        assert!(
            !control
                .query(&AnchorKey::file("dead.rs"), 100)
                .unwrap()
                .is_empty(),
            "control must retain the dead pairs (proving the prune did work)"
        );

        // a.rs must no longer list dead.rs as a partner in the pruned store.
        let a_partners = pruned.query(&AnchorKey::file("a.rs"), 100).unwrap();
        assert!(
            a_partners
                .iter()
                .all(|r| r.partner.as_str() != AnchorKey::file("dead.rs").as_str()),
            "a.rs must not retain dead.rs as a partner"
        );

        // The live a.rs<->b.rs pair must be byte-identical to the control's.
        let a_to_b_pruned = pruned
            .pair_row(&AnchorKey::file("a.rs"), &AnchorKey::file("b.rs"))
            .unwrap()
            .expect("live a<->b pair must survive pruning");
        let a_to_b_control = control
            .pair_row(&AnchorKey::file("a.rs"), &AnchorKey::file("b.rs"))
            .unwrap()
            .expect("control must have a<->b pair");
        assert_eq!(a_to_b_pruned.shared_commits, a_to_b_control.shared_commits);
        assert_eq!(a_to_b_pruned.last_commit_ts, a_to_b_control.last_commit_ts);
        assert!(
            (a_to_b_pruned.weighted_score - a_to_b_control.weighted_score).abs() < 1e-12,
            "surviving live pair weighted_score must be byte-identical to control"
        );

        // Every active commit that survives must still have a ledger edge:
        // c1 only touched a/b/dead pairs; after eviction its a<->b edge keeps
        // it alive. (No orphan-commit assertion possible here since c1 retains
        // a live edge; the orphan path is covered by the store-level test.)
        let active_oids = pruned.active_commit_oids().unwrap();
        assert!(!active_oids.is_empty());
    }

    // ---------- delta equivalence ----------

    #[test]
    fn delta_after_new_commit_matches_scratch_file_level() {
        let repo = TestRepo::init();
        let now_ts = 2_000_000_000i64;
        let mut cfg = test_cfg(now_ts);
        cfg.window_days = 365_000;

        repo.commit(&[("a.rs", "v1"), ("b.rs", "v1")], now_ts - 86400 * 30, "c1");
        let delta_store = CouplingStore::open_in_memory().unwrap();
        cold_build(&delta_store, &repo.path(), &cfg).unwrap();

        repo.commit(&[("a.rs", "v2"), ("b.rs", "v2")], now_ts - 86400, "c2");
        match apply_head_delta(&delta_store, &repo.path(), &cfg).unwrap() {
            DeltaOutcome::Applied {
                incoming_commits,
                outgoing_commits,
                ..
            } => {
                assert_eq!(incoming_commits, 1);
                assert_eq!(outgoing_commits, 0);
            }
            other => panic!("expected Applied, got {other:?}"),
        }

        let scratch = CouplingStore::open_in_memory().unwrap();
        cold_build(&scratch, &repo.path(), &cfg).unwrap();
        let d = delta_store.query(&AnchorKey::file("a.rs"), 10).unwrap();
        let s = scratch.query(&AnchorKey::file("a.rs"), 10).unwrap();
        assert_eq!(d[0].shared_commits, s[0].shared_commits);
        assert_eq!(d[0].last_commit_ts, s[0].last_commit_ts);
        assert!((d[0].weighted_score - s[0].weighted_score).abs() < 1e-9);
    }

    #[test]
    fn delta_evicts_commits_falling_out_of_bounded_window_matches_scratch() {
        // The critical regression: with max_commits=2, three commits cause
        // the oldest to drop out of the bounded window. Delta must subtract
        // the evicted commit's contributions so the aggregate matches a
        // fresh cold-build.
        let repo = TestRepo::init();
        let now_ts = 2_000_000_000i64;
        let mut cfg = test_cfg(now_ts);
        cfg.window_days = 365_000;
        cfg.max_commits = 2;

        repo.commit(
            &[("a.rs", "1"), ("b.rs", "1")],
            now_ts - 86400 * 10,
            "oldest",
        );
        repo.commit(&[("a.rs", "2"), ("b.rs", "2")], now_ts - 86400 * 5, "c2");
        let delta_store = CouplingStore::open_in_memory().unwrap();
        cold_build(&delta_store, &repo.path(), &cfg).unwrap();
        assert_eq!(
            delta_store
                .query(&AnchorKey::file("a.rs"), 10)
                .unwrap()
                .first()
                .unwrap()
                .shared_commits,
            2
        );

        repo.commit(&[("a.rs", "3"), ("b.rs", "3")], now_ts - 86400, "c3");
        apply_head_delta(&delta_store, &repo.path(), &cfg).unwrap();

        let scratch = CouplingStore::open_in_memory().unwrap();
        cold_build(&scratch, &repo.path(), &cfg).unwrap();

        let d = delta_store.query(&AnchorKey::file("a.rs"), 10).unwrap();
        let s = scratch.query(&AnchorKey::file("a.rs"), 10).unwrap();
        assert_eq!(
            d[0].shared_commits, s[0].shared_commits,
            "evicted commit must be subtracted"
        );
        assert_eq!(d[0].shared_commits, 2);
        assert_eq!(d[0].last_commit_ts, s[0].last_commit_ts);
        assert!((d[0].weighted_score - s[0].weighted_score).abs() < 1e-9);
    }

    #[test]
    fn delta_deletes_pair_when_all_contributing_commits_evicted() {
        let repo = TestRepo::init();
        let now_ts = 2_000_000_000i64;
        let mut cfg = test_cfg(now_ts);
        cfg.window_days = 365_000;
        cfg.max_commits = 1;

        repo.commit(&[("a.rs", "1"), ("b.rs", "1")], now_ts - 86400 * 10, "c1");
        let store = CouplingStore::open_in_memory().unwrap();
        cold_build(&store, &repo.path(), &cfg).unwrap();
        assert_eq!(store.query(&AnchorKey::file("a.rs"), 10).unwrap().len(), 1);

        repo.commit(&[("c.rs", "1"), ("d.rs", "1")], now_ts - 86400, "c2");
        apply_head_delta(&store, &repo.path(), &cfg).unwrap();
        assert!(
            store
                .query(&AnchorKey::file("a.rs"), 10)
                .unwrap()
                .is_empty(),
            "pair must be deleted when its only commit is evicted"
        );
        assert_eq!(store.query(&AnchorKey::file("c.rs"), 10).unwrap().len(), 1);
    }

    #[test]
    fn delta_matches_scratch_with_symbols_under_eviction() {
        let repo = TestRepo::init();
        let now_ts = 2_000_000_000i64;
        let mut cfg = test_cfg(now_ts);
        cfg.window_days = 365_000;
        cfg.max_commits = 2;
        cfg.include_symbols = true;

        repo.commit(
            &[("foo.rs", "fn alpha() {}\n"), ("bar.rs", "fn beta() {}\n")],
            now_ts - 86400 * 10,
            "c1",
        );
        repo.commit(
            &[
                ("foo.rs", "fn alpha() { 1 }\n"),
                ("bar.rs", "fn beta() { 1 }\n"),
            ],
            now_ts - 86400 * 5,
            "c2",
        );
        let delta_store = CouplingStore::open_in_memory().unwrap();
        cold_build(&delta_store, &repo.path(), &cfg).unwrap();

        repo.commit(
            &[
                ("foo.rs", "fn alpha() { 2 }\n"),
                ("bar.rs", "fn beta() { 2 }\n"),
            ],
            now_ts - 86400,
            "c3",
        );
        apply_head_delta(&delta_store, &repo.path(), &cfg).unwrap();

        let scratch = CouplingStore::open_in_memory().unwrap();
        cold_build(&scratch, &repo.path(), &cfg).unwrap();

        let d = delta_store
            .query(&AnchorKey::symbol("foo.rs", "alpha", "fn"), 10)
            .unwrap();
        let s = scratch
            .query(&AnchorKey::symbol("foo.rs", "alpha", "fn"), 10)
            .unwrap();
        assert_eq!(d[0].shared_commits, s[0].shared_commits);
        assert_eq!(d[0].last_commit_ts, s[0].last_commit_ts);
        assert!((d[0].weighted_score - s[0].weighted_score).abs() < 1e-9);
    }

    #[test]
    fn delta_rescales_weighted_score_when_reference_ts_advances() {
        let repo = TestRepo::init();
        let ts_cold = 2_000_000_000i64;
        let mut cfg = test_cfg(ts_cold);
        cfg.window_days = 365_000;
        cfg.half_life_days = 30;

        repo.commit(&[("a.rs", "x"), ("b.rs", "y")], ts_cold - 86400, "seed");
        let store = CouplingStore::open_in_memory().unwrap();
        cold_build(&store, &repo.path(), &cfg).unwrap();
        let before = store
            .query(&AnchorKey::file("a.rs"), 1)
            .unwrap()
            .first()
            .unwrap()
            .weighted_score;

        let mut cfg2 = cfg.clone();
        cfg2.reference_ts = ts_cold + 86400 * 30;
        apply_head_delta(&store, &repo.path(), &cfg2).unwrap();
        let after = store
            .query(&AnchorKey::file("a.rs"), 1)
            .unwrap()
            .first()
            .unwrap()
            .weighted_score;
        assert!(
            (after - before * 0.5).abs() < 1e-9,
            "one half-life advance should halve the score; before={before} after={after}"
        );
    }

    #[test]
    fn delta_noop_when_head_and_reference_match() {
        let repo = TestRepo::init();
        let now = now_sec();
        repo.commit(&[("a.rs", "x"), ("b.rs", "y")], now - 3600, "seed");
        let store = CouplingStore::open_in_memory().unwrap();
        let cfg = test_cfg(now);
        cold_build(&store, &repo.path(), &cfg).unwrap();
        match apply_head_delta(&store, &repo.path(), &cfg).unwrap() {
            DeltaOutcome::NoOp { .. } => {}
            other => panic!("expected NoOp, got {other:?}"),
        }
    }

    #[test]
    fn delta_from_empty_store_matches_scratch_cold_build() {
        let repo = TestRepo::init();
        let now_ts = 2_000_000_000i64;
        let mut cfg = test_cfg(now_ts);
        cfg.window_days = 365_000;

        repo.commit(&[("a.rs", "x"), ("b.rs", "y")], now_ts - 3600, "c1");
        repo.commit(&[("a.rs", "y"), ("b.rs", "z")], now_ts - 1800, "c2");

        let delta_store = CouplingStore::open_in_memory().unwrap();
        apply_head_delta(&delta_store, &repo.path(), &cfg).unwrap();
        let scratch = CouplingStore::open_in_memory().unwrap();
        cold_build(&scratch, &repo.path(), &cfg).unwrap();

        let d = delta_store.query(&AnchorKey::file("a.rs"), 10).unwrap();
        let s = scratch.query(&AnchorKey::file("a.rs"), 10).unwrap();
        assert_eq!(d[0].shared_commits, s[0].shared_commits);
        assert!((d[0].weighted_score - s[0].weighted_score).abs() < 1e-9);
    }

    #[test]
    fn delta_on_non_ancestral_head_matches_scratch() {
        let store = CouplingStore::open_in_memory().unwrap();
        let now_ts = 2_000_000_000i64;
        let mut cfg = test_cfg(now_ts);
        cfg.window_days = 365_000;

        let old_repo = TestRepo::init();
        old_repo.commit(&[("old1.rs", "x"), ("old2.rs", "y")], now_ts - 3600, "old");
        cold_build(&store, &old_repo.path(), &cfg).unwrap();

        let new_repo = TestRepo::init();
        new_repo.commit(&[("new1.rs", "x"), ("new2.rs", "y")], now_ts - 1800, "new");
        apply_head_delta(&store, &new_repo.path(), &cfg).unwrap();

        let scratch = CouplingStore::open_in_memory().unwrap();
        cold_build(&scratch, &new_repo.path(), &cfg).unwrap();

        assert!(
            store
                .query(&AnchorKey::file("old1.rs"), 10)
                .unwrap()
                .is_empty(),
            "old repo's pairs must be fully evicted"
        );
        let d = store.query(&AnchorKey::file("new1.rs"), 10).unwrap();
        let s = scratch.query(&AnchorKey::file("new1.rs"), 10).unwrap();
        assert_eq!(d[0].shared_commits, s[0].shared_commits);
        assert!((d[0].weighted_score - s[0].weighted_score).abs() < 1e-9);
    }

    #[test]
    fn delta_updates_last_head_and_reference_ts() {
        let repo = TestRepo::init();
        let now = now_sec();
        repo.commit(&[("a.rs", "x"), ("b.rs", "y")], now - 3600, "c1");
        let store = CouplingStore::open_in_memory().unwrap();
        cold_build(&store, &repo.path(), &test_cfg(now)).unwrap();
        let old_head = store.last_head().unwrap().unwrap();

        let new_oid = repo.commit(&[("a.rs", "x2"), ("b.rs", "y2")], now - 1800, "c2");
        let mut cfg = test_cfg(now);
        cfg.reference_ts = now + 60;
        apply_head_delta(&store, &repo.path(), &cfg).unwrap();

        assert_ne!(store.last_head().unwrap().unwrap(), old_head);
        assert_eq!(store.last_head().unwrap().unwrap(), new_oid.to_string());
        assert_eq!(store.last_reference_ts().unwrap(), Some(now + 60));
    }

    #[test]
    fn delta_repeated_application_matches_scratch() {
        // Apply five deltas, each adding a new commit at the horizon.
        // After each, compare to scratch cold-build.
        let repo = TestRepo::init();
        let now_ts = 2_000_000_000i64;
        let mut cfg = test_cfg(now_ts);
        cfg.window_days = 365_000;
        cfg.max_commits = 3;

        repo.commit(&[("a.rs", "0"), ("b.rs", "0")], now_ts - 86400 * 100, "c0");
        let delta_store = CouplingStore::open_in_memory().unwrap();
        cold_build(&delta_store, &repo.path(), &cfg).unwrap();

        for i in 1..=5 {
            repo.commit(
                &[("a.rs", &format!("v{i}")), ("b.rs", &format!("v{i}"))],
                now_ts - 86400 * (100 - i),
                &format!("c{i}"),
            );
            apply_head_delta(&delta_store, &repo.path(), &cfg).unwrap();

            let scratch = CouplingStore::open_in_memory().unwrap();
            cold_build(&scratch, &repo.path(), &cfg).unwrap();

            let d = delta_store.query(&AnchorKey::file("a.rs"), 10).unwrap();
            let s = scratch.query(&AnchorKey::file("a.rs"), 10).unwrap();
            assert_eq!(d[0].shared_commits, s[0].shared_commits, "iter {i} shared");
            assert_eq!(d[0].last_commit_ts, s[0].last_commit_ts, "iter {i} ts");
            assert!(
                (d[0].weighted_score - s[0].weighted_score).abs() < 1e-9,
                "iter {i} score: delta={} scratch={}",
                d[0].weighted_score,
                s[0].weighted_score
            );
        }
    }

    // ---------- helpers ----------

    #[test]
    fn extension_of_returns_tail() {
        assert_eq!(extension_of("foo.rs"), Some("rs"));
        assert_eq!(extension_of("a/b/c.py"), Some("py"));
        assert_eq!(extension_of("noext"), None);
    }

    #[test]
    fn size_weight_is_monotone_decreasing() {
        assert!(size_weight(2) > size_weight(5));
        assert!(size_weight(5) > size_weight(20));
        assert_eq!(size_weight(1), 0.0);
        assert_eq!(size_weight(0), 0.0);
    }

    #[test]
    fn language_supports_parsing_rejects_config_types() {
        use crate::domain::LanguageId;
        assert!(!language_supports_parsing(&LanguageId::Json));
        assert!(language_supports_parsing(&LanguageId::Rust));
    }
}
