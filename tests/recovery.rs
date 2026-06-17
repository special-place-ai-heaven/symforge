// Server-only integration test: depends on `#[cfg(feature = "server")]`
// protocol/surface machinery. Gating the whole file keeps
// `--no-default-features --features embed --all-targets` compiling.
#![cfg(feature = "server")]

//! Recoverable cold start (010 US4 / TR-02 + TR-03, FR-011/012/013).
//!
//! T028 — `compact_surface_index_not_loaded_message_never_mentions_blocked_tools`:
//! On the compact surface, NO empty-index / "not loaded" recovery message names a
//! tool the compact dispatch gate forbids (`index_folder` is gated; only the
//! compact-3 `symforge` / `symforge_edit` / `status` are callable). Every such
//! message instead names a recovery the agent CAN perform on its surface
//! (re-launch from the project root, or the documented `SYMFORGE_SURFACE=full`
//! opt-out). This is the executable form of FR-012 / SC-004.
//!
//! All compact-reachable empty-index strings funnel through ONE source —
//! `format::empty_index_recovery_hint(profile)` — which `format::empty_guard_message`
//! (the `loading_guard!` macro target and the ~26 guard sites) and the
//! `symforge_edit` apply guard both call (N-5). Asserting the funnel proves every
//! site routed through it is clean; the standalone `what_changed` / `SessionStale`
//! strings are covered by in-crate unit tests (`tools.rs`, `edit_tools.rs`) that
//! run on the same profile-aware logic.
//!
//! T029 — cold-start root discovery resolves the workspace, not the home dir:
//! the `SYMFORGE_WORKSPACE_ROOT` override threaded by `symforge init` is honored
//! by `find_project_root` and validated through the same trust-boundary guard, so
//! a forbidden (home / broad) override is ignored. Live-verify gap noted below.

#[path = "support/stel_surface_env.rs"]
mod stel_surface_env;

use symforge::discovery::{WORKSPACE_ROOT_ENV, find_project_root};
use symforge::protocol::format::{empty_guard_message, empty_index_recovery_hint};
use symforge::protocol::surface_probe::{SurfaceProfile, surface_profile_from_env};
use tempfile::TempDir;

/// Tools the compact dispatch gate forbids; naming any of these in a recovery
/// message reachable on the compact surface is the TR-02 dead-end (the agent is
/// told to call a tool its surface rejects at `tools/call`). The compact surface
/// exposes only `symforge` / `symforge_edit` / `status`, so any other tool name
/// in a compact recovery string is a dead-end. This is a representative set of
/// the legacy tools an empty-index/recovery message would plausibly name — not
/// just the historical `index_folder` — so a future message that swaps in a
/// different gated tool is still caught.
const COMPACT_BLOCKED_TOOLS: &[&str] = &[
    "index_folder",
    "get_repo_map",
    "search_text",
    "search_symbols",
    "search_files",
    "find_references",
    "find_dependents",
    "get_symbol",
    "get_file_context",
    "get_file_content",
    "replace_symbol_body",
    "what_changed",
];

fn assert_no_blocked_tool(message: &str, context: &str) {
    for blocked in COMPACT_BLOCKED_TOOLS {
        assert!(
            !message.contains(blocked),
            "compact recovery message [{context}] names the surface-forbidden tool \
             `{blocked}`: {message}"
        );
    }
}

// ---------------------------------------------------------------------------
// T028: compact recovery never names a blocked tool, always names a callable one
// ---------------------------------------------------------------------------

#[tokio::test]
async fn compact_surface_index_not_loaded_message_never_mentions_blocked_tools() {
    let _serialize = stel_surface_env::COMPACT_ENV_LOCK.lock().await;
    let _surface = stel_surface_env::set_symforge_surface("compact");

    assert_eq!(
        surface_profile_from_env(),
        SurfaceProfile::Compact,
        "this test must run on the compact surface"
    );

    // The centralized hint, computed from the active (compact) surface.
    let hint = empty_index_recovery_hint(SurfaceProfile::Compact);
    assert_no_blocked_tool(&hint, "empty_index_recovery_hint(Compact)");

    // The env-resolved guard message (the `loading_guard!` macro target and the
    // ~26 guard call sites all route through this; on compact it must resolve to
    // the surface-aware hint, not the legacy "Call index_folder" string).
    let guard = empty_guard_message();
    assert_no_blocked_tool(
        &guard,
        "empty_guard_message() under SYMFORGE_SURFACE=compact",
    );

    // Both must name a recovery the agent CAN actually perform on compact.
    for (msg, ctx) in [(&hint, "hint"), (&guard, "guard")] {
        assert!(
            msg.contains("project root"),
            "compact recovery [{ctx}] must name the re-launch-from-root step: {msg}"
        );
        assert!(
            msg.contains("SYMFORGE_SURFACE=full"),
            "compact recovery [{ctx}] must name the documented opt-out: {msg}"
        );
    }
}

/// The same message on the full surface MAY name `index_folder` (it is callable
/// there) — the hint is computed from the active surface, never a fixed string.
#[tokio::test]
async fn full_surface_recovery_may_name_index_folder() {
    let _serialize = stel_surface_env::COMPACT_ENV_LOCK.lock().await;
    let _surface = stel_surface_env::set_symforge_surface("full");

    assert_eq!(surface_profile_from_env(), SurfaceProfile::Full);

    // The env-resolved guard now reflects the full surface and is allowed to name
    // index_folder — proving the message is surface-derived, not hardcoded.
    let guard = empty_guard_message();
    assert!(
        guard.contains("index_folder"),
        "full-surface guard may name index_folder (it is callable): {guard}"
    );

    // And the centralized hint agrees per-profile.
    assert!(empty_index_recovery_hint(SurfaceProfile::Full).contains("index_folder"));
    assert!(!empty_index_recovery_hint(SurfaceProfile::Compact).contains("index_folder"));
}

// ---------------------------------------------------------------------------
// T029: cold-start root discovery resolves the workspace, not the home dir
// ---------------------------------------------------------------------------

/// A real workspace handed to `find_project_root` via `SYMFORGE_WORKSPACE_ROOT`
/// (the env `symforge init` writes) is honored — cold start indexes that
/// workspace instead of falling back to the (forbidden) home dir.
#[tokio::test]
async fn workspace_root_env_override_resolves_the_workspace() {
    let _serialize = stel_surface_env::COMPACT_ENV_LOCK.lock().await;
    let workspace = TempDir::new().unwrap();
    let canonical = std::fs::canonicalize(workspace.path()).unwrap();

    let _root = stel_surface_env::EnvVarGuard::set(
        WORKSPACE_ROOT_ENV,
        &workspace.path().display().to_string(),
    );

    let resolved = find_project_root().expect("workspace override must resolve a root");
    let resolved_canonical = std::fs::canonicalize(&resolved).unwrap_or(resolved);
    assert_eq!(
        resolved_canonical, canonical,
        "find_project_root must honor SYMFORGE_WORKSPACE_ROOT over CWD discovery"
    );
}

/// A forbidden override (the user's home directory) is IGNORED — the override is
/// validated through the same trust-boundary guard as CWD discovery, so it can
/// never widen what gets auto-indexed. It must NOT resolve to the home dir.
#[tokio::test]
async fn workspace_root_env_override_rejects_forbidden_home_dir() {
    let _serialize = stel_surface_env::COMPACT_ENV_LOCK.lock().await;

    let home = if cfg!(windows) {
        std::env::var("USERPROFILE").ok()
    } else {
        std::env::var("HOME").ok()
    };
    let Some(home) = home else {
        // No home dir available in this environment; nothing to assert.
        return;
    };

    let _root = stel_surface_env::EnvVarGuard::set(WORKSPACE_ROOT_ENV, &home);

    // The home dir is a forbidden root, so the override is rejected and discovery
    // falls back to CWD. The result (whatever CWD resolves to) must never be the
    // home dir itself — that was the TR-03 empty-index trap.
    let canonical_home = std::fs::canonicalize(&home).unwrap_or_else(|_| home.clone().into());
    if let Some(resolved) = find_project_root() {
        let resolved_canonical = std::fs::canonicalize(&resolved).unwrap_or(resolved);
        assert_ne!(
            resolved_canonical, canonical_home,
            "a forbidden SYMFORGE_WORKSPACE_ROOT=home must never resolve to the home dir"
        );
    }
}

// LIVE-VERIFY GAP (honest disclosure):
// These tests prove the *mechanism* — `find_project_root` honors a validated
// workspace override, and `symforge init` writes that override + a non-`%USERPROFILE%`
// wrapper CWD (asserted in `src/cli/init.rs` unit tests). They do NOT drive the real
// Claude Desktop launch (CWD = System32 on Windows), which cannot be reproduced in a
// unit/integration test without the Desktop process. The end-to-end "cold start under
// Claude Desktop now binds a populated index" claim must be confirmed by a live dogfood
// (install via `symforge init --client claude-desktop`, launch Desktop, read `status`).
