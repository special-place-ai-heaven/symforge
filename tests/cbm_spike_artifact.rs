//! SP-0B — zstd artifact round-trip falsifier (Program 015).
//!
//! Planning artifact only (`#[ignore]`). Run after C-S0-003:
//! `cargo test cbm_spike_artifact -- --ignored --test-threads=1`
//!
//! GO: per-file content_hash unchanged after compress/decompress.
//! See `specs/015-cbm-capability-ports/planning/sprint-0-spike-spec.md`.

#[test]
#[ignore = "015 S0 spike — implement in C-S0-003"]
fn cbm_spike_artifact_hash_round_trip() {
    unimplemented!("zstd export/import hash verify");
}
