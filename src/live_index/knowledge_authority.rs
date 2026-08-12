use std::collections::BTreeMap;
use std::ops::Range;

use crate::domain::{
    FileDisposition, HistoryCoverage, HistoryLimit, IndexTargets, LanguageId, SourceIdentity,
    SourceVersion, WorkingTreeState,
};

use super::knowledge_bridge::{
    BridgeEvidenceKind, BridgeResolution, CodeAnchor, CodeAnchorId, DerivedCoverage,
    DerivedLimitKind, KnowledgeAnchor, KnowledgeAnchorId, KnowledgeBridge, LimitBreach,
};
use super::store::{CodeSignalsSnapshot, LiveIndex, PublishedGeneration};

pub const AUTHORITY_RULE_VERSION: u32 = 1;
pub const KNOWLEDGE_POLICY_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum KnowledgeLifecycle {
    Active,
    Proposed,
    Accepted,
    Implemented,
    Deferred,
    Rejected,
    Withdrawn,
    Deprecated,
    Superseded,
    Archived,
    Historical,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LifecycleEvidence {
    PolicyEntry { entry_id: String },
    DeclaredSpan(KnowledgeAnchor),
    ArchivePathRule { rule_id: String },
    None,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum AuthorityDomain {
    CurrentImplementation,
    NormativeIntent,
    Decision,
    Operations,
    Governance,
    HistoricalRecord,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AuthorityDomainEvidence {
    PolicyEntry {
        entry_id: String,
    },
    DeclaredSpan(KnowledgeAnchor),
    RoleRule {
        rule_id: String,
        anchor: KnowledgeAnchor,
    },
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum CodeEvidenceDisplay {
    DeterministicConflict,
    BrokenAnchor,
    ImplementationGap,
    SuspectedConflict,
    RelevantCodeChangedSinceDocument,
    ReviewDue,
    Partial,
    Unresolved,
    ConsistentForCheckedClaims,
    NotApplicable,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodeEvidenceSummary {
    pub display: CodeEvidenceDisplay,
    pub consistent_rule_ids: Vec<String>,
    pub broken_link_indices: Vec<u32>,
    pub deterministic_conflict_ids: Vec<String>,
    pub suspected_conflict_ids: Vec<String>,
    pub implementation_gap_ids: Vec<String>,
    pub relevant_code_change_count: u32,
    pub review_signal_ids: Vec<String>,
    pub unresolved_semantics: bool,
    pub not_applicable: bool,
    pub coverage: DerivedCoverage,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodeEvidenceFacts {
    pub consistent_rule_ids: Vec<String>,
    pub broken_link_indices: Vec<u32>,
    pub deterministic_conflict_ids: Vec<String>,
    pub suspected_conflict_ids: Vec<String>,
    pub implementation_gap_ids: Vec<String>,
    pub relevant_code_change_count: u32,
    pub review_signal_ids: Vec<String>,
    pub unresolved_semantics: bool,
    pub not_applicable: bool,
    pub coverage: DerivedCoverage,
}

impl CodeEvidenceFacts {
    pub fn from_timeline(timeline: &DocumentTimeline) -> Self {
        let mut review_signal_ids = Vec::new();
        if timeline.filesystem_created.is_some() && timeline.filesystem_modified.is_some() {
            review_signal_ids.push("filesystem-time-hint".to_string());
        }
        let filesystem_clock_skew = timeline
            .filesystem_created
            .as_ref()
            .and_then(|evidence| evidence.unix_seconds)
            .zip(
                timeline
                    .filesystem_modified
                    .as_ref()
                    .and_then(|evidence| evidence.unix_seconds),
            )
            .is_some_and(|(created, modified)| created > modified);
        let git_clock_skew = timeline
            .git_first_seen
            .as_ref()
            .and_then(|evidence| evidence.unix_seconds)
            .zip(
                timeline
                    .git_last_touch
                    .as_ref()
                    .and_then(|evidence| evidence.unix_seconds),
            )
            .is_some_and(|(first_seen, last_touch)| first_seen > last_touch);
        if filesystem_clock_skew || git_clock_skew {
            review_signal_ids.push("clock-skew-detected".to_string());
        }
        if timeline.working_tree_changed {
            review_signal_ids.push("working-tree-changed".to_string());
        }

        let mut relevant_code_change_count = 0_u32;
        for change in &timeline.relevant_code_changes {
            match change.topologically_after_document {
                Some(true) => {
                    relevant_code_change_count = relevant_code_change_count.saturating_add(1);
                }
                None => review_signal_ids.push(change.rule_id.clone()),
                Some(false) => {}
            }
        }
        if !timeline.coverage.complete_to_root || !timeline.coverage.limitations.is_empty() {
            review_signal_ids.push("temporal-coverage-incomplete".to_string());
        }

        Self {
            consistent_rule_ids: Vec::new(),
            broken_link_indices: Vec::new(),
            deterministic_conflict_ids: Vec::new(),
            suspected_conflict_ids: Vec::new(),
            implementation_gap_ids: Vec::new(),
            relevant_code_change_count,
            review_signal_ids,
            unresolved_semantics: false,
            not_applicable: false,
            coverage: DerivedCoverage::Complete,
        }
    }
}

pub fn summarize_code_evidence(mut facts: CodeEvidenceFacts) -> CodeEvidenceSummary {
    facts.consistent_rule_ids.sort();
    facts.consistent_rule_ids.dedup();
    facts.broken_link_indices.sort_unstable();
    facts.broken_link_indices.dedup();
    facts.deterministic_conflict_ids.sort();
    facts.deterministic_conflict_ids.dedup();
    facts.suspected_conflict_ids.sort();
    facts.suspected_conflict_ids.dedup();
    facts.implementation_gap_ids.sort();
    facts.implementation_gap_ids.dedup();
    facts.review_signal_ids.sort();
    facts.review_signal_ids.dedup();

    let display = if !facts.deterministic_conflict_ids.is_empty() {
        CodeEvidenceDisplay::DeterministicConflict
    } else if !facts.broken_link_indices.is_empty() {
        CodeEvidenceDisplay::BrokenAnchor
    } else if !facts.implementation_gap_ids.is_empty() {
        CodeEvidenceDisplay::ImplementationGap
    } else if !facts.suspected_conflict_ids.is_empty() {
        CodeEvidenceDisplay::SuspectedConflict
    } else if facts.relevant_code_change_count > 0 {
        CodeEvidenceDisplay::RelevantCodeChangedSinceDocument
    } else if !facts.review_signal_ids.is_empty() {
        CodeEvidenceDisplay::ReviewDue
    } else if matches!(facts.coverage, DerivedCoverage::Truncated { .. }) {
        CodeEvidenceDisplay::Partial
    } else if facts.unresolved_semantics {
        CodeEvidenceDisplay::Unresolved
    } else if !facts.consistent_rule_ids.is_empty() {
        CodeEvidenceDisplay::ConsistentForCheckedClaims
    } else {
        CodeEvidenceDisplay::NotApplicable
    };

    CodeEvidenceSummary {
        display,
        consistent_rule_ids: facts.consistent_rule_ids,
        broken_link_indices: facts.broken_link_indices,
        deterministic_conflict_ids: facts.deterministic_conflict_ids,
        suspected_conflict_ids: facts.suspected_conflict_ids,
        implementation_gap_ids: facts.implementation_gap_ids,
        relevant_code_change_count: facts.relevant_code_change_count,
        review_signal_ids: facts.review_signal_ids,
        unresolved_semantics: facts.unresolved_semantics,
        not_applicable: facts.not_applicable,
        coverage: facts.coverage,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum KnowledgeVoice {
    Current,
    Intent,
    NeedsReview,
    Unknown,
    HistoryOnly,
    Suppressed,
}

pub fn derive_voice(
    lifecycle: KnowledgeLifecycle,
    authority_domain: AuthorityDomain,
    code_evidence: &CodeEvidenceSummary,
) -> KnowledgeVoice {
    if matches!(
        lifecycle,
        KnowledgeLifecycle::Rejected
            | KnowledgeLifecycle::Withdrawn
            | KnowledgeLifecycle::Deprecated
            | KnowledgeLifecycle::Superseded
            | KnowledgeLifecycle::Archived
            | KnowledgeLifecycle::Historical
    ) {
        return KnowledgeVoice::HistoryOnly;
    }

    if matches!(
        authority_domain,
        AuthorityDomain::NormativeIntent | AuthorityDomain::Decision | AuthorityDomain::Governance
    ) {
        return KnowledgeVoice::Intent;
    }

    if authority_domain == AuthorityDomain::CurrentImplementation
        && code_evidence.display == CodeEvidenceDisplay::DeterministicConflict
    {
        return KnowledgeVoice::Suppressed;
    }

    if lifecycle == KnowledgeLifecycle::Unknown || authority_domain == AuthorityDomain::Unknown {
        return KnowledgeVoice::Unknown;
    }

    match code_evidence.display {
        CodeEvidenceDisplay::ConsistentForCheckedClaims => KnowledgeVoice::Current,
        CodeEvidenceDisplay::NotApplicable => KnowledgeVoice::Unknown,
        CodeEvidenceDisplay::DeterministicConflict => KnowledgeVoice::NeedsReview,
        CodeEvidenceDisplay::BrokenAnchor
        | CodeEvidenceDisplay::ImplementationGap
        | CodeEvidenceDisplay::SuspectedConflict
        | CodeEvidenceDisplay::RelevantCodeChangedSinceDocument
        | CodeEvidenceDisplay::ReviewDue
        | CodeEvidenceDisplay::Partial
        | CodeEvidenceDisplay::Unresolved => KnowledgeVoice::NeedsReview,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum TimeProvenance {
    FilesystemBirth,
    FilesystemModified,
    GitFirstSeen,
    GitLastTouch,
    WorkingTreeObservation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TimeEvidence {
    pub unix_seconds: Option<i64>,
    pub provenance: TimeProvenance,
    pub coverage: HistoryCoverage,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocumentTimeline {
    pub filesystem_created: Option<TimeEvidence>,
    pub filesystem_modified: Option<TimeEvidence>,
    pub git_first_seen: Option<TimeEvidence>,
    pub git_last_touch: Option<TimeEvidence>,
    pub working_tree_changed: bool,
    pub relevant_code_changes: Vec<CodeChangeEvidence>,
    pub coverage: HistoryCoverage,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodeChangeEvidence {
    pub anchor: CodeAnchor,
    pub commit_id: Option<String>,
    pub unix_seconds: Option<i64>,
    pub topologically_after_document: Option<bool>,
    pub rule_id: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum EvidenceConfidence {
    Deterministic,
    StrongCandidate,
    ReviewSignal,
    Unresolved,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RemediationPrecondition {
    ProtectedRole,
    UniqueContentUnknown,
    InboundLiveLinks { count: u64 },
    MissingSuccessor,
    SuccessorCoverageIncomplete,
    SourceCoverageDegraded,
    WorkingTreeDirtyOrUntracked,
    UnsupportedPath,
    RequiresUserJudgment,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RemediationAction {
    Keep,
    Update,
    RelabelIntent,
    MergeInto { target: KnowledgeAnchor },
    MarkSuperseded { successor: KnowledgeAnchor },
    Archive,
    DeletionCandidate { retained: KnowledgeAnchor },
    NeedsReview,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemediationProposal {
    pub action: RemediationAction,
    pub confidence: EvidenceConfidence,
    pub evidence_ids: Vec<String>,
    pub unmet_preconditions: Vec<RemediationPrecondition>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnowledgeAuthorityRecord {
    pub unit: KnowledgeAnchor,
    pub lifecycle: KnowledgeLifecycle,
    pub lifecycle_evidence: LifecycleEvidence,
    pub authority_domain: AuthorityDomain,
    pub authority_domain_evidence: AuthorityDomainEvidence,
    pub code_evidence: CodeEvidenceSummary,
    pub voice: KnowledgeVoice,
    pub successor: Option<KnowledgeAnchor>,
    pub timeline: DocumentTimeline,
    pub proposal: RemediationProposal,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnowledgePolicy {
    pub version: u32,
    pub entries: Vec<KnowledgePolicyEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnowledgePolicyTarget {
    pub path: String,
    pub content_hash: String,
    pub unit_byte_range: Option<Range<u32>>,
    pub unit_hash: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnowledgePolicyEntry {
    pub entry_id: String,
    pub target: KnowledgePolicyTarget,
    pub lifecycle: KnowledgeLifecycle,
    pub authority_domain: Option<AuthorityDomain>,
    pub superseded_by: Option<KnowledgePolicyTarget>,
    pub evidence: Vec<PolicyEvidenceRef>,
    pub justification_code: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PolicyEvidenceRef {
    pub rule_id: String,
    pub knowledge: Option<KnowledgePolicyTarget>,
    pub code: Option<CodeAnchorId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PolicyParseError {
    Malformed,
    UnsupportedVersion { found: Option<u32> },
    InvalidField { field: String },
}

pub fn parse_knowledge_policy(bytes: &[u8]) -> Result<KnowledgePolicy, PolicyParseError> {
    let text = std::str::from_utf8(bytes).map_err(|_| PolicyParseError::Malformed)?;
    let document = text
        .parse::<toml_edit::DocumentMut>()
        .map_err(|_| PolicyParseError::Malformed)?;
    let version = document
        .get("version")
        .and_then(toml_edit::Item::as_integer)
        .and_then(|value| u32::try_from(value).ok());
    if version != Some(KNOWLEDGE_POLICY_VERSION) {
        return Err(PolicyParseError::UnsupportedVersion { found: version });
    }

    let mut entries = Vec::new();
    if let Some(tables) = document
        .get("entries")
        .and_then(toml_edit::Item::as_array_of_tables)
    {
        for (index, table) in tables.iter().enumerate() {
            entries.push(parse_policy_entry(table, index)?);
        }
    } else if document.get("entries").is_some() {
        return Err(invalid_policy_field("entries"));
    }

    entries.sort_by(|left, right| left.entry_id.cmp(&right.entry_id));
    if entries
        .windows(2)
        .any(|pair| pair[0].entry_id == pair[1].entry_id)
    {
        return Err(invalid_policy_field("entries.entry_id"));
    }
    Ok(KnowledgePolicy {
        version: KNOWLEDGE_POLICY_VERSION,
        entries,
    })
}

fn parse_policy_entry(
    table: &toml_edit::Table,
    index: usize,
) -> Result<KnowledgePolicyEntry, PolicyParseError> {
    let prefix = format!("entries[{index}]");
    let entry_id = required_policy_string(table, "entry_id", &prefix)?;
    let lifecycle = parse_lifecycle(&required_policy_string(table, "lifecycle", &prefix)?)
        .ok_or_else(|| invalid_policy_field(format!("{prefix}.lifecycle")))?;
    let authority_domain = table
        .get("authority_domain")
        .map(|item| {
            item.as_str()
                .and_then(parse_authority_domain)
                .ok_or_else(|| invalid_policy_field(format!("{prefix}.authority_domain")))
        })
        .transpose()?;
    let justification_code = required_policy_string(table, "justification_code", &prefix)?;
    let target = table
        .get("target")
        .and_then(toml_edit::Item::as_table)
        .ok_or_else(|| invalid_policy_field(format!("{prefix}.target")))
        .and_then(|target| parse_policy_target(target, &format!("{prefix}.target")))?;
    let superseded_by = table
        .get("superseded_by")
        .map(|item| {
            item.as_table()
                .ok_or_else(|| invalid_policy_field(format!("{prefix}.superseded_by")))
                .and_then(|target| parse_policy_target(target, &format!("{prefix}.superseded_by")))
        })
        .transpose()?;

    let mut evidence = Vec::new();
    if let Some(tables) = table
        .get("evidence")
        .and_then(toml_edit::Item::as_array_of_tables)
    {
        for (evidence_index, evidence_table) in tables.iter().enumerate() {
            evidence.push(parse_policy_evidence(
                evidence_table,
                &format!("{prefix}.evidence[{evidence_index}]"),
            )?);
        }
    } else if table.get("evidence").is_some() {
        return Err(invalid_policy_field(format!("{prefix}.evidence")));
    }

    Ok(KnowledgePolicyEntry {
        entry_id,
        target,
        lifecycle,
        authority_domain,
        superseded_by,
        evidence,
        justification_code,
    })
}

fn parse_policy_target(
    table: &toml_edit::Table,
    prefix: &str,
) -> Result<KnowledgePolicyTarget, PolicyParseError> {
    let path = required_policy_string(table, "path", prefix)?;
    if !is_safe_policy_path(&path) {
        return Err(invalid_policy_field(format!("{prefix}.path")));
    }
    let content_hash = required_policy_string(table, "content_hash", prefix)?;
    let unit_hash = optional_policy_string(table, "unit_hash", prefix)?;
    let unit_byte_range = table
        .get("unit_byte_range")
        .map(|item| parse_unit_range(item, &format!("{prefix}.unit_byte_range")))
        .transpose()?;
    if unit_byte_range.is_some() != unit_hash.is_some() {
        return Err(invalid_policy_field(format!("{prefix}.unit_byte_range")));
    }
    Ok(KnowledgePolicyTarget {
        path,
        content_hash,
        unit_byte_range,
        unit_hash,
    })
}

fn parse_policy_evidence(
    table: &toml_edit::Table,
    prefix: &str,
) -> Result<PolicyEvidenceRef, PolicyParseError> {
    let rule_id = required_policy_string(table, "rule_id", prefix)?;
    let code = optional_policy_string(table, "code_path", prefix)?
        .map(|path| {
            if is_safe_policy_path(&path) {
                Ok(CodeAnchorId::File { path })
            } else {
                Err(invalid_policy_field(format!("{prefix}.code_path")))
            }
        })
        .transpose()?;
    let knowledge_path = optional_policy_string(table, "knowledge_path", prefix)?;
    let knowledge_hash = optional_policy_string(table, "knowledge_content_hash", prefix)?;
    let knowledge = match (knowledge_path, knowledge_hash) {
        (None, None) => None,
        (Some(path), Some(content_hash)) if is_safe_policy_path(&path) => {
            Some(KnowledgePolicyTarget {
                path,
                content_hash,
                unit_byte_range: None,
                unit_hash: None,
            })
        }
        _ => return Err(invalid_policy_field(format!("{prefix}.knowledge"))),
    };
    Ok(PolicyEvidenceRef {
        rule_id,
        knowledge,
        code,
    })
}

fn required_policy_string(
    table: &toml_edit::Table,
    key: &str,
    prefix: &str,
) -> Result<String, PolicyParseError> {
    optional_policy_string(table, key, prefix)?
        .ok_or_else(|| invalid_policy_field(format!("{prefix}.{key}")))
}

fn optional_policy_string(
    table: &toml_edit::Table,
    key: &str,
    prefix: &str,
) -> Result<Option<String>, PolicyParseError> {
    match table.get(key) {
        None => Ok(None),
        Some(item) => item
            .as_str()
            .filter(|value| !value.is_empty())
            .map(|value| Some(value.to_string()))
            .ok_or_else(|| invalid_policy_field(format!("{prefix}.{key}"))),
    }
}

fn parse_unit_range(item: &toml_edit::Item, field: &str) -> Result<Range<u32>, PolicyParseError> {
    let array = item
        .as_array()
        .filter(|array| array.len() == 2)
        .ok_or_else(|| invalid_policy_field(field))?;
    let start = array
        .get(0)
        .and_then(toml_edit::Value::as_integer)
        .and_then(|value| u32::try_from(value).ok())
        .ok_or_else(|| invalid_policy_field(field))?;
    let end = array
        .get(1)
        .and_then(toml_edit::Value::as_integer)
        .and_then(|value| u32::try_from(value).ok())
        .filter(|end| *end > start)
        .ok_or_else(|| invalid_policy_field(field))?;
    Ok(start..end)
}

fn parse_lifecycle(value: &str) -> Option<KnowledgeLifecycle> {
    match value {
        "active" => Some(KnowledgeLifecycle::Active),
        "proposed" => Some(KnowledgeLifecycle::Proposed),
        "accepted" => Some(KnowledgeLifecycle::Accepted),
        "implemented" => Some(KnowledgeLifecycle::Implemented),
        "deferred" => Some(KnowledgeLifecycle::Deferred),
        "rejected" => Some(KnowledgeLifecycle::Rejected),
        "withdrawn" => Some(KnowledgeLifecycle::Withdrawn),
        "deprecated" => Some(KnowledgeLifecycle::Deprecated),
        "superseded" => Some(KnowledgeLifecycle::Superseded),
        "archived" => Some(KnowledgeLifecycle::Archived),
        "historical" => Some(KnowledgeLifecycle::Historical),
        "unknown" => Some(KnowledgeLifecycle::Unknown),
        _ => None,
    }
}

fn parse_authority_domain(value: &str) -> Option<AuthorityDomain> {
    match value {
        "current_implementation" => Some(AuthorityDomain::CurrentImplementation),
        "normative_intent" => Some(AuthorityDomain::NormativeIntent),
        "decision" => Some(AuthorityDomain::Decision),
        "operations" => Some(AuthorityDomain::Operations),
        "governance" => Some(AuthorityDomain::Governance),
        "historical_record" => Some(AuthorityDomain::HistoricalRecord),
        "unknown" => Some(AuthorityDomain::Unknown),
        _ => None,
    }
}

fn is_safe_policy_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.starts_with('\\')
        && !path.contains('\\')
        && !path.contains(':')
        && path
            .split('/')
            .all(|component| !component.is_empty() && component != "." && component != "..")
}

fn invalid_policy_field(field: impl Into<String>) -> PolicyParseError {
    PolicyParseError::InvalidField {
        field: field.into(),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum KnowledgeAuthorityScope {
    Default,
    Current,
    Intent,
    History,
    All,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DerivedResourceUsage {
    pub knowledge_cards: u64,
    pub bridge_links: u64,
    pub authority_records: u64,
    pub derived_metadata_bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PolicyLedgerStatus {
    Absent,
    Valid,
    Malformed,
    UnsupportedVersion { found: Option<u32> },
    InvalidEntries,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AuthorityVersions {
    pub authority_rule_version: u32,
    pub policy_version: u32,
    pub secret_policy_version: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnowledgeAuthorityView {
    pub source: Option<SourceIdentity>,
    pub source_version: Option<SourceVersion>,
    pub content_generation: u64,
    pub records: Vec<KnowledgeAuthorityRecord>,
    pub finding_index: BTreeMap<String, u32>,
    pub policy_digest: String,
    pub policy_status: PolicyLedgerStatus,
    pub curation_eligible: bool,
    pub skipped_suppression_ids: Vec<String>,
    pub coverage: DerivedCoverage,
    pub usage: DerivedResourceUsage,
    pub versions: AuthorityVersions,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AuthorityLimits {
    pub max_authority_records: usize,
    pub max_findings: usize,
    pub max_metadata_bytes: usize,
}

impl Default for AuthorityLimits {
    fn default() -> Self {
        Self {
            max_authority_records: 50_000,
            max_findings: 100_000,
            max_metadata_bytes: 32 * 1024 * 1024,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AuthorityTemporalIndex {
    pub timelines: BTreeMap<String, DocumentTimeline>,
}

impl AuthorityTemporalIndex {
    pub fn from_published(published: &PublishedGeneration) -> Self {
        Self::from_components(
            &published.live,
            published.source_version.as_deref(),
            &published.code_signals,
        )
    }

    pub fn from_components(
        live: &LiveIndex,
        source_version: Option<&SourceVersion>,
        code_signals: &CodeSignalsSnapshot,
    ) -> Self {
        let coverage = code_signals.coverage.as_ref().clone();
        let working_tree_changed =
            source_version.is_some_and(|version| version.working_tree == WorkingTreeState::Dirty);
        let mut timelines = BTreeMap::new();
        for (path, file) in &live.files {
            let filesystem_modified = (file.mtime_secs != 0).then(|| TimeEvidence {
                unix_seconds: i64::try_from(file.mtime_secs).ok(),
                provenance: TimeProvenance::FilesystemModified,
                coverage: coverage.clone(),
            });
            let git_last_touch = code_signals.temporal.files.get(path).map(|_| TimeEvidence {
                unix_seconds: None,
                provenance: TimeProvenance::GitLastTouch,
                coverage: coverage.clone(),
            });
            timelines.insert(
                path.clone(),
                DocumentTimeline {
                    filesystem_created: None,
                    filesystem_modified,
                    git_first_seen: None,
                    git_last_touch,
                    working_tree_changed,
                    relevant_code_changes: Vec::new(),
                    coverage: coverage.clone(),
                },
            );
        }
        Self { timelines }
    }
}

#[derive(Clone)]
struct AuthorityUnitState {
    anchor: KnowledgeAnchor,
    lifecycle: KnowledgeLifecycle,
    lifecycle_evidence: LifecycleEvidence,
    authority_domain: AuthorityDomain,
    authority_domain_evidence: AuthorityDomainEvidence,
    successor: Option<KnowledgeAnchor>,
    policy_finding_ids: Vec<String>,
}

struct PolicyEvaluation {
    digest: String,
    status: PolicyLedgerStatus,
    policy: Option<KnowledgePolicy>,
    invalid_entry_ids: Vec<String>,
    global_finding_rule: Option<&'static str>,
}

#[allow(clippy::too_many_arguments)]
pub fn build_knowledge_authority(
    live: &LiveIndex,
    source: &SourceIdentity,
    source_version: &SourceVersion,
    content_generation: u64,
    bridge: &KnowledgeBridge,
    temporal: &AuthorityTemporalIndex,
    secret_policy_version: u32,
    limits: &AuthorityLimits,
) -> KnowledgeAuthorityView {
    let mut units = collect_authority_units(live, source, content_generation);
    let policy = evaluate_policy(live, &units);
    apply_policy(&mut units, &policy);

    let mut candidates = Vec::with_capacity(units.len());
    for unit in units {
        let timeline = temporal
            .timelines
            .get(&unit.anchor.path)
            .cloned()
            .unwrap_or_else(unavailable_timeline);
        let mut facts = facts_for_unit(&unit, bridge, &timeline);
        facts
            .suspected_conflict_ids
            .extend(unit.policy_finding_ids.iter().cloned());
        if let Some(rule) = policy.global_finding_rule {
            facts
                .review_signal_ids
                .push(stable_finding_id(&unit.anchor, rule, "policy-ledger"));
        }
        let code_evidence = summarize_code_evidence(facts);
        let voice = derive_voice(unit.lifecycle, unit.authority_domain, &code_evidence);
        let proposal = derive_proposal(
            unit.lifecycle,
            unit.authority_domain,
            voice,
            unit.successor.as_ref(),
            &code_evidence,
        );
        candidates.push(KnowledgeAuthorityRecord {
            unit: unit.anchor,
            lifecycle: unit.lifecycle,
            lifecycle_evidence: unit.lifecycle_evidence,
            authority_domain: unit.authority_domain,
            authority_domain_evidence: unit.authority_domain_evidence,
            code_evidence,
            voice,
            successor: unit.successor,
            timeline,
            proposal,
        });
    }

    candidates.sort_by(|left, right| {
        (
            !is_reserved_suppression_record(left),
            left.unit.path.as_str(),
            left.unit.byte_range.start,
        )
            .cmp(&(
                !is_reserved_suppression_record(right),
                right.unit.path.as_str(),
                right.unit.byte_range.start,
            ))
    });

    let mut breaches = coverage_breaches(&bridge.coverage);
    let mut records = Vec::new();
    let mut skipped_suppression_ids = Vec::new();
    let mut metadata_bytes = 0_usize;
    for record in candidates {
        let required_metadata = authority_record_metadata_bytes(&record);
        let record_fits = records.len() < limits.max_authority_records;
        let metadata_fits = metadata_bytes
            .checked_add(required_metadata)
            .is_some_and(|total| total <= limits.max_metadata_bytes);
        if record_fits && metadata_fits {
            metadata_bytes = metadata_bytes.saturating_add(required_metadata);
            records.push(record);
            continue;
        }

        if !record_fits {
            note_breach(&mut breaches, DerivedLimitKind::AuthorityRecords, 1);
        }
        if !metadata_fits {
            note_breach(&mut breaches, DerivedLimitKind::MetadataBytes, 1);
        }
        if is_reserved_suppression_record(&record) {
            skipped_suppression_ids.push(stable_finding_id(
                &record.unit,
                "budget-suppression-state",
                "skipped-suppression",
            ));
        }
    }
    skipped_suppression_ids.sort();
    skipped_suppression_ids.dedup();

    let mut finding_index = BTreeMap::new();
    for (record_index, record) in records.iter().enumerate() {
        for finding_id in record_finding_ids(record) {
            if finding_index.len() >= limits.max_findings {
                note_breach(&mut breaches, DerivedLimitKind::Findings, 1);
                continue;
            }
            finding_index
                .entry(finding_id)
                .or_insert_with(|| u32::try_from(record_index).unwrap_or(u32::MAX));
        }
    }

    normalize_breaches(&mut breaches);
    let coverage = if breaches.is_empty() {
        DerivedCoverage::Complete
    } else {
        DerivedCoverage::Truncated { breaches }
    };
    let usage = DerivedResourceUsage {
        knowledge_cards: u64::try_from(bridge.cards.len()).unwrap_or(u64::MAX),
        bridge_links: u64::try_from(bridge.forward.len()).unwrap_or(u64::MAX),
        authority_records: u64::try_from(records.len()).unwrap_or(u64::MAX),
        derived_metadata_bytes: u64::try_from(metadata_bytes).unwrap_or(u64::MAX),
    };
    let curation_eligible = matches!(
        policy.status,
        PolicyLedgerStatus::Absent | PolicyLedgerStatus::Valid
    );

    KnowledgeAuthorityView {
        source: Some(source.clone()),
        source_version: Some(source_version.clone()),
        content_generation,
        records,
        finding_index,
        policy_digest: policy.digest,
        policy_status: policy.status,
        curation_eligible,
        skipped_suppression_ids,
        coverage,
        usage,
        versions: AuthorityVersions {
            authority_rule_version: AUTHORITY_RULE_VERSION,
            policy_version: KNOWLEDGE_POLICY_VERSION,
            secret_policy_version,
        },
    }
}

fn collect_authority_units(
    live: &LiveIndex,
    source: &SourceIdentity,
    content_generation: u64,
) -> Vec<AuthorityUnitState> {
    let manifest_targets: BTreeMap<&str, IndexTargets> = live
        .manifest_entries
        .iter()
        .filter_map(|entry| {
            let path = entry.path.normalized_utf8.as_deref()?;
            match entry.disposition {
                FileDisposition::Indexed { targets, .. } => Some((path, targets)),
                _ => None,
            }
        })
        .collect();
    let mut paths: Vec<&str> = live.files.keys().map(String::as_str).collect();
    paths.sort_unstable();
    let mut units = Vec::new();

    for path in paths {
        if path == ".symforge-knowledge.toml" {
            continue;
        }
        let Some(file) = live.files.get(path) else {
            continue;
        };
        let targets = manifest_targets
            .get(path)
            .copied()
            .unwrap_or_else(|| IndexTargets::for_path(path, Some(&file.language)));
        if !targets.includes_knowledge() {
            continue;
        }

        let projected = if file.language == LanguageId::Markdown {
            crate::knowledge::project_markdown_sections(
                source,
                path,
                &file.content_hash,
                &file.symbols,
            )
        } else {
            Vec::new()
        };
        if projected.is_empty() {
            if file.content.is_empty() {
                continue;
            }
            push_authority_unit(
                &mut units,
                source,
                content_generation,
                path,
                file,
                0..u32::try_from(file.content.len()).unwrap_or(u32::MAX),
                1..line_count_half_open(&file.content),
                &[],
            );
        } else {
            for projected_unit in projected {
                push_authority_unit(
                    &mut units,
                    source,
                    content_generation,
                    path,
                    file,
                    projected_unit.byte_range,
                    projected_unit.line_range,
                    &projected_unit.heading_path,
                );
            }
        }
    }
    units
}

#[allow(clippy::too_many_arguments)]
fn push_authority_unit(
    units: &mut Vec<AuthorityUnitState>,
    source: &SourceIdentity,
    content_generation: u64,
    path: &str,
    file: &super::store::IndexedFile,
    byte_range: Range<u32>,
    line_range: Range<u32>,
    heading_path: &[String],
) {
    let anchor = KnowledgeAnchor {
        id: KnowledgeAnchorId {
            path: path.to_string(),
            content_hash: file.content_hash.clone(),
            start_byte: byte_range.start,
        },
        source: source.clone(),
        content_generation,
        path: path.to_string(),
        content_hash: file.content_hash.clone(),
        byte_range: byte_range.clone(),
        line_range,
    };
    let bytes = file
        .content
        .get(byte_range.start as usize..byte_range.end as usize)
        .unwrap_or(&[]);
    let (lifecycle, lifecycle_evidence) = derive_native_lifecycle(path, bytes, &anchor);
    let (authority_domain, authority_domain_evidence) =
        derive_native_authority_domain(path, heading_path, &anchor);
    units.push(AuthorityUnitState {
        anchor,
        lifecycle,
        lifecycle_evidence,
        authority_domain,
        authority_domain_evidence,
        successor: None,
        policy_finding_ids: Vec::new(),
    });
}

fn line_count_half_open(bytes: &[u8]) -> u32 {
    let lines = bytes.iter().filter(|byte| **byte == b'\n').count() + 1;
    u32::try_from(lines.saturating_add(1)).unwrap_or(u32::MAX)
}

fn derive_native_lifecycle(
    path: &str,
    bytes: &[u8],
    anchor: &KnowledgeAnchor,
) -> (KnowledgeLifecycle, LifecycleEvidence) {
    if let Ok(text) = std::str::from_utf8(bytes) {
        for line in text.lines() {
            let normalized = line.trim().to_ascii_lowercase();
            if let Some(value) = normalized
                .strip_prefix("status:")
                .map(str::trim)
                .and_then(parse_lifecycle)
            {
                return (value, LifecycleEvidence::DeclaredSpan(anchor.clone()));
            }
        }
    }
    if path.split('/').any(|component| {
        matches!(
            component.to_ascii_lowercase().as_str(),
            "archive" | "archived"
        )
    }) {
        return (
            KnowledgeLifecycle::Archived,
            LifecycleEvidence::ArchivePathRule {
                rule_id: "authority-path-archive-v1".to_string(),
            },
        );
    }
    // No declared status, no archive path: nothing was observed, so nothing is
    // claimed. This previously returned `Active` with `LifecycleEvidence::None`
    // and the surface printed `lifecycle=active` as though it had been derived
    // from evidence -- and `derive_voice` then consumed that invented `Active`
    // to report `voice=current`. The hygiene contract is explicit that lifecycle
    // always cites hash-valid policy or exact declared evidence and that code
    // does not assign lifecycle, and `Unknown` is a legal value for exactly this
    // case.
    (KnowledgeLifecycle::Unknown, LifecycleEvidence::None)
}

fn derive_native_authority_domain(
    path: &str,
    heading_path: &[String],
    anchor: &KnowledgeAnchor,
) -> (AuthorityDomain, AuthorityDomainEvidence) {
    let heading = heading_path
        .last()
        .map(|value| value.to_ascii_lowercase())
        .unwrap_or_default();
    let path_lower = path.to_ascii_lowercase();
    let (domain, rule_id) = if heading.contains("current implementation")
        || heading.contains("current behavior")
        || path_lower.ends_with("readme.md")
    {
        (
            AuthorityDomain::CurrentImplementation,
            "authority-current-v1",
        )
    } else if heading.contains("intent")
        || heading.contains("requirement")
        || path_lower.contains("/spec")
        || path_lower.contains("/design")
        || path_lower.contains("/rfc")
    {
        (AuthorityDomain::NormativeIntent, "authority-intent-v1")
    } else if heading.contains("decision") || path_lower.contains("/adr") {
        (AuthorityDomain::Decision, "authority-decision-v1")
    } else if heading.contains("operation")
        || heading.contains("runbook")
        || path_lower.contains("/ops/")
    {
        (AuthorityDomain::Operations, "authority-operations-v1")
    } else if heading.contains("governance")
        || heading.contains("security")
        || path_lower.contains("governance")
        || path_lower.ends_with("codeowners")
    {
        (AuthorityDomain::Governance, "authority-governance-v1")
    } else if heading.contains("history")
        || path_lower.contains("changelog")
        || path_lower.contains("/archive/")
    {
        (AuthorityDomain::HistoricalRecord, "authority-history-v1")
    } else {
        return (AuthorityDomain::Unknown, AuthorityDomainEvidence::Unknown);
    };
    (
        domain,
        AuthorityDomainEvidence::RoleRule {
            rule_id: rule_id.to_string(),
            anchor: anchor.clone(),
        },
    )
}

fn evaluate_policy(live: &LiveIndex, units: &[AuthorityUnitState]) -> PolicyEvaluation {
    let Some(file) = live.files.get(".symforge-knowledge.toml") else {
        return PolicyEvaluation {
            digest: crate::hash::digest_hex(&[]),
            status: PolicyLedgerStatus::Absent,
            policy: None,
            invalid_entry_ids: Vec::new(),
            global_finding_rule: None,
        };
    };
    let digest = crate::hash::digest_hex(&file.content);
    let parsed = match parse_knowledge_policy(&file.content) {
        Ok(policy) => policy,
        Err(PolicyParseError::Malformed) => {
            return PolicyEvaluation {
                digest,
                status: PolicyLedgerStatus::Malformed,
                policy: None,
                invalid_entry_ids: Vec::new(),
                global_finding_rule: Some("policy-malformed-v1"),
            };
        }
        Err(PolicyParseError::UnsupportedVersion { found }) => {
            return PolicyEvaluation {
                digest,
                status: PolicyLedgerStatus::UnsupportedVersion { found },
                policy: None,
                invalid_entry_ids: Vec::new(),
                global_finding_rule: Some("policy-unsupported-version-v1"),
            };
        }
        Err(PolicyParseError::InvalidField { .. }) => {
            return PolicyEvaluation {
                digest,
                status: PolicyLedgerStatus::Malformed,
                policy: None,
                invalid_entry_ids: Vec::new(),
                global_finding_rule: Some("policy-invalid-field-v1"),
            };
        }
    };

    let cyclic_targets = cyclic_policy_target_keys(&parsed.entries);
    let mut invalid_entry_ids = Vec::new();
    for entry in &parsed.entries {
        if cyclic_targets.contains(&policy_target_key(&entry.target))
            || !policy_target_is_current(live, units, &entry.target)
            || entry
                .superseded_by
                .as_ref()
                .is_some_and(|target| !policy_target_is_current(live, units, target))
            || policy_conflicts_with_native(units, entry)
            || (entry.lifecycle == KnowledgeLifecycle::Superseded && entry.superseded_by.is_none())
        {
            invalid_entry_ids.push(entry.entry_id.clone());
        }
    }
    invalid_entry_ids.sort();
    invalid_entry_ids.dedup();
    let status = if invalid_entry_ids.is_empty() {
        PolicyLedgerStatus::Valid
    } else {
        PolicyLedgerStatus::InvalidEntries
    };
    PolicyEvaluation {
        digest,
        status,
        policy: Some(parsed),
        invalid_entry_ids,
        global_finding_rule: None,
    }
}

fn policy_target_is_current(
    live: &LiveIndex,
    units: &[AuthorityUnitState],
    target: &KnowledgePolicyTarget,
) -> bool {
    let Some(file) = live.files.get(&target.path) else {
        return false;
    };
    if file.content_hash != target.content_hash {
        return false;
    }
    let Some(range) = target.unit_byte_range.as_ref() else {
        return true;
    };
    let Some(unit) = units
        .iter()
        .find(|unit| unit.anchor.path == target.path && unit.anchor.byte_range == *range)
    else {
        return false;
    };
    let Some(bytes) = file.content.get(range.start as usize..range.end as usize) else {
        return false;
    };
    target.unit_hash.as_deref() == Some(crate::hash::digest_hex(bytes).as_str())
        && unit.anchor.content_hash == target.content_hash
}

fn policy_conflicts_with_native(
    units: &[AuthorityUnitState],
    entry: &KnowledgePolicyEntry,
) -> bool {
    units
        .iter()
        .filter(|unit| unit_matches_target(unit, &entry.target))
        .any(|unit| {
            (!matches!(unit.lifecycle_evidence, LifecycleEvidence::None)
                && unit.lifecycle != entry.lifecycle)
                || entry.authority_domain.is_some_and(|domain| {
                    !matches!(
                        unit.authority_domain_evidence,
                        AuthorityDomainEvidence::Unknown
                    ) && unit.authority_domain != domain
                })
        })
}

fn apply_policy(units: &mut [AuthorityUnitState], evaluation: &PolicyEvaluation) {
    let Some(policy) = evaluation.policy.as_ref() else {
        return;
    };
    let policy_is_fully_valid = evaluation.invalid_entry_ids.is_empty();
    for entry in &policy.entries {
        for unit_index in 0..units.len() {
            if !unit_matches_target(&units[unit_index], &entry.target) {
                continue;
            }
            if !policy_is_fully_valid {
                let finding = stable_finding_id(
                    &units[unit_index].anchor,
                    "policy-entry-invalid-v1",
                    &entry.entry_id,
                );
                units[unit_index].policy_finding_ids.push(finding);
                continue;
            }

            units[unit_index].lifecycle = entry.lifecycle;
            units[unit_index].lifecycle_evidence = LifecycleEvidence::PolicyEntry {
                entry_id: entry.entry_id.clone(),
            };
            if let Some(domain) = entry.authority_domain {
                units[unit_index].authority_domain = domain;
                units[unit_index].authority_domain_evidence =
                    AuthorityDomainEvidence::PolicyEntry {
                        entry_id: entry.entry_id.clone(),
                    };
            }
            units[unit_index].successor = entry.superseded_by.as_ref().and_then(|target| {
                units
                    .iter()
                    .find(|candidate| unit_matches_target(candidate, target))
                    .map(|candidate| candidate.anchor.clone())
            });
        }
    }
}

fn unit_matches_target(unit: &AuthorityUnitState, target: &KnowledgePolicyTarget) -> bool {
    unit.anchor.path == target.path
        && target
            .unit_byte_range
            .as_ref()
            .is_none_or(|range| unit.anchor.byte_range == *range)
}

fn policy_target_key(target: &KnowledgePolicyTarget) -> String {
    match &target.unit_byte_range {
        Some(range) => format!("{}:{}..{}", target.path, range.start, range.end),
        None => format!("{}:*", target.path),
    }
}

fn cyclic_policy_target_keys(
    entries: &[KnowledgePolicyEntry],
) -> std::collections::BTreeSet<String> {
    let edges: BTreeMap<String, String> = entries
        .iter()
        .filter_map(|entry| {
            entry.superseded_by.as_ref().map(|successor| {
                (
                    policy_target_key(&entry.target),
                    policy_target_key(successor),
                )
            })
        })
        .collect();
    let mut cyclic = std::collections::BTreeSet::new();
    for start in edges.keys() {
        let mut seen = std::collections::BTreeSet::new();
        let mut current = start.as_str();
        while let Some(next) = edges.get(current) {
            if !seen.insert(current.to_string()) {
                cyclic.extend(seen);
                break;
            }
            current = next;
        }
    }
    cyclic
}

fn facts_for_unit(
    unit: &AuthorityUnitState,
    bridge: &KnowledgeBridge,
    timeline: &DocumentTimeline,
) -> CodeEvidenceFacts {
    let mut facts = CodeEvidenceFacts::from_timeline(timeline);
    facts.coverage = bridge.coverage.clone();
    let mut linked_code_ids = std::collections::BTreeSet::new();
    let mut saw_link = false;
    for (index, link) in bridge.forward.iter().enumerate() {
        if link.evidence.path != unit.anchor.path
            || link.evidence.byte_range.start < unit.anchor.byte_range.start
            || link.evidence.byte_range.start >= unit.anchor.byte_range.end
        {
            continue;
        }
        saw_link = true;
        let rule_id = link.id.0.clone();
        match &link.resolution {
            BridgeResolution::ResolvedExact(anchor) => {
                facts.consistent_rule_ids.push(rule_id);
                linked_code_ids.insert(anchor.id.clone());
            }
            BridgeResolution::ResolvedDeclaredSet { matched_count, .. } if *matched_count > 0 => {
                facts.consistent_rule_ids.push(rule_id);
            }
            BridgeResolution::ResolvedDeclaredSet { .. }
            | BridgeResolution::Ambiguous { .. }
            | BridgeResolution::Missing => {
                if matches!(
                    link.evidence_kind,
                    BridgeEvidenceKind::SupportedStructuredValue { .. }
                ) {
                    match unit.authority_domain {
                        AuthorityDomain::CurrentImplementation => {
                            facts.deterministic_conflict_ids.push(rule_id)
                        }
                        AuthorityDomain::NormativeIntent
                        | AuthorityDomain::Decision
                        | AuthorityDomain::Governance => facts.implementation_gap_ids.push(rule_id),
                        _ => facts.suspected_conflict_ids.push(rule_id),
                    }
                } else {
                    facts
                        .broken_link_indices
                        .push(u32::try_from(index).unwrap_or(u32::MAX));
                }
            }
        }
    }
    if !saw_link {
        facts.not_applicable = true;
    }
    facts.relevant_code_change_count = u32::try_from(
        timeline
            .relevant_code_changes
            .iter()
            .filter(|change| {
                change.topologically_after_document == Some(true)
                    && linked_code_ids.contains(&change.anchor.id)
            })
            .count(),
    )
    .unwrap_or(u32::MAX);
    facts
}

fn unavailable_timeline() -> DocumentTimeline {
    let coverage = HistoryCoverage {
        complete_to_root: false,
        limitations: vec![HistoryLimit::Unavailable],
    };
    DocumentTimeline {
        filesystem_created: None,
        filesystem_modified: None,
        git_first_seen: None,
        git_last_touch: None,
        working_tree_changed: false,
        relevant_code_changes: Vec::new(),
        coverage,
    }
}

fn derive_proposal(
    lifecycle: KnowledgeLifecycle,
    authority_domain: AuthorityDomain,
    voice: KnowledgeVoice,
    successor: Option<&KnowledgeAnchor>,
    code_evidence: &CodeEvidenceSummary,
) -> RemediationProposal {
    let evidence_ids = summary_finding_ids(code_evidence);
    if lifecycle == KnowledgeLifecycle::Superseded
        && let Some(successor) = successor
    {
        return RemediationProposal {
            action: RemediationAction::MarkSuperseded {
                successor: successor.clone(),
            },
            confidence: EvidenceConfidence::Deterministic,
            evidence_ids,
            unmet_preconditions: Vec::new(),
        };
    }
    match voice {
        KnowledgeVoice::Current => RemediationProposal {
            action: RemediationAction::Keep,
            confidence: EvidenceConfidence::Deterministic,
            evidence_ids,
            unmet_preconditions: Vec::new(),
        },
        KnowledgeVoice::Intent
            if code_evidence.display == CodeEvidenceDisplay::ImplementationGap =>
        {
            RemediationProposal {
                action: RemediationAction::Update,
                confidence: EvidenceConfidence::StrongCandidate,
                evidence_ids,
                unmet_preconditions: vec![RemediationPrecondition::RequiresUserJudgment],
            }
        }
        KnowledgeVoice::HistoryOnly if authority_domain == AuthorityDomain::HistoricalRecord => {
            RemediationProposal {
                action: RemediationAction::Archive,
                confidence: EvidenceConfidence::StrongCandidate,
                evidence_ids,
                unmet_preconditions: vec![RemediationPrecondition::RequiresUserJudgment],
            }
        }
        _ => RemediationProposal {
            action: RemediationAction::NeedsReview,
            confidence: EvidenceConfidence::ReviewSignal,
            evidence_ids,
            unmet_preconditions: vec![RemediationPrecondition::RequiresUserJudgment],
        },
    }
}

fn is_reserved_suppression_record(record: &KnowledgeAuthorityRecord) -> bool {
    record.voice == KnowledgeVoice::Suppressed
        || matches!(
            record.lifecycle,
            KnowledgeLifecycle::Superseded
                | KnowledgeLifecycle::Archived
                | KnowledgeLifecycle::Historical
        )
}

fn summary_finding_ids(summary: &CodeEvidenceSummary) -> Vec<String> {
    let mut ids = Vec::new();
    ids.extend(summary.deterministic_conflict_ids.iter().cloned());
    ids.extend(summary.suspected_conflict_ids.iter().cloned());
    ids.extend(summary.implementation_gap_ids.iter().cloned());
    ids.extend(summary.review_signal_ids.iter().cloned());
    ids.sort();
    ids.dedup();
    ids
}

fn record_finding_ids(record: &KnowledgeAuthorityRecord) -> Vec<String> {
    let mut ids = summary_finding_ids(&record.code_evidence);
    ids.extend(record.proposal.evidence_ids.iter().cloned());
    ids.sort();
    ids.dedup();
    ids
}

fn stable_finding_id(anchor: &KnowledgeAnchor, rule_id: &str, kind: &str) -> String {
    let source = serde_json::to_vec(&anchor.source)
        .expect("SourceIdentity serialization is infallible for its closed data model");
    let mut bytes = Vec::with_capacity(
        source.len()
            + anchor.path.len()
            + anchor.content_hash.len()
            + rule_id.len()
            + kind.len()
            + 8,
    );
    bytes.extend_from_slice(&source);
    bytes.extend_from_slice(anchor.path.as_bytes());
    bytes.extend_from_slice(anchor.content_hash.as_bytes());
    bytes.extend_from_slice(&anchor.byte_range.start.to_le_bytes());
    bytes.extend_from_slice(rule_id.as_bytes());
    bytes.extend_from_slice(kind.as_bytes());
    crate::hash::digest_hex(&bytes)
}

fn authority_record_metadata_bytes(record: &KnowledgeAuthorityRecord) -> usize {
    record.unit.path.len()
        + record.unit.content_hash.len()
        + summary_finding_ids(&record.code_evidence)
            .iter()
            .map(String::len)
            .sum::<usize>()
        + 128
}

fn coverage_breaches(coverage: &DerivedCoverage) -> Vec<LimitBreach> {
    match coverage {
        DerivedCoverage::Complete => Vec::new(),
        DerivedCoverage::Truncated { breaches } => breaches.clone(),
    }
}

fn note_breach(breaches: &mut Vec<LimitBreach>, kind: DerivedLimitKind, omitted: u64) {
    breaches.push(LimitBreach { kind, omitted });
}

fn normalize_breaches(breaches: &mut Vec<LimitBreach>) {
    breaches.sort_by_key(|breach| breach.kind);
    let mut normalized: Vec<LimitBreach> = Vec::new();
    for breach in breaches.drain(..) {
        if let Some(last) = normalized
            .last_mut()
            .filter(|last| last.kind == breach.kind)
        {
            last.omitted = last.omitted.saturating_add(breach.omitted);
        } else {
            normalized.push(breach);
        }
    }
    *breaches = normalized;
}

impl Default for KnowledgeAuthorityView {
    fn default() -> Self {
        Self {
            source: None,
            source_version: None,
            content_generation: 0,
            records: Vec::new(),
            finding_index: BTreeMap::new(),
            policy_digest: crate::hash::digest_hex(&[]),
            policy_status: PolicyLedgerStatus::Absent,
            curation_eligible: false,
            skipped_suppression_ids: Vec::new(),
            coverage: DerivedCoverage::Complete,
            usage: DerivedResourceUsage::default(),
            versions: AuthorityVersions {
                authority_rule_version: AUTHORITY_RULE_VERSION,
                policy_version: KNOWLEDGE_POLICY_VERSION,
                secret_policy_version: crate::knowledge::SECRET_POLICY_VERSION,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{RepositoryId, SourceId, SourceLocation};
    use crate::live_index::{LiveIndex, SharedIndex};
    use std::fs;
    use std::path::Path;
    use tempfile::TempDir;

    fn source() -> SourceIdentity {
        SourceIdentity {
            repository_id: RepositoryId::new("repo"),
            source_id: SourceId::new("source"),
            location: SourceLocation::WorkingTree {
                worktree_id: "worktree".into(),
            },
        }
    }

    fn complete_history() -> HistoryCoverage {
        HistoryCoverage {
            complete_to_root: true,
            limitations: Vec::new(),
        }
    }

    fn facts() -> CodeEvidenceFacts {
        CodeEvidenceFacts {
            consistent_rule_ids: Vec::new(),
            broken_link_indices: Vec::new(),
            deterministic_conflict_ids: Vec::new(),
            suspected_conflict_ids: Vec::new(),
            implementation_gap_ids: Vec::new(),
            relevant_code_change_count: 0,
            review_signal_ids: Vec::new(),
            unresolved_semantics: false,
            not_applicable: false,
            coverage: DerivedCoverage::Complete,
        }
    }

    fn write(root: &Path, path: &str, content: &str) {
        let destination = root.join(path);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(destination, content).unwrap();
    }

    fn fixture(files: &[(&str, &str)]) -> (TempDir, SharedIndex) {
        let root = TempDir::new().unwrap();
        for (path, content) in files {
            write(root.path(), path, content);
        }
        let shared = LiveIndex::load(root.path()).unwrap();
        (root, shared)
    }

    fn build(shared: &SharedIndex, limits: AuthorityLimits) -> KnowledgeAuthorityView {
        let published = shared.published_generation();
        let temporal = AuthorityTemporalIndex {
            timelines: published
                .live
                .files
                .keys()
                .map(|path| {
                    (
                        path.clone(),
                        DocumentTimeline {
                            filesystem_created: None,
                            filesystem_modified: None,
                            git_first_seen: None,
                            git_last_touch: None,
                            working_tree_changed: false,
                            relevant_code_changes: Vec::new(),
                            coverage: complete_history(),
                        },
                    )
                })
                .collect(),
        };
        build_knowledge_authority(
            &published.live,
            published.source.as_deref().unwrap(),
            published.source_version.as_deref().unwrap(),
            published.content_generation,
            &published.bridge,
            &temporal,
            published.manifest.as_deref().unwrap().secret_policy_version,
            &limits,
        )
    }

    #[test]
    fn gate_h_authority_axes_and_display_contract_are_representable() {
        let lifecycle = KnowledgeLifecycle::Accepted;
        let domain = AuthorityDomain::NormativeIntent;
        let voice = KnowledgeVoice::Intent;
        let display = CodeEvidenceDisplay::ImplementationGap;

        assert_eq!(lifecycle, KnowledgeLifecycle::Accepted);
        assert_eq!(domain, AuthorityDomain::NormativeIntent);
        assert_eq!(voice, KnowledgeVoice::Intent);
        assert_eq!(display, CodeEvidenceDisplay::ImplementationGap);
    }

    #[test]
    fn aggregate_display_precedence_never_erases_stronger_or_parallel_facts() {
        let mut input = facts();
        input.consistent_rule_ids.push("consistent-1".into());
        input.broken_link_indices.push(7);
        input.deterministic_conflict_ids.push("conflict-1".into());
        input.implementation_gap_ids.push("gap-1".into());
        input.suspected_conflict_ids.push("suspect-1".into());
        input.relevant_code_change_count = 2;
        input.review_signal_ids.push("mtime-only".into());
        input.unresolved_semantics = true;

        let summary = summarize_code_evidence(input);

        assert_eq!(summary.display, CodeEvidenceDisplay::DeterministicConflict);
        assert_eq!(summary.broken_link_indices, [7]);
        assert_eq!(summary.implementation_gap_ids, ["gap-1"]);
        assert_eq!(summary.suspected_conflict_ids, ["suspect-1"]);
        assert_eq!(summary.relevant_code_change_count, 2);
        assert_eq!(summary.review_signal_ids, ["mtime-only"]);
        assert!(summary.unresolved_semantics);
    }

    #[test]
    fn clocks_are_review_signals_and_only_topological_code_change_is_relevant_change() {
        let timeline = DocumentTimeline {
            filesystem_created: Some(TimeEvidence {
                unix_seconds: Some(100),
                provenance: TimeProvenance::FilesystemBirth,
                coverage: complete_history(),
            }),
            filesystem_modified: Some(TimeEvidence {
                unix_seconds: Some(500),
                provenance: TimeProvenance::FilesystemModified,
                coverage: complete_history(),
            }),
            git_first_seen: None,
            git_last_touch: None,
            working_tree_changed: false,
            relevant_code_changes: vec![
                CodeChangeEvidence {
                    anchor: CodeAnchor {
                        source: source(),
                        content_generation: 4,
                        id: CodeAnchorId::File {
                            path: "src/lib.rs".into(),
                        },
                        content_hash: "hash-lib".into(),
                        line_range: 1..2,
                    },
                    commit_id: Some("newer".into()),
                    unix_seconds: Some(400),
                    topologically_after_document: None,
                    rule_id: "clock-only".into(),
                },
                CodeChangeEvidence {
                    anchor: CodeAnchor {
                        source: source(),
                        content_generation: 4,
                        id: CodeAnchorId::File {
                            path: "src/main.rs".into(),
                        },
                        content_hash: "hash-main".into(),
                        line_range: 1..2,
                    },
                    commit_id: Some("descendant".into()),
                    unix_seconds: Some(300),
                    topologically_after_document: Some(true),
                    rule_id: "topology-proven".into(),
                },
            ],
            coverage: complete_history(),
        };

        let summary = summarize_code_evidence(CodeEvidenceFacts::from_timeline(&timeline));

        assert_eq!(summary.relevant_code_change_count, 1);
        assert_eq!(
            summary.display,
            CodeEvidenceDisplay::RelevantCodeChangedSinceDocument
        );
        assert!(
            summary
                .review_signal_ids
                .iter()
                .any(|id| id == "clock-only")
        );
        assert!(
            summary
                .review_signal_ids
                .iter()
                .any(|id| id == "filesystem-time-hint")
        );
    }

    #[test]
    fn temporal_limit_dirty_and_clock_skew_matrix_never_upgrades_hints_to_proof() {
        for limitation in [
            HistoryLimit::Shallow,
            HistoryLimit::WindowLimited,
            HistoryLimit::RenameFollowLimited,
            HistoryLimit::DivergentHistory,
            HistoryLimit::WorkingTreeOnly,
            HistoryLimit::Unavailable,
        ] {
            let timeline = DocumentTimeline {
                filesystem_created: None,
                filesystem_modified: None,
                git_first_seen: None,
                git_last_touch: None,
                working_tree_changed: false,
                relevant_code_changes: Vec::new(),
                coverage: HistoryCoverage {
                    complete_to_root: false,
                    limitations: vec![limitation],
                },
            };
            let summary = summarize_code_evidence(CodeEvidenceFacts::from_timeline(&timeline));
            assert_eq!(summary.display, CodeEvidenceDisplay::ReviewDue);
            assert_eq!(summary.relevant_code_change_count, 0);
            assert!(
                summary
                    .review_signal_ids
                    .iter()
                    .any(|id| id == "temporal-coverage-incomplete")
            );
        }

        let dirty = DocumentTimeline {
            filesystem_created: None,
            filesystem_modified: None,
            git_first_seen: None,
            git_last_touch: None,
            working_tree_changed: true,
            relevant_code_changes: Vec::new(),
            coverage: complete_history(),
        };
        let dirty_summary = summarize_code_evidence(CodeEvidenceFacts::from_timeline(&dirty));
        assert_eq!(dirty_summary.display, CodeEvidenceDisplay::ReviewDue);
        assert!(
            dirty_summary
                .review_signal_ids
                .iter()
                .any(|id| id == "working-tree-changed")
        );

        let skewed = DocumentTimeline {
            filesystem_created: Some(TimeEvidence {
                unix_seconds: Some(500),
                provenance: TimeProvenance::FilesystemBirth,
                coverage: complete_history(),
            }),
            filesystem_modified: Some(TimeEvidence {
                unix_seconds: Some(100),
                provenance: TimeProvenance::FilesystemModified,
                coverage: complete_history(),
            }),
            git_first_seen: None,
            git_last_touch: None,
            working_tree_changed: false,
            relevant_code_changes: Vec::new(),
            coverage: complete_history(),
        };
        let skewed_summary = summarize_code_evidence(CodeEvidenceFacts::from_timeline(&skewed));
        assert_eq!(skewed_summary.display, CodeEvidenceDisplay::ReviewDue);
        assert_eq!(skewed_summary.relevant_code_change_count, 0);
        assert!(
            skewed_summary
                .review_signal_ids
                .iter()
                .any(|id| id == "clock-skew-detected")
        );
    }

    #[test]
    fn lifecycle_domain_evidence_and_voice_are_independent() {
        let consistent = summarize_code_evidence(CodeEvidenceFacts {
            consistent_rule_ids: vec!["checked".into()],
            ..facts()
        });
        let conflict = summarize_code_evidence(CodeEvidenceFacts {
            deterministic_conflict_ids: vec!["mismatch".into()],
            ..facts()
        });
        let gap = summarize_code_evidence(CodeEvidenceFacts {
            implementation_gap_ids: vec!["intent-diverged".into()],
            ..facts()
        });

        assert_eq!(
            derive_voice(
                KnowledgeLifecycle::Accepted,
                AuthorityDomain::CurrentImplementation,
                &consistent,
            ),
            KnowledgeVoice::Current
        );
        assert_eq!(
            derive_voice(
                KnowledgeLifecycle::Accepted,
                AuthorityDomain::CurrentImplementation,
                &conflict,
            ),
            KnowledgeVoice::Suppressed
        );
        assert_eq!(
            derive_voice(
                KnowledgeLifecycle::Accepted,
                AuthorityDomain::NormativeIntent,
                &gap,
            ),
            KnowledgeVoice::Intent
        );
        assert_eq!(
            derive_voice(
                KnowledgeLifecycle::Archived,
                AuthorityDomain::HistoricalRecord,
                &consistent,
            ),
            KnowledgeVoice::HistoryOnly
        );
        assert_eq!(KnowledgeLifecycle::Accepted, KnowledgeLifecycle::Accepted);
    }

    #[test]
    fn versioned_policy_parser_accepts_exact_whole_file_and_half_open_unit_targets() {
        let policy = parse_knowledge_policy(
            br#"
version = 1

[[entries]]
entry_id = "archive-old"
lifecycle = "superseded"
authority_domain = "historical_record"
justification_code = "duplicate-confirmed"

[entries.target]
path = "docs/old.md"
content_hash = "whole-hash"
unit_byte_range = [8, 42]
unit_hash = "unit-hash"

[entries.superseded_by]
path = "docs/current.md"
content_hash = "successor-hash"

[[entries.evidence]]
rule_id = "policy-exact-successor"
code_path = "src/lib.rs"
"#,
        )
        .unwrap();

        assert_eq!(policy.version, KNOWLEDGE_POLICY_VERSION);
        assert_eq!(policy.entries.len(), 1);
        let entry = &policy.entries[0];
        assert_eq!(entry.entry_id, "archive-old");
        assert_eq!(entry.lifecycle, KnowledgeLifecycle::Superseded);
        assert_eq!(
            entry.authority_domain,
            Some(AuthorityDomain::HistoricalRecord)
        );
        assert_eq!(entry.target.unit_byte_range, Some(8..42));
        assert_eq!(entry.target.unit_hash.as_deref(), Some("unit-hash"));
        assert_eq!(
            entry
                .superseded_by
                .as_ref()
                .map(|target| target.path.as_str()),
            Some("docs/current.md")
        );
        assert_eq!(entry.evidence.len(), 1);
    }

    #[test]
    fn malformed_unsupported_or_unsafe_policy_is_typed_and_never_partially_accepted() {
        assert!(matches!(
            parse_knowledge_policy(b"version = ["),
            Err(PolicyParseError::Malformed)
        ));
        assert!(matches!(
            parse_knowledge_policy(b"version = 2\nentries = []"),
            Err(PolicyParseError::UnsupportedVersion { found: Some(2) })
        ));
        assert!(matches!(
            parse_knowledge_policy(
                br#"
version = 1
[[entries]]
entry_id = "escape"
lifecycle = "archived"
justification_code = "unsafe"
[entries.target]
path = "../outside.md"
content_hash = "hash"
"#,
            ),
            Err(PolicyParseError::InvalidField { .. })
        ));
    }

    #[test]
    fn mixed_units_keep_independent_domain_evidence_lifecycle_and_voice() {
        let (_root, shared) = fixture(&[
            (
                "docs/mixed.md",
                "# Current implementation\nstatus: active\ncode_path = \"src/lib.rs\"\n\n# Broken current implementation\nstatus: active\ncode_path = \"src/missing.rs\"\n\n# Intent\ncode_path = \"src/future.rs\"\n",
            ),
            ("src/lib.rs", "pub fn ready() {}\n"),
        ]);

        let view = build(&shared, AuthorityLimits::default());

        assert_eq!(view.records.len(), 3);
        let current = view
            .records
            .iter()
            .find(|record| record.unit.byte_range.start == 0)
            .unwrap();
        assert_eq!(
            current.authority_domain,
            AuthorityDomain::CurrentImplementation
        );
        assert_eq!(
            current.code_evidence.display,
            CodeEvidenceDisplay::ConsistentForCheckedClaims
        );
        assert_eq!(current.voice, KnowledgeVoice::Current);

        let broken = view
            .records
            .iter()
            .find(|record| {
                record.authority_domain == AuthorityDomain::CurrentImplementation
                    && record.code_evidence.display == CodeEvidenceDisplay::DeterministicConflict
            })
            .unwrap();
        assert_eq!(broken.lifecycle, KnowledgeLifecycle::Active);
        assert_eq!(broken.voice, KnowledgeVoice::Suppressed);

        let intent = view
            .records
            .iter()
            .find(|record| record.authority_domain == AuthorityDomain::NormativeIntent)
            .unwrap();
        assert_eq!(
            intent.code_evidence.display,
            CodeEvidenceDisplay::ImplementationGap
        );
        assert_eq!(intent.voice, KnowledgeVoice::Intent);
        assert_eq!(view, build(&shared, AuthorityLimits::default()));
    }

    /// A unit with no declared status and no archive path has no lifecycle
    /// evidence, so the derived lifecycle must be `Unknown` rather than an
    /// invented `Active`.
    ///
    /// Paired with the declared case in the same test: `status: active` still
    /// yields `Active`, so this is not a guard that refuses everything. Without
    /// that pairing, returning `Unknown` unconditionally would satisfy the
    /// negative assertion perfectly.
    #[test]
    fn lifecycle_without_evidence_is_unknown_not_active() {
        let (_root, shared) = fixture(&[
            (
                "docs/undeclared.md",
                "# Current implementation\ncode_path = \"src/lib.rs\"\n",
            ),
            (
                "docs/declared.md",
                "# Current implementation\nstatus: active\ncode_path = \"src/lib.rs\"\n",
            ),
            ("src/lib.rs", "pub fn ready() {}\n"),
        ]);

        let view = build(&shared, AuthorityLimits::default());

        let undeclared = view
            .records
            .iter()
            .find(|record| record.unit.path.contains("undeclared"))
            .expect("the undeclared unit is indexed");
        assert_eq!(
            undeclared.lifecycle,
            KnowledgeLifecycle::Unknown,
            "a unit with no declared status must not be reported as Active"
        );
        assert_eq!(
            undeclared.voice,
            KnowledgeVoice::Unknown,
            "an unevidenced lifecycle must not be consumed as voice=current"
        );
        assert!(
            matches!(undeclared.lifecycle_evidence, LifecycleEvidence::None),
            "an Unknown lifecycle must not cite evidence it does not have"
        );

        let declared = view
            .records
            .iter()
            .find(|record| record.unit.path.contains("declared.md"))
            .expect("the declared unit is indexed");
        assert_eq!(
            declared.lifecycle,
            KnowledgeLifecycle::Active,
            "a declared status must still be honoured, or this guard is vacuous"
        );
    }

    #[test]
    fn code_authority_fixture_matrix_never_erases_intent_governance_operations_or_history() {
        let (_root, shared) = fixture(&[
            (
                "docs/current.md",
                "# Current implementation\nstatus: active\ncode_path = \"src/lib.rs\"\n",
            ),
            (
                "docs/current-broken.md",
                "# Current implementation\ncode_path = \"src/missing.rs\"\n",
            ),
            (
                "docs/intent.md",
                "# Intent\ncode_path = \"src/missing.rs\"\n",
            ),
            (
                "docs/decision.md",
                "# Decision\ncode_path = \"src/missing.rs\"\n",
            ),
            (
                "docs/governance.md",
                "# Governance\ncode_path = \"src/missing.rs\"\n",
            ),
            (
                "docs/ops/runbook.md",
                "# Runbook\nstatus: active\ncode_path = \"src/missing.rs\"\n",
            ),
            (
                "docs/archive/history.md",
                "# History\ncode_path = \"src/missing.rs\"\n",
            ),
            ("src/lib.rs", "pub fn live() {}\n"),
        ]);

        let view = build(&shared, AuthorityLimits::default());
        let record = |path: &str| {
            view.records
                .iter()
                .find(|record| record.unit.path == path)
                .unwrap()
        };

        assert_eq!(record("docs/current.md").voice, KnowledgeVoice::Current);
        assert_eq!(
            record("docs/current-broken.md").voice,
            KnowledgeVoice::Suppressed
        );
        for (path, domain) in [
            ("docs/intent.md", AuthorityDomain::NormativeIntent),
            ("docs/decision.md", AuthorityDomain::Decision),
            ("docs/governance.md", AuthorityDomain::Governance),
        ] {
            let record = record(path);
            assert_eq!(record.authority_domain, domain);
            assert_eq!(
                record.code_evidence.display,
                CodeEvidenceDisplay::ImplementationGap
            );
            assert_eq!(record.voice, KnowledgeVoice::Intent);
        }
        let operations = record("docs/ops/runbook.md");
        assert_eq!(operations.authority_domain, AuthorityDomain::Operations);
        assert_eq!(
            operations.code_evidence.display,
            CodeEvidenceDisplay::SuspectedConflict
        );
        assert_eq!(operations.voice, KnowledgeVoice::NeedsReview);

        let history = record("docs/archive/history.md");
        assert_eq!(history.authority_domain, AuthorityDomain::HistoricalRecord);
        assert_eq!(history.lifecycle, KnowledgeLifecycle::Archived);
        assert_eq!(history.voice, KnowledgeVoice::HistoryOnly);
        assert_eq!(
            view.records
                .iter()
                .filter(|record| record.voice == KnowledgeVoice::Suppressed)
                .count(),
            1,
            "only a deterministic current-implementation conflict may be suppressed"
        );
    }

    #[test]
    fn stale_or_conflicting_policy_loses_suppression_while_valid_exact_policy_controls_voice() {
        let document = "# Notes\nThe current path is stable.\n";
        let (root, shared) = fixture(&[("docs/current.md", document)]);
        let document_hash = shared
            .read()
            .files
            .get("docs/current.md")
            .unwrap()
            .content_hash
            .clone();
        write(
            root.path(),
            ".symforge-knowledge.toml",
            &format!(
                "version = 1\n[[entries]]\nentry_id = \"archive\"\nlifecycle = \"archived\"\nauthority_domain = \"historical_record\"\njustification_code = \"confirmed\"\n[entries.target]\npath = \"docs/current.md\"\ncontent_hash = \"{document_hash}\"\n"
            ),
        );
        shared.reload(root.path()).unwrap();
        let valid = build(&shared, AuthorityLimits::default());
        assert_eq!(valid.policy_status, PolicyLedgerStatus::Valid);
        assert!(valid.curation_eligible);
        assert!(
            valid
                .records
                .iter()
                .all(|record| record.voice == KnowledgeVoice::HistoryOnly)
        );

        write(
            root.path(),
            ".symforge-knowledge.toml",
            "version = 1\n[[entries]]\nentry_id = \"stale\"\nlifecycle = \"archived\"\njustification_code = \"old\"\n[entries.target]\npath = \"docs/current.md\"\ncontent_hash = \"stale-hash\"\n",
        );
        shared.reload(root.path()).unwrap();
        let stale = build(&shared, AuthorityLimits::default());
        assert_eq!(stale.policy_status, PolicyLedgerStatus::InvalidEntries);
        assert!(!stale.curation_eligible);
        assert!(
            stale
                .records
                .iter()
                .all(|record| record.voice != KnowledgeVoice::HistoryOnly)
        );

        write(
            root.path(),
            "docs/current.md",
            "# Current implementation\nstatus: active\n",
        );
        shared.reload(root.path()).unwrap();
        let active_hash = shared
            .read()
            .files
            .get("docs/current.md")
            .unwrap()
            .content_hash
            .clone();
        write(
            root.path(),
            ".symforge-knowledge.toml",
            &format!(
                "version = 1\n[[entries]]\nentry_id = \"conflict\"\nlifecycle = \"archived\"\njustification_code = \"conflicts-native\"\n[entries.target]\npath = \"docs/current.md\"\ncontent_hash = \"{active_hash}\"\n"
            ),
        );
        shared.reload(root.path()).unwrap();
        let conflict = build(&shared, AuthorityLimits::default());
        assert_eq!(conflict.policy_status, PolicyLedgerStatus::InvalidEntries);
        assert!(!conflict.curation_eligible);
        assert!(
            conflict
                .records
                .iter()
                .all(|record| record.lifecycle == KnowledgeLifecycle::Active)
        );
    }

    #[test]
    fn supersession_cycles_and_stale_unit_hashes_disable_all_policy_authority() {
        let (cycle_root, cycle_shared) =
            fixture(&[("docs/a.md", "# A\nold A\n"), ("docs/b.md", "# B\nold B\n")]);
        let a_hash = cycle_shared.read().files["docs/a.md"].content_hash.clone();
        let b_hash = cycle_shared.read().files["docs/b.md"].content_hash.clone();
        write(
            cycle_root.path(),
            ".symforge-knowledge.toml",
            &format!(
                "version = 1\n\
                 [[entries]]\nentry_id = \"a-to-b\"\nlifecycle = \"superseded\"\njustification_code = \"cycle-a\"\n\
                 [entries.target]\npath = \"docs/a.md\"\ncontent_hash = \"{a_hash}\"\n\
                 [entries.superseded_by]\npath = \"docs/b.md\"\ncontent_hash = \"{b_hash}\"\n\
                 [[entries]]\nentry_id = \"b-to-a\"\nlifecycle = \"superseded\"\njustification_code = \"cycle-b\"\n\
                 [entries.target]\npath = \"docs/b.md\"\ncontent_hash = \"{b_hash}\"\n\
                 [entries.superseded_by]\npath = \"docs/a.md\"\ncontent_hash = \"{a_hash}\"\n"
            ),
        );
        cycle_shared.reload(cycle_root.path()).unwrap();
        let cycle = build(&cycle_shared, AuthorityLimits::default());
        assert_eq!(cycle.policy_status, PolicyLedgerStatus::InvalidEntries);
        assert!(!cycle.curation_eligible);
        assert!(cycle.records.iter().all(|record| {
            !matches!(
                record.voice,
                KnowledgeVoice::HistoryOnly | KnowledgeVoice::Suppressed
            )
        }));

        let (unit_root, unit_shared) = fixture(&[("docs/unit.md", "# Unit\nbody\n")]);
        let ungoverned = build(&unit_shared, AuthorityLimits::default());
        let unit = ungoverned.records.first().unwrap().unit.clone();
        write(
            unit_root.path(),
            ".symforge-knowledge.toml",
            &format!(
                "version = 1\n[[entries]]\nentry_id = \"stale-unit\"\nlifecycle = \"archived\"\n\
                 justification_code = \"stale-unit-hash\"\n[entries.target]\npath = \"{}\"\n\
                 content_hash = \"{}\"\nunit_byte_range = [{}, {}]\nunit_hash = \"stale-unit-hash\"\n",
                unit.path, unit.content_hash, unit.byte_range.start, unit.byte_range.end,
            ),
        );
        unit_shared.reload(unit_root.path()).unwrap();
        let stale_unit = build(&unit_shared, AuthorityLimits::default());
        assert_eq!(stale_unit.policy_status, PolicyLedgerStatus::InvalidEntries);
        assert!(!stale_unit.curation_eligible);
        assert!(stale_unit.records.iter().all(|record| {
            !matches!(
                record.voice,
                KnowledgeVoice::HistoryOnly | KnowledgeVoice::Suppressed
            )
        }));
        assert_eq!(
            unit_shared.read().files["docs/unit.md"].content,
            b"# Unit\nbody\n",
            "invalid policy must not hide raw safe knowledge bytes"
        );
    }

    #[test]
    fn authority_budget_reserves_suppression_and_fails_closed_when_it_cannot_fit() {
        let document = "# Current implementation\ncode_path = \"src/missing.rs\"\n\n# Intent\ncode_path = \"src/future.rs\"\n";
        let (_root, shared) = fixture(&[("docs/mixed.md", document)]);

        let reserved = build(
            &shared,
            AuthorityLimits {
                max_authority_records: 1,
                ..AuthorityLimits::default()
            },
        );
        assert_eq!(reserved.records.len(), 1);
        assert_eq!(reserved.records[0].voice, KnowledgeVoice::Suppressed);
        assert!(matches!(
            reserved.coverage,
            DerivedCoverage::Truncated { .. }
        ));
        assert!(reserved.skipped_suppression_ids.is_empty());
        assert!(shared.read().files.contains_key("docs/mixed.md"));

        let closed = build(
            &shared,
            AuthorityLimits {
                max_authority_records: 0,
                ..AuthorityLimits::default()
            },
        );
        assert!(closed.records.is_empty());
        assert_eq!(closed.skipped_suppression_ids.len(), 1);
        assert!(matches!(closed.coverage, DerivedCoverage::Truncated { .. }));
    }

    #[test]
    fn authority_is_published_in_one_bundle_and_stale_prepared_work_is_rejected() {
        let (root, shared) = fixture(&[
            (
                "docs/current.md",
                "# Current implementation\ncode_path = \"src/lib.rs\"\n",
            ),
            ("src/lib.rs", "pub fn first() {}\n"),
        ]);
        let initial = shared.published_generation();
        assert_eq!(initial.authority.source.as_ref(), initial.source.as_deref());
        assert_eq!(
            initial.authority.content_generation,
            initial.content_generation
        );
        assert!(!initial.authority.records.is_empty());

        let stale = shared.prepare_authority_rebuild();
        write(root.path(), "src/lib.rs", "pub fn second() {}\n");
        shared.reload(root.path()).unwrap();
        assert!(!shared.publish_prepared_authority(stale));

        let latest = shared.prepare_authority_rebuild();
        let before = shared.published_generation();
        assert!(shared.publish_prepared_authority(latest));
        let after = shared.published_generation();
        assert!(after.publication_generation > before.publication_generation);
        assert_eq!(after.content_generation, before.content_generation);
        assert_eq!(
            after
                .manifest
                .as_ref()
                .map(|manifest| manifest.digest.as_str()),
            before
                .manifest
                .as_ref()
                .map(|manifest| manifest.digest.as_str())
        );
        assert_eq!(
            after.authority.source_version,
            after.source_version.as_deref().cloned()
        );
    }

    fn commit(repo: &git2::Repository, message: &str) -> git2::Oid {
        let signature = git2::Signature::now("SymForge Test", "test@example.invalid").unwrap();
        let mut index = repo.index().unwrap();
        index
            .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
            .unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let parent = repo
            .head()
            .ok()
            .and_then(|head| head.target())
            .and_then(|oid| repo.find_commit(oid).ok());
        match parent.as_ref() {
            Some(parent) => repo
                .commit(
                    Some("HEAD"),
                    &signature,
                    &signature,
                    message,
                    &tree,
                    &[parent],
                )
                .unwrap(),
            None => repo
                .commit(Some("HEAD"), &signature, &signature, message, &tree, &[])
                .unwrap(),
        }
    }

    #[test]
    fn bytes_identical_commit_rejects_old_authority_target_and_converges_on_new_tip() {
        let root = TempDir::new().unwrap();
        write(
            root.path(),
            "docs/current.md",
            "# Current implementation\ncode_path = \"src/lib.rs\"\n",
        );
        write(root.path(), "src/lib.rs", "pub fn stable() {}\n");
        let repo = git2::Repository::init(root.path()).unwrap();
        let first = commit(&repo, "first");
        let shared = LiveIndex::load(root.path()).unwrap();
        let old = shared.published_generation();
        let old_content_generation = old.content_generation;
        let old_manifest_digest = old.manifest.as_ref().unwrap().digest.clone();
        let stale = shared.prepare_authority_rebuild();

        let second = commit(&repo, "bytes-identical second");
        assert_ne!(first, second);
        assert!(shared.refresh_source_metadata());
        let refreshed = shared.published_generation();
        assert_eq!(refreshed.content_generation, old_content_generation);
        assert_eq!(
            refreshed.manifest.as_ref().unwrap().digest,
            old_manifest_digest
        );
        assert_ne!(refreshed.source_version, old.source_version);
        assert_eq!(
            refreshed.authority.source_version,
            refreshed.source_version.as_deref().cloned()
        );
        assert!(!shared.publish_prepared_authority(stale));

        let latest = shared.prepare_authority_rebuild();
        assert!(shared.publish_prepared_authority(latest));
        let accepted = shared.published_generation();
        assert_eq!(accepted.content_generation, old_content_generation);
        assert_eq!(
            accepted.authority.source_version,
            accepted.source_version.as_deref().cloned()
        );
        assert_eq!(
            accepted
                .authority
                .source_version
                .as_ref()
                .and_then(|version| version.commit.as_deref()),
            accepted
                .source_version
                .as_deref()
                .and_then(|version| version.commit.as_deref())
        );
    }
}
