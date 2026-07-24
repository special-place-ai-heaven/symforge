// SF-AAP-001 regression: existing literal repo-relative paths must ALWAYS beat
// symbol/generated-path heuristics in `edit_plan`. The failure mode is
// `find_candidates_cascade`'s qualification-stripping (`rsplit_once('.')`): a
// path-shaped target like `config.json` is stripped to `json` and matches an
// unrelated symbol named `json` in another file, so `plan_edit` recommended a
// DIFFERENT path instead of the file the caller named.
//
// Server-only integration test (mirrors edit_plan_symbol_line.rs gating).
#![cfg(feature = "server")]

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime};

use symforge::live_index::LiveIndex;
use symforge::live_index::git_temporal::{GitTemporalIndex, GitTemporalState, GitTemporalStats};
use symforge::protocol::edit_plan::plan_edit;
use tempfile::TempDir;

fn write_file(dir: &Path, name: &str, content: &str) {
    let path = dir.join(name);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, content).unwrap();
}

fn empty_temporal() -> GitTemporalIndex {
    GitTemporalIndex {
        files: HashMap::new(),
        stats: GitTemporalStats {
            total_commits_analyzed: 0,
            analysis_window_days: 90,
            hotspots: vec![],
            most_coupled: vec![],
            computed_at: SystemTime::now(),
            compute_duration: Duration::ZERO,
        },
        state: GitTemporalState::Unavailable("not a git repo".to_string()),
    }
}

/// A `.json` file that has no colliding symbols of its own, plus a Rust file
/// with a free function named exactly `json` (the extension of the target).
/// `plan_edit("config.json")` must target `config.json`, not the `json` symbol
/// in `src/lib.rs`.
#[test]
fn edit_plan_existing_literal_path_beats_extension_symbol_collision() {
    let dir = TempDir::new().expect("tempdir");
    // Decoy symbols named after the extensions of the literal-path targets.
    write_file(
        dir.path(),
        "src/lib.rs",
        "fn json() {}\nfn md() {}\nfn ts() {}\n",
    );
    write_file(dir.path(), "config.json", "{\"a\":1}\n");
    write_file(dir.path(), "docs/notes.md", "# Title\n\nBody text.\n");
    write_file(
        dir.path(),
        "types/foo.d.ts",
        "export declare const x: number;\n",
    );

    let shared = LiveIndex::load(dir.path()).expect("LiveIndex::load failed");
    let index = shared.read();
    let temporal = empty_temporal();

    // Sanity: the literal files are actually indexed (so file_hit can be set).
    let indexed: Vec<String> = index.all_files().map(|(p, _)| p.to_string()).collect();
    assert!(
        indexed.iter().any(|p| p == "config.json"),
        "fixture invalid: config.json not indexed; files: {indexed:?}"
    );

    for (target, collided_symbol) in [
        ("config.json", "json"),
        ("docs/notes.md", "md"),
        ("types/foo.d.ts", "ts"),
    ] {
        // Every named case must be a real indexed file — a skipped case is a
        // silently vacuous assertion, so fail loudly if the fixture regressed.
        assert!(
            indexed.iter().any(|p| p == target),
            "fixture invalid: named case {target:?} not indexed; files: {indexed:?}"
        );
        let plan = plan_edit(&index, &temporal, target);

        assert!(
            plan.contains(&format!("Found file: {target}")),
            "edit_plan must target the existing literal path {target:?}, not a \
             symbol heuristic. Plan:\n{plan}"
        );
        assert!(
            !plan.contains(&format!("{collided_symbol} in src/lib.rs")),
            "edit_plan must NOT recommend a different path via extension-stripping \
             collision (symbol {collided_symbol:?}) for literal path {target:?}. \
             Plan:\n{plan}"
        );
    }
}

/// AAP-001 defect 1: a path-shaped basename (`notes.md`) that matches several
/// files by trailing segment must pick a DETERMINISTIC winner (sorted-first)
/// and report the ambiguity — never a HashMap-iteration-order coin flip.
#[test]
fn edit_plan_ambiguous_basename_is_deterministic() {
    let dir = TempDir::new().expect("tempdir");
    write_file(dir.path(), "docs/notes.md", "# Docs\n\nDocs body.\n");
    write_file(
        dir.path(),
        "archive/notes.md",
        "# Archive\n\nArchive body.\n",
    );

    let shared = LiveIndex::load(dir.path()).expect("LiveIndex::load failed");
    let index = shared.read();
    let temporal = empty_temporal();

    let indexed: Vec<String> = index.all_files().map(|(p, _)| p.to_string()).collect();
    for expected in ["docs/notes.md", "archive/notes.md"] {
        assert!(
            indexed.iter().any(|p| p == expected),
            "fixture invalid: {expected} not indexed; files: {indexed:?}"
        );
    }

    // `archive/notes.md` sorts before `docs/notes.md`, so it is the stable pick.
    let plan = plan_edit(&index, &temporal, "notes.md");
    assert!(
        plan.contains("Found file: archive/notes.md"),
        "ambiguous basename must resolve deterministically to the sorted-first \
         path. Plan:\n{plan}"
    );
    assert!(
        plan.contains("Ambiguous file target 'notes.md'"),
        "an ambiguous basename must surface the ambiguity, not a silent pick. \
         Plan:\n{plan}"
    );
    // Determinism: repeated calls (each re-iterating the HashMap) must agree.
    for _ in 0..8 {
        assert_eq!(
            plan_edit(&index, &temporal, "notes.md"),
            plan,
            "ambiguous-basename resolution must be stable across calls"
        );
    }
}

/// AAP-001 defect 2: a BARE identifier (`agents`, no `.`/`/`) that merely
/// suffix-matches an extensionless file (`tools/agents`) must NOT short-circuit
/// to the file plan — it must run the symbol cascade so `fn agents` is reached.
#[test]
fn edit_plan_bare_target_reaches_symbol_over_extensionless_file() {
    let dir = TempDir::new().expect("tempdir");
    write_file(dir.path(), "src/lib.rs", "fn agents() {}\n");
    // `agents` is a recognized extensionless narrative basename → indexed as
    // Text, so its path (`tools/agents`) is a real trailing-segment suffix.
    write_file(dir.path(), "tools/agents", "team roster\n");

    let shared = LiveIndex::load(dir.path()).expect("LiveIndex::load failed");
    let index = shared.read();
    let temporal = empty_temporal();

    let indexed: Vec<String> = index.all_files().map(|(p, _)| p.to_string()).collect();
    assert!(
        indexed.iter().any(|p| p == "tools/agents"),
        "fixture invalid: extensionless tools/agents not indexed; files: {indexed:?}"
    );

    let plan = plan_edit(&index, &temporal, "agents");
    assert!(
        plan.contains("agents in src/lib.rs"),
        "bare target must reach the `fn agents` symbol, not be suppressed by the \
         extensionless file `tools/agents`. Plan:\n{plan}"
    );
    assert!(
        !plan.contains("Found file: tools/agents"),
        "bare target must not short-circuit to the extensionless-file plan. \
         Plan:\n{plan}"
    );
}
