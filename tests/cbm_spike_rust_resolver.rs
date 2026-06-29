//! SP-0C — Rust resolver benchmark falsifier (Program 015).
//!
//! Planning artifact only (`#[ignore]`). Run after C-S0-004:
//! `cargo test cbm_spike_rust_resolver -- --ignored --test-threads=1`
//!
//! GO: ≥60% on `tests/fixtures/cbm_resolver_rust/` (S0); ≥80% at S3.
//! See `specs/015-cbm-capability-ports/planning/sprint-0-spike-spec.md`.

#[test]
#[ignore = "015 S0 spike — implement in C-S0-004"]
fn cbm_spike_rust_resolver_fixture_pass_rate() {
    unimplemented!("resolver vs manifest.json cases");
}
