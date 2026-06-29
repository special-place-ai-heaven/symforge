//! team artifact round-trip — Program 015 S1a (planning skeleton).
//!
//! Implement in C-S1A-005:
//! `cargo test team_artifact -- --ignored --test-threads=1`

#[test]
#[ignore = "015 S1a — implement in C-S1A-005"]
fn team_artifact_zstd_round_trip_preserves_content_hash() {
    unimplemented!("export → import → verify content_hash");
}

#[test]
#[ignore = "015 S1a — implement in C-S1A-005"]
fn team_artifact_corrupt_quarantines_without_partial_serve() {
    unimplemented!("bad zst → quarantine/artifacts/");
}
