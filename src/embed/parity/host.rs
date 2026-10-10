//! Transport-neutral room binding for an embedded SymForge source.
//!
//! Wire requests carry no root, room authority, or state path. A trusted host
//! keeps one opaque `HostRoom` per authorized room and chooses that capability
//! before dispatching a deserialized request. Terminal Commander is a separate
//! provider behind the host's broker; this module has no terminal dependency.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use super::source_options::EmbeddedOpenOptions;

use serde::{Deserialize, Serialize};

use crate::embed::parity::edit::{EditApplyAuthority, EditError, WireEditRequest};
use crate::embed::parity::knowledge::{WireKnowledgeError, WireKnowledgeRequest};
use crate::embed::parity::read::WithheldMeta;
use crate::embed::parity::remediation::{
    SecretApplyAuthority, SecretExternalTool, SecretFindingsClaim, SecretRemediationApplied,
    SecretRemediationError, SecretRemediationPreview, SecretRemediationRefusalKind,
    SecretRemediationRequest, SecretRemediationScope, SecretScanLimits,
};
use crate::embed::parity::session::{
    CacheDisposition, QuerySession, SessionCacheLimits, SessionEvidence,
};
use crate::embed::parity::{
    QueryClaim, QueryLimits, QueryObservation, QueryOperationKind, QueryOutput, QueryPolicy,
    QueryRefusalKind, QueryRequest, QueryUsage,
};
use crate::embed::{
    EmbeddedSourceHandle, EmbeddedSourceSpec, ProcessIndexRuntime, SourceCloseReceipt,
};

const MAX_ROOM_ID_BYTES: usize = 128;
const MAX_SCOPE_BYTES: usize = 256;
const MAX_WIRE_REQUEST_BYTES: u32 = 1_048_576;
const MAX_WIRE_RESPONSE_BYTES: u32 = 2_097_152;
static NEXT_HOST_INSTANCE: AtomicU64 = AtomicU64::new(1);

fn new_instance_identity() -> String {
    let opened_nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!(
        "sf-{}-{opened_nanos:x}-{}",
        std::process::id(),
        NEXT_HOST_INSTANCE.fetch_add(1, Ordering::Relaxed)
    )
}

/// Host-enforced limits. They may be raised for large sources; query ceilings
/// are the existing engine's 10,000 rows and 1 MiB returned payload.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostLimits {
    pub max_request_bytes: u32,
    pub max_response_bytes: u32,
    pub max_query_results: u32,
    pub max_query_bytes: u32,
    pub max_indexed_files: u64,
    pub max_indexed_bytes: u64,
    pub max_duration_ms: u64,
    pub max_secret_scan_files: u32,
    pub max_secret_findings: u32,
    pub max_secret_scan_bytes: u64,
    pub max_session_cache_bytes: u64,
    pub max_session_cache_entries: u32,
}

impl Default for HostLimits {
    fn default() -> Self {
        Self {
            max_request_bytes: 65_536,
            max_response_bytes: 1_048_576,
            max_query_results: 1_000,
            max_query_bytes: 262_144,
            max_indexed_files: 100_000,
            max_indexed_bytes: 1_073_741_824,
            max_duration_ms: 120_000,
            max_secret_scan_files: 4_096,
            max_secret_findings: 256,
            max_secret_scan_bytes: 64 * 1024 * 1024,
            max_session_cache_bytes: SessionCacheLimits::default().max_bytes,
            max_session_cache_entries: SessionCacheLimits::default().max_entries,
        }
    }
}

impl HostLimits {
    fn validate(self) -> Result<(), HostRefusal> {
        if self.max_request_bytes == 0
            || self.max_request_bytes > MAX_WIRE_REQUEST_BYTES
            || self.max_response_bytes < 4096
            || self.max_response_bytes > MAX_WIRE_RESPONSE_BYTES
            || self.max_query_results == 0
            || self.max_query_results > 10_000
            || self.max_query_bytes == 0
            || self.max_query_bytes > 1_048_576
            || self.max_indexed_files == 0
            || self.max_indexed_bytes == 0
            || self.max_duration_ms == 0
            || self.max_secret_scan_files == 0
            || self.max_secret_scan_files > 100_000
            || self.max_secret_findings == 0
            || self.max_secret_findings > 10_000
            || self.max_secret_scan_bytes == 0
            || self.max_session_cache_bytes > SessionCacheLimits::default().max_bytes
            || self.max_session_cache_entries > SessionCacheLimits::default().max_entries
        {
            return Err(HostRefusal::new(HostRefusalKind::InvalidConfig));
        }
        Ok(())
    }
}

/// Serializable limits and room label. Source selection never comes from wire.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostRoomConfig {
    pub room_id: String,
    pub limits: HostLimits,
}

/// Rights constructed by trusted host code, never from a wire request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HostRights {
    pub query: bool,
    pub refresh: bool,
    pub checkpoint: bool,
    pub export_artifact: bool,
    pub edit: bool,
    pub curate_knowledge: bool,
    pub inspect_secrets: bool,
    pub remediate_secrets: bool,
    pub derived_state_prepare: bool,
}

impl HostRights {
    pub fn read_only() -> Self {
        Self {
            query: true,
            refresh: false,
            checkpoint: false,
            export_artifact: false,
            edit: false,
            curate_knowledge: false,
            inspect_secrets: false,
            remediate_secrets: false,
            derived_state_prepare: false,
        }
    }
}

/// Opaque source grant. AAP's authenticated room broker must create and retain
/// it from the guest provider's authoritative visible workspace and trust.
/// JSON cannot construct this type or select a different host filesystem root.
pub struct HostRoomGrant {
    room_id: String,
    source_root: PathBuf,
    source_scope: String,
    rights: HostRights,
    secret_external_tool: Option<SecretExternalTool>,
}

impl HostRoomGrant {
    pub fn new(
        room_id: String,
        source_root: PathBuf,
        source_scope: String,
        rights: HostRights,
    ) -> Result<Self, HostRefusal> {
        if room_id.is_empty()
            || room_id.len() > MAX_ROOM_ID_BYTES
            || source_scope.is_empty()
            || source_scope.len() > MAX_SCOPE_BYTES
            || !source_root.is_absolute()
            || !source_root.is_dir()
        {
            return Err(HostRefusal::new(HostRefusalKind::InvalidConfig));
        }
        Ok(Self {
            room_id,
            source_root,
            source_scope,
            rights,
            secret_external_tool: None,
        })
    }

    /// Add a trusted local encryptor; wire requests cannot set this descriptor.
    pub fn with_secret_external_tool(mut self, tool: SecretExternalTool) -> Self {
        self.secret_external_tool = Some(tool);
        self
    }
}

/// The cancellation/deadline handle shared by query, lifecycle and host waits.
/// Cancellation is observed at safe checkpoints. A checkpoint already writing
/// reports its actual result, even if the deadline expires during the write.
#[derive(Clone)]
pub struct OperationControl {
    deadline: Instant,
    cancelled: Arc<AtomicBool>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OperationStop {
    Cancelled,
    DeadlineExceeded,
}

impl OperationControl {
    pub fn new(timeout: Duration) -> Result<Self, HostRefusal> {
        let deadline = Instant::now()
            .checked_add(timeout)
            .filter(|_| !timeout.is_zero())
            .ok_or_else(|| HostRefusal::new(HostRefusalKind::InvalidConfig))?;
        Ok(Self {
            deadline,
            cancelled: Arc::new(AtomicBool::new(false)),
        })
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    pub fn check(&self) -> Result<(), OperationStop> {
        if self.cancelled.load(Ordering::Acquire) {
            return Err(OperationStop::Cancelled);
        }
        if Instant::now() >= self.deadline {
            return Err(OperationStop::DeadlineExceeded);
        }
        Ok(())
    }

    pub fn remaining(&self) -> Duration {
        self.deadline.saturating_duration_since(Instant::now())
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub enum HostRequest {
    Status,
    Health,
    Catalog,
    Resource(HostResourceRequest),
    Prompt(HostPromptRequest),
    Query {
        request: QueryRequest,
        limits: QueryLimits,
    },
    Edit {
        request: WireEditRequest,
        operation_key: Option<String>,
    },
    Knowledge {
        request: WireKnowledgeRequest,
        operation_key: Option<String>,
    },
    InspectSecrets {
        scope: SecretRemediationScope,
        limits: SecretScanLimits,
    },
    PreviewSecretRemediation {
        request: SecretRemediationRequest,
        limits: SecretScanLimits,
    },
    ApplySecretRemediation {
        preview: SecretRemediationPreview,
        operation_key: String,
    },
    Refresh,
    Checkpoint {
        verify_after_write: bool,
        export_artifact: bool,
    },
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub enum HostResourceRequest {
    RepoHealth,
    RepoOutline,
    RepoMap,
    RepoChangesUncommitted,
    FileContext {
        path: String,
        max_tokens: Option<u64>,
    },
    FileContent {
        path: String,
        start_line: Option<u32>,
        end_line: Option<u32>,
    },
    FileContentOptions(crate::embed::parity::read::FileContentRequest),
    SymbolDetail {
        selector: crate::embed::parity::QuerySymbolSelector,
    },
    SymbolDetailOptions(crate::embed::parity::symbol::SymbolReadRequest),
    SymbolContext {
        selector: crate::embed::parity::QuerySymbolSelector,
    },
    SymbolContextOptions(crate::embed::parity::symbol_context::SymbolContextRequest),
    ToolsCatalog,
    Glossary,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub enum HostPromptRequest {
    Admin,
    KnowledgeHygiene {
        path_prefix: Option<String>,
    },
    Review {
        path: Option<String>,
        focus: Option<String>,
    },
    Architecture {
        area: Option<String>,
    },
    Triage {
        symptom: String,
        path: Option<String>,
    },
    Onboard {
        area: Option<String>,
    },
    Refactor {
        goal: String,
        target: Option<String>,
    },
    Debug {
        error: String,
        path: Option<String>,
    },
}

/// Read-only service observation supplied by the host, never inferred from a
/// deserialized prompt request. `Unobserved` refuses the admin prompt.
pub enum HostPromptContext {
    Unobserved,
    DashboardNotRunning,
    DashboardRunning { url: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum HostPhase {
    Blocked,
    Current,
    Loading,
    Refreshing,
    Stopped,
    Stopping,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum HostStateLocation {
    ProjectLocal,
    UserLocal,
    MemoryOnly,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostProgress {
    pub files_discovered: u64,
    pub files_parsed: u64,
    pub symbols_found: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostStatus {
    pub room_id: String,
    pub engine: HostEngineIdentity,
    pub phase: HostPhase,
    pub binding_identity: String,
    pub source_capture_publication_identity: Option<String>,
    pub source_version: u64,
    pub observer_epoch: u64,
    pub progress: HostProgress,
}

/// Diagnostic build/runtime identity; the build version is not a release attestation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostEngineIdentity {
    pub engine: String,
    pub engine_version: String,
    pub api_version: u32,
    pub snapshot_format_version: u32,
    pub secret_policy_version: u32,
    pub grammars: Vec<String>,
    pub instance_identity: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostSourceProof {
    pub binding_identity: String,
    /// The frozen source-capture counter, distinct from the query serving token.
    pub source_capture_publication_identity: String,
    pub source_version: u64,
    pub publication_generation: u64,
    pub content_generation: u64,
    pub source_identity_digest: String,
    pub source_version_digest: String,
    pub manifest_digest: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum HostVerifyPhase {
    NotNeeded,
    Pending,
    Running,
    Completed,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostPublishedHealth {
    pub proof: HostSourceProof,
    pub state_location: HostStateLocation,
    pub files: u64,
    pub symbols: u64,
    pub parsed: u64,
    pub partial_parse: u64,
    pub failed_parse: u64,
    pub load_source: String,
    pub snapshot_verify: HostVerifyPhase,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostHealth {
    pub status: HostStatus,
    /// `None` means no current, source-proofed publication was captured.
    pub current: Option<HostPublishedHealth>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum HostCheckpointOutcome {
    WrittenCurrent,
    WrittenButSourceMoved,
    VerificationFailed,
    Uncertain,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum HostArtifactOutcome {
    NotRequested,
    Written { compressed_bytes: u64 },
    Unavailable,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostCheckpointReceipt {
    pub outcome: HostCheckpointOutcome,
    pub written_files: u64,
    pub written_bytes: u64,
    pub artifact: HostArtifactOutcome,
    pub proof: Option<HostSourceProof>,
    pub completed_after_deadline: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HostQueryReply {
    pub output: QueryOutput,
    pub usage: QueryUsage,
    pub session_evidence: Option<SessionEvidence>,
    pub cache_disposition: CacheDisposition,
    /// Opaque handle for exact bounded retrieval within this room's session.
    pub retrieval_handle: Option<String>,
    pub semantic_operation: QueryOperationKind,
    pub operation_identity: String,
    pub canonical_argument_hash: String,
    pub semantic_operation_schema_version: u32,
    /// The frozen V11 source-capture receipt kind, not the semantic query kind.
    pub source_capture_kind: String,
    pub source_capture_schema_version: u32,
    pub source_capture_canonical_argument_hash: String,
    pub evaluation_identity: Option<String>,
    /// Process-incarnation producer identity for this serving claim.
    pub producing_runtime_identity: String,
    pub source_capture_runtime_identity: String,
    pub provenance: HostClaimProvenance,
    /// Restart-safe serving token; use this for query pagination and receipts.
    pub publication_identity: String,
    /// Frozen source-capture counter, which may repeat after a restart.
    pub source_capture_publication_identity: String,
    pub source_version: u64,
    pub publication_generation: u64,
    pub content_generation: u64,
    pub truncated: bool,
    pub partial_files: u64,
    pub withheld_files: u64,
    pub observations: Vec<QueryObservation>,
    pub effective_limits: QueryLimits,
}

impl HostQueryReply {
    fn from_claim(claim: &QueryClaim, limits: QueryLimits) -> Self {
        Self {
            output: claim.value().clone(),
            usage: claim.usage(),
            session_evidence: claim.session_evidence().cloned(),
            cache_disposition: claim.cache_disposition(),
            retrieval_handle: claim.retrieve_handle().map(str::to_owned),
            semantic_operation: claim.operation(),
            operation_identity: claim.operation_identity().to_owned(),
            canonical_argument_hash: claim.canonical_argument_hash().to_owned(),
            semantic_operation_schema_version: claim.operation_receipt().schema_version(),
            source_capture_kind: claim.source_capture_receipt_kind().kind_name().to_owned(),
            source_capture_schema_version: claim.source_capture_schema_version(),
            source_capture_canonical_argument_hash: claim
                .source_capture_canonical_argument_hash()
                .to_owned(),
            evaluation_identity: claim.evaluation_identity().map(str::to_owned),
            producing_runtime_identity: claim.producing_runtime_identity().to_owned(),
            source_capture_runtime_identity: claim.source_capture_runtime_identity().to_owned(),
            provenance: HostClaimProvenance {
                identity: claim.provenance().identity().to_owned(),
                kind_name: claim.provenance().kind_name().to_owned(),
                authorities: claim
                    .provenance()
                    .authorities()
                    .iter()
                    .map(|authority| HostAuthorityIdentity {
                        identity: authority.identity().to_owned(),
                        kind_name: authority.kind_name().to_owned(),
                    })
                    .collect(),
            },
            publication_identity: claim.publication_identity().to_owned(),
            source_capture_publication_identity: claim
                .source_capture_publication_identity()
                .to_owned(),
            source_version: claim.source_version(),
            publication_generation: claim.publication_generation(),
            content_generation: claim.content_generation(),
            truncated: claim.truncated(),
            partial_files: claim.partial_files(),
            withheld_files: claim.withheld_files(),
            observations: claim.observations().to_vec(),
            effective_limits: limits,
        }
    }
}

/// Wire identities describe observed sources but never convey native authority.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostClaimProvenance {
    pub identity: String,
    pub kind_name: String,
    pub authorities: Vec<HostAuthorityIdentity>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostAuthorityIdentity {
    pub identity: String,
    pub kind_name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostRefreshReceipt {
    pub ticket_identity: String,
    pub requested_source_version: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostCloseReport {
    pub already_terminal: bool,
    pub terminal_source_version: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostShutdownReport {
    pub closed_sources: u64,
    pub joined_workers: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostCatalog {
    pub engine: HostEngineIdentity,
    pub limits: HostLimits,
    pub query_operations: Vec<String>,
    pub edit_operations: Vec<String>,
    pub knowledge_operations: Vec<String>,
    pub secret_operations: Vec<String>,
    pub resources: Vec<HostResourceDefinition>,
    pub resource_templates: Vec<HostResourceTemplateDefinition>,
    pub prompts: Vec<HostPromptDefinition>,
    pub can_refresh: bool,
    pub can_checkpoint: bool,
    pub can_export_artifact: bool,
    pub can_prepare_derived_state: bool,
    pub encrypt_available: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostPromptDefinition {
    pub name: String,
    pub requires_dashboard_observation: bool,
    pub dashboard_observation_available: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostResourceDefinition {
    pub uri: String,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostResourceTemplateDefinition {
    pub uri_template: String,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HostResourceReply {
    pub uri: String,
    pub content: HostResourceContent,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
// A short-lived wire reply value moved once into its response; boxing would only add an allocation.
#[allow(clippy::large_enum_variant)]
pub enum HostResourceContent {
    Health(HostHealth),
    Query(HostQueryReply),
    Catalog(HostCatalog),
    Text(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostPromptMessage {
    pub role: String,
    pub text: Option<String>,
    pub resource_uri: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostPromptReply {
    pub name: String,
    pub description: String,
    pub messages: Vec<HostPromptMessage>,
}

#[derive(Serialize, Deserialize)]
#[non_exhaustive]
pub enum HostResponse {
    Status(HostStatus),
    Health(HostHealth),
    Catalog(HostCatalog),
    Resource(HostResourceReply),
    Prompt(HostPromptReply),
    Query(HostQueryReply),
    Edit(serde_json::Value),
    Knowledge(serde_json::Value),
    SecretFindings(SecretFindingsClaim),
    SecretPreview(SecretRemediationPreview),
    SecretApplied(SecretRemediationApplied),
    Refresh(HostRefreshReceipt),
    Checkpoint(HostCheckpointReceipt),
}

impl std::fmt::Debug for HostResponse {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let kind = match self {
            Self::Status(_) => "Status",
            Self::Health(_) => "Health",
            Self::Catalog(_) => "Catalog",
            Self::Resource(_) => "Resource",
            Self::Prompt(_) => "Prompt",
            Self::Query(_) => "Query",
            Self::Edit(_) => "Edit",
            Self::Knowledge(_) => "Knowledge",
            Self::SecretFindings(_) => "SecretFindings",
            Self::SecretPreview(_) => "SecretPreview",
            Self::SecretApplied(_) => "SecretApplied",
            Self::Refresh(_) => "Refresh",
            Self::Checkpoint(_) => "Checkpoint",
        };
        formatter.debug_tuple("HostResponse").field(&kind).finish()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum HostRefusalKind {
    InvalidConfig,
    InvalidRequest,
    RequestTooLarge,
    ResponseTooLarge,
    Denied,
    SourceUnavailable,
    PersistenceUnavailable,
    ResourceLimit,
    Cancelled,
    DeadlineExceeded,
    EngineFailure,
    ServiceContextUnavailable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum HostEffectCertainty {
    Committed,
    Uncertain,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostRecoveryEvidence {
    pub certainty: HostEffectCertainty,
    pub operation_kind: String,
    pub canonical_request_hash: String,
    pub operation_key_digest: String,
    pub room_identity_digest: String,
    pub source_root_digest: String,
    pub source_scope_digest: String,
    pub route: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostRefusal {
    pub kind: HostRefusalKind,
    pub query_kind: Option<QueryRefusalKind>,
    pub semantic_operation: Option<QueryOperationKind>,
    pub operation_identity: Option<String>,
    pub retry: Option<HostRetryAdvice>,
    pub remediation_kind: Option<SecretRemediationRefusalKind>,
    pub edit_kind: Option<String>,
    pub knowledge_kind: Option<String>,
    pub recovery: Option<Box<HostRecoveryEvidence>>,
    /// Redacted finding metadata from the shared source admission gate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub withheld: Option<Box<WithheldMeta>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum HostRetryAdvice {
    Automatic,
    Never,
    OnEvent,
    Operator,
}

impl HostRefusal {
    pub(crate) fn new(kind: HostRefusalKind) -> Self {
        Self {
            kind,
            query_kind: None,
            semantic_operation: None,
            operation_identity: None,
            retry: None,
            remediation_kind: None,
            edit_kind: None,
            knowledge_kind: None,
            recovery: None,
            withheld: None,
        }
    }

    fn from_query(refusal: crate::embed::parity::QueryRefusal) -> Self {
        let kind = match refusal.kind() {
            QueryRefusalKind::Cancelled => HostRefusalKind::Cancelled,
            QueryRefusalKind::DeadlineExceeded => HostRefusalKind::DeadlineExceeded,
            QueryRefusalKind::OutputAdmissionRejected => HostRefusalKind::ResponseTooLarge,
            QueryRefusalKind::InvalidRequest
            | QueryRefusalKind::UnsupportedOption
            | QueryRefusalKind::BudgetTooSmall
            | QueryRefusalKind::StaleHandle
            | QueryRefusalKind::PolicyMismatch => HostRefusalKind::InvalidRequest,
            QueryRefusalKind::ForeignSession => HostRefusalKind::Denied,
            QueryRefusalKind::SessionRequired => HostRefusalKind::EngineFailure,
            _ => HostRefusalKind::SourceUnavailable,
        };
        let retry = match refusal.retry() {
            crate::embed::RetryAdvice::Automatic => HostRetryAdvice::Automatic,
            crate::embed::RetryAdvice::Never => HostRetryAdvice::Never,
            crate::embed::RetryAdvice::OnEvent => HostRetryAdvice::OnEvent,
            crate::embed::RetryAdvice::Operator => HostRetryAdvice::Operator,
        };
        Self {
            kind,
            query_kind: Some(refusal.kind()),
            semantic_operation: Some(refusal.operation()),
            operation_identity: Some(refusal.operation_identity().to_owned()),
            retry: Some(retry),
            remediation_kind: None,
            edit_kind: None,
            knowledge_kind: None,
            recovery: None,
            withheld: refusal.withheld().cloned().map(Box::new),
        }
    }

    fn from_edit(error: EditError) -> Self {
        match error {
            EditError::Source(_) => Self::new(HostRefusalKind::SourceUnavailable),
            EditError::Edit(kind) => {
                use crate::embed::parity::edit::EditErrorKind as Kind;
                let (host_kind, name) = match kind {
                    Kind::WriteUncertain => (HostRefusalKind::EngineFailure, "WriteUncertain"),
                    Kind::ReplayUnavailable => {
                        (HostRefusalKind::PersistenceUnavailable, "ReplayUnavailable")
                    }
                    Kind::WriteAuthorityRefused => {
                        (HostRefusalKind::Denied, "WriteAuthorityRefused")
                    }
                    Kind::Cancelled => (HostRefusalKind::Cancelled, "Cancelled"),
                    Kind::ReplayConflict => (HostRefusalKind::InvalidRequest, "ReplayConflict"),
                    Kind::StaleGeneration => {
                        (HostRefusalKind::SourceUnavailable, "StaleGeneration")
                    }
                    Kind::StaleContent => (HostRefusalKind::SourceUnavailable, "StaleContent"),
                    _ => (HostRefusalKind::InvalidRequest, "EditRefused"),
                };
                let mut refusal = Self::new(host_kind);
                refusal.edit_kind = Some(name.to_owned());
                refusal
            }
        }
    }

    fn from_knowledge(error: WireKnowledgeError) -> Self {
        use crate::embed::parity::knowledge::{
            KnowledgeCurationError as C, KnowledgeReviewError as R,
        };
        let (kind, name) = match error {
            WireKnowledgeError::InvalidKind => (HostRefusalKind::InvalidRequest, "InvalidKind"),
            WireKnowledgeError::Review(R::Source(_))
            | WireKnowledgeError::Curation(C::Source(_)) => {
                (HostRefusalKind::SourceUnavailable, "SourceUnavailable")
            }
            WireKnowledgeError::Review(R::BoundExceeded) => {
                (HostRefusalKind::ResourceLimit, "BoundExceeded")
            }
            WireKnowledgeError::Curation(C::Uncertain) => {
                (HostRefusalKind::EngineFailure, "Uncertain")
            }
            WireKnowledgeError::Curation(C::DurableStateUnavailable) => (
                HostRefusalKind::PersistenceUnavailable,
                "DurableStateUnavailable",
            ),
            WireKnowledgeError::Curation(C::WriteAuthorityRefused) => {
                (HostRefusalKind::Denied, "WriteAuthorityRefused")
            }
            _ => (HostRefusalKind::InvalidRequest, "KnowledgeRefused"),
        };
        let mut refusal = Self::new(kind);
        refusal.knowledge_kind = Some(name.to_owned());
        refusal
    }

    fn from_remediation(error: SecretRemediationError) -> Self {
        match error {
            SecretRemediationError::Source(_) => Self::new(HostRefusalKind::SourceUnavailable),
            SecretRemediationError::Refused(kind) => {
                use SecretRemediationRefusalKind as R;
                let host_kind = match kind {
                    R::Cancelled => HostRefusalKind::Cancelled,
                    R::DeadlineExceeded => HostRefusalKind::DeadlineExceeded,
                    R::ResourceLimit => HostRefusalKind::ResourceLimit,
                    R::ReplayUnavailable => HostRefusalKind::PersistenceUnavailable,
                    R::WriteAuthorityRefused => HostRefusalKind::Denied,
                    R::WriteUncertain => HostRefusalKind::EngineFailure,
                    R::SourceUnavailable
                    | R::ScanIndeterminate
                    | R::StalePublication
                    | R::StaleContent => HostRefusalKind::SourceUnavailable,
                    _ => HostRefusalKind::InvalidRequest,
                };
                let mut refusal = Self::new(host_kind);
                refusal.remediation_kind = Some(kind);
                refusal
            }
        }
    }
}

type SharedHostSources = Arc<Mutex<HashMap<PathBuf, SharedHostSource>>>;
type ActiveRoomIds = Arc<Mutex<HashMap<String, Arc<()>>>>;

/// Service-owned runtime for admitting several independent rooms.
/// This value stays in trusted host code and never crosses the wire.
pub struct HostRuntimeOwner {
    runtime: Arc<ProcessIndexRuntime>,
    open_options: EmbeddedOpenOptions,
    instance_identity: String,
    stopped: Mutex<bool>,
    sources: SharedHostSources,
    active_room_ids: ActiveRoomIds,
}

struct SharedHostSource {
    scope: String,
    source: Arc<EmbeddedSourceHandle>,
    rooms: usize,
    closing: bool,
}

impl HostRuntimeOwner {
    fn reserve_room_id(&self, room_id: &str) -> Result<Arc<()>, HostRefusal> {
        let mut active = self
            .active_room_ids
            .lock()
            .map_err(|_| HostRefusal::new(HostRefusalKind::EngineFailure))?;
        if active.contains_key(room_id) {
            return Err(HostRefusal::new(HostRefusalKind::Denied));
        }
        let token = Arc::new(());
        active.insert(room_id.to_owned(), Arc::clone(&token));
        Ok(token)
    }

    fn release_room_id(&self, room_id: &str, token: &Arc<()>) {
        if let Ok(mut active) = self.active_room_ids.lock()
            && active
                .get(room_id)
                .is_some_and(|current| Arc::ptr_eq(current, token))
        {
            active.remove(room_id);
        }
    }

    /// One runtime may serve multiple independently authorized source rooms.
    pub fn new(runtime: ProcessIndexRuntime) -> Self {
        Self::new_with_options(runtime, EmbeddedOpenOptions::default())
    }

    /// Select one immutable state policy for every source admitted by this owner.
    /// The option is trusted Rust configuration and is never read from JSON.
    pub fn new_with_options(
        runtime: ProcessIndexRuntime,
        open_options: EmbeddedOpenOptions,
    ) -> Self {
        Self {
            runtime: Arc::new(runtime),
            open_options,
            instance_identity: new_instance_identity(),
            stopped: Mutex::new(false),
            sources: Arc::new(Mutex::new(HashMap::new())),
            active_room_ids: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Service-level shutdown closes all rooms admitted through this owner.
    /// Stop routing room traffic before calling it.
    pub fn shutdown(&self, control: &OperationControl) -> Result<HostShutdownReport, HostRefusal> {
        control.check().map_err(map_stop)?;
        let mut stopped = self
            .stopped
            .lock()
            .map_err(|_| HostRefusal::new(HostRefusalKind::EngineFailure))?;
        *stopped = true;
        shutdown_runtime(&self.runtime, control)
    }

    fn admitted_source(
        &self,
        root: &PathBuf,
        scope: &str,
    ) -> Result<Arc<EmbeddedSourceHandle>, HostRefusal> {
        let mut sources = self
            .sources
            .lock()
            .map_err(|_| HostRefusal::new(HostRefusalKind::EngineFailure))?;
        if let Some(entry) = sources.get_mut(root) {
            if entry.scope != scope {
                return Err(HostRefusal::new(HostRefusalKind::Denied));
            }
            if entry.closing || !entry.source.is_open() {
                return Err(HostRefusal::new(HostRefusalKind::SourceUnavailable));
            }
            entry.rooms += 1;
            return Ok(Arc::clone(&entry.source));
        }
        let source = Arc::new(
            self.runtime
                .open_embedded_source_with_options(
                    EmbeddedSourceSpec::current_worktree(root.clone()),
                    self.open_options.clone(),
                )
                .map_err(|_| HostRefusal::new(HostRefusalKind::SourceUnavailable))?,
        );
        sources.insert(
            root.clone(),
            SharedHostSource {
                scope: scope.to_owned(),
                source: Arc::clone(&source),
                rooms: 1,
                closing: false,
            },
        );
        Ok(source)
    }
}

/// A room's non-serializable authority and source lifetime. The host broker
/// maps authenticated room traffic to this value; `HostRequest` does not.
pub struct HostRoom {
    room_id: String,
    source_scope: String,
    source_root: PathBuf,
    project_name: String,
    prompt_context: HostPromptContext,
    engine: HostEngineIdentity,
    rights: HostRights,
    limits: HostLimits,
    runtime: Arc<ProcessIndexRuntime>,
    owns_runtime: bool,
    source: Arc<EmbeddedSourceHandle>,
    closed: AtomicBool,
    owner_sources: Option<SharedHostSources>,
    owner_room_ids: Option<ActiveRoomIds>,
    room_token: Option<Arc<()>>,
    session: Mutex<Option<Arc<QuerySession>>>,
    secret_external_tool: Option<SecretExternalTool>,
}

impl HostRoom {
    fn release_room_id(&self) {
        if let (Some(active), Some(token)) = (&self.owner_room_ids, &self.room_token)
            && let Ok(mut active) = active.lock()
            && active
                .get(&self.room_id)
                .is_some_and(|current| Arc::ptr_eq(current, token))
        {
            active.remove(&self.room_id);
        }
    }

    fn begin_close_shared_if_last(&self) -> Result<Option<SourceCloseReceipt>, HostRefusal> {
        let registry = self
            .owner_sources
            .as_ref()
            .ok_or_else(|| HostRefusal::new(HostRefusalKind::EngineFailure))?;
        {
            let mut sources = registry
                .lock()
                .map_err(|_| HostRefusal::new(HostRefusalKind::EngineFailure))?;
            let entry = sources
                .get_mut(&self.source_root)
                .ok_or_else(|| HostRefusal::new(HostRefusalKind::EngineFailure))?;
            if entry.scope != self.source_scope
                || !Arc::ptr_eq(&entry.source, &self.source)
                || entry.rooms == 0
                || entry.closing
            {
                return Err(HostRefusal::new(HostRefusalKind::EngineFailure));
            }
            entry.rooms -= 1;
            if entry.rooms != 0 {
                return Ok(None);
            }
            entry.closing = true;
        }
        // Keep the Closing entry visible while the source joins. Other roots
        // can open; this root cannot be rebound before the old handle releases.
        let receipt = self.source.begin_close();
        let mut sources = registry
            .lock()
            .map_err(|_| HostRefusal::new(HostRefusalKind::EngineFailure))?;
        if sources.get(&self.source_root).is_some_and(|entry| {
            entry.closing && entry.rooms == 0 && Arc::ptr_eq(&entry.source, &self.source)
        }) {
            sources.remove(&self.source_root);
        }
        Ok(Some(receipt))
    }

    pub fn open(
        runtime: ProcessIndexRuntime,
        grant: HostRoomGrant,
        config: HostRoomConfig,
    ) -> Result<Self, HostRefusal> {
        Self::open_with_prompt_context(runtime, grant, config, HostPromptContext::Unobserved)
    }

    pub fn open_with_prompt_context(
        runtime: ProcessIndexRuntime,
        grant: HostRoomGrant,
        config: HostRoomConfig,
        prompt_context: HostPromptContext,
    ) -> Result<Self, HostRefusal> {
        Self::open_bound(
            Arc::new(runtime),
            true,
            new_instance_identity(),
            grant,
            config,
            prompt_context,
            None,
        )
    }

    /// Admit a room to a service-owned runtime without granting it shutdown.
    pub fn open_in_runtime(
        owner: &HostRuntimeOwner,
        grant: HostRoomGrant,
        config: HostRoomConfig,
    ) -> Result<Self, HostRefusal> {
        Self::open_in_runtime_with_prompt_context(
            owner,
            grant,
            config,
            HostPromptContext::Unobserved,
        )
    }

    pub fn open_in_runtime_with_prompt_context(
        owner: &HostRuntimeOwner,
        grant: HostRoomGrant,
        config: HostRoomConfig,
        prompt_context: HostPromptContext,
    ) -> Result<Self, HostRefusal> {
        let stopped = owner
            .stopped
            .lock()
            .map_err(|_| HostRefusal::new(HostRefusalKind::EngineFailure))?;
        if *stopped {
            return Err(HostRefusal::new(HostRefusalKind::Denied));
        }
        Self::open_bound(
            Arc::clone(&owner.runtime),
            false,
            owner.instance_identity.clone(),
            grant,
            config,
            prompt_context,
            Some(owner),
        )
    }

    fn open_bound(
        runtime: Arc<ProcessIndexRuntime>,
        owns_runtime: bool,
        instance_identity: String,
        grant: HostRoomGrant,
        config: HostRoomConfig,
        prompt_context: HostPromptContext,
        owner: Option<&HostRuntimeOwner>,
    ) -> Result<Self, HostRefusal> {
        config.limits.validate()?;
        if config.room_id != grant.room_id {
            return Err(HostRefusal::new(HostRefusalKind::Denied));
        }
        let root = grant
            .source_root
            .canonicalize()
            .map_err(|_| HostRefusal::new(HostRefusalKind::SourceUnavailable))?;
        let project_name = root
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(&grant.room_id)
            .to_owned();
        let room_token = owner
            .map(|owner| owner.reserve_room_id(&grant.room_id))
            .transpose()?;
        let source = if let Some(owner) = owner {
            match owner.admitted_source(&root, &grant.source_scope) {
                Ok(source) => source,
                Err(refusal) => {
                    if let Some(token) = &room_token {
                        owner.release_room_id(&grant.room_id, token);
                    }
                    return Err(refusal);
                }
            }
        } else {
            Arc::new(
                runtime
                    .open_embedded_source(EmbeddedSourceSpec::current_worktree(root.clone()))
                    .map_err(|_| HostRefusal::new(HostRefusalKind::SourceUnavailable))?,
            )
        };
        let info = crate::embed::engine_info();
        let engine = HostEngineIdentity {
            engine: "SymForge".to_owned(),
            engine_version: info.version.to_owned(),
            api_version: crate::embed::parity::API_VERSION,
            snapshot_format_version: info.snapshot_format_version,
            secret_policy_version: info.secret_policy_version,
            grammars: info
                .grammars
                .iter()
                .map(|grammar| (*grammar).to_owned())
                .collect(),
            instance_identity,
        };
        Ok(Self {
            room_id: grant.room_id,
            source_scope: grant.source_scope,
            source_root: root,
            project_name,
            prompt_context,
            engine,
            rights: grant.rights,
            limits: config.limits,
            runtime,
            owns_runtime,
            source,
            closed: AtomicBool::new(false),
            owner_sources: owner.map(|owner| Arc::clone(&owner.sources)),
            owner_room_ids: owner.map(|owner| Arc::clone(&owner.active_room_ids)),
            room_token,
            session: Mutex::new(None),
            secret_external_tool: grant.secret_external_tool,
        })
    }

    fn query_session(&self) -> Result<Arc<QuerySession>, HostRefusal> {
        let mut session = self
            .session
            .lock()
            .map_err(|_| HostRefusal::new(HostRefusalKind::EngineFailure))?;
        if session.is_none() {
            *session = Some(Arc::new(
                self.source
                    .new_query_session_with_limits(SessionCacheLimits {
                        max_bytes: self.limits.max_session_cache_bytes,
                        max_entries: self.limits.max_session_cache_entries,
                    })
                    .map_err(|_| HostRefusal::new(HostRefusalKind::SourceUnavailable))?,
            ));
        }
        Ok(Arc::clone(session.as_ref().expect("session initialized")))
    }

    /// Trusted host reset for one logical context; existing retrieval handles expire.
    pub fn reset_query_session(&self) -> Result<(), HostRefusal> {
        if !self.rights.query {
            return Err(HostRefusal::new(HostRefusalKind::Denied));
        }
        if let Some(session) = self
            .session
            .lock()
            .map_err(|_| HostRefusal::new(HostRefusalKind::EngineFailure))?
            .as_ref()
        {
            session.reset();
        }
        Ok(())
    }

    fn replay_scope(&self) -> String {
        let bytes = serde_json::to_vec(&(&self.room_id, &self.source_scope))
            .expect("room identity serializes");
        format!("host-room-v1:{}", crate::hash::digest_hex(&bytes))
    }

    fn secret_scan_limits(&self, requested: SecretScanLimits) -> SecretScanLimits {
        SecretScanLimits {
            max_files: requested.max_files.min(self.limits.max_secret_scan_files),
            max_findings: requested.max_findings.min(self.limits.max_secret_findings),
            max_total_bytes: requested
                .max_total_bytes
                .min(self.limits.max_secret_scan_bytes),
        }
    }

    fn recovery_evidence(
        &self,
        request: &HostRequest,
        operation_key: &str,
        certainty: HostEffectCertainty,
    ) -> Box<HostRecoveryEvidence> {
        let request_bytes = serde_json::to_vec(request).expect("typed request serializes");
        let route = match certainty {
            HostEffectCertainty::Committed => {
                "reissue_exact_request_same_key_with_larger_host_frame_limit"
            }
            HostEffectCertainty::Uncertain => "inspect_and_reconcile_durable_replay_before_retry",
        };
        Box::new(HostRecoveryEvidence {
            certainty,
            operation_kind: match request {
                HostRequest::Edit { .. } => "edit",
                HostRequest::Knowledge { .. } => "knowledge_curation",
                HostRequest::ApplySecretRemediation { .. } => "secret_remediation",
                _ => "unknown",
            }
            .to_owned(),
            canonical_request_hash: crate::hash::digest_hex(&request_bytes),
            operation_key_digest: crate::hash::digest_hex(operation_key.as_bytes()),
            room_identity_digest: crate::hash::digest_hex(self.room_id.as_bytes()),
            source_root_digest: crate::hash::digest_hex(
                self.source_root.to_string_lossy().as_bytes(),
            ),
            source_scope_digest: crate::hash::digest_hex(self.source_scope.as_bytes()),
            route: route.to_owned(),
        })
    }

    pub fn room_id(&self) -> &str {
        &self.room_id
    }
    pub fn source_scope(&self) -> &str {
        &self.source_scope
    }
    pub fn engine_identity(&self) -> &HostEngineIdentity {
        &self.engine
    }
    pub fn catalog(&self) -> HostCatalog {
        HostCatalog {
            engine: self.engine.clone(),
            limits: self.limits,
            query_operations: if self.rights.query {
                [
                    "File",
                    "FileContext",
                    "RepoMap",
                    "Symbol",
                    "InspectMatch",
                    "Context",
                    "References",
                    "Dependents",
                    "SearchSymbols",
                    "SearchText",
                    "SearchFiles",
                    "SearchKnowledge",
                    "Graph",
                    "Syntax",
                    "Diff",
                    "Impact",
                    "Explore",
                    "Conventions",
                    "ContextInventory",
                    "InvestigationSuggest",
                    "Retrieve",
                ]
                .into_iter()
                .map(str::to_owned)
                .collect()
            } else {
                Vec::new()
            },
            edit_operations: if self.rights.edit {
                [
                    "plan",
                    "preview",
                    "apply",
                    "batch_edit",
                    "batch_insert",
                    "batch_rename",
                ]
                .into_iter()
                .map(str::to_owned)
                .collect()
            } else {
                Vec::new()
            },
            knowledge_operations: if self.rights.curate_knowledge {
                vec![
                    "review".to_owned(),
                    "curate_preview".to_owned(),
                    "curate_apply".to_owned(),
                ]
            } else if self.rights.query {
                vec!["review".to_owned()]
            } else {
                Vec::new()
            },
            secret_operations: if self.rights.remediate_secrets {
                vec![
                    "inspect".to_owned(),
                    "preview".to_owned(),
                    "apply".to_owned(),
                ]
            } else if self.rights.inspect_secrets {
                vec!["inspect".to_owned()]
            } else {
                Vec::new()
            },
            resources: [
                ("symforge://repo/health", "repo-health"),
                ("symforge://repo/outline", "repo-outline"),
                ("symforge://repo/map", "repo-map"),
                (
                    "symforge://repo/changes/uncommitted",
                    "repo-changes-uncommitted",
                ),
                ("symforge://tools/catalog", "tools-catalog"),
                ("symforge://glossary", "glossary"),
            ]
            .into_iter()
            .filter(|(_, name)| {
                self.rights.query
                    || !matches!(
                        *name,
                        "repo-outline" | "repo-map" | "repo-changes-uncommitted"
                    )
            })
            .map(|(uri, name)| HostResourceDefinition {
                uri: uri.into(),
                name: name.into(),
            })
            .collect(),
            resource_templates: if self.rights.query {
                [
                    ("symforge://file/context?path={path}&max_tokens={max_tokens}", "file-context"),
                    ("symforge://file/content?path={path}&start_line={start_line}&end_line={end_line}&around_line={around_line}&around_match={around_match}&match_occurrence={match_occurrence}&context_lines={context_lines}&show_line_numbers={show_line_numbers}&header={header}", "file-content"),
                    ("symforge://symbol/detail?path={path}&name={name}&kind={kind}", "symbol-detail"),
                    ("symforge://symbol/context?name={name}&file={file}", "symbol-context"),
                ]
                .into_iter()
                .map(|(uri_template, name)| HostResourceTemplateDefinition {
                    uri_template: uri_template.into(),
                    name: name.into(),
                })
                .collect()
            } else {
                Vec::new()
            },
            prompts: [
                "symforge-admin",
                "symforge-review",
                "symforge-architecture",
                "symforge-triage",
                "symforge-onboard",
                "symforge-refactor",
                "symforge-debug",
                "symforge-knowledge-hygiene",
            ]
            .into_iter()
            .map(|name| HostPromptDefinition {
                name: name.to_owned(),
                requires_dashboard_observation: name == "symforge-admin",
                dashboard_observation_available: name != "symforge-admin"
                    || !matches!(&self.prompt_context, HostPromptContext::Unobserved),
            })
            .collect(),
            can_refresh: self.rights.refresh,
            can_checkpoint: self.rights.checkpoint,
            can_export_artifact: self.rights.export_artifact,
            can_prepare_derived_state: self.rights.derived_state_prepare,
            encrypt_available: self.rights.remediate_secrets && self.secret_external_tool.is_some(),
        }
    }

    fn query_resource(
        &self,
        uri: &str,
        request: QueryRequest,
        control: &OperationControl,
    ) -> Result<HostResourceReply, HostRefusal> {
        if !self.rights.query {
            return Err(HostRefusal::new(HostRefusalKind::Denied));
        }
        let limits = QueryLimits {
            max_results: QueryLimits::default()
                .max_results
                .min(self.limits.max_query_results),
            max_bytes: QueryLimits::default()
                .max_bytes
                .min(self.limits.max_query_bytes),
        };
        let session = self.query_session()?;
        let mut admitted = None;
        self.source
            .query_with_policy_admitted(
                &request,
                limits,
                QueryPolicy {
                    allow_derived_state_preparation: self.rights.derived_state_prepare,
                },
                Some(&session),
                Some(control),
                |claim| {
                    let reply = HostResourceReply {
                        uri: uri.to_owned(),
                        content: HostResourceContent::Query(HostQueryReply::from_claim(
                            claim, limits,
                        )),
                    };
                    let fits = serde_json::to_vec(&HostResponse::Resource(reply.clone()))
                        .is_ok_and(|bytes| bytes.len() <= self.limits.max_response_bytes as usize);
                    if fits {
                        admitted = Some(reply);
                    }
                    fits
                },
            )
            .map_err(HostRefusal::from_query)?;
        Ok(admitted.expect("admitted resource response was encoded before session commit"))
    }

    fn resource(
        &self,
        resource: &HostResourceRequest,
        control: &OperationControl,
    ) -> Result<HostResourceReply, HostRefusal> {
        let (uri, content) = match resource {
            HostResourceRequest::RepoHealth => ("symforge://repo/health",
                HostResourceContent::Health(crate::embed::lifecycle::embed_host::health(
                    &self.source, &self.room_id, &self.engine))),
            HostResourceRequest::RepoOutline => {
                return self.query_resource(
                    "symforge://repo/outline",
                    QueryRequest::RepoMap(crate::embed::parity::read_context::RepoMapRequest {
                        detail: Some("full".to_owned()),
                        ..Default::default()
                    }),
                    control,
                );
            }
            HostResourceRequest::RepoMap => {
                return self.query_resource(
                    "symforge://repo/map",
                    QueryRequest::RepoMap(Default::default()),
                    control,
                );
            }
            HostResourceRequest::RepoChangesUncommitted => {
                // Same request the MCP resource sends: every option unset, which
                // resolves to the uncommitted mode when a repository root exists.
                return self.query_resource(
                    "symforge://repo/changes/uncommitted",
                    QueryRequest::WhatChanged(Default::default()),
                    control,
                );
            }
            HostResourceRequest::FileContext { path, max_tokens } => {
                return self.query_resource(
                    "symforge://file/context",
                    QueryRequest::FileContext(crate::embed::parity::read_context::FileContextRequest {
                        path: path.clone(),
                        max_tokens: *max_tokens,
                        ..Default::default()
                    }),
                    control,
                );
            }
            HostResourceRequest::FileContent { path, start_line, end_line } => {
                return self.query_resource(
                    "symforge://file/content",
                    QueryRequest::FileContent(crate::embed::parity::read::FileContentRequest {
                        path: path.clone(),
                        start_line: *start_line,
                        end_line: *end_line,
                        ..Default::default()
                    }),
                    control,
                );
            }
            HostResourceRequest::FileContentOptions(options) => {
                return self.query_resource("symforge://file/content", QueryRequest::FileContent(options.clone()), control);
            }
            HostResourceRequest::SymbolDetail { selector } => {
                return self.query_resource(
                    "symforge://symbol/detail",
                    QueryRequest::SymbolRead(crate::embed::parity::symbol::SymbolReadRequest {
                        path: selector.path.clone(),
                        name: selector.name.clone(),
                        kind: selector.kind.clone(),
                        symbol_line: selector.line,
                        ..Default::default()
                    }),
                    control,
                );
            }
            HostResourceRequest::SymbolDetailOptions(options) => {
                return self.query_resource("symforge://symbol/detail", QueryRequest::SymbolRead(options.clone()), control);
            }
            HostResourceRequest::SymbolContext { selector } => {
                return self.query_resource(
                    "symforge://symbol/context",
                    QueryRequest::SymbolContext(crate::embed::parity::symbol_context::SymbolContextRequest {
                        name: selector.name.clone(),
                        file: Some(selector.path.clone()),
                        symbol_kind: selector.kind.clone(),
                        symbol_line: selector.line,
                        ..Default::default()
                    }),
                    control,
                );
            }
            HostResourceRequest::SymbolContextOptions(options) => {
                return self.query_resource("symforge://symbol/context", QueryRequest::SymbolContext(options.clone()), control);
            }
            HostResourceRequest::ToolsCatalog => ("symforge://tools/catalog",
                HostResourceContent::Catalog(self.catalog())),
            HostResourceRequest::Glossary => ("symforge://glossary",
                HostResourceContent::Text("SymForge indexes an admitted source publication. A room's source proof identifies the exact publication examined; query limits and partial/withheld counts describe the returned projection. Checkpoint persists local recovery state. Replay requires the same request hash and verified post-image before a cached mutation result is reusable.".to_owned())),
        };
        Ok(HostResourceReply {
            uri: uri.to_owned(),
            content,
        })
    }
    pub fn limits(&self) -> HostLimits {
        self.limits
    }

    pub fn control(&self) -> Result<OperationControl, HostRefusal> {
        OperationControl::new(Duration::from_millis(self.limits.max_duration_ms))
    }

    /// The trusted broker closes a room after it has stopped routing requests.
    /// A client request cannot choose when another room closes.
    pub fn close(&self, control: &OperationControl) -> Result<HostCloseReport, HostRefusal> {
        control.check().map_err(map_stop)?;
        if !self.owns_runtime {
            let already_terminal = self.closed.swap(true, Ordering::AcqRel);
            if let Some(session) = self
                .session
                .lock()
                .map_err(|_| HostRefusal::new(HostRefusalKind::EngineFailure))?
                .as_ref()
            {
                session.reset();
            }
            if already_terminal {
                return Ok(HostCloseReport {
                    already_terminal: true,
                    terminal_source_version: self.source.runtime_view().source_version,
                });
            }
            self.release_room_id();
            if let Some(receipt) = self.begin_close_shared_if_last()? {
                let report = receipt
                    .wait(Instant::now() + control.remaining())
                    .map_err(|_| HostRefusal::new(HostRefusalKind::EngineFailure))?;
                return Ok(HostCloseReport {
                    already_terminal: report.already_terminal,
                    terminal_source_version: report.terminal_source_version,
                });
            }
            return Ok(HostCloseReport {
                already_terminal: false,
                terminal_source_version: self.source.runtime_view().source_version,
            });
        }
        self.closed.store(true, Ordering::Release);
        let receipt = self.source.begin_close();
        let report = receipt
            .wait(Instant::now() + control.remaining())
            .map_err(|_| HostRefusal::new(HostRefusalKind::EngineFailure))?;
        Ok(HostCloseReport {
            already_terminal: report.already_terminal,
            terminal_source_version: report.terminal_source_version,
        })
    }

    /// Trusted host teardown for the runtime owned by this room.
    pub fn shutdown(&self, control: &OperationControl) -> Result<HostShutdownReport, HostRefusal> {
        if !self.owns_runtime {
            return Err(HostRefusal::new(HostRefusalKind::Denied));
        }
        shutdown_runtime(&self.runtime, control)
    }

    /// Dispatch only after the broker has selected this opaque room capability.
    /// The byte limit is checked before JSON parsing or allocating DTO fields.
    pub fn dispatch_wire(
        &self,
        request_json: &[u8],
        control: &OperationControl,
    ) -> Result<Vec<u8>, HostRefusal> {
        if request_json.len() > self.limits.max_request_bytes as usize {
            return Err(HostRefusal::new(HostRefusalKind::RequestTooLarge));
        }
        let request: HostRequest = serde_json::from_slice(request_json)
            .map_err(|_| HostRefusal::new(HostRefusalKind::InvalidRequest))?;
        let response = self.dispatch(&request, control)?;
        serde_json::to_vec(&response).map_err(|_| HostRefusal::new(HostRefusalKind::EngineFailure))
    }

    pub fn dispatch(
        &self,
        request: &HostRequest,
        control: &OperationControl,
    ) -> Result<HostResponse, HostRefusal> {
        control.check().map_err(map_stop)?;
        if self.closed.load(Ordering::Acquire) {
            return Err(HostRefusal::new(HostRefusalKind::SourceUnavailable));
        }
        if control.remaining() > Duration::from_millis(self.limits.max_duration_ms) {
            return Err(HostRefusal::new(HostRefusalKind::InvalidConfig));
        }
        let request_bytes = serde_json::to_vec(request)
            .map_err(|_| HostRefusal::new(HostRefusalKind::InvalidRequest))?;
        if request_bytes.len() > self.limits.max_request_bytes as usize {
            return Err(HostRefusal::new(HostRefusalKind::RequestTooLarge));
        }
        let response = match request {
            HostRequest::Status => {
                HostResponse::Status(crate::embed::lifecycle::embed_host::status(
                    &self.source,
                    &self.room_id,
                    &self.engine,
                ))
            }
            HostRequest::Health => {
                HostResponse::Health(crate::embed::lifecycle::embed_host::health(
                    &self.source,
                    &self.room_id,
                    &self.engine,
                ))
            }
            HostRequest::Catalog => HostResponse::Catalog(self.catalog()),
            HostRequest::Resource(resource) => {
                HostResponse::Resource(self.resource(resource, control)?)
            }
            HostRequest::Prompt(prompt) => HostResponse::Prompt(render_prompt(
                prompt,
                &self.project_name,
                &self.prompt_context,
            )?),
            HostRequest::Query { request, limits } => {
                if !self.rights.query {
                    return Err(HostRefusal::new(HostRefusalKind::Denied));
                }
                let limits = QueryLimits {
                    max_results: limits.max_results.min(self.limits.max_query_results),
                    max_bytes: limits.max_bytes.min(self.limits.max_query_bytes),
                };
                let session = self.query_session()?;
                let mut admitted = None;
                self.source
                    .query_with_policy_admitted(
                        request,
                        limits,
                        QueryPolicy {
                            allow_derived_state_preparation: self.rights.derived_state_prepare,
                        },
                        Some(&session),
                        Some(control),
                        |claim| {
                            let response =
                                HostResponse::Query(HostQueryReply::from_claim(claim, limits));
                            let fits = serde_json::to_vec(&response).is_ok_and(|bytes| {
                                bytes.len() <= self.limits.max_response_bytes as usize
                            });
                            if fits {
                                admitted = Some(response);
                            }
                            fits
                        },
                    )
                    .map_err(HostRefusal::from_query)?;
                admitted.expect("admitted query response was encoded before session commit")
            }
            HostRequest::Edit {
                request: edit,
                operation_key,
            } => {
                if !self.rights.edit {
                    return Err(HostRefusal::new(HostRefusalKind::Denied));
                }
                let reply = if edit.is_apply() {
                    let key = operation_key
                        .as_deref()
                        .filter(|key| !key.trim().is_empty())
                        .ok_or_else(|| HostRefusal::new(HostRefusalKind::InvalidRequest))?;
                    let authority = EditApplyAuthority::for_source_root(
                        self.source_root.clone(),
                        self.replay_scope(),
                        Arc::clone(&control.cancelled),
                    )
                    .map_err(|kind| HostRefusal::from_edit(EditError::Edit(kind)))?;
                    self.source
                        .apply_wire_edit(edit, &authority, key)
                        .map_err(|error| {
                            let mut refusal = HostRefusal::from_edit(error);
                            if refusal.edit_kind.as_deref() == Some("WriteUncertain") {
                                refusal.recovery = Some(self.recovery_evidence(
                                    request,
                                    key,
                                    HostEffectCertainty::Uncertain,
                                ));
                            }
                            refusal
                        })?
                } else {
                    self.source
                        .preview_wire_edit(edit)
                        .map_err(HostRefusal::from_edit)?
                };
                HostResponse::Edit(serde_json::to_value(reply).map_err(|_| {
                    let mut refusal = HostRefusal::new(HostRefusalKind::EngineFailure);
                    if let Some(key) = operation_key.as_deref().filter(|_| edit.is_apply()) {
                        refusal.recovery = Some(self.recovery_evidence(
                            request,
                            key,
                            HostEffectCertainty::Committed,
                        ));
                    }
                    refusal
                })?)
            }
            HostRequest::Knowledge {
                request: knowledge,
                operation_key,
            } => {
                let is_review = matches!(knowledge, WireKnowledgeRequest::Review { .. });
                if !(is_review && self.rights.query || !is_review && self.rights.curate_knowledge) {
                    return Err(HostRefusal::new(HostRefusalKind::Denied));
                }
                let reply = if knowledge.is_apply() {
                    let key = operation_key
                        .as_deref()
                        .filter(|key| !key.trim().is_empty())
                        .ok_or_else(|| HostRefusal::new(HostRefusalKind::InvalidRequest))?;
                    let authority = EditApplyAuthority::for_source_root(
                        self.source_root.clone(),
                        self.replay_scope(),
                        Arc::clone(&control.cancelled),
                    )
                    .map_err(|kind| HostRefusal::from_edit(EditError::Edit(kind)))?;
                    self.source
                        .apply_wire_knowledge(knowledge, &authority, key)
                        .map_err(|error| {
                            let mut refusal = HostRefusal::from_knowledge(error);
                            if refusal.knowledge_kind.as_deref() == Some("Uncertain") {
                                refusal.recovery = Some(self.recovery_evidence(
                                    request,
                                    key,
                                    HostEffectCertainty::Uncertain,
                                ));
                            }
                            refusal
                        })?
                } else {
                    self.source
                        .preview_wire_knowledge(knowledge)
                        .map_err(HostRefusal::from_knowledge)?
                };
                HostResponse::Knowledge(serde_json::to_value(reply).map_err(|_| {
                    let mut refusal = HostRefusal::new(HostRefusalKind::EngineFailure);
                    if let Some(key) = operation_key.as_deref().filter(|_| knowledge.is_apply()) {
                        refusal.recovery = Some(self.recovery_evidence(
                            request,
                            key,
                            HostEffectCertainty::Committed,
                        ));
                    }
                    refusal
                })?)
            }
            HostRequest::InspectSecrets { scope, limits } => {
                if !self.rights.inspect_secrets {
                    return Err(HostRefusal::new(HostRefusalKind::Denied));
                }
                HostResponse::SecretFindings(
                    self.source
                        .inspect_secret_findings(
                            scope,
                            self.secret_scan_limits(*limits),
                            Some(control),
                            self.secret_external_tool.as_ref(),
                        )
                        .map_err(HostRefusal::from_remediation)?,
                )
            }
            HostRequest::PreviewSecretRemediation {
                request: secret,
                limits,
            } => {
                if !self.rights.remediate_secrets {
                    return Err(HostRefusal::new(HostRefusalKind::Denied));
                }
                HostResponse::SecretPreview(
                    self.source
                        .preview_secret_remediation(
                            secret,
                            self.secret_scan_limits(*limits),
                            Some(control),
                        )
                        .map_err(HostRefusal::from_remediation)?,
                )
            }
            HostRequest::ApplySecretRemediation {
                preview,
                operation_key,
            } => {
                if !self.rights.remediate_secrets || operation_key.trim().is_empty() {
                    return Err(HostRefusal::new(if self.rights.remediate_secrets {
                        HostRefusalKind::InvalidRequest
                    } else {
                        HostRefusalKind::Denied
                    }));
                }
                let authority = SecretApplyAuthority::for_source_root(
                    self.source_root.clone(),
                    self.replay_scope(),
                    Arc::clone(&control.cancelled),
                    self.secret_scan_limits(SecretScanLimits {
                        max_files: self.limits.max_secret_scan_files,
                        max_findings: self.limits.max_secret_findings,
                        max_total_bytes: self.limits.max_secret_scan_bytes,
                    }),
                    Some(control.clone()),
                    self.secret_external_tool.clone(),
                )
                .map_err(|_| HostRefusal::new(HostRefusalKind::InvalidConfig))?;
                let applied = self
                    .source
                    .apply_secret_remediation(preview, &authority, operation_key)
                    .map_err(|error| {
                        let mut refusal = HostRefusal::from_remediation(error);
                        if refusal.remediation_kind
                            == Some(SecretRemediationRefusalKind::WriteUncertain)
                        {
                            refusal.recovery = Some(self.recovery_evidence(
                                request,
                                operation_key,
                                HostEffectCertainty::Uncertain,
                            ));
                        }
                        refusal
                    })?;
                HostResponse::SecretApplied(applied)
            }
            HostRequest::Refresh => {
                if !self.rights.refresh {
                    return Err(HostRefusal::new(HostRefusalKind::Denied));
                }
                let ticket = self
                    .source
                    .request_refresh()
                    .map_err(|_| HostRefusal::new(HostRefusalKind::SourceUnavailable))?;
                HostResponse::Refresh(HostRefreshReceipt {
                    ticket_identity: ticket.ticket_identity().to_owned(),
                    requested_source_version: ticket.requested_source_version(),
                })
            }
            HostRequest::Checkpoint {
                verify_after_write,
                export_artifact,
            } => {
                if !self.rights.checkpoint || (*export_artifact && !self.rights.export_artifact) {
                    return Err(HostRefusal::new(HostRefusalKind::Denied));
                }
                HostResponse::Checkpoint(crate::embed::lifecycle::embed_host::checkpoint(
                    &self.source,
                    self.limits,
                    *verify_after_write,
                    *export_artifact,
                    control,
                )?)
            }
        };
        let response_bytes = serde_json::to_vec(&response)
            .map_err(|_| HostRefusal::new(HostRefusalKind::EngineFailure))?;
        if response_bytes.len() > self.limits.max_response_bytes as usize {
            let mut refusal = HostRefusal::new(HostRefusalKind::ResponseTooLarge);
            let committed_key = match request {
                HostRequest::Edit {
                    request,
                    operation_key,
                } if request.is_apply() => operation_key.as_deref(),
                HostRequest::Knowledge {
                    request,
                    operation_key,
                } if request.is_apply() => operation_key.as_deref(),
                HostRequest::ApplySecretRemediation { operation_key, .. } => {
                    Some(operation_key.as_str())
                }
                _ => None,
            };
            if let Some(key) = committed_key {
                refusal.recovery =
                    Some(self.recovery_evidence(request, key, HostEffectCertainty::Committed));
            }
            return Err(refusal);
        }
        Ok(response)
    }
}

impl Drop for HostRoom {
    fn drop(&mut self) {
        if !self.owns_runtime && !self.closed.swap(true, Ordering::AcqRel) {
            let _ = self.begin_close_shared_if_last();
        }
        self.release_room_id();
    }
}

fn shutdown_runtime(
    runtime: &ProcessIndexRuntime,
    control: &OperationControl,
) -> Result<HostShutdownReport, HostRefusal> {
    control.check().map_err(map_stop)?;
    let receipt = runtime.begin_shutdown();
    let report = receipt
        .wait(Instant::now() + control.remaining())
        .map_err(|_| HostRefusal::new(HostRefusalKind::EngineFailure))?;
    Ok(HostShutdownReport {
        closed_sources: report.closed_sources,
        joined_workers: report.joined_workers,
    })
}

#[cfg(test)]
mod resource_contract_tests {
    use super::*;

    #[test]
    fn read_room_catalog_exposes_shipped_static_and_template_resources() {
        let repository = tempfile::tempdir().unwrap();
        git2::Repository::init(repository.path()).unwrap();
        std::fs::create_dir(repository.path().join("src")).unwrap();
        std::fs::write(repository.path().join("src/a.rs"), b"pub fn a() {}\n").unwrap();
        let grant = HostRoomGrant::new(
            "resource-room".into(),
            repository.path().to_path_buf(),
            "resource-source".into(),
            HostRights::read_only(),
        )
        .unwrap();
        let room = HostRoom::open(
            ProcessIndexRuntime::acquire().unwrap(),
            grant,
            HostRoomConfig {
                room_id: "resource-room".into(),
                limits: HostLimits::default(),
            },
        )
        .unwrap();
        let HostResponse::Catalog(catalog) = room
            .dispatch(&HostRequest::Catalog, &room.control().unwrap())
            .unwrap()
        else {
            panic!("catalog response");
        };
        let uris = catalog
            .resources
            .iter()
            .map(|resource| resource.uri.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(
            uris,
            std::collections::BTreeSet::from([
                "symforge://repo/health",
                "symforge://repo/outline",
                "symforge://repo/map",
                "symforge://repo/changes/uncommitted",
                "symforge://tools/catalog",
                "symforge://glossary",
            ])
        );
        let templates = catalog
            .resource_templates
            .iter()
            .map(|resource| resource.uri_template.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(
            templates,
            std::collections::BTreeSet::from([
                "symforge://file/context?path={path}&max_tokens={max_tokens}",
                "symforge://file/content?path={path}&start_line={start_line}&end_line={end_line}&around_line={around_line}&around_match={around_match}&match_occurrence={match_occurrence}&context_lines={context_lines}&show_line_numbers={show_line_numbers}&header={header}",
                "symforge://symbol/detail?path={path}&name={name}&kind={kind}",
                "symforge://symbol/context?name={name}&file={file}",
            ])
        );

        let control = room.control().unwrap();
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let HostResponse::Status(status) =
                room.dispatch(&HostRequest::Status, &control).unwrap()
            else {
                panic!("status response");
            };
            if status.phase == HostPhase::Current {
                break;
            }
            assert!(Instant::now() < deadline, "source did not publish");
            std::thread::sleep(Duration::from_millis(10));
        }
        for (request, uri, operation) in [
            (
                HostResourceRequest::RepoOutline,
                "symforge://repo/outline",
                QueryOperationKind::RepoMap,
            ),
            (
                HostResourceRequest::RepoMap,
                "symforge://repo/map",
                QueryOperationKind::RepoMap,
            ),
            (
                HostResourceRequest::RepoChangesUncommitted,
                "symforge://repo/changes/uncommitted",
                QueryOperationKind::WhatChanged,
            ),
            (
                HostResourceRequest::FileContext {
                    path: "src/a.rs".into(),
                    max_tokens: None,
                },
                "symforge://file/context",
                QueryOperationKind::FileContext,
            ),
            (
                HostResourceRequest::FileContentOptions(
                    crate::embed::parity::read::FileContentRequest {
                        path: "src/a.rs".into(),
                        ..Default::default()
                    },
                ),
                "symforge://file/content",
                QueryOperationKind::File,
            ),
            (
                HostResourceRequest::SymbolDetailOptions(
                    crate::embed::parity::symbol::SymbolReadRequest {
                        path: "src/a.rs".into(),
                        name: "a".into(),
                        ..Default::default()
                    },
                ),
                "symforge://symbol/detail",
                QueryOperationKind::Symbol,
            ),
            (
                HostResourceRequest::SymbolContextOptions(
                    crate::embed::parity::symbol_context::SymbolContextRequest {
                        name: "a".into(),
                        file: Some("src/a.rs".into()),
                        ..Default::default()
                    },
                ),
                "symforge://symbol/context",
                QueryOperationKind::Context,
            ),
        ] {
            let HostResponse::Resource(reply) = room
                .dispatch(&HostRequest::Resource(request), &control)
                .unwrap()
            else {
                panic!("resource response");
            };
            assert_eq!(reply.uri, uri);
            let HostResourceContent::Query(query) = reply.content else {
                panic!("query-backed resource");
            };
            assert_eq!(query.semantic_operation, operation);
            assert!(!query.publication_identity.is_empty());
        }
    }
}

#[cfg(test)]
mod frame_admission_tests {
    use super::*;
    use crate::embed::parity::read::FileContentRequest;

    #[test]
    fn oversized_read_frame_does_not_commit_room_session_history() {
        let repository = tempfile::tempdir().unwrap();
        git2::Repository::init(repository.path()).unwrap();
        std::fs::create_dir(repository.path().join("docs")).unwrap();
        let escaped = "é\\\"\t".repeat(1_200);
        std::fs::write(repository.path().join("docs/escaped.txt"), escaped).unwrap();
        let mut limits = HostLimits::default();
        limits.max_response_bytes = 4_096;
        limits.max_query_bytes = 16_384;
        let grant = HostRoomGrant::new(
            "frame-room".into(),
            repository.path().to_path_buf(),
            "frame-source".into(),
            HostRights::read_only(),
        )
        .unwrap();
        let room = HostRoom::open(
            ProcessIndexRuntime::acquire().unwrap(),
            grant,
            HostRoomConfig {
                room_id: "frame-room".into(),
                limits,
            },
        )
        .unwrap();
        let control = room.control().unwrap();
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let HostResponse::Status(status) =
                room.dispatch(&HostRequest::Status, &control).unwrap()
            else {
                panic!("status response");
            };
            if status.phase == HostPhase::Current {
                break;
            }
            assert!(Instant::now() < deadline, "source did not publish");
            std::thread::sleep(Duration::from_millis(10));
        }
        let session = room.query_session().unwrap();
        let revision_before = session.revision();
        let cache_before = session.cache_status();
        let refusal = room
            .dispatch(
                &HostRequest::Query {
                    request: QueryRequest::FileContent(FileContentRequest {
                        path: "docs/escaped.txt".into(),
                        ..Default::default()
                    }),
                    limits: QueryLimits {
                        max_results: 1_000,
                        max_bytes: 16_384,
                    },
                },
                &control,
            )
            .expect_err("escaped frame must exceed the exact wire cap");
        assert_eq!(refusal.kind, HostRefusalKind::ResponseTooLarge);
        assert_eq!(session.revision(), revision_before);
        assert_eq!(session.cache_status(), cache_before);

        let HostResponse::Query(inventory) = room
            .dispatch(
                &HostRequest::Query {
                    request: QueryRequest::ContextInventory,
                    limits: QueryLimits::default(),
                },
                &control,
            )
            .unwrap()
        else {
            panic!("inventory response");
        };
        let QueryOutput::ContextInventory(inventory) = inventory.output else {
            panic!("inventory output");
        };
        assert!(
            inventory
                .fetched_files
                .iter()
                .all(|file| file.path != "docs/escaped.txt")
        );
    }

    #[test]
    fn oversized_resource_wrapper_does_not_commit_room_session_history() {
        let repository = tempfile::tempdir().unwrap();
        git2::Repository::init(repository.path()).unwrap();
        std::fs::create_dir(repository.path().join("docs")).unwrap();
        let file = repository.path().join("docs/clip.txt");
        let mut content_bytes = 2_500usize;
        std::fs::write(&file, "a".repeat(content_bytes)).unwrap();
        let mut room_limits = HostLimits::default();
        room_limits.max_response_bytes = 16_384;
        room_limits.max_query_bytes = 16_384;
        let mut room = HostRoom::open(
            ProcessIndexRuntime::acquire().unwrap(),
            HostRoomGrant::new(
                "resource-frame-room".into(),
                repository.path().to_path_buf(),
                "resource-frame-source".into(),
                HostRights::read_only(),
            )
            .unwrap(),
            HostRoomConfig {
                room_id: "resource-frame-room".into(),
                limits: room_limits,
            },
        )
        .unwrap();
        let control = room.control().unwrap();
        let current_version = |room: &HostRoom| {
            let deadline = Instant::now() + Duration::from_secs(20);
            loop {
                let HostResponse::Status(status) =
                    room.dispatch(&HostRequest::Status, &control).unwrap()
                else {
                    panic!("status response");
                };
                if status.phase == HostPhase::Current {
                    return status.source_version;
                }
                assert!(Instant::now() < deadline, "source did not publish");
                std::thread::sleep(Duration::from_millis(10));
            }
        };
        current_version(&room);
        let session = room.query_session().unwrap();
        // The exact request the FileContent resource below dispatches, so the
        // calibrated frame sizes are the ones the cap is applied to.
        let query = QueryRequest::FileContent(crate::embed::parity::read::FileContentRequest {
            path: "docs/clip.txt".into(),
            start_line: None,
            end_line: None,
            ..Default::default()
        });
        let limits = QueryLimits {
            max_results: QueryLimits::default()
                .max_results
                .min(room_limits.max_query_results),
            max_bytes: QueryLimits::default()
                .max_bytes
                .min(room_limits.max_query_bytes),
        };
        let measure = || {
            let mut sizes = None;
            let _ = room.source.query_with_policy_admitted(
                &query,
                limits,
                QueryPolicy::default(),
                Some(&session),
                Some(&control),
                |claim| {
                    let reply = HostQueryReply::from_claim(claim, limits);
                    let query_bytes = serde_json::to_vec(&HostResponse::Query(reply.clone()))
                        .unwrap()
                        .len();
                    let resource_bytes =
                        serde_json::to_vec(&HostResponse::Resource(HostResourceReply {
                            uri: "symforge://file/content".into(),
                            content: HostResourceContent::Query(reply),
                        }))
                        .unwrap()
                        .len();
                    sizes = Some((query_bytes, resource_bytes));
                    false
                },
            );
            sizes.expect("prospective query and resource frames")
        };
        for _ in 0..4 {
            let (query_bytes, resource_bytes) = measure();
            if query_bytes >= 4_096 && resource_bytes > query_bytes {
                break;
            }
            content_bytes = content_bytes.saturating_add(4_096usize.saturating_sub(query_bytes));
            let before = current_version(&room);
            std::fs::write(&file, "a".repeat(content_bytes)).unwrap();
            let deadline = Instant::now() + Duration::from_secs(20);
            loop {
                let HostResponse::Status(status) =
                    room.dispatch(&HostRequest::Status, &control).unwrap()
                else {
                    panic!("status response");
                };
                if status.phase == HostPhase::Current && status.source_version > before {
                    break;
                }
                assert!(Instant::now() < deadline, "source refresh did not publish");
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        let (query_bytes, resource_bytes) = measure();
        assert!(
            query_bytes >= 4_096 && resource_bytes > query_bytes,
            "query frame {query_bytes} bytes; resource frame {resource_bytes} bytes"
        );
        room.limits.max_response_bytes = u32::try_from(query_bytes).unwrap();
        let revision_before = session.revision();
        let cache_before = session.cache_status();
        let refusal = room
            .dispatch(
                &HostRequest::Resource(HostResourceRequest::FileContent {
                    path: "docs/clip.txt".into(),
                    start_line: None,
                    end_line: None,
                }),
                &control,
            )
            .expect_err("outer resource frame exceeds the exact cap");
        assert_eq!(refusal.kind, HostRefusalKind::ResponseTooLarge);
        assert_eq!(session.revision(), revision_before);
        assert_eq!(session.cache_status(), cache_before);
        let HostResponse::Query(inventory) = room
            .dispatch(
                &HostRequest::Query {
                    request: QueryRequest::ContextInventory,
                    limits: QueryLimits::default(),
                },
                &control,
            )
            .unwrap()
        else {
            panic!("inventory response");
        };
        let QueryOutput::ContextInventory(inventory) = inventory.output else {
            panic!("inventory output");
        };
        assert!(
            inventory
                .fetched_files
                .iter()
                .all(|file| file.path != "docs/clip.txt")
        );
    }
}

pub(crate) fn map_stop(stop: OperationStop) -> HostRefusal {
    HostRefusal::new(match stop {
        OperationStop::Cancelled => HostRefusalKind::Cancelled,
        OperationStop::DeadlineExceeded => HostRefusalKind::DeadlineExceeded,
    })
}

fn render_prompt(
    request: &HostPromptRequest,
    project_name: &str,
    context: &HostPromptContext,
) -> Result<HostPromptReply, HostRefusal> {
    let (name, description, instruction, resources, dynamic_path): (
        &str,
        &str,
        String,
        &[&str],
        Option<&str>,
    ) = match request {
        HostPromptRequest::Admin => {
            let dashboard_url = match context {
                HostPromptContext::Unobserved => {
                    return Err(HostRefusal::new(HostRefusalKind::ServiceContextUnavailable));
                }
                HostPromptContext::DashboardNotRunning => None,
                HostPromptContext::DashboardRunning { url } => Some(url.as_str()),
            };
            (
                "symforge-admin",
                "Open the SymForge operator dashboard (reuse the running server, or start it via `symforge admin`).",
                crate::prompt_engine::build_admin_instructions(project_name, dashboard_url),
                &[],
                None,
            )
        }
        HostPromptRequest::KnowledgeHygiene { path_prefix } => (
            "symforge-knowledge-hygiene",
            "Review exact knowledge evidence and prepare advisory remediation proposals.",
            crate::prompt_engine::build_knowledge_hygiene_instructions(
                project_name,
                path_prefix.as_deref(),
            ),
            &["symforge://repo/health", "symforge://repo/map"],
            None,
        ),
        HostPromptRequest::Review { path, focus } => (
            "symforge-review",
            "Review code using SymForge resources and targeted tools.",
            crate::prompt_engine::build_code_review_instructions(
                project_name,
                path.as_deref(),
                focus.as_deref(),
            ),
            &["symforge://repo/health", "symforge://repo/map"],
            path.as_deref(),
        ),
        HostPromptRequest::Architecture { area } => (
            "symforge-architecture",
            "Map repository architecture using SymForge resources and cross-reference tools.",
            crate::prompt_engine::build_architecture_map_instructions(
                project_name,
                area.as_deref(),
            ),
            &[
                "symforge://repo/map",
                "symforge://repo/outline",
                "symforge://repo/health",
            ],
            None,
        ),
        HostPromptRequest::Triage { symptom, path } => (
            "symforge-triage",
            "Triage failures using SymForge runtime health, changed files, and local context.",
            crate::prompt_engine::build_failure_triage_instructions(
                project_name,
                symptom,
                path.as_deref(),
            ),
            &[
                "symforge://repo/health",
                "symforge://repo/changes/uncommitted",
                "symforge://repo/map",
            ],
            path.as_deref(),
        ),
        HostPromptRequest::Onboard { area } => (
            "symforge-onboard",
            "Onboard to a codebase using layered SymForge exploration.",
            crate::prompt_engine::build_onboard_instructions(project_name, area.as_deref()),
            &[
                "symforge://repo/map",
                "symforge://repo/outline",
                "symforge://repo/health",
                "symforge://tools/catalog",
            ],
            None,
        ),
        HostPromptRequest::Refactor { goal, target } => (
            "symforge-refactor",
            "Plan a refactoring with full impact analysis using SymForge.",
            crate::prompt_engine::build_refactor_instructions(
                project_name,
                goal,
                target.as_deref(),
            ),
            &["symforge://repo/map", "symforge://repo/health"],
            target.as_deref(),
        ),
        HostPromptRequest::Debug { error, path } => (
            "symforge-debug",
            "Debug a problem using SymForge call tracing and impact analysis.",
            crate::prompt_engine::build_debug_instructions(project_name, error, path.as_deref()),
            &[
                "symforge://repo/health",
                "symforge://repo/changes/uncommitted",
            ],
            path.as_deref(),
        ),
    };
    let mut messages = vec![HostPromptMessage {
        role: "user".into(),
        text: Some(instruction),
        resource_uri: None,
    }];
    messages.extend(resources.iter().map(|uri| HostPromptMessage {
        role: "user".into(),
        text: None,
        resource_uri: Some((*uri).to_owned()),
    }));
    if let Some(path) = dynamic_path {
        messages.push(HostPromptMessage {
            role: "user".into(),
            text: None,
            resource_uri: Some(format!(
                "symforge://file/context?path={}&max_tokens=200",
                encode_uri_value(path)
            )),
        });
    }
    if let HostPromptRequest::Architecture { area: Some(area) } = request {
        messages.push(HostPromptMessage {
            role: "user".into(),
            text: Some(format!(
                "Prioritize the area or subsystem named '{area}' if it exists."
            )),
            resource_uri: None,
        });
    }
    Ok(HostPromptReply {
        name: name.into(),
        description: description.into(),
        messages,
    })
}

fn encode_uri_value(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~' | b'/') {
            encoded.push(char::from(byte));
        } else {
            use std::fmt::Write;
            let _ = write!(encoded, "%{byte:02X}");
        }
    }
    encoded
}
