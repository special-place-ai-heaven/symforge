// Server-only integration test: depends on `#[cfg(feature = "server")]`
// surface machinery. Gating the whole file keeps
// `--no-default-features --features embed --all-targets` compiling.
#![cfg(feature = "server")]

//! Surface-default conformance (spike-gate flip, 2026-07-03): the env-absent
//! `tools/list` surface is FULL, and `SYMFORGE_SURFACE=compact` is the opt-in
//! escape hatch that is still ENFORCED at `tools/call` (P1-A), not just hidden
//! from `tools/list`.
//!
//! With `SYMFORGE_SURFACE` unset, `surface_profile_from_env` resolves to `Full`
//! and the advertised tool list is the legacy surface with the `symforge`
//! facade filtered out. With `SYMFORGE_SURFACE=compact`, the advertised list is
//! exactly the three compact tools (`symforge`, `symforge_edit`, `status`) and
//! every legacy `tools/call` is rejected at dispatch (FR-008/FR-009, SC-004).
//! `SYMFORGE_SURFACE=full` is identical to the default.
//!
//! These cases drive the PRODUCTION list source (`compact_surface_tools` for the
//! compact opt-in, `list_tools_for_profile` for the full default) and the
//! PRODUCTION dispatch gate (`enforce_compact_surface`, the exact function the
//! production `ServerHandler::call_tool` calls before routing — shared by stdio
//! and the HTTP `/mcp` path). They avoid binding a network transport while still
//! exercising the real code, so the test cannot pass while prod schemas or
//! enforcement diverge. `SYMFORGE_SURFACE` is process-global, so the cases
//! serialize on the shared `COMPACT_ENV_LOCK` and save/restore the variable via
//! RAII guards to prevent cross-test env bleed (the suite already runs
//! `--test-threads=1`).

#[path = "support/stel_surface_env.rs"]
mod stel_surface_env;

use std::collections::BTreeSet;

use symforge::protocol::surface_probe::{
    SurfaceProfile, enforce_compact_surface, list_tools_for_profile, surface_profile_from_env,
};
use symforge::stel::{COMPACT_TOOL_NAMES, compact_surface_tools};

/// Names from the PRODUCTION compact `tools/list` source (`compact_surface_tools`).
fn production_compact_names() -> BTreeSet<String> {
    compact_surface_tools()
        .into_iter()
        .map(|tool| tool.name.to_string())
        .collect()
}

fn advertised_names(profile: SurfaceProfile) -> BTreeSet<String> {
    list_tools_for_profile(profile)
        .into_iter()
        .map(|tool| tool.name.to_string())
        .collect()
}

/// Shared assertions for a full-surface advertised tool list: legacy tool count,
/// no `symforge` facade, legacy read tools present.
fn assert_full_surface(names: &BTreeSet<String>) {
    assert!(
        names.len() >= 30,
        "full surface must advertise the legacy tool surface; got {} tools",
        names.len()
    );
    assert!(
        !names.contains("symforge"),
        "full surface must not advertise the compact `symforge` facade"
    );
    assert!(
        names.contains("get_symbol"),
        "full surface must advertise the legacy read tools"
    );
}

#[tokio::test]
async fn unset_surface_resolves_full_and_advertises_full_surface() {
    let _serialize = stel_surface_env::COMPACT_ENV_LOCK.lock().await;
    let _surface = stel_surface_env::clear_symforge_surface();

    let profile = surface_profile_from_env();
    assert_eq!(
        profile,
        SurfaceProfile::Full,
        "unset SYMFORGE_SURFACE must resolve to the full default (spike-gate flip)"
    );

    assert_full_surface(&advertised_names(profile));
}

#[tokio::test]
async fn surface_compact_resolves_compact_and_advertises_three_tools() {
    let _serialize = stel_surface_env::COMPACT_ENV_LOCK.lock().await;
    let _surface = stel_surface_env::set_symforge_surface("compact");

    let profile = surface_profile_from_env();
    assert_eq!(
        profile,
        SurfaceProfile::Compact,
        "SYMFORGE_SURFACE=compact must resolve to the compact-3 opt-in surface"
    );

    // Assert through the PRODUCTION list source, not the measurement probe.
    let names = production_compact_names();
    let expected: BTreeSet<String> = COMPACT_TOOL_NAMES.iter().map(|n| n.to_string()).collect();
    assert_eq!(
        names, expected,
        "production compact tools/list must advertise exactly the compact-3 surface"
    );
    assert_eq!(names.len(), 3, "compact opt-in must expose exactly 3 tools");
}

#[tokio::test]
async fn surface_full_resolves_full_and_advertises_legacy_surface() {
    let _serialize = stel_surface_env::COMPACT_ENV_LOCK.lock().await;
    let _surface = stel_surface_env::set_symforge_surface("full");

    let profile = surface_profile_from_env();
    assert_eq!(
        profile,
        SurfaceProfile::Full,
        "SYMFORGE_SURFACE=full must resolve to the full surface (same as the default)"
    );

    assert_full_surface(&advertised_names(profile));
}

/// P1-A: on the compact opt-in surface, `tools/call` for a legacy tool is
/// REJECTED by the production dispatch gate — hiding it from `tools/list` is not
/// enough.
#[tokio::test]
async fn compact_surface_rejects_legacy_tool_call_at_dispatch() {
    let _serialize = stel_surface_env::COMPACT_ENV_LOCK.lock().await;
    let _surface = stel_surface_env::set_symforge_surface("compact");

    assert_eq!(
        surface_profile_from_env(),
        SurfaceProfile::Compact,
        "SYMFORGE_SURFACE=compact must resolve to the compact surface"
    );

    // A representative legacy tool that is hidden from compact `tools/list`.
    let rejected = enforce_compact_surface("search_text");
    let err = rejected.expect_err(
        "compact surface must REJECT a legacy `tools/call` at dispatch, not just hide it from \
         tools/list",
    );
    assert!(
        err.message.contains("compact surface"),
        "rejection must name the compact-surface policy; got: {}",
        err.message
    );
    assert!(
        err.message.contains("full"),
        "rejection must point at the full surface; got: {}",
        err.message
    );

    // The three advertised compact tools are NOT gated.
    for name in COMPACT_TOOL_NAMES {
        assert!(
            enforce_compact_surface(name).is_ok(),
            "advertised compact tool `{name}` must be allowed on the compact surface"
        );
    }
}

/// P1-A: on the full default surface, the same legacy `tools/call` is ALLOWED —
/// the gate must never block the default surface.
#[tokio::test]
async fn full_default_allows_legacy_tool_call_at_dispatch() {
    let _serialize = stel_surface_env::COMPACT_ENV_LOCK.lock().await;
    let _surface = stel_surface_env::clear_symforge_surface();

    assert_eq!(
        surface_profile_from_env(),
        SurfaceProfile::Full,
        "unset SYMFORGE_SURFACE must resolve to the full default"
    );

    assert!(
        enforce_compact_surface("search_text").is_ok(),
        "full default must allow a legacy `tools/call`"
    );
    // Compact facades are still callable on full (handlers gate them internally).
    for name in COMPACT_TOOL_NAMES {
        assert!(
            enforce_compact_surface(name).is_ok(),
            "tool `{name}` must not be dispatch-gated on the full surface"
        );
    }
}

/// FR-015 / M-003: the default full advertised surface is EXACTLY 40 tools.
///
/// `assert_full_surface` only floors the count at 30; this pins the exact
/// contract number so adding or dropping a `#[tool]` without a matching
/// surface/spec update fails here. The 40 is tied to its cause: the advertised
/// full surface is the registered router (`tool_definitions`, 41 tools including
/// the `symforge` compact facade) with exactly `symforge` filtered out. Combined
/// with `tests/conformance.rs` (registered set == `EXPECTED_TOOLS`, which tracks
/// `SYMFORGE_TOOL_NAMES`), this pins advertised == allowlist minus `symforge`.
#[tokio::test]
async fn full_surface_advertises_exactly_40_tools() {
    let _serialize = stel_surface_env::COMPACT_ENV_LOCK.lock().await;
    let _surface = stel_surface_env::clear_symforge_surface();

    assert_eq!(
        surface_profile_from_env(),
        SurfaceProfile::Full,
        "unset SYMFORGE_SURFACE must resolve to the full default"
    );

    let advertised = advertised_names(SurfaceProfile::Full);
    assert_eq!(
        advertised.len(),
        40,
        "default full surface must advertise exactly 40 tools (FR-015); got {}: {:?}",
        advertised.len(),
        advertised
    );

    // Tie the number to its cause: advertised == the registered router minus
    // only the `symforge` compact facade — never any other hidden tool.
    let registered: BTreeSet<String> = symforge::protocol::SymForgeServer::tool_definitions()
        .into_iter()
        .map(|tool| tool.name.to_string())
        .collect();
    let mut expected = registered.clone();
    assert!(
        expected.remove("symforge"),
        "registered router must contain the `symforge` facade the full surface filters out"
    );
    assert_eq!(
        advertised, expected,
        "full surface must be the registered router minus only the `symforge` facade"
    );
}

// ---- SF-TE-05: opt-in core profile (`SYMFORGE_SURFACE=core`) ----

fn core_names() -> BTreeSet<String> {
    symforge::stel::CORE_TOOL_NAMES
        .iter()
        .map(|n| n.to_string())
        .collect()
}

#[tokio::test]
async fn surface_core_resolves_core_and_advertises_core_tools() {
    let _serialize = stel_surface_env::COMPACT_ENV_LOCK.lock().await;
    let _surface = stel_surface_env::set_symforge_surface("CORE");
    let profile = surface_profile_from_env();
    assert_eq!(profile, SurfaceProfile::Core);
    assert_eq!(advertised_names(profile), core_names());
}

#[tokio::test]
async fn unknown_surface_value_resolves_full() {
    let _serialize = stel_surface_env::COMPACT_ENV_LOCK.lock().await;
    let _surface = stel_surface_env::set_symforge_surface("everything");
    assert_eq!(surface_profile_from_env(), SurfaceProfile::Full);
}

/// Every core name is a registered tool, and core is a strict subset of full.
#[tokio::test]
async fn core_tools_are_registered_and_a_subset_of_full() {
    let _serialize = stel_surface_env::COMPACT_ENV_LOCK.lock().await;
    let full = advertised_names(SurfaceProfile::Full);
    let core = advertised_names(SurfaceProfile::Core);
    assert_eq!(
        core,
        core_names(),
        "every core name must be a registered tool"
    );
    assert!(core.is_subset(&full) && core.len() < full.len());
}

/// Listing only: on core every full-surface tool outside the core list passes
/// the production dispatch gate. The compact `symforge` facade is not one of
/// them: its own handler refuses it off compact (asserted over HTTP in
/// `serve_http_attach.rs`).
#[tokio::test]
async fn core_still_dispatches_unlisted_tools() {
    let _serialize = stel_surface_env::COMPACT_ENV_LOCK.lock().await;
    let _surface = stel_surface_env::set_symforge_surface("core");
    assert_eq!(surface_profile_from_env(), SurfaceProfile::Core);

    let core = advertised_names(SurfaceProfile::Core);
    let unlisted: Vec<String> = advertised_names(SurfaceProfile::Full)
        .difference(&core)
        .cloned()
        .collect();
    assert!(unlisted.contains(&"detect_impact".to_string()));
    for name in unlisted.iter().map(String::as_str) {
        assert!(
            enforce_compact_surface(name).is_ok(),
            "core profile must not refuse unlisted tool `{name}`"
        );
    }
}
