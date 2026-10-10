#![cfg(feature = "embed")]
use std::{
    fs,
    sync::Arc,
    time::{Duration, Instant},
};
use symforge::embed::parity::read::FileContentRequest;
use symforge::embed::parity::session::{
    CacheDisposition, SessionCacheLimits, SessionCreationRefusal,
};
use symforge::embed::parity::{QueryLimits, QueryRequest};
use symforge::embed::{
    EmbeddedSourceHandle, EmbeddedSourceSpec, ProcessIndexRuntime, SourceRuntimePhase,
};
fn open(runtime: &ProcessIndexRuntime, path: &std::path::Path) -> EmbeddedSourceHandle {
    let handle = runtime
        .open_embedded_source(EmbeddedSourceSpec::current_worktree(path.to_path_buf()))
        .unwrap();
    let until = Instant::now() + Duration::from_secs(20);
    while handle.runtime_view().phase != SourceRuntimePhase::Current {
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(5));
    }
    handle
}
#[test]
fn host_cache_limits_are_effective_and_reset_cannot_increase_them() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("lib.rs"), "pub fn content() {}\n").unwrap();
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let handle = open(&runtime, root.path());
    let limits = SessionCacheLimits {
        max_bytes: 32,
        max_entries: 1,
    };
    let session = handle.new_query_session_with_limits(limits).unwrap();
    assert_eq!(session.cache_limits(), limits);
    let claim = handle
        .query_with_session(
            &QueryRequest::FileContent(FileContentRequest {
                path: "lib.rs".into(),
                ..Default::default()
            }),
            QueryLimits::default(),
            &session,
            None,
        )
        .unwrap();
    assert_eq!(claim.cache_disposition(), CacheDisposition::SkippedCapacity);
    assert!(claim.retrieve_handle().is_none());
    let status = session.cache_status();
    assert!(status.resident_bytes <= 32);
    assert!(status.resident_entries <= 1);
    let identity = session.identity().to_owned();
    let before = session.revision();
    let reset = session.reset_with_receipt();
    assert_eq!(reset.identity, identity);
    assert_eq!(reset.revision_before, before);
    assert_eq!(reset.revision_after, before + 1);
    assert_eq!(session.cache_limits(), limits);
    assert_eq!(session.cache_status().resident_bytes, 0);
    assert!(matches!(
        handle.new_query_session_with_limits(SessionCacheLimits {
            max_bytes: 33 * 1024 * 1024,
            max_entries: 256
        }),
        Err(SessionCreationRefusal::InvalidLimits)
    ));
}
#[test]
fn concurrent_queries_and_resets_have_linear_session_revisions() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("lib.rs"), "pub fn content() {}\n").unwrap();
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let handle = Arc::new(open(&runtime, root.path()));
    let session = Arc::new(handle.new_query_session().unwrap());
    let isolated = handle.new_query_session().unwrap();
    assert_ne!(session.identity(), isolated.identity());
    let start = Arc::new(std::sync::Barrier::new(3));
    let query_thread = {
        let handle = Arc::clone(&handle);
        let session = Arc::clone(&session);
        let start = Arc::clone(&start);
        std::thread::spawn(move || {
            start.wait();
            (0..30)
                .map(|_| {
                    let claim = handle
                        .query_with_session(
                            &QueryRequest::SearchSymbols {
                                query: Some("content".into()),
                                path_prefix: None,
                                kind: None,
                                include_tests: true,
                            },
                            QueryLimits::default(),
                            &session,
                            None,
                        )
                        .unwrap();
                    claim.session_evidence().unwrap().clone()
                })
                .collect::<Vec<_>>()
        })
    };
    let reset_thread = {
        let session = Arc::clone(&session);
        let start = Arc::clone(&start);
        std::thread::spawn(move || {
            start.wait();
            (0..30)
                .map(|_| session.reset_with_receipt())
                .collect::<Vec<_>>()
        })
    };
    start.wait();
    let queries = query_thread.join().unwrap();
    let resets = reset_thread.join().unwrap();
    let mut intervals = queries
        .iter()
        .map(|e| (e.revision_before, e.revision_after))
        .chain(resets.iter().map(|e| (e.revision_before, e.revision_after)))
        .collect::<Vec<_>>();
    intervals.sort_unstable();
    assert_eq!(intervals.len(), 60);
    for (i, (before, after)) in intervals.iter().enumerate() {
        assert_eq!((*before, *after), (i as u64, i as u64 + 1));
    }
    assert_eq!(session.revision(), 60);
    assert_eq!(isolated.revision(), 0);
}
