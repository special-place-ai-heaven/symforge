//! Protocol-free token-economics constants shared by the calibration math and
//! the (server-only) L2 controller.
//!
//! D3-ROOT extract-up: these four `u32` floors were the ONLY tie binding the
//! protocol-free `calibration` math to the server-only `controller`. Lifting
//! them into `stel_core` lets the durable ledger + calibration compile under
//! `any(server, embed)`. The server-side `controller` re-exports them
//! (`pub use crate::stel_core::consts::*;`) so every existing caller path
//! (`crate::stel::controller::COMPACT_SCHEMA_TOKENS`, ...) resolves unchanged.

/// Compact-3 worst-case schema tax per call (A-006 conservative path; no amortization credit).
pub const COMPACT_SCHEMA_TOKENS: u32 = 45;
/// Compact `symforge` invoke overhead per call (schema example + Phase 0 doctrine).
pub const COMPACT_INVOKE_TOKENS: u32 = 80;
/// Static per-step predicted-response floor used when a step carries no real
/// byte sizes (plan-only callers / fixtures). Matches the planner's
/// `est_response_tokens` (`planner.rs`); named here so the auto-tune (feature
/// 013) can derive a corrected replacement and the wiring can fall back to it.
pub const STATIC_RESPONSE_FLOOR: u32 = 400;
/// Static per-step manual-baseline floor (the `est_manual_tokens` counterpart to
/// [`STATIC_RESPONSE_FLOOR`]); the auto-tune's `manual_floor` replaces it when a
/// validated tuning is in force.
pub const STATIC_MANUAL_FLOOR: u32 = 800;
