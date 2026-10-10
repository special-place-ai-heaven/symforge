//! Read-only projections over an admitted immutable publication. No index or
//! parser implementation type crosses the supported embedded boundary.

use std::collections::{BTreeMap, BTreeSet};
use std::hash::BuildHasher;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use crate::domain::{IndexTargets, ReferenceKind, ReferenceRecord, SymbolKind, SymbolRecord};
use crate::embed::parity::host::{OperationControl, OperationStop};
use crate::embed::parity::*;
use crate::live_index::disambiguation::{SymbolSelectorMatch, resolve_symbol_selector};
use crate::live_index::search::{
    self, PathScope, ResultLimit, SymbolSearchOptions, TextSearchOptions,
};
use crate::live_index::{IndexedFile, LiveIndex, ParseStatus, PublishedGeneration};

use super::embedded::EmbeddedSourceHandle;
use super::public_api::{
    EmbedClaim, EmbedClaimProvenance, OperationKind, RetryAdvice, SourceRefusalKind,
};

pub(super) struct EmbeddedQuerySnapshot {
    pub root: PathBuf,
    pub state_dir: Option<PathBuf>,
    pub project_state: Option<crate::domain::ProjectStateDir>,
    pub source_set: Arc<crate::live_index::PublishedSourceSet>,
    pub generation: Arc<PublishedGeneration>,
    pub binding_identity: String,
    pub publication_identity: String,
    pub serving_publication_identity: String,
    pub source_version: u64,
    pub authority_publication: crate::lifecycle_identity::PublicationIdentity,
    pub authority: Arc<super::activation::ProjectSourceAuthority>,
    pub state_anchor: Option<super::embedded::AdmittedStateAnchor>,
}

/// A process incarnation namespace, separate from the frozen core's counters.
/// RandomState supplies independently seeded standard-library randomness; PID
/// and time provide additional separation. This identifier is not write authority.
pub(super) fn process_instance_identity() -> &'static str {
    static IDENTITY: OnceLock<String> = OnceLock::new();
    IDENTITY.get_or_init(|| {
        let seed = (std::process::id(), std::time::SystemTime::now());
        let first = std::collections::hash_map::RandomState::new().hash_one(seed);
        let second = std::collections::hash_map::RandomState::new().hash_one(seed);
        format!("embed-instance-{first:016x}{second:016x}")
    })
}

pub(super) fn serving_publication_identity(
    binding: &str,
    publication: &str,
    source_version: u64,
) -> String {
    let canonical = serde_json::to_vec(&(
        "symforge.embed.serving-publication",
        API_VERSION,
        process_instance_identity(),
        binding,
        publication,
        source_version,
    ))
    .expect("publication identity fields serialize");
    format!("embed-serving-v1-{}", crate::hash::digest_hex(&canonical))
}

impl EmbeddedQuerySnapshot {
    pub(super) fn record_commitment(&self, paths: &[PathBuf]) {
        crate::live_index::frecency::bump(&self.root, self.project_state.as_ref(), paths);
    }
}

/// Authoritative operation receipt for the additive query contract. Source
/// authority capture is separate evidence and never substitutes for this kind.
#[derive(Debug, serde::Serialize)]
pub struct QueryOperationReceipt {
    #[serde(rename = "operation_kind")]
    kind: QueryOperationKind,
    identity: String,
    canonical_argument_hash: String,
    schema_version: u32,
}

impl QueryOperationReceipt {
    pub fn operation_kind(&self) -> QueryOperationKind {
        self.kind
    }
    pub fn identity(&self) -> &str {
        &self.identity
    }
    pub fn canonical_argument_hash(&self) -> &str {
        &self.canonical_argument_hash
    }
    pub fn schema_version(&self) -> u32 {
        self.schema_version
    }
}

/// A successful bounded projection, including exactly which publication was
/// examined and whether the returned rows/source were truncated.
#[derive(Debug)]
pub struct QueryClaim {
    claim: EmbedClaim<QueryOutput>,
    operation: QueryOperationReceipt,
    publication_identity: String,
    source_capture_publication_identity: String,
    source_version: u64,
    publication_generation: u64,
    content_generation: u64,
    truncated: bool,
    partial_files: u64,
    withheld_files: u64,
    observations: Vec<QueryObservation>,
    retrieve_handle: Option<String>,
    usage: QueryUsage,
    session_evidence: Option<session::SessionEvidence>,
    cache_disposition: session::CacheDisposition,
}

impl QueryClaim {
    pub fn usage(&self) -> QueryUsage {
        self.usage
    }
    pub fn session_evidence(&self) -> Option<&session::SessionEvidence> {
        self.session_evidence.as_ref()
    }
    pub fn cache_disposition(&self) -> session::CacheDisposition {
        self.cache_disposition
    }
    pub fn value(&self) -> &QueryOutput {
        self.claim.value()
    }
    pub fn provenance(&self) -> &EmbedClaimProvenance {
        self.claim.provenance()
    }
    pub fn operation(&self) -> QueryOperationKind {
        self.operation.operation_kind()
    }
    pub fn operation_identity(&self) -> &str {
        self.operation.identity()
    }
    pub fn operation_receipt(&self) -> &QueryOperationReceipt {
        &self.operation
    }
    pub fn publication_identity(&self) -> &str {
        &self.publication_identity
    }
    /// Raw frozen source-capture counter identity. It is not a resumable cursor.
    pub fn source_capture_publication_identity(&self) -> &str {
        &self.source_capture_publication_identity
    }
    pub fn source_version(&self) -> u64 {
        self.source_version
    }
    pub fn publication_generation(&self) -> u64 {
        self.publication_generation
    }
    pub fn content_generation(&self) -> u64 {
        self.content_generation
    }
    pub fn truncated(&self) -> bool {
        self.truncated
    }
    pub fn partial_files(&self) -> u64 {
        self.partial_files
    }
    pub fn withheld_files(&self) -> u64 {
        self.withheld_files
    }
    pub fn observations(&self) -> &[QueryObservation] {
        &self.observations
    }
    /// Session-local handle for the exact served projection and its original
    /// publication metadata. Resetting the session expires this handle.
    pub fn retrieve_handle(&self) -> Option<&str> {
        self.retrieve_handle.as_deref()
    }
    pub fn canonical_argument_hash(&self) -> &str {
        self.operation.canonical_argument_hash()
    }
    pub fn evaluation_identity(&self) -> Option<&str> {
        self.claim
            .evaluation()
            .map(|evaluation| evaluation.identity())
    }
    pub fn producing_runtime_identity(&self) -> &str {
        process_instance_identity()
    }
    pub fn source_capture_runtime_identity(&self) -> &str {
        self.claim.producing_runtime_identity()
    }
    pub fn source_capture_canonical_argument_hash(&self) -> &str {
        self.claim.operation().canonical_argument_hash()
    }
    /// Kind of the frozen core read-authority receipt. `operation()` identifies
    /// the actual additive query operation.
    pub fn source_capture_receipt_kind(&self) -> OperationKind {
        self.claim.operation().operation_kind()
    }
    pub fn source_capture_schema_version(&self) -> u32 {
        self.claim.operation().schema_version()
    }
}

/// Refusals contain typed reasons and a request digest, never query text,
/// source bytes or diagnostic excerpts that could disclose withheld content.
#[derive(Debug)]
pub struct QueryRefusal {
    kind: QueryRefusalKind,
    operation: QueryOperationKind,
    operation_identity: String,
    retry: RetryAdvice,
    withheld: Option<crate::embed::parity::read::WithheldMeta>,
}

impl QueryRefusal {
    pub fn withheld(&self) -> Option<&crate::embed::parity::read::WithheldMeta> {
        self.withheld.as_ref()
    }
    pub fn kind(&self) -> QueryRefusalKind {
        self.kind
    }
    pub fn operation(&self) -> QueryOperationKind {
        self.operation
    }
    pub fn operation_identity(&self) -> &str {
        &self.operation_identity
    }
    pub fn retry(&self) -> RetryAdvice {
        self.retry
    }
}

impl std::fmt::Display for QueryRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "embedded query {:?} refused: {:?} ({})",
            self.operation, self.kind, self.operation_identity
        )
    }
}

impl std::error::Error for QueryRefusal {}

pub(super) fn execute(
    handle: &EmbeddedSourceHandle,
    request: &QueryRequest,
    limits: QueryLimits,
) -> Result<QueryClaim, QueryRefusal> {
    execute_inner(
        handle,
        request,
        limits,
        None,
        None,
        QueryPolicy::default(),
        |_| true,
    )
}

pub(super) fn execute_with_control(
    handle: &EmbeddedSourceHandle,
    request: &QueryRequest,
    limits: QueryLimits,
    control: &OperationControl,
) -> Result<QueryClaim, QueryRefusal> {
    execute_inner(
        handle,
        request,
        limits,
        Some(control.clone()),
        None,
        QueryPolicy::default(),
        |_| true,
    )
}

pub(super) fn execute_with_session(
    handle: &EmbeddedSourceHandle,
    request: &QueryRequest,
    limits: QueryLimits,
    session: &super::embed_session::QuerySession,
    control: Option<&OperationControl>,
) -> Result<QueryClaim, QueryRefusal> {
    execute_inner(
        handle,
        request,
        limits,
        control.cloned(),
        Some(session),
        QueryPolicy::default(),
        |_| true,
    )
}

pub(super) fn execute_with_policy(
    handle: &EmbeddedSourceHandle,
    request: &QueryRequest,
    limits: QueryLimits,
    policy: QueryPolicy,
    session: Option<&super::embed_session::QuerySession>,
    control: Option<&OperationControl>,
) -> Result<QueryClaim, QueryRefusal> {
    execute_inner(
        handle,
        request,
        limits,
        control.cloned(),
        session,
        policy,
        |_| true,
    )
}

pub(super) fn execute_with_policy_admitted(
    handle: &EmbeddedSourceHandle,
    request: &QueryRequest,
    limits: QueryLimits,
    policy: QueryPolicy,
    session: Option<&super::embed_session::QuerySession>,
    control: Option<&OperationControl>,
    admit: impl FnOnce(&QueryClaim) -> bool,
) -> Result<QueryClaim, QueryRefusal> {
    execute_inner(
        handle,
        request,
        limits,
        control.cloned(),
        session,
        policy,
        admit,
    )
}

fn execute_inner(
    handle: &EmbeddedSourceHandle,
    request: &QueryRequest,
    limits: QueryLimits,
    control: Option<OperationControl>,
    session: Option<&super::embed_session::QuerySession>,
    policy: QueryPolicy,
    admit: impl FnOnce(&QueryClaim) -> bool,
) -> Result<QueryClaim, QueryRefusal> {
    let normalized =
        serde_json::to_vec(&("symforge.embed.query", API_VERSION, request, limits, policy))
            .expect("query request fields serialize");
    let identity = crate::hash::digest_hex(&normalized);
    let refuse = |kind, retry| QueryRefusal {
        kind,
        operation: request.operation(),
        operation_identity: identity.clone(),
        retry,
        withheld: None,
    };
    let mut budget =
        Budget::new(limits, control).map_err(|kind| refuse(kind, RetryAdvice::Never))?;
    validate_request(request).map_err(|kind| refuse(kind, RetryAdvice::Never))?;
    budget
        .check()
        .map_err(|kind| refuse(kind, RetryAdvice::Never))?;
    let snapshot = handle
        .capture_query_snapshot(&normalized)
        .map_err(|error| {
            refuse(
                if error.kind() == SourceRefusalKind::AdmissionUnavailable {
                    QueryRefusalKind::AdmissionUnavailable
                } else {
                    QueryRefusalKind::SourceUnavailable
                },
                error.retry(),
            )
        })?;
    debug_assert!(Arc::ptr_eq(
        &snapshot.generation,
        &snapshot.source_set.current_generation()
    ));
    if let Some(session) = session {
        session
            .check_binding(&snapshot)
            .map_err(|kind| refuse(kind, RetryAdvice::Never))?;
    }
    let session_operation = session.map(|session| session.begin_operation());
    let mut observations = Vec::new();
    budget
        .check()
        .map_err(|kind| refuse(kind, RetryAdvice::Never))?;
    let projected = project(
        &snapshot,
        request,
        &mut budget,
        &mut observations,
        session,
        policy,
    );
    // A cooperative stop must never be reported as a successful partial page or
    // as a budget error raised at the same checkpoint.
    budget
        .check()
        .map_err(|kind| refuse(kind, RetryAdvice::Never))?;
    let value = projected.map_err(|kind| {
        let mut refusal = refuse(
            kind,
            if matches!(
                kind,
                QueryRefusalKind::StalePublication | QueryRefusalKind::SourceUnavailable
            ) {
                RetryAdvice::OnEvent
            } else {
                RetryAdvice::Never
            },
        );
        refusal.withheld = budget.withheld.clone();
        refusal
    })?;
    let usage = QueryUsage {
        rows: u64::from(limits.max_results) - budget.remaining_rows() as u64,
        decoded_bytes: super::embed_usage::decoded_bytes(&value),
    };
    budget
        .check()
        .map_err(|kind| refuse(kind, RetryAdvice::Never))?;
    if usage.decoded_bytes > u64::from(limits.max_bytes) {
        return Err(refuse(QueryRefusalKind::BudgetTooSmall, RetryAdvice::Never));
    }
    let partial_files = snapshot
        .generation
        .live
        .all_files()
        .filter(|(_, file)| !matches!(file.parse_status, ParseStatus::Parsed))
        .count() as u64;
    let withheld_files = snapshot.generation.manifest.as_ref().map_or(0, |manifest| {
        manifest
            .entries
            .iter()
            .filter(|entry| {
                !matches!(
                    entry.disposition,
                    crate::domain::FileDisposition::Indexed { .. }
                )
            })
            .count() as u64
    });
    budget
        .check()
        .map_err(|kind| refuse(kind, RetryAdvice::Never))?;
    let prepared_session_result = if budget.reused_handle.is_some() {
        None
    } else {
        session.and_then(|session| {
            session.prepare_result(
                &snapshot,
                request,
                &value,
                budget.cache_output.as_ref(),
                if budget.cache_output.is_some() {
                    budget.cache_truncated
                } else {
                    budget.truncated
                },
            )
        })
    };
    let retrieve_handle = budget.reused_handle.clone().or_else(|| {
        prepared_session_result
            .as_ref()
            .and_then(|prepared| prepared.handle.clone())
    });
    let claim = super::public_api::live_claim(
        value,
        OperationKind::IndexCensus,
        &normalized,
        &snapshot.binding_identity,
        &snapshot.publication_identity,
        snapshot.source_version,
    );
    let cache_disposition = if session.is_none() {
        session::CacheDisposition::NoSession
    } else if budget.reused_handle.is_some() {
        session::CacheDisposition::Reused
    } else if retrieve_handle.is_some() {
        session::CacheDisposition::Stored
    } else if prepared_session_result.is_none() {
        session::CacheDisposition::NotApplicable
    } else {
        session::CacheDisposition::SkippedCapacity
    };
    let session_evidence = session_operation
        .as_ref()
        .map(|operation| operation.evidence());
    let operation_identity = crate::hash::digest_hex(
        &serde_json::to_vec(&(
            "symforge.embed.query.receipt",
            API_VERSION,
            &identity,
            &snapshot.serving_publication_identity,
            &session_evidence,
        ))
        .expect("query receipt fields serialize"),
    );
    let operation = QueryOperationReceipt {
        kind: request.operation(),
        canonical_argument_hash: identity.clone(),
        identity: operation_identity,
        schema_version: API_VERSION,
    };
    let result = QueryClaim {
        claim,
        operation,
        publication_identity: snapshot.serving_publication_identity.clone(),
        source_capture_publication_identity: snapshot.publication_identity.clone(),
        source_version: snapshot.source_version,
        publication_generation: snapshot.generation.publication_generation,
        content_generation: snapshot.generation.content_generation,
        truncated: budget.truncated,
        partial_files,
        withheld_files,
        observations,
        retrieve_handle,
        usage,
        session_evidence,
        cache_disposition,
    };
    let admitted = admit(&result);
    budget
        .check()
        .map_err(|kind| refuse(kind, RetryAdvice::Never))?;
    if !admitted {
        return Err(refuse(
            QueryRefusalKind::OutputAdmissionRejected,
            RetryAdvice::Never,
        ));
    }
    let current = handle
        .capture_query_snapshot(&normalized)
        .map_err(|_| refuse(QueryRefusalKind::SourceUnavailable, RetryAdvice::OnEvent))?;
    if current.serving_publication_identity != snapshot.serving_publication_identity {
        return Err(refuse(
            QueryRefusalKind::StalePublication,
            RetryAdvice::OnEvent,
        ));
    }

    // No fallible projection remains. The source and exact transport envelope have been
    // admitted while the source-bound session operation gate is still held.
    let committed_path = match result.value() {
        QueryOutput::File { file, .. } => Some(file.path.as_str()),
        QueryOutput::Symbol(symbol) => Some(symbol.path.as_str()),
        QueryOutput::Context(context) => Some(context.symbol.path.as_str()),
        QueryOutput::SymbolContext(context)
            if context.estimate.is_none() && context.refusal.is_none() =>
        {
            context.path.as_deref()
        }
        QueryOutput::FileContent(content) if !content.cache_hit => Some(content.path.as_str()),
        QueryOutput::FileContext(content)
            if !content.cache_hit && content.estimated_tokens.is_none() =>
        {
            Some(content.path.as_str())
        }
        QueryOutput::SourcePage(page) => Some(page.path.as_str()),
        _ => None,
    };
    if let Some(path) = committed_path {
        snapshot.record_commitment(&[PathBuf::from(path)]);
    }
    if let QueryOutput::SymbolRead(content) = result.value() {
        if !content.cache_hit && content.estimated_tokens.is_none() {
            let paths = content
                .entries
                .iter()
                .filter(|entry| entry.source.is_some())
                .map(|entry| PathBuf::from(&entry.path))
                .collect::<Vec<_>>();
            snapshot.record_commitment(&paths);
        }
    }
    if let QueryOutput::InspectMatch(content) = result.value() {
        if content.estimated_tokens.is_none() {
            snapshot.record_commitment(&[PathBuf::from(&content.path)]);
        }
    }
    if let Some(session) = session {
        session.commit_observation(result.value());
        if let Some(prepared) = prepared_session_result {
            let committed =
                session.commit_result(&snapshot, request, result.value(), prepared, limits);
            debug_assert_eq!(committed.as_deref(), result.retrieve_handle());
        }
    }
    if let Some(operation) = session_operation {
        let committed = operation.commit();
        debug_assert_eq!(Some(committed), result.session_evidence);
    }
    Ok(result)
}

pub(super) struct Budget {
    rows: usize,
    bytes: usize,
    pub(super) truncated: bool,
    control: Option<OperationControl>,
    limits: QueryLimits,
    pub(super) withheld: Option<crate::embed::parity::read::WithheldMeta>,
    pub(super) reused_handle: Option<String>,
    pub(super) cache_output: Option<QueryOutput>,
    pub(super) cache_truncated: bool,
}

#[cfg(test)]
mod admission_tests {
    use super::*;
    use crate::embed::{EmbeddedSourceSpec, ProcessIndexRuntime, SourceRuntimePhase};
    use std::time::{Duration, Instant};

    #[test]
    fn child_persistent_commitment_root_replacement() {
        if std::env::var_os("SYMFORGE_PRECOMMIT_ROOT_CHILD").is_none() {
            return;
        }
        assert_eq!(
            crate::live_index::frecency::collection_policy_from_env(),
            crate::capability::FrecencyCollectionPolicy::Persistent
        );
        let parent = tempfile::tempdir().unwrap();
        let root = parent.path().join("source");
        let moved = parent.path().join("original");
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("lib.rs"), "pub fn loaded() {}\n").unwrap();
        let runtime = ProcessIndexRuntime::acquire().unwrap();
        let handle = runtime
            .open_embedded_source(EmbeddedSourceSpec::current_worktree(root.clone()))
            .unwrap();
        let until = Instant::now() + Duration::from_secs(20);
        while handle.runtime_view().phase != SourceRuntimePhase::Current {
            assert!(Instant::now() < until);
            std::thread::sleep(Duration::from_millis(5));
        }
        let snapshot = handle.capture_query_snapshot(b"fresh-store-check").unwrap();
        let state = snapshot
            .project_state
            .as_ref()
            .expect("persistent source state");
        let db = crate::live_index::frecency::frecency_db_path(state);
        assert!(
            !db.exists(),
            "fixture must exercise the first persistent store open"
        );
        let session = handle.new_query_session().unwrap();
        let before = session.cache_status();
        let request = QueryRequest::FileContent(crate::embed::parity::read::FileContentRequest {
            path: "lib.rs".into(),
            ..Default::default()
        });
        let mut prospective_handle = None;
        let result = handle.query_with_policy_admitted(
            &request,
            QueryLimits::default(),
            QueryPolicy::default(),
            Some(&session),
            None,
            |claim| {
                prospective_handle = claim.retrieve_handle().map(str::to_owned);
                std::fs::rename(&root, &moved).unwrap();
                std::fs::create_dir(&root).unwrap();
                std::fs::write(root.join("lib.rs"), "pub fn replacement() {}\n").unwrap();
                true
            },
        );
        assert!(
            !db.exists(),
            "a rejected source must not open a persistent store beneath its replacement"
        );
        let refusal = result.unwrap_err();
        assert!(matches!(
            refusal.kind(),
            QueryRefusalKind::SourceUnavailable | QueryRefusalKind::StalePublication
        ));
        assert!(prospective_handle.is_some());
        assert_eq!(session.revision(), 0);
        assert_eq!(session.cache_status(), before);
        handle.close().unwrap();
    }

    #[test]
    fn root_replacement_after_admission_cannot_commit_persistent_ranking_or_session_state() {
        let child = crate::process_util::hidden_command(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "internals::index_lifecycle::embed_query::admission_tests::child_persistent_commitment_root_replacement",
                "--nocapture",
            ])
            .env("SYMFORGE_PRECOMMIT_ROOT_CHILD", "1")
            .env("SYMFORGE_FRECENCY", "persistent")
            .output()
            .unwrap();
        assert!(
            child.status.success(),
            "persistent commitment child failed: {} {}",
            String::from_utf8_lossy(&child.stdout),
            String::from_utf8_lossy(&child.stderr)
        );
        assert!(
            String::from_utf8_lossy(&child.stdout).contains("running 1 test"),
            "persistent commitment child must execute its exact fixture"
        );
    }

    #[test]
    fn cancellation_after_output_admission_does_not_commit_prepared_session() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("lib.rs"), "pub fn loaded() {}\n").unwrap();
        let runtime = ProcessIndexRuntime::acquire().unwrap();
        let handle = runtime
            .open_embedded_source(EmbeddedSourceSpec::current_worktree(
                root.path().to_path_buf(),
            ))
            .unwrap();
        let until = Instant::now() + Duration::from_secs(20);
        while handle.runtime_view().phase != SourceRuntimePhase::Current {
            assert!(Instant::now() < until);
            std::thread::sleep(Duration::from_millis(5));
        }
        let session = handle.new_query_session().unwrap();
        let before = session.cache_status();
        let control = OperationControl::new(Duration::from_secs(20)).unwrap();
        let request = QueryRequest::FileContent(crate::embed::parity::read::FileContentRequest {
            path: "lib.rs".into(),
            ..Default::default()
        });
        let mut prospective_handle = None;
        let refusal = handle
            .query_with_policy_admitted(
                &request,
                QueryLimits::default(),
                QueryPolicy::default(),
                Some(&session),
                Some(&control),
                |claim| {
                    assert_eq!(claim.session_evidence().unwrap().revision_before, 0);
                    assert_eq!(claim.session_evidence().unwrap().revision_after, 1);
                    prospective_handle = claim.retrieve_handle().map(str::to_owned);
                    control.cancel();
                    true
                },
            )
            .unwrap_err();
        assert_eq!(refusal.kind(), QueryRefusalKind::Cancelled);
        assert_eq!(session.revision(), 0);
        assert_eq!(session.cache_status(), before);
        let rejected = handle
            .query_with_session(
                &QueryRequest::Retrieve {
                    handle: prospective_handle.unwrap(),
                    offset: 0,
                },
                QueryLimits::default(),
                &session,
                None,
            )
            .unwrap_err();
        assert_eq!(rejected.kind(), QueryRefusalKind::StaleHandle);
        assert_eq!(session.revision(), 0);
        let delivered = handle
            .query_with_session(&request, QueryLimits::default(), &session, None)
            .unwrap();
        assert_eq!(delivered.session_evidence().unwrap().revision_after, 1);
        assert_eq!(session.revision(), 1);
        assert_eq!(session.cache_status().resident_entries, 1);
    }
}

impl Budget {
    pub(super) fn projection_budget(&self) -> Self {
        Self {
            rows: self.rows,
            bytes: self.bytes,
            truncated: self.truncated,
            control: self.control.clone(),
            limits: self.limits,
            withheld: None,
            reused_handle: None,
            cache_output: None,
            cache_truncated: false,
        }
    }
    pub(super) fn limits(&self) -> QueryLimits {
        self.limits
    }
    pub(super) fn remaining_rows(&self) -> usize {
        self.rows
    }

    pub(super) fn remaining_bytes(&self) -> usize {
        self.bytes
    }

    pub(super) fn charge_bytes(&mut self, bytes: usize) -> Result<(), QueryRefusalKind> {
        self.check()?;
        if bytes > self.bytes {
            return Err(QueryRefusalKind::BudgetTooSmall);
        }
        self.bytes -= bytes;
        Ok(())
    }

    pub(super) fn cap_tokens(&mut self, max_tokens: Option<u64>) {
        if let Some(tokens) = max_tokens {
            self.bytes = self
                .bytes
                .min(usize::try_from(tokens.saturating_mul(4)).unwrap_or(usize::MAX));
        }
    }
    fn new(
        limits: QueryLimits,
        control: Option<OperationControl>,
    ) -> Result<Self, QueryRefusalKind> {
        if limits.max_results == 0
            || limits.max_results > 10_000
            || limits.max_bytes == 0
            || limits.max_bytes > 1_048_576
        {
            return Err(QueryRefusalKind::InvalidRequest);
        }
        Ok(Self {
            rows: limits.max_results as usize,
            bytes: limits.max_bytes as usize,
            truncated: false,
            control,
            limits,
            withheld: None,
            reused_handle: None,
            cache_output: None,
            cache_truncated: false,
        })
    }

    pub(super) fn check(&self) -> Result<(), QueryRefusalKind> {
        let Some(control) = &self.control else {
            return Ok(());
        };
        control.check().map_err(|stop| match stop {
            OperationStop::Cancelled => QueryRefusalKind::Cancelled,
            OperationStop::DeadlineExceeded => QueryRefusalKind::DeadlineExceeded,
        })
    }

    pub(super) fn row(&mut self, bytes: usize) -> bool {
        if self.check().is_err() {
            return false;
        }
        if self.rows == 0 || bytes > self.bytes {
            self.truncated = true;
            return false;
        }
        self.rows -= 1;
        self.bytes -= bytes;
        true
    }

    pub(super) fn required(&mut self, bytes: usize) -> Result<(), QueryRefusalKind> {
        if self.row(bytes) {
            Ok(())
        } else {
            Err(QueryRefusalKind::BudgetTooSmall)
        }
    }

    pub(super) fn source(&mut self, bytes: &[u8]) -> Vec<u8> {
        let count = bytes.len().min(self.remaining_bytes());
        self.bytes -= count;
        self.truncated |= count < bytes.len();
        bytes[..count].to_vec()
    }

    pub(super) fn text(&mut self, text: &str) -> Result<String, QueryRefusalKind> {
        self.required(0)?;
        let mut end = text.len().min(self.bytes);
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        if end == 0 && !text.is_empty() {
            return Err(QueryRefusalKind::BudgetTooSmall);
        }
        self.bytes -= end;
        self.truncated |= end < text.len();
        Ok(text[..end].to_owned())
    }
}

pub(super) fn validate_path(path: &str, prefix: bool) -> Result<(), QueryRefusalKind> {
    if (path.is_empty() && !prefix)
        || path.starts_with('/')
        || path.contains(['\\', ':', '\0'])
        || path
            .split('/')
            .any(|segment| segment == "." || segment == "..")
    {
        return Err(QueryRefusalKind::InvalidRequest);
    }
    Ok(())
}

pub(super) fn validate_kind(kind: Option<&str>) -> Result<(), QueryRefusalKind> {
    if let Some(kind) = kind
        && !matches!(
            kind.to_ascii_lowercase().as_str(),
            "all"
                | "fn"
                | "function"
                | "method"
                | "class"
                | "struct"
                | "enum"
                | "interface"
                | "mod"
                | "module"
                | "const"
                | "constant"
                | "let"
                | "variable"
                | "type"
                | "trait"
                | "impl"
                | "other"
                | "key"
                | "section"
                | "macro-generated"
        )
    {
        return Err(QueryRefusalKind::UnsupportedOption);
    }
    Ok(())
}

fn reference_kind(kind: Option<&str>) -> Result<Option<ReferenceKind>, QueryRefusalKind> {
    Ok(match kind {
        None | Some("all") => None,
        Some("call") => Some(ReferenceKind::Call),
        Some("import") => Some(ReferenceKind::Import),
        Some("type_usage") => Some(ReferenceKind::TypeUsage),
        Some("macro_use") => Some(ReferenceKind::MacroUse),
        Some("implements") => Some(ReferenceKind::Implements),
        Some("value_use") => Some(ReferenceKind::ValueUse),
        _ => return Err(QueryRefusalKind::UnsupportedOption),
    })
}

fn validate_selector(selector: &QuerySymbolSelector) -> Result<(), QueryRefusalKind> {
    validate_path(&selector.path, false)?;
    validate_kind(selector.kind.as_deref())?;
    if selector.name.is_empty() || selector.line == Some(0) {
        return Err(QueryRefusalKind::InvalidRequest);
    }
    Ok(())
}

pub(super) fn validate_request(request: &QueryRequest) -> Result<(), QueryRefusalKind> {
    match request {
        QueryRequest::TextSearch(input) => super::embed_search::validate_text(input)?,
        QueryRequest::FileSearch(input) => super::embed_file_search::validate(input)?,
        QueryRequest::FileContent(input) => super::embed_read::validate(input)?,
        QueryRequest::RepoMap(input) => super::embed_read_context::validate_repo_map(input)?,
        QueryRequest::FileContext(input) => {
            super::embed_read_context::validate_file_context(input)?
        }
        QueryRequest::SymbolRead(input) => super::embed_symbol::validate(input)?,
        QueryRequest::SymbolContext(input) => super::embed_symbol_context::validate(input)?,
        QueryRequest::InspectMatch(input) => super::embed_symbol::validate_inspect(input)?,
        QueryRequest::ReferenceSearch(input) => super::embed_reference::validate(input)?,
        QueryRequest::DependentSearch(input) => super::embed_reference::validate_dependents(input)?,
        QueryRequest::SourcePage(input) => super::embed_read::validate_page(input)?,
        QueryRequest::SearchKnowledge(input) => {
            crate::knowledge::search::validate_input(input)
                .map_err(|_| QueryRefusalKind::InvalidRequest)?;
        }
        QueryRequest::SymbolSearch(input) => super::embed_search::validate_symbol(input)?,
        QueryRequest::Explore(options) => {
            if options.query.trim().is_empty()
                || options.max_tokens == Some(0)
                || !(1..=3).contains(&options.depth)
                || options.limit == 0
                || options.limit > 10_000
            {
                return Err(QueryRefusalKind::InvalidRequest);
            }
            if crate::knowledge::guard_query(&options.query).is_err() {
                return Err(QueryRefusalKind::AdmissionUnavailable);
            }
            if let Some(prefix) = &options.path_prefix {
                validate_path(prefix, true)?;
            }
            super::guidance::filters::parse_language_filter(options.language.as_deref())
                .map_err(|_| QueryRefusalKind::UnsupportedOption)?;
        }
        QueryRequest::WhatChanged(input) => super::embed_changes::validate_what_changed(input)?,
        QueryRequest::DiffSymbols(input) => super::embed_changes::validate_diff_symbols(input)?,
        QueryRequest::DetectImpact(input) => super::embed_detect_impact::validate(input)?,
        QueryRequest::Conventions | QueryRequest::ContextInventory => {}
        QueryRequest::InvestigationSuggest { focus } => {
            if focus
                .as_ref()
                .is_some_and(|focus| crate::knowledge::guard_query(focus).is_err())
            {
                return Err(QueryRefusalKind::AdmissionUnavailable);
            }
        }
        QueryRequest::Retrieve { handle, .. } => {
            if handle.trim().len() != 12
                || !handle.trim().bytes().all(|byte| byte.is_ascii_hexdigit())
            {
                return Err(QueryRefusalKind::InvalidRequest);
            }
        }
        QueryRequest::File {
            path,
            start_line,
            end_line,
        } => {
            validate_path(path, false)?;
            if start_line == &Some(0)
                || end_line == &Some(0)
                || start_line
                    .zip(*end_line)
                    .is_some_and(|(start, end)| start > end)
            {
                return Err(QueryRefusalKind::InvalidRequest);
            }
        }
        QueryRequest::Symbol { selector } | QueryRequest::Context { selector } => {
            validate_selector(selector)?
        }
        QueryRequest::References {
            name, target, kind, ..
        } => {
            if name.is_empty() {
                return Err(QueryRefusalKind::InvalidRequest);
            }
            reference_kind(kind.as_deref())?;
            if let Some(selector) = target {
                validate_selector(selector)?;
            }
        }
        QueryRequest::Dependents { path }
        | QueryRequest::Syntax { path }
        | QueryRequest::Diff { path, .. }
        | QueryRequest::Impact { path, .. } => validate_path(path, false)?,
        QueryRequest::SearchSymbols {
            path_prefix, kind, ..
        } => {
            if let Some(prefix) = path_prefix {
                validate_path(prefix, true)?;
            }
            validate_kind(kind.as_deref())?;
        }
        QueryRequest::SearchText {
            path_prefix, query, ..
        }
        | QueryRequest::SearchFiles {
            path_prefix, query, ..
        } => {
            if let Some(prefix) = path_prefix {
                validate_path(prefix, true)?;
            }
            if query.trim().is_empty() {
                return Err(QueryRefusalKind::InvalidRequest);
            }
        }
        QueryRequest::Graph {
            path_prefix,
            offset,
            expected_publication,
        } => {
            if let Some(prefix) = path_prefix {
                validate_path(prefix, true)?;
            }
            if *offset > 0 && expected_publication.is_none() {
                return Err(QueryRefusalKind::InvalidRequest);
            }
        }
    }
    Ok(())
}

fn file<'a>(live: &'a LiveIndex, path: &str) -> Result<&'a IndexedFile, QueryRefusalKind> {
    live.get_file(path).ok_or_else(|| {
        if live.capture_file_disposition(path).is_some() {
            QueryRefusalKind::AdmissionUnavailable
        } else {
            QueryRefusalKind::NotFound
        }
    })
}

fn selected<'a>(
    live: &'a LiveIndex,
    selector: &QuerySymbolSelector,
) -> Result<(&'a IndexedFile, usize, &'a SymbolRecord), QueryRefusalKind> {
    let file = file(live, &selector.path)?;
    match resolve_symbol_selector(
        file,
        &selector.name,
        selector
            .kind
            .as_deref()
            .filter(|kind| !kind.eq_ignore_ascii_case("all")),
        selector.line,
    ) {
        SymbolSelectorMatch::Selected(index, symbol) => Ok((file, index, symbol)),
        SymbolSelectorMatch::NotFound => Err(QueryRefusalKind::NotFound),
        SymbolSelectorMatch::Ambiguous(_) => Err(QueryRefusalKind::AmbiguousSymbol),
    }
}

fn parse_status(file: &IndexedFile) -> &'static str {
    match file.parse_status {
        ParseStatus::Parsed => "parsed",
        ParseStatus::PartialParse { .. } => "partial",
        ParseStatus::Failed { .. } => "failed",
    }
}

fn file_row(file: &IndexedFile) -> QueryFile {
    QueryFile {
        path: file.relative_path.clone(),
        language: file.language.to_string(),
        content_hash: Some(file.content_hash.clone()),
        parse_status: parse_status(file).into(),
        is_test: file.classification.is_test,
        is_generated: file.classification.is_generated,
        is_vendor: file.classification.is_vendor,
        symbol_count: file.symbols.len() as u32,
        reference_count: file.references.len() as u32,
        byte_len: Some(file.byte_len),
        source: None,
        source_byte_start: 0,
        source_byte_end: 0,
    }
}

pub(super) fn symbol_row(
    file: &IndexedFile,
    index: usize,
    symbol: &SymbolRecord,
) -> Result<QuerySymbol, QueryRefusalKind> {
    if symbol.byte_range.0 > symbol.byte_range.1
        || symbol.byte_range.1 as usize > file.content.len()
    {
        return Err(QueryRefusalKind::InvalidSpan);
    }
    let kind = if symbol.kind == SymbolKind::MacroGenerated {
        "macro-generated".into()
    } else {
        format!("{:?}", symbol.kind).to_lowercase()
    };
    let identity = crate::hash::digest_hex(
        format!(
            "symbol-v1:{:?}:{kind}:{:?}:{:?}",
            file.relative_path, symbol.name, symbol.byte_range
        )
        .as_bytes(),
    );
    Ok(QuerySymbol {
        identity,
        path: file.relative_path.clone(),
        symbol_index: index as u32,
        name: symbol.name.clone(),
        kind,
        depth: symbol.depth,
        start_line: symbol.line_range.0 + 1,
        end_line: symbol.line_range.1 + 1,
        byte_start: symbol.byte_range.0 as u64,
        byte_end: symbol.byte_range.1 as u64,
        source: None,
        source_byte_start: 0,
        source_byte_end: 0,
    })
}

fn reference_row(
    file: &IndexedFile,
    index: usize,
    reference: &ReferenceRecord,
) -> Result<QueryReference, QueryRefusalKind> {
    if reference.byte_range.0 > reference.byte_range.1
        || reference.byte_range.1 as usize > file.content.len()
    {
        return Err(QueryRefusalKind::InvalidSpan);
    }
    let enclosing_symbol_identity = reference
        .enclosing_symbol_index
        .map(|index| {
            let symbol = file
                .symbols
                .get(index as usize)
                .ok_or(QueryRefusalKind::InvalidSpan)?;
            Ok(symbol_row(file, index as usize, symbol)?.identity)
        })
        .transpose()?;
    Ok(QueryReference {
        path: file.relative_path.clone(),
        reference_index: index as u32,
        name: reference.name.clone(),
        qualified_name: reference.qualified_name.clone(),
        kind: reference.kind.to_string(),
        start_line: reference.line_range.0 + 1,
        end_line: reference.line_range.1 + 1,
        byte_start: reference.byte_range.0 as u64,
        byte_end: reference.byte_range.1 as u64,
        enclosing_symbol_index: reference.enclosing_symbol_index,
        enclosing_symbol_identity,
    })
}

fn file_size(row: &QueryFile) -> usize {
    row.path.len()
        + row.language.len()
        + row.content_hash.as_ref().map_or(0, String::len)
        + row.parse_status.len()
}
pub(super) fn symbol_size(row: &QuerySymbol) -> usize {
    row.identity.len() + row.path.len() + row.name.len() + row.kind.len()
}
fn reference_size(row: &QueryReference) -> usize {
    row.path.len()
        + row.name.len()
        + row.kind.len()
        + row.qualified_name.as_ref().map_or(0, String::len)
        + row
            .enclosing_symbol_identity
            .as_ref()
            .map_or(0, String::len)
}

fn symbol_source(
    file: &IndexedFile,
    index: usize,
    symbol: &SymbolRecord,
    budget: &mut Budget,
) -> Result<QuerySymbol, QueryRefusalKind> {
    let mut row = symbol_row(file, index, symbol)?;
    budget.required(symbol_size(&row))?;
    let start = symbol.effective_start() as usize;
    let end = symbol.item_end() as usize;
    let bytes = file
        .content
        .get(start..end)
        .ok_or(QueryRefusalKind::InvalidSpan)?;
    let source = budget.source(bytes);
    row.source_byte_start = start as u64;
    row.source_byte_end = (start + source.len()) as u64;
    row.source = Some(source);
    Ok(row)
}

fn line_span(
    bytes: &[u8],
    start: Option<u32>,
    end: Option<u32>,
) -> Result<(usize, usize), QueryRefusalKind> {
    if bytes.is_empty() {
        return if start.is_none() && end.is_none() {
            Ok((0, 0))
        } else {
            Err(QueryRefusalKind::InvalidRequest)
        };
    }
    let mut starts = vec![0];
    for (index, byte) in bytes.iter().enumerate() {
        if *byte == b'\n' && index + 1 < bytes.len() {
            starts.push(index + 1);
        }
    }
    let first = start.unwrap_or(1) as usize;
    let last = end.map_or(starts.len(), |line| line as usize);
    if first == 0 || first > last || last > starts.len() {
        return Err(QueryRefusalKind::InvalidRequest);
    }
    Ok((
        starts[first - 1],
        starts.get(last).copied().unwrap_or(bytes.len()),
    ))
}

fn keep_reference(file: &IndexedFile, reference: &ReferenceRecord, include_tests: bool) -> bool {
    include_tests
        || (!file.classification.is_test
            && !file.symbols.iter().any(|symbol| {
                symbol.kind == SymbolKind::Module
                    && matches!(symbol.name.as_str(), "test" | "tests")
                    && symbol.byte_range.0 <= reference.byte_range.0
                    && reference.byte_range.1 <= symbol.byte_range.1
            }))
}

fn references(
    live: &LiveIndex,
    refs: Vec<(&str, &ReferenceRecord)>,
    include_tests: bool,
    budget: &mut Budget,
) -> Result<Vec<QueryReference>, QueryRefusalKind> {
    let mut refs = refs;
    refs.sort_by(|(left_path, left), (right_path, right)| {
        left_path
            .cmp(right_path)
            .then(left.byte_range.cmp(&right.byte_range))
            .then(left.name.cmp(&right.name))
    });
    let mut rows = Vec::new();
    for (path, reference) in refs {
        budget.check()?;
        let file = file(live, path)?;
        if !keep_reference(file, reference, include_tests) {
            continue;
        }
        let index = file
            .references
            .iter()
            .position(|candidate| std::ptr::eq(candidate, reference))
            .ok_or(QueryRefusalKind::InvalidSpan)?;
        let row = reference_row(file, index, reference)?;
        if !budget.row(reference_size(&row)) {
            break;
        }
        rows.push(row);
    }
    Ok(rows)
}

fn scope(prefix: Option<&str>) -> PathScope {
    prefix.map_or(PathScope::Any, |prefix| {
        PathScope::prefix(prefix.trim_end_matches('/'))
    })
}

fn project(
    snapshot: &EmbeddedQuerySnapshot,
    request: &QueryRequest,
    budget: &mut Budget,
    observations: &mut Vec<QueryObservation>,
    session: Option<&super::embed_session::QuerySession>,
    policy: QueryPolicy,
) -> Result<QueryOutput, QueryRefusalKind> {
    let live = snapshot.generation.live.as_ref();
    match request {
        QueryRequest::TextSearch(input) => super::embed_search::text(snapshot, input, budget),
        QueryRequest::FileSearch(input) => {
            super::embed_file_search::execute(snapshot, input, policy, budget)
        }
        QueryRequest::FileContent(input) => {
            super::embed_read::content(snapshot, input, session, budget, observations)
        }
        QueryRequest::RepoMap(input) => {
            super::embed_read_context::repo_map(snapshot, input, budget)
        }
        QueryRequest::FileContext(input) => {
            super::embed_read_context::file_context(snapshot, input, session, budget)
        }
        QueryRequest::SourcePage(input) => super::embed_read::page(snapshot, input, budget),
        QueryRequest::SymbolRead(input) => {
            super::embed_symbol::read(snapshot, input, session, budget)
        }
        QueryRequest::SymbolContext(input) => {
            super::embed_symbol_context::project(snapshot, input, budget)
        }
        QueryRequest::InspectMatch(input) => super::embed_symbol::inspect(snapshot, input, budget),
        QueryRequest::ReferenceSearch(input) => {
            super::embed_reference::references(snapshot, input, budget, observations)
        }
        QueryRequest::DependentSearch(input) => {
            super::embed_reference::dependents(snapshot, input, budget, observations)
        }
        QueryRequest::SearchKnowledge(input) => {
            super::embed_knowledge_query::project(snapshot, input, budget)
        }
        QueryRequest::SymbolSearch(input) => super::embed_search::symbols(snapshot, input, budget),
        QueryRequest::ContextInventory => session
            .ok_or(QueryRefusalKind::SessionRequired)?
            .inventory(budget)
            .map(QueryOutput::ContextInventory),
        QueryRequest::InvestigationSuggest { focus } => session
            .ok_or(QueryRefusalKind::SessionRequired)?
            .investigate(snapshot, focus.as_deref(), budget)
            .map(QueryOutput::InvestigationSuggestion),
        QueryRequest::Retrieve { handle, offset } => session
            .ok_or(QueryRefusalKind::SessionRequired)?
            .retrieve(snapshot, handle, *offset, budget)
            .map(QueryOutput::RetrievedOutput),
        QueryRequest::Explore(options) => super::embed_guidance::explore(live, options, budget),
        QueryRequest::WhatChanged(input) => {
            super::embed_changes::what_changed(snapshot, input, budget)
        }
        QueryRequest::DiffSymbols(input) => {
            super::embed_changes::diff_symbols(snapshot, input, budget)
        }
        QueryRequest::DetectImpact(input) => {
            super::embed_detect_impact::project(snapshot, input, budget)
        }
        QueryRequest::Conventions => super::embed_guidance::conventions(live, budget),
        QueryRequest::File {
            path,
            start_line,
            end_line,
        } => {
            let file = file(live, path)?;
            let mut row = file_row(file);
            budget.required(file_size(&row))?;
            let (start, end) = line_span(&file.content, *start_line, *end_line)?;
            let source = budget.source(&file.content[start..end]);
            row.source_byte_start = start as u64;
            row.source_byte_end = (start + source.len()) as u64;
            row.source = Some(source);
            let mut symbols = Vec::new();
            for (index, symbol) in file.symbols.iter().enumerate() {
                let symbol = symbol_row(file, index, symbol)?;
                if !budget.row(symbol_size(&symbol)) {
                    break;
                }
                symbols.push(symbol);
            }
            let imports = references(
                live,
                file.references
                    .iter()
                    .filter(|reference| reference.kind == ReferenceKind::Import)
                    .map(|reference| (path.as_str(), reference))
                    .collect(),
                true,
                budget,
            )?;
            Ok(QueryOutput::File {
                file: row,
                symbols,
                imports,
            })
        }
        QueryRequest::Symbol { selector } => {
            let (file, index, symbol) = selected(live, selector)?;
            Ok(QueryOutput::Symbol(symbol_source(
                file, index, symbol, budget,
            )?))
        }
        QueryRequest::Context { selector } => {
            context(live, selector, budget).map(QueryOutput::Context)
        }
        QueryRequest::References {
            name,
            target,
            kind,
            include_tests,
        } => {
            let kind = reference_kind(kind.as_deref())?;
            let refs = if let Some(selector) = target {
                selected(live, selector)?;
                live.find_exact_references_for_symbol(
                    &selector.path,
                    &selector.name,
                    selector.kind.as_deref(),
                    selector.line,
                    kind,
                )
                .map_err(|_| QueryRefusalKind::InvalidRequest)?
            } else {
                live.find_references_for_name(name, kind, false)
            };
            references(live, refs, *include_tests, budget).map(QueryOutput::References)
        }
        QueryRequest::Dependents { path } => {
            file(live, path)?;
            references(live, live.find_dependents_for_file(path), true, budget)
                .map(QueryOutput::Dependents)
        }
        QueryRequest::SearchSymbols {
            query,
            path_prefix,
            kind,
            include_tests,
        } => {
            let mut options = SymbolSearchOptions::for_current_code_search(budget.rows);
            options.path_scope = scope(path_prefix.as_deref());
            options.noise_policy.include_tests = *include_tests;
            options.result_limit = ResultLimit::new(budget.rows);
            let found = search::search_symbols_with_options(
                live,
                query.as_deref().unwrap_or(""),
                kind.as_deref(),
                &options,
            );
            budget.truncated |= found.overflow_count > 0;
            let mut rows = Vec::new();
            let mut consumed = BTreeSet::new();
            for hit in found.hits {
                let file = file(live, &hit.path)?;
                let (index, symbol) = file
                    .symbols
                    .iter()
                    .enumerate()
                    .find(|(index, symbol)| {
                        !consumed.contains(&(hit.path.clone(), *index))
                            && symbol.name == hit.name
                            && symbol.line_range.0 + 1 == hit.line
                            && symbol.kind.to_string() == hit.kind
                            && kind.as_deref().is_none_or(|filter| {
                                filter.eq_ignore_ascii_case("all")
                                    || search::kind_filter_matches(filter, &symbol.kind)
                            })
                    })
                    .ok_or(QueryRefusalKind::InvalidSpan)?;
                consumed.insert((hit.path.clone(), index));
                let row = symbol_row(file, index, symbol)?;
                if !budget.row(symbol_size(&row)) {
                    break;
                }
                rows.push(row);
            }
            Ok(QueryOutput::Symbols(rows))
        }
        QueryRequest::SearchText {
            query,
            path_prefix,
            regex,
            case_sensitive,
            include_tests,
        } => {
            let mut options = TextSearchOptions::for_current_code_search();
            options.path_scope = scope(path_prefix.as_deref());
            options.noise_policy.include_tests = *include_tests;
            options.case_sensitive = Some(*case_sensitive);
            options.total_limit = budget.rows;
            options.max_per_file = budget.rows;
            let found = search::search_text_with_options(live, Some(query), None, *regex, &options)
                .map_err(|_| QueryRefusalKind::InvalidRequest)?;
            budget.truncated |= found.overflow_count > 0;
            let mut rows = Vec::new();
            'files: for hit_file in found.files {
                for hit in hit_file.matches {
                    let Some(row) = text_line(
                        live,
                        &hit_file.path,
                        hit.line_number as u32,
                        hit.line,
                        budget,
                    )?
                    else {
                        break 'files;
                    };
                    rows.push(row);
                }
            }
            Ok(QueryOutput::Text(rows))
        }
        QueryRequest::SearchFiles {
            query,
            path_prefix,
            include_tests,
        } => search_files(live, query, path_prefix.as_deref(), *include_tests, budget),
        QueryRequest::Graph {
            path_prefix,
            offset,
            expected_publication,
        } => {
            if expected_publication
                .as_deref()
                .is_some_and(|expected| expected != snapshot.serving_publication_identity)
            {
                return Err(QueryRefusalKind::StalePublication);
            }
            graph(live, path_prefix.as_deref(), *offset, budget).map(QueryOutput::Graph)
        }
        QueryRequest::Syntax { path } => {
            let file = file(live, path)?;
            let diagnostic = file
                .parse_diagnostic
                .as_ref()
                .map(|diagnostic| diagnostic.message.clone());
            budget.required(
                path.len()
                    + file.content_hash.len()
                    + parse_status(file).len()
                    + diagnostic.as_ref().map_or(0, String::len),
            )?;
            Ok(QueryOutput::Syntax(QuerySyntax {
                path: path.clone(),
                content_hash: file.content_hash.clone(),
                valid: matches!(file.parse_status, ParseStatus::Parsed),
                parse_status: parse_status(file).into(),
                diagnostic,
                line: file
                    .parse_diagnostic
                    .as_ref()
                    .and_then(|diagnostic| diagnostic.line),
                column: file
                    .parse_diagnostic
                    .as_ref()
                    .and_then(|diagnostic| diagnostic.column),
            }))
        }
        QueryRequest::Diff {
            path,
            from_ref,
            to_ref,
        } => diff(
            snapshot,
            path,
            from_ref,
            to_ref.as_deref(),
            budget,
            observations,
        )
        .map(QueryOutput::Diff),
        QueryRequest::Impact { path, base_ref } => {
            let diff = diff(snapshot, path, base_ref, None, budget, observations)?;
            let dependents = references(live, live.find_dependents_for_file(path), true, budget)?;
            Ok(QueryOutput::Impact { diff, dependents })
        }
    }
}

fn context(
    live: &LiveIndex,
    selector: &QuerySymbolSelector,
    budget: &mut Budget,
) -> Result<QueryContext, QueryRefusalKind> {
    let (file, index, symbol) = selected(live, selector)?;
    let row = symbol_source(file, index, symbol, budget)?;
    let callers = live
        .find_exact_references_for_symbol(
            &selector.path,
            &selector.name,
            selector.kind.as_deref(),
            selector.line,
            Some(ReferenceKind::Call),
        )
        .map_err(|_| QueryRefusalKind::InvalidRequest)?;
    let callers = references(live, callers, true, budget)?;
    let (unresolved, callees): (Vec<_>, Vec<_>) = live
        .callees_for_symbol(&selector.path, index)
        .into_iter()
        .partition(|reference| {
            reference.name == symbol.name
                && reference.byte_range.0 > 0
                && file.content.get(reference.byte_range.0 as usize - 1) == Some(&b'.')
        });
    let callees = references(
        live,
        callees
            .into_iter()
            .map(|reference| (selector.path.as_str(), reference))
            .collect(),
        true,
        budget,
    )?;
    let unresolved_same_name_member_calls = references(
        live,
        unresolved
            .into_iter()
            .map(|reference| (selector.path.as_str(), reference))
            .collect(),
        true,
        budget,
    )?;
    let type_refs = live.type_refs_for_symbol(&selector.path, index);
    let type_names: Vec<_> = type_refs
        .iter()
        .map(|reference| reference.name.as_str())
        .filter(|name| *name != symbol.name)
        .collect();
    let type_usages = references(
        live,
        type_refs
            .iter()
            .map(|reference| (selector.path.as_str(), *reference))
            .collect(),
        true,
        budget,
    )?;
    let mut dependencies = Vec::new();
    for dependency in live.resolve_type_dependencies(&type_names, 2) {
        let file = file_by_path(live, &dependency.file_path)?;
        if let Some((index, symbol)) = file.symbols.iter().enumerate().find(|(_, symbol)| {
            symbol.name == dependency.name && symbol.line_range == dependency.line_range
        }) {
            let row = symbol_row(file, index, symbol)?;
            if !budget.row(symbol_size(&row)) {
                break;
            }
            dependencies.push(row);
        }
    }
    Ok(QueryContext {
        symbol: row,
        callers,
        callees,
        type_usages,
        dependencies,
        unresolved_same_name_member_calls,
    })
}

// Avoid shadowing the selector's source file in context construction.
fn file_by_path<'a>(live: &'a LiveIndex, path: &str) -> Result<&'a IndexedFile, QueryRefusalKind> {
    file(live, path)
}

pub(super) fn text_line(
    live: &LiveIndex,
    path: &str,
    line: u32,
    preview: String,
    budget: &mut Budget,
) -> Result<Option<QueryTextMatch>, QueryRefusalKind> {
    let file = file(live, path)?;
    let (start, mut end) = line_span(&file.content, Some(line), Some(line))?;
    while end > start && matches!(file.content[end - 1], b'\r' | b'\n') {
        end -= 1;
    }
    let enclosing_symbol = crate::domain::find_enclosing_symbol(&file.symbols, line - 1)
        .map(|index| symbol_row(file, index as usize, &file.symbols[index as usize]))
        .transpose()?;
    if !budget.row(path.len() + preview.len() + enclosing_symbol.as_ref().map_or(0, symbol_size)) {
        return Ok(None);
    }
    Ok(Some(QueryTextMatch {
        path: path.to_owned(),
        line,
        byte_start: start as u64,
        byte_end: end as u64,
        preview,
        enclosing_symbol,
    }))
}

fn graph(
    live: &LiveIndex,
    prefix: Option<&str>,
    offset: u64,
    budget: &mut Budget,
) -> Result<QueryGraph, QueryRefusalKind> {
    let mut graph = QueryGraph {
        files: Vec::new(),
        symbols: Vec::new(),
        references: Vec::new(),
        next_offset: None,
    };
    let mut cursor = 0u64;
    let scope = scope(prefix);
    let mut files: Vec<_> = live
        .all_files()
        .filter(|(path, _)| scope.matches(path))
        .collect();
    files.sort_unstable_by(|left, right| left.0.cmp(right.0));
    for (_, file) in files {
        budget.check()?;
        if cursor >= offset {
            let row = file_row(file);
            if !budget.row(file_size(&row)) {
                if cursor == offset {
                    return Err(QueryRefusalKind::BudgetTooSmall);
                }
                graph.next_offset = Some(cursor);
                return Ok(graph);
            }
            graph.files.push(row);
        }
        cursor += 1;
        for (index, symbol) in file.symbols.iter().enumerate() {
            budget.check()?;
            if cursor >= offset {
                let row = symbol_row(file, index, symbol)?;
                if !budget.row(symbol_size(&row)) {
                    if cursor == offset {
                        return Err(QueryRefusalKind::BudgetTooSmall);
                    }
                    graph.next_offset = Some(cursor);
                    return Ok(graph);
                }
                graph.symbols.push(row);
            }
            cursor += 1;
        }
        for (index, reference) in file.references.iter().enumerate() {
            budget.check()?;
            if cursor >= offset {
                let row = reference_row(file, index, reference)?;
                if !budget.row(reference_size(&row)) {
                    if cursor == offset {
                        return Err(QueryRefusalKind::BudgetTooSmall);
                    }
                    graph.next_offset = Some(cursor);
                    return Ok(graph);
                }
                graph.references.push(row);
            }
            cursor += 1;
        }
    }
    if offset > cursor {
        return Err(QueryRefusalKind::InvalidRequest);
    }
    Ok(graph)
}

fn search_files(
    live: &LiveIndex,
    query: &str,
    prefix: Option<&str>,
    include_tests: bool,
    budget: &mut Budget,
) -> Result<QueryOutput, QueryRefusalKind> {
    let found = live.capture_search_files_view_with_test_filter(
        query,
        budget.rows,
        None,
        None,
        true,
        true,
        &scope(prefix),
        include_tests,
    );
    let mut rows = Vec::new();
    match found {
        crate::live_index::SearchFilesView::EmptyQuery => {
            return Err(QueryRefusalKind::InvalidRequest);
        }
        crate::live_index::SearchFilesView::NotFound { .. } => {}
        crate::live_index::SearchFilesView::Found {
            hits,
            overflow_count,
            ..
        } => {
            budget.truncated |= overflow_count > 0;
            for hit in hits {
                let classification = crate::domain::FileClassification::for_code_path(&hit.path);
                let row = live
                    .get_file(&hit.path)
                    .map(file_row)
                    .unwrap_or_else(|| QueryFile {
                        path: hit.path.clone(),
                        language: crate::domain::LanguageId::from_path(&hit.path)
                            .map(|language| language.to_string())
                            .unwrap_or_default(),
                        content_hash: None,
                        parse_status: format!(
                            "metadata-only:{}",
                            hit.metadata_reason.unwrap_or_default()
                        ),
                        is_test: classification.is_test,
                        is_generated: classification.is_generated,
                        is_vendor: classification.is_vendor,
                        symbol_count: 0,
                        reference_count: 0,
                        byte_len: None,
                        source: None,
                        source_byte_start: 0,
                        source_byte_end: 0,
                    });
                if !budget.row(file_size(&row)) {
                    break;
                }
                rows.push(row);
            }
        }
    }
    Ok(QueryOutput::Files(rows))
}

fn git_blob(
    root: &Path,
    path: &str,
    reference: &str,
    observations: &mut Vec<QueryObservation>,
) -> Result<(String, Option<Vec<u8>>), QueryRefusalKind> {
    if crate::knowledge::sensitive_path_rule_at(path, &root.join(path)).is_some() {
        return Err(QueryRefusalKind::AdmissionUnavailable);
    }
    let repo = git2::Repository::open(root).map_err(|_| QueryRefusalKind::GitUnavailable)?;
    let commit = repo
        .revparse_single(reference)
        .and_then(|object| object.peel_to_commit())
        .map_err(|_| QueryRefusalKind::GitUnavailable)?;
    let commit_id = commit.id().to_string();
    let tree = commit
        .tree()
        .map_err(|_| QueryRefusalKind::GitUnavailable)?;
    let entry = match tree.get_path(Path::new(path)) {
        Ok(entry) => entry,
        Err(error) if error.code() == git2::ErrorCode::NotFound => {
            observations.push(QueryObservation {
                kind: "git-absence".into(),
                identity: format!("{commit_id}:{path}"),
                content_hash: String::new(),
            });
            return Ok((commit_id, None));
        }
        Err(_) => return Err(QueryRefusalKind::GitUnavailable),
    };
    if entry.kind() != Some(git2::ObjectType::Blob) || entry.filemode() == 0o120000 {
        return Err(QueryRefusalKind::AdmissionUnavailable);
    }
    let blob = repo
        .find_blob(entry.id())
        .map_err(|_| QueryRefusalKind::GitUnavailable)?;
    let language = crate::domain::LanguageId::from_path(path);
    if blob.size() > crate::domain::index::METADATA_ONLY_CODE_BYTES as usize
        || !matches!(
            crate::knowledge::classify_stable_content(
                path,
                IndexTargets::for_path(path, language.as_ref()),
                blob.content()
            ),
            crate::knowledge::StableContentAdmission::Admitted
        )
    {
        return Err(QueryRefusalKind::AdmissionUnavailable);
    }
    observations.push(QueryObservation {
        kind: "git-blob".into(),
        identity: format!("{commit_id}:{path}"),
        content_hash: crate::hash::digest_hex(blob.content()),
    });
    Ok((commit_id, Some(blob.content().to_vec())))
}

type DiffSymbols = BTreeMap<(String, String, u32), (String, u32)>;

fn diff_symbols(path: &str, bytes: Option<&[u8]>) -> Result<(DiffSymbols, bool), QueryRefusalKind> {
    let Some(bytes) = bytes else {
        return Ok((BTreeMap::new(), false));
    };
    let language =
        crate::domain::LanguageId::from_path(path).ok_or(QueryRefusalKind::UnsupportedOption)?;
    let parsed = crate::parsing::process_file(path, bytes, language);
    let partial = !matches!(parsed.outcome, crate::domain::FileOutcome::Processed);
    let mut counts = BTreeMap::new();
    let mut symbols = BTreeMap::new();
    for symbol in parsed.symbols {
        let kind = format!("{:?}", symbol.kind).to_lowercase();
        let occurrence = counts
            .entry((kind.clone(), symbol.name.clone()))
            .or_insert(0u32);
        let body = bytes
            .get(symbol.byte_range.0 as usize..symbol.byte_range.1 as usize)
            .ok_or(QueryRefusalKind::InvalidSpan)?;
        symbols.insert(
            (kind, symbol.name, *occurrence),
            (crate::hash::digest_hex(body), symbol.line_range.0 + 1),
        );
        *occurrence += 1;
    }
    Ok((symbols, partial))
}

fn diff(
    snapshot: &EmbeddedQuerySnapshot,
    path: &str,
    from_ref: &str,
    to_ref: Option<&str>,
    budget: &mut Budget,
    observations: &mut Vec<QueryObservation>,
) -> Result<QuerySymbolDiff, QueryRefusalKind> {
    let (from_commit, before) = git_blob(&snapshot.root, path, from_ref, observations)?;
    let (to_commit, after) = if let Some(reference) = to_ref {
        let (commit, bytes) = git_blob(&snapshot.root, path, reference, observations)?;
        (Some(commit), bytes)
    } else {
        let live = snapshot.generation.live.as_ref();
        let current = live.get_file(path);
        if current.is_none() && live.capture_file_disposition(path).is_some() {
            return Err(QueryRefusalKind::AdmissionUnavailable);
        }
        (None, current.map(|file| file.content.clone()))
    };
    budget.required(path.len() + from_commit.len() + to_commit.as_ref().map_or(0, String::len))?;
    let (before, before_partial) = diff_symbols(path, before.as_deref())?;
    let (after, after_partial) = diff_symbols(path, after.as_deref())?;
    let mut keys: Vec<_> = before.keys().chain(after.keys()).cloned().collect();
    keys.sort();
    keys.dedup();
    let mut changes = Vec::new();
    for (kind, name, occurrence) in keys {
        let key = (kind.clone(), name.clone(), occurrence);
        let before = before.get(&key);
        let after = after.get(&key);
        if before.map(|entry| &entry.0) == after.map(|entry| &entry.0) {
            continue;
        }
        let change = match (before, after) {
            (None, Some(_)) => "added",
            (Some(_), None) => "removed",
            _ => "modified",
        };
        let before_hash = before.map(|entry| entry.0.clone());
        let after_hash = after.map(|entry| entry.0.clone());
        let size = kind.len()
            + name.len()
            + change.len()
            + before_hash.as_ref().map_or(0, String::len)
            + after_hash.as_ref().map_or(0, String::len);
        if !budget.row(size) {
            break;
        }
        changes.push(QuerySymbolChange {
            name,
            kind,
            occurrence,
            change: change.into(),
            before_hash,
            after_hash,
            before_line: before.map(|entry| entry.1),
            after_line: after.map(|entry| entry.1),
        });
    }
    Ok(QuerySymbolDiff {
        path: path.into(),
        from_commit,
        to_commit,
        changes,
        partial: before_partial || after_partial,
    })
}
