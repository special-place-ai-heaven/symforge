// Server-only integration test: depends on a `#[cfg(feature = "server")]`
// module (protocol/daemon/cli/sidecar/watcher/analytics). Gating the whole
// file keeps `--no-default-features --features embed --all-targets` compiling.
#![cfg(feature = "server")]

use std::sync::mpsc::{Receiver, Sender};
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow};
use symforge::analytics::{
    AnalyticsConfig, AnalyticsEnqueueOutcome, AnalyticsObservation, AnalyticsQueueStatus,
    AnalyticsRecorder, AnalyticsScope, AnalyticsStore, AnalyticsSurface, AnalyticsWriteOutcome,
    AnalyticsWriter, MAX_ANALYTICS_QUEUE_ERROR_BYTES, MAX_TOOL_NAME_BYTES, SqliteAnalyticsStore,
};
use symforge::protocol::result_status::OutcomeClass;

fn sample_observation(tool_name: impl Into<String>) -> AnalyticsObservation {
    AnalyticsObservation::new(
        tool_name,
        AnalyticsSurface::Tool,
        AnalyticsScope::Session,
        120,
        Some(30),
        Duration::from_millis(7),
        true,
        OutcomeClass::Found,
    )
}

fn wait_for_status(
    recorder: &AnalyticsRecorder,
    predicate: impl Fn(&AnalyticsQueueStatus) -> bool,
) -> AnalyticsQueueStatus {
    let started = Instant::now();
    loop {
        let status = recorder.status();
        if predicate(&status) {
            return status;
        }
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "timed out waiting for analytics queue status; last status: {status:?}"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn enabled_recorder_writes_bounded_metadata_in_background() {
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join(".symforge").join("analytics.db");
    let store = AnalyticsStore::open(AnalyticsConfig::enabled(&db_path)).unwrap();
    let recorder = AnalyticsRecorder::start(store, AnalyticsScope::Session, 8);

    let long_tool_name = "read_tool_".repeat(64);
    assert_eq!(
        recorder.enqueue(sample_observation(&long_tool_name)),
        AnalyticsEnqueueOutcome::Enqueued
    );
    assert_eq!(recorder.status().enqueued, 1);
    drop(recorder);

    let records = SqliteAnalyticsStore::open(&db_path)
        .unwrap()
        .recent_records(10)
        .unwrap();
    assert_eq!(records.len(), 1);
    assert!(records[0].tool_name.len() <= MAX_TOOL_NAME_BYTES);
    assert!(records[0].tool_name.ends_with("..."));
    assert!(long_tool_name.starts_with(records[0].tool_name.trim_end_matches("...")));
    assert_eq!(records[0].surface, "tool");
    assert_eq!(records[0].configured_scope, "session");
    assert_eq!(records[0].outcome_class, "found");
}

#[test]
fn disabled_recorder_does_not_create_database_or_enqueue() {
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join(".symforge").join("analytics.db");
    let recorder = AnalyticsRecorder::disabled(&db_path);

    match recorder.enqueue(sample_observation("get_file_content")) {
        AnalyticsEnqueueOutcome::Disabled(status) => assert!(status.is_disabled()),
        other => panic!("disabled recorder must report disabled, got {other:?}"),
    }

    assert_eq!(recorder.status().configured_scope, AnalyticsScope::Disabled);
    assert!(!db_path.exists(), "disabled analytics must not create a DB");
    drop(recorder);
    assert!(
        !db_path.exists(),
        "disabled analytics must remain no-footprint"
    );
}

struct BlockingWriter {
    started: Sender<()>,
    release: Receiver<()>,
    blocked_once: bool,
}

impl AnalyticsWriter for BlockingWriter {
    fn write(&mut self, _observation: &AnalyticsObservation) -> Result<AnalyticsWriteOutcome> {
        if !self.blocked_once {
            self.blocked_once = true;
            self.started.send(()).unwrap();
            self.release.recv().unwrap();
        }
        Ok(AnalyticsWriteOutcome::Recorded { id: 1 })
    }
}

#[test]
fn full_queue_drops_without_blocking_the_caller() {
    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let recorder = AnalyticsRecorder::start_with_writer_for_tests(
        AnalyticsScope::Session,
        1,
        BlockingWriter {
            started: started_tx,
            release: release_rx,
            blocked_once: false,
        },
    );

    assert_eq!(
        recorder.enqueue(sample_observation("first")),
        AnalyticsEnqueueOutcome::Enqueued
    );
    started_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("writer should receive first event");

    assert_eq!(
        recorder.enqueue(sample_observation("second")),
        AnalyticsEnqueueOutcome::Enqueued
    );
    assert_eq!(
        recorder.enqueue(sample_observation("third")),
        AnalyticsEnqueueOutcome::DroppedFull
    );

    let status = recorder.status();
    assert_eq!(status.enqueued, 2);
    assert_eq!(status.dropped_full, 1);
    assert_eq!(status.recorded, 0);

    release_tx.send(()).unwrap();
    drop(recorder);
}

struct FailingWriter;

impl AnalyticsWriter for FailingWriter {
    fn write(&mut self, _observation: &AnalyticsObservation) -> Result<AnalyticsWriteOutcome> {
        Err(anyhow!(
            "synthetic analytics writer failure with extra context that must stay bounded"
        ))
    }
}

#[test]
fn writer_failure_is_reported_as_status_metadata() {
    let recorder =
        AnalyticsRecorder::start_with_writer_for_tests(AnalyticsScope::Session, 4, FailingWriter);

    assert_eq!(
        recorder.enqueue(sample_observation("search_text")),
        AnalyticsEnqueueOutcome::Enqueued
    );

    let status = wait_for_status(&recorder, |status| status.write_failures == 1);
    assert_eq!(status.enqueued, 1);
    assert_eq!(status.recorded, 0);
    let error = status
        .last_writer_error
        .expect("writer failure should be retained as status metadata");
    assert!(error.contains("synthetic analytics writer failure"));
    assert!(error.len() <= MAX_ANALYTICS_QUEUE_ERROR_BYTES);

    drop(recorder);
}
