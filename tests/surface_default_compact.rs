//! US2 / T021 conformance: the default `tools/list` surface is compact-3.
//!
//! With `SYMFORGE_SURFACE` unset, `surface_profile_from_env` resolves to
//! `Compact` and the advertised tool list is exactly the three compact tools
//! (`symforge`, `symforge_edit`, `status`). With `SYMFORGE_SURFACE=full`, the
//! legacy (32-tool) surface is returned — the documented backward-compatible
//! opt-out (FR-008/FR-009, SC-004).
//!
//! These cases drive `surface_profile_from_env` and the `tools/list` builder
//! (`list_tools_for_profile`) directly to avoid binding a network transport.
//! `SYMFORGE_SURFACE` is process-global, so the cases serialize on the shared
//! `COMPACT_ENV_LOCK` and save/restore the variable via RAII guards to prevent
//! cross-test env bleed (the suite already runs `--test-threads=1`).

#[path = "support/stel_surface_env.rs"]
mod stel_surface_env;

use std::collections::BTreeSet;

use symforge::protocol::surface_probe::{
    SurfaceProfile, list_tools_for_profile, surface_profile_from_env,
};

/// Canonical compact-3 surface names (mirrors `stel::COMPACT_TOOL_NAMES`).
const COMPACT_TOOL_NAMES: [&str; 3] = ["symforge", "symforge_edit", "status"];

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

    let names = advertised_names(profile);
    let expected: BTreeSet<String> = COMPACT_TOOL_NAMES.iter().map(|n| n.to_string()).collect();
    assert_eq!(
        names, expected,
        "default tools/list must advertise exactly the compact-3 surface"
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
