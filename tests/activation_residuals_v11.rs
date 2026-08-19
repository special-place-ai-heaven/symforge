//! Feature 020 Slice 4 — carried residual-family oracles (T037 / 020:T072).
//!
//! The activation campaign's frozen T037 roster carries five residual
//! families from the Slice 3 evidence. This file hosts their oracles:
//!
//! * the CCR half of the repeat-cache/CCR publication-identity fence
//!   (frozen `ccr` category assertions: "CCR cannot originate truth or
//!   extend a lease", "CCR handles encode the source publication identity",
//!   "Evicted or foreign generations return typed unavailability");
//! * the replay-authority forbidden shortcut (frozen Slice 3 residual:
//!   `ReplayRecord` v1 binds no source identity, so an identical stored
//!   success could replay after the source moved past the recorded state —
//!   Slice 4 must persist and VERIFY a typed, source-bound operation
//!   receipt).
//!
//! The read-tools half of the publication-identity fence is proven by
//! `session_cache_hit.rs::stale_publication_never_satisfies_the_repeat_read_cache`.

// Server-only integration test: drives protocol tool dispatch. File-level
// gate, same as `activation_cut_v11.rs`.
#![cfg(feature = "server")]

use std::path::{Path, PathBuf};

use serde_json::json;
use symforge::domain::{RootCandidateSource, RootRequestMode, RootResolution};
use symforge::live_index::LiveIndex;
use symforge::protocol::SymForgeServer;

/// Extract the CCR handle from a compressed tool response's footer.
fn ccr_hash(body: &str) -> Option<String> {
    body.split("hash=\"")
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .map(str::to_string)
}

fn write_fixture(root: &Path, files: &[(&str, &str)]) {
    for (rel, content) in files {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("dir");
        std::fs::write(&path, content).expect("file");
    }
}

/// A plain single-project server over `root` (no durable state).
fn server_over(root: &Path) -> SymForgeServer {
    let shared = LiveIndex::load(root).expect("fixture load");
    SymForgeServer::new(
        shared,
        "activation_residuals".to_string(),
        std::sync::Arc::new(parking_lot::Mutex::new(
            symforge::watcher::WatcherInfo::default(),
        )),
        Some(root.to_path_buf()),
        None,
    )
}

/// A server with durable project state (required by the mutation replay
/// store), following the bound-root fixture shape used by
/// `call_time_frecency.rs`.
fn durable_server_over(raw_root: &Path) -> (PathBuf, SymForgeServer) {
    let RootResolution::Bound(binding) = symforge::discovery::resolve_root_candidate(
        raw_root,
        RootCandidateSource::LaunchCwd,
        RootRequestMode::Automatic,
    ) else {
        panic!("fixture root must bind");
    };
    let root = binding.canonical_root.clone();
    let state_placement = symforge::discovery::resolve_state_placement(&binding);
    let shared = LiveIndex::load_for_state_placement(&root, &state_placement)
        .expect("fixture load with state placement");
    let server = SymForgeServer::new_with_state_placement(
        shared,
        "activation_residuals_durable".to_string(),
        std::sync::Arc::new(parking_lot::Mutex::new(
            symforge::watcher::WatcherInfo::default(),
        )),
        Some(root.clone()),
        Some(state_placement),
        None,
    );
    (root, server)
}

// ── CCR publication-identity fence ─────────────────────────────────────────

/// The frozen `ccr` category requires handles to ENCODE the source
/// publication identity: the same rendered bytes produced under two
/// different publications must mint two different handles, a replay of a
/// superseded rendering must label itself rather than pass as current, and
/// an evicted or unknown handle must answer with typed unavailability.
#[tokio::test]
async fn ccr_handles_bind_the_rendering_publication_identity() {
    let dir = tempfile::tempdir().expect("root");
    let root = dir.path();
    write_fixture(
        root,
        &[
            (
                "src/hay.rs",
                "pub fn needle_alpha_one() {}\npub fn needle_alpha_two() {}\npub fn needle_alpha_three() {}\n",
            ),
            ("src/other.rs", "pub fn unrelated() -> u64 {\n    1\n}\n"),
        ],
    );
    // Deterministic absolute mtimes: a same-second rewrite would defeat the
    // freshen's mtime guard (the C9 fixture lesson).
    let backdate = |secs: u64| {
        std::fs::File::options()
            .write(true)
            .open(root.join("src/other.rs"))
            .expect("open")
            .set_times(std::fs::FileTimes::new().set_modified(
                std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(secs),
            ))
            .expect("set mtime");
    };
    backdate(1_000_000_001);
    let server = server_over(root);

    // Force a CCR offload with a one-token budget; the footer carries the
    // handle.
    let search_params = json!({ "query": "needle_alpha", "max_tokens": 1 });
    let first = server
        .dispatch_tool_for_tests("search_text", search_params.clone())
        .await;
    let first_handle = ccr_hash(&first)
        .unwrap_or_else(|| panic!("the budgeted search did not CCR-compress:\n{first}"));

    // Positive control: the handle redeems while its rendering publication
    // is still current.
    let redeemed = server
        .dispatch_tool_for_tests("symforge_retrieve", json!({ "hash": first_handle }))
        .await;
    assert!(
        redeemed.contains("needle_alpha"),
        "a current handle must redeem the stored rendering:\n{redeemed}"
    );

    // Move the publication WITHOUT changing the rendered search output: the
    // mutated file matches no part of the query, so the re-rendered bytes
    // are identical and only the publication identity differs.
    std::fs::write(
        root.join("src/other.rs"),
        "pub fn unrelated() -> u64 {\n    2\n}\n",
    )
    .expect("mutate unrelated file");
    backdate(1_000_000_002);
    let freshened = server
        .dispatch_tool_for_tests(
            "get_file_content",
            json!({ "path": "src/other.rs", "force_refresh": true }),
        )
        .await;
    assert!(
        freshened.contains("2"),
        "the freshen must observe the moved publication:\n{freshened}"
    );

    let second = server
        .dispatch_tool_for_tests("search_text", search_params)
        .await;
    let second_handle = ccr_hash(&second)
        .unwrap_or_else(|| panic!("the repeated search did not CCR-compress:\n{second}"));

    // THE FENCE: identical rendered bytes under a moved publication must not
    // collide onto one handle — the handle encodes the rendering
    // publication's identity, not just the bytes.
    assert_ne!(
        first_handle, second_handle,
        "the CCR handle failed to encode the source publication identity: \
         the publication moved but the handle did not"
    );

    // A replay of the superseded rendering must say so: it is a bound
    // rendering cache, not fresh authority (frozen: "CCR cannot originate
    // truth or extend a lease").
    let stale_replay = server
        .dispatch_tool_for_tests("symforge_retrieve", json!({ "hash": first_handle }))
        .await;
    assert!(
        stale_replay.contains("CCR replay"),
        "a superseded rendering must be labeled as a replay, not served as \
         current output:\n{stale_replay}"
    );

    // Typed unavailability for an evicted/unknown handle (positive control
    // for the typed-refusal arm).
    let unknown = server
        .dispatch_tool_for_tests("symforge_retrieve", json!({ "hash": "000000000000" }))
        .await;
    assert!(
        unknown.contains("stale or expired handle"),
        "an unknown handle must answer with typed unavailability:\n{unknown}"
    );
}

// ── Replay-authority forbidden shortcut ────────────────────────────────────

/// A stored success may be replayed ONLY while the current source still
/// holds the post-image that success produced. `ReplayRecord` v1 bound no
/// source identity, so an identical retry after an external edit replayed a
/// success the disk no longer holds — reporting an operation whose current
/// truth nobody observed.
#[tokio::test]
async fn replay_never_serves_a_stored_success_the_current_source_does_not_hold() {
    let dir = tempfile::tempdir().expect("root");
    write_fixture(
        dir.path(),
        &[(
            "src/target.rs",
            "pub fn stable_anchor() -> u64 {\n    1\n}\n",
        )],
    );
    let (root, server) = durable_server_over(dir.path());

    let edit_params = json!({
        "path": "src/target.rs",
        "name": "stable_anchor",
        "new_body": "pub fn stable_anchor() -> u64 {\n    2\n}",
        "idempotency_key": "replay-fence-key",
    });

    let first = server
        .dispatch_tool_for_tests("replace_symbol_body", edit_params.clone())
        .await;
    assert!(
        !first.starts_with("Error"),
        "the first apply must succeed:\n{first}"
    );
    let after_first =
        std::fs::read_to_string(root.join("src/target.rs")).expect("read after first apply");
    assert!(
        after_first.contains("    2"),
        "the first apply must land on disk:\n{after_first}"
    );

    // Positive control: an identical retry against the UNCHANGED post-image
    // replays the stored success verbatim (idempotency preserved).
    let retry = server
        .dispatch_tool_for_tests("replace_symbol_body", edit_params.clone())
        .await;
    assert_eq!(
        retry, first,
        "an identical retry against the intact post-image must replay the stored success"
    );

    // The source moves past the recorded state: an external writer replaces
    // the file. The stored success's post-image is no longer on disk.
    std::fs::write(
        root.join("src/target.rs"),
        "pub fn stable_anchor() -> u64 {\n    99\n}\n",
    )
    .expect("external mutation");

    let after_external = server
        .dispatch_tool_for_tests("replace_symbol_body", edit_params)
        .await;

    // THE FENCE: the stored success must NOT replay — its post-image is
    // gone, so serving it verbatim would report an operation whose result
    // the current source does not hold. A fresh dispatch (with its own
    // honest outcome) is the only truthful answer.
    assert_ne!(
        after_external, first,
        "a stored success was replayed although the current source no longer \
         holds its post-image"
    );
    let final_disk =
        std::fs::read_to_string(root.join("src/target.rs")).expect("read after final dispatch");
    assert!(
        !final_disk.contains("99") || after_external.starts_with("Error"),
        "the response and the disk disagree: the dispatch neither re-applied \
         the edit nor reported a typed non-success.\nresponse:\n{after_external}\ndisk:\n{final_disk}"
    );
}
