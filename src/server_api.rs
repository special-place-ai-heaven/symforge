//! Feature 020 V11, T048/D4 — the flip-ready server API module.
//!
//! REAL, not fictional: the contract's four `symforge::server_api` atoms name
//! this module, and activation is ONE KEYWORD — the `pub(crate)` on the
//! `lib.rs` declaration becomes `pub`, at which point the public-API census
//! gains exactly these atoms and never before. Std-only by design so the
//! embed build compiles it unused, no `pub use` re-exports it anywhere, and
//! it holds NO call edge into `index_lifecycle`: wiring `run` to the dark
//! factory is Slice 4 activation work, not a stand-in's business.

// Unreachable BY DESIGN until the keyword flip: no consumer may exist before
// the activation cut, and the crate denies warnings, so the module carries its
// own allow — scoped here, never crate-wide — with this sentence as the
// receipt. Slice 4 deletes it when `run` gains its real caller.
#![allow(dead_code)]

use std::ffi::OsString;

/// Why the server could not come up.
#[derive(Debug)]
pub enum ServerBootstrapError {
    /// The V11 server entry is not wired until the activation cut: this
    /// stand-in refuses rather than pretending a server ran.
    ActivationPending,
}

impl std::fmt::Display for ServerBootstrapError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ActivationPending => {
                write!(
                    f,
                    "server_api::run is not wired until the V11 activation cut"
                )
            }
        }
    }
}

impl std::error::Error for ServerBootstrapError {}

/// How a completed server run ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerExit {
    /// Clean shutdown.
    Clean,
}

/// The contract entry point, `run(args) -> Result<ServerExit,
/// ServerBootstrapError>`. A STAND-IN: it refuses with `ActivationPending`
/// and consumes nothing — reporting a server run nothing performed would be
/// the invariant violation this feature exists to prevent. Slice 4 wires it.
pub fn run(_args: Vec<OsString>) -> Result<ServerExit, ServerBootstrapError> {
    Err(ServerBootstrapError::ActivationPending)
}
