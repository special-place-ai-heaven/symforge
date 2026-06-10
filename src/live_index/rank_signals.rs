//! Rank-signal extension point for the search ranker fusion.
//!
//! Feature tentacles register additional [`RankSignal`] impls to contribute to
//! the weighted sum computed by [`combine`]. This layer has no knowledge of
//! specific features — see ADR 0012 for the pattern. The two default signals
//! (`PathMatchSignal`, `CoChangeSignal`) score a candidate path against the
//! shared [`RankCtx`]; `capture_search_files_view` uses [`combine`] as its
//! primary sort key while preserving tier labels for the formatter.
//!
//! `PathMatchSignal` returns one of a small set of tier weights
//! (strong / basename / prefix / loose / none) that reproduce the previous
//! tier-bucket concatenation ordering: `Strong > Basename > Prefix > Loose`.
//! `CoChangeSignal` consumes caller-prepared co-change evidence. Default
//! callers leave those inputs absent so it contributes `0.0`; callers that opt
//! into `rank_by="path+cochange"` provide anchor and partner evidence.

use std::path::Path;
use std::sync::{OnceLock, RwLock};

use super::query::path_has_component;

/// Weight applied to paths that the caller treats as a strong path match
/// (exact path, suffix with path context, or basename match whose component
/// tokens are all present in the candidate path).
pub const STRONG_PATH_SCORE: f32 = 1000.0;
/// Weight applied to paths whose basename matches but whose non-basename
/// component tokens are absent (or unspecified).
pub const BASENAME_SCORE: f32 = 100.0;
/// Weight applied to paths whose basename stem starts with the query's
/// basename token (prefix match). Ranks below basename, above loose.
pub const PREFIX_SCORE: f32 = 50.0;
/// Weight applied to paths that contain every query token as a case-insensitive
/// substring - the weakest path-relevance match the live ranker surfaces.
pub const LOOSE_PATH_SCORE: f32 = 10.0;
/// Minimum file-level shared commits required before co-change evidence can
/// affect ranking. A single shared commit is treated as weak calibration noise.
pub const FILE_LEVEL_CO_CHANGE_FLOOR: u32 = 2;
/// Co-change fusion only applies when the anchor itself reaches basename-tier
/// path confidence. Prefix/loose anchors keep the baseline path ordering.
pub const CO_CHANGE_ANCHOR_CONFIDENCE_FLOOR: f32 = BASENAME_SCORE;

// Keep chore anchors hardcoded until SymForge has a broader workspace-config
// trust policy for ranking inputs. These files change across unrelated tasks
// often enough that they should never drive co-change promotion by default.
const CHORE_ANCHOR_FILENAMES: &[&str] = &[
    "Cargo.lock",
    "package-lock.json",
    "uv.lock",
    "poetry.lock",
    "yarn.lock",
    "pnpm-lock.yaml",
    "CHANGELOG.md",
    ".release-please-manifest.json",
];

/// Contextual inputs shared by every registered `RankSignal` when scoring a
/// candidate path. Fields are borrowed from the caller for the duration of a
/// single `combine()` invocation.
#[derive(Debug, Clone, Copy)]
pub struct RankCtx<'a> {
    /// Normalized user query that produced the candidate set (may be empty).
    pub query: &'a str,
    /// Tokenized query as interpreted by the caller (e.g., path components).
    pub tokens: &'a [String],
    /// Optional current editor file used for proximity-style boosts.
    pub current_file: Option<&'a str>,
    /// Optional anchor path for co-change fusion.
    pub target_path: Option<&'a str>,
    /// Optional number of observed co-changes for the candidate path.
    pub co_change_count: Option<u32>,
    /// Optional normalized co-change score prepared by the caller.
    pub co_change_weighted_score: Option<f32>,
}

impl<'a> RankCtx<'a> {
    /// Construct an empty context with no query, tokens, or anchors.
    pub const fn empty() -> Self {
        Self {
            query: "",
            tokens: &[],
            current_file: None,
            target_path: None,
            co_change_count: None,
            co_change_weighted_score: None,
        }
    }
}

impl Default for RankCtx<'_> {
    fn default() -> Self {
        Self::empty()
    }
}

/// Extension point for contributing to the search ranker's weighted sum.
///
/// Implementations must be object-safe so they can be stored as
/// `Box<dyn RankSignal>` inside the process-wide registry.
pub trait RankSignal: Send + Sync {
    /// Stable identifier used for diagnostics.
    fn name(&self) -> &'static str;

    /// Per-signal weight applied to its `score()` contribution during fusion.
    fn weight(&self) -> f32;

    /// Score the given `path` against the shared `ctx`. Return `0.0` when the
    /// signal has nothing to say — this keeps the fusion well-defined when
    /// required inputs are missing.
    fn score(&self, path: &Path, ctx: &RankCtx<'_>) -> f32;
}

/// Path-match signal — classifies a candidate path against the query tokens
/// and returns one of the tier weights (`STRONG_PATH_SCORE`, `BASENAME_SCORE`,
/// `PREFIX_SCORE`, `LOOSE_PATH_SCORE`) or `0.0` for no match. The classification
/// mirrors the bucket logic previously inlined in `capture_search_files_view`.
pub struct PathMatchSignal;

/// Repo-relative path equality tolerant of presentation differences between a
/// raw caller-supplied anchor and normalized index keys: `\` vs `/` separators
/// and a leading `./`. Case is preserved — index keys are exact.
fn is_same_repo_relative_path(a: &str, b: &str) -> bool {
    fn normalize(s: &str) -> String {
        let forward = s.replace('\\', "/");
        forward.trim_start_matches("./").to_string()
    }
    normalize(a) == normalize(b)
}

impl RankSignal for PathMatchSignal {
    fn name(&self) -> &'static str {
        "path_match"
    }

    fn weight(&self) -> f32 {
        1.0
    }

    fn score(&self, path: &Path, ctx: &RankCtx<'_>) -> f32 {
        if ctx.tokens.is_empty() {
            return 0.0;
        }

        let path_str = path.to_string_lossy();
        let path_lower = path_str.to_ascii_lowercase();
        let query_lower = ctx.query.to_ascii_lowercase();
        let has_path_context = ctx.query.contains('/');

        let is_exact = !query_lower.is_empty() && path_lower == query_lower;
        let is_suffix =
            has_path_context && !query_lower.is_empty() && path_lower.ends_with(&query_lower);

        let basename_token = ctx.tokens.last().map(String::as_str).unwrap_or("");
        let component_tokens: &[String] = if ctx.tokens.len() > 1 {
            &ctx.tokens[..ctx.tokens.len() - 1]
        } else {
            &[]
        };

        let file_basename = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let file_stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();

        // A stem-only query that names the ANCHOR (e.g. `work_item` for
        // `work_item.rs`) promotes to BASENAME tier so co-change fusion clears
        // the anchor-confidence floor (SF-006). The promotion applies ONLY when
        // the path being scored IS the co-change anchor (`ctx.target_path`) —
        // the CoChangeSignal floor check and `anchor_path_match_score` both
        // score the anchor itself — so an anchor-gate fix does not globally
        // reshape candidate path tiers (review finding 3, post-v7.19.0).
        // The `file_stem == basename_token` arm is gated behind the same `>= 3`
        // length guard the prefix path uses below (query.rs:~1236) so 1-2 char
        // stems (`a`, `io`) stay prefix-tier and do not jump to basename tier;
        // genuine prefixes (`work` vs `work_item`) also stay prefix-tier
        // because neither basename nor stem equals them.
        //
        // The comparison is normalization-tolerant (Bugbot, PR #270): indexed
        // candidate keys are forward-slash, `./`-free repo-relative paths, but
        // the anchor string can arrive raw from the caller (`./`-prefixed or
        // backslashed). A raw-vs-normalized mismatch would silently strip the
        // anchor file's own basename tier when it appears among candidates.
        let scoring_the_anchor = ctx
            .target_path
            .is_some_and(|target| is_same_repo_relative_path(target, &path_str));
        let has_basename_match = !basename_token.is_empty()
            && (file_basename == basename_token
                || (scoring_the_anchor
                    && basename_token.len() >= 3
                    && file_stem == basename_token));
        let has_all_components = !component_tokens.is_empty()
            && component_tokens
                .iter()
                .all(|component| path_has_component(&path_str, component));

        if is_exact || is_suffix || (has_basename_match && has_all_components) {
            return STRONG_PATH_SCORE;
        }
        if has_basename_match {
            return BASENAME_SCORE;
        }
        if basename_token.len() >= 3 && file_stem.starts_with(basename_token) {
            return PREFIX_SCORE;
        }
        if ctx.tokens.iter().all(|token| path_lower.contains(token)) {
            return LOOSE_PATH_SCORE;
        }

        0.0
    }
}

/// Co-change signal — consumes caller-prepared weighted coupling evidence once
/// anchor confidence, chore-anchor, and shared-commit gates pass. Missing or
/// rejected evidence contributes `0.0`, preserving default path-match ranking.
pub struct CoChangeSignal;

impl RankSignal for CoChangeSignal {
    fn name(&self) -> &'static str {
        "co_change"
    }

    fn weight(&self) -> f32 {
        1.0
    }

    fn score(&self, _path: &Path, ctx: &RankCtx<'_>) -> f32 {
        let Some(target_path) = ctx.target_path else {
            return 0.0;
        };
        if is_chore_anchor_path(target_path) {
            return 0.0;
        }
        if PathMatchSignal.score(Path::new(target_path), ctx) < CO_CHANGE_ANCHOR_CONFIDENCE_FLOOR {
            return 0.0;
        }

        let Some(co_change_count) = ctx.co_change_count else {
            return 0.0;
        };
        if co_change_count < FILE_LEVEL_CO_CHANGE_FLOOR {
            return 0.0;
        }

        let Some(weighted_score) = ctx.co_change_weighted_score else {
            return 0.0;
        };
        if !weighted_score.is_finite() || weighted_score <= 0.0 {
            return 0.0;
        }

        weighted_score
    }
}

/// Reason a co-change anchor failed to drive promotion, computed once per
/// `search_files` call at anchor level (not per candidate). Mirrors the gate
/// order inside [`CoChangeSignal::score`]: chore-anchor exclusion fires before
/// the anchor-confidence floor, so the variants are checked in that same order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnchorCoChangeRejection {
    /// The anchor is a hardcoded chore file (lockfile, changelog, workflow);
    /// excluded from co-change promotion before the confidence gate.
    ChoreAnchor,
    /// The anchor reached only prefix-tier path confidence (a stem prefix, not
    /// the anchor basename), below the basename-tier floor.
    BelowConfidenceFloor,
}

/// Classify why a co-change anchor was rejected, given the anchor's path-match
/// score and its on-disk path. Returns `None` when the anchor cleared both
/// gates (chore-anchor exclusion and the basename-tier confidence floor), in
/// which case any fallback is a downstream cause (e.g. no neighbor key matched
/// a returned candidate). Computed once at anchor level by the caller.
pub fn classify_anchor_cochange_rejection(
    anchor_path: &str,
    anchor_path_match_score: f32,
) -> Option<AnchorCoChangeRejection> {
    if is_chore_anchor_path(anchor_path) {
        return Some(AnchorCoChangeRejection::ChoreAnchor);
    }
    if anchor_path_match_score < CO_CHANGE_ANCHOR_CONFIDENCE_FLOOR {
        return Some(AnchorCoChangeRejection::BelowConfidenceFloor);
    }
    None
}

pub(crate) fn is_chore_anchor_path(path: &str) -> bool {
    let normalized = path.replace('\\', "/");
    let normalized = normalized.trim_start_matches("./");
    let file_name = normalized.rsplit('/').next().unwrap_or(normalized);
    if CHORE_ANCHOR_FILENAMES.contains(&file_name) {
        return true;
    }

    let Some(workflow_name) = normalized.strip_prefix(".github/workflows/") else {
        return false;
    };
    workflow_name.ends_with(".yml") || workflow_name.ends_with(".yaml")
}

fn registry() -> &'static RwLock<Vec<Box<dyn RankSignal>>> {
    static REGISTRY: OnceLock<RwLock<Vec<Box<dyn RankSignal>>>> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        let defaults: Vec<Box<dyn RankSignal>> =
            vec![Box::new(PathMatchSignal), Box::new(CoChangeSignal)];
        RwLock::new(defaults)
    })
}

/// Append a `RankSignal` to the process-wide registry. Feature tentacles call
/// this at their own initialization time; the registry never empties.
pub fn register(signal: Box<dyn RankSignal>) {
    let mut guard = registry().write().expect("rank_signals registry poisoned");
    guard.push(signal);
}

/// Weighted-sum fusion over every registered `RankSignal`.
pub fn combine(path: &Path, ctx: &RankCtx<'_>) -> f32 {
    let guard = registry().read().expect("rank_signals registry poisoned");
    guard
        .iter()
        .map(|signal| signal.weight() * signal.score(path, ctx))
        .sum()
}

#[cfg(test)]
fn registered_count() -> usize {
    registry()
        .read()
        .expect("rank_signals registry poisoned")
        .len()
}

#[cfg(test)]
fn reset_for_tests() {
    let mut guard = registry().write().expect("rank_signals registry poisoned");
    guard.clear();
    guard.push(Box::new(PathMatchSignal));
    guard.push(Box::new(CoChangeSignal));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_register_two_signals() {
        reset_for_tests();
        assert_eq!(registered_count(), 2);
    }

    #[test]
    fn default_signals_expose_stable_names() {
        assert_eq!(PathMatchSignal.name(), "path_match");
        assert_eq!(CoChangeSignal.name(), "co_change");
    }

    #[test]
    fn default_signals_score_zero_on_any_input() {
        let ctx = RankCtx::empty();
        assert_eq!(PathMatchSignal.score(Path::new("foo.rs"), &ctx), 0.0);
        assert_eq!(CoChangeSignal.score(Path::new("foo.rs"), &ctx), 0.0);
    }

    #[test]
    fn rank_signal_is_object_safe() {
        let _boxed: Box<dyn RankSignal> = Box::new(PathMatchSignal);
        let _erased: &dyn RankSignal = &CoChangeSignal;
    }

    #[test]
    fn combine_with_defaults_returns_zero() {
        reset_for_tests();
        let ctx = RankCtx::empty();
        assert_eq!(
            combine(Path::new("src/live_index/rank_signals.rs"), &ctx),
            0.0
        );
    }

    #[test]
    fn combine_sums_weighted_scores_from_registered_signals() {
        struct FixedTwoTimesThree;
        impl RankSignal for FixedTwoTimesThree {
            fn name(&self) -> &'static str {
                "__test_two_times_three"
            }
            fn weight(&self) -> f32 {
                2.0
            }
            fn score(&self, _path: &Path, _ctx: &RankCtx<'_>) -> f32 {
                3.0
            }
        }

        struct FixedHalfTimesFour;
        impl RankSignal for FixedHalfTimesFour {
            fn name(&self) -> &'static str {
                "__test_half_times_four"
            }
            fn weight(&self) -> f32 {
                0.5
            }
            fn score(&self, _path: &Path, _ctx: &RankCtx<'_>) -> f32 {
                4.0
            }
        }

        reset_for_tests();
        register(Box::new(FixedTwoTimesThree));
        register(Box::new(FixedHalfTimesFour));

        let ctx = RankCtx::empty();
        let total = combine(Path::new("anything"), &ctx);
        // defaults contribute 0.0; (2.0 * 3.0) + (0.5 * 4.0) = 8.0
        assert!((total - 8.0).abs() < f32::EPSILON);

        reset_for_tests();
    }

    #[test]
    fn rank_ctx_default_matches_empty() {
        let default = RankCtx::default();
        let empty = RankCtx::empty();
        assert_eq!(default.query, empty.query);
        assert_eq!(default.tokens.len(), empty.tokens.len());
        assert_eq!(default.current_file, empty.current_file);
        assert_eq!(default.target_path, empty.target_path);
        assert_eq!(default.co_change_count, empty.co_change_count);
        assert_eq!(
            default.co_change_weighted_score,
            empty.co_change_weighted_score
        );
    }

    #[test]
    fn rank_ctx_defaults_co_change_inputs_to_none() {
        let ctx = RankCtx::empty();
        assert_eq!(ctx.co_change_count, None);
        assert_eq!(ctx.co_change_weighted_score, None);
    }

    #[test]
    fn co_change_signal_ignores_absent_or_zero_inputs_for_now() {
        let none_ctx = RankCtx::empty();
        let zero_ctx = RankCtx {
            co_change_count: Some(0),
            co_change_weighted_score: Some(0.0),
            ..RankCtx::empty()
        };

        assert_eq!(CoChangeSignal.score(Path::new("foo.rs"), &none_ctx), 0.0);
        assert_eq!(CoChangeSignal.score(Path::new("foo.rs"), &zero_ctx), 0.0);
    }

    fn ctx_with<'a>(query: &'a str, tokens: &'a [String]) -> RankCtx<'a> {
        RankCtx {
            query,
            tokens,
            current_file: None,
            target_path: None,
            co_change_count: None,
            co_change_weighted_score: None,
        }
    }

    /// Review finding 3 (post-v7.19.0): the SF-006 stem-equals-basename
    /// promotion is ANCHOR-ONLY. Scoring the co-change anchor itself (path ==
    /// ctx.target_path) promotes a ≥3-char stem query to basename tier so the
    /// anchor-confidence floor clears; scoring an ordinary CANDIDATE with the
    /// same stem keeps the pre-SF-006 prefix tier, so the anchor-gate fix does
    /// not globally reshape candidate path scoring.
    #[test]
    fn stem_promotion_applies_to_anchor_only() {
        let tokens = vec!["work_item".to_string()];

        // Anchor scoring: target_path IS the scored path -> basename tier.
        let anchor_ctx = RankCtx {
            target_path: Some("src/stores/work_item.rs"),
            ..ctx_with("work_item", &tokens)
        };
        assert_eq!(
            PathMatchSignal.score(Path::new("src/stores/work_item.rs"), &anchor_ctx),
            BASENAME_SCORE,
            "a stem query naming the anchor must promote to basename tier"
        );

        // Candidate scoring: same stem match, but the scored path is NOT the
        // anchor -> stays prefix tier (pre-SF-006 behavior).
        let candidate_ctx = RankCtx {
            target_path: Some("src/stores/other_anchor.rs"),
            ..ctx_with("work_item", &tokens)
        };
        assert_eq!(
            PathMatchSignal.score(Path::new("src/stores/work_item.rs"), &candidate_ctx),
            PREFIX_SCORE,
            "a non-anchor candidate must not receive the stem promotion"
        );

        // No anchor at all (plain path search): also prefix tier.
        assert_eq!(
            PathMatchSignal.score(
                Path::new("src/stores/work_item.rs"),
                &ctx_with("work_item", &tokens)
            ),
            PREFIX_SCORE,
            "anchorless scoring must not receive the stem promotion"
        );

        // Exact basename queries are unaffected by the gate in all roles.
        let basename_tokens = vec!["work_item.rs".to_string()];
        assert_eq!(
            PathMatchSignal.score(
                Path::new("src/stores/work_item.rs"),
                &ctx_with("work_item.rs", &basename_tokens)
            ),
            BASENAME_SCORE,
            "an exact basename match keeps basename tier without anchor status"
        );

        // Normalization tolerance (Bugbot, PR #270): a raw caller-supplied
        // anchor (`./`-prefixed or backslashed) must still be recognized as
        // the anchor when the scored candidate uses the normalized index key.
        for raw_anchor in ["./src/stores/work_item.rs", r"src\stores\work_item.rs"] {
            let raw_ctx = RankCtx {
                target_path: Some(raw_anchor),
                ..ctx_with("work_item", &tokens)
            };
            assert_eq!(
                PathMatchSignal.score(Path::new("src/stores/work_item.rs"), &raw_ctx),
                BASENAME_SCORE,
                "raw anchor `{raw_anchor}` must match the normalized candidate key"
            );
        }
    }

    #[test]
    fn path_match_strong_on_exact_path() {
        let tokens = vec![
            "src".to_string(),
            "protocol".to_string(),
            "tools.rs".to_string(),
        ];
        let ctx = ctx_with("src/protocol/tools.rs", &tokens);
        assert_eq!(
            PathMatchSignal.score(Path::new("src/protocol/tools.rs"), &ctx),
            STRONG_PATH_SCORE
        );
    }

    #[test]
    fn path_match_strong_on_suffix_with_path_context() {
        let tokens = vec!["live_index".to_string(), "search.rs".to_string()];
        let ctx = ctx_with("live_index/search.rs", &tokens);
        assert_eq!(
            PathMatchSignal.score(Path::new("src/live_index/search.rs"), &ctx),
            STRONG_PATH_SCORE
        );
    }

    #[test]
    fn path_match_strong_on_basename_plus_component_tokens() {
        let tokens = vec!["protocol".to_string(), "tools.rs".to_string()];
        let ctx = ctx_with("protocol/tools.rs", &tokens);
        // `src/protocol/tools.rs` is a suffix match, so also Strong.
        assert_eq!(
            PathMatchSignal.score(Path::new("src/protocol/tools.rs"), &ctx),
            STRONG_PATH_SCORE
        );
        // `src/sidecar/tools.rs` has the basename but lacks the `protocol`
        // component — demotes to Basename tier.
        assert_eq!(
            PathMatchSignal.score(Path::new("src/sidecar/tools.rs"), &ctx),
            BASENAME_SCORE
        );
    }

    #[test]
    fn path_match_basename_only_without_component_tokens() {
        let tokens = vec!["tools.rs".to_string()];
        let ctx = ctx_with("tools.rs", &tokens);
        assert_eq!(
            PathMatchSignal.score(Path::new("src/protocol/tools.rs"), &ctx),
            BASENAME_SCORE
        );
    }

    #[test]
    fn path_match_prefix_on_basename_stem() {
        let tokens = vec!["orchestrat".to_string()];
        let ctx = ctx_with("orchestrat", &tokens);
        assert_eq!(
            PathMatchSignal.score(Path::new("src/orchestrator.rs"), &ctx),
            PREFIX_SCORE
        );
        assert_eq!(
            PathMatchSignal.score(Path::new("src/orchestration.rs"), &ctx),
            PREFIX_SCORE
        );
    }

    #[test]
    fn path_match_prefix_requires_minimum_token_length() {
        let tokens = vec!["or".to_string()];
        let ctx = ctx_with("or", &tokens);
        // Too short for prefix, but `or` appears in `orchestrator.rs` path → Loose.
        assert_eq!(
            PathMatchSignal.score(Path::new("src/orchestrator.rs"), &ctx),
            LOOSE_PATH_SCORE
        );
    }

    #[test]
    fn path_match_loose_when_all_tokens_contained() {
        let tokens = vec!["protocol".to_string()];
        let ctx = ctx_with("protocol", &tokens);
        assert_eq!(
            PathMatchSignal.score(Path::new("src/protocol/tools.rs"), &ctx),
            LOOSE_PATH_SCORE
        );
    }

    #[test]
    fn path_match_zero_when_no_token_matches() {
        let tokens = vec!["definitely_not_in_fixture".to_string()];
        let ctx = ctx_with("definitely_not_in_fixture", &tokens);
        assert_eq!(
            PathMatchSignal.score(Path::new("src/protocol/tools.rs"), &ctx),
            0.0
        );
    }

    #[test]
    #[allow(clippy::assertions_on_constants)] // intentional: lock-in tier ordering invariant
    fn path_match_tier_ordering_preserved_by_score() {
        // The migration's invariant: combine() yields Strong > Basename >
        // Prefix > Loose > no-match. This lock-in test fails loudly if a
        // future weight tweak re-orders tiers.
        assert!(STRONG_PATH_SCORE > BASENAME_SCORE);
        assert!(BASENAME_SCORE > PREFIX_SCORE);
        assert!(PREFIX_SCORE > LOOSE_PATH_SCORE);
        assert!(LOOSE_PATH_SCORE > 0.0);
    }

    #[test]
    fn path_match_case_insensitive() {
        let tokens = vec!["tools.rs".to_string()];
        let ctx = ctx_with("tools.rs", &tokens);
        assert_eq!(
            PathMatchSignal.score(Path::new("SRC/Protocol/Tools.RS"), &ctx),
            BASENAME_SCORE
        );
    }
}
