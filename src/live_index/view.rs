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
    pub fn rebase(&mut self, new_base: &IndexBase, still_dirty: &std::collections::HashSet<String>) {
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
            None => self
                .base
                .all_files()
                .map(|(p, f)| (p.clone(), f))
                .collect(),
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
        live_v2
            .files
            .insert("f0.rs".to_string(), Arc::new(make_file("f0.rs", "REINDEXED")));
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
            pristine.upsert(format!("f{i}.rs"), Arc::new(make_file(&format!("f{i}.rs"), "DIRTY")));
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
        let still_dirty: HashSet<String> =
            ["f0.rs".to_string(), "f2.rs".to_string()].into_iter().collect();
        overlay.rebase(&base_v2, &still_dirty);

        assert_eq!(overlay.delta_count(), 2, "absorbed delta f1.rs must be dropped");
        assert!(overlay.deltas.contains_key("f0.rs"));
        assert!(overlay.deltas.contains_key("f2.rs"));
        assert!(!overlay.deltas.contains_key("f1.rs"));
        assert!(overlay.is_valid_against(&base_v2), "re-fenced to new base");

        // The rebased overlay reads correctly through a view.
        let view = IndexView::new(&base_v2, Some(&overlay)).expect("view");
        assert_eq!(view.get_file("f0.rs").unwrap().content, b"DIRTY0");
        assert!(view.get_file("f2.rs").is_none(), "tombstone survives rebase");
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
}
