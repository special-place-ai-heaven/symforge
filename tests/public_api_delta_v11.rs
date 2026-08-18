//! Feature 020 V11, T048 — the dark wrap table and the export delta, RED
//! before `public_api.rs` exists.
//!
//! One oracle remains here: it recomputes the export delta from
//! `public-api-v11.json` minus the live census and compares it to the
//! checked-in closed JSON — with per-atom D12/D13 obligations taken from the
//! wrap table, never from path identity, so a 1:1 re-export of internals
//! cannot rubber-stamp itself. The companion wrapper contract-shape oracle
//! (`dark_wrappers_match_contract_shapes`) moved in-crate to
//! `src/index_lifecycle/public_api.rs::dark_wrapper_oracles` at the start of
//! the Slice 4 activation cut, so `refusal_probe_for_test` could tighten to
//! `all(test, feature = "server")` and stop shipping in the release binary.

#![cfg(feature = "server")]

use std::collections::BTreeSet;

use symforge::live_index::index_lifecycle::public_api;

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
    // Whitespace-tolerant, aligned with the checker's regex rather than a
    // literal prefix (the census-parser divergence minor).
    let live_mods: BTreeSet<String> = lib
        .lines()
        .filter_map(|line| {
            let mut words = line.split_whitespace();
            if words.next() != Some("pub") || words.next() != Some("mod") {
                return None;
            }
            let name = words.next()?.strip_suffix(';')?;
            if words.next().is_some() || name.is_empty() {
                return None;
            }
            Some(format!("symforge::{name}"))
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

    // C7 ruling: `verbatim-reexport` covers EXACTLY the three enums minted
    // contract-verbatim in lifecycle_identity, and the claim is checked
    // against the module's SOURCE — an actual `pub use` — never trusted as a
    // self-report in the table.
    let verbatim: BTreeSet<&str> = table
        .iter()
        .filter(|e| e.obligation == "verbatim-reexport")
        .map(|e| e.atom)
        .collect();
    assert_eq!(
        verbatim,
        [
            "symforge::embed::OperationKind",
            "symforge::embed::RetryAdvice",
            "symforge::embed::SourceRefusalKind",
        ]
        .into_iter()
        .collect::<BTreeSet<_>>(),
        "the third vocabulary word covers exactly the contract-verbatim enums"
    );
    let module_source = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/index_lifecycle/public_api.rs"
    ))
    .expect("boundary module source");
    let reexport_line = module_source
        .lines()
        .find(|line| {
            line.trim_start()
                .starts_with("pub use crate::lifecycle_identity::")
        })
        .expect("the nameability re-export exists in the boundary module");
    for name in ["OperationKind", "RetryAdvice", "SourceRefusalKind"] {
        assert!(
            reexport_line.contains(name),
            "the re-export names {name}: {reexport_line}"
        );
    }

    // The checked-in delta equals the recomputation. The comparison binds
    // the PRE-WRITE checked-in content (C14 ruling): a regeneration run on a
    // stale artifact FAILS while writing the fix, and the rerun WITHOUT the
    // opt-in is the verification — never a write-then-compare tautology.
    let checked_in_text = std::fs::read_to_string(delta_path).expect("delta file exists");
    let recomputed_text = public_api::render_export_delta(&contract_text, &lib);
    if std::env::var("SYMFORGE_WRITE_DELTA").as_deref() == Ok("1") {
        std::fs::write(delta_path, &recomputed_text).expect("write regenerated delta");
    }
    let checked_in: serde_json::Value =
        serde_json::from_str(&checked_in_text).expect("delta is closed JSON");
    let recomputed: serde_json::Value =
        serde_json::from_str(&recomputed_text).expect("recomputed delta is closed JSON");
    assert_eq!(
        checked_in, recomputed,
        "the checked-in delta must equal the recomputation; regenerate \
         deliberately if this fails — a write-mode run asserts against the \
         PRE-write content, so it reports the drift it just repaired"
    );

    // C14's subtraction, pinned INDEPENDENTLY of the renderer: this leg's
    // own contract atoms minus this leg's own census must equal the
    // artifact's field — exact match, so V10's live `pub mod embed` must
    // never launder the 60 missing embed item atoms out of the delta.
    let expected_minus: Vec<&String> = atoms
        .iter()
        .filter(|atom| !live_mods.contains(*atom))
        .collect();
    let recorded_minus: Vec<&str> = recomputed["introduced_minus_live"]
        .as_array()
        .expect("introduced_minus_live array")
        .iter()
        .map(|value| value.as_str().expect("atom string"))
        .collect();
    assert_eq!(
        recorded_minus,
        expected_minus
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>(),
        "the artifact's subtraction must equal an independent recomputation"
    );
}
