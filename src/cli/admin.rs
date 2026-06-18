//! `symforge admin` — open (or start + open) the operator dashboard (009 US3).
//!
//! Reuses a running operator server when one is reachable on the remembered port
//! (no duplicate server, FR-015); otherwise starts one on a verified-free port
//! and opens/returns the dashboard URL. This module is the thin admin-verb layer
//! over the shipped 004 serve + 006 admin dashboard.
//!
//! Phase 1 (T003) lands the clap arg surface + a `run` skeleton that returns a
//! clear "not yet implemented" notice. The reachability check, reuse-or-start
//! serve helper, and browser open land in later phases (T011/T023).

use clap::Args;

/// Flags for `symforge admin` (see `contracts/admin-cli.md`).
#[derive(Args, Debug, Clone)]
pub struct AdminCliArgs {
    /// Do not attempt to open a browser; print/return the dashboard URL only.
    #[arg(long)]
    pub no_open: bool,
}

/// Entry point for `symforge admin`.
///
/// Phase 1 skeleton: the reachability -> reuse/start -> open flow lands in
/// Phase US3 (T011/T023). Until then this returns a clear not-yet-implemented
/// error rather than a fake success.
pub fn run(_args: AdminCliArgs) -> anyhow::Result<()> {
    anyhow::bail!(
        "symforge admin: the dashboard admin verb is not yet implemented (009 US3). \
         Start the server with `symforge serve` and open the printed `/admin` URL."
    )
}
