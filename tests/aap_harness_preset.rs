//! 008 US3 (T017) — AAP-aware harness scan + onboarding banner.
//!
//! Verifies that the harness hub treats AAP as a **distinct AAP-typed target**
//! (not a generic MCP-client JSON) and that its presets never overwrite the AAP
//! embed path dependency with a stdio-spawn config (FR-005 / SC-003/SC-004), and
//! that the onboarding banner mentions the AAP embed path when AAP is detected
//! (FR-006).
//!
//! Detection is driven two ways, both fixtures-only (never a real AAP checkout):
//! * the `aap_target_from` injection seam with an explicit `AapDetection`
//!   (no process env), and
//! * the `aap_target` env path with `AAP_ROOT` pointed at a committed fixture,
//!   serialized + restored under a lock (`--test-threads=1`).
#![cfg(feature = "server")]

use std::path::PathBuf;
use std::sync::Mutex;

use symforge::cli::harness::{AapPresetChoice, aap_target, aap_target_from};
use symforge::cli::onboarding::{self, AapBanner, OnboardingSink};
use symforge::server::aap::AapDetection;

const AAP_FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/aap");

/// Serializes `AAP_ROOT` mutation for the one env-driven test below.
static ENV_LOCK: Mutex<()> = Mutex::new(());

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(AAP_FIXTURES).join(name)
}

/// A detected `AapDetection` rooted at the named fixture (env-source), for the
/// injection-seam tests (no process env mutation).
fn detected_at(name: &str) -> AapDetection {
    let root = fixture(name);
    // Drive through the public `resolve_with` seam with an explicit AAP_ROOT
    // value so detection is `EnvVar`-sourced and deterministic.
    AapDetection::resolve_with(Some(root.into_os_string()), None)
}

// ---------------------------------------------------------------------------
// Distinct AAP-typed target (SC-004) + presets (SC-003)
// ---------------------------------------------------------------------------

#[test]
fn aap_is_a_distinct_typed_target_not_generic_mcp_json() {
    // A detected AAP root yields an AAP-typed target carrying the AAP root +
    // source — it is NOT a HarnessTarget/HarnessId (Cursor/Claude) JSON entry.
    let det = detected_at("drift");
    assert!(det.detected, "fixture root must detect");

    let target = aap_target_from(&det, None);
    assert!(target.detected);
    assert_eq!(target.source, Some("env"));
    assert_eq!(target.root.as_deref(), Some(fixture("drift").as_path()));
    // The embed path dep is present and is a path dep (the AAP-native route).
    assert!(
        target.embed_dep_is_path_not_stdio(),
        "AAP embed dep must be a path dep, never stdio: {}",
        target.embed_path_dep
    );
}

#[test]
fn embed_only_is_the_default_http_only_when_serve_active() {
    let det = detected_at("drift");

    // No serve attach URL → embed-only is the sole preset (the default).
    let embed_only = aap_target_from(&det, None);
    assert_eq!(
        embed_only.presets,
        vec![AapPresetChoice::EmbedOnly],
        "embed-only is the default when no serve URL is available"
    );
    assert!(!embed_only.offers_http());

    // A serve attach URL → embed-only PLUS the HTTP preset.
    let with_http = aap_target_from(&det, Some("http://127.0.0.1:8787/mcp"));
    assert!(with_http.offers_http(), "HTTP preset offered with serve URL");
    assert!(
        with_http.presets.contains(&AapPresetChoice::EmbedOnly),
        "embed-only is still offered alongside HTTP"
    );

    // An empty serve URL is treated as absent (no HTTP preset).
    let empty = aap_target_from(&det, Some(""));
    assert!(!empty.offers_http());
}

#[test]
fn embed_path_dep_is_never_a_stdio_spawn_config() {
    // SC-003: the AAP embed dep must NEVER be overwritten with a stdio-spawn
    // config. Both presets preserve the path dep; assert the canonical shape.
    let det = detected_at("match");
    for serve in [None, Some("http://127.0.0.1:8787/mcp")] {
        let target = aap_target_from(&det, serve);
        let dep = &target.embed_path_dep;
        assert!(dep.contains("path = \"../symforge\""), "path dep: {dep}");
        assert!(dep.contains("features = [\"embed\"]"), "embed feature: {dep}");
        assert!(!dep.contains("command"), "never a stdio command: {dep}");
        assert!(!dep.contains("stdio"), "never stdio: {dep}");
        assert!(!dep.contains("args"), "never spawn args: {dep}");
        assert!(target.embed_dep_is_path_not_stdio());
    }
}

#[test]
fn not_detected_aap_yields_no_presets() {
    // A not-detected AAP (absent AAP_ROOT + no sibling) is a clean, typed,
    // empty target — no presets, no fabricated root.
    let det = AapDetection::resolve_with(
        Some(std::ffi::OsString::from("/definitely/not/a/real/aap/root/008-us3")),
        None,
    );
    assert!(!det.detected);
    let target = aap_target_from(&det, Some("http://127.0.0.1:8787/mcp"));
    assert!(!target.detected);
    assert!(target.root.is_none());
    assert!(target.source.is_none());
    assert!(
        target.presets.is_empty(),
        "no presets apply when AAP is not detected"
    );
}

#[test]
fn aap_target_env_path_detects_fixture_root() {
    // The live `aap_target()` reads AAP_ROOT in-process; serialize + restore.
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let root = fixture("no-pin");
    let prior = std::env::var_os("AAP_ROOT");
    #[allow(unsafe_code)] // test-only env mutation under ENV_LOCK + --test-threads=1.
    // SAFETY: serialized by ENV_LOCK; the suite runs single-threaded.
    unsafe {
        std::env::set_var("AAP_ROOT", &root)
    };

    let target = aap_target(Some("http://127.0.0.1:8787/mcp"));
    assert!(target.detected, "AAP_ROOT at a fixture must detect");
    assert_eq!(target.source, Some("env"));
    assert_eq!(target.root.as_deref(), Some(root.as_path()));
    assert!(target.offers_http(), "serve URL present => HTTP preset offered");
    assert!(target.embed_dep_is_path_not_stdio());

    #[allow(unsafe_code)] // test-only env restore under ENV_LOCK + --test-threads=1.
    // SAFETY: serialized by ENV_LOCK; the suite runs single-threaded.
    unsafe {
        match prior {
            Some(v) => std::env::set_var("AAP_ROOT", v),
            None => std::env::remove_var("AAP_ROOT"),
        }
    };
}

// ---------------------------------------------------------------------------
// Onboarding banner mentions the embed path when AAP detected (FR-006)
// ---------------------------------------------------------------------------

/// Recording sink: captures banner lines without touching stderr or a browser.
#[derive(Default)]
struct RecordingSink {
    lines: Vec<String>,
}

impl OnboardingSink for RecordingSink {
    fn line(&mut self, text: &str) {
        self.lines.push(text.to_string());
    }
    fn offer_open(&mut self, _url: &str) -> bool {
        false
    }
}

#[test]
fn banner_mentions_admin_and_embed_path_when_aap_detected() {
    let dir = tempfile::tempdir().unwrap();
    let state_path = dir.path().join("onboarding.json");

    // Build the AAP banner from a detected target's embed dep (the same snippet
    // serve.rs surfaces).
    let target = aap_target_from(&detected_at("match"), None);
    let aap = AapBanner {
        admin_url: "http://127.0.0.1:8787/admin".to_string(),
        embed_path_dep: target.embed_path_dep.clone(),
    };

    let mut sink = RecordingSink::default();
    let shown = onboarding::maybe_show_banner_with_aap(
        &state_path,
        "8.1.0",
        "http://127.0.0.1:8787/mcp",
        Some(&aap),
        &mut sink,
    );
    assert!(shown, "banner shows on first run");
    let text = sink.lines.join("\n");
    assert!(text.contains("/admin"), "banner mentions /admin: {text}");
    assert!(
        text.contains("path = \"../symforge\"") && text.contains("features = [\"embed\"]"),
        "banner mentions the AAP embed path dependency: {text}"
    );
}
