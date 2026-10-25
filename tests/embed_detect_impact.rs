#![cfg(feature = "embed")]

use std::{
    fs,
    path::Path,
    time::{Duration, Instant},
};

use symforge::embed::parity::detect_impact::{
    DetectImpactRequest, ImpactEvidenceStatus, ImpactResolution, ImpactScope,
};
use symforge::embed::parity::host::OperationControl;
use symforge::embed::parity::source_options::GitPreparationOptions;
use symforge::embed::parity::{QueryLimits, QueryOutput, QueryRefusalKind, QueryRequest};
use symforge::embed::{EmbeddedSourceSpec, ProcessIndexRuntime, SourceRuntimePhase};

fn commit(repo: &git2::Repository, paths: &[&str]) {
    let mut index = repo.index().unwrap();
    for path in paths {
        index.add_path(Path::new(path)).unwrap();
    }
    index.write().unwrap();
    let tree_id = index.write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    let signature = git2::Signature::now("Fixture", "fixture@example.invalid").unwrap();
    let previous = repo.head().ok().and_then(|head| head.peel_to_commit().ok());
    let parents: Vec<_> = previous.iter().collect();
    repo.commit(
        Some("HEAD"),
        &signature,
        &signature,
        "fixture",
        &tree,
        &parents,
    )
    .unwrap();
}

#[test]
fn bound_detect_impact_preserves_body_delta_blast_and_full_risk_counts() {
    let root = tempfile::tempdir().unwrap();
    let scratch = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("src")).unwrap();
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cbm_impact");
    let paths = [
        "Cargo.toml",
        "src/lib.rs",
        "src/a.rs",
        "src/b.rs",
        "src/c.rs",
        "src/main.rs",
    ];
    for path in &paths {
        fs::copy(fixture.join(path), root.path().join(path)).unwrap();
    }
    let repo = git2::Repository::init(root.path()).unwrap();
    commit(&repo, &paths);
    fs::write(
        root.path().join("src/a.rs"),
        "use cbm_impact_fixture::core;\n\npub fn call_a() -> u32 {\n    core() + 1\n}\n",
    )
    .unwrap();
    commit(&repo, &["src/a.rs"]);

    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let handle = runtime
        .open_embedded_source(EmbeddedSourceSpec::current_worktree(
            root.path().to_path_buf(),
        ))
        .unwrap();
    let until = Instant::now() + Duration::from_secs(20);
    while handle.runtime_view().phase != SourceRuntimePhase::Current {
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(5));
    }
    handle
        .prepare_git_view(
            &GitPreparationOptions::new(scratch.path().to_path_buf(), 64 * 1024 * 1024, 5_000),
            &OperationControl::new(Duration::from_secs(20)).unwrap(),
        )
        .unwrap();

    let result = handle
        .query(
            &QueryRequest::DetectImpact(DetectImpactRequest {
                since: Some("HEAD~1".to_owned()),
                depth: 2,
                scope: ImpactScope::Symbols,
                ..Default::default()
            }),
            QueryLimits::default(),
        )
        .unwrap();
    let QueryOutput::DetectImpact(impact) = result.value() else {
        panic!("typed detect-impact result expected")
    };
    assert_eq!(impact.changed_files, ["src/a.rs"]);
    assert_eq!(impact.changed_symbols.len(), 1);
    assert_eq!(impact.changed_symbols[0].name, "call_a");
    assert_eq!(impact.blast_radius.len(), 1);
    assert_eq!(impact.blast_radius[0].symbol, "src/main.rs::main");
    assert_eq!(impact.blast_radius[0].hop, 1);
    assert_eq!(impact.blast_radius[0].risk, "critical");
    assert_eq!(
        impact.blast_radius[0].evidence_status,
        ImpactEvidenceStatus::Available
    );
    let evidence = impact.blast_radius[0]
        .evidence
        .as_ref()
        .expect("blast row has its call chain");
    assert_eq!(evidence.len(), 1);
    assert_eq!(evidence[0].caller.name, "main");
    assert_eq!(evidence[0].callee.name, "call_a");
    assert_eq!(evidence[0].call_site.path, "src/main.rs");
    assert!(evidence[0].call_site.byte_range[0] < evidence[0].call_site.byte_range[1]);
    assert_eq!(evidence[0].call_site.line_range, [7, 8]);
    assert_eq!(evidence[0].resolution, ImpactResolution::UniqueName);
    assert_eq!(impact.risk_summary.critical, 1);
    assert_eq!(impact.pagination.blast_radius.total, 1);
    assert!(!impact.pagination.blast_radius.truncated);

    let budget_probe = QueryRequest::DetectImpact(DetectImpactRequest {
        since: Some("HEAD~1".to_owned()),
        depth: 2,
        scope: ImpactScope::Symbols,
        ..Default::default()
    });
    let mut lower_bytes = 1u32;
    let mut upper_bytes = 2_048u32;
    let mut truncated_probe = None;
    while lower_bytes <= upper_bytes {
        let max_bytes = lower_bytes + (upper_bytes - lower_bytes) / 2;
        match handle.query(
            &budget_probe,
            QueryLimits {
                max_results: QueryLimits::default().max_results,
                max_bytes,
            },
        ) {
            Ok(candidate) => {
                let is_truncated = match candidate.value() {
                    QueryOutput::DetectImpact(impact) => impact
                        .blast_radius
                        .first()
                        .is_some_and(|row| row.evidence_status == ImpactEvidenceStatus::Truncated),
                    _ => false,
                };
                if is_truncated {
                    truncated_probe = Some((candidate, max_bytes));
                }
                if max_bytes == 1 {
                    break;
                }
                upper_bytes = max_bytes - 1;
            }
            Err(_) => lower_bytes = max_bytes + 1,
        }
    }
    let (low_budget_result, max_bytes) = truncated_probe
        .expect("a constrained budget fits the report while omitting its optional evidence chain");
    let QueryOutput::DetectImpact(low_budget_impact) = low_budget_result.value() else {
        panic!("typed detect-impact result expected with a constrained byte budget")
    };
    assert_eq!(
        low_budget_impact.blast_radius.len(),
        impact.blast_radius.len()
    );
    assert_eq!(low_budget_impact.risk_summary, impact.risk_summary);
    assert_eq!(
        low_budget_impact.pagination.blast_radius.total,
        impact.pagination.blast_radius.total
    );
    assert!(!low_budget_impact.pagination.blast_radius.truncated);
    assert_eq!(
        low_budget_impact.blast_radius[0].evidence_status,
        ImpactEvidenceStatus::Truncated
    );
    assert!(low_budget_impact.blast_radius[0].evidence.is_none());
    assert!(low_budget_result.usage().decoded_bytes <= u64::from(max_bytes));

    let files_result = handle
        .query(
            &QueryRequest::DetectImpact(DetectImpactRequest {
                since: Some("HEAD~1".to_owned()),
                depth: 2,
                scope: ImpactScope::Files,
                ..Default::default()
            }),
            QueryLimits::default(),
        )
        .unwrap();
    let QueryOutput::DetectImpact(files_impact) = files_result.value() else {
        panic!("typed file-scope detect-impact result expected")
    };
    assert_eq!(files_impact.blast_radius.len(), 1);
    assert_eq!(files_impact.blast_radius[0].symbol, "src/main.rs");
    let file_evidence = files_impact.blast_radius[0]
        .evidence
        .as_ref()
        .expect("file-scope representative retains its chain");
    assert_eq!(file_evidence.last().unwrap().caller.path, "src/main.rs");
    assert_eq!(file_evidence.last().unwrap().caller.name, "main");
    handle.close().unwrap();
}

#[test]
fn explicit_paths_impact_equals_union_of_per_file_results_and_refuses_outside_path() {
    use std::collections::BTreeSet;

    let root = tempfile::tempdir().unwrap();
    let scratch = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("src")).unwrap();
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cbm_impact");
    let paths = [
        "Cargo.toml",
        "src/lib.rs",
        "src/a.rs",
        "src/b.rs",
        "src/c.rs",
        "src/main.rs",
    ];
    for path in &paths {
        fs::copy(fixture.join(path), root.path().join(path)).unwrap();
    }
    let repo = git2::Repository::init(root.path()).unwrap();
    commit(&repo, &paths);

    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let handle = runtime
        .open_embedded_source(EmbeddedSourceSpec::current_worktree(
            root.path().to_path_buf(),
        ))
        .unwrap();
    let until = Instant::now() + Duration::from_secs(20);
    while handle.runtime_view().phase != SourceRuntimePhase::Current {
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(5));
    }
    handle
        .prepare_git_view(
            &GitPreparationOptions::new(scratch.path().to_path_buf(), 64 * 1024 * 1024, 5_000),
            &OperationControl::new(Duration::from_secs(20)).unwrap(),
        )
        .unwrap();

    let query_paths = |explicit: Vec<&str>| {
        let result = handle
            .query(
                &QueryRequest::DetectImpact(DetectImpactRequest {
                    paths: Some(explicit.into_iter().map(String::from).collect()),
                    ..Default::default()
                }),
                QueryLimits::default(),
            )
            .unwrap();
        let QueryOutput::DetectImpact(impact) = result.value() else {
            panic!("typed detect-impact result expected")
        };
        impact.clone()
    };

    let impact_a = query_paths(vec!["src/a.rs"]);
    let impact_b = query_paths(vec!["src/b.rs"]);
    let impact_ab = query_paths(vec!["src/a.rs", "src/b.rs"]);

    assert_eq!(impact_ab.changed_files, ["src/a.rs", "src/b.rs"]);
    let union_files: BTreeSet<_> = impact_a
        .changed_files
        .iter()
        .chain(impact_b.changed_files.iter())
        .cloned()
        .collect();
    let ab_files: BTreeSet<_> = impact_ab.changed_files.iter().cloned().collect();
    assert_eq!(ab_files, union_files);

    let union_symbols: BTreeSet<_> = impact_a
        .changed_symbols
        .iter()
        .chain(impact_b.changed_symbols.iter())
        .map(|s| (s.path.clone(), s.name.clone(), s.kind.clone()))
        .collect();
    let ab_symbols: BTreeSet<_> = impact_ab
        .changed_symbols
        .iter()
        .map(|s| (s.path.clone(), s.name.clone(), s.kind.clone()))
        .collect();
    assert!(!ab_symbols.is_empty());
    assert_eq!(ab_symbols, union_symbols);

    let union_blast: BTreeSet<_> = impact_a
        .blast_radius
        .iter()
        .chain(impact_b.blast_radius.iter())
        .map(|b| b.symbol.clone())
        .collect();
    let ab_blast: BTreeSet<_> = impact_ab
        .blast_radius
        .iter()
        .map(|b| b.symbol.clone())
        .collect();
    assert!(!ab_blast.is_empty());
    assert_eq!(ab_blast, union_blast);

    for outside in [
        "../outside.rs".to_owned(),
        scratch
            .path()
            .join("outside.rs")
            .to_string_lossy()
            .into_owned(),
    ] {
        let refusal = handle
            .query(
                &QueryRequest::DetectImpact(DetectImpactRequest {
                    paths: Some(vec![outside]),
                    ..Default::default()
                }),
                QueryLimits::default(),
            )
            .unwrap_err();
        assert_eq!(refusal.kind(), QueryRefusalKind::InvalidRequest);
    }

    handle.close().unwrap();
}
