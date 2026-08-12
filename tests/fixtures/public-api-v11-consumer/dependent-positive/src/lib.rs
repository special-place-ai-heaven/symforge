//! External dependent-crate input for the frozen V11 public API allowlist.
//!
//! This is intentionally pre-activation source. It must not be reported as
//! compiling until T049 introduces the production-unreachable dark adapter.

#![allow(dead_code, drop_bounds)]

#[cfg(feature = "embed")]
mod embed_consumer {
    use std::panic::{RefUnwindSafe, UnwindSafe};
    use std::path::PathBuf;
    use std::time::Instant;

    use symforge::embed::{
        engine_info, AtomicAuthority, Claim, ClaimProvenance, EmbeddedSourceHandle,
        EmbeddedSourceSpec, EngineInfo, EvaluationProvenance, OperationKind,
        OperationReceipt, ProcessIndexRuntime, ReceiptWaitError, RefreshTicket, RetryAdvice,
        ShutdownReceipt, ShutdownReport, SourceCloseReceipt, SourceCloseReport, SourceRefusal,
        SourceRefusalKind, SourceRuntimePhase, SourceRuntimeView, SymbolMatch,
        SymbolSearchRequest, SymbolSearchResult, TextMatch, TextSearchRequest, TextSearchResult,
    };

    pub const ENGINE_INFO: fn() -> EngineInfo = engine_info;

    pub fn name_every_export() {
        let _ = std::any::type_name::<AtomicAuthority>();
        let _ = std::any::type_name::<Claim<()>>();
        let _ = std::any::type_name::<ClaimProvenance>();
        let _ = std::any::type_name::<EmbeddedSourceHandle>();
        let _ = std::any::type_name::<EmbeddedSourceSpec>();
        let _ = std::any::type_name::<EngineInfo>();
        let _ = std::any::type_name::<EvaluationProvenance>();
        let _ = std::any::type_name::<OperationKind>();
        let _ = std::any::type_name::<OperationReceipt>();
        let _ = std::any::type_name::<ProcessIndexRuntime>();
        let _ = std::any::type_name::<ReceiptWaitError>();
        let _ = std::any::type_name::<RefreshTicket>();
        let _ = std::any::type_name::<RetryAdvice>();
        let _ = std::any::type_name::<ShutdownReceipt>();
        let _ = std::any::type_name::<ShutdownReport>();
        let _ = std::any::type_name::<SourceCloseReceipt>();
        let _ = std::any::type_name::<SourceCloseReport>();
        let _ = std::any::type_name::<SourceRefusal>();
        let _ = std::any::type_name::<SourceRefusalKind>();
        let _ = std::any::type_name::<SourceRuntimePhase>();
        let _ = std::any::type_name::<SourceRuntimeView>();
        let _ = std::any::type_name::<SymbolMatch>();
        let _ = std::any::type_name::<SymbolSearchRequest>();
        let _ = std::any::type_name::<SymbolSearchResult>();
        let _ = std::any::type_name::<TextMatch>();
        let _ = std::any::type_name::<TextSearchRequest>();
        let _ = std::any::type_name::<TextSearchResult>();
    }

    pub fn inspect_engine_info(value: &EngineInfo) -> (&'static str, &'static [&'static str], u32, u32) {
        (
            value.version,
            value.grammars,
            value.snapshot_format_version,
            value.secret_policy_version,
        )
    }

    pub fn inspect_authority(value: &AtomicAuthority) -> (&str, &'static str) {
        (value.identity(), value.kind_name())
    }

    pub fn inspect_claim<T>(
        value: &Claim<T>,
    ) -> (
        &T,
        &ClaimProvenance,
        &OperationReceipt,
        Option<&EvaluationProvenance>,
        &str,
    ) {
        (
            value.value(),
            value.provenance(),
            value.operation(),
            value.evaluation(),
            value.producing_runtime_identity(),
        )
    }

    pub fn inspect_claim_provenance(
        value: &ClaimProvenance,
    ) -> (&[AtomicAuthority], usize, &str, &'static str) {
        (
            value.authorities(),
            value.authority_count(),
            value.identity(),
            value.kind_name(),
        )
    }

    pub fn make_source_spec(root: PathBuf) -> EmbeddedSourceSpec {
        EmbeddedSourceSpec::current_worktree(root)
    }

    pub fn inspect_evaluation(value: &EvaluationProvenance) -> &str {
        value.identity()
    }

    pub fn inspect_operation(
        value: &OperationReceipt,
    ) -> (&str, &str, OperationKind, u32) {
        (
            value.canonical_argument_hash(),
            value.identity(),
            value.operation_kind(),
            value.schema_version(),
        )
    }

    pub fn use_handle(
        handle: &EmbeddedSourceHandle,
        symbols: &SymbolSearchRequest,
        text: &TextSearchRequest,
    ) -> (
        SourceCloseReceipt,
        Result<RefreshTicket, SourceRefusal>,
        SourceRuntimeView,
        Result<Claim<SymbolSearchResult>, SourceRefusal>,
        Result<Claim<TextSearchResult>, SourceRefusal>,
    ) {
        (
            handle.begin_close(),
            handle.request_refresh(),
            handle.runtime_view(),
            handle.search_symbols(symbols),
            handle.search_text(text),
        )
    }

    pub fn use_runtime(
        runtime: &ProcessIndexRuntime,
        spec: EmbeddedSourceSpec,
    ) -> (
        ShutdownReceipt,
        Result<EmbeddedSourceHandle, SourceRefusal>,
        Result<ProcessIndexRuntime, SourceRefusal>,
    ) {
        (
            runtime.begin_shutdown(),
            runtime.open_embedded_source(spec),
            ProcessIndexRuntime::acquire(),
        )
    }

    pub fn inspect_refresh(value: &RefreshTicket) -> (&OperationReceipt, u64, &str) {
        (
            value.operation(),
            value.requested_source_version(),
            value.ticket_identity(),
        )
    }

    pub fn wait_for_shutdown(
        value: &ShutdownReceipt,
        deadline: Instant,
    ) -> Result<ShutdownReport, ReceiptWaitError> {
        value.wait(deadline)
    }

    pub fn wait_for_source_close(
        value: &SourceCloseReceipt,
        deadline: Instant,
    ) -> Result<SourceCloseReport, ReceiptWaitError> {
        value.wait(deadline)
    }

    pub fn inspect_refusal(
        value: &SourceRefusal,
    ) -> (&str, SourceRefusalKind, &OperationReceipt, RetryAdvice) {
        (
            value.evidence_identity(),
            value.kind(),
            value.operation(),
            value.retry(),
        )
    }

    pub fn name_every_enum_variant() {
        let _: [OperationKind; 7] = [
            OperationKind::AcquireRuntime,
            OperationKind::CloseSource,
            OperationKind::OpenEmbeddedSource,
            OperationKind::RefreshSource,
            OperationKind::SearchSymbols,
            OperationKind::SearchText,
            OperationKind::ShutdownRuntime,
        ];
        let _: [ReceiptWaitError; 2] = [
            ReceiptWaitError::DeadlineElapsed,
            ReceiptWaitError::WouldSelfWait,
        ];
        let _: [RetryAdvice; 4] = [
            RetryAdvice::Automatic,
            RetryAdvice::Never,
            RetryAdvice::OnEvent,
            RetryAdvice::Operator,
        ];
        let _: [SourceRefusalKind; 4] = [
            SourceRefusalKind::AdmissionUnavailable,
            SourceRefusalKind::InvalidSelection,
            SourceRefusalKind::SelectionUnavailable,
            SourceRefusalKind::SourceUnavailable,
        ];
        let _: [SourceRuntimePhase; 6] = [
            SourceRuntimePhase::Blocked,
            SourceRuntimePhase::Current,
            SourceRuntimePhase::Loading,
            SourceRuntimePhase::Refreshing,
            SourceRuntimePhase::Stopped,
            SourceRuntimePhase::Stopping,
        ];
    }

    pub fn construct_public_field_types() -> (
        ShutdownReport,
        SourceCloseReport,
        SourceRuntimeView,
        SymbolSearchRequest,
        SymbolSearchResult,
        TextSearchRequest,
        TextSearchResult,
    ) {
        let symbol_match = SymbolMatch {
            end_line: 2,
            kind: String::new(),
            name: String::new(),
            path: String::new(),
            start_line: 1,
        };
        let text_match = TextMatch {
            byte_end: 1,
            byte_start: 0,
            line: 1,
            path: String::new(),
            preview: String::new(),
        };
        (
            ShutdownReport {
                closed_sources: 0,
                joined_workers: 0,
            },
            SourceCloseReport {
                already_terminal: false,
                terminal_source_version: 0,
            },
            SourceRuntimeView {
                binding_identity: String::new(),
                current_publication_identity: None,
                observer_epoch: 0,
                phase: SourceRuntimePhase::Loading,
                source_version: 0,
            },
            SymbolSearchRequest {
                limit: 1,
                path_prefix: None,
                query: None,
            },
            SymbolSearchResult {
                matches: vec![symbol_match],
                truncated: false,
            },
            TextSearchRequest {
                case_sensitive: true,
                limit: 1,
                path_prefix: None,
                query: String::new(),
            },
            TextSearchResult {
                matches: vec![text_match],
                truncated: false,
            },
        )
    }

    fn require_clone<T: Clone>() {}
    fn require_copy<T: Copy>() {}
    fn require_debug<T: core::fmt::Debug>() {}
    fn require_display<T: core::fmt::Display>() {}
    fn require_drop<T: Drop>() {}
    fn require_eq<T: Eq>() {}
    fn require_error<T: std::error::Error>() {}
    fn require_partial_eq<T: PartialEq>() {}
    fn require_auto_traits<T: Send + Sync + Unpin + RefUnwindSafe + UnwindSafe>() {}

    pub fn assert_direct_trait_edges() {
        require_drop::<EmbeddedSourceHandle>();
        require_clone::<EngineInfo>();
        require_copy::<EngineInfo>();
        require_debug::<EngineInfo>();
        require_eq::<EngineInfo>();
        require_partial_eq::<EngineInfo>();
        require_clone::<ProcessIndexRuntime>();
        require_drop::<ProcessIndexRuntime>();
        require_debug::<ReceiptWaitError>();
        require_display::<ReceiptWaitError>();
        require_error::<ReceiptWaitError>();
        require_debug::<SourceRefusal>();
        require_display::<SourceRefusal>();
        require_error::<SourceRefusal>();
    }

    pub fn assert_positive_auto_traits() {
        require_auto_traits::<AtomicAuthority>();
        require_auto_traits::<Claim<()>>();
        require_auto_traits::<ClaimProvenance>();
        require_auto_traits::<EmbeddedSourceSpec>();
        require_auto_traits::<EngineInfo>();
        require_auto_traits::<EvaluationProvenance>();
        require_auto_traits::<OperationKind>();
        require_auto_traits::<OperationReceipt>();
        require_auto_traits::<ReceiptWaitError>();
        require_auto_traits::<RetryAdvice>();
        require_auto_traits::<ShutdownReport>();
        require_auto_traits::<SourceCloseReport>();
        require_auto_traits::<SourceRefusal>();
        require_auto_traits::<SourceRefusalKind>();
        require_auto_traits::<SourceRuntimePhase>();
        require_auto_traits::<SourceRuntimeView>();
        require_auto_traits::<SymbolMatch>();
        require_auto_traits::<SymbolSearchRequest>();
        require_auto_traits::<SymbolSearchResult>();
        require_auto_traits::<TextMatch>();
        require_auto_traits::<TextSearchRequest>();
        require_auto_traits::<TextSearchResult>();
    }

    fn require_send_sync_unpin<T: Send + Sync + Unpin>() {}

    pub fn assert_handle_auto_traits() {
        require_send_sync_unpin::<EmbeddedSourceHandle>();
        require_send_sync_unpin::<ProcessIndexRuntime>();
        require_send_sync_unpin::<RefreshTicket>();
        require_send_sync_unpin::<ShutdownReceipt>();
        require_send_sync_unpin::<SourceCloseReceipt>();
    }
}

#[cfg(feature = "server")]
mod server_consumer {
    use std::ffi::OsString;
    use std::panic::{RefUnwindSafe, UnwindSafe};

    use symforge::server_api::{run, ServerBootstrapError, ServerExit};

    pub const RUN: fn(Vec<OsString>) -> Result<ServerExit, ServerBootstrapError> = run;

    fn require_debug<T: core::fmt::Debug>() {}
    fn require_display<T: core::fmt::Display>() {}
    fn require_error<T: std::error::Error>() {}
    fn require_auto_traits<T: Send + Sync + Unpin + RefUnwindSafe + UnwindSafe>() {}

    pub fn name_server_api() {
        let _ = std::any::type_name::<ServerBootstrapError>();
        let _: [ServerExit; 2] = [ServerExit::RefusedToStart, ServerExit::Success];
        require_debug::<ServerBootstrapError>();
        require_display::<ServerBootstrapError>();
        require_error::<ServerBootstrapError>();
        require_auto_traits::<ServerBootstrapError>();
        require_auto_traits::<ServerExit>();
    }
}

