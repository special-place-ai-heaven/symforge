//! 008 US1 (T005) — AAP detection + embed-pin comparison against fixtures.
//!
//! Drives [`symforge::server::aap`] over the committed `tests/fixtures/aap/*`
//! AAP-shaped directories:
//!
//! * **drift** — `Cargo.lock` pins an OLD symforge version → `Drift`.
//! * **match** — a lock synthesized from the live `running_version()` → `Match`
//!   (robust against release-please bumps; the committed fixture documents intent).
//! * **no-pin** — a valid lock with no symforge package → `PinUnknown` (no false
//!   drift warning).
//! * **missing-lock** — a directory with no `Cargo.lock` → `PinUnknown`.
//! * **not-detected** — no `AAP_ROOT` and no sibling → clean not-detected.
//!
//! Detection is driven via the `resolve_with` injection seam (fixtures only) so
//! the suite never touches a real AAP checkout. The single test that exercises
//! the `AAP_ROOT` env path serializes + restores the variable (the suite runs
//! `--test-threads=1`).
#![cfg(feature = "server")]

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use symforge::server::aap::{
    AapDetection, DetectionSource, EmbedPinComparison, IntegrationMode, read_symforge_pin,
    running_version,
};

/// Serializes process-env mutation for the one env-driven test below.
static ENV_LOCK: Mutex<()> = Mutex::new(());

const AAP_FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/aap");

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(AAP_FIXTURES).join(name)
}

// ---------------------------------------------------------------------------
// Detection
// ---------------------------------------------------------------------------

#[test]
fn detect_via_aap_root_env_against_fixture() {
    let _guard = ENV_LOCK.lock().unwrap();
    let root = fixture("drift");
    let prior = std::env::var_os("AAP_ROOT");
    #[allow(unsafe_code)] // test-only env mutation under ENV_LOCK + --test-threads=1.
    // SAFETY: serialized by ENV_LOCK; the suite runs single-threaded.
    unsafe {
        std::env::set_var("AAP_ROOT", &root)
    };

    let det = AapDetection::resolve();
    assert!(det.detected, "AAP_ROOT pointing at a fixture must detect");
    assert_eq!(det.source, Some(DetectionSource::EnvVar));
    assert_eq!(det.root.as_deref(), Some(root.as_path()));

    // Restore the prior env state.
    #[allow(unsafe_code)] // test-only env restore under ENV_LOCK + --test-threads=1.
    unsafe {
        match prior {
            Some(v) => std::env::set_var("AAP_ROOT", v),
            None => std::env::remove_var("AAP_ROOT"),
        }
    };
}

#[test]
fn not_detected_when_no_env_and_no_sibling() {
    // A symforge root whose parent has no Agent_Army_Professionals sibling.
    let tmp = tempfile::tempdir().unwrap();
    let symforge_root = tmp.path().join("symforge");
    std::fs::create_dir_all(&symforge_root).unwrap();
    let det = AapDetection::resolve_with(None, Some(&symforge_root));
    assert!(!det.detected, "no env + no sibling => clean not-detected");
    assert!(det.root.is_none());
    assert!(det.source.is_none());
}

#[test]
fn aap_root_set_but_absent_is_clean_not_detected() {
    let det = AapDetection::resolve_with(
        Some(std::ffi::OsString::from(
            "/definitely/not/a/real/aap/root/008",
        )),
        None,
    );
    assert!(
        !det.detected,
        "AAP_ROOT absent path => not-detected, no error"
    );
}

// ---------------------------------------------------------------------------
// Pin comparison
// ---------------------------------------------------------------------------

#[test]
fn drift_fixture_pins_old_version_and_flags_drift() {
    let root = fixture("drift");
    let pin = read_symforge_pin(&root);
    assert_eq!(pin.as_deref(), Some("7.0.0"), "drift fixture pins 7.0.0");

    let cmp = EmbedPinComparison::for_root(&root);
    // The running crate is not 7.0.0 (it tracks Cargo.toml), so this is Drift.
    assert!(cmp.is_drift(), "old pin vs running version must flag drift");
    assert_eq!(cmp.label(), "drift");
    assert_eq!(cmp.pinned_version(), Some("7.0.0"));
    assert_eq!(cmp.running_version(), running_version());
}

#[test]
fn match_when_pin_equals_running_version() {
    // Robust against release-please version bumps: synthesize a lock pinning the
    // live running_version() in a temp dir (the committed match/ fixture pins a
    // literal version that documents the same intent).
    let tmp = tempfile::tempdir().unwrap();
    write_symforge_lock(tmp.path(), running_version());

    let pin = read_symforge_pin(tmp.path());
    assert_eq!(pin.as_deref(), Some(running_version()));

    let cmp = EmbedPinComparison::for_root(tmp.path());
    assert!(!cmp.is_drift(), "matching pin must not flag drift");
    assert_eq!(cmp.label(), "match");
    assert_eq!(cmp.pinned_version(), Some(running_version()));
}

#[test]
fn committed_match_fixture_documents_current_version() {
    // The committed match/ fixture should track Cargo.toml's version so the
    // static-fixture read path is meaningful. If a release-please bump moves the
    // crate version, this surfaces the stale fixture (it does NOT gate the match
    // detection logic — that uses the synthesized lock above).
    let pin = read_symforge_pin(&fixture("match"));
    assert_eq!(
        pin.as_deref(),
        Some(running_version()),
        "committed match/Cargo.lock should pin the current crate version \
         (update tests/fixtures/aap/match/Cargo.lock after a version bump)"
    );
}

#[test]
fn no_pin_fixture_is_pin_unknown_no_false_drift() {
    let root = fixture("no-pin");
    assert!(
        read_symforge_pin(&root).is_none(),
        "no-pin fixture has no symforge package"
    );
    let cmp = EmbedPinComparison::for_root(&root);
    assert!(
        !cmp.is_drift(),
        "no pin must NOT raise a false drift warning"
    );
    assert_eq!(cmp.label(), "pin_unknown");
    assert!(cmp.pinned_version().is_none());
}

#[test]
fn missing_lock_fixture_is_pin_unknown() {
    let root = fixture("missing-lock");
    assert!(root.is_dir(), "missing-lock fixture dir must exist");
    assert!(
        read_symforge_pin(&root).is_none(),
        "missing-lock has no Cargo.lock"
    );
    let cmp = EmbedPinComparison::for_root(&root);
    assert_eq!(cmp.label(), "pin_unknown");
    assert!(!cmp.is_drift());
}

// ---------------------------------------------------------------------------
// Integration mode
// ---------------------------------------------------------------------------

#[test]
fn integration_mode_reflects_detection_and_serve() {
    assert_eq!(
        IntegrationMode::classify(false, false),
        IntegrationMode::None
    );
    assert_eq!(
        IntegrationMode::classify(true, false),
        IntegrationMode::Embed
    );
    assert_eq!(IntegrationMode::classify(true, true), IntegrationMode::Both);
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

/// Write a minimal AAP-shaped `Cargo.lock` pinning `symforge = version`.
fn write_symforge_lock(dir: &Path, version: &str) {
    let lock = format!(
        "# @generated\nversion = 4\n\n\
         [[package]]\nname = \"aap-code-intel\"\nversion = \"0.1.0\"\n\n\
         [[package]]\nname = \"symforge\"\nversion = \"{version}\"\n"
    );
    std::fs::write(dir.join("Cargo.lock"), lock).unwrap();
}
