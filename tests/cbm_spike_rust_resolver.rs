#![cfg(feature = "cbm-spike")]
//! SP-0C — Rust resolver benchmark falsifier (Program 015).
//!
//! Planning artifact only (`#[ignore]`). Run:
//! `cargo test --test cbm_spike_rust_resolver -- --ignored --test-threads=1`
//!
//! GO: >= 60% verdict accuracy on `tests/fixtures/cbm_resolver_rust/` (S0);
//! 80% is the S3 target. See the fixture README for the metric definition and
//! `specs/015-cbm-capability-ports/planning/sprint-0-spike-spec.md`.

use std::collections::HashMap;
use std::path::PathBuf;

use serde_json::Value;
use symforge::parsing::resolver::{ResolvedCall, ResolverStrategy, resolve_rust_source};

fn strategy_label(s: ResolverStrategy) -> &'static str {
    match s {
        ResolverStrategy::SameFile => "same_file",
        ResolverStrategy::Import => "import",
        ResolverStrategy::Unresolved => "unresolved",
    }
}

#[test]
#[ignore = "015 S0 spike — planning falsifier, run with --ignored"]
fn cbm_spike_rust_resolver_fixture_pass_rate() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cbm_resolver_rust");
    let manifest_raw =
        std::fs::read_to_string(dir.join("manifest.json")).expect("read manifest.json");
    let manifest: Value = serde_json::from_str(&manifest_raw).expect("parse manifest.json");
    let fixtures = manifest["fixtures"].as_array().expect("fixtures array");

    let mut total = 0usize;
    let mut correct = 0usize;
    let mut in_scope_total = 0usize;
    let mut in_scope_correct = 0usize;
    let mut not_found: Vec<String> = Vec::new();
    let mut wrong: Vec<String> = Vec::new();

    for fx in fixtures {
        let file = fx["file"].as_str().expect("file");
        let source = std::fs::read_to_string(dir.join(file)).expect("read fixture source");
        let resolved = resolve_rust_source(&source);

        let mut by_key: HashMap<(&str, u32), &ResolvedCall> = HashMap::new();
        for call in &resolved {
            by_key.insert((call.name.as_str(), call.line), call);
        }

        for case in fx["cases"].as_array().expect("cases array") {
            let name = case["name"].as_str().expect("case name");
            let line = case["line"].as_u64().expect("case line") as u32;
            let expected_strategy = case["expected_strategy"]
                .as_str()
                .expect("expected_strategy");
            let expected_callee = case["expected_callee"].as_str(); // None when JSON null
            let in_scope = expected_strategy != "unresolved";

            total += 1;
            if in_scope {
                in_scope_total += 1;
            }

            let Some(call) = by_key.get(&(name, line)) else {
                not_found.push(format!("{file}: {name}@{line} not produced by resolver"));
                continue;
            };

            let got_strategy = strategy_label(call.strategy);
            let got_callee = call.callee_qname.as_deref();
            let hit = got_strategy == expected_strategy && got_callee == expected_callee;

            if hit {
                correct += 1;
                if in_scope {
                    in_scope_correct += 1;
                }
            } else {
                wrong.push(format!(
                    "{file}: {name}@{line} got ({got_strategy}, {got_callee:?}) want ({expected_strategy}, {expected_callee:?})"
                ));
            }
        }
    }

    let verdict_pct = 100.0 * correct as f64 / total as f64;
    let recall_pct = if in_scope_total == 0 {
        0.0
    } else {
        100.0 * in_scope_correct as f64 / in_scope_total as f64
    };

    eprintln!(
        "SP-0C resolver: verdict {correct}/{total} = {verdict_pct:.1}% | in-scope callee recall {in_scope_correct}/{in_scope_total} = {recall_pct:.1}%"
    );
    for w in &wrong {
        eprintln!("  miss: {w}");
    }
    for nf in &not_found {
        eprintln!("  NOT FOUND: {nf}");
    }

    assert!(total > 0, "manifest produced no cases");
    assert!(
        not_found.is_empty(),
        "manifest/line drift — these cases were not produced by the resolver: {not_found:?}"
    );
    assert!(
        verdict_pct >= 60.0,
        "SP-0C NO-GO: verdict accuracy {verdict_pct:.1}% < 60% — fallback: same-file-only v1, defer cross-file to S3.1"
    );
}
