#![cfg(feature = "embed")]
//! MCP `status` parity: an embedded source renders the shared STEL readout
//! over its own index, query-session ledger and durable calibration store.
//! The server lib test `status_matches_embed_parity_golden` in
//! `src/protocol/tools.rs` asserts the same golden against MCP output.

use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

use symforge::embed::parity::QueryPolicy;
use symforge::embed::parity::host::{
    HostLimits, HostRefusalKind, HostRequest, HostResponse, HostRights, HostRoom, HostRoomConfig,
    HostRoomGrant,
};
use symforge::embed::parity::stel::{StelStatusDetail, StelStatusRequest};
use symforge::embed::{
    EmbeddedSourceHandle, EmbeddedSourceSpec, ProcessIndexRuntime, SourceRuntimePhase,
};

/// Shared with the MCP golden. `{root}`, `{project}` and `{version}` stand for
/// the bound root, the project name and the engine version.
const FULL_GOLDEN: &str = "── stel status ──
surface: full
symforge_version: {version}
phase0_go: 07b42a8
phase0_evidence: 08f7d14
l1_planner: wired
l2_economics: wired
l3_bypass: wired
l4_ledger: in_memory
handler_symforge: wired
handler_status: wired
handler_symforge_edit: preview-and-apply
ledger_events: 0
project_root: {root}
index_ready: true
index_files: 1
deferred: b_results
superseded: multi_step_planner (client agentic loop + find-fusion)
──
project: {project}
index_symbols: 1
session_tokens: 0
last_ledger_decision: none
last_ledger_route: none
durable_ledger: events=0 net_vs_manual=0 sessions=0
── calibration (observational) ──
events: 0
serve: 0
degrade: 0
bypass: 0
cache_hit: 0
pff_bypass: 0
legacy_executed: 0
schema_tokens: 0
invoke_tokens: 0
predicted_net_total: 0
predicted_response_tokens: 0
actual_response_tokens: 0
calibration: deferred
tuning: deferred
──
secret_dismissals: 0";

const NOT_APPLICABLE: &[&str] = &[
    "daemon_env_surface: not_applicable (",
    "daemon_instance: not_applicable (",
    "daemon_degraded_local_fallback: not_applicable (",
    "daemon_version: not_applicable (",
    "proxy_owned_lines: not_applicable (",
];

fn fixture(root: &Path) {
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/lib.rs"), "pub fn answer() -> u32 { 42 }\n").unwrap();
}

fn open(runtime: &ProcessIndexRuntime, root: &Path) -> EmbeddedSourceHandle {
    let handle = runtime
        .open_embedded_source(EmbeddedSourceSpec::current_worktree(root.to_path_buf()))
        .unwrap();
    let until = Instant::now() + Duration::from_secs(20);
    while handle.runtime_view().phase != SourceRuntimePhase::Current {
        assert!(Instant::now() < until, "source never reached Current");
        std::thread::sleep(Duration::from_millis(5));
    }
    handle
}

fn golden(root: &Path) -> String {
    let canonical = dunce::canonicalize(root).unwrap();
    FULL_GOLDEN
        .replace("{version}", env!("CARGO_PKG_VERSION"))
        .replace("{root}", &canonical.to_string_lossy().replace('\\', "/"))
        .replace(
            "{project}",
            canonical.file_name().unwrap().to_str().unwrap(),
        )
}

fn full() -> StelStatusRequest {
    StelStatusRequest {
        detail: Some(StelStatusDetail::Full),
        ..Default::default()
    }
}

const DERIVED: QueryPolicy = QueryPolicy {
    allow_derived_state_preparation: true,
};

#[test]
fn status_renders_the_mcp_readout_and_reports_daemon_lines_not_applicable() {
    let root = tempfile::tempdir().unwrap();
    fixture(root.path());
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let handle = open(&runtime, root.path());
    let session = handle.new_query_session().unwrap();

    let report = handle
        .stel_status(&full(), Some(&session), DERIVED)
        .unwrap();
    let (body, tail) = report
        .rendered
        .split_once("\ndaemon_env_surface: not_applicable")
        .expect("not-applicable lines follow the shared body");
    assert_eq!(body, golden(root.path()), "{}", report.rendered);
    let tail = format!("daemon_env_surface: not_applicable{tail}");
    for line in NOT_APPLICABLE {
        assert!(tail.contains(line), "{tail}");
    }
    assert_eq!(report.not_applicable.len(), NOT_APPLICABLE.len());

    // The compact projection is MCP's compact body.
    let compact = handle
        .stel_status(&StelStatusRequest::default(), Some(&session), DERIVED)
        .unwrap();
    let compact_golden = golden(root.path());
    let compact_golden = compact_golden.split("\n──\nproject: ").next().unwrap();
    assert!(
        compact
            .rendered
            .starts_with(&format!("{compact_golden}\n──\nsecret_dismissals: 0\n")),
        "{}",
        compact.rendered
    );
}

#[test]
fn reset_calibration_clears_the_durable_store_or_reports_none_without_one() {
    let root = tempfile::tempdir().unwrap();
    fixture(root.path());
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let handle = open(&runtime, root.path());
    let reset = StelStatusRequest {
        reset_calibration: Some(true),
        ..full()
    };

    let durable = handle.stel_status(&reset, None, DERIVED).unwrap();
    assert!(
        durable.rendered.contains(
            "\nsecret_dismissals: 0\ncalibration_reset: cleared 0 sample(s) + active tuning (state -> deferred)\n"
        ),
        "{}",
        durable.rendered
    );

    let root = tempfile::tempdir().unwrap();
    fixture(root.path());
    let handle = open(&runtime, root.path());
    let storeless = handle
        .stel_status(&reset, None, QueryPolicy::default())
        .unwrap();
    assert!(
        storeless
            .rendered
            .contains("\ndurable_ledger: unavailable\n"),
        "{}",
        storeless.rendered
    );
    assert!(
        storeless.rendered.contains(
            "\ncalibration_reset: no durable store; in-memory calibration is already deferred\n"
        ),
        "{}",
        storeless.rendered
    );
}

#[test]
fn host_status_report_requires_the_checkpoint_right_to_reset_calibration() {
    let root = tempfile::tempdir().unwrap();
    fixture(root.path());
    let runtime = ProcessIndexRuntime::acquire().unwrap();
    let grant = HostRoomGrant::new(
        "status-room".into(),
        dunce::canonicalize(root.path()).unwrap(),
        "status-scope".into(),
        HostRights::read_only(),
    )
    .unwrap();
    let room = HostRoom::open(
        runtime,
        grant,
        HostRoomConfig {
            room_id: "status-room".into(),
            limits: HostLimits::default(),
        },
    )
    .unwrap();
    let control = room.control().unwrap();
    let until = Instant::now() + Duration::from_secs(20);
    let report = loop {
        match room
            .dispatch(&HostRequest::StatusReport(full()), &control)
            .unwrap()
        {
            HostResponse::StatusReport(report) if report.report.is_some() => break report,
            _ => {
                assert!(Instant::now() < until, "no status report");
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    };
    let rendered = report.report.unwrap().rendered;
    assert!(rendered.contains("\nsurface: full\n"), "{rendered}");
    // Read-only rights hold no derived-state permission: no durable store.
    assert!(
        rendered.contains("\ndurable_ledger: unavailable\n"),
        "{rendered}"
    );

    let refused = room
        .dispatch(
            &HostRequest::StatusReport(StelStatusRequest {
                reset_calibration: Some(true),
                ..full()
            }),
            &control,
        )
        .unwrap_err();
    assert_eq!(refused.kind, HostRefusalKind::Denied);
}
