// Engine code is fully exercised by the default (server) build, where dead-code
// checking stays on. In `embed` builds an embedder uses only a subset of the
// engine API, so unused-but-public engine helpers are expected — not dead.
#![cfg_attr(not(feature = "server"), allow(dead_code))]

// ── The retired V10 raw surface (Feature 020 Slice 4, C5 — the exposure
// flip). The frozen migration category `v10-01-raw-crate-root-modules`
// removes every raw `pub mod` from this file; the declarations live in the
// private `internals` wrapper and the re-imports below restore every
// `crate::X` path unchanged. The public census (one atom per `pub mod`
// line HERE) holds exactly the kept + introduced modules: `embed` and
// `server_api`. The `__test-internals` arm re-exports the same names
// publicly for this repository's own integration suite ONLY — no supported
// configuration cell enables that feature (see internals.rs and
// Cargo.toml).
mod internals;

#[cfg(not(feature = "__test-internals"))]
// Engine modules the embed cell compiles but does not name from its own
// tree (server-only consumers) read as unused imports there; the server
// build keeps the lint.
#[cfg_attr(not(feature = "server"), allow(unused_imports))]
pub(crate) use internals::{
    capability, discovery, domain, edit_safety, git, gitignore_hygiene, hash, idempotency,
    knowledge, live_index, parsing, paths, process_util, watcher_state,
};
#[cfg(feature = "__test-internals")]
pub use internals::{
    capability, discovery, domain, edit_safety, git, gitignore_hygiene, hash, idempotency,
    knowledge, live_index, parsing, paths, process_util, watcher_state,
};

// The dark lifecycle directory rides its own re-import pair so the darkness
// sweep can allowlist each arm as an exact line (a declaration edge, not a
// call edge — the old live_index mount's status).
#[cfg(not(feature = "__test-internals"))]
pub(crate) use internals::index_lifecycle;
#[cfg(feature = "__test-internals")]
pub use internals::index_lifecycle;

#[cfg(all(
    any(feature = "server", feature = "embed"),
    not(feature = "__test-internals")
))]
#[cfg_attr(not(feature = "server"), allow(unused_imports))]
pub(crate) use internals::stel_core;
#[cfg(all(
    any(feature = "server", feature = "embed"),
    feature = "__test-internals"
))]
pub use internals::stel_core;

#[cfg(all(feature = "server", not(feature = "__test-internals")))]
pub(crate) use internals::{
    analytics, cli, daemon, observability, path_shadow, protocol, server, sidecar, stel,
    version_registry, watcher, worktree,
};
#[cfg(all(feature = "server", feature = "__test-internals"))]
pub use internals::{
    analytics, cli, daemon, observability, path_shadow, protocol, server, sidecar, stel,
    version_registry, watcher, worktree,
};

// Feature 020 V11 identity minting, shared by the lifecycle runtime and the
// protocol provenance types so both draw from ONE process-wide counter.
// The doorless-build lint carve-out matches internals.rs: contract-minted
// items reachable only through the (cfg'd-out) door read as dead there,
// while every linting gate builds with the door open.
#[cfg_attr(not(feature = "__test-internals"), allow(dead_code))]
pub(crate) mod lifecycle_identity;

// Feature 020 V11 (C5 — the keyword flip executed): the real server API,
// public exactly as the frozen contract's four `symforge::server_api` atoms
// pin it, behind the contract's `feature=server` availability gate. The
// embed-v11 projection excludes this module, so no embed cell carries it.
#[cfg(feature = "server")]
pub mod server_api;

// ── The V11 embedded-source facade (kept module, C5 contents) ──
#[cfg(feature = "embed")]
pub mod embed;
