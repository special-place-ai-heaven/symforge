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
//!
//! ## Single-Open & Per-File Refresh Rules
//!
//! - **Single open handle per canonical root per process (`S12`)**:
//!   [`ProcessIndexRuntime::open_embedded_source`] enforces at most one open
//!   [`EmbeddedSourceHandle`] per canonical project folder across the entire
//!   process (even across multiple [`ProcessIndexRuntime::acquire`] instances).
//!   A second open of an already-open canonical root refuses with
//!   [`SourceRefusal`] ([`SourceRefusalKind::SelectionUnavailable`],
//!   [`OperationKind::OpenEmbeddedSource`], [`RetryAdvice::OnEvent`]) until the
//!   existing handle is closed (`close()`, `begin_close()`, or [`Drop`]).
//!   Embedders that share an index across components in one process should
//!   share the [`EmbeddedSourceHandle`] (e.g. behind [`std::sync::Arc`]) rather
//!   than opening the same folder twice.
//! - **Single-file re-admission (`S10`)**:
//!   [`EmbeddedSourceHandle::request_refresh`] queues an asynchronous
//!   whole-worktree reload. To re-index a single modified or newly created file
//!   synchronously without re-parsing the rest of the worktree, call
//!   `handle.query(&QueryRequest::FileImpact(FileImpactRequest { path, new_file, .. }), limits)`.
//!   `FileImpact` re-reads and re-parses only `path` through the canonical
//!   single-file admission seam (`O(1)` single-file cost), publishes the
//!   resulting `Current` generation (`source_version` advances), and leaves
//!   [`EmbeddedSourceHandle::index_census`] and subsequent queries immediately
//!   consistent for `path`.

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
    CensusFile, CensusParseStatus, CensusVerifyPhase, EmbedAtomicAuthority as AtomicAuthority,
    EmbedClaim as Claim, EmbedClaimProvenance as ClaimProvenance,
    EmbedEvaluationProvenance as EvaluationProvenance, EmbedOperationReceipt as OperationReceipt,
    EmbedRefreshTicket as RefreshTicket, EmbedShutdownReceipt as ShutdownReceipt,
    EmbedSourceRefusal as SourceRefusal, EmbeddedSourceSpec, IndexCensus, IndexProgress,
    KnowledgeMatch, KnowledgeSearchRequest, KnowledgeSearchResult, OperationKind,
    ProcessRuntimeApi as ProcessIndexRuntime, RetryAdvice, ShutdownReport, SourceCloseReport,
    SourceRefusalKind, SourceRuntimePhase, SourceRuntimeView, SymbolMatch, SymbolSearchRequest,
    SymbolSearchResult, TextMatch, TextSearchRequest, TextSearchResult,
};

/// Crate-internal door to the lifecycle modules the parity children consume,
/// so this file stays the only embed file that names the boundary.
pub(crate) mod lifecycle {
    pub(crate) use crate::index_lifecycle::{
        embed_edit_body, embed_federation, embed_host, embed_query, embed_session, guidance,
        public_api,
    };
}

/// Additive engine query contracts, versioned separately from the frozen V11 core.
pub mod parity;

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
        AtomicAuthority, CensusFile, CensusParseStatus, CensusVerifyPhase, Claim, ClaimProvenance,
        EmbeddedSourceHandle, EmbeddedSourceSpec, EngineInfo, EvaluationProvenance, IndexCensus,
        IndexProgress, KnowledgeMatch, KnowledgeSearchRequest, KnowledgeSearchResult,
        OperationKind, OperationReceipt, ProcessIndexRuntime, ReceiptWaitError, RefreshTicket,
        RetryAdvice, ShutdownReceipt, ShutdownReport, SourceCloseReceipt, SourceCloseReport,
        SourceRefusal, SourceRefusalKind, SourceRuntimePhase, SourceRuntimeView, SymbolMatch,
        SymbolSearchRequest, SymbolSearchResult, TextMatch, TextSearchRequest, TextSearchResult,
        engine_info,
    };

    #[test]
    fn facade_contract_is_stable() {
        fn _assert_named<T>() {}
        _assert_named::<AtomicAuthority>();
        _assert_named::<CensusFile>();
        _assert_named::<CensusParseStatus>();
        _assert_named::<CensusVerifyPhase>();
        let _reference_count: fn(&CensusFile) -> u32 = |file| file.reference_count;
        let _parse_status: fn(&CensusFile) -> CensusParseStatus = |file| file.parse_status;
        let _snapshot_verify: fn(&IndexCensus) -> CensusVerifyPhase =
            |census| census.snapshot_verify;
        let _reserve_with_lease: fn(
            &crate::embed::parity::replay::ReplayStore,
            &crate::embed::parity::replay::ReplayKey,
            &crate::embed::parity::replay::RequestFingerprint,
            std::time::Duration,
        ) -> Result<
            crate::embed::parity::replay::ReserveOutcome,
            crate::embed::parity::replay::ReplayError,
        > = crate::embed::parity::replay::ReplayStore::reserve_with_lease;
        let _lease_remaining: fn(
            &crate::embed::parity::replay::ReplayStore,
            &crate::embed::parity::replay::ReplayKey,
        ) -> Result<
            Option<std::time::Duration>,
            crate::embed::parity::replay::ReplayError,
        > = crate::embed::parity::replay::ReplayStore::lease_remaining;
        let _took_over: fn(&crate::embed::parity::replay::OwnerLease) -> bool =
            crate::embed::parity::replay::OwnerLease::took_over;
        let _response_digest: fn(&crate::embed::parity::replay::ReplayOutcome) -> &str =
            crate::embed::parity::replay::ReplayOutcome::response_digest;
        let _from_embedder_digest: fn(
            crate::embed::parity::replay::OutcomeKind,
            usize,
            &str,
            &[u8],
        ) -> Result<
            crate::embed::parity::replay::ReplayOutcome,
            crate::embed::parity::replay::ReplayError,
        > = crate::embed::parity::replay::ReplayOutcome::from_embedder_digest;
        let _mark_started_with_lease: fn(
            &crate::embed::parity::replay::ReplayStore,
            &crate::embed::parity::replay::OwnerLease,
            std::time::Duration,
        ) -> Result<
            crate::embed::parity::replay::ReplayRecord,
            crate::embed::parity::replay::ReplayError,
        > = crate::embed::parity::replay::ReplayStore::mark_started_with_lease;
        let _uncertain_since: fn(
            &crate::embed::parity::replay::ReplayRecord,
        ) -> Option<std::time::SystemTime> =
            crate::embed::parity::replay::ReplayRecord::uncertain_since;
        let _ = (
            _reference_count,
            _parse_status,
            _snapshot_verify,
            _reserve_with_lease,
            _lease_remaining,
            _took_over,
            _response_digest,
            _from_embedder_digest,
            _mark_started_with_lease,
            _uncertain_since,
        );
        _assert_named::<Claim<u64>>();
        _assert_named::<ClaimProvenance>();
        _assert_named::<EmbeddedSourceHandle>();
        _assert_named::<EmbeddedSourceSpec>();
        _assert_named::<EvaluationProvenance>();
        _assert_named::<IndexCensus>();
        _assert_named::<IndexProgress>();
        _assert_named::<KnowledgeMatch>();
        _assert_named::<KnowledgeSearchRequest>();
        _assert_named::<KnowledgeSearchResult>();
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
        let _census: fn(&EmbeddedSourceHandle) -> Result<Claim<IndexCensus>, SourceRefusal> =
            EmbeddedSourceHandle::index_census;
        let _progress: fn(&EmbeddedSourceHandle) -> IndexProgress =
            EmbeddedSourceHandle::index_progress;
        let _knowledge: fn(
            &EmbeddedSourceHandle,
            &KnowledgeSearchRequest,
        ) -> Result<Claim<KnowledgeSearchResult>, SourceRefusal> =
            EmbeddedSourceHandle::search_knowledge;
        let _ = (
            _acquire,
            _open,
            _spec,
            _engine_info,
            _census,
            _progress,
            _knowledge,
        );

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
