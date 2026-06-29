//! Engine-internal base+overlay index primitive (Feature 012 de-risk spike).
//!
//! This module generalizes the existing `ArcSwap<LiveIndex>` snapshot into a
//! read surface = an immutable shared **base** (`Arc<LiveIndex>`, the existing
//! snapshot, UNCHANGED) + an optional per-consumer copy-on-write **overlay** of
//! dirty/uncommitted deltas. See `specs/012-harness-agnostic-mcp/{spec,research,
//! data-model,quickstart}.md` (D1 base+overlay, D2 generation-fence
//! invalidation, D3 embed-facade boundary).
//!
//! SEMVER: these types are intentionally **NOT** part of the frozen `embed`
//! contract (`src/embed.rs` `contract` module). They are reachable by embedders
//! only via the deep-path re-export `symforge::embed::live_index::view::*` and
//! are explicitly UNSTABLE (overlay-invalidation internals may churn at MINOR).
//! Do not add them to `embed.rs` or its contract test (D3 / SC-011).
//!
//! Invariants:
//! * `LiveIndex` is unchanged; the commit identity lives on [`IndexBase`], never
//!   inside `LiveIndex` (avoids touching the frozen `LiveIndex` contract).
//! * An [`IndexBase`] is immutable once published; a new commit produces a NEW
//!   `IndexBase` (new key + incremented `base_generation`), never a mutation.
//! * An [`Overlay`] is owned by exactly one consumer; isolation (SC-003) is an
//!   ownership invariant, not a runtime check.
//! * `base_generation` is a NEW counter distinct from
//!   `SharedIndexHandle::project_generation` (project identity vs commit
//!   identity, research D2).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use crate::domain::{ReferenceKind, ReferenceRecord};

use super::search::{
    DEFAULT_MAX_PER_FILE, SymbolMatchTier, SymbolSearchOptions, TextSearchError, TextSearchOptions,
    TextSearchResult, compute_test_ranges, current_code_search_keeps_file,
    search_symbols_with_options, search_text_with_options, truncate_display_line,
};
use super::store::{IndexedFile, LiveIndex};
use crate::live_index::query::is_filtered_name;

/// The commit identity component of a [`BaseKey`].
///
/// `Sha` carries `head_sha` (`git.rs:403`); `Dirtyless` is the sentinel used
/// when the canonical root is not a git repository (so a non-git tree still has
/// a stable, shareable base identity).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum CommitId {
    /// Resolved git HEAD sha for the canonical root.
    Sha(String),
    /// Sentinel for a non-git canonical root.
    Dirtyless,
}

/// Identity of an immutable set of repository facts.
///
/// Two consumers with an equal `BaseKey` MUST share one [`IndexBase`]
/// allocation (SC-002). Different worktrees of one logical repo have different
/// `canonical_root`s and therefore distinct keys (no cross-state contamination,
/// User Story 3 edge case).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct BaseKey {
    /// The declared, canonicalized workspace root.
    pub canonical_root: PathBuf,
    /// `head_sha` or [`CommitId::Dirtyless`] when not a git repo.
    pub commit: CommitId,
}

impl BaseKey {
    pub fn new(canonical_root: impl Into<PathBuf>, commit: CommitId) -> Self {
        Self {
            canonical_root: canonical_root.into(),
            commit,
        }
    }
}

/// Wraps the existing immutable snapshot with a sharable identity + fence token.
///
/// The base payload type is `Arc<LiveIndex>` — already contracted in `embed.rs`
/// — so the base layer needs zero facade change (D3 / SC-011). `base_generation`
/// is a NEW monotonic counter, distinct from `project_generation`.
#[derive(Clone)]
pub struct IndexBase {
    /// Sharable identity `(canonical_root, commit)`.
    pub key: BaseKey,
    /// The existing immutable snapshot, shared by `Arc` across consumers.
    ///
    /// STALENESS (documented limitation): this is a SNAPSHOT frozen at the
    /// instant the base was interned/opened — the `Arc<LiveIndex>` handle taken
    /// from the project's `ArcSwap` at intern time. It is NOT live: the project's
    /// background watcher keeps swapping a NEW `Arc<LiveIndex>` into the project's
    /// `ArcSwap` as files change on disk, but those swaps do NOT rewrite this
    /// already-interned handle. So a cross-project read served from this base goes
    /// stale after ANY watcher-picked-up change to the project (edit, add, delete
    /// on disk) — NOT only after a git commit. `base_generation` advances only
    /// when a NEW base is published (a fresh intern), which today does not
    /// re-trigger on watcher reloads of an already-interned base. Re-interning a
    /// base on watcher-observed change so a long-lived cross-project session
    /// tracks current repository state (the live-freshness rebase) is the deferred
    /// Phase 4 work; until then a session should re-open (retarget / additive
    /// re-add) a project to pick up a fresh base.
    pub index: Arc<LiveIndex>,
    /// Monotonic fence token, bumped when a NEW base is published (a fresh intern,
    /// e.g. a new commit producing a new `BaseKey`). See the `index` field's
    /// STALENESS note: this generation does NOT advance for watcher reloads of an
    /// already-interned base, so it is a publish-identity token, not a
    /// live-freshness signal.
    pub base_generation: u64,
}

impl IndexBase {
    /// Publish a base for `key` at `base_generation` wrapping `index`.
    pub fn new(key: BaseKey, index: Arc<LiveIndex>, base_generation: u64) -> Self {
        Self {
            key,
            index,
            base_generation,
        }
    }
}

/// A single consumer's copy-on-write change to one file relative to its base.
#[derive(Clone)]
pub enum FileDelta {
    /// The overlay's version of a file shadows the base (added or edited).
    Upsert(Arc<IndexedFile>),
    /// The file is deleted in the overlay; it is hidden from the base.
    Tombstone,
}

/// A single consumer's copy-on-write deltas over one [`IndexBase`].
///
/// Holds ONLY dirty/uncommitted files; an absent key means "see base". An
/// overlay is owned by exactly one consumer and is never shared or read by
/// another consumer's [`IndexView`] (isolation, SC-003).
#[derive(Clone)]
pub struct Overlay {
    /// The base this overlay was derived against (identity half of the fence).
    pub base_key: BaseKey,
    /// The base generation this overlay was derived against (fence token).
    pub base_generation: u64,
    /// Dirty/uncommitted deltas, keyed by forward-slash relative path.
    pub deltas: HashMap<String, FileDelta>,
}

impl Overlay {
    /// A fresh, empty overlay fenced to `base`.
    pub fn fresh(base: &IndexBase) -> Self {
        Self {
            base_key: base.key.clone(),
            base_generation: base.base_generation,
            deltas: HashMap::new(),
        }
    }

    /// Number of live deltas (the dirty-set size K).
    pub fn delta_count(&self) -> usize {
        self.deltas.len()
    }

    /// Validity check against a base (D2): an overlay is valid iff its key and
    /// generation both match the base it is read against.
    pub fn is_valid_against(&self, base: &IndexBase) -> bool {
        self.base_key == base.key && self.base_generation == base.base_generation
    }
}

/// Error returned when an [`Overlay`] is read against a base it is not fenced to.
///
/// Carries the expected (base) and actual (overlay) generations for diagnostics.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StaleOverlay {
    pub base_generation: u64,
    pub overlay_generation: u64,
}

impl std::fmt::Display for StaleOverlay {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "stale overlay: base generation {} != overlay generation {}",
            self.base_generation, self.overlay_generation
        )
    }
}

impl std::error::Error for StaleOverlay {}

/// The read surface: an immutable base plus an optional per-consumer overlay.
///
/// `overlay: None` is byte-for-byte today's behavior (the migration seam, D1).
/// Resolution: the overlay shadows the base on a key collision; a
/// [`FileDelta::Tombstone`] hides a base file.
pub struct IndexView<'a> {
    base: &'a LiveIndex,
    overlay: Option<&'a Overlay>,
}

impl<'a> IndexView<'a> {
    /// Build a view over `base`, optionally applying `overlay`.
    ///
    /// Returns `Err(StaleOverlay)` if the overlay's fence does not match the
    /// base's `base_generation` (never serve stale; D2). `overlay: None` always
    /// succeeds and reproduces today's base-only behavior.
    pub fn new(base: &'a IndexBase, overlay: Option<&'a Overlay>) -> Result<Self, StaleOverlay> {
        if let Some(ov) = overlay {
            // Fence on generation (the data-model constructor rule). Key
            // mismatch is also invalid, but generation is the monotonic token
            // the spike asserts on; a key change always rides a generation bump.
            if ov.base_generation != base.base_generation || ov.base_key != base.key {
                return Err(StaleOverlay {
                    base_generation: base.base_generation,
                    overlay_generation: ov.base_generation,
                });
            }
        }
        Ok(Self {
            base: base.index.as_ref(),
            overlay,
        })
    }

    /// The degenerate base-only view (the migration seam; no fence needed).
    pub fn base_only(base: &'a LiveIndex) -> Self {
        Self {
            base,
            overlay: None,
        }
    }

    /// Resolve a file by relative path: overlay shadows base; a tombstone hides
    /// the base file (returns `None`).
    pub fn get_file(&self, relative_path: &str) -> Option<&IndexedFile> {
        if let Some(ov) = self.overlay
            && let Some(delta) = ov.deltas.get(relative_path)
        {
            return match delta {
                FileDelta::Upsert(file) => Some(file.as_ref()),
                FileDelta::Tombstone => None,
            };
        }
        self.base.get_file(relative_path)
    }

    /// Iterate all resolved `(path, file)` pairs: base files not shadowed by an
    /// overlay delta, plus overlay upserts; tombstoned base files are excluded.
    ///
    /// Allocates a result `Vec` because resolution merges two maps; this is a
    /// convenience iterator, not a hot path. The single-consumer path
    /// (`overlay: None`) returns base files directly with no overlay work.
    pub fn all_files(&self) -> Vec<(String, &IndexedFile)> {
        match self.overlay {
            None => self.base.all_files().map(|(p, f)| (p.clone(), f)).collect(),
            Some(ov) => {
                let mut out: Vec<(String, &IndexedFile)> = Vec::new();
                // Base files, minus any shadowed/tombstoned by the overlay.
                for (path, file) in self.base.all_files() {
                    match ov.deltas.get(path) {
                        Some(FileDelta::Tombstone) => continue,
                        Some(FileDelta::Upsert(_)) => continue, // emitted below
                        None => out.push((path.clone(), file)),
                    }
                }
                // Overlay upserts (added files + edited shadows).
                for (path, delta) in &ov.deltas {
                    if let FileDelta::Upsert(file) = delta {
                        out.push((path.clone(), file.as_ref()));
                    }
                }
                out
            }
        }
    }

    /// Live overlay deltas, if any (`None` for a base-only view). Used by the
    /// post-filter search paths to identify the small dirty set.
    fn overlay_deltas(&self) -> Option<&'a HashMap<String, FileDelta>> {
        self.overlay.map(|ov| &ov.deltas)
    }

    /// Symbol-name search resolved THROUGH the overlay (research D1: symbol-name
    /// search resolves through the overlay, not via the base's derived indices).
    ///
    /// For a view with NO live deltas (base-only, OR an empty overlay — the US1
    /// cross-project case, since overlays are never written there) this delegates
    /// straight to the engine's option-honoring [`search_symbols_with_options`]
    /// over the base, so the caller's `options` (path scope, language filter,
    /// noise policy, and the `result_limit` ranking) ARE honored on the
    /// cross-project read path (D11 scoping + D14 ranking). When live overlay
    /// deltas ARE present we cannot call the base function (it would scan the
    /// stale base file map), so we scan the OVERLAY-RESOLVED file set
    /// ([`all_files`]) directly with the name-match + tiering rules the engine
    /// uses. Correctness over speed: this is a linear scan of the resolved set.
    ///
    /// `// DEFERRED (D-B0):` the overlay-present scan applies only the name/kind
    /// tiering needed for cross-project attribution; honoring the full option
    /// surface (path scope, language, noise) over the dirty set is the deferred
    /// per-overlay derived-index work. That branch is unreached on the US1
    /// cross-project path (overlays are empty there); until then the overlay
    /// symbol search is a minimal, correct superset.
    ///
    /// [`all_files`]: IndexView::all_files
    pub fn search_symbols(
        &self,
        query: &str,
        kind_filter: Option<&str>,
        options: &SymbolSearchOptions,
    ) -> Vec<ViewSymbolHit> {
        // No live deltas (absent OR empty overlay) -> reuse the engine's
        // option-honoring search verbatim over the base. This IS the US1
        // cross-project path; caller scoping + result_limit ranking are honored.
        let no_deltas = match self.overlay_deltas() {
            None => true,
            Some(deltas) => deltas.is_empty(),
        };
        if no_deltas {
            let result = search_symbols_with_options(self.base, query, kind_filter, options);
            return result
                .hits
                .into_iter()
                .map(|hit| ViewSymbolHit {
                    name: hit.name,
                    path: hit.path,
                    kind: hit.kind,
                    line: hit.line,
                    tier: hit.tier,
                })
                .collect();
        }

        let query_lower = query.to_lowercase();
        let mut hits: Vec<ViewSymbolHit> = Vec::new();
        let mut resolved = self.all_files();
        // Stable ordering before tiering so equal-tier ties are deterministic.
        resolved.sort_by(|a, b| a.0.cmp(&b.0));

        for (path, file) in resolved {
            for sym in &file.symbols {
                if let Some(filter) = kind_filter
                    && !filter.eq_ignore_ascii_case("all")
                    && !symbol_kind_matches(filter, &sym.kind)
                {
                    continue;
                }
                let name_lower = sym.name.to_lowercase();
                if !name_lower.contains(&query_lower) {
                    continue;
                }
                let tier = if name_lower == query_lower {
                    SymbolMatchTier::Exact
                } else if name_lower.starts_with(&query_lower) {
                    SymbolMatchTier::Prefix
                } else {
                    SymbolMatchTier::Substring
                };
                hits.push(ViewSymbolHit {
                    name: sym.name.clone(),
                    path: path.clone(),
                    kind: sym.kind.to_string(),
                    line: sym.line_range.0 + 1,
                    tier,
                });
            }
        }

        hits.sort_by(|a, b| {
            a.tier
                .cmp(&b.tier)
                .then(a.name.cmp(&b.name))
                .then(a.path.cmp(&b.path))
        });
        hits
    }

    /// Text search served BASE-ONLY + an overlay POST-FILTER over the dirty set
    /// (research D1 sub-decision).
    ///
    /// `search_text` depends on the base's repo-wide `trigram_index`, which the
    /// overlay does NOT re-derive (that is the DEFERRED per-consumer derived
    /// index). So we:
    /// 1. run the engine [`search_text_with_options`] against the immutable base
    ///    index (honoring the caller's `options` — path scope, language, noise,
    ///    limits — so cross-project text scoping is honored, D11);
    /// 2. DROP every base file-result whose path is a live overlay delta — its
    ///    base content is stale (the consumer edited or deleted that file);
    /// 3. directly SCAN each overlay `Upsert`'s current content for the same
    ///    query and append fresh matches for it.
    ///
    /// The net result reflects the consumer's overlay: a dirty edit's stale base
    /// hit is never returned, and the upserted file's real hits are. Cost is
    /// O(base text search) + O(dirty set scan), never an overlay-wide reindex.
    ///
    /// The overlay scan mirrors the base file-level filtering: each upserted
    /// file is gated by [`current_code_search_keeps_file`] (scope=Code, default
    /// noise policy hiding test/generated files, personal-tooling excluded) so a
    /// dirty test/generated file does not leak hits the base would suppress, and
    /// the per-file scan applies the same `#[cfg(test)]` line suppression,
    /// per-file cap (`DEFAULT_MAX_PER_FILE`), and line truncation. Appended
    /// overlay files are sorted by path for deterministic output (Principle V).
    ///
    /// `// DEFERRED (012 full tier):` a per-overlay trigram index would let the
    /// dirty-file scan use the same accelerated path as the base. Until then the
    /// dirty files are scanned linearly (correct, bounded by the small dirty
    /// set). DOCUMENTED LIMITATION: after the post-filter, `total_matches` is
    /// RECOMPUTED as the sum of the returned files' displayed match lines (each
    /// bounded by `DEFAULT_MAX_PER_FILE`), so it is coherent with `files` but is
    /// NOT the base's pre-cap visible total. Because the merged result differs
    /// from the base result, the base's `overflow_count` and
    /// `suppressed_by_noise` would be STALE; rather than carry misleading base
    /// numbers, the overlay branch sets `overflow_count = 0` (no precise
    /// merged-overflow claim) and `suppressed_by_noise` to ONLY the overlay
    /// `#[cfg(test)]` suppressions actually counted while scanning the dirty set.
    /// Per-consumer derived indices that would make these globally exact are the
    /// deferred work.
    pub fn search_text(
        &self,
        query: Option<&str>,
        terms: Option<&[String]>,
        regex: bool,
        options: &TextSearchOptions,
    ) -> Result<TextSearchResult, TextSearchError> {
        let mut result = search_text_with_options(self.base, query, terms, regex, options)?;

        let Some(deltas) = self.overlay_deltas() else {
            return Ok(result);
        };
        if deltas.is_empty() {
            return Ok(result);
        }

        // (2) Drop base file-results shadowed by ANY live delta (upsert or
        // tombstone): the base content for those paths is stale or removed.
        result.files.retain(|file| !deltas.contains_key(&file.path));

        // (3) Scan each overlay Upsert directly for the same matcher and append
        // its current matches. A Tombstone contributes nothing (file removed).
        //
        // Overlay files are gathered into a vec and SORTED BY PATH before
        // appending so the merged output order is DETERMINISTIC (Principle V/IV);
        // `deltas` is a HashMap whose iteration order is not stable. Each
        // upserted file is first put through the SAME file-level scope+noise gate
        // the base applies (`current_code_search_keeps_file`) so an upserted
        // test/generated/vendor/personal-tooling file does NOT leak hits the base
        // would suppress; then `scan_file_lines` applies the in-file
        // `#[cfg(test)]` suppression, the per-file cap, and line truncation.
        let matcher = overlay_text_matcher(query, terms, regex)?;
        let mut overlay_suppressed = 0usize;
        if let Some(matcher) = matcher {
            let mut appended: Vec<super::search::TextFileMatches> = Vec::new();
            for (path, delta) in deltas {
                let FileDelta::Upsert(file) = delta else {
                    continue;
                };
                // File-level scope+noise gate (mirrors the base's
                // file_matches_text_base_scope + file_hidden_by_search_policy at
                // the default code-search preset).
                if !current_code_search_keeps_file(path, &file.classification) {
                    continue;
                }
                let scan = scan_file_lines(file, &matcher);
                overlay_suppressed += scan.suppressed;
                if !scan.matches.is_empty() {
                    appended.push(super::search::TextFileMatches {
                        path: path.clone(),
                        matches: scan.matches,
                        rendered_lines: None,
                        callers: None,
                    });
                }
            }
            // Deterministic ordering of the appended overlay files.
            appended.sort_by(|a, b| a.path.cmp(&b.path));
            result.files.extend(appended);
        }

        // Recompute `total_matches` to stay COHERENT with the post-filtered
        // `files` set: dropping a base file-result must subtract its matches, and
        // appended overlay scans must add theirs. We recount from the retained +
        // appended `matches` lists rather than incrementally patching the base
        // counter, which would leave it stale after the retain above. NOTE: this
        // makes `total_matches` reflect the displayed match lines (each file's
        // `matches` is bounded by `DEFAULT_MAX_PER_FILE`); it is an honest count
        // of the merged result, not the base's pre-cap visible total. See the
        // DOCUMENTED LIMITATION on this method.
        result.total_matches = result.files.iter().map(|f| f.matches.len()).sum();

        // `overflow_count` and `suppressed_by_noise` from the BASE search
        // described the pre-filter base result; after dropping base files and
        // appending overlay scans they no longer describe the merged result, so
        // carrying the base numbers would be a LIE. We cannot cheaply re-derive a
        // globally exact visible-overflow across the overlay (that is the
        // deferred per-consumer derived index), so we report a COHERENT,
        // honest-but-coarse value instead of stale base numbers:
        //   * overflow_count -> 0 (we do not claim a precise hidden-overflow
        //     count across the merged set);
        //   * suppressed_by_noise -> only the overlay `#[cfg(test)]` suppressions
        //     we actually counted while scanning the dirty set (the base's own
        //     suppression total is not re-derived across the post-filter).
        // This keeps every reported field consistent with the result we return.
        result.overflow_count = 0;
        result.suppressed_by_noise = overlay_suppressed;

        Ok(result)
    }

    /// Reference search served BASE-ONLY + an overlay POST-FILTER over the dirty
    /// set (research D1 sub-decision), mirroring [`search_text`].
    ///
    /// `find_references` depends on the base's repo-wide `reverse_index`, which
    /// the overlay does NOT re-derive (DEFERRED). So we:
    /// 1. collect base references for `name` via `find_references_for_name`;
    /// 2. DROP base hits whose file is a live overlay delta (stale or deleted);
    /// 3. directly scan each overlay `Upsert`'s `references` for `name` and
    ///    append fresh hits for it.
    ///
    /// Returns `(path, ReferenceRecord)` pairs owned by the view's lifetime.
    ///
    /// Like [`search_text`], the overlay scan applies the SAME file-level
    /// scope+noise gate as the base ([`current_code_search_keeps_file`]) so a
    /// dirty test/generated file does not leak references, honors
    /// `include_filtered` (skipping [`is_filtered_name`] references when
    /// `!include_filtered`, mirroring the base), and sorts appended overlay files
    /// by path for deterministic output (Principle V).
    ///
    /// `// DEFERRED (012 full tier):` a per-overlay reverse index would replace
    /// the linear `references` scan of the dirty files. Until then the dirty set
    /// is scanned directly (correct, O(dirty set)). DOCUMENTED LIMITATION: the
    /// overlay scan resolves only the SIMPLE-name match against each upserted
    /// file's `references` (and qualified-name equality); it does not reproduce
    /// the base's alias-map resolution across overlay files. Alias resolution
    /// across dirty files is part of the deferred derived-index work.
    ///
    /// [`search_text`]: IndexView::search_text
    pub fn find_references(
        &self,
        name: &str,
        kind_filter: Option<ReferenceKind>,
        include_filtered: bool,
    ) -> Vec<(String, &'a ReferenceRecord)> {
        let mut out: Vec<(String, &'a ReferenceRecord)> = self
            .base
            .find_references_for_name(name, kind_filter, include_filtered)
            .into_iter()
            .map(|(path, reference)| (path.to_string(), reference))
            .collect();

        let Some(deltas) = self.overlay_deltas() else {
            return out;
        };
        if deltas.is_empty() {
            return out;
        }

        // (2) Drop base hits whose file is dirty (stale or deleted).
        out.retain(|(path, _)| !deltas.contains_key(path));

        // (3) Scan overlay Upserts directly for matching references.
        //
        // Overlay files are gathered and SORTED BY PATH before appending so the
        // merged output order is DETERMINISTIC (Principle V); `deltas` iteration
        // is unordered. Each upserted file is gated by the SAME file-level
        // scope+noise filter as the base (`current_code_search_keeps_file`) so a
        // dirty test/generated/vendor/personal-tooling file does not leak
        // references the base would suppress. We mirror the base's
        // `include_filtered` behavior by skipping `is_filtered_name` references
        // (single-letter generics / language builtins) when `!include_filtered`.
        let is_qualified = name.contains("::") || name.contains('.');
        let mut appended: Vec<(String, &'a ReferenceRecord)> = Vec::new();
        for (path, delta) in deltas {
            let FileDelta::Upsert(file) = delta else {
                continue;
            };
            if !current_code_search_keeps_file(path, &file.classification) {
                continue;
            }
            for reference in &file.references {
                let name_matches = if is_qualified {
                    reference.qualified_name.as_deref() == Some(name)
                } else {
                    reference.name == name
                };
                if !name_matches {
                    continue;
                }
                if let Some(kf) = kind_filter
                    && reference.kind != kf
                {
                    continue;
                }
                // Mirror the base: hide filtered names unless explicitly asked.
                if !include_filtered && is_filtered_name(&reference.name, &file.language) {
                    continue;
                }
                appended.push((path.clone(), reference));
            }
        }
        appended.sort_by(|a, b| a.0.cmp(&b.0));
        out.extend(appended);

        out
    }
}

/// A symbol hit from a single [`IndexView`] search, before project attribution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ViewSymbolHit {
    pub name: String,
    pub path: String,
    pub kind: String,
    pub line: u32,
    pub tier: SymbolMatchTier,
}

/// Match `filter` (display string or common semantic alias) against a symbol
/// kind. A local mirror of the engine search's `kind_filter_matches` (which is
/// private to `search.rs`); kept minimal and overlay-internal.
fn symbol_kind_matches(filter: &str, kind: &crate::domain::SymbolKind) -> bool {
    if kind.to_string().eq_ignore_ascii_case(filter) {
        return true;
    }
    matches!(
        (filter.to_ascii_lowercase().as_str(), kind),
        ("variable", crate::domain::SymbolKind::Variable)
            | ("function", crate::domain::SymbolKind::Function)
            | ("method", crate::domain::SymbolKind::Method)
            | ("module", crate::domain::SymbolKind::Module)
            | ("constant", crate::domain::SymbolKind::Constant)
    )
}

/// A compiled line matcher used to scan overlay `Upsert` files directly. Mirrors
/// the engine's literal/regex/terms branching at the line level (the overlay
/// post-filter cannot reuse the base's trigram-accelerated path).
enum OverlayTextMatcher {
    /// Any of the literal terms appears in the line (case folded per `fold`).
    Terms { terms: Vec<String>, fold: bool },
    /// The compiled regex matches the line.
    Regex(regex::Regex),
}

impl OverlayTextMatcher {
    fn is_match(&self, line: &str) -> bool {
        match self {
            Self::Terms { terms, fold } => {
                if *fold {
                    let line_lower = line.to_lowercase();
                    terms.iter().any(|t| line_lower.contains(&t.to_lowercase()))
                } else {
                    terms.iter().any(|t| line.contains(t))
                }
            }
            Self::Regex(re) => re.is_match(line),
        }
    }
}

/// Build the overlay line matcher from the same `(query, terms, regex)` inputs
/// the engine `search_text` takes. Returns `Ok(None)` when there is nothing to
/// match (empty query/terms) so the caller skips the dirty-set scan.
fn overlay_text_matcher(
    query: Option<&str>,
    terms: Option<&[String]>,
    regex: bool,
) -> Result<Option<OverlayTextMatcher>, TextSearchError> {
    if regex {
        let pattern = query
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .unwrap_or("");
        if pattern.is_empty() {
            return Err(TextSearchError::EmptyRegexQuery);
        }
        let re = regex::RegexBuilder::new(pattern)
            .case_insensitive(true)
            .build()
            .map_err(|error| TextSearchError::InvalidRegex {
                pattern: pattern.to_string(),
                error: error.to_string(),
            })?;
        return Ok(Some(OverlayTextMatcher::Regex(re)));
    }

    let collected: Vec<String> = match terms {
        Some(raw) if !raw.is_empty() => raw
            .iter()
            .map(|t| t.trim())
            .filter(|t| !t.is_empty())
            .map(ToOwned::to_owned)
            .collect(),
        _ => query
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(|text| vec![text.to_string()])
            .unwrap_or_default(),
    };
    if collected.is_empty() {
        return Ok(None);
    }
    Ok(Some(OverlayTextMatcher::Terms {
        terms: collected,
        // Literal search is case-insensitive by default in the engine path
        // (`case_sensitive = regex` => false for literals).
        fold: true,
    }))
}

/// Result of scanning one overlay `Upsert` file for the matcher: the bounded,
/// truncated display matches plus the count of matches suppressed by the
/// in-file Rust `#[cfg(test)]` noise filter.
struct OverlayFileScan {
    matches: Vec<super::search::TextLineMatch>,
    suppressed: usize,
}

/// Scan an overlay file's content line by line, mirroring the base
/// `collect_text_matches` semantics for an upserted dirty file:
/// * line numbers are 1-based, trailing `\r` trimmed;
/// * matches inside Rust `#[cfg(test)]`/`mod tests` ranges are suppressed (and
///   counted) under the default code-search noise policy;
/// * at most [`DEFAULT_MAX_PER_FILE`] visible match lines are RETAINED, each
///   passed through [`truncate_display_line`] so a single huge line cannot
///   detonate the result.
///
/// File-level scope/noise gating (test/generated/vendor/personal-tooling files)
/// is the CALLER's responsibility via [`current_code_search_keeps_file`]; this
/// function assumes the file already passed that gate and only applies the
/// in-file (`#[cfg(test)]`) line suppression + per-file cap + truncation.
fn scan_file_lines(file: &IndexedFile, matcher: &OverlayTextMatcher) -> OverlayFileScan {
    // Rust test-module line ranges, suppressed under the default code-search
    // noise policy (which excludes tests). Non-Rust files have none.
    let test_ranges: Vec<(u32, u32)> = if file.language == crate::domain::LanguageId::Rust {
        compute_test_ranges(file)
    } else {
        Vec::new()
    };

    let content = String::from_utf8_lossy(&file.content);
    let mut matches = Vec::new();
    let mut suppressed = 0usize;
    for (line_idx, line) in content.lines().enumerate() {
        let line = line.trim_end_matches('\r');
        if !matcher.is_match(line) {
            continue;
        }
        // Suppress (and count) matches inside Rust #[cfg(test)] modules.
        if !test_ranges.is_empty() {
            let line_num = line_idx as u32;
            if test_ranges
                .iter()
                .any(|&(start, end)| line_num >= start && line_num <= end)
            {
                suppressed += 1;
                continue;
            }
        }
        // Cap the RETAINED display matches at the base's per-file limit, while
        // still counting (above) every suppressed line. We do not need a visible
        // overflow count here because the overlay branch reports overflow as
        // "unknown across overlay" (see search_text), but bounding the retained
        // lines is required to match base output size.
        if matches.len() < DEFAULT_MAX_PER_FILE {
            matches.push(super::search::TextLineMatch {
                line_number: line_idx + 1,
                line: truncate_display_line(line),
                enclosing_symbol: None,
            });
        }
    }
    OverlayFileScan {
        matches,
        suppressed,
    }
}

/// Selects which projects in a [`WorkingSet`] a cross-project query targets
/// (FR-004 / SC-001).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Targets {
    /// A single project by id.
    One(String),
    /// An explicit subset of project ids.
    Subset(Vec<String>),
    /// Every project in the working set.
    All,
}

impl Targets {
    /// Whether `project_id` is selected by these targets.
    fn selects(&self, project_id: &str) -> bool {
        match self {
            Self::One(id) => id == project_id,
            Self::Subset(ids) => ids.iter().any(|id| id == project_id),
            Self::All => true,
        }
    }
}

/// A query hit tagged with the project it came from (FR-004 / SC-001). Generic
/// over the per-view hit payload so symbol/text/reference queries can all attach
/// source attribution uniformly.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectHit<T> {
    /// The id of the working-set project this hit came from.
    pub project_id: String,
    /// The underlying per-view hit.
    pub hit: T,
}

/// One project in a consumer's [`WorkingSet`]: a shared base + a private overlay.
///
/// The `base` is an `Arc<IndexBase>` so two entries (here or in another
/// consumer's working set) at the same `(canonical_root, commit)` share ONE
/// allocation (SC-002). The `overlay` is owned by exactly one entry and is never
/// shared (SC-003 — isolation by ownership).
#[derive(Clone)]
pub struct WorkingSetEntry {
    /// Stable per-consumer project identity (used for hit attribution + targeting).
    pub project_id: String,
    /// The shared immutable base (shared by `Arc` across consumers, SC-002).
    pub base: Arc<IndexBase>,
    /// This consumer's private copy-on-write overlay over `base`.
    pub overlay: Overlay,
}

/// A single consumer's set of open projects (data-model `WorkingSet`).
///
/// Supports add/remove/get/iter and cross-project query with source-attributed
/// results (FR-004 / SC-001). Mutations affect ONLY this consumer's working set
/// (FR-006). Two consumers' working sets that hold the same `(canonical_root,
/// commit)` base MUST be passed the SAME `Arc<IndexBase>` by the caller so the
/// base allocation is shared (SC-002); see [`WorkingSet::add`].
#[derive(Clone, Default)]
pub struct WorkingSet {
    entries: Vec<WorkingSetEntry>,
}

impl WorkingSet {
    /// An empty working set.
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Add (or replace) a project keyed by `project_id`, attaching a fresh empty
    /// overlay fenced to `base`.
    ///
    /// SC-002 CONTRACT: the CALLER owns base sharing. To share one `IndexBase`
    /// allocation across two projects (in this set or across consumers), pass a
    /// clone of the SAME `Arc<IndexBase>` (`Arc::clone`) to each `add`. This
    /// method stores the `Arc` as given; it never deep-copies the base. Adding
    /// the same `(canonical_root, commit)` via distinct `Arc`s would NOT share —
    /// that is a caller error the registry layer (Phase 3) is responsible for
    /// avoiding by interning bases by `BaseKey`. See
    /// [`WorkingSet::shares_base_with`] for an assertion helper.
    ///
    /// Re-adding an existing `project_id` replaces its entry (base + a fresh
    /// overlay), discarding the old overlay's deltas.
    pub fn add(&mut self, project_id: impl Into<String>, base: Arc<IndexBase>) {
        let project_id = project_id.into();
        let overlay = Overlay::fresh(&base);
        let entry = WorkingSetEntry {
            project_id: project_id.clone(),
            base,
            overlay,
        };
        if let Some(slot) = self.entries.iter_mut().find(|e| e.project_id == project_id) {
            *slot = entry;
        } else {
            self.entries.push(entry);
        }
    }

    /// Remove a project by id. Returns the removed entry if present.
    pub fn remove(&mut self, project_id: &str) -> Option<WorkingSetEntry> {
        let pos = self
            .entries
            .iter()
            .position(|e| e.project_id == project_id)?;
        Some(self.entries.remove(pos))
    }

    /// Borrow a project entry by id.
    pub fn get(&self, project_id: &str) -> Option<&WorkingSetEntry> {
        self.entries.iter().find(|e| e.project_id == project_id)
    }

    /// Mutably borrow a project entry by id (e.g. to record overlay deltas).
    pub fn get_mut(&mut self, project_id: &str) -> Option<&mut WorkingSetEntry> {
        self.entries.iter_mut().find(|e| e.project_id == project_id)
    }

    /// Iterate the working-set entries in insertion order.
    pub fn iter(&self) -> impl Iterator<Item = &WorkingSetEntry> {
        self.entries.iter()
    }

    /// Number of open projects.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the working set is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// SC-002 assertion helper: do two projects in THIS working set share one
    /// `IndexBase` allocation (`Arc::ptr_eq`)? `false` if either id is absent.
    /// The registry layer relies on this to verify base interning.
    pub fn shares_base_with(&self, project_a: &str, project_b: &str) -> bool {
        match (self.get(project_a), self.get(project_b)) {
            (Some(a), Some(b)) => Arc::ptr_eq(&a.base, &b.base),
            _ => false,
        }
    }

    /// Build the [`IndexView`] for one entry (base + its private overlay).
    ///
    /// Returns `Err(StaleOverlay)` if the entry's overlay is no longer fenced to
    /// its base (it must rebase first; never serves stale).
    fn view_for<'a>(entry: &'a WorkingSetEntry) -> Result<IndexView<'a>, StaleOverlay> {
        IndexView::new(&entry.base, Some(&entry.overlay))
    }

    /// Cross-project symbol search (FR-004 / SC-001): for each TARGETED entry,
    /// build its [`IndexView`], run the overlay-aware [`IndexView::search_symbols`],
    /// and tag every hit with its source `project_id`.
    ///
    /// Entries whose overlay is stale are skipped (cannot serve stale); the
    /// caller is expected to rebase before querying. Hits are returned grouped
    /// by project in working-set order (each project's hits are internally
    /// tier-sorted by the view search).
    pub fn search_symbols(
        &self,
        targets: &Targets,
        query: &str,
        kind_filter: Option<&str>,
        options: &SymbolSearchOptions,
    ) -> Vec<ProjectHit<ViewSymbolHit>> {
        let mut out = Vec::new();
        for entry in self
            .entries
            .iter()
            .filter(|e| targets.selects(&e.project_id))
        {
            let Ok(view) = Self::view_for(entry) else {
                continue;
            };
            for hit in view.search_symbols(query, kind_filter, options) {
                out.push(ProjectHit {
                    project_id: entry.project_id.clone(),
                    hit,
                });
            }
        }
        out
    }

    /// Cross-project text search (FR-004 / SC-001): per targeted entry, run the
    /// overlay-post-filtered [`IndexView::search_text`] and tag results by
    /// project. A per-project text-search error is propagated (the whole query
    /// fails) so the caller sees an honest error, not a silent partial result.
    pub fn search_text(
        &self,
        targets: &Targets,
        query: Option<&str>,
        terms: Option<&[String]>,
        regex: bool,
        options: &TextSearchOptions,
    ) -> Result<Vec<ProjectHit<TextSearchResult>>, TextSearchError> {
        let mut out = Vec::new();
        for entry in self
            .entries
            .iter()
            .filter(|e| targets.selects(&e.project_id))
        {
            let Ok(view) = Self::view_for(entry) else {
                continue;
            };
            let result = view.search_text(query, terms, regex, options)?;
            out.push(ProjectHit {
                project_id: entry.project_id.clone(),
                hit: result,
            });
        }
        Ok(out)
    }

    /// Cross-project reference search (FR-004 / SC-001): per targeted entry, run
    /// the overlay-post-filtered [`IndexView::find_references`] and tag each
    /// `(path, ReferenceRecord)` hit by project.
    pub fn find_references(
        &self,
        targets: &Targets,
        name: &str,
        kind_filter: Option<ReferenceKind>,
        include_filtered: bool,
    ) -> Vec<ProjectHit<(String, ReferenceRecord)>> {
        let mut out = Vec::new();
        for entry in self
            .entries
            .iter()
            .filter(|e| targets.selects(&e.project_id))
        {
            let Ok(view) = Self::view_for(entry) else {
                continue;
            };
            for (path, reference) in view.find_references(name, kind_filter, include_filtered) {
                out.push(ProjectHit {
                    project_id: entry.project_id.clone(),
                    hit: (path, reference.clone()),
                });
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{FileClassification, LanguageId};
    use crate::live_index::store::{IndexedFile, LiveIndex, ParseStatus};
    use std::time::Instant;

    /// Build a minimal synthetic `IndexedFile` for spike tests. All fields are
    /// `pub`; content is the load-bearing bit we assert resolution on.
    fn make_file(path: &str, content: &str) -> IndexedFile {
        IndexedFile {
            relative_path: path.to_string(),
            language: LanguageId::Rust,
            classification: FileClassification::for_code_path(path),
            content: content.as_bytes().to_vec(),
            symbols: Vec::new(),
            parse_status: ParseStatus::Parsed,
            parse_diagnostic: None,
            byte_len: content.len() as u64,
            content_hash: format!("h:{}", content.len()),
            references: Vec::new(),
            alias_map: HashMap::new(),
            mtime_secs: 0,
        }
    }

    /// Build a synthetic base `LiveIndex` of `n` files: `f{0}.rs .. f{n-1}.rs`.
    /// Uses sibling (`super`) access to `empty_live_index()` + the `files` map —
    /// engine-internal, no server/network deps (compiles under `embed`).
    fn synthetic_index(n: usize) -> LiveIndex {
        let mut idx = LiveIndex::empty_live_index();
        idx.is_empty = false;
        idx.loaded_at = Instant::now();
        let mut files: HashMap<String, Arc<IndexedFile>> = HashMap::with_capacity(n);
        for i in 0..n {
            let path = format!("f{i}.rs");
            let file = make_file(&path, &format!("fn f{i}() {{}}"));
            files.insert(path, Arc::new(file));
        }
        idx.files = files;
        idx
    }

    fn base_key(commit: &str) -> BaseKey {
        BaseKey::new("/repo/root", CommitId::Sha(commit.to_string()))
    }

    // ── Proof 1 — SC-002 shared base, no second files-map clone ──────────────
    #[test]
    fn proof1_shared_base_no_clone() {
        let live = synthetic_index(100);
        // Snapshot the strong_count of an inner file Arc BEFORE wrapping.
        let probe = Arc::clone(live.files.get("f0.rs").expect("f0"));
        let inner_count_baseline = Arc::strong_count(&probe);

        let base = Arc::new(IndexBase::new(base_key("c0"), Arc::new(live), 1));

        // Consumer #1 attaches an overlay.
        let overlay_1 = Overlay::fresh(&base);
        // Consumer #2 attaches an overlay over the SAME Arc<IndexBase>.
        let base_2 = Arc::clone(&base);
        let overlay_2 = Overlay::fresh(&base_2);

        // SC-002: the two bases are the SAME allocation.
        assert!(
            Arc::ptr_eq(&base, &base_2),
            "consumers must share one IndexBase allocation"
        );
        // The shared LiveIndex is also the same allocation.
        assert!(Arc::ptr_eq(&base.index, &base_2.index));

        // Building views for both consumers must NOT clone the base files map.
        let _view_1 = IndexView::new(&base, Some(&overlay_1)).expect("v1");
        let _view_2 = IndexView::new(&base_2, Some(&overlay_2)).expect("v2");

        // The inner per-file Arc strong_count must NOT have doubled by adding a
        // second consumer: no second `files` map was cloned (only `&LiveIndex`
        // borrows). The only extra ref to `probe` is `probe` itself.
        let inner_count_after = Arc::strong_count(&probe);
        assert_eq!(
            inner_count_after, inner_count_baseline,
            "adding consumer #2 must not clone the base files map \
             (inner file Arc strong_count must not increase): {inner_count_baseline} -> {inner_count_after}"
        );
    }

    // ── Proof 3 — no forced reload (ArcSwap immutability) ────────────────────
    #[test]
    fn proof3_no_forced_reload() {
        let live = synthetic_index(3);
        let base_v1 = Arc::new(IndexBase::new(base_key("c0"), Arc::new(live), 1));

        // Consumer B holds a view over the v1 snapshot.
        let overlay_b = Overlay::fresh(&base_v1);
        let view_b = IndexView::new(&base_v1, Some(&overlay_b)).expect("B view v1");
        assert_eq!(view_b.get_file("f0.rs").unwrap().content, b"fn f0() {}");

        // Consumer A reindexes -> a NEW IndexBase (new generation) with mutated
        // content. The old Arc<LiveIndex> is immutable and untouched.
        let mut live_v2 = synthetic_index(3);
        // Sibling access: mutate the v2 file map to simulate A's reindex.
        live_v2.files.insert(
            "f0.rs".to_string(),
            Arc::new(make_file("f0.rs", "REINDEXED")),
        );
        let _base_v2 = Arc::new(IndexBase::new(base_key("c1"), Arc::new(live_v2), 2));

        // B's existing view over the PRIOR snapshot still reads the old content:
        // ArcSwap immutability — no forced reload, no interruption.
        assert_eq!(
            view_b.get_file("f0.rs").unwrap().content,
            b"fn f0() {}",
            "B's view over the prior snapshot must be unaffected by A's reindex"
        );
    }

    // ── Proof 4 — stale fence ────────────────────────────────────────────────
    #[test]
    fn proof4_stale_fence() {
        let live = synthetic_index(2);
        let base_v1 = IndexBase::new(base_key("c0"), Arc::new(live), 1);

        // Overlay derived against generation 1.
        let overlay = Overlay::fresh(&base_v1);
        assert!(overlay.is_valid_against(&base_v1));

        // Base advances to generation 2 (new commit) -> SAME index payload but
        // bumped generation. The overlay is now stale.
        let base_v2 = IndexBase::new(
            base_key("c1"),
            Arc::clone(&base_v1.index),
            base_v1.base_generation + 1,
        );

        // IndexView is not Debug (it borrows &LiveIndex), so match explicitly
        // rather than unwrap_err().
        match IndexView::new(&base_v2, Some(&overlay)) {
            Err(err) => assert_eq!(
                err,
                StaleOverlay {
                    base_generation: 2,
                    overlay_generation: 1
                },
                "reading a gen-1 overlay against a gen-2 base must be StaleOverlay"
            ),
            Ok(_) => panic!("expected StaleOverlay, got a valid view"),
        }
        assert!(!overlay.is_valid_against(&base_v2));
    }

    // ─────────────────────────────────────────────────────────────────────────
    // WorkingSet + cross-project query (Phase 2)
    // ─────────────────────────────────────────────────────────────────────────

    use crate::domain::{SymbolKind, SymbolRecord};

    /// Build a synthetic indexed file carrying ONE symbol named `symbol_name`
    /// (so symbol search has something to match) and the given content.
    fn make_file_with_symbol(path: &str, symbol_name: &str, content: &str) -> IndexedFile {
        let mut file = make_file(path, content);
        file.symbols = vec![SymbolRecord {
            name: symbol_name.to_string(),
            kind: SymbolKind::Function,
            depth: 0,
            sort_order: 0,
            byte_range: (0, content.len() as u32),
            line_range: (0, 0),
            doc_byte_range: None,
            item_byte_range: None,
        }];
        file
    }

    /// Build a base `IndexBase` whose single file `f.rs` defines a function
    /// `symbol_name` and contains `content` (used for cross-project search).
    fn base_with_symbol(
        root: &str,
        commit: &str,
        symbol_name: &str,
        content: &str,
    ) -> Arc<IndexBase> {
        let mut idx = LiveIndex::empty_live_index();
        idx.is_empty = false;
        idx.loaded_at = Instant::now();
        let mut files: HashMap<String, Arc<IndexedFile>> = HashMap::new();
        files.insert(
            "f.rs".to_string(),
            Arc::new(make_file_with_symbol("f.rs", symbol_name, content)),
        );
        idx.trigram_index = crate::live_index::trigram::TrigramIndex::build_from_files(&files);
        idx.files = files;
        idx.rebuild_reverse_index();
        Arc::new(IndexBase::new(
            BaseKey::new(root, CommitId::Sha(commit.to_string())),
            Arc::new(idx),
            1,
        ))
    }

    #[test]
    fn working_set_add_remove_get() {
        let mut ws = WorkingSet::new();
        assert!(ws.is_empty());

        let base_a = base_with_symbol("/a", "c0", "alpha_fn", "fn alpha_fn() {}");
        let base_b = base_with_symbol("/b", "c0", "beta_fn", "fn beta_fn() {}");
        ws.add("A", Arc::clone(&base_a));
        ws.add("B", Arc::clone(&base_b));

        assert_eq!(ws.len(), 2);
        assert!(ws.get("A").is_some());
        assert!(ws.get("B").is_some());
        assert!(ws.get("missing").is_none());

        // Remove returns the entry; get then misses.
        let removed = ws.remove("A").expect("A present");
        assert_eq!(removed.project_id, "A");
        assert!(ws.get("A").is_none());
        assert_eq!(ws.len(), 1);

        // Removing an absent id is a clean None.
        assert!(ws.remove("A").is_none());

        // iter yields the surviving entry.
        let ids: Vec<&str> = ws.iter().map(|e| e.project_id.as_str()).collect();
        assert_eq!(ids, vec!["B"]);
    }

    // ── SC-001: cross-project search returns source-attributed hits ───────────
    #[test]
    fn cross_project_search_is_source_attributed() {
        let mut ws = WorkingSet::new();
        // Three projects, each defining a function whose name contains "shared".
        ws.add(
            "A",
            base_with_symbol("/a", "c0", "shared_a", "fn shared_a() {}"),
        );
        ws.add(
            "B",
            base_with_symbol("/b", "c0", "shared_b", "fn shared_b() {}"),
        );
        ws.add(
            "C",
            base_with_symbol("/c", "c0", "shared_c", "fn shared_c() {}"),
        );

        let hits = ws.search_symbols(
            &Targets::All,
            "shared",
            None,
            &SymbolSearchOptions::for_current_code_search(usize::MAX),
        );
        assert_eq!(hits.len(), 3, "one hit per project");

        // Every hit is attributed to its source project, and the project's own
        // symbol is the one returned (no cross-leak).
        let mut by_project: HashMap<&str, &str> = HashMap::new();
        for h in &hits {
            by_project.insert(h.project_id.as_str(), h.hit.name.as_str());
        }
        assert_eq!(by_project.get("A"), Some(&"shared_a"));
        assert_eq!(by_project.get("B"), Some(&"shared_b"));
        assert_eq!(by_project.get("C"), Some(&"shared_c"));
    }

    // ── subset / single targeting ─────────────────────────────────────────────
    #[test]
    fn cross_project_search_respects_targets() {
        let mut ws = WorkingSet::new();
        ws.add(
            "A",
            base_with_symbol("/a", "c0", "shared_a", "fn shared_a() {}"),
        );
        ws.add(
            "B",
            base_with_symbol("/b", "c0", "shared_b", "fn shared_b() {}"),
        );
        ws.add(
            "C",
            base_with_symbol("/c", "c0", "shared_c", "fn shared_c() {}"),
        );

        // Single.
        let one = ws.search_symbols(
            &Targets::One("B".to_string()),
            "shared",
            None,
            &SymbolSearchOptions::for_current_code_search(usize::MAX),
        );
        assert_eq!(one.len(), 1);
        assert_eq!(one[0].project_id, "B");
        assert_eq!(one[0].hit.name, "shared_b");

        // Subset.
        let subset = ws.search_symbols(
            &Targets::Subset(vec!["A".to_string(), "C".to_string()]),
            "shared",
            None,
            &SymbolSearchOptions::for_current_code_search(usize::MAX),
        );
        let projects: std::collections::BTreeSet<&str> =
            subset.iter().map(|h| h.project_id.as_str()).collect();
        assert_eq!(
            projects,
            ["A", "C"]
                .into_iter()
                .collect::<std::collections::BTreeSet<_>>()
        );

        // A target naming a project not in the set yields no hits (the caller
        // layer turns "project not open" into an explicit error; the primitive
        // simply does not match it).
        let none = ws.search_symbols(
            &Targets::One("Z".to_string()),
            "shared",
            None,
            &SymbolSearchOptions::for_current_code_search(usize::MAX),
        );
        assert!(none.is_empty());
    }

    // ── SC-002: two entries on the same base share ONE Arc (ptr_eq) ───────────
    #[test]
    fn working_set_shares_base_arc() {
        let mut ws = WorkingSet::new();
        // Caller shares the SAME Arc<IndexBase> for two projects (e.g. two
        // logical views of one canonical_root+commit).
        let shared = base_with_symbol("/repo", "c0", "f", "fn f() {}");
        ws.add("view1", Arc::clone(&shared));
        ws.add("view2", Arc::clone(&shared));

        let a = ws.get("view1").unwrap();
        let b = ws.get("view2").unwrap();
        assert!(
            Arc::ptr_eq(&a.base, &b.base),
            "two entries on the same shared Arc<IndexBase> must be one allocation (SC-002)"
        );
        assert!(Arc::ptr_eq(&a.base.index, &b.base.index));
        // The helper agrees.
        assert!(ws.shares_base_with("view1", "view2"));

        // A distinct base for a different project does NOT share.
        ws.add("other", base_with_symbol("/other", "c0", "g", "fn g() {}"));
        assert!(!ws.shares_base_with("view1", "other"));
    }

    // ── cross-project text query is source-attributed ────────────────────────
    #[test]
    fn cross_project_text_search_attribution() {
        let mut ws = WorkingSet::new();
        ws.add("A", base_with_symbol("/a", "c0", "f", "let NEEDLE = 1;"));
        ws.add("B", base_with_symbol("/b", "c0", "g", "let other = 2;"));
        ws.add("C", base_with_symbol("/c", "c0", "h", "let NEEDLE = 3;"));

        let results = ws
            .search_text(
                &Targets::All,
                Some("NEEDLE"),
                None,
                false,
                &TextSearchOptions::for_current_code_search(),
            )
            .expect("cross-project text search");
        // Only projects whose content has NEEDLE produce non-empty results, but
        // every targeted project yields a (possibly empty) attributed result.
        let with_matches: Vec<&str> = results
            .iter()
            .filter(|r| r.hit.total_matches > 0)
            .map(|r| r.project_id.as_str())
            .collect();
        assert_eq!(with_matches, vec!["A", "C"]);
    }

    // ── stale-overlay entry is skipped by the working-set query ───────────────
    #[test]
    fn working_set_skips_stale_overlay_entry() {
        let mut ws = WorkingSet::new();
        ws.add(
            "A",
            base_with_symbol("/a", "c0", "shared_a", "fn shared_a() {}"),
        );
        // Forcibly desync A's overlay fence (simulating a base advance without a
        // rebase). The query must SKIP it (never serve stale) rather than panic.
        ws.get_mut("A").unwrap().overlay.base_generation = 999;

        let hits = ws.search_symbols(
            &Targets::All,
            "shared",
            None,
            &SymbolSearchOptions::for_current_code_search(usize::MAX),
        );
        assert!(
            hits.is_empty(),
            "a stale-overlay entry must be skipped, not served"
        );
    }

    /// Build a base from explicit (path, language, symbol_name) tuples, each file
    /// defining one function named `symbol_name`. Used to prove the cross-project
    /// search HONORS caller options (path scope, language, result_limit) on the
    /// empty-overlay path (B1 / D11 + D14).
    fn base_with_files(
        root: &str,
        commit: &str,
        files: &[(&str, LanguageId, &str)],
    ) -> Arc<IndexBase> {
        let mut idx = LiveIndex::empty_live_index();
        idx.is_empty = false;
        idx.loaded_at = Instant::now();
        let mut map: HashMap<String, Arc<IndexedFile>> = HashMap::new();
        for (path, lang, sym) in files {
            let mut file = make_file_with_symbol(path, sym, &format!("fn {sym}() {{}}"));
            file.language = lang.clone();
            map.insert((*path).to_string(), Arc::new(file));
        }
        idx.trigram_index = crate::live_index::trigram::TrigramIndex::build_from_files(&map);
        idx.files = map;
        idx.rebuild_reverse_index();
        Arc::new(IndexBase::new(
            BaseKey::new(root, CommitId::Sha(commit.to_string())),
            Arc::new(idx),
            1,
        ))
    }

    // ── B1: cross-project search HONORS caller scoping + limit (D11 + D14) ─────
    // These options were previously LOUDLY REFUSED at the daemon guard; now they
    // are threaded through the engine's option-honoring search.
    #[test]
    fn cross_project_search_honors_scoping_options() {
        use crate::live_index::search::PathScope;

        let mut ws = WorkingSet::new();
        // One project; three files all defining `widget`: two Rust (src/, other/)
        // and one Python under src/.
        ws.add(
            "A",
            base_with_files(
                "/a",
                "c0",
                &[
                    ("src/keep.rs", LanguageId::Rust, "widget"),
                    ("other/skip.rs", LanguageId::Rust, "widget"),
                    ("src/also.py", LanguageId::Python, "widget"),
                ],
            ),
        );

        // path_scope = src/ -> the other/ file is EXCLUDED (D11 path scoping).
        let mut scoped = SymbolSearchOptions::for_current_code_search(usize::MAX);
        scoped.path_scope = PathScope::prefix("src");
        let hits = ws.search_symbols(&Targets::All, "widget", None, &scoped);
        let paths: std::collections::BTreeSet<&str> =
            hits.iter().map(|h| h.hit.path.as_str()).collect();
        assert!(
            paths.contains("src/keep.rs") && paths.contains("src/also.py"),
            "src/-scoped search must include the src/ hits: {paths:?}"
        );
        assert!(
            !paths.contains("other/skip.rs"),
            "src/-scoped search must EXCLUDE other/ hits (path scoping honored): {paths:?}"
        );

        // language = Rust -> the Python file is EXCLUDED (D11 language scoping).
        let mut rust_only = SymbolSearchOptions::for_current_code_search(usize::MAX);
        rust_only.language_filter = Some(LanguageId::Rust);
        let rust_hits = ws.search_symbols(&Targets::All, "widget", None, &rust_only);
        let rust_paths: std::collections::BTreeSet<&str> =
            rust_hits.iter().map(|h| h.hit.path.as_str()).collect();
        assert!(
            rust_paths.contains("src/keep.rs") && rust_paths.contains("other/skip.rs"),
            "Rust-filtered search keeps the Rust files: {rust_paths:?}"
        );
        assert!(
            !rust_paths.contains("src/also.py"),
            "Rust-filtered search must EXCLUDE the Python file (language scoping honored): {rust_paths:?}"
        );

        // result_limit = 1 -> the per-project hits are bounded + tier-ranked, not
        // an unbounded usize::MAX dump (D14).
        let bounded = SymbolSearchOptions::for_current_code_search(1);
        let bounded_hits = ws.search_symbols(&Targets::All, "widget", None, &bounded);
        assert_eq!(
            bounded_hits.len(),
            1,
            "result_limit=1 bounds the per-project hits (D14): got {}",
            bounded_hits.len()
        );
    }
}
