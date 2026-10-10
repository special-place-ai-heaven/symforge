//! The `detect_impact` blast-radius computation, shared by the MCP handler
//! and the embedded query lane. Callers own repository access, the read gate
//! sink and rendering; the change seed, the graph walk and the risk ordering
//! live here once.

use std::collections::HashMap;

use crate::live_index::LiveIndex;
use crate::live_index::graph::{RiskTier, SymbolId};

/// Maximum depth accepted before `detect_impact` clamps + warns
/// (contracts/detect-impact.md § Risk tiers / error catalog: "depth capped").
pub(crate) const DETECT_IMPACT_MAX_DEPTH: u8 = 5;

/// ponytail: fixed safety cap on EACH list (`changed_files`, `changed_symbols`,
/// `blast_radius`) returned per call — the bound that stops the 54 MB / 291K-symbol
/// dump seen on a large repo. The frozen contract's `pagination` envelope has no
/// `offset`/`limit` input field to page past it — add one if a real need for
/// deeper paging shows up. `risk_summary` is always counted over the full set.
pub(crate) const DETECT_IMPACT_MAX_RETURNED: usize = 200;

/// The interpreted `detect_impact` options.
pub(crate) struct ImpactRequest<'a> {
    pub base_branch: Option<&'a str>,
    pub since: Option<&'a str>,
    pub depth: u8,
    /// `scope=files` aggregates blast nodes per file; otherwise per symbol.
    pub files_scope: bool,
    pub include_untracked: bool,
    pub include_data: bool,
}

/// The complete, uncapped blast radius. `blast_entries` is ordered most severe
/// first, then nearest hop, then name, so any prefix keeps the worst nodes.
pub(crate) struct ImpactReport {
    pub changed_files: Vec<String>,
    pub changed_symbols: Vec<SymbolId>,
    pub blast_entries: Vec<(String, u32, RiskTier)>,
    pub include_data: bool,
    pub source_filtered_out: usize,
    pub requested_depth: u8,
    pub effective_depth: u8,
    pub base_disclosure: Option<String>,
    pub staleness_note: Option<String>,
}

impl ImpactReport {
    /// Risk counts over the FULL blast set, never the capped list.
    pub(crate) fn risk_counts(&self) -> HashMap<&'static str, u32> {
        let mut risk_counts: HashMap<&'static str, u32> = HashMap::new();
        for (_, _, risk) in &self.blast_entries {
            *risk_counts.entry(risk.as_str()).or_insert(0) += 1;
        }
        risk_counts
    }
}

/// An admitted git-object read: `(ref, path)` to gated text.
pub(crate) type GitTextRead<'a> = dyn FnMut(&str, &str) -> Result<Option<String>, String> + 'a;
/// An admitted working-tree read: `path` to gated text.
pub(crate) type WorktreeTextRead<'a> = dyn FnMut(&str) -> Result<Option<String>, String> + 'a;

/// Compute the git blast radius. `read_git(ref, path)` and `read_worktree(path)`
/// must be admitted reads (the shared read gate); a refusal is treated as "no
/// content", the conservative seed. `Err` carries the caller-ready message.
pub(crate) fn compute_detect_impact(
    live: &LiveIndex,
    repo: &crate::git::GitRepo,
    request: &ImpactRequest<'_>,
    read_git: &mut GitTextRead<'_>,
    read_worktree: &mut WorktreeTextRead<'_>,
) -> Result<ImpactReport, String> {
    let requested_depth = request.depth;
    let effective_depth = requested_depth.min(DETECT_IMPACT_MAX_DEPTH);

    // contracts/detect-impact.md § Input: `base_branch` defaults to `main`
    // when the caller supplies neither `base_branch` nor `since`. Without
    // this, the STEL-upgraded path (`route_impact` plans only
    // `{"scope":"files"}`) would silently fall through to uncommitted-only
    // and return an empty blast radius on a clean tree with committed-but-
    // unmerged work — the silent no-op the depth-default already guards
    // against. A repo with no `main` branch degrades to the "Invalid git
    // ref" error below (git2 ref resolution fails), never a panic; pass an
    // explicit `base_branch`/`since` for non-`main` default branches.
    //
    // Wave-1 defect fix (2026-07-02, contracts/detect-impact.md § 2026-07-02
    // addendum): the DEFAULT substitution now prefers `origin/main` over
    // local `main`. A local `main` that lags the remote (e.g. 83 commits
    // behind) produces a confidently-wrong blast radius against a stale base;
    // the shared remote ref is the intended comparison. An EXPLICIT
    // caller-passed `base_branch:"main"` still means local `main` (unchanged).
    // The resolved ref (and any staleness) is disclosed in the response
    // header so the output is self-describing.
    let mut staleness_note: Option<String> = None;
    let base_branch: Option<String> = if request.base_branch.is_none() && request.since.is_none() {
        let origin = repo.resolve_ref_commit("origin/main");
        let local = repo.resolve_ref_commit("main");
        let resolved = match (origin, local) {
            (Some(origin_oid), Some(local_oid)) => {
                if origin_oid != local_oid {
                    // Direction-aware disclosure (Wave 1 Fix 3): behind is the
                    // motivating stale-local case; ahead means unpushed local
                    // work landed in the base; both means diverged. `ahead` =
                    // commits in local not in origin; `behind` = the reverse.
                    staleness_note = Some(match repo.ahead_behind(local_oid, origin_oid) {
                        Some((ahead, behind)) if behind > 0 && ahead == 0 => {
                            "local main is behind origin/main; using origin/main".to_string()
                        }
                        Some((ahead, behind)) if ahead > 0 && behind == 0 => {
                            "local main is ahead of origin/main (unpushed commits); using \
                             origin/main — the blast radius includes your unpushed work \
                             relative to the shared base"
                                .to_string()
                        }
                        Some((ahead, behind)) if ahead > 0 && behind > 0 => {
                            "local main and origin/main have diverged; using origin/main"
                                .to_string()
                        }
                        // No common ancestor (or the unreachable both-zero given
                        // origin != local): fall back to the direction-neutral note.
                        _ => "local main differs from origin/main; using origin/main".to_string(),
                    });
                }
                "origin/main"
            }
            (Some(_), None) => "origin/main",
            // No origin/main (or neither ref exists): fall back to local
            // `main`. When neither exists, merge_git_changed_paths surfaces
            // the existing "Invalid git ref" error below, unchanged.
            (None, _) => "main",
        };
        Some(resolved.to_string())
    } else {
        request.base_branch.map(str::to_owned)
    };
    // The base ref disclosed in the response header: the resolved default or
    // the explicit caller value; `None` when only `since` was supplied.
    let base_disclosure: Option<String> = base_branch.clone();

    let changed_files = match repo.merge_git_changed_paths(
        base_branch.as_deref(),
        request.since,
        request.include_untracked,
    ) {
        Ok(paths) => paths,
        Err(e) => return Err(format!("Error: Invalid git ref: {e}")),
    };

    // US1 (018) FR-001/FR-002: source-focus the impact seed by default so
    // non-source data files (e.g. untracked JSON) and their key-symbols
    // don't drive the blast radius. `include_data=true` restores the prior
    // inclusive changed-set. language=None never errors, so keep the seed
    // on the impossible Err rather than silently emptying the blast radius.
    let include_data = request.include_data;
    let unfiltered_changed_total = changed_files.len();
    let changed_files = if include_data {
        changed_files
    } else {
        super::changes::filter_paths_by_prefix_and_language(changed_files.clone(), None, None, true)
            .unwrap_or(changed_files)
    };
    // Recovered finding #1: disclose how many changed paths the source-focus
    // default removed, so an empty/shrunken blast radius is self-describing.
    let source_filtered_out = unfiltered_changed_total.saturating_sub(changed_files.len());

    // PART A (019): the seed is a BODY delta, not every symbol of a changed
    // file. Reparse each changed file's BASE blob and its CURRENT working
    // tree with the same extractor `diff_symbols` uses
    // (`extract_symbols_for_diff` -> name+body-hash pairs), then seed only
    // the symbols whose body was added, modified, or removed. A 1-line edit
    // in a 20-symbol file now seeds 1 symbol, not 20; a comment/whitespace
    // shift that leaves every body byte-identical seeds 0.
    //
    // Base ref: mirror `merge_git_changed_paths`'s OWN base selection so the
    // body-delta compares against the same base the changed-file set came
    // from. `since=<ref>` diffs `<ref>...HEAD`; the `WORKTREE` sentinel and
    // `base_branch` both diff against the committed tip / branch, with the
    // working tree as the current side. A mismatched base (e.g. always HEAD)
    // would find zero deltas for a committed `since` range on a clean tree.
    let since_trimmed = request.since.map(str::trim).filter(|s| !s.is_empty());
    let seed_base_ref = match since_trimmed {
        Some("WORKTREE") => "HEAD",
        Some(since_ref) => since_ref,
        None => base_branch.as_deref().unwrap_or("HEAD"),
    };
    let (changed_symbols, blast_radius) = {
        // Render from the SAME captured bundle the receipt names (D16).
        let guard = live;
        let mut changed_symbols: Vec<crate::live_index::graph::SymbolId> = Vec::new();
        for path in &changed_files {
            // Kind lookup for the CURRENT symbols comes from the live index;
            // removed symbols (absent from current) default to Function to
            // match the graph's own entry-point default.
            let kind_by_name: HashMap<&str, crate::domain::SymbolKind> = guard
                .files
                .get(path)
                .map(|f| {
                    f.symbols
                        .iter()
                        .map(|s| (s.name.as_str(), s.kind))
                        .collect()
                })
                .unwrap_or_default();

            // Gated (D8 closed): the git-object store is a disclosure lane
            // exactly like the working tree below. A refusal collapses to
            // "no base content", the same conservative seed as the worktree
            // arm — withheld beats disclosed for a demoted file's symbols.
            let base_content = read_git(seed_base_ref, path)
                .unwrap_or_default()
                .unwrap_or_default();
            // Gated: a refusal collapses to "no current content", which
            // seeds conservatively from the index rather than disclosing
            // the demoted file's current symbol names or signatures.
            let current_content = read_worktree(path).unwrap_or_default().unwrap_or_default();

            // Unsupported/config languages return None from the extractor;
            // we cannot body-diff them, so fall back to seeding every
            // indexed symbol of the file (the old conservative behavior)
            // rather than silently seeding nothing.
            let base_syms = crate::parsing::extract_symbols_for_diff(&base_content, path);
            let current_syms = crate::parsing::extract_symbols_for_diff(&current_content, path);
            let (Some(base_syms), Some(current_syms)) = (base_syms, current_syms) else {
                if let Some(file) = guard.files.get(path) {
                    for sym in &file.symbols {
                        changed_symbols.push(crate::live_index::graph::SymbolId {
                            path: path.clone(),
                            name: sym.name.clone(),
                            kind: sym.kind,
                        });
                    }
                }
                continue;
            };

            // ponytail: name-keyed body-hash maps, matching `diff_symbols`'
            // own name-keyed comparison. Same-name overloads in one file
            // collapse to their last body hash; a resolver-aware seed
            // (C-S2-001) would key on a stable symbol id instead.
            let base_by_name: HashMap<&str, &str> = base_syms
                .iter()
                .map(|(n, h)| (n.as_str(), h.as_str()))
                .collect();
            let current_by_name: HashMap<&str, &str> = current_syms
                .iter()
                .map(|(n, h)| (n.as_str(), h.as_str()))
                .collect();

            let mut seed = |name: &str| {
                changed_symbols.push(crate::live_index::graph::SymbolId {
                    path: path.clone(),
                    name: name.to_string(),
                    kind: kind_by_name
                        .get(name)
                        .copied()
                        .unwrap_or(crate::domain::SymbolKind::Function),
                });
            };

            // Seeds follow source order, each name once: iterating the maps
            // themselves listed the same change in a different order on
            // every call.
            // Added (in current, not base) or modified (body hash differs).
            let mut seen = std::collections::HashSet::new();
            for (name, _) in &current_syms {
                let name = name.as_str();
                if !seen.insert(name) {
                    continue;
                }
                match base_by_name.get(name) {
                    None => seed(name),
                    Some(base_hash) if Some(base_hash) != current_by_name.get(name) => seed(name),
                    _ => {}
                }
            }
            // Removed (in base, not current) — seeded so downstream callers
            // of a deleted symbol still appear in the blast radius.
            let mut seen = std::collections::HashSet::new();
            for (name, _) in &base_syms {
                let name = name.as_str();
                if seen.insert(name) && !current_by_name.contains_key(name) {
                    seed(name);
                }
            }
        }
        let graph = crate::live_index::graph::GraphProjection::from_index(guard);
        let blast_radius = crate::live_index::graph::compute_impact(
            &graph,
            &changed_symbols,
            effective_depth as u32,
        );
        (changed_symbols, blast_radius)
    };

    // scope=files aggregates per-symbol blast nodes to file granularity
    // (nearest hop / matching risk wins per file); scope=symbols (default)
    // keeps one entry per symbol.
    let mut blast_entries: Vec<(String, u32, crate::live_index::graph::RiskTier)> =
        match request.files_scope {
            false => blast_radius
                .iter()
                // PART C (019): carry disambiguating identity. Two blast
                // nodes that share a bare name (e.g. `main` in different
                // files, or two `run`s the graph kept distinct) must render
                // as distinct `path::name` entries, not collapse into one
                // indistinguishable `"main"`. The path already disambiguates
                // same-name defs across files; kind is folded into the node
                // identity by the graph, so `path::name` is sufficient here.
                .map(|node| {
                    (
                        format!("{}::{}", node.symbol.path, node.symbol.name),
                        node.hop,
                        node.risk,
                    )
                })
                .collect(),
            true => {
                let mut by_file: HashMap<String, (u32, crate::live_index::graph::RiskTier)> =
                    HashMap::new();
                for node in &blast_radius {
                    by_file
                        .entry(node.symbol.path.clone())
                        .and_modify(|(hop, risk)| {
                            if node.hop < *hop {
                                *hop = node.hop;
                                *risk = node.risk;
                            }
                        })
                        .or_insert((node.hop, node.risk));
                }
                by_file
                    .into_iter()
                    .map(|(path, (hop, risk))| (path, hop, risk))
                    .collect()
            }
        };
    // Fix 1 (Wave 1, 2026-07-02): keep the most severe blast nodes when the
    // list is capped — sort by risk severity desc, then hop asc, then name.
    blast_entries.sort_by(|a, b| {
        b.2.severity_rank()
            .cmp(&a.2.severity_rank())
            .then(a.1.cmp(&b.1))
            .then_with(|| a.0.cmp(&b.0))
    });

    Ok(ImpactReport {
        changed_files,
        changed_symbols,
        blast_entries,
        include_data,
        source_filtered_out,
        requested_depth,
        effective_depth,
        base_disclosure,
        staleness_note,
    })
}
