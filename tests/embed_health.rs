#![cfg(feature = "embed")]
//! MCP parity for the embedded host's health surfaces and static resources.
//!
//! The quarantine-window lines asserted here are the exact lines the MCP
//! `health` handler renders for the same fixture and paging; the server lib
//! test `health_quarantine_paging_matches_embed_parity_golden` in
//! `src/protocol/tools.rs` asserts the same golden against MCP output.

use std::fs;
use std::time::{Duration, Instant};

use symforge::embed::ProcessIndexRuntime;
use symforge::embed::parity::host::{
    HostHealthProjection, HostHealthReport, HostHealthRequest, HostLimits, HostPhase, HostRequest,
    HostResourceContent, HostResourceRequest, HostResponse, HostRights, HostRoom, HostRoomConfig,
    HostRoomGrant,
};

/// Shared with the MCP golden: three repo-owned partial parses.
fn fixture() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    git2::Repository::init(root.path()).unwrap();
    fs::create_dir(root.path().join("src")).unwrap();
    fs::write(
        root.path().join("src/lib.rs"),
        b"pub fn answer() -> u32 { 42 }\n",
    )
    .unwrap();
    for name in ["a", "b", "c"] {
        fs::write(
            root.path().join(format!("src/broken_{name}.rs")),
            b"pub fn broken( {\n",
        )
        .unwrap();
    }
    root
}

fn open_room(root: &std::path::Path) -> HostRoom {
    let grant = HostRoomGrant::new(
        "health-room".to_owned(),
        root.to_path_buf(),
        "health-room-guest-source".to_owned(),
        HostRights::read_only(),
    )
    .unwrap();
    let room = HostRoom::open(
        ProcessIndexRuntime::acquire().unwrap(),
        grant,
        HostRoomConfig {
            room_id: "health-room".to_owned(),
            limits: HostLimits::default(),
        },
    )
    .unwrap();
    let control = room.control().unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let HostResponse::Status(status) = room.dispatch(&HostRequest::Status, &control).unwrap()
        else {
            unreachable!()
        };
        if status.phase == HostPhase::Current {
            return room;
        }
        assert!(Instant::now() < deadline, "source failed to publish");
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn report(room: &HostRoom, request: &HostRequest) -> HostHealthReport {
    let control = room.control().unwrap();
    // Exercise the serialized lane: options must survive the wire.
    let wire = serde_json::to_vec(request).unwrap();
    let HostResponse::HealthReport(report) =
        serde_json::from_slice(&room.dispatch_wire(&wire, &control).unwrap()).unwrap()
    else {
        panic!("health report response")
    };
    report
}

fn assert_in_order(text: &str, needles: &[&str]) {
    let mut from = 0;
    for needle in needles {
        let at = text[from..]
            .find(needle)
            .unwrap_or_else(|| panic!("missing `{needle}` after byte {from} in:\n{text}"));
        from += at + needle.len();
    }
}

const PARTIAL_REASON: &str = "repo-owned partial parse; best-effort symbols may be incomplete";

#[test]
fn health_report_pages_quarantine_and_reports_process_sections_as_not_applicable() {
    let root = fixture();
    let room = open_room(root.path());

    let full = report(
        &room,
        &HostRequest::HealthReport(HostHealthRequest {
            quarantine_limit: Some(1),
            quarantine_offset: Some(1),
        }),
    );
    assert_eq!(full.projection, HostHealthProjection::Full);
    assert_eq!(full.health.status.room_id, "health-room");
    assert!(full.health.current.is_some(), "typed health stays attached");
    let text = full.report.as_deref().expect("shared report rendered");
    assert_in_order(
        text,
        &[
            "Status: Ready",
            "Watcher: off",
            "Parse/span quarantine registry: total=3 unexpected_partial=3 failed=0 showing=1 offset=1 omitted=1",
            &format!("\n  2. src/broken_b.rs [unexpected_partial] - {PARTIAL_REASON}"),
            "\n  (use health with quarantine_limit/quarantine_offset to page the full list)",
            "\nRuntime: mode=embedded | runtime_state=embedded | version=",
            "session_id=embedded-room-health-room",
            "\ndaemon_degradation: not_applicable (",
            "\nsidecar: not_applicable (",
            "\nhook_adoption: not_applicable (",
            "\nGit temporal",
            "\nCapabilities:\n  frecency: ",
            "\n  worktree routing: not_applicable (embedded edit requests carry no working_directory)",
            "\nKnowledge curation: capability=",
            "\n── Worktree-awareness misuse ──\nedit tool calls without working_directory (last hour): not_applicable (embedded edit requests carry no working_directory)",
            "\nsecret_dismissals: ",
            "\nversion_drift: not_applicable (",
            "\npath_shadow: not_applicable (",
        ],
    );
    if std::env::var("SYMFORGE_FRECENCY").as_deref() != Ok("1") {
        assert!(!text.contains("Top frecent files"), "frecency is env-gated");
    }
    assert!(!text.contains("src/broken_a.rs [unexpected_partial]"));
    assert!(!text.contains("src/broken_c.rs [unexpected_partial]"));
    let sections: Vec<&str> = full
        .not_applicable
        .iter()
        .map(|entry| entry.section.as_str())
        .collect();
    for expected in [
        "daemon_degradation",
        "project_mismatch",
        "sidecar",
        "token_savings",
        "tool_call_counts",
        "hook_adoption",
        "worktree_misuse",
        "version_drift",
        "path_shadow",
        "filesystem_watcher",
    ] {
        assert!(sections.contains(&expected), "missing N/A {expected}");
    }

    // Defaults match MCP `health` with no arguments: offset 0, limit 10.
    let default = report(
        &room,
        &HostRequest::HealthReport(HostHealthRequest::default()),
    );
    let text = default.report.unwrap();
    assert!(text.contains("showing=3 offset=0 omitted=0"), "{text}");
    for name in ["a", "b", "c"] {
        assert!(text.contains(&format!("src/broken_{name}.rs [unexpected_partial]")));
    }

    // MCP clamps the limit to 1..=1000.
    let clamped = report(
        &room,
        &HostRequest::HealthReport(HostHealthRequest {
            quarantine_limit: Some(0),
            quarantine_offset: None,
        }),
    );
    assert!(
        clamped
            .report
            .unwrap()
            .contains("showing=1 offset=0 omitted=2")
    );

    let compact = report(&room, &HostRequest::HealthCompact);
    assert_eq!(compact.projection, HostHealthProjection::Compact);
    let text = compact.report.unwrap();
    assert_in_order(
        &text,
        &[
            "Status: Ready | Files: ",
            "\nWatcher: off | Stale-mutation rejections: 0",
            "\nParse/span quarantine: total=3 unexpected_partial=3 failed=0 showing=3 offset=0 omitted=0",
            "\nRuntime: mode=embedded | version=",
            "\nsidecar: not_applicable (",
            "| Worktree misuse/hour: not_applicable",
            "\nCapabilities: frecency=",
            "; worktree=not_applicable (",
            "\nKnowledge curation: capability=",
            "\nsecret_dismissals: ",
            "\nversion_drift: not_applicable (",
        ],
    );
    assert!(!text.contains("Top frecent files"), "frecency is full-only");

    // The repo/health resource is MCP `health` with default paging.
    let control = room.control().unwrap();
    let HostResponse::Resource(resource) = room
        .dispatch(
            &HostRequest::Resource(HostResourceRequest::RepoHealth),
            &control,
        )
        .unwrap()
    else {
        panic!("resource response")
    };
    assert_eq!(resource.uri, "symforge://repo/health");
    let HostResourceContent::HealthReport(resource) = resource.content else {
        panic!("repo/health renders the health report")
    };
    assert_eq!(resource.projection, HostHealthProjection::Full);
    assert!(
        resource
            .report
            .unwrap()
            .contains("showing=3 offset=0 omitted=0")
    );
}

/// Frecency diagnostics are env-gated on both surfaces (`SYMFORGE_FRECENCY=1`).
/// The parent re-runs this exact test in a child process with the flag set,
/// so no test mutates the shared process environment.
#[test]
fn health_report_renders_frecency_section_when_enabled() {
    const NAME: &str = "health_report_renders_frecency_section_when_enabled";
    if std::env::var("SYMFORGE_FRECENCY").as_deref() != Ok("1") {
        let status = symforge::process_util::hidden_command(std::env::current_exe().unwrap())
            .args(["--exact", NAME, "--test-threads=1"])
            .env("SYMFORGE_FRECENCY", "1")
            .status()
            .unwrap();
        assert!(status.success(), "frecency child failed: {status}");
        return;
    }
    let root = fixture();
    let room = open_room(root.path());
    let full = report(
        &room,
        &HostRequest::HealthReport(HostHealthRequest::default()),
    );
    assert_in_order(
        full.report.as_deref().unwrap(),
        &[
            "\nsecret_dismissals: ",
            "\n── Top frecent files ──\n  (no frecency rows recorded yet)",
            "\nversion_drift: not_applicable (",
        ],
    );
}

#[test]
fn tools_catalog_and_glossary_resources_render_the_mcp_text() {
    let root = fixture();
    let room = open_room(root.path());
    let control = room.control().unwrap();
    let text = |request: HostResourceRequest| {
        let HostResponse::Resource(reply) = room
            .dispatch(&HostRequest::Resource(request), &control)
            .unwrap()
        else {
            panic!("resource response")
        };
        let HostResourceContent::Text(text) = reply.content else {
            panic!("static resources render text")
        };
        (reply.uri, text)
    };
    let (uri, catalog) = text(HostResourceRequest::ToolsCatalog);
    assert_eq!(uri, "symforge://tools/catalog");
    assert!(catalog.starts_with("SymForge tool catalog — grouped by workflow."));
    assert_in_order(
        &catalog,
        &[
            "\n## orientation — ",
            "\n## search — ",
            "\n## dry-run-edits — ",
            "\n## diagnostics — ",
            "\nTip: ask `which tool should I use for <topic>?`",
        ],
    );
    let (uri, glossary) = text(HostResourceRequest::Glossary);
    assert_eq!(uri, "symforge://glossary");
    assert!(glossary.starts_with("SymForge glossary — surface vocabulary."));
    assert!(glossary.contains("## Project binding"));

    // Room rights still gate the typed operation catalog.
    let HostResponse::Catalog(typed) = room.dispatch(&HostRequest::Catalog, &control).unwrap()
    else {
        panic!("catalog response")
    };
    assert!(typed.edit_operations.is_empty(), "read-only room");

    #[cfg(feature = "server")]
    {
        assert_eq!(
            catalog,
            symforge::protocol::smart_query::render_tool_catalog()
        );
        assert_eq!(glossary, symforge::protocol::smart_query::render_glossary());
    }
}
