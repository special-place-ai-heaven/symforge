//! `stel_core` — the protocol-free STEL storage + calibration seam.
//!
//! D3-ROOT extract-up. This module holds the parts of STEL that are pure
//! STORAGE + deterministic MATH with NO dependency on the transport/protocol
//! stack (rmcp / axum / reqwest):
//!
//! - [`types`]       — POD wire/domain types (serde + schemars only).
//! - [`ledger_store`] — the durable SQLite economics ledger (`rusqlite`).
//! - [`calibration`]  — observational summary + the auto-tune pure math.
//! - [`consts`]       — the four token-economics floors the math keys on.
//!
//! Because none of these touch `crate::protocol`, `stel_core` is gated
//! `#[cfg(any(feature = "server", feature = "embed"))]` (see `src/lib.rs`) and
//! compiles under the engine-only `embed` facade — delivering FR-001 embed
//! durability — as well as the full `server` build.
//!
//! The server-only [`crate::stel`] module RE-EXPORTS these submodules
//! (`pub use crate::stel_core::{types, ledger_store, calibration};` and a
//! consts shim on `controller`), so every existing `crate::stel::…` caller
//! path resolves unchanged: the move is behavior-preserving for the server
//! build, additive for the embed build.

pub mod calibration;
pub mod consts;
pub mod ledger_store;
pub mod types;
