//! Feature 020 V11, `symforge::server_api` — the public server entry
//! (C5: the keyword flip executed, the entry wired).
//!
//! The contract's four atoms name this module: [`run`], [`ServerExit`],
//! [`ServerBootstrapError`], and the module itself, public behind the
//! frozen `feature=server` availability gate (the embed-v11 projection
//! excludes it). `run` dispatches the whole symforge CLI through
//! `cli::entry::run_main` — the binary is a shim over this door, and the
//! retired raw module surface is no longer reachable from outside the
//! crate.

use std::ffi::OsString;

/// Why the server could not come up. OPAQUE by contract (C1 ruling): the
/// frozen record pins `kind: "struct"` with no public fields and
/// `has_nonpublic_fields: true`, so external code can neither construct one
/// nor match it exhaustively — every future refusal cause is non-breaking.
///
/// The cause chain is CAPTURED AS TEXT at construction: the frozen
/// trait_impls pin all five auto traits on this type, and holding the
/// dispatch error itself (`anyhow::Error`) would lose `RefUnwindSafe`.
#[derive(Debug)]
pub struct ServerBootstrapError {
    reason: String,
}

impl ServerBootstrapError {
    /// Wrap a dispatch failure, rendering its full cause chain.
    fn from_dispatch(error: &anyhow::Error) -> Self {
        Self {
            reason: format!("{error:#}"),
        }
    }
}

impl std::fmt::Display for ServerBootstrapError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.reason)
    }
}

impl std::error::Error for ServerBootstrapError {}

/// How a completed server run ended. The two variants are TRANSCRIBED from
/// the frozen contract record — an earlier draft invented a `Clean` variant
/// here, the exact T043 failure mode, caught by T049's consumer fixtures and
/// corrected against `public-api-v11.json`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerExit {
    /// The server declined to come up and reported that as its exit (the
    /// cli-serve contract maps this to process exit code 2).
    RefusedToStart,
    /// Clean successful shutdown.
    Success,
}

/// The contract entry point: run the symforge CLI with `args` (`argv\[0\]`
/// included). Dispatches every subcommand exactly as the binary always
/// has — daemon, serve, stdio MCP, init/setup/admin/hook/trust/update —
/// and maps the typed outcome onto the contract shapes. Argument-parse
/// failures follow CLI convention (clap prints usage and exits).
pub fn run(args: Vec<OsString>) -> Result<ServerExit, ServerBootstrapError> {
    match crate::cli::entry::run_main(args) {
        Ok(crate::cli::entry::MainExit::Success) => Ok(ServerExit::Success),
        Ok(crate::cli::entry::MainExit::ServeRefusedToStart) => Ok(ServerExit::RefusedToStart),
        Err(error) => Err(ServerBootstrapError::from_dispatch(&error)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// C5: the wired entry preserves the contract shapes the dark stand-in
    /// pinned — the two exit variants stay a closed set, the bootstrap
    /// error keeps all five auto traits, and `run` actually dispatches
    /// (the `--version` fast path completes and reports success).
    #[test]
    fn wired_run_dispatches_and_the_contract_shapes_hold() {
        let exit = run(vec![
            OsString::from("symforge"),
            OsString::from("--version"),
        ])
        .expect("the --version fast path succeeds");
        assert_eq!(exit, ServerExit::Success);

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

        // The error renders its captured cause chain.
        let refusal = ServerBootstrapError::from_dispatch(&anyhow::anyhow!("boom"));
        assert!(format!("{refusal}").contains("boom"));
        let _as_error: &dyn std::error::Error = &refusal;
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
