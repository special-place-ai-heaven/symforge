//! The retired V10 raw module surface (Feature 020 Slice 4, C5 — the
//! exposure flip). Every module the frozen migration category
//! `v10-01-raw-crate-root-modules` REMOVES from the public crate root is
//! declared here, inside a private wrapper, and re-imported at the crate
//! root with `pub(crate)` visibility — every `crate::X` path in the tree
//! resolves unchanged while `lib.rs`'s public census holds exactly the
//! kept + introduced V11 atoms (`embed`, `server_api`).
//!
//! The `__test-internals` dunder feature (enabled ONLY by this repository's
//! dev-dependency on itself — no supported configuration cell, no default
//! feature, no downstream consumer) swaps the root re-imports to `pub`, so
//! the integration suite keeps naming `symforge::X` without widening any
//! supported cell's public graph.
//!
//! Every `#[path]` below is a same-directory remount the wrapper requires
//! (child files of a non-mod-rs module would otherwise resolve under
//! `src/internals/`); each is allowlisted in the source-splicing sweep.

// Without the door, the wrapped modules' formerly-public items are
// structurally unreachable, and rustc counts an unreachable re-export as an
// unused import and an unreachable pub item as dead code. Suppress those
// lints ONLY in the doorless build: every gate that lints (`cargo test`,
// `clippy --all-targets`) builds with the door open through the self
// dev-dependency, so genuinely dead items stay caught.
#![cfg_attr(not(feature = "__test-internals"), allow(unused_imports, dead_code))]

// ── Engine: always compiled (parsing + live_index + query + git + shared base) ──
#[path = "capability/mod.rs"]
pub mod capability;
#[path = "discovery/mod.rs"]
pub mod discovery;
#[path = "domain/mod.rs"]
pub mod domain;
#[path = "edit_safety/mod.rs"]
pub mod edit_safety;
#[path = "git.rs"]
pub mod git;
#[path = "gitignore_hygiene.rs"]
pub mod gitignore_hygiene;
#[path = "hash.rs"]
pub mod hash;
#[path = "idempotency.rs"]
pub mod idempotency;
// The dark lifecycle directory, mounted at the crate root since C5 (the
// campaign's pre-plotted flip: the `#[path]` mount inside `live_index`
// became this declaration; `live_index/mod.rs` aliases it so every
// `crate::live_index::index_lifecycle::` path resolves unchanged).
#[path = "index_lifecycle/mod.rs"]
pub mod index_lifecycle;
#[path = "knowledge/mod.rs"]
pub mod knowledge;
#[path = "live_index/mod.rs"]
pub mod live_index;
#[path = "parsing/mod.rs"]
pub mod parsing;
#[path = "paths.rs"]
pub mod paths;
// Console-flash-free child spawning (CREATE_NO_WINDOW on Windows); used by
// the daemon's helper spawns, worktree listing, PATH-shadow probes, the
// updater, and the integration suite (through the test door). Not
// server-gated: embed-cfg'd test code in git/store/discovery spawns git
// through it.
#[path = "process_util.rs"]
pub mod process_util;
// Watcher state snapshot types (data only) — used by engine health stats;
// the notify-based watcher runtime lives in the server-gated `watcher`.
#[path = "watcher_state.rs"]
pub mod watcher_state;

// ── Protocol-free STEL storage + calibration seam (D3-ROOT extract-up) ──
#[cfg(any(feature = "server", feature = "embed"))]
#[path = "stel_core/mod.rs"]
pub mod stel_core;

// ── Server surface: excluded from `--no-default-features --features embed` ──
#[cfg(feature = "server")]
#[path = "analytics/mod.rs"]
pub mod analytics;
#[cfg(feature = "server")]
#[path = "cli/mod.rs"]
pub mod cli;
#[cfg(feature = "server")]
#[path = "daemon.rs"]
pub mod daemon;
#[cfg(feature = "server")]
#[path = "observability.rs"]
pub mod observability;
// Proactive PATH-shadow detection: warns when a bare `symforge` resolves to
// a different (stale) install than the one we believe we are.
#[cfg(feature = "server")]
#[path = "path_shadow.rs"]
pub mod path_shadow;
#[cfg(feature = "server")]
#[path = "protocol/mod.rs"]
pub mod protocol;
// Transport-agnostic operator server spine (v8): `symforge serve` over /mcp.
#[cfg(feature = "server")]
#[path = "server/mod.rs"]
pub mod server;
#[cfg(feature = "server")]
#[path = "sidecar/mod.rs"]
pub mod sidecar;
#[cfg(feature = "server")]
#[path = "stel/mod.rs"]
pub mod stel;
#[cfg(feature = "server")]
#[path = "version_registry.rs"]
pub mod version_registry;
#[cfg(feature = "server")]
#[path = "watcher/mod.rs"]
pub mod watcher;
// Worktree routing hooks into the protocol edit registry — server-only.
#[cfg(feature = "server")]
#[path = "worktree.rs"]
pub mod worktree;
