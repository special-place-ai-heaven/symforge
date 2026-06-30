#![cfg(feature = "cbm-spike")]
//! SP-0B — zstd artifact round-trip falsifier (Program 015).
//!
//! Planning artifact only (`#[ignore]`). Run:
//! `cargo test --test cbm_spike_artifact -- --ignored --test-threads=1`
//!
//! GO: per-file content_hash unchanged after compress/decompress.
//! Corrupt compressed bytes -> import error, no partial state.
//! See `specs/015-cbm-capability-ports/planning/sprint-0-spike-spec.md`.

use std::path::PathBuf;

use symforge::live_index::{LiveIndex, persist};

#[test]
#[ignore = "015 S0 spike — planning falsifier, run with --ignored"]
fn cbm_spike_artifact_hash_round_trip() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let shared = LiveIndex::load(&root).expect("load symforge index");
    let index = shared.read();

    let report = persist::spike_zstd_round_trip(&index, &root).expect("zstd round trip");
    let ratio = report.raw_bytes as f64 / report.compressed_bytes.max(1) as f64;
    eprintln!(
        "SP-0B artifact: files={} matched={} mismatches={} raw={}B zstd={}B ratio={ratio:.2}x",
        report.files,
        report.matched,
        report.mismatches.len(),
        report.raw_bytes,
        report.compressed_bytes,
    );

    assert!(report.files > 0, "index should contain files");
    assert!(
        report.mismatches.is_empty(),
        "SP-0B NO-GO: content_hash mismatches: {:?}",
        report.mismatches
    );
    assert_eq!(
        report.matched, report.files,
        "every per-file content_hash must survive the round-trip"
    );

    // Error catalog: corrupt compressed bytes -> import error, no partial state.
    let good = persist::spike_compress_snapshot(&index, &root).expect("compress snapshot");
    let truncated = &good[..good.len() / 2];
    assert!(
        persist::spike_import_compressed(truncated).is_err(),
        "truncated zstd frame must error, not partially import"
    );
    assert!(
        persist::spike_import_compressed(b"not a zstd stream at all").is_err(),
        "garbage bytes must error"
    );
}
