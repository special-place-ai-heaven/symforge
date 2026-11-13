//! Issue #876: a publication carries the previous knowledge bridge and
//! authority instead of rebuilding them when its change cannot reach
//! knowledge, and what it carries equals a from-scratch build.
//!
//! One test in this binary on purpose: it reads process-wide build counters,
//! which a concurrently running test would move.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use symforge::knowledge::SECRET_POLICY_VERSION;
use symforge::live_index::LiveIndex;
use symforge::live_index::knowledge_authority::{
    AuthorityLimits, AuthorityTemporalIndex, KnowledgeAuthorityView, build_knowledge_authority,
    full_authority_builds,
};
use symforge::live_index::knowledge_bridge::{
    BridgeLimits, KnowledgeBridge, build_knowledge_bridge, full_bridge_builds,
};
use symforge::live_index::single_file::{ReindexResult, remove_file, update_file_from_disk};
use symforge::live_index::store::{PublishedGeneration, SharedIndex};

const GUIDE: &str = "# Guide\n\nStart at [alpha](../src/alpha.rs).\n\n\
`alpha_entry` runs first, `shared_name` exists twice, and `later_name` does not exist yet.\n";

fn write(root: &Path, rel: &str, text: &str) {
    let path = root.join(rel);
    std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
    std::fs::write(path, text).expect("write");
}

fn fresh_bridge(generation: &PublishedGeneration) -> KnowledgeBridge {
    build_knowledge_bridge(
        &generation.live,
        generation.source.as_deref().expect("bound source"),
        generation.content_generation,
        &BridgeLimits::default(),
    )
}

fn fresh_authority(
    generation: &PublishedGeneration,
    bridge: &KnowledgeBridge,
) -> KnowledgeAuthorityView {
    build_knowledge_authority(
        &generation.live,
        generation.source.as_deref().expect("bound source"),
        generation
            .source_version
            .as_deref()
            .expect("source version"),
        generation.content_generation,
        bridge,
        &AuthorityTemporalIndex::from_published(generation),
        generation
            .manifest
            .as_ref()
            .map_or(SECRET_POLICY_VERSION, |manifest| {
                manifest.secret_policy_version
            }),
        &AuthorityLimits::default(),
    )
}

/// The generation's bridge and authority equal a from-scratch build over its
/// own index at its own content generation.
fn assert_matches_fresh(step: &str, generation: &PublishedGeneration) {
    let bridge = fresh_bridge(generation);
    assert!(
        *generation.bridge == bridge,
        "{step}: published bridge differs from a fresh build"
    );
    assert!(
        *generation.authority == fresh_authority(generation, &bridge),
        "{step}: published authority differs from a fresh build"
    );
}

/// Publish one step and report whether the bridge and authority were rebuilt.
fn step(
    shared: &SharedIndex,
    name: &str,
    publish: impl FnOnce(),
) -> (Arc<PublishedGeneration>, bool, bool) {
    let pinned = shared.published_generation();
    let pinned_bridge = Arc::clone(&pinned.bridge);
    let (bridges, authorities) = (full_bridge_builds(), full_authority_builds());
    publish();
    let rebuilt_bridge = full_bridge_builds() != bridges;
    let rebuilt_authority = full_authority_builds() != authorities;
    let current = shared.published_generation();
    assert!(
        current.publication_generation > pinned.publication_generation,
        "{name}: nothing was published"
    );
    assert_matches_fresh(name, &current);
    // The generation a session pinned before the step still answers from the
    // bridge it was built with, and that bridge is still right for it.
    assert!(
        Arc::ptr_eq(&pinned.bridge, &pinned_bridge),
        "{name}: pinned bridge moved"
    );
    assert_matches_fresh(&format!("{name} (pinned)"), &pinned);
    (current, rebuilt_bridge, rebuilt_authority)
}

#[test]
fn publications_carry_knowledge_only_when_a_fresh_build_would_equal_it() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    write(
        root,
        "src/alpha.rs",
        "pub fn alpha_entry() -> u64 {\n    1\n}\n",
    );
    write(root, "src/dup_a.rs", "pub fn shared_name() {}\n");
    write(root, "src/dup_b.rs", "pub fn shared_name() {}\n");
    write(
        root,
        "src/unrelated.rs",
        "pub fn unrelated() -> u64 {\n    1\n}\n",
    );
    write(root, "docs/guide.md", GUIDE);
    let shared = LiveIndex::load(root).expect("load");
    let initial = shared.published_generation();
    assert!(
        initial.bridge.forward.len() >= 3,
        "fixture must link a path and symbols"
    );
    assert_matches_fresh("initial", &initial);

    let reindex = |rel: &str| {
        let outcome = update_file_from_disk(&shared, root, rel);
        assert!(
            matches!(outcome, ReindexResult::Reindexed),
            "{rel}: {outcome:?}"
        );
    };

    // A code file nothing points at: carried, with a new content generation.
    let before = shared.published_generation().content_generation;
    let (after, bridge, authority) = step(&shared, "unrelated edit", || {
        write(
            root,
            "src/unrelated.rs",
            "pub fn unrelated() -> u64 {\n    2\n}\n",
        );
        reindex("src/unrelated.rs");
    });
    assert!(
        after.content_generation > before,
        "edit must be a content change"
    );
    assert!(!bridge && !authority, "unrelated edit rebuilt knowledge");

    // Same bytes, newer mtime: carried.
    let (_, bridge, authority) = step(&shared, "hash-skip", || {
        std::thread::sleep(Duration::from_millis(1100));
        let bytes = std::fs::read(root.join("src/unrelated.rs")).expect("read");
        std::fs::write(root.join("src/unrelated.rs"), bytes).expect("rewrite");
        let outcome = update_file_from_disk(&shared, root, "src/unrelated.rs");
        assert!(matches!(outcome, ReindexResult::HashSkip), "{outcome:?}");
    });
    assert!(!bridge && !authority, "hash-skip rebuilt knowledge");

    // The code file now defines a name the guide refers to: rebuilt.
    let (_, bridge, _) = step(&shared, "referenced symbol added", || {
        write(
            root,
            "src/unrelated.rs",
            "pub fn unrelated() -> u64 {\n    2\n}\n\npub fn later_name() {}\n",
        );
        reindex("src/unrelated.rs");
    });
    assert!(bridge, "a newly defined referenced name must rebuild");

    // One of two definitions of an ambiguous name moves: rebuilt.
    let (_, bridge, _) = step(&shared, "ambiguous match edit", || {
        write(root, "src/dup_b.rs", "// moved\npub fn shared_name() {}\n");
        reindex("src/dup_b.rs");
    });
    assert!(bridge, "editing a referenced symbol's file must rebuild");

    // The linked file changes: rebuilt.
    let (_, bridge, _) = step(&shared, "linked file edit", || {
        write(
            root,
            "src/alpha.rs",
            "pub fn alpha_entry() -> u64 {\n    2\n}\n",
        );
        reindex("src/alpha.rs");
    });
    assert!(bridge, "editing a linked file must rebuild");

    // The knowledge file itself changes: rebuilt.
    let (_, bridge, authority) = step(&shared, "knowledge edit", || {
        write(root, "docs/guide.md", &format!("{GUIDE}\nOne more line.\n"));
        reindex("docs/guide.md");
    });
    assert!(bridge && authority, "editing knowledge must rebuild");

    // Delete: the path set shrinks, rebuilt.
    let (_, bridge, _) = step(&shared, "delete", || {
        std::fs::remove_file(root.join("src/unrelated.rs")).expect("delete");
        assert!(remove_file(&shared, "src/unrelated.rs"), "delete published");
    });
    assert!(bridge, "a delete must rebuild");

    // Rename: one path leaves and another arrives, rebuilt at each step.
    let (_, bridge, _) = step(&shared, "rename (remove)", || {
        std::fs::rename(root.join("src/dup_a.rs"), root.join("src/dup_c.rs")).expect("rename");
        assert!(remove_file(&shared, "src/dup_a.rs"), "remove published");
    });
    assert!(bridge, "a rename's removal must rebuild");
    let (_, bridge, _) = step(&shared, "rename (add)", || reindex("src/dup_c.rs"));
    assert!(bridge, "a rename's addition must rebuild");

    // The very first generation still answers from its own knowledge.
    assert_matches_fresh("initial (still pinned)", &initial);
}
