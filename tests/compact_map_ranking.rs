// Server-only integration test: depends on a `#[cfg(feature = "server")]`
// module (protocol/daemon/cli/sidecar/watcher/analytics). Gating the whole
// file keeps `--no-default-features --features embed --all-targets` compiling.
#![cfg(feature = "server")]

//! Acceptance coverage for the importance-ranked compact repo map
//! (feature 007, US3 / Phase C).
//!
//! Contract: `specs/007-intelligence-pattern-ports/contracts/compact-map-ranking.md`.
//!
//! In the DEFAULT `detail=compact` repo map, the file-bearing "Key types"
//! entry-point section is ordered by importance:
//!
//! ```text
//! rank_key(file) = (dependent_count DESC, churn_score DESC, relative_path ASC)
//! ```
//!
//! and each line annotated `… (path) (→N)` when the file's distinct dependent
//! count `N >= 2` (bare `(path)` when `N < 2`). Ordering must be deterministic
//! across repeated renders (stable tie-break). The `detail=full` /
//! `detail=tree` outputs are produced by different code paths and are NOT
//! exercised here — their byte-for-byte stability is covered by the
//! `repo_outline` / `file_tree` lib tests that this phase does not touch.
//!
//! A fresh tempdir is not a git repo, so the git temporal index degrades to
//! `Unavailable`; churn defaults to `0.0` for every file. The ranking therefore
//! resolves entirely on `(dependent_count DESC, relative_path ASC)` here, which
//! is exactly what these fixtures pin. The churn tie-break is exercised against
//! a real `Ready` temporal index in the manual quickstart.

use std::fs;
use std::sync::Arc;

use parking_lot::Mutex;
use serde_json::json;
use symforge::live_index::LiveIndex;
use symforge::protocol::SymForgeServer;
use symforge::watcher::WatcherInfo;
use tempfile::TempDir;

// ─── Fixture ─────────────────────────────────────────────────────────────────

struct Fixture {
    _dir: TempDir,
    server: SymForgeServer,
}

impl Fixture {
    fn new(files: &[(&str, &str)]) -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().to_path_buf();
        for (rel, content) in files {
            let path = root.join(rel);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).expect("create parent dir");
            }
            fs::write(&path, content).expect("write fixture file");
        }
        let shared = LiveIndex::load(&root).expect("LiveIndex::load");
        let watcher_info = Arc::new(Mutex::new(WatcherInfo::default()));
        let server = SymForgeServer::new(
            shared,
            "compact_map_ranking_test".to_string(),
            watcher_info,
            Some(root),
            None,
        );
        Self { _dir: dir, server }
    }

    /// Distinct dependent FILE count for `path`, computed via the exact query the
    /// compact-map renderer uses — so ranking assertions stay deterministic
    /// regardless of the parser's per-reference edge details.
    fn expected_dependents(&self, path: &str) -> usize {
        self.server
            .index()
            .read()
            .capture_find_dependents_view(path)
            .files
            .len()
    }

    /// Render the DEFAULT compact repo map (detail omitted → "compact").
    async fn compact_map(&self) -> String {
        self.server
            .dispatch_tool_for_tests("get_repo_map", json!({}))
            .await
    }

    /// Render the compact repo map with an explicit `detail=compact`.
    async fn compact_map_explicit(&self) -> String {
        self.server
            .dispatch_tool_for_tests("get_repo_map", json!({ "detail": "compact" }))
            .await
    }
}

/// One hub type (`zeta.rs::Core`) imported and used by three distinct consumer
/// files, and one leaf type (`alpha.rs::Leaf`) nothing references. Under the
/// real parser this yields `>= 2` distinct dependents for `zeta.rs` and `0` for
/// `alpha.rs`, so the importance ordering and the `(→N)` annotation are both
/// meaningful.
///
/// The hub is named so it sorts ALPHABETICALLY LAST (`zeta`) and the leaf
/// ALPHABETICALLY FIRST (`alpha`). Under the legacy alphabetical sort the leaf
/// would precede the hub; importance ranking must invert that, so the ordering
/// assertion genuinely proves the rank key rather than coincidental path order.
fn hub_and_leaf() -> Vec<(&'static str, &'static str)> {
    vec![
        (
            "src/lib.rs",
            "pub mod zeta;\npub mod alpha;\npub mod a;\npub mod b;\npub mod c;\n",
        ),
        ("src/zeta.rs", "pub struct Core {\n    pub seed: u32,\n}\n"),
        ("src/alpha.rs", "pub struct Leaf {\n    pub idle: u32,\n}\n"),
        (
            "src/a.rs",
            "use crate::zeta::Core;\n\npub fn a(c: &Core) -> u32 {\n    c.seed\n}\n",
        ),
        (
            "src/b.rs",
            "use crate::zeta::Core;\n\npub fn b(c: &Core) -> u32 {\n    c.seed + 1\n}\n",
        ),
        (
            "src/c.rs",
            "use crate::zeta::Core;\n\npub fn c(c: &Core) -> u32 {\n    c.seed + 2\n}\n",
        ),
    ]
}

/// Locate the 1-based ordinal of the first compact-map "Key types" line that
/// mentions `(src/<stem>.rs)`. Returns `None` when the file is absent from the
/// section. Only lines inside the `Key types:` block are considered.
fn entry_position(map: &str, file_rel: &str) -> Option<usize> {
    let needle = format!("({file_rel})");
    let mut in_section = false;
    let mut ordinal = 0usize;
    for line in map.lines() {
        if line.starts_with("Key types:") {
            in_section = true;
            continue;
        }
        if !in_section {
            continue;
        }
        // The block ends at the first blank line / non-entry line after it began.
        if !line.starts_with("  ") {
            break;
        }
        ordinal += 1;
        if line.contains(&needle) {
            return Some(ordinal);
        }
    }
    None
}

/// Extract the full compact-map entry line for `(src/<stem>.rs)`, if present.
fn entry_line<'a>(map: &'a str, file_rel: &str) -> Option<&'a str> {
    let needle = format!("({file_rel})");
    map.lines()
        .filter(|l| l.starts_with("  "))
        .find(|l| l.contains(&needle))
}

// ─── (a) Importance ordering: hub ranks before leaf ──────────────────────────

#[tokio::test]
async fn core_ranks_before_leaf_in_compact_map() {
    let fx = Fixture::new(&hub_and_leaf());

    let core_deps = fx.expected_dependents("src/zeta.rs");
    let leaf_deps = fx.expected_dependents("src/alpha.rs");
    assert!(
        core_deps >= 2,
        "fixture must give src/zeta.rs >= 2 distinct dependents for a meaningful \
         ranking assertion (got {core_deps})"
    );
    assert_eq!(
        leaf_deps, 0,
        "fixture must give src/alpha.rs zero dependents (got {leaf_deps})"
    );

    let map = fx.compact_map().await;

    let core_pos = entry_position(&map, "src/zeta.rs").unwrap_or_else(|| {
        panic!("compact map must list src/zeta.rs in Key types:\n{map}");
    });
    let leaf_pos = entry_position(&map, "src/alpha.rs").unwrap_or_else(|| {
        panic!("compact map must list src/alpha.rs in Key types:\n{map}");
    });

    // `zeta` sorts alphabetically AFTER `alpha`, so a passing assertion here
    // can only come from importance ranking inverting path order.
    assert!(
        core_pos < leaf_pos,
        "high-fan-in src/zeta.rs (deps={core_deps}) must rank before zero-fan-in \
         src/alpha.rs (deps={leaf_deps}) despite sorting later alphabetically; \
         core_pos={core_pos} leaf_pos={leaf_pos}\n{map}"
    );
}

// ─── (b) Annotation: N>=2 shows (→N); N<2 shows none ──────────────────────────

#[tokio::test]
async fn high_fan_in_annotated_low_fan_in_not() {
    let fx = Fixture::new(&hub_and_leaf());
    let core_deps = fx.expected_dependents("src/zeta.rs");
    assert!(
        core_deps >= 2,
        "fixture precondition: hub has >= 2 dependents"
    );

    let map = fx.compact_map().await;

    let core_line = entry_line(&map, "src/zeta.rs")
        .unwrap_or_else(|| panic!("compact map must contain a src/zeta.rs entry line\n{map}"));
    let leaf_line = entry_line(&map, "src/alpha.rs")
        .unwrap_or_else(|| panic!("compact map must contain a src/alpha.rs entry line\n{map}"));

    let needle = format!("(→{core_deps})");
    assert!(
        core_line.contains(&needle),
        "src/zeta.rs entry must carry the fan-in annotation {needle:?}\nline: {core_line:?}"
    );
    assert!(
        !leaf_line.contains("(→"),
        "src/alpha.rs (0 dependents) must NOT carry a (→N) annotation\nline: {leaf_line:?}"
    );
}

// ─── (c) Deterministic: identical state ⇒ identical render ────────────────────

#[tokio::test]
async fn compact_map_render_is_deterministic() {
    let fx = Fixture::new(&hub_and_leaf());

    let first = fx.compact_map().await;
    let second = fx.compact_map().await;

    assert_eq!(
        first, second,
        "two renders of the same index must be byte identical (stable tie-break)\n\
         first:\n{first}\nsecond:\n{second}"
    );
}

// ─── (d) detail omitted == detail="compact" (same renderer) ───────────────────

#[tokio::test]
async fn default_detail_matches_explicit_compact() {
    let fx = Fixture::new(&hub_and_leaf());

    let default_map = fx.compact_map().await;
    let explicit_map = fx.compact_map_explicit().await;

    assert_eq!(
        default_map, explicit_map,
        "default (no detail) and detail=\"compact\" must render identically\n\
         default:\n{default_map}\nexplicit:\n{explicit_map}"
    );
}
