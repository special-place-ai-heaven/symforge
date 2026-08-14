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

/// How a completed server run ended. The two variants are TRANSCRIBED from
/// the frozen contract record — an earlier draft invented a `Clean` variant
/// here, the exact T043 failure mode, caught by T049's consumer fixtures and
/// corrected against `public-api-v11.json`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerExit {
    /// The server declined to come up and reported that as its exit.
    RefusedToStart,
    /// Clean successful shutdown.
    Success,
}

/// The contract entry point, `run(args) -> Result<ServerExit,
/// ServerBootstrapError>`. A STAND-IN: it refuses with `ActivationPending`
/// and consumes nothing — reporting a server run nothing performed would be
/// the invariant violation this feature exists to prevent. Slice 4 wires it.
pub fn run(_args: Vec<OsString>) -> Result<ServerExit, ServerBootstrapError> {
    Err(ServerBootstrapError::ActivationPending)
}

// T049: `server_api` is `pub(crate)` until the keyword flip, so NO external
// crate — the dependent-positive fixture and the integration tests included —
// can name it. That unreachability is the point, and it means the contract
// shapes can only be pinned from inside the crate: this test is the
// server-consumer leg of the AAP migration receipt.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dark_run_refuses_and_the_contract_shapes_hold() {
        let refusal = run(Vec::new())
            .expect_err("the stand-in refuses; a server run nothing performed is not reported");
        let rendered = format!("{refusal}");
        assert!(
            rendered.contains("activation cut"),
            "Display names the cut: {rendered}"
        );
        let _as_error: &dyn std::error::Error = &refusal;

        // The contract's two exit variants, verbatim from the frozen record;
        // the match keeps the set CLOSED — a third variant fails compilation
        // here before it can drift the surface.
        let exits = [ServerExit::RefusedToStart, ServerExit::Success];
        for exit in exits {
            match exit {
                ServerExit::RefusedToStart | ServerExit::Success => {}
            }
        }

        fn all_five_auto<
            T: Send + Sync + Unpin + std::panic::RefUnwindSafe + std::panic::UnwindSafe,
        >() {
        }
        all_five_auto::<ServerBootstrapError>();
        all_five_auto::<ServerExit>();
    }
}
