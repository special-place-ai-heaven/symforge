//! Frozen fixture: a REAL copy of src/stel_core/types.rs, plus a rename target.
//! Used by scripts/verify-tools.cjs to hunt dropped-match bugs on real repo code.
//! Frozen on purpose — do not re-sync from the live file, or snapshots drift.
pub mod code_target;
pub mod types;
