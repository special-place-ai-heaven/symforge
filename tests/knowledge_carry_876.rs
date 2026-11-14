//! Issue #876: a publication carries the previous knowledge bridge and
//! authority instead of rebuilding them when its change cannot reach
//! knowledge, and what it carries equals a from-scratch build.
//!
//! Every step asserts both directions: the published bridge and authority
//! equal a fresh build over that generation's own index, and the build
//! counters moved exactly when the step's change could reach knowledge. The
//! counters are per thread and a publication builds on the thread that
//! publishes, so concurrent tests and background fills cannot move them.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use symforge::domain::WorkingTreeState;
use symforge::knowledge::SECRET_POLICY_VERSION;
use symforge::live_index::knowledge_authority::{
    AuthorityLimits, AuthorityTemporalIndex, KnowledgeAuthorityView, build_knowledge_authority,
    full_authority_builds,
};
use symforge::live_index::knowledge_bridge::{
    BridgeLimits, BridgeResolution, DerivedCoverage, KnowledgeBridge, build_knowledge_bridge,
    force_bridge_carry_for_test, force_partial_bridge_update_for_test, full_bridge_builds,
    partial_bridge_updates,
};
use symforge::live_index::single_file::{ReindexResult, remove_file, update_file_from_disk};
use symforge::live_index::store::{PublishedGeneration, SharedIndex, SharedIndexHandle};
use symforge::live_index::{LiveIndex, persist};

/// Links a code path, names a symbol defined once, a symbol defined in more
/// files than the ambiguous-sample budget keeps (so the bridge is truncated),
/// and a symbol nothing defines yet.
const GUIDE: &str = "# Guide\n\nStart at [alpha](../src/alpha.rs).\n\n\
`alpha_entry` runs first, `shared_name` is everywhere, and `later_name` does not exist yet.\n";
/// More definitions of `shared_name` than `max_ambiguous_samples` (8).
const DUPLICATES: usize = 10;

fn write(root: &Path, rel: &str, text: &str) {
    let path = root.join(rel);
    std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
    std::fs::write(path, text).expect("write");
}

fn git(root: &Path, args: &[&str]) {
    let status = symforge::process_util::hidden_command("git")
        .arg("-C")
        .arg(root)
        .args([
            "-c",
            "user.name=fixture",
            "-c",
            "user.email=fixture@invalid",
        ])
        .args(args)
        .output()
        .expect("git");
    assert!(status.status.success(), "git {args:?} failed");
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

/// Whether the generation's bridge and authority equal a from-scratch build
/// over its own index at its own content generation.
fn matches_fresh(generation: &PublishedGeneration) -> (bool, bool) {
    let bridge = fresh_bridge(generation);
    let authority = fresh_authority(generation, &bridge);
    (
        *generation.bridge == bridge,
        *generation.authority == authority,
    )
}

fn assert_matches_fresh(step: &str, generation: &PublishedGeneration) {
    let (bridge, authority) = matches_fresh(generation);
    assert!(bridge, "{step}: bridge differs from a fresh build");
    assert!(authority, "{step}: authority differs from a fresh build");
}

fn digest(generation: &PublishedGeneration) -> String {
    symforge::hash::digest_hex(
        format!("{:?}{:?}", generation.bridge, generation.authority).as_bytes(),
    )
}

/// A generation held the way a session holds it, with what it answered from
/// when it was pinned.
struct Pinned {
    step: String,
    generation: Arc<PublishedGeneration>,
    bridge: *const KnowledgeBridge,
    digest: String,
}

/// How a step must produce its bridge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Bridge {
    /// Carried as is, anchors restamped.
    Carry,
    /// Carried with the anchors into changed code files recomputed.
    Partial,
    /// Built from scratch.
    Full,
}

struct Harness<'a> {
    root: &'a Path,
    shared: SharedIndex,
    pinned: Vec<Pinned>,
}

impl Harness<'_> {
    fn reindex(&self, rel: &str) {
        let outcome = update_file_from_disk(&self.shared, self.root, rel);
        assert!(
            matches!(outcome, ReindexResult::Reindexed),
            "{rel}: {outcome:?}"
        );
    }

    /// Publish one step; assert it published, equals a fresh build, and
    /// produced the bridge and authority exactly the expected way.
    fn step(
        &mut self,
        name: &str,
        expect_bridge: Bridge,
        expect_authority_rebuild: bool,
        publish: impl FnOnce(&Self),
    ) -> Arc<PublishedGeneration> {
        let before = self.shared.published_generation();
        self.pinned.push(Pinned {
            step: name.to_string(),
            bridge: Arc::as_ptr(&before.bridge),
            digest: digest(&before),
            generation: Arc::clone(&before),
        });
        let (bridges, partials, authorities) = (
            full_bridge_builds(),
            partial_bridge_updates(),
            full_authority_builds(),
        );
        publish(self);
        let bridge = match (
            full_bridge_builds() != bridges,
            partial_bridge_updates() != partials,
        ) {
            (false, false) => Bridge::Carry,
            (false, true) => Bridge::Partial,
            (true, false) => Bridge::Full,
            (true, true) => panic!("{name}: one publication both updated and rebuilt"),
        };
        let rebuilt_authority = full_authority_builds() != authorities;
        let after = self.shared.published_generation();
        assert!(
            after.publication_generation > before.publication_generation,
            "{name}: nothing was published"
        );
        assert_eq!(
            (bridge, rebuilt_authority),
            (expect_bridge, expect_authority_rebuild),
            "{name}: (bridge, authority rebuilt)"
        );
        assert_matches_fresh(name, &after);
        after
    }

    /// Every generation pinned along the way still holds the same bridge
    /// allocation with the same content, and that content is still right for
    /// its own index and content generation.
    fn assert_pins_hold(&self) {
        for pin in &self.pinned {
            assert!(
                std::ptr::eq(Arc::as_ptr(&pin.generation.bridge), pin.bridge),
                "{}: pinned bridge moved",
                pin.step
            );
            assert_eq!(
                digest(&pin.generation),
                pin.digest,
                "{}: pinned knowledge changed",
                pin.step
            );
            assert_matches_fresh(&format!("{} (pinned)", pin.step), &pin.generation);
        }
    }
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
    for n in 0..DUPLICATES {
        write(
            root,
            &format!("src/dup_{n}.rs"),
            "pub fn shared_name() {}\n",
        );
    }
    write(
        root,
        "src/unrelated.rs",
        "pub fn unrelated() -> u64 {\n    1\n}\n",
    );
    write(root, "src/notes.rs", "pub fn notes() {}\n");
    write(root, "docs/guide.md", GUIDE);
    write(root, "CODEOWNERS", "/src/ @fixture-team\n");
    git(root, &["init", "-q"]);
    git(root, &["add", "-A"]);
    git(root, &["commit", "-q", "-m", "fixture"]);

    // Fresh load.
    let shared = LiveIndex::load(root).expect("load");
    let initial = shared.published_generation();
    assert_matches_fresh("fresh load", &initial);
    assert!(
        matches!(initial.bridge.coverage, DerivedCoverage::Truncated { .. }),
        "fixture must truncate the bridge: {:?}",
        initial.bridge.coverage
    );
    assert!(
        initial.bridge.forward.iter().any(|link| matches!(
            link.resolution,
            BridgeResolution::ResolvedDeclaredSet { .. }
        )),
        "fixture must carry an ownership selector"
    );
    let mut h = Harness {
        root,
        shared,
        pinned: Vec::new(),
    };

    // Clean to dirty: the source version moves, so authority rebuilds; the
    // bridge rebuilds only if the source identity moved with it.
    let source_before = h.shared.published_generation().source.clone();
    let before = h.shared.published_generation();
    let (bridges, authorities) = (full_bridge_builds(), full_authority_builds());
    write(
        root,
        "src/unrelated.rs",
        "pub fn unrelated() -> u64 {\n    2\n}\n",
    );
    h.reindex("src/unrelated.rs");
    let after = h.shared.published_generation();
    assert_eq!(
        before.source_version.as_ref().map(|v| v.working_tree),
        Some(WorkingTreeState::Clean)
    );
    assert_eq!(
        after.source_version.as_ref().map(|v| v.working_tree),
        Some(WorkingTreeState::Dirty)
    );
    assert_eq!(
        full_bridge_builds() != bridges,
        after.source != source_before,
        "clean-to-dirty: bridge rebuilt exactly when the source identity moved"
    );
    assert!(
        full_authority_builds() != authorities,
        "clean-to-dirty: authority must rebuild"
    );
    assert_matches_fresh("clean-to-dirty", &after);

    // A code file under an ownership selector that nothing links, on a
    // truncated bridge: carried, at a new content generation.
    let after = h.step(
        "unrelated edit, truncated bridge",
        Bridge::Carry,
        false,
        |h| {
            write(
                h.root,
                "src/unrelated.rs",
                "pub fn unrelated() -> u64 {\n    3\n}\n",
            );
            h.reindex("src/unrelated.rs");
        },
    );
    assert!(after.content_generation > before.content_generation);
    assert!(matches!(
        after.bridge.coverage,
        DerivedCoverage::Truncated { .. }
    ));

    h.step("hash-skip", Bridge::Carry, false, |h| {
        std::thread::sleep(Duration::from_millis(1100));
        let bytes = std::fs::read(h.root.join("src/unrelated.rs")).expect("read");
        std::fs::write(h.root.join("src/unrelated.rs"), bytes).expect("rewrite");
        let outcome = update_file_from_disk(&h.shared, h.root, "src/unrelated.rs");
        assert!(matches!(outcome, ReindexResult::HashSkip), "{outcome:?}");
    });
    h.step(
        "new code file changes ownership counts",
        Bridge::Full,
        true,
        |h| {
            write(h.root, "src/extra.rs", "pub fn extra() {}\n");
            h.reindex("src/extra.rs");
        },
    );
    h.step("ownership selector change", Bridge::Full, true, |h| {
        write(
            h.root,
            "CODEOWNERS",
            "/src/ @fixture-team\n/docs/ @fixture-docs\n",
        );
        h.reindex("CODEOWNERS");
    });
    h.step("referenced symbol added", Bridge::Full, true, |h| {
        write(
            h.root,
            "src/unrelated.rs",
            "pub fn unrelated() -> u64 {\n    3\n}\n\npub fn later_name() {}\n",
        );
        h.reindex("src/unrelated.rs");
    });
    // A changed file that a link resolves to, with every referenced name still
    // defined the same number of times: only its anchors are recomputed, and
    // authority, which never reads a code anchor's lines or hash, is carried.
    h.step("ambiguous match edit", Bridge::Partial, false, |h| {
        write(
            h.root,
            "src/dup_3.rs",
            "// moved\npub fn shared_name() {}\n",
        );
        h.reindex("src/dup_3.rs");
    });
    h.step("linked file edit", Bridge::Partial, false, |h| {
        write(
            h.root,
            "src/alpha.rs",
            "pub fn alpha_entry() -> u64 {\n    2\n}\n",
        );
        h.reindex("src/alpha.rs");
    });
    // Symbols unchanged but the hash moved: the anchors carry the hash, so
    // this is an update, not a plain carry.
    h.step("linked file comment append", Bridge::Partial, false, |h| {
        write(
            h.root,
            "src/alpha.rs",
            "pub fn alpha_entry() -> u64 {\n    2\n}\n// note\n",
        );
        h.reindex("src/alpha.rs");
    });
    // The symbol's start line, part of its anchor id, moves.
    h.step("linked symbol moves down", Bridge::Partial, false, |h| {
        write(
            h.root,
            "src/alpha.rs",
            "// header\n\npub fn alpha_entry() -> u64 {\n    2\n}\n// note\n",
        );
        h.reindex("src/alpha.rs");
    });
    // dup_9 defines `shared_name` but sorts past the eight kept samples, so
    // no anchor in the bridge points into it.
    h.step(
        "match beyond the sample cutoff",
        Bridge::Carry,
        false,
        |h| {
            write(
                h.root,
                "src/dup_9.rs",
                "// moved\npub fn shared_name() {}\n",
            );
            h.reindex("src/dup_9.rs");
        },
    );
    // A referenced name disappears from one file: resolutions change.
    h.step("linked symbol renamed", Bridge::Full, true, |h| {
        write(
            h.root,
            "src/alpha.rs",
            "// header\n\npub fn alpha_start() -> u64 {\n    2\n}\n// note\n",
        );
        h.reindex("src/alpha.rs");
    });
    h.step("knowledge edit", Bridge::Full, true, |h| {
        write(
            h.root,
            "docs/guide.md",
            &format!("{GUIDE}\nOne more line.\n"),
        );
        h.reindex("docs/guide.md");
    });

    // A policy file may name any file, so with one present an unrelated code
    // edit still carries the bridge but rebuilds authority.
    h.step("policy file added", Bridge::Full, true, |h| {
        write(h.root, ".symforge-knowledge.toml", "version = 1\n");
        h.reindex(".symforge-knowledge.toml");
    });
    h.step(
        "unrelated edit with a policy file",
        Bridge::Carry,
        true,
        |h| {
            write(h.root, "src/extra.rs", "pub fn extra() -> u8 {\n    1\n}\n");
            h.reindex("src/extra.rs");
        },
    );
    h.step("policy file edit", Bridge::Full, true, |h| {
        write(
            h.root,
            ".symforge-knowledge.toml",
            "version = 1\n# edited\n",
        );
        h.reindex(".symforge-knowledge.toml");
    });
    h.step("policy file removed", Bridge::Full, true, |h| {
        std::fs::remove_file(h.root.join(".symforge-knowledge.toml")).expect("delete");
        assert!(remove_file(&h.shared, ".symforge-knowledge.toml"));
    });

    // A code file becomes knowledge: notes.rs is renamed to notes.md.
    h.step("code to knowledge (remove .rs)", Bridge::Full, true, |h| {
        std::fs::rename(h.root.join("src/notes.rs"), h.root.join("src/notes.md")).expect("rename");
        assert!(remove_file(&h.shared, "src/notes.rs"));
    });
    h.step("code to knowledge (add .md)", Bridge::Full, true, |h| {
        h.reindex("src/notes.md")
    });
    h.step("delete", Bridge::Full, true, |h| {
        std::fs::remove_file(h.root.join("src/unrelated.rs")).expect("delete");
        assert!(remove_file(&h.shared, "src/unrelated.rs"));
    });
    h.assert_pins_hold();

    // Snapshot restore, then the first publication after it.
    let placement = {
        let binding = match symforge::discovery::resolve_root_candidate(
            root,
            symforge::domain::RootCandidateSource::LaunchCwd,
            symforge::domain::RootRequestMode::Automatic,
        ) {
            symforge::domain::RootResolution::Bound(binding) => binding,
            other => panic!("fixture root should bind: {other:?}"),
        };
        symforge::discovery::resolve_state_placement(&binding)
    };
    persist::checkpoint_shared_index(&h.shared, root, &placement).expect("checkpoint");
    let snapshot = persist::load_snapshot(root, &placement).expect("snapshot");
    let restored = SharedIndexHandle::shared(persist::snapshot_to_live_index(snapshot, root));
    let restored_initial = restored.published_generation();
    let placeholder = matches!(restored_initial.bridge.coverage, DerivedCoverage::Loading);
    if !placeholder {
        assert_matches_fresh("restored", &restored_initial);
    }
    let mut r = Harness {
        root,
        shared: restored,
        pinned: Vec::new(),
    };
    r.step(
        "first publication after restore",
        if placeholder {
            Bridge::Full
        } else {
            Bridge::Carry
        },
        placeholder,
        |r| {
            write(r.root, "src/extra.rs", "pub fn extra() -> u8 {\n    2\n}\n");
            r.reindex("src/extra.rs");
        },
    );
    r.assert_pins_hold();

    // Negative control for the partial update: with the per-name count check
    // skipped, bringing back a name the guide refers to only refreshes
    // alpha.rs's file anchor and leaves `alpha_entry` unresolved, which the
    // equivalence check must reject.
    let (bridges, partials) = (full_bridge_builds(), partial_bridge_updates());
    force_partial_bridge_update_for_test(true);
    write(
        root,
        "src/alpha.rs",
        "pub fn alpha_entry() -> u64 {\n    3\n}\n",
    );
    h.reindex("src/alpha.rs");
    force_partial_bridge_update_for_test(false);
    assert_eq!(
        full_bridge_builds(),
        bridges,
        "the forced update must not rebuild"
    );
    assert_ne!(
        partial_bridge_updates(),
        partials,
        "the forced update must take the partial path"
    );
    let (bridge_ok, _) = matches_fresh(&h.shared.published_generation());
    assert!(
        !bridge_ok,
        "a partial update that skips the count check must fail the equivalence check"
    );

    // Negative control: forcing the carry over a knowledge edit must leave a
    // bridge the equivalence check rejects.
    let (bridges, _) = (full_bridge_builds(), full_authority_builds());
    force_bridge_carry_for_test(true);
    write(
        root,
        "docs/guide.md",
        "# Guide\n\nRewritten without links.\n",
    );
    h.reindex("docs/guide.md");
    force_bridge_carry_for_test(false);
    assert_eq!(
        full_bridge_builds(),
        bridges,
        "the forced carry must not rebuild"
    );
    let (bridge_ok, _) = matches_fresh(&h.shared.published_generation());
    assert!(
        !bridge_ok,
        "a stale carried bridge must fail the equivalence check"
    );
}
