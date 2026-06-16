//! US2 / T021 conformance: the default `tools/list` surface is compact-3, and
//! the compact surface is ENFORCED at `tools/call`, not just hidden from
//! `tools/list` (P1-A).
//!
//! With `SYMFORGE_SURFACE` unset, `surface_profile_from_env` resolves to
//! `Compact` and the advertised tool list is exactly the three compact tools
//! (`symforge`, `symforge_edit`, `status`). With `SYMFORGE_SURFACE=full`, the
//! legacy (32-tool) surface is returned — the documented backward-compatible
//! opt-out (FR-008/FR-009, SC-004).
//!
//! These cases drive the PRODUCTION list source (`compact_surface_tools`, the
//! same vec the real `SymForgeServer::list_tools` returns on the compact
//! surface) and the PRODUCTION dispatch gate (`enforce_compact_surface`, the
//! exact function the production `ServerHandler::call_tool` calls before routing
//! — shared by stdio and the HTTP `/mcp` path). They avoid binding a network
//! transport while still exercising the real code, so the test cannot pass while
//! prod schemas or enforcement diverge. `SYMFORGE_SURFACE` is process-global, so
//! the cases serialize on the shared `COMPACT_ENV_LOCK` and save/restore the
//! variable via RAII guards to prevent cross-test env bleed (the suite already
//! runs `--test-threads=1`).

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

#[tokio::test]
async fn unset_surface_resolves_compact_and_advertises_three_tools() {
    let _serialize = stel_surface_env::COMPACT_ENV_LOCK.lock().await;
    let _surface = stel_surface_env::clear_symforge_surface();

    let profile = surface_profile_from_env();
    assert_eq!(
        profile,
        SurfaceProfile::Compact,
        "unset SYMFORGE_SURFACE must resolve to the compact-3 default"
    );

    // Assert through the PRODUCTION list source, not the measurement probe.
    let names = production_compact_names();
    let expected: BTreeSet<String> = COMPACT_TOOL_NAMES.iter().map(|n| n.to_string()).collect();
    assert_eq!(
        names, expected,
        "production compact tools/list must advertise exactly the compact-3 surface"
    );
    assert_eq!(
        names.len(),
        3,
        "compact default must expose exactly 3 tools"
    );
}

#[tokio::test]
async fn surface_full_resolves_full_and_advertises_legacy_surface() {
    let _serialize = stel_surface_env::COMPACT_ENV_LOCK.lock().await;
    let _surface = stel_surface_env::set_symforge_surface("full");

    let profile = surface_profile_from_env();
    assert_eq!(
        profile,
        SurfaceProfile::Full,
        "SYMFORGE_SURFACE=full must restore the legacy surface profile"
    );

    let names = advertised_names(profile);
    assert!(
        names.len() >= 30,
        "full opt-out must advertise the legacy (32-tool) surface; got {} tools",
        names.len()
    );
    assert!(
        !names.contains("symforge"),
        "legacy full surface must not advertise the compact `symforge` facade"
    );
    // The compact facades are absent; the legacy read tools are present.
    assert!(
        names.contains("get_symbol"),
        "legacy full surface must advertise the legacy read tools"
    );
}

/// P1-A: on the compact default, `tools/call` for a legacy tool is REJECTED by
/// the production dispatch gate — hiding it from `tools/list` is not enough.
#[tokio::test]
async fn compact_default_rejects_legacy_tool_call_at_dispatch() {
    let _serialize = stel_surface_env::COMPACT_ENV_LOCK.lock().await;
    let _surface = stel_surface_env::clear_symforge_surface();

    assert_eq!(
        surface_profile_from_env(),
        SurfaceProfile::Compact,
        "unset SYMFORGE_SURFACE must resolve to the compact default"
    );

    // A representative legacy tool that is hidden from compact `tools/list`.
    let rejected = enforce_compact_surface("search_text");
    let err = rejected.expect_err(
        "compact default must REJECT a legacy `tools/call` at dispatch, not just hide it from \
         tools/list",
    );
    assert!(
        err.message.contains("compact surface"),
        "rejection must name the compact-surface policy; got: {}",
        err.message
    );
    assert!(
        err.message.contains("SYMFORGE_SURFACE=full"),
        "rejection must point at the documented opt-out; got: {}",
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

/// P1-A: with `SYMFORGE_SURFACE=full`, the same legacy `tools/call` is ALLOWED —
/// the gate must not block the documented backward-compatible opt-out.
#[tokio::test]
async fn full_surface_allows_legacy_tool_call_at_dispatch() {
    let _serialize = stel_surface_env::COMPACT_ENV_LOCK.lock().await;
    let _surface = stel_surface_env::set_symforge_surface("full");

    assert_eq!(
        surface_profile_from_env(),
        SurfaceProfile::Full,
        "SYMFORGE_SURFACE=full must resolve to the full surface"
    );

    assert!(
        enforce_compact_surface("search_text").is_ok(),
        "full surface must allow legacy `tools/call` (the documented opt-out)"
    );
    // Compact facades are still callable on full (handlers gate them internally).
    for name in COMPACT_TOOL_NAMES {
        assert!(
            enforce_compact_surface(name).is_ok(),
            "tool `{name}` must not be dispatch-gated on the full surface"
        );
    }
}
