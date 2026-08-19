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
//!   receipt);
//! * the falsifiable D14 live-observer contrast (the Slice 3 model-level
//!   oracle preserves a generation nothing could move; here the SAME live
//!   publication fence provably moves under the observer lane and stays
//!   byte-identical across failed read observations);
//! * the D16 structured activation boundary (the typed evidence receipt
//!   names the exact immutable publication that rendered the body, held as
//!   captured data across later publications; cross-process atomicity under
//!   arbitrary concurrent publication is NOT claimed).
//!
//! The read-tools half of the publication-identity fence is proven by
//! `session_cache_hit.rs::stale_publication_never_satisfies_the_repeat_read_cache`.
//! The cancelled non-abortable `index_folder` residual needs a live daemon and
//! its `#[cfg(test)]` spawn helpers, so its oracle lives in `src/daemon.rs`
//! (`a_cancelled_activation_never_governs_until_an_observed_resync`).

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
    server_with_shared(root).1
}

/// Same fixture, but keeps a clone of the `SharedIndex` handle so the test can
/// observe the server's own publications (the fence and the receipt) from
/// outside the dispatch.
fn server_with_shared(root: &Path) -> (symforge::live_index::store::SharedIndex, SymForgeServer) {
    let shared = LiveIndex::load(root).expect("fixture load");
    let observer = std::sync::Arc::clone(&shared);
    let server = SymForgeServer::new(
        shared,
        "activation_residuals".to_string(),
        std::sync::Arc::new(parking_lot::Mutex::new(
            symforge::watcher::WatcherInfo::default(),
        )),
        Some(root.to_path_buf()),
        None,
    );
    (observer, server)
}

/// Deterministic absolute mtime: a same-second rewrite would defeat the
/// freshen's mtime guard (the C9 fixture lesson).
fn backdate(path: &Path, secs: u64) {
    std::fs::File::options()
        .write(true)
        .open(path)
        .expect("open for backdate")
        .set_times(
            std::fs::FileTimes::new().set_modified(
                std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(secs),
            ),
        )
        .expect("set mtime");
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

// ── D14: live-observer invalidation, made falsifiable ──────────────────────

/// The model-level D14 oracle
/// (`read_gate_authority_v11.rs::a_failed_observation_refuses_without_disturbing_the_current_generation`)
/// is unfalsifiable on its own: its model generation has no lane that could
/// move it, so the preservation assertion cannot fail. This is the live
/// contrast that makes the claim falsifiable: on one real runtime, the SAME
/// publication fence that provably MOVES under the observer lane (refresh and
/// confirmed-absent removal) stays byte-identical across failed read
/// observations. Only the observer seam may invalidate the Current
/// generation, and only on its own independent evidence.
#[tokio::test]
async fn a_failed_read_observation_preserves_the_fence_the_observer_moves() {
    let dir = tempfile::tempdir().expect("root");
    let root = dir.path();
    write_fixture(
        root,
        &[
            ("src/kept.rs", "pub fn kept_anchor() -> u64 {\n    1\n}\n"),
            ("src/doomed.rs", "pub fn doomed_anchor() {}\n"),
        ],
    );
    backdate(&root.join("src/kept.rs"), 1_000_000_001);
    let (fence_handle, server) = server_with_shared(root);

    // Failed pure observation #1: a path the generation never held and the
    // disk does not hold. The read refuses with a typed miss; the fence must
    // not move — a failure yields no evidence to publish.
    let before = fence_handle.publication_fence();
    let missing = server
        .dispatch_tool_for_tests("get_file_content", json!({ "path": "src/gone.rs" }))
        .await;
    assert!(
        missing.contains("File not found"),
        "a never-indexed missing path must refuse with a typed miss:\n{missing}"
    );
    assert_eq!(
        fence_handle.publication_fence(),
        before,
        "a failed read observation must not disturb the publication fence"
    );

    // Failed pure observation #2: an admission refusal (path traversal) is
    // equally evidence-free.
    let outside = server
        .dispatch_tool_for_tests("get_file_content", json!({ "path": "../outside.rs" }))
        .await;
    assert!(
        outside.contains("outside the repository"),
        "a traversal path must refuse as a containment violation:\n{outside}"
    );
    assert_eq!(
        fence_handle.publication_fence(),
        before,
        "an admission refusal must not disturb the publication fence"
    );

    // THE CONTRAST that makes the preservation falsifiable: the observer lane
    // on the SAME runtime moves the same fence. A request-path freshen that
    // observes changed bytes republishes.
    std::fs::write(
        root.join("src/kept.rs"),
        "pub fn kept_anchor() -> u64 {\n    2\n}\n",
    )
    .expect("mutate kept file");
    backdate(&root.join("src/kept.rs"), 1_000_000_002);
    let refreshed = server
        .dispatch_tool_for_tests(
            "get_file_content",
            json!({ "path": "src/kept.rs", "force_refresh": true }),
        )
        .await;
    assert!(
        refreshed.contains("    2"),
        "the observer refresh must serve the republished bytes:\n{refreshed}"
    );
    let after_refresh = fence_handle.publication_fence();
    assert!(
        after_refresh.content_generation > before.content_generation,
        "control: the observer lane must move the fence the failed reads preserved \
         (before {before:?}, after {after_refresh:?})"
    );

    // The preservation holds against the MOVED fence too — it is not an
    // artifact of a counter nothing touches (the exact D14 unfalsifiability
    // being repaired).
    let missing_again = server
        .dispatch_tool_for_tests("get_file_content", json!({ "path": "src/gone.rs" }))
        .await;
    assert!(missing_again.contains("File not found"), "{missing_again}");
    assert_eq!(
        fence_handle.publication_fence(),
        after_refresh,
        "a failed read observation must also preserve a fence that has already moved"
    );

    // Observer removal on independent evidence: an indexed file deleted on
    // disk. The targeted freshen CONFIRMS the absence under a publication
    // fence and removes it — the response is a miss, but unlike the pure
    // failures above, this miss carries observer evidence and the fence moves.
    std::fs::remove_file(root.join("src/doomed.rs")).expect("delete indexed file");
    let removed = server
        .dispatch_tool_for_tests("get_file_content", json!({ "path": "src/doomed.rs" }))
        .await;
    assert!(
        !removed.contains("doomed_anchor"),
        "a deleted file's bytes must not be served from a stale generation:\n{removed}"
    );
    let after_removal = fence_handle.publication_fence();
    assert!(
        after_removal.content_generation > after_refresh.content_generation,
        "the observer's confirmed-absent removal must publish \
         (after refresh {after_refresh:?}, after removal {after_removal:?})"
    );

    // And with the removal already published, the identical read is again a
    // pure failed observation: no new evidence, no movement.
    let settled = server
        .dispatch_tool_for_tests("get_file_content", json!({ "path": "src/doomed.rs" }))
        .await;
    assert!(!settled.contains("doomed_anchor"), "{settled}");
    assert_eq!(
        fence_handle.publication_fence(),
        after_removal,
        "re-reading an already-removed path yields no evidence and must not republish"
    );
}

// ── D16: the structured activation boundary, adjudicated ───────────────────

/// Carried Slice 3 residual: the daemon evidence header was "ancillary
/// metadata, not a transaction". The adjudicated Slice 4 boundary is
/// per-response and structured: the typed evidence receipt names the exact
/// immutable publication that rendered the body — captured once at the
/// handler's boundary, held as DATA in the dispatch scope, and therefore
/// immune to publications that land between the body render and the receipt
/// attach. Cross-process atomicity under arbitrary concurrent publication is
/// deliberately NOT claimed; what is claimed is that no response can pair one
/// publication's body with another publication's receipt.
#[tokio::test]
async fn the_evidence_receipt_names_the_publication_that_rendered_the_body() {
    let dir = tempfile::tempdir().expect("root");
    let root = dir.path();
    write_fixture(
        root,
        &[(
            "src/lib.rs",
            "pub fn boundary_anchor() -> u64 {\n    1\n}\n",
        )],
    );
    backdate(&root.join("src/lib.rs"), 1_000_000_001);
    let (observer, server) = server_with_shared(root);
    let seeded = observer.published_generation().publication_generation;

    // The source moves before the call; the handler's own freshen republishes
    // and renders the NEW bytes, so the rendering publication is NOT the one
    // current at dispatch entry.
    std::fs::write(
        root.join("src/lib.rs"),
        "pub fn boundary_anchor() -> u64 {\n    2\n}\n",
    )
    .expect("mutate source");
    backdate(&root.join("src/lib.rs"), 1_000_000_002);

    let (body, rendered_publication, receipt) =
        symforge::protocol::result_status::with_project_evidence_scope(None, async {
            let body = server
                .dispatch_tool_for_tests(
                    "get_file_content",
                    json!({ "path": "src/lib.rs", "force_refresh": true }),
                )
                .await;
            let rendered_publication = observer.published_generation().publication_generation;
            // A publication lands AFTER the body render but BEFORE the receipt
            // would be attached — the deterministic form of "arbitrary
            // concurrent publication" at this boundary.
            std::fs::write(
                root.join("src/lib.rs"),
                "pub fn boundary_anchor() -> u64 {\n    3\n}\n",
            )
            .expect("late mutate");
            backdate(&root.join("src/lib.rs"), 1_000_000_003);
            let late = symforge::live_index::single_file::update_file_from_disk(
                &observer,
                root,
                "src/lib.rs",
            );
            let late = format!("{late:?}");
            assert!(
                late.contains("Reindexed"),
                "control: the late publication must actually land: {late}"
            );
            let receipt = symforge::protocol::result_status::current_project_evidence();
            (body, rendered_publication, receipt)
        })
        .await;

    assert!(
        body.contains("    2"),
        "the body must render the in-call republished bytes:\n{body}"
    );
    assert!(
        rendered_publication > seeded,
        "control: the in-call freshen really moved the publication \
         (seeded {seeded}, rendered {rendered_publication})"
    );
    let receipt = receipt.expect("a rendered read must record a typed evidence receipt");

    // THE BOUNDARY, both directions: the receipt is the rendering publication
    // — not the pre-dispatch seed (stale receipt on fresh body), and not the
    // later publication that raced in before attach (fresh receipt on a body
    // it never rendered). Either pairing would be the half-published response.
    assert_eq!(
        receipt.generation, rendered_publication,
        "the receipt must name the publication that rendered the body"
    );
    let moved = observer.published_generation().publication_generation;
    assert!(
        moved > rendered_publication,
        "control: the post-render publication really superseded the rendering one"
    );

    // The wire form is TYPED: the receipt round-trips through its serialized
    // JSON as the same typed value. The recorded D16 gap named an UNTYPED
    // wire `_meta`; the boundary is a typed struct on both sides of the wire.
    let wire = serde_json::to_value(&receipt).expect("receipt serializes");
    let parsed: symforge::protocol::result_status::ProjectEvidence =
        serde_json::from_value(wire).expect("the wire value parses back as the typed receipt");
    assert_eq!(
        parsed, receipt,
        "the evidence receipt must survive the wire as the same typed value"
    );
}
