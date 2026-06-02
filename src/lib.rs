// Engine code is fully exercised by the default (server) build, where dead-code
// checking stays on. In `embed` builds an embedder uses only a subset of the
// engine API, so unused-but-public engine helpers are expected — not dead.
#![cfg_attr(not(feature = "server"), allow(dead_code))]

// ── Engine: always compiled (parsing + live_index + query + git + shared base) ──
pub mod domain;
pub mod git;
pub mod hash;
pub mod live_index;
pub mod parsing;
pub mod paths;
// Watcher state snapshot types (data only) — used by engine health stats; the
// notify-based watcher runtime lives in the server-gated `watcher` module.
pub mod watcher_state;

// Engine-adjacent helpers (no server deps); used by the engine and the server.
pub mod capability;
pub mod discovery;
pub mod edit_safety;
pub mod idempotency;

// ── Server surface: excluded from `--no-default-features --features embed` ──
// daemon/sidecar/protocol-server/CLI + their heavy deps (axum, rmcp, clap,
// reqwest, notify, tracing-subscriber). Library embedders never compile these.
#[cfg(feature = "server")]
pub mod analytics;
#[cfg(feature = "server")]
pub mod cli;
#[cfg(feature = "server")]
pub mod daemon;
#[cfg(feature = "server")]
pub mod observability;
#[cfg(feature = "server")]
pub mod protocol;
#[cfg(feature = "server")]
pub mod sidecar;
#[cfg(feature = "server")]
pub mod version_registry;
#[cfg(feature = "server")]
pub mod watcher;
// Worktree routing hooks into the protocol edit registry — server-only.
#[cfg(feature = "server")]
pub mod worktree;

// ── Engine-only facade for library embedders (e.g. AAP) ──
#[cfg(feature = "embed")]
pub mod embed;
