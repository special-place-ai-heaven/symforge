//! Feature 020 V11, T048 — the dark wrap table and the export delta, RED
//! before `public_api.rs` exists.
//!
//! Two oracles. The first pins the runtime-checkable contract shapes on the
//! dark wrappers: kind-prefixed identity strings stored at wrap time, the
//! `evidence-absent` sentinel that the identity renderer can never emit,
//! `Display` + `Error` on the refusal wrapper, and the honest-refusal dark
//! behavior of the four V11 handle methods. The second recomputes the export
//! delta from `public-api-v11.json` minus the live census and compares it to
//! the checked-in closed JSON — with per-atom D12/D13 obligations taken from
//! the wrap table, never from path identity, so a 1:1 re-export of internals
//! cannot rubber-stamp itself.

#![cfg(feature = "server")]

use std::collections::BTreeSet;

use symforge::live_index::index_lifecycle::embedded::EmbeddedSourceFactory;
use symforge::live_index::index_lifecycle::public_api::{
    self, EVIDENCE_ABSENT, EmbedOperationReceipt, EmbedSourceRefusal,
    PROVISIONAL_ACQUIRE_PROCESS_BYTES, ProcessRuntimeApi, SymbolSearchRequest, TextSearchRequest,
};
use symforge::live_index::index_lifecycle::registry::ProjectKey;

// ── The wrappers match the contract shapes ─────────────────────────────────

#[test]
fn dark_wrappers_match_contract_shapes() {
    // acquire takes NO arguments per the atom and delegates to incarnate with
    // the NAMED provisional constant — never live V10 env policy.
    let runtime = ProcessRuntimeApi::acquire().expect("the dark acquisition admits");
    // The provisional budget is a named constant; clippy rightly refuses a
    // constant assertion, so the pin is that acquire() DELEGATED with it —
    // the runtime exists — and the constant's value lives in the D-ledger.
    let _ = PROVISIONAL_ACQUIRE_PROCESS_BYTES;

    // The refusal wrapper: kind-prefixed identity strings, stored at wrap
    // time; Display and Error implemented; the sentinel reserved for
    // refusals that examined nothing.
    let refusal: EmbedSourceRefusal = runtime
        .refusal_probe_for_test()
        .expect_err("the probe yields the wrapper's honest dark refusal");
    let evidence = refusal.evidence_identity();
    assert_eq!(
        evidence, EVIDENCE_ABSENT,
        "a refusal that examined no authority renders the closed sentinel"
    );
    assert!(
        !evidence.starts_with("auth-"),
        "the sentinel is a token the identity renderer cannot emit"
    );
    let operation: &EmbedOperationReceipt = refusal.operation();
    assert!(
        operation.identity().starts_with("op-"),
        "operation identities are kind-prefixed, got {}",
        operation.identity()
    );
    assert!(
        operation
            .identity()
            .trim_start_matches("op-")
            .parse::<u64>()
            .is_ok(),
        "the prefix is followed by the counter digits"
    );
    let first = operation.identity().to_string();
    assert_eq!(
        operation.identity(),
        first,
        "the rendered string is STORED at wrap time — stable across calls"
    );
    // Display + Error are contract trait impls, exercised not just derived.
    let rendered = format!("{refusal}");
    assert!(
        rendered.contains("SourceUnavailable"),
        "Display names the refusal kind: {rendered}"
    );
    let _as_error: &dyn std::error::Error = &refusal;

    // The four V11 handle methods, under their contract shapes, refusing
    // honestly in the dark rather than fabricating empty results.
    let factory = EmbeddedSourceFactory::new();
    let handle = factory
        .open(ProjectKey::new("src-a"))
        .expect("open admits a fresh key");

    let view = handle.runtime_view();
    assert!(
        view.binding_identity.starts_with("source-"),
        "the view's binding identity is kind-prefixed: {}",
        view.binding_identity
    );
    assert!(
        view.current_publication_identity.is_none(),
        "a dark handle has NO publication; inventing one would be fabricated \
         completion"
    );
    assert_eq!(view.observer_epoch, 0, "no observer has been registered");

    let refusal = handle
        .search_symbols(&SymbolSearchRequest {
            query: Some("anchor".to_string()),
            path_prefix: None,
            limit: 10,
        })
        .expect_err("no generation is bound, so a symbol search refuses");
    assert_eq!(refusal.kind_name(), "SourceUnavailable");

    let refusal = handle
        .search_text(&TextSearchRequest {
            query: "anchor".to_string(),
            path_prefix: None,
            limit: 10,
            case_sensitive: false,
        })
        .expect_err("no generation is bound, so a text search refuses");
    assert_eq!(refusal.kind_name(), "SourceUnavailable");

    let refusal = handle
        .request_refresh()
        .expect_err("a dark refresh cannot run, so the ticket is refused honestly");
    assert_eq!(refusal.kind_name(), "SourceUnavailable");
}

// ── The delta is recomputed, never trusted ─────────────────────────────────

#[test]
fn export_delta_matches_frozen_contract_atoms() {
    // The checked-in artifact.
    let delta_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/docs/reviews/FEATURE-020-EXPORT-DELTA-v11.json"
    );
    // Recompute leg 1: the frozen atoms.
    let contract_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/specs/020-repository-knowledge-index/contracts/public-api-v11.json"
    );
    let contract_text = std::fs::read_to_string(contract_path).expect("contract exists");
    let contract: serde_json::Value =
        serde_json::from_str(&contract_text).expect("contract parses");
    let atoms: BTreeSet<String> = contract["migration_v10"]["introduced_v11_atoms"]
        .as_array()
        .expect("atoms array")
        .iter()
        .map(|a| a.as_str().expect("atom string").to_string())
        .collect();
    assert_eq!(atoms.len(), 64, "the frozen atom count");

    // Recompute leg 2: the LIVE census, by the same rule the checker uses —
    // pub mod lines in lib.rs. server_api must NOT appear: it is pub(crate),
    // flip-ready, invisible to the census until activation.
    let lib = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/lib.rs"))
        .expect("lib.rs");
    let live_mods: BTreeSet<String> = lib
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim_start();
            trimmed
                .strip_prefix("pub mod ")
                .and_then(|rest| rest.strip_suffix(';'))
                .map(|name| format!("symforge::{name}"))
        })
        .collect();
    assert!(
        !live_mods.contains("symforge::server_api"),
        "server_api stays pub-crate until activation flips one keyword"
    );

    // Recompute leg 3: the wrap table. Obligations come from HERE — the
    // module's own judgment of shape-vs-contract — never from path identity.
    let table = public_api::wrap_table();
    let table_atoms: BTreeSet<String> = table.iter().map(|e| e.atom.to_string()).collect();
    assert_eq!(
        table_atoms,
        atoms
            .iter()
            .filter(|a| a.split("::").count() <= 3)
            .cloned()
            .collect::<BTreeSet<_>>(),
        "the wrap table covers exactly the top-level introduced atoms"
    );
    assert!(
        table
            .iter()
            .filter(|e| e.atom.starts_with("symforge::embed::"))
            .all(|e| e.obligation != "direct-reexport"),
        "D12/D13: no embed atom may be satisfied by a 1:1 re-export of an internal path"
    );

    // The checked-in delta equals the recomputation. Regeneration is a
    // DELIBERATE act behind the same env opt-in pattern as the closure
    // census: the write happens, and the comparison below still runs.
    let recomputed = public_api::render_export_delta(&contract_text, &lib);
    if std::env::var("SYMFORGE_WRITE_DELTA").as_deref() == Ok("1") {
        std::fs::write(delta_path, &recomputed).expect("write regenerated delta");
    }
    let checked_in: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(delta_path).expect("delta file exists"))
            .expect("delta is closed JSON");
    let recomputed: serde_json::Value =
        serde_json::from_str(&recomputed).expect("recomputed delta is closed JSON");
    assert_eq!(
        checked_in, recomputed,
        "the checked-in delta must equal the recomputation from the frozen \
         contract minus the live census; regenerate deliberately if this fails"
    );
}
