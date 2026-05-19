//! Regression coverage for the `get_file_context` output-size contract.
//!
//! SRTK02 pins a representative, stable corpus so context-ratio regressions do
//! not hide behind whichever files happen to exist in the working repository.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use parking_lot::Mutex;
use serde_json::{Value, json};
use symforge::live_index::LiveIndex;
use symforge::protocol::SymForgeServer;
use symforge::watcher::WatcherInfo;
use tempfile::TempDir;

const MAX_CONTEXT_RATIO: f64 = 0.50;
const MIN_RATIO_FIXTURE_BYTES: usize = 100;
const TINY_FILE_EXCLUSION_REASON: &str = "files under 100 bytes are not ratio-gated because fixed context envelope overhead can dominate raw bytes";

#[derive(Debug, Clone, Copy)]
struct FixtureSpec {
    label: &'static str,
    path: &'static str,
}

const FIXTURES: &[FixtureSpec] = &[
    FixtureSpec {
        label: "rust",
        path: "rust/service.rs",
    },
    FixtureSpec {
        label: "python",
        path: "python/pipeline.py",
    },
    FixtureSpec {
        label: "typescript",
        path: "typescript/dashboard.ts",
    },
    FixtureSpec {
        label: "json",
        path: "json/policy.json",
    },
    FixtureSpec {
        label: "markdown",
        path: "markdown/runbook.md",
    },
];

#[derive(Debug)]
struct RatioMeasurement {
    label: &'static str,
    path: &'static str,
    raw_bytes: usize,
    context_bytes: usize,
}

impl RatioMeasurement {
    fn ratio(&self) -> f64 {
        self.context_bytes as f64 / self.raw_bytes as f64
    }
}

struct FixtureProject {
    _dir: TempDir,
    server: SymForgeServer,
}

impl FixtureProject {
    fn new() -> Self {
        let dir = TempDir::new().expect("temp project");
        let root = dir.path().to_path_buf();
        copy_ratio_fixtures(&root);

        let index = LiveIndex::load(&root).expect("load ratio fixture index");
        let watcher_info = Arc::new(Mutex::new(WatcherInfo::default()));
        let server = SymForgeServer::new(
            index,
            "persist_compression_ratio_test".to_string(),
            watcher_info,
            Some(root.clone()),
            None,
        );

        Self { _dir: dir, server }
    }
}

async fn call_get_file_context(server: &SymForgeServer, path: &str) -> String {
    call(server, "get_file_context", json!({ "path": path })).await
}

async fn call(server: &SymForgeServer, tool: &str, params: Value) -> String {
    server.dispatch_tool_for_tests(tool, params).await
}

fn copy_ratio_fixtures(destination_root: &Path) {
    let fixture_root = ratio_fixture_root();
    for fixture in FIXTURES {
        let source = fixture_root.join(fixture.path);
        let destination = destination_root.join(fixture.path);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).expect("create fixture parent directory");
        }
        fs::copy(&source, &destination)
            .unwrap_or_else(|error| panic!("copy fixture {} failed: {error}", fixture.path));
    }
}

fn ratio_fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("compression_ratio")
}

fn fixture_bytes(path: &str) -> Vec<u8> {
    fs::read(ratio_fixture_root().join(path))
        .unwrap_or_else(|error| panic!("read ratio fixture {path}: {error}"))
}

fn ratio_report(measurements: &[RatioMeasurement]) -> String {
    measurements
        .iter()
        .map(|measurement| {
            format!(
                "{} {}: raw={} context={} ratio={:.3}",
                measurement.label,
                measurement.path,
                measurement.raw_bytes,
                measurement.context_bytes,
                measurement.ratio(),
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[tokio::test]
async fn get_file_context_stays_under_half_raw_bytes_on_stable_fixtures() {
    let project = FixtureProject::new();
    let mut measurements = Vec::new();

    for fixture in FIXTURES {
        let raw_bytes = fixture_bytes(fixture.path).len();
        assert!(
            raw_bytes >= MIN_RATIO_FIXTURE_BYTES,
            "{} fixture `{}` is only {} bytes; {}",
            fixture.label,
            fixture.path,
            raw_bytes,
            TINY_FILE_EXCLUSION_REASON,
        );

        let context = call_get_file_context(&project.server, fixture.path).await;
        assert!(
            !context.starts_with("File not found"),
            "get_file_context could not see fixture `{}`; response:\n{}",
            fixture.path,
            context,
        );
        assert!(
            context.contains(fixture.path),
            "get_file_context response should name fixture `{}`; response:\n{}",
            fixture.path,
            context,
        );

        measurements.push(RatioMeasurement {
            label: fixture.label,
            path: fixture.path,
            raw_bytes,
            context_bytes: context.len(),
        });
    }

    let failures = measurements
        .iter()
        .filter(|measurement| measurement.ratio() > MAX_CONTEXT_RATIO)
        .collect::<Vec<_>>();

    assert!(
        failures.is_empty(),
        "get_file_context output exceeded {:.0}% of raw bytes.\n{}",
        MAX_CONTEXT_RATIO * 100.0,
        ratio_report(&measurements),
    );
}

#[test]
fn files_under_100_bytes_have_a_named_exclusion_reason() {
    let tiny_source = b"fn x() {}\n";
    assert!(tiny_source.len() < MIN_RATIO_FIXTURE_BYTES);
    assert!(
        TINY_FILE_EXCLUSION_REASON.contains("fixed context envelope"),
        "tiny-file exclusion must name the reason, got: {TINY_FILE_EXCLUSION_REASON}",
    );
}
