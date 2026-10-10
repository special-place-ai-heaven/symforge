//! Source-bound session bookkeeping shared with the MCP session engines.

use parking_lot::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::embed::parity::session::{
    ContextInventory, InvestigationSuggestion, RetrievedOutput, SessionCacheLimits,
    SessionCacheStatus, SessionEvidence, SessionFile, SessionResetReceipt, SessionSummary,
    SessionSymbol,
};
use crate::embed::parity::{
    API_VERSION, QueryOperationKind, QueryOutput, QueryRefusalKind, QueryRequest,
};

use super::embed_query::{Budget, EmbeddedQuerySnapshot, process_instance_identity};
use super::guidance::compression::{CcrPublicationIdentity, CcrRetrieveError, CcrStore};
use super::guidance::session::SessionContext;

/// Opaque context history for one admitted source incarnation. It owns no source
/// handle or write authority. Separate instances have independent histories and
/// caches, including when they refer to the same source.
pub struct QuerySession {
    binding_identity: String,
    runtime_identity: String,
    identity: String,
    limits: SessionCacheLimits,
    revision: AtomicU64,
    reset_epoch: AtomicU64,
    operation_gate: Mutex<()>,
    inner: Mutex<SessionState>,
    /// MCP's per-server STEL L4 session ledger: one economics event per
    /// facade answer, read back by `status`.
    stel_ledger: crate::stel::ledger::SessionLedger,
}

struct SessionState {
    context: SessionContext,
    cache: CcrStore,
}

pub(super) struct PreparedSessionResult {
    body: String,
    tokens: u32,
    pub(super) handle: Option<String>,
}

impl std::fmt::Debug for QuerySession {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("QuerySession")
            .finish_non_exhaustive()
    }
}

impl QuerySession {
    pub(super) fn new(snapshot: &EmbeddedQuerySnapshot) -> Self {
        Self::with_limits(snapshot, SessionCacheLimits::default())
    }

    pub(super) fn with_limits(
        snapshot: &EmbeddedQuerySnapshot,
        limits: SessionCacheLimits,
    ) -> Self {
        static NEXT_SESSION: AtomicU64 = AtomicU64::new(1);
        let runtime_identity = process_instance_identity().to_owned();
        let identity = crate::hash::digest_hex(
            &serde_json::to_vec(&(
                runtime_identity.as_str(),
                snapshot.binding_identity.as_str(),
                NEXT_SESSION.fetch_add(1, Ordering::Relaxed),
            ))
            .expect("session identity fields serialize"),
        );
        Self {
            binding_identity: snapshot.binding_identity.clone(),
            runtime_identity,
            identity,
            limits,
            revision: AtomicU64::new(0),
            reset_epoch: AtomicU64::new(0),
            operation_gate: Mutex::new(()),
            inner: Mutex::new(SessionState {
                context: SessionContext::new(),
                cache: CcrStore::with_limits(
                    limits.max_bytes as usize,
                    limits.max_entries as usize,
                ),
            }),
            stel_ledger: crate::stel::ledger::SessionLedger::new(),
        }
    }

    /// Clear this session's context history and expire its retrieval handles.
    /// Its source binding remains fixed.
    pub fn reset(&self) {
        self.reset_with_receipt();
    }

    pub fn identity(&self) -> &str {
        &self.identity
    }

    pub fn revision(&self) -> u64 {
        self.revision.load(Ordering::Acquire)
    }

    pub fn cache_limits(&self) -> SessionCacheLimits {
        self.limits
    }

    pub fn cache_status(&self) -> SessionCacheStatus {
        let state = self.inner.lock();
        let (bytes, entries) = state.cache.resident_usage();
        SessionCacheStatus {
            limits: self.limits,
            resident_bytes: bytes as u64,
            resident_entries: entries as u32,
        }
    }

    pub fn reset_with_receipt(&self) -> SessionResetReceipt {
        let _operation = self.operation_gate.lock();
        let revision_before = self.revision();
        let mut state = self.inner.lock();
        state.context.reset();
        state.cache = CcrStore::with_limits(
            self.limits.max_bytes as usize,
            self.limits.max_entries as usize,
        );
        self.reset_epoch.fetch_add(1, Ordering::AcqRel);
        self.revision.store(revision_before + 1, Ordering::Release);
        SessionResetReceipt {
            identity: self.identity.clone(),
            revision_before,
            revision_after: revision_before + 1,
        }
    }

    pub(super) fn stel_ledger(&self) -> &crate::stel::ledger::SessionLedger {
        &self.stel_ledger
    }

    /// The MCP session context's served-token total.
    pub(super) fn served_tokens(&self) -> u64 {
        self.inner.lock().context.snapshot().total_tokens
    }

    pub(super) fn begin_operation(&self) -> SessionOperation<'_> {
        let guard = self.operation_gate.lock();
        SessionOperation {
            session: self,
            _guard: guard,
            revision_before: self.revision(),
        }
    }

    pub(super) fn check_binding(
        &self,
        snapshot: &EmbeddedQuerySnapshot,
    ) -> Result<(), QueryRefusalKind> {
        if !self.matches_source_binding(&snapshot.binding_identity) {
            return Err(QueryRefusalKind::ForeignSession);
        }
        Ok(())
    }

    pub(super) fn matches_source_binding(&self, binding: &str) -> bool {
        self.binding_identity == binding && self.runtime_identity == process_instance_identity()
    }

    fn publication(&self, snapshot: &EmbeddedQuerySnapshot) -> CcrPublicationIdentity {
        CcrPublicationIdentity {
            source_digest: crate::hash::digest_hex(
                &serde_json::to_vec(&(
                    self.identity.as_str(),
                    self.reset_epoch.load(Ordering::Acquire),
                ))
                .expect("session epoch fields serialize"),
            ),
            content_generation: snapshot.generation.content_generation,
        }
    }

    fn file_content_hash(
        snapshot: &EmbeddedQuerySnapshot,
        request: &crate::embed::parity::read::FileContentRequest,
        limits: crate::embed::parity::QueryLimits,
    ) -> u64 {
        let mut request = request.clone();
        request.force_refresh = None;
        super::guidance::session::hash_value(&serde_json::json!({
            "input": request,
            "publication": snapshot.serving_publication_identity,
            "limits": limits,
        }))
    }

    pub(super) fn file_content_cache_hit(
        &self,
        snapshot: &EmbeddedQuerySnapshot,
        request: &crate::embed::parity::read::FileContentRequest,
        limits: crate::embed::parity::QueryLimits,
    ) -> Option<(String, String)> {
        let state = self.inner.lock();
        let meta = state.context.try_file_content_cache_hit(
            &request.path,
            Self::file_content_hash(snapshot, request, limits),
            request.force_refresh == Some(true),
        )?;
        state.cache.get(&meta.retrieve_handle)?;
        let body =
            super::guidance::source::format_session_cache_hit_body(&meta, "session_repeat_read");
        Some((meta.retrieve_handle, body))
    }

    pub(super) fn prior_file_content_fetch(
        &self,
        snapshot: &EmbeddedQuerySnapshot,
        request: &crate::embed::parity::read::FileContentRequest,
        limits: crate::embed::parity::QueryLimits,
    ) -> Option<super::guidance::session::SessionFetchRecord> {
        self.inner.lock().context.prior_fetch_for_dedup(
            super::guidance::session::FetchKind::FileContent,
            &request.path,
            "",
            Self::file_content_hash(snapshot, request, limits),
        )
    }

    fn file_context_hash(
        snapshot: &EmbeddedQuerySnapshot,
        request: &crate::embed::parity::read_context::FileContextRequest,
        limits: crate::embed::parity::QueryLimits,
    ) -> u64 {
        let mut request = request.clone();
        request.force_refresh = false;
        super::guidance::session::hash_value(&serde_json::json!({
            "input": request,
            "publication": snapshot.serving_publication_identity,
            "limits": limits,
        }))
    }

    pub(super) fn file_context_cache_hit(
        &self,
        snapshot: &EmbeddedQuerySnapshot,
        request: &crate::embed::parity::read_context::FileContextRequest,
        limits: crate::embed::parity::QueryLimits,
    ) -> Option<(String, String)> {
        let state = self.inner.lock();
        let meta = state.context.try_file_context_cache_hit(
            &request.path,
            Self::file_context_hash(snapshot, request, limits),
            request.force_refresh,
        )?;
        state.cache.get(&meta.retrieve_handle)?;
        let body =
            super::guidance::source::format_session_cache_hit_body(&meta, "session_repeat_read");
        Some((meta.retrieve_handle, body))
    }

    pub(super) fn prior_file_context_fetch(
        &self,
        snapshot: &EmbeddedQuerySnapshot,
        request: &crate::embed::parity::read_context::FileContextRequest,
        limits: crate::embed::parity::QueryLimits,
    ) -> Option<super::guidance::session::SessionFetchRecord> {
        self.inner.lock().context.prior_fetch_for_dedup(
            super::guidance::session::FetchKind::FileContext,
            &request.path,
            "",
            Self::file_context_hash(snapshot, request, limits),
        )
    }

    fn symbol_read_hash(
        snapshot: &EmbeddedQuerySnapshot,
        request: &crate::embed::parity::symbol::SymbolReadRequest,
        limits: crate::embed::parity::QueryLimits,
    ) -> u64 {
        let mut request = request.clone();
        request.force_refresh = None;
        super::guidance::session::hash_value(&serde_json::json!({
            "input": request,
            "publication": snapshot.serving_publication_identity,
            "limits": limits,
        }))
    }

    pub(super) fn symbol_read_cache_hit(
        &self,
        snapshot: &EmbeddedQuerySnapshot,
        request: &crate::embed::parity::symbol::SymbolReadRequest,
        limits: crate::embed::parity::QueryLimits,
    ) -> Option<(String, String)> {
        let state = self.inner.lock();
        let meta = state.context.try_symbol_cache_hit(
            &request.path,
            &request.name,
            Self::symbol_read_hash(snapshot, request, limits),
            request.force_refresh == Some(true),
        )?;
        state.cache.get(&meta.retrieve_handle)?;
        let body =
            super::guidance::source::format_session_cache_hit_body(&meta, "session_repeat_read");
        Some((meta.retrieve_handle, body))
    }

    pub(super) fn prepare_result(
        &self,
        snapshot: &EmbeddedQuerySnapshot,
        request: &QueryRequest,
        output: &QueryOutput,
        cached_output: Option<&QueryOutput>,
        truncated: bool,
    ) -> Option<PreparedSessionResult> {
        if matches!(
            request,
            QueryRequest::ContextInventory | QueryRequest::Retrieve { .. }
        ) {
            return None;
        }
        if matches!(output, QueryOutput::SymbolRead(value) if value.estimated_tokens.is_some())
            || matches!(output, QueryOutput::InspectMatch(value) if value.estimated_tokens.is_some())
            || matches!(output, QueryOutput::ReferenceSearch(value) if value.estimated_tokens.is_some())
            || matches!(output, QueryOutput::DependentSearch(value) if value.estimated_tokens.is_some())
            || matches!(output, QueryOutput::RepoMap(value) if value.estimated_tokens.is_some())
            || matches!(output, QueryOutput::FileContext(value) if value.estimated_tokens.is_some())
            || matches!(output, QueryOutput::SymbolContext(value) if value.estimate.is_some())
        {
            return None;
        }
        #[derive(serde::Serialize)]
        struct ServedQuery<'a> {
            api_version: u32,
            operation: QueryOperationKind,
            serving_publication_identity: &'a str,
            source_capture_publication_identity: &'a str,
            source_version: u64,
            publication_generation: u64,
            content_generation: u64,
            truncated: bool,
            output: &'a QueryOutput,
        }
        let body = serde_json::to_string(&ServedQuery {
            api_version: API_VERSION,
            operation: request.operation(),
            serving_publication_identity: &snapshot.serving_publication_identity,
            source_capture_publication_identity: &snapshot.publication_identity,
            source_version: snapshot.source_version,
            publication_generation: snapshot.generation.publication_generation,
            content_generation: snapshot.generation.content_generation,
            truncated,
            output: cached_output.unwrap_or(output),
        })
        .expect("bounded query output fields serialize");
        let tokens = (serde_json::to_vec(output)
            .expect("query output serializes")
            .len()
            / 4)
        .min(u32::MAX as usize) as u32;
        let state = self.inner.lock();
        let handle = state.cache.preview_insert(
            operation_name(request.operation()),
            &body,
            Some(&self.publication(snapshot)),
        );
        Some(PreparedSessionResult {
            body,
            tokens,
            handle,
        })
    }

    pub(super) fn commit_result(
        &self,
        snapshot: &EmbeddedQuerySnapshot,
        request: &QueryRequest,
        output: &QueryOutput,
        prepared: PreparedSessionResult,
        limits: crate::embed::parity::QueryLimits,
    ) -> Option<String> {
        let PreparedSessionResult {
            body,
            tokens,
            handle: expected_handle,
        } = prepared;
        let mut state = self.inner.lock();
        record_context(&mut state.context, output, tokens);
        let tool = operation_name(request.operation());
        if !matches!(
            output,
            QueryOutput::File { .. }
                | QueryOutput::Symbol(_)
                | QueryOutput::Context(_)
                | QueryOutput::FileContent(_)
                | QueryOutput::FileContext(_)
                | QueryOutput::SourcePage(_)
                | QueryOutput::SymbolRead(_)
                | QueryOutput::SymbolContext(_)
                | QueryOutput::InspectMatch(_)
        ) {
            state.context.record_summary_output(tool, tokens);
        }
        if let QueryOutput::SourcePage(page) = output {
            state.context.record_file(&page.path, tokens);
        }
        let Some(handle) = state
            .cache
            .try_insert(tool, body, Some(self.publication(snapshot)))
        else {
            if let QueryOutput::FileContent(content) = output {
                state.context.record_file(&content.path, tokens);
            }
            if let QueryOutput::FileContext(content) = output {
                state.context.record_file(&content.path, tokens);
            }
            return None;
        };
        if let QueryRequest::FileContent(request) = request {
            state.context.record_file_content_fetch(
                &request.path,
                Self::file_content_hash(snapshot, request, limits),
                tokens,
                &handle,
            );
        }
        if let QueryRequest::FileContext(request) = request {
            state.context.record_file_context_fetch(
                &request.path,
                Self::file_context_hash(snapshot, request, limits),
                tokens,
                &handle,
            );
        }
        if let QueryRequest::SymbolRead(request) = request {
            state.context.record_detailed_fetch(
                super::guidance::session::FetchKind::Symbol,
                &request.path,
                &request.name,
                Self::symbol_read_hash(snapshot, request, limits),
                tokens,
                &handle,
            );
        }
        debug_assert_eq!(expected_handle.as_deref(), Some(handle.as_str()));
        Some(handle)
    }

    pub(super) fn inventory(
        &self,
        budget: &mut Budget,
    ) -> Result<ContextInventory, QueryRefusalKind> {
        budget.required(0)?;
        let state = self.inner.lock();
        let snapshot = state.context.snapshot();
        let cache = state.cache.economics();
        let mut symbols = |rows: Vec<(String, String, u32)>| {
            let mut result = Vec::new();
            for (path, name, approximate_tokens) in rows {
                if !budget.row(path.len() + name.len()) {
                    break;
                }
                result.push(SessionSymbol {
                    path,
                    name,
                    approximate_tokens,
                });
            }
            result
        };
        let fetched_symbols = symbols(snapshot.fetched_symbols);
        let listed_symbols = symbols(snapshot.listed_symbols);
        let mut files = |rows: Vec<(String, u32)>| {
            let mut result = Vec::new();
            for (path, approximate_tokens) in rows {
                if !budget.row(path.len()) {
                    break;
                }
                result.push(SessionFile {
                    path,
                    approximate_tokens,
                });
            }
            result
        };
        let fetched_files = files(snapshot.fetched_files);
        let listed_files = files(snapshot.listed_files);
        let mut summary_outputs = Vec::new();
        for (operation, approximate_tokens) in snapshot.summary_outputs {
            if !budget.row(operation.len()) {
                break;
            }
            summary_outputs.push(SessionSummary {
                operation,
                approximate_tokens,
            });
        }
        budget.check()?;
        Ok(ContextInventory {
            fetched_symbols,
            listed_symbols,
            fetched_files,
            listed_files,
            summary_outputs,
            total_tokens: snapshot.total_tokens,
            duration_secs: snapshot.duration_secs,
            cache_hits: snapshot.cache_hit_count,
            cached_outputs: cache.offloads,
            bytes_stored: cache.bytes_stored,
            retrieves: cache.retrieves,
            bytes_retrieved: cache.bytes_retrieved,
        })
    }

    pub(super) fn investigate(
        &self,
        snapshot: &EmbeddedQuerySnapshot,
        focus: Option<&str>,
        budget: &mut Budget,
    ) -> Result<InvestigationSuggestion, QueryRefusalKind> {
        budget.check()?;
        let state = self.inner.lock();
        let text = super::guidance::investigation::suggest_next_steps(
            &snapshot.generation.live,
            &state.context,
            focus,
        );
        budget.check()?;
        Ok(InvestigationSuggestion {
            text: budget.text(&text)?,
        })
    }

    pub(super) fn retrieve(
        &self,
        snapshot: &EmbeddedQuerySnapshot,
        handle: &str,
        offset: u64,
        budget: &mut Budget,
    ) -> Result<RetrievedOutput, QueryRefusalKind> {
        let handle = handle.trim().to_ascii_lowercase();
        if handle.len() != 12 || !handle.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(QueryRefusalKind::InvalidRequest);
        }
        budget.required(0)?;
        let current = self.publication(snapshot);
        let state = self.inner.lock();
        let retrieved = state
            .cache
            .get_checked(
                &handle,
                crate::knowledge::SECRET_POLICY_VERSION,
                Some(&current),
            )
            .map_err(|error| match error {
                CcrRetrieveError::ForeignPublication => QueryRefusalKind::ForeignSession,
                CcrRetrieveError::PublicationUnverifiable => QueryRefusalKind::SourceUnavailable,
                CcrRetrieveError::SecretPolicyMismatch => QueryRefusalKind::PolicyMismatch,
            })?
            .ok_or(QueryRefusalKind::StaleHandle)?;
        let publication = retrieved
            .publication
            .as_ref()
            .ok_or(QueryRefusalKind::SourceUnavailable)?;
        let offset_index = usize::try_from(offset).map_err(|_| QueryRefusalKind::InvalidRequest)?;
        let remaining = retrieved
            .formatted_bytes
            .as_bytes()
            .get(offset_index..)
            .ok_or(QueryRefusalKind::InvalidRequest)?;
        let bytes = budget.source(remaining);
        let next = offset + bytes.len() as u64;
        Ok(RetrievedOutput {
            bytes,
            byte_start: offset,
            next_offset: (next < retrieved.formatted_bytes.len() as u64).then_some(next),
            total_bytes: retrieved.formatted_bytes.len() as u64,
            content_generation: publication.content_generation,
            current_content_generation: current.content_generation,
            superseded: *publication != current,
        })
    }

    pub(super) fn commit_observation(&self, output: &QueryOutput) {
        let mut state = self.inner.lock();
        match output {
            QueryOutput::FileContent(content) if content.cache_hit => {
                state.context.record_cache_hit()
            }
            QueryOutput::SymbolRead(content) if content.cache_hit => {
                state.context.record_cache_hit()
            }
            QueryOutput::FileContext(content) if content.cache_hit => {
                state.context.record_cache_hit()
            }
            QueryOutput::RetrievedOutput(output) => {
                state.cache.record_page_retrieval(output.bytes.len())
            }
            _ => {}
        }
    }
}

pub(super) struct SessionOperation<'a> {
    session: &'a QuerySession,
    _guard: parking_lot::MutexGuard<'a, ()>,
    revision_before: u64,
}
impl SessionOperation<'_> {
    pub(super) fn evidence(&self) -> SessionEvidence {
        let revision_after = self.revision_before + 1;
        SessionEvidence {
            identity: self.session.identity.clone(),
            revision_before: self.revision_before,
            revision_after,
        }
    }

    pub(super) fn commit(self) -> SessionEvidence {
        let evidence = self.evidence();
        self.session
            .revision
            .store(evidence.revision_after, Ordering::Release);
        evidence
    }
}

fn record_context(context: &mut SessionContext, output: &QueryOutput, tokens: u32) {
    match output {
        QueryOutput::File { file, .. } => context.record_file(&file.path, tokens),
        QueryOutput::Symbol(symbol) => context.record_symbol(&symbol.path, &symbol.name, tokens),
        QueryOutput::Context(query_context) => context.record_symbol(
            &query_context.symbol.path,
            &query_context.symbol.name,
            tokens,
        ),
        QueryOutput::SymbolContext(content) if content.estimate.is_none() => {
            if let Some(symbol) = &content.symbol {
                context.record_symbol(&symbol.path, &symbol.name, tokens);
            }
            for candidate in &content.candidates {
                context.record_listed_symbol(&candidate.path, &candidate.name);
            }
        }
        QueryOutput::SymbolRead(content) if content.estimated_tokens.is_none() => {
            for entry in &content.entries {
                if entry.source.is_some() {
                    if let Some(symbol) = &entry.symbol {
                        context.record_symbol(&symbol.path, &symbol.name, tokens);
                    } else {
                        context.record_file(&entry.path, tokens);
                    }
                }
                for candidate in &entry.candidates {
                    context.record_listed_symbol(&candidate.path, &candidate.name);
                }
            }
        }
        QueryOutput::InspectMatch(content) if content.estimated_tokens.is_none() => {
            context.record_file(&content.path, tokens);
        }
        QueryOutput::Symbols(symbols) => {
            for symbol in symbols {
                context.record_listed_symbol(&symbol.path, &symbol.name);
            }
        }
        QueryOutput::ReferenceSearch(references) => {
            for candidate in &references.target_candidates {
                context.record_listed_symbol(&candidate.path, &candidate.name);
            }
            for file in &references.files {
                context.record_listed_file(&file.path, 0);
            }
        }
        QueryOutput::DependentSearch(dependents) => {
            for file in &dependents.files {
                context.record_listed_file(&file.path, 0);
            }
        }
        QueryOutput::SymbolSearch(result) => {
            for hit in &result.symbols {
                context.record_listed_symbol(&hit.symbol.path, &hit.symbol.name);
            }
            for hit in &result.text_fallback {
                context.record_listed_file(&hit.path, 0);
            }
        }
        QueryOutput::TextSearch(result) => {
            use crate::embed::parity::search::TextRows;
            match &result.rows {
                TextRows::Files(files) => {
                    for file in files {
                        context.record_listed_file(&file.path, 0);
                        for hit in &file.matches {
                            if let Some(symbol) = &hit.enclosing_symbol {
                                context.record_listed_symbol(&symbol.path, &symbol.name);
                            }
                        }
                    }
                }
                TextRows::Symbols { groups, top_level } => {
                    for group in groups {
                        context.record_listed_symbol(&group.symbol.path, &group.symbol.name);
                    }
                    for hit in top_level {
                        context.record_listed_file(&hit.path, 0);
                    }
                }
                TextRows::Names(_) => {}
            }
        }
        QueryOutput::Files(files) => {
            for file in files {
                context.record_listed_file(&file.path, 0);
            }
        }
        QueryOutput::FileSearch(result) => {
            for hit in &result.hits {
                context.record_listed_file(&hit.path, 0);
            }
            match &result.resolution {
                Some(crate::embed::parity::search::FileResolution::Resolved { path, .. }) => {
                    context.record_listed_file(path, 0);
                }
                Some(crate::embed::parity::search::FileResolution::Ambiguous {
                    candidates,
                    ..
                }) => {
                    for path in candidates {
                        context.record_listed_file(path, 0);
                    }
                }
                _ => {}
            }
        }
        QueryOutput::Text(matches) => {
            for hit in matches {
                if let Some(symbol) = &hit.enclosing_symbol {
                    context.record_listed_symbol(&symbol.path, &symbol.name);
                } else {
                    context.record_listed_file(&hit.path, 0);
                }
            }
        }
        QueryOutput::Exploration(exploration) => {
            for symbol in &exploration.symbols {
                context.record_listed_symbol(&symbol.path, &symbol.name);
            }
            for file in &exploration.related_files {
                context.record_listed_file(&file.path, 0);
            }
        }
        // An Ask records exactly what its routed query returned, under the
        // same rules that query would have recorded on its own.
        QueryOutput::Ask(ask) => {
            if let Some(output) = ask.output.as_deref() {
                record_context(context, output, tokens);
            }
        }
        _ => {}
    }
}

fn operation_name(operation: QueryOperationKind) -> &'static str {
    match operation {
        QueryOperationKind::File => "get_file_content",
        QueryOperationKind::FileContext => "get_file_context",
        QueryOperationKind::RepoMap => "get_repo_map",
        QueryOperationKind::Symbol => "get_symbol",
        QueryOperationKind::InspectMatch => "inspect_match",
        QueryOperationKind::Context => "get_symbol_context",
        QueryOperationKind::References => "find_references",
        QueryOperationKind::Dependents => "find_dependents",
        QueryOperationKind::SearchSymbols => "search_symbols",
        QueryOperationKind::SearchText => "search_text",
        QueryOperationKind::SearchFiles => "search_files",
        QueryOperationKind::SearchKnowledge => "search_knowledge",
        QueryOperationKind::Graph => "graph",
        QueryOperationKind::Syntax => "validate_file_syntax",
        QueryOperationKind::Diff => "diff_symbols",
        QueryOperationKind::Impact => "analyze_file_impact",
        QueryOperationKind::Explore => "explore",
        QueryOperationKind::Conventions => "conventions",
        QueryOperationKind::ContextInventory => "context_inventory",
        QueryOperationKind::InvestigationSuggest => "investigation_suggest",
        QueryOperationKind::EditPlan => "edit_plan",
        QueryOperationKind::Retrieve => "symforge_retrieve",
        QueryOperationKind::WhatChanged => "what_changed",
        QueryOperationKind::DiffSymbols => "diff_symbols",
        QueryOperationKind::DetectImpact => "detect_impact",
        QueryOperationKind::Ask => "ask",
    }
}
