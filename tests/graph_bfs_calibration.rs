//! Graph BFS real-repo-scale latency calibration (Program 015, C-S1A-002).
//!
//! Promoted from the SP-0A spike falsifier (`sprint-0-spike-spec.md`, GO:
//! p95 inbound BFS depth-5 < 200ms on the symforge repo index) into a
//! permanent ignored perf-regression check once `graph.rs` graduated to
//! always-on production code backing `detect_impact` (see
//! `src/live_index/mod.rs`). Mirrors the sibling
//! `tests/team_artifact_calibration.rs` promotion and this repo's existing
//! `calibrate_current_repo_smoke` / `test_load_perf_1000_files` ignored
//! real-repo smoke coverage.
//!
//! It earns its keep beyond `tests/detect_impact.rs` (functional blast-radius
//! correctness on a tiny synthetic fixture) and `graph.rs`'s own unit tests:
//! only the real repo index provides the node/edge scale a p95 latency bound
//! can regress against. `#[ignore]` — run in scheduled/manual CI perf smoke:
//! `cargo test --test graph_bfs_calibration -- --ignored --test-threads=1`.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use symforge::domain::SymbolKind;
use symforge::live_index::LiveIndex;
use symforge::live_index::graph::{GraphProjection, SymbolId};

#[test]
#[ignore = "real-repo-scale calibration — run with --ignored"]
fn graph_bfs_real_repo_p95_calibration() {
    // Error catalog: empty index BFS must return empty, not panic.
    let empty = LiveIndex::empty();
    {
        let idx = empty.read();
        let graph = GraphProjection::from_index(&idx);
        assert_eq!(graph.node_count(), 0, "empty index has no nodes");
        let probe = SymbolId {
            path: "nope.rs".to_string(),
            name: "missing".to_string(),
            kind: SymbolKind::Function,
        };
        assert!(
            graph.inbound_bfs(&probe, 5, 10_000).is_empty(),
            "BFS over empty graph must be empty"
        );
    }

    // Real corpus: the symforge repo itself.
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let shared = LiveIndex::load(&root).expect("load symforge index");
    let index = shared.read();

    let build_start = Instant::now();
    let graph = GraphProjection::from_index(&index);
    let build_ms = build_start.elapsed().as_millis();

    let starts = graph.top_inbound_targets(20);
    assert!(
        !starts.is_empty(),
        "symforge index should yield inbound Call edges"
    );

    // Warm up (parser caches, allocator) before timing.
    for s in &starts {
        std::hint::black_box(graph.inbound_bfs(s, 5, 100_000));
    }

    let iters = 50;
    let mut samples: Vec<Duration> = Vec::with_capacity(iters * starts.len());
    for _ in 0..iters {
        for s in &starts {
            let t = Instant::now();
            let reached = graph.inbound_bfs(s, 5, 100_000);
            samples.push(t.elapsed());
            std::hint::black_box(reached);
        }
    }
    samples.sort();

    let p95 = samples[(samples.len() * 95 / 100).min(samples.len() - 1)];
    let p50 = samples[samples.len() / 2];
    let max = *samples.last().expect("samples non-empty");

    eprintln!(
        "graph BFS depth5 calibration: nodes={} edges={} build={}ms roots={} samples={} p50={:?} p95={:?} max={:?}",
        graph.node_count(),
        graph.edge_count(),
        build_ms,
        starts.len(),
        samples.len(),
        p50,
        p95,
        max,
    );

    assert!(
        p95 < Duration::from_millis(200),
        "graph BFS perf regression: p95={p95:?} exceeds the 200ms bound"
    );
}
