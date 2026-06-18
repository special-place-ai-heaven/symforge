//! `symforge setup` — guided operator setup wizard (009 US2).
//!
//! Orchestrates the shipped 004/005/006 capabilities (harness scan/apply,
//! serve, admin dashboard) into one guided command: scan -> choose -> restate
//! -> apply (with restorable backups) -> serve-start -> open -> persist. This
//! module is the thin wizard layer; it does not reimplement scan/apply/serve.
//!
//! Phase 1 (T003) lands the clap arg surface + a `run` skeleton that returns a
//! clear "not yet implemented" notice. The flow steps (scan/choose/apply/serve)
//! and the `SetupSink` seam land in later phases (T009/T014-T019).

use clap::{Args, ValueEnum};

/// Which parts of SymForge a setup run configures.
///
/// Mirrors the `--installation-type` contract values (contracts/setup-cli.md):
/// `in-harness` configures harness MCP attach only; `server` starts the
/// operator server/dashboard; `both` does both.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum InstallationType {
    /// Configure harness MCP attach entries only (no operator server).
    #[value(name = "in-harness")]
    InHarness,
    /// Start the operator server / dashboard.
    Server,
    /// Configure harnesses and start the operator server.
    Both,
}

/// Flags for `symforge setup` (see `contracts/setup-cli.md`).
#[derive(Args, Debug, Clone)]
pub struct SetupCliArgs {
    /// Drive with pre-supplied answers: no terminal read, no browser, no network
    /// probe beyond the bind (FR-014).
    #[arg(long)]
    pub non_interactive: bool,

    /// Pre-answer the install type (`in-harness` | `server` | `both`).
    #[arg(long, value_enum)]
    pub installation_type: Option<InstallationType>,

    /// Preferred bind port (else the verified-free suggestion).
    #[arg(long)]
    pub port: Option<u16>,

    /// Comma-separated harness ids to configure (else all detected).
    #[arg(long, value_delimiter = ',')]
    pub harnesses: Vec<String>,

    /// Auto-confirm the restated action plan (for scripts).
    #[arg(long)]
    pub yes: bool,
}

/// Entry point for `symforge setup`.
///
/// Phase 1 skeleton: the wizard flow (scan/choose/apply/serve-start/open/persist)
/// is implemented in Phase US2 (T014-T019). Until then this returns a clear
/// not-yet-implemented error rather than a fake success.
pub fn run(_args: SetupCliArgs) -> anyhow::Result<()> {
    anyhow::bail!(
        "symforge setup: the guided setup wizard is not yet implemented (009 US2). \
         Use `symforge init` to configure harnesses and `symforge serve` to start the dashboard."
    )
}
