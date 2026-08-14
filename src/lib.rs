// Engine code is fully exercised by the default (server) build, where dead-code
// checking stays on. In `embed` builds an embedder uses only a subset of the
// engine API, so unused-but-public engine helpers are expected — not dead.
#![cfg_attr(not(feature = "server"), allow(dead_code))]

// ── Engine: always compiled (parsing + live_index + query + git + shared base) ──
pub mod domain;
pub mod git;
pub mod hash;
pub mod knowledge;
// Feature 020 V11 identity minting, shared by the dark lifecycle runtime and the
// protocol provenance types so both draw from ONE process-wide counter.
//
// `pub(crate)` is load-bearing, not a style choice: the frozen public-API census
// adds one atom per `pub mod NAME;` line in THIS file, so a plain `pub mod` here
// would widen a surface Slice 3 must leave frozen. Modules that expose one of
// these types publicly re-export it instead.
pub(crate) mod lifecycle_identity;
// Feature 020 V11, D4, amended by the C2 ruling (2026-08-14): the REAL
// server_api module, flip-ready. TWO gates are load-bearing: `pub(crate)`
// keeps it out of the census until activation flips that one keyword, and
// the `cfg(feature = "server")` matches the frozen contract's availability
// column — the embed-v11 projection EXCLUDES this module, so embed cells
// must never grow its four atoms. D4's earlier "std-only so the embed build
// compiles it unused" sentence is amended: std-only stays true, but the
// module no longer compiles under embed at all. No `pub use` anywhere; no
// call edge into index_lifecycle. Do NOT "tidy" either gate before the cut.
pub mod live_index;
pub mod parsing;
pub mod paths;
#[cfg(feature = "server")]
pub(crate) mod server_api;
// Watcher state snapshot types (data only) — used by engine health stats; the
// notify-based watcher runtime lives in the server-gated `watcher` module.
pub mod watcher_state;

// Engine-adjacent helpers (no server deps); used by the engine and the server.
pub mod capability;
pub mod discovery;
pub mod edit_safety;
pub mod gitignore_hygiene;
pub mod idempotency;

// ── Protocol-free STEL storage + calibration seam (D3-ROOT extract-up) ──
// The durable economics ledger + calibration math are pure STORAGE + MATH with
// no transport/protocol dependency, so they compile under BOTH the full server
// build AND the engine-only `embed` facade — delivering FR-001 embed
// durability. The server-only `stel` module re-exports these submodules so
// every existing `crate::stel::{types,ledger_store,calibration}` caller path
// resolves unchanged. Gated `any(server, embed)`: dead under neither, so no
// false embed-capability signal (unlike a bare `any(...)` on a server-coupled
// module).
#[cfg(any(feature = "server", feature = "embed"))]
pub mod stel_core;

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
// Proactive PATH-shadow detection: warns when a bare `symforge` resolves to a
// different (stale) install than the one we believe we are. Used by cli::init,
// cli::update, and protocol health — all server-only.
#[cfg(feature = "server")]
pub mod path_shadow;
// Console-flash-free child spawning (CREATE_NO_WINDOW on Windows); used by the
// daemon's helper spawns, worktree listing, PATH-shadow probes, and the updater.
// NOT server-gated: std-only, and embed-cfg'd test code in git/store/discovery
// spawns git through it (the embed --lib test build compiles those modules).
#[doc(hidden)] // public so integration tests share the no-console spawn helper
pub mod process_util;
#[cfg(feature = "server")]
pub mod protocol;
// Transport-agnostic operator server spine (v8): `symforge serve` over /mcp.
#[cfg(feature = "server")]
pub mod server;
#[cfg(feature = "server")]
pub mod sidecar;
#[cfg(feature = "server")]
pub mod stel;
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
