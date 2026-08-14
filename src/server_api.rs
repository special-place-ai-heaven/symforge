//! Feature 020 V11, T048/D4 (amended by the C2 ruling) — the flip-ready
//! server API module.
//!
//! REAL, not fictional: the contract's four `symforge::server_api` atoms name
//! this module, and activation is ONE KEYWORD — the `pub(crate)` on the
//! `lib.rs` declaration becomes `pub` BEHIND THE ALREADY-PRESENT
//! `cfg(feature = "server")` gate, at which point the census gains exactly
//! these atoms in server graphs and never in an embed cell: the frozen
//! contract pins this module's availability as `feature=server` and the
//! embed-v11 projection EXCLUDES it. D4's earlier "std-only so the embed
//! build compiles it unused" sentence is amended — std-only stays true, but
//! the module no longer compiles under embed at all. No `pub use` re-exports
//! it anywhere, and it holds NO call edge into `index_lifecycle`: wiring
//! `run` to the dark factory is Slice 4 activation work, not a stand-in's
//! business.

// Unreachable BY DESIGN until the keyword flip: no consumer may exist before
// the activation cut, and the crate denies warnings, so the module carries its
// own allow — scoped here, never crate-wide — with this sentence as the
// receipt. Slice 4 deletes it when `run` gains its real caller.
#![allow(dead_code)]

use std::ffi::OsString;

/// Why the server could not come up. OPAQUE by contract (C1 ruling): the
/// frozen record pins `kind: "struct"` with no public fields and
/// `has_nonpublic_fields: true`, so external code can neither construct one
/// nor match it exhaustively — every future refusal cause is non-breaking.
/// An earlier draft shipped a public enum with an `ActivationPending`
/// variant, the third T043-class invention caught in this file; corrected
/// against `public-api-v11.json`.
#[derive(Debug)]
pub struct ServerBootstrapError {
    reason: &'static str,
}

impl ServerBootstrapError {
    /// The stand-in's only cause until the activation cut: the V11 server
    /// entry is not wired, and this refuses rather than pretending a server
    /// ran.
    fn activation_pending() -> Self {
        Self {
            reason: "server_api::run is not wired until the V11 activation cut",
        }
    }
}

impl std::fmt::Display for ServerBootstrapError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.reason)
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
/// ServerBootstrapError>`. A STAND-IN: it refuses with the opaque
/// activation-pending bootstrap error and consumes nothing — reporting a
/// server run nothing performed would be the invariant violation this
/// feature exists to prevent. Slice 4 wires it.
pub fn run(_args: Vec<OsString>) -> Result<ServerExit, ServerBootstrapError> {
    Err(ServerBootstrapError::activation_pending())
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

    /// C1 ruling: pin the ITEM KIND, not just the trait surface. The frozen
    /// contract makes `ServerBootstrapError` an OPAQUE STRUCT — external
    /// constructability must be impossible, which no runtime value-check can
    /// observe from inside the crate, so the pin is a source assertion: the
    /// declaration is a struct, never an enum, and its body holds no `pub`
    /// field. An enum with a public variant survived every trait oracle
    /// once; this is what would have caught it.
    #[test]
    fn bootstrap_error_is_an_opaque_struct_not_an_enum() {
        let source = include_str!("server_api.rs");
        // The needles are built at runtime with a REAL leading newline so
        // they match only a column-zero declaration, never these string
        // literals (which sit mid-line in the included source of this very
        // test) — without this, the pin would match itself and be vacuous.
        let struct_needle = format!("\n{} ServerBootstrapError", "pub struct");
        let enum_needle = format!("\n{} ServerBootstrapError", "pub enum");
        assert!(
            source.contains(&struct_needle),
            "the contract pins kind: struct"
        );
        assert!(
            !source.contains(&enum_needle),
            "an enum here is the invented-surface defect the contract's \
             opaque struct exists to prevent"
        );
        let declaration = source
            .split(struct_needle.as_str())
            .nth(1)
            .expect("declaration present");
        // A tuple struct (`pub struct X(pub ...)`) has no brace body, and a
        // naive `find('{')` would silently scan some LATER block — so the
        // brace must be the first non-whitespace after the name (round-2
        // hardening of this pin).
        let after_name = declaration.trim_start();
        assert!(
            after_name.starts_with('{'),
            "the declaration must open a brace body immediately — a tuple \
             struct's parenthesized fields would evade the field scan"
        );
        let body_end = after_name.find('}').expect("struct body closes");
        let body = &after_name[1..body_end];
        assert!(
            !body.contains("pub "),
            "has_nonpublic_fields: every field stays private, got: {body}"
        );
    }
}
