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
    SymbolMatchTier, TextSearchError, TextSearchResult, search_symbols as base_search_symbols,
    search_text as base_search_text,
};
use super::store::{IndexedFile, LiveIndex};

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
    pub index: Arc<LiveIndex>,
    /// Monotonic fence token; bumped only when a new commit publishes a new base.
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

    /// Record an upsert delta for `rel_path`.
    pub fn upsert(&mut self, rel_path: impl Into<String>, file: Arc<IndexedFile>) {
        self.deltas.insert(rel_path.into(), FileDelta::Upsert(file));
    }

    /// Record a tombstone (deletion) delta for `rel_path`.
    pub fn tombstone(&mut self, rel_path: impl Into<String>) {
        self.deltas.insert(rel_path.into(), FileDelta::Tombstone);
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

    /// Rebase this overlay onto a NEW base after a commit advance (D2 trigger
    /// (a) / data-model state transition `dirty --base commit advances-->`).
    ///
    /// `still_dirty` is the recomputed dirty set from `uncommitted_paths()`
    /// (`git.rs:68`) — the set of paths that remain uncommitted against the new
    /// base. Deltas whose path is no longer in `still_dirty` were absorbed by
    /// the new base (committed) and are dropped; the rest are kept and re-fenced
    /// to the new base.
    ///
    /// COST: O(K) where K = `self.deltas.len()`. It iterates only the overlay's
    /// own deltas and probes the (typically small) `still_dirty` set. It does
    /// NOT scan the base's N-file map. This is the spike's load-bearing claim
    /// (the named CoW perf cliff falsifier).
    pub fn rebase(
        &mut self,
        new_base: &IndexBase,
        still_dirty: &std::collections::HashSet<String>,
    ) {
        // Drop deltas absorbed by the new base (no longer uncommitted). Retain
        // touches each delta exactly once -> O(K), independent of base size N.
        self.deltas.retain(|path, _| still_dirty.contains(path));
        // Re-fence to the new base.
        self.base_key = new_base.key.clone();
        self.base_generation = new_base.base_generation;
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
        if let Some(ov) = self.overlay {
            if let Some(delta) = ov.deltas.get(relative_path) {
                return match delta {
                    FileDelta::Upsert(file) => Some(file.as_ref()),
                    FileDelta::Tombstone => None,
                };
            }
        }
        self.base.get_file(relative_path)
    }

    /// Iterate all resolved `(path, file)` pairs: base files not shadowed by an
    /// overlay delta, plus overlay upserts; tombstoned base files are excluded.
    ///
    /// Allocates a result `Vec` because resolution merges two maps; the spike's
    /// hot-path claim is about [`Overlay::rebase`], not this convenience
    /// iterator. The single-consumer path (`overlay: None`) returns base files
    /// directly with no overlay work.
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
    /// For a base-only view this delegates straight to the engine's
    /// [`base_search_symbols`] over the base index (byte-for-byte today's
    /// behavior). When an overlay is present we cannot call the base function
    /// (it would scan the stale base file map), so we scan the OVERLAY-RESOLVED
    /// file set ([`all_files`]) directly with the same name-match + tiering rules
    /// the engine search uses, then sort and bound identically. Correctness over
    /// speed: this is a linear scan of the resolved file set.
    ///
    /// `// DEFERRED (012 full tier):` the engine's richer
    /// `search_symbols_with_options` (noise policy, test-module suppression,
    /// path scope, language filter) is intentionally NOT reproduced here — the
    /// overlay path applies only the name/kind tiering needed for cross-project
    /// attribution. Wiring the full option surface through the overlay is the
    /// deferred derived-index work; until then the overlay symbol search is a
    /// minimal, correct superset (it may include hits the option-filtered base
    /// path would suppress).
    ///
    /// [`all_files`]: IndexView::all_files
    pub fn search_symbols(&self, query: &str, kind_filter: Option<&str>) -> Vec<ViewSymbolHit> {
        // Fast path: no overlay -> reuse the engine search verbatim over the base.
        if self.overlay.is_none() {
            let result = base_search_symbols(self.base, query, kind_filter, usize::MAX);
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
    /// 1. run the engine [`base_search_text`] against the immutable base index;
    /// 2. DROP every base file-result whose path is a live overlay delta — its
    ///    base content is stale (the consumer edited or deleted that file);
    /// 3. directly SCAN each overlay `Upsert`'s current content for the same
    ///    query and append fresh matches for it.
    ///
    /// The net result reflects the consumer's overlay: a dirty edit's stale base
    /// hit is never returned, and the upserted file's real hits are. Cost is
    /// O(base text search) + O(dirty set scan), never an overlay-wide reindex.
    ///
    /// `// DEFERRED (012 full tier):` a per-overlay trigram index would let the
    /// dirty-file scan use the same accelerated path as the base. Until then the
    /// dirty files are scanned linearly (correct, bounded by the small dirty
    /// set). DOCUMENTED LIMITATION: after the post-filter, `total_matches` is
    /// RECOMPUTED as the sum of the returned files' displayed match lines (each
    /// bounded by the engine's `max_per_file`), so it is coherent with `files`
    /// but is NOT the base's pre-cap visible total. `overflow_count` and
    /// `suppressed_by_noise` are carried through from the base search unchanged
    /// (base-accurate, not re-derived across the overlay) — honest, not globally
    /// re-ranked. Per-consumer derived indices that would make these globally
    /// exact are the deferred work.
    pub fn search_text(
        &self,
        query: Option<&str>,
        terms: Option<&[String]>,
        regex: bool,
    ) -> Result<TextSearchResult, TextSearchError> {
        let mut result = base_search_text(self.base, query, terms, regex)?;

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
        let matcher = overlay_text_matcher(query, terms, regex)?;
        if let Some(matcher) = matcher {
            for (path, delta) in deltas {
                if let FileDelta::Upsert(file) = delta {
                    let line_matches = scan_file_lines(file, &matcher);
                    if !line_matches.is_empty() {
                        result.files.push(super::search::TextFileMatches {
                            path: path.clone(),
                            matches: line_matches,
                            rendered_lines: None,
                            callers: None,
                        });
                    }
                }
            }
        }

        // Recompute `total_matches` to stay COHERENT with the post-filtered
        // `files` set: dropping a base file-result must subtract its matches, and
        // appended overlay scans must add theirs. We recount from the retained +
        // appended `matches` lists rather than incrementally patching the base
        // counter, which would leave it stale after the retain above. NOTE: this
        // makes `total_matches` reflect the displayed match lines (each file's
        // `matches` is already bounded by the engine's `max_per_file`); it is an
        // honest count of the merged result, not the base's pre-cap visible
        // total. See the DOCUMENTED LIMITATION on this method.
        result.total_matches = result.files.iter().map(|f| f.matches.len()).sum();

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
    /// `// DEFERRED (012 full tier):` a per-overlay reverse index would replace
    /// the linear `references` scan of the dirty files. Until then the dirty set
    /// is scanned directly (correct, O(dirty set)). DOCUMENTED LIMITATION: the
    /// overlay scan resolves only the SIMPLE-name match against each upserted
    /// file's `references` (and qualified-name equality); it does not reproduce
    /// the base's alias-map resolution across overlay files. Alias resolution
    /// across dirty files is part of the deferred derived-index work.
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
        let is_qualified = name.contains("::") || name.contains('.');
        for (path, delta) in deltas {
            if let FileDelta::Upsert(file) = delta {
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
                    out.push((path.clone(), reference));
                }
            }
        }

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

/// Scan an overlay file's content line by line, returning [`TextLineMatch`]es
/// for the matcher. Line numbers are 1-based, trailing `\r` trimmed — matching
/// the engine's `collect_text_matches` line handling.
fn scan_file_lines(
    file: &IndexedFile,
    matcher: &OverlayTextMatcher,
) -> Vec<super::search::TextLineMatch> {
    let content = String::from_utf8_lossy(&file.content);
    let mut matches = Vec::new();
    for (line_idx, line) in content.lines().enumerate() {
        let line = line.trim_end_matches('\r');
        if matcher.is_match(line) {
            matches.push(super::search::TextLineMatch {
                line_number: line_idx + 1,
                line: line.to_string(),
                enclosing_symbol: None,
            });
        }
    }
    matches
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
            for hit in view.search_symbols(query, kind_filter) {
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
            let result = view.search_text(query, terms, regex)?;
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
    use std::collections::HashSet;
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

    // ── Proof 2 — SC-003 isolation ──────────────────────────────────────────
    #[test]
    fn proof2_overlay_isolation() {
        let live = synthetic_index(3); // f0.rs, f1.rs, f2.rs
        let base = Arc::new(IndexBase::new(base_key("c0"), Arc::new(live), 1));

        // Consumer A upserts "foo" (an edited f0.rs) and tombstones f1.rs.
        let mut overlay_a = Overlay::fresh(&base);
        overlay_a.upsert("f0.rs", Arc::new(make_file("f0.rs", "A_EDITED")));
        overlay_a.tombstone("f1.rs");

        // Consumer B has a fresh (empty) overlay over the SAME base.
        let overlay_b = Overlay::fresh(&base);

        let view_a = IndexView::new(&base, Some(&overlay_a)).expect("A view");
        let view_b = IndexView::new(&base, Some(&overlay_b)).expect("B view");

        // A sees its own overlay content.
        assert_eq!(view_a.get_file("f0.rs").unwrap().content, b"A_EDITED");
        // A's tombstone hides f1.rs from A.
        assert!(view_a.get_file("f1.rs").is_none(), "tombstone hides for A");

        // B sees base content for f0.rs (A's upsert is INVISIBLE to B).
        assert_eq!(view_b.get_file("f0.rs").unwrap().content, b"fn f0() {}");
        // B still sees f1.rs (A's tombstone is INVISIBLE to B).
        assert!(
            view_b.get_file("f1.rs").is_some(),
            "A's tombstone must not affect B"
        );
        assert_eq!(view_b.get_file("f1.rs").unwrap().content, b"fn f1() {}");
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

    // ── Proof 5 — THE FALSIFIER: rebase is O(K), independent of N ─────────────
    //
    // Build a base of N files, hold an overlay of K dirty files, advance the
    // base (new generation), time the rebase. PASS iff rebase time tracks K and
    // is independent of N. We assert it directly: the rebase touches only the K
    // deltas (and the still_dirty probe set), never the N base files.
    //
    // METHODOLOGY: All base/index construction (the only N-sized allocation in
    // the system) happens OUTSIDE the timed region. The timer wraps a tight loop
    // of `ITERS` rebases over pre-built, reused inputs and divides by `ITERS`,
    // so we measure steady-state `Overlay::rebase` cost free of adjacent
    // large-allocation cache/allocator noise (the artifact that contaminated a
    // naive build-then-time-once loop). The base is held by `Arc` and merely
    // borrowed by `rebase` — it is never cloned or scanned.
    #[test]
    fn proof5_falsifier_rebase_is_ok_not_on() {
        let ns = [1_000usize, 10_000usize];
        let ks = [1usize, 8usize, 64usize];

        let mut table: Vec<(usize, usize, f64)> = Vec::new();
        for &n in &ns {
            for &k in &ks {
                let per_rebase_ns = bench_rebase_steady_state(n, k);
                table.push((n, k, per_rebase_ns));
            }
        }

        println!("\n=== Proof 5 falsifier: steady-state overlay rebase cost ===");
        println!("{:>8} {:>6} {:>16}", "N", "K", "rebase (ns/op)");
        for &(n, k, t) in &table {
            println!("{n:>8} {k:>6} {t:>16.1}");
        }

        // ASSERTION 1 — N-INDEPENDENCE: for a fixed K, rebase time must NOT scale
        // with N. An O(N) whole-repo rescan from N=1k to N=10k (10x) would blow
        // up ~10x. We require t(10k)/t(1k) < 2.5 (generous slack for sub-us timer
        // noise on a warm steady-state measurement). O(N) fails this hard (~10x).
        for &k in &ks {
            let t_1k = lookup(&table, 1_000, k).max(0.1);
            let t_10k = lookup(&table, 10_000, k);
            let ratio = t_10k / t_1k;
            println!("K={k}: t(10k)/t(1k) = {ratio:.2} (PASS if < 2.5 -> independent of N)");
            assert!(
                ratio < 2.5,
                "FALSIFIER TRIPPED: rebase scaled with N for K={k} \
                 (t_1k={t_1k:.1}ns, t_10k={t_10k:.1}ns, ratio={ratio:.2}); \
                 rebase is NOT O(K) -> revisit the primitive design"
            );
        }

        // ASSERTION 2 — O(K) SHAPE: for a fixed N, rebase cost grows with K (it is
        // work proportional to the dirty set). Going K=1 -> K=64 (64x) must
        // increase cost, but stay near-linear in K, not super-linear. We assert
        // t(K=64) > t(K=1) (real per-delta work) and t(K=64)/t(K=1) < 200
        // (linear-ish; an O(K*N) or worse would explode). This confirms the cost
        // IS in K, which is the whole point.
        for &n in &ns {
            let t_k1 = lookup(&table, n, 1).max(0.1);
            let t_k64 = lookup(&table, n, 64);
            let ratio = t_k64 / t_k1;
            println!("N={n}: t(K=64)/t(K=1) = {ratio:.2} (cost lives in K, linear-ish)");
            assert!(
                t_k64 > t_k1,
                "rebase cost should grow with the dirty set K (N={n}): \
                 t(K=1)={t_k1:.1}ns, t(K=64)={t_k64:.1}ns"
            );
            assert!(
                ratio < 200.0,
                "rebase cost should be near-linear in K, not super-linear (N={n}): \
                 t(K=1)={t_k1:.1}ns, t(K=64)={t_k64:.1}ns, ratio={ratio:.2}"
            );
        }

        // Correctness of the rebase semantics (drop absorbed, keep dirty, re-fence).
        assert_rebase_semantics();
    }

    fn lookup(table: &[(usize, usize, f64)], n: usize, k: usize) -> f64 {
        table
            .iter()
            .find(|&&(nn, kk, _)| nn == n && kk == k)
            .map(|&(_, _, t)| t)
            .expect("table cell")
    }

    /// Measure steady-state per-rebase cost for a base of N files with a K-dirty
    /// overlay. Construction is OUTSIDE the timer; the timed region is a tight
    /// loop of pure `rebase` calls over reused, pre-built inputs.
    fn bench_rebase_steady_state(n: usize, k: usize) -> f64 {
        use std::time::Instant as TInstant;

        // ── build inputs ONCE, outside any timing ──
        let base_v1 = IndexBase::new(base_key("c0"), Arc::new(synthetic_index(n)), 1);
        let base_v2 = IndexBase::new(base_key("c1"), Arc::new(synthetic_index(n)), 2);

        // A pristine overlay with K dirty upserts; cloned per iteration so each
        // rebase starts from the same K-sized state (clone of a K-entry map is
        // itself O(K), but is done OUTSIDE the timed region per iteration).
        let mut pristine = Overlay::fresh(&base_v1);
        for i in 0..k {
            pristine.upsert(
                format!("f{i}.rs"),
                Arc::new(make_file(&format!("f{i}.rs"), "DIRTY")),
            );
        }
        // still_dirty keeps the even-indexed half (absorbed: odd indices).
        let still_dirty: HashSet<String> = (0..k)
            .filter(|i| i % 2 == 0)
            .map(|i| format!("f{i}.rs"))
            .collect();

        const ITERS: usize = 2_000;

        // Warm-up (touch the code path, prime caches) — untimed.
        {
            let mut o = pristine.clone();
            o.rebase(&base_v2, &still_dirty);
            std::hint::black_box(&o);
        }

        // Pre-build the per-iteration overlays OUTSIDE the timer so the timed
        // region contains ONLY rebase work (no clone, no allocation of N).
        let mut overlays: Vec<Overlay> = (0..ITERS).map(|_| pristine.clone()).collect();

        let start = TInstant::now();
        for o in overlays.iter_mut() {
            o.rebase(&base_v2, &still_dirty);
            std::hint::black_box(&*o);
        }
        let total = start.elapsed().as_nanos() as f64;
        std::hint::black_box(&overlays);

        // Sanity on the last one.
        debug_assert!(overlays[ITERS - 1].is_valid_against(&base_v2));
        debug_assert_eq!(overlays[ITERS - 1].delta_count(), still_dirty.len());

        total / ITERS as f64
    }

    /// Verify rebase drops absorbed deltas, keeps still-dirty, and re-fences.
    fn assert_rebase_semantics() {
        let live = synthetic_index(10);
        let base_v1 = IndexBase::new(base_key("c0"), Arc::new(live), 1);
        let mut overlay = Overlay::fresh(&base_v1);
        overlay.upsert("f0.rs", Arc::new(make_file("f0.rs", "DIRTY0")));
        overlay.upsert("f1.rs", Arc::new(make_file("f1.rs", "DIRTY1")));
        overlay.tombstone("f2.rs");
        assert_eq!(overlay.delta_count(), 3);

        let live_v2 = synthetic_index(10);
        let base_v2 = IndexBase::new(base_key("c1"), Arc::new(live_v2), 2);
        // f1.rs got committed (absorbed); f0.rs and f2.rs remain dirty.
        let still_dirty: HashSet<String> = ["f0.rs".to_string(), "f2.rs".to_string()]
            .into_iter()
            .collect();
        overlay.rebase(&base_v2, &still_dirty);

        assert_eq!(
            overlay.delta_count(),
            2,
            "absorbed delta f1.rs must be dropped"
        );
        assert!(overlay.deltas.contains_key("f0.rs"));
        assert!(overlay.deltas.contains_key("f2.rs"));
        assert!(!overlay.deltas.contains_key("f1.rs"));
        assert!(overlay.is_valid_against(&base_v2), "re-fenced to new base");

        // The rebased overlay reads correctly through a view.
        let view = IndexView::new(&base_v2, Some(&overlay)).expect("view");
        assert_eq!(view.get_file("f0.rs").unwrap().content, b"DIRTY0");
        assert!(
            view.get_file("f2.rs").is_none(),
            "tombstone survives rebase"
        );
        // f1.rs now reads the base (committed content), overlay no longer shadows.
        assert_eq!(view.get_file("f1.rs").unwrap().content, b"fn f1() {}");
    }

    // ── all_files resolution sanity (overlay merge) ──────────────────────────
    #[test]
    fn all_files_merges_overlay() {
        let live = synthetic_index(3); // f0,f1,f2
        let base = IndexBase::new(base_key("c0"), Arc::new(live), 1);
        let mut overlay = Overlay::fresh(&base);
        overlay.upsert("f0.rs", Arc::new(make_file("f0.rs", "EDIT")));
        overlay.tombstone("f1.rs");
        overlay.upsert("new.rs", Arc::new(make_file("new.rs", "NEW")));

        let view = IndexView::new(&base, Some(&overlay)).expect("view");
        let mut paths: Vec<String> = view.all_files().into_iter().map(|(p, _)| p).collect();
        paths.sort();
        // f1.rs tombstoned out; new.rs added; f0.rs/f2.rs present.
        assert_eq!(paths, vec!["f0.rs", "f2.rs", "new.rs"]);

        // base_only view == today's behavior.
        let base_view = IndexView::base_only(base.index.as_ref());
        let mut base_paths: Vec<String> =
            base_view.all_files().into_iter().map(|(p, _)| p).collect();
        base_paths.sort();
        assert_eq!(base_paths, vec!["f0.rs", "f1.rs", "f2.rs"]);
    }

    // ─────────────────────────────────────────────────────────────────────────
    // WorkingSet + cross-project query (Phase 2)
    // ─────────────────────────────────────────────────────────────────────────

    use crate::domain::{ReferenceKind, ReferenceRecord, SymbolKind, SymbolRecord};

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

    #[test]
    fn working_set_add_replaces_existing_id() {
        let mut ws = WorkingSet::new();
        let base_v1 = base_with_symbol("/a", "c0", "old_fn", "fn old_fn() {}");
        ws.add("A", base_v1);
        // Record an overlay delta, then re-add the same id -> fresh overlay.
        ws.get_mut("A")
            .unwrap()
            .overlay
            .upsert("dirty.rs", Arc::new(make_file("dirty.rs", "X")));
        assert_eq!(ws.get("A").unwrap().overlay.delta_count(), 1);

        let base_v2 = base_with_symbol("/a", "c1", "new_fn", "fn new_fn() {}");
        ws.add("A", base_v2);
        assert_eq!(ws.len(), 1, "re-add same id must replace, not duplicate");
        assert_eq!(
            ws.get("A").unwrap().overlay.delta_count(),
            0,
            "re-add resets the overlay"
        );
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

        let hits = ws.search_symbols(&Targets::All, "shared", None);
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
        let one = ws.search_symbols(&Targets::One("B".to_string()), "shared", None);
        assert_eq!(one.len(), 1);
        assert_eq!(one[0].project_id, "B");
        assert_eq!(one[0].hit.name, "shared_b");

        // Subset.
        let subset = ws.search_symbols(
            &Targets::Subset(vec!["A".to_string(), "C".to_string()]),
            "shared",
            None,
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
        let none = ws.search_symbols(&Targets::One("Z".to_string()), "shared", None);
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

    // ── overlay post-filter: search_text reflects overlay deltas ──────────────
    #[test]
    fn view_search_text_reflects_overlay_deltas() {
        // Base file f.rs contains the token NEEDLE.
        let base = base_with_symbol("/a", "c0", "f", "let NEEDLE = 1;");
        let mut overlay = Overlay::fresh(&base);

        // Sanity: base-only view finds the base hit.
        let base_view = IndexView::new(&base, None).unwrap();
        let base_hits = base_view
            .search_text(Some("NEEDLE"), None, false)
            .expect("base text search");
        assert_eq!(base_hits.total_matches, 1);
        assert_eq!(base_hits.files.len(), 1);
        assert_eq!(base_hits.files[0].path, "f.rs");

        // Consumer edits f.rs so the NEEDLE is GONE, and adds new.rs WITH NEEDLE.
        overlay.upsert("f.rs", Arc::new(make_file("f.rs", "let other = 2;")));
        overlay.upsert("new.rs", Arc::new(make_file("new.rs", "let NEEDLE = 3;")));

        let view = IndexView::new(&base, Some(&overlay)).unwrap();
        let result = view
            .search_text(Some("NEEDLE"), None, false)
            .expect("overlay text search");

        let paths: std::collections::BTreeSet<&str> =
            result.files.iter().map(|f| f.path.as_str()).collect();
        // The stale base hit for the now-edited f.rs is DROPPED (post-filter (2)).
        assert!(
            !paths.contains("f.rs"),
            "dirty edit's stale base hit must not be returned"
        );
        // The upserted new.rs hit IS returned (post-filter (3)).
        assert!(
            paths.contains("new.rs"),
            "an upserted file's fresh hit must be returned"
        );
        assert_eq!(result.total_matches, 1, "only the live overlay hit remains");
    }

    // ── overlay post-filter: tombstone removes a base text hit ────────────────
    #[test]
    fn view_search_text_tombstone_drops_base_hit() {
        let base = base_with_symbol("/a", "c0", "f", "let NEEDLE = 1;");
        let mut overlay = Overlay::fresh(&base);
        overlay.tombstone("f.rs");

        let view = IndexView::new(&base, Some(&overlay)).unwrap();
        let result = view
            .search_text(Some("NEEDLE"), None, false)
            .expect("text search");
        assert_eq!(
            result.total_matches, 0,
            "a tombstoned file contributes no text hits"
        );
        assert!(result.files.is_empty());
    }

    // ── overlay post-filter: find_references reflects overlay deltas ──────────
    #[test]
    fn view_find_references_reflects_overlay_deltas() {
        // Build a base where caller.rs references `target_fn` once.
        let mut idx = LiveIndex::empty_live_index();
        idx.is_empty = false;
        idx.loaded_at = Instant::now();
        let mut caller = make_file("caller.rs", "fn use_it() { target_fn(); }");
        caller.references = vec![ReferenceRecord {
            name: "target_fn".to_string(),
            qualified_name: None,
            kind: ReferenceKind::Call,
            byte_range: (14, 23),
            line_range: (0, 0),
            enclosing_symbol_index: None,
        }];
        let mut files: HashMap<String, Arc<IndexedFile>> = HashMap::new();
        files.insert("caller.rs".to_string(), Arc::new(caller));
        idx.files = files;
        idx.rebuild_reverse_index();
        let base = Arc::new(IndexBase::new(
            BaseKey::new("/a", CommitId::Sha("c0".to_string())),
            Arc::new(idx),
            1,
        ));

        // Base-only: one reference to target_fn in caller.rs.
        let base_view = IndexView::new(&base, None).unwrap();
        let base_refs = base_view.find_references("target_fn", None, true);
        assert_eq!(base_refs.len(), 1);
        assert_eq!(base_refs[0].0, "caller.rs");

        // Consumer edits caller.rs to remove the call (its base ref is now stale),
        // and adds new_caller.rs that DOES call target_fn.
        let mut overlay = Overlay::fresh(&base);
        overlay.upsert(
            "caller.rs",
            Arc::new(make_file("caller.rs", "fn use_it() { /* removed */ }")),
        );
        let mut new_caller = make_file("new_caller.rs", "fn n() { target_fn(); }");
        new_caller.references = vec![ReferenceRecord {
            name: "target_fn".to_string(),
            qualified_name: None,
            kind: ReferenceKind::Call,
            byte_range: (9, 18),
            line_range: (0, 0),
            enclosing_symbol_index: None,
        }];
        overlay.upsert("new_caller.rs", Arc::new(new_caller));

        let view = IndexView::new(&base, Some(&overlay)).unwrap();
        let refs = view.find_references("target_fn", None, true);
        let paths: std::collections::BTreeSet<&str> =
            refs.iter().map(|(p, _)| p.as_str()).collect();
        // The stale base reference in the edited caller.rs is dropped.
        assert!(
            !paths.contains("caller.rs"),
            "dirty edit's stale base reference must not be returned"
        );
        // The upserted new_caller.rs reference is returned.
        assert!(
            paths.contains("new_caller.rs"),
            "an upserted file's reference must be returned"
        );
        assert_eq!(refs.len(), 1);
    }

    // ── cross-project text query is source-attributed ────────────────────────
    #[test]
    fn cross_project_text_search_attribution() {
        let mut ws = WorkingSet::new();
        ws.add("A", base_with_symbol("/a", "c0", "f", "let NEEDLE = 1;"));
        ws.add("B", base_with_symbol("/b", "c0", "g", "let other = 2;"));
        ws.add("C", base_with_symbol("/c", "c0", "h", "let NEEDLE = 3;"));

        let results = ws
            .search_text(&Targets::All, Some("NEEDLE"), None, false)
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

        let hits = ws.search_symbols(&Targets::All, "shared", None);
        assert!(
            hits.is_empty(),
            "a stale-overlay entry must be skipped, not served"
        );
    }
}
