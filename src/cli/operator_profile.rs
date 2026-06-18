//! `OperatorSetupProfile` — operator-local setup convenience state (009, D5).
//!
//! Persisted to `<project>/.symforge/operator-setup.json`, mirroring the
//! `OnboardingState` load/save pattern (`cli::onboarding`). Drives reuse-if-running
//! and idempotent re-run (FR-012/013). This is operator convenience state, not an
//! index (Constitution I unaffected).
//!
//! Phase 1 (T003) is a compiling skeleton: the fields, `load()`/`save()`, and the
//! `.symforge/operator-setup.json` path land in Phase 3 (Foundational, T008).
//! Logic is intentionally deferred here.

/// On-disk filename for the operator setup profile inside the SymForge data dir.
pub const OPERATOR_SETUP_PROFILE_FILE: &str = "operator-setup.json";
