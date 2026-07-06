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
