//! SP-0A — Graph BFS latency falsifier (Program 015).
//!
//! Planning artifact only (`#[ignore]`). Run after C-S0-002:
//! `cargo test cbm_spike_graph_bfs -- --ignored --test-threads=1`
//!
//! GO: p95 inbound BFS depth-5 <200ms on symforge repo index.
//! See `specs/015-cbm-capability-ports/planning/sprint-0-spike-spec.md`.

#[test]
#[ignore = "015 S0 spike — implement in C-S0-002"]
fn cbm_spike_graph_bfs_p95_under_threshold() {
    unimplemented!("GraphProjection BFS benchmark");
}
