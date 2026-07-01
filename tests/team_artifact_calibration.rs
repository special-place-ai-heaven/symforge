//! Team artifact real-repo-scale calibration (Program 015, C-S1A-005).
//!
//! Promoted from the SP-0B spike falsifier (`research.md` § SP-0B, GO
//! 2026-06-30: 607/607 content_hash byte-exact, 3.61x ratio) into a permanent
//! ignored regression check, mirroring the existing `calibrate_current_repo_smoke`
//! / `test_load_perf_1000_files` real-repo smoke coverage (see
//! `tests/coupling_calibration.rs`, `tests/live_index_integration.rs`). A small
//! synthetic fixture (`tests/team_artifact.rs`) cannot exercise the same
//! file-count / language-mix / compression-ratio realism this repo's own
//! index provides, so this check earns its keep as a standing calibration
//! signal rather than duplicate coverage of `tests/team_artifact.rs`'s
//! contract-level assertions.
//!
//! Uses only in-memory round-trip helpers (`persist::artifact_round_trip_report`,
//! `persist::compress_snapshot`, `persist::decompress_artifact_bytes`) — no
//! disk I/O, so it never touches this live repo's own `.symforge/` directory.
//!
//! Run: `cargo test --test team_artifact_calibration -- --ignored --test-threads=1`

use std::path::PathBuf;

use symforge::live_index::{LiveIndex, persist};

#[test]
#[ignore = "real-repo-scale calibration — run with --ignored"]
fn team_artifact_real_repo_round_trip_calibration() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let shared = LiveIndex::load(&root).expect("load symforge index");
    let index = shared.read();

    let report = persist::artifact_round_trip_report(&index, &root).expect("zstd round trip");
    let ratio = report.raw_bytes as f64 / report.compressed_bytes.max(1) as f64;
    eprintln!(
        "team artifact calibration: files={} matched={} mismatches={} raw={}B zstd={}B ratio={ratio:.2}x",
        report.files,
        report.matched,
        report.mismatches.len(),
        report.raw_bytes,
        report.compressed_bytes,
    );

    assert!(report.files > 0, "index should contain files");
    assert!(
        report.mismatches.is_empty(),
        "content_hash mismatches after round-trip: {:?}",
        report.mismatches
    );
    assert_eq!(
        report.matched, report.files,
        "every per-file content_hash must survive the round-trip"
    );

    // Error catalog: corrupt compressed bytes -> import error, no partial state.
    let good = persist::compress_snapshot(&index, &root).expect("compress snapshot");
    let truncated = &good[..good.len() / 2];
    assert!(
        persist::decompress_artifact_bytes(truncated).is_err(),
        "truncated zstd frame must error, not partially import"
    );
    assert!(
        persist::decompress_artifact_bytes(b"not a zstd stream at all").is_err(),
        "garbage bytes must error"
    );
}
