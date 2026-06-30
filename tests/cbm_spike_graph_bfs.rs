#![cfg(feature = "cbm-spike")]
//! SP-0A — Graph BFS latency falsifier (Program 015).
//!
//! Planning artifact only (`#[ignore]`). Run:
//! `cargo test --test cbm_spike_graph_bfs -- --ignored --test-threads=1`
//!
//! GO: p95 inbound BFS depth-5 < 200ms on the symforge repo index
//! (target < 50ms local). Empty index -> empty result, never panic.
//! See `specs/015-cbm-capability-ports/planning/sprint-0-spike-spec.md`.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use symforge::domain::SymbolKind;
use symforge::live_index::LiveIndex;
use symforge::live_index::graph::{GraphProjection, SymbolId};

#[test]
#[ignore = "015 S0 spike — planning falsifier, run with --ignored"]
fn cbm_spike_graph_bfs_p95_under_threshold() {
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
        "SP-0A BFS depth5: nodes={} edges={} build={}ms roots={} samples={} p50={:?} p95={:?} max={:?}",
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
        "SP-0A NO-GO: p95={p95:?} exceeds 200ms GO bar"
    );
}
