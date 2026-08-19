//! `symforge::embed` — the V11 embedded-source facade (Feature 020 Slice 4,
//! C5: the exposure flip).
//!
//! Depend on it with:
//!
//! ```toml
//! symforge = { version = "*", default-features = false, features = ["embed"] }
//! ```
//!
//! This namespace exposes EXACTLY the frozen contract's V11 embed atoms
//! (`specs/020-repository-knowledge-index/contracts/public-api-v11.json`,
//! `migration_v10.introduced_v11_atoms`) plus the kept engine identity
//! (`EngineInfo`/`engine_info`). The V10 raw surface this file used to
//! re-export — live-index state, authorityless search, raw per-file
//! mutation, snapshot loaders, parser/domain types, `GitRepo`, the STEL
//! ledger, and the deep module re-exports — is RETIRED per the frozen
//! category dispositions: an embedder now opens ONE
//! [`EmbeddedSourceHandle`] through [`ProcessIndexRuntime`] and works with
//! typed claims, receipts, and refusals instead of raw engine internals.
//!
//! SEMVER-PUBLIC: this facade is the interface contract between the
//! SymForge engine and its embedders. This activation cut is the
//! contract's one planned MAJOR break (the V10→V11 migration the Feature
//! 020 campaign froze); from here, any breaking change to a name or
//! signature below is a MAJOR bump. The `#[cfg(test)]` `contract` module
//! at the bottom names every contracted item so a rename, removal, or
//! signature drift becomes a COMPILE FAILURE in SymForge's own embed test
//! suite rather than a downstream surprise.

// ── The kept engine identity (migration category v10-03, decision: keep) ──

/// Engine identity for embedder readiness evidence: one call, no I/O.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct EngineInfo {
    /// The symforge crate version compiled into this engine.
    pub version: &'static str,
    /// On-disk snapshot format version written/restored by this engine.
    pub snapshot_format_version: u32,
    /// Secret-detector policy version enforced at admission and raw reads.
    pub secret_policy_version: u32,
    /// Stable lowercase names of every supported grammar.
    pub grammars: &'static [&'static str],
}

/// The [`EngineInfo`] for this build. Values are compile-time constants.
pub fn engine_info() -> EngineInfo {
    const GRAMMAR_NAMES: [&str; crate::domain::LanguageId::ALL.len()] = {
        let mut names = [""; crate::domain::LanguageId::ALL.len()];
        let mut i = 0;
        while i < crate::domain::LanguageId::ALL.len() {
            names[i] = crate::domain::LanguageId::ALL[i].name();
            i += 1;
        }
        names
    };
    EngineInfo {
        version: env!("CARGO_PKG_VERSION"),
        snapshot_format_version: crate::live_index::persist::SNAPSHOT_FORMAT_VERSION,
        secret_policy_version: crate::knowledge::SECRET_POLICY_VERSION,
        grammars: &GRAMMAR_NAMES,
    }
}

// ── The introduced V11 atoms (migration_v10.introduced_v11_atoms) ──
//
// Contract shapes live in the boundary module
// (`index_lifecycle/public_api.rs`) and on the seam-pinned handle
// (`index_lifecycle/embedded.rs`); the wrap table
// (`public_api::V11PublicApi::wrap_table`) records how each atom is
// satisfied — a contract-shaped wrapper (`wrapped-here`) or a
// contract-verbatim enum made nameable by re-export
// (`verbatim-reexport`). Nothing pre-existing leaks: the internal names
// are aliased to the contract names HERE, and this facade is their only
// public door.

pub use crate::index_lifecycle::embedded::SourceCloseReceipt;
pub use crate::index_lifecycle::embedded::{EmbeddedSourceHandle, ReceiptWaitError};
pub use crate::index_lifecycle::public_api::{
    EmbedAtomicAuthority as AtomicAuthority, EmbedClaim as Claim,
    EmbedClaimProvenance as ClaimProvenance, EmbedEvaluationProvenance as EvaluationProvenance,
    EmbedOperationReceipt as OperationReceipt, EmbedRefreshTicket as RefreshTicket,
    EmbedShutdownReceipt as ShutdownReceipt, EmbedSourceRefusal as SourceRefusal,
    EmbeddedSourceSpec, OperationKind, ProcessRuntimeApi as ProcessIndexRuntime, RetryAdvice,
    ShutdownReport, SourceCloseReport, SourceRefusalKind, SourceRuntimePhase, SourceRuntimeView,
    SymbolMatch, SymbolSearchRequest, SymbolSearchResult, TextMatch, TextSearchRequest,
    TextSearchResult,
};

#[cfg(test)]
mod contract {
    //! Compile-time tripwire for the semver-public V11 embedder facade.
    //!
    //! Names every contracted atom so a removed or renamed item fails the
    //! `use`, and pins the runtime/handle entry points with bindings so a
    //! signature drift fails to type-check. The deep behavioral pins live
    //! in the activation-cut oracles (`tests/activation_cut_v11.rs`).
    #[allow(unused_imports)]
    use crate::embed::{
        AtomicAuthority, Claim, ClaimProvenance, EmbeddedSourceHandle, EmbeddedSourceSpec,
        EngineInfo, EvaluationProvenance, OperationKind, OperationReceipt, ProcessIndexRuntime,
        ReceiptWaitError, RefreshTicket, RetryAdvice, ShutdownReceipt, ShutdownReport,
        SourceCloseReceipt, SourceCloseReport, SourceRefusal, SourceRefusalKind,
        SourceRuntimePhase, SourceRuntimeView, SymbolMatch, SymbolSearchRequest,
        SymbolSearchResult, TextMatch, TextSearchRequest, TextSearchResult, engine_info,
    };

    #[test]
    fn facade_contract_is_stable() {
        fn _assert_named<T>() {}
        _assert_named::<AtomicAuthority>();
        _assert_named::<Claim<u64>>();
        _assert_named::<ClaimProvenance>();
        _assert_named::<EmbeddedSourceHandle>();
        _assert_named::<EmbeddedSourceSpec>();
        _assert_named::<EvaluationProvenance>();
        _assert_named::<OperationKind>();
        _assert_named::<OperationReceipt>();
        _assert_named::<ProcessIndexRuntime>();
        _assert_named::<ReceiptWaitError>();
        _assert_named::<RefreshTicket>();
        _assert_named::<RetryAdvice>();
        _assert_named::<ShutdownReceipt>();
        _assert_named::<ShutdownReport>();
        _assert_named::<SourceCloseReceipt>();
        _assert_named::<SourceCloseReport>();
        _assert_named::<SourceRefusal>();
        _assert_named::<SourceRefusalKind>();
        _assert_named::<SourceRuntimePhase>();
        _assert_named::<SourceRuntimeView>();
        _assert_named::<SymbolMatch>();
        _assert_named::<SymbolSearchRequest>();
        _assert_named::<SymbolSearchResult>();
        _assert_named::<TextMatch>();
        _assert_named::<TextSearchRequest>();
        _assert_named::<TextSearchResult>();

        // The contract entry points, pinned as bindings so arity or type
        // drift fails compilation.
        let _acquire: fn() -> Result<ProcessIndexRuntime, SourceRefusal> =
            ProcessIndexRuntime::acquire;
        let _open: fn(
            &ProcessIndexRuntime,
            EmbeddedSourceSpec,
        ) -> Result<EmbeddedSourceHandle, SourceRefusal> =
            ProcessIndexRuntime::open_embedded_source;
        let _spec: fn(std::path::PathBuf) -> EmbeddedSourceSpec =
            EmbeddedSourceSpec::current_worktree;
        let _engine_info: fn() -> EngineInfo = engine_info;
        let _ = (_acquire, _open, _spec, _engine_info);

        // Engine identity is real data, not just a signature.
        let info = crate::embed::engine_info();
        assert!(!info.version.is_empty());
        assert_eq!(
            info.grammars.len(),
            crate::domain::LanguageId::ALL.len(),
            "every grammar is reported"
        );
        assert!(info.grammars.contains(&"rust"));
        assert!(info.snapshot_format_version >= 7);
    }
}
