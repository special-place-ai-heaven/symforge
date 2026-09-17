//! Server-independent typed knowledge retrieval.
//!
//! Extraction, ranking, limit, and withheld accounting live here so MCP
//! formatting and a future embed handle share one seam. This module never
//! renders MCP strings and never re-parses Markdown: heading paths are an
//! on-demand projection over already-indexed section symbols.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::ops::Range;

use crate::domain::{CoverageStatus, FreshnessStatus, LanguageId, SourceResponseEnvelope};
use crate::knowledge::{guard_hit, guard_query, project_markdown_sections};

use super::knowledge_authority::{
    AuthorityDomain, CodeEvidenceDisplay, KnowledgeAuthorityRecord, KnowledgeLifecycle,
    KnowledgeVoice,
};
use super::knowledge_bridge::{
    BridgeEvidenceKind, BridgeResolution, DerivedCoverage, KnowledgeAnchor,
};
use super::search::PathScope;
use super::store::{PublishedGeneration, PublishedIndexStatus};

const MAX_LIMIT: usize = 100;
const MAX_IDS_PER_HIT: usize = 8;
const MAX_BRIDGE_PREVIEWS_PER_HIT: usize = 4;
/// Excerpt bound in Unicode CHARACTERS, not bytes (SIFT-WS1).
pub(crate) const EXCERPT_MAX_CHARS: usize = 240;

/// Retrieval voice filter. Mirrors the MCP `authority_scope` vocabulary
/// without taking a protocol dependency.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KnowledgeRetrieveAuthorityScope {
    Default,
    Current,
    Intent,
    History,
    All,
}

/// Why a candidate was withheld from the result. Neutral on purpose:
/// [`crate::domain::MetadataOnlyReason::SensitivePath`] and
/// [`crate::domain::MetadataOnlyReason::SensitiveContent`] both collapse here
/// so the seam never discloses which detector fired.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KnowledgeWithheldReason {
    PolicyWithheld,
}

/// Why a lane could not be searched. Protocol maps these to the frozen
/// readiness strings; the seam does not format MCP output.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KnowledgeLaneReadiness {
    IndexScoutingOrVerifying,
    NoValidSource,
    EnvelopeUnavailable,
    EvidenceWithheld,
}

/// Why a successful search produced zero hits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KnowledgeRetrieveAbsence {
    QueryTooWeak,
    EvidenceWithheld,
    EvidenceNoncurrent,
    NoEvidenceDegraded,
    NoEvidenceComplete,
}

impl KnowledgeRetrieveAbsence {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::QueryTooWeak => "query_too_weak",
            Self::EvidenceWithheld => "evidence_withheld",
            Self::EvidenceNoncurrent => "evidence_noncurrent",
            Self::NoEvidenceDegraded => "no_evidence_degraded",
            Self::NoEvidenceComplete => "no_evidence_complete",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KnowledgeRetrieveError {
    SensitiveQuery,
    SensitivePathPrefix,
    EmptyQuery,
    InvalidPathPrefix,
}

/// Normalized retrieval request. `phrase` is lowercased; `terms` are the
/// significant tokens used for matching; `path_prefix` is repository-relative
/// without a trailing slash.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnowledgeRetrieveRequest {
    pub phrase: String,
    pub terms: Vec<String>,
    pub path_prefix: Option<String>,
    pub authority_scope: KnowledgeRetrieveAuthorityScope,
    pub limit: usize,
}

impl KnowledgeRetrieveRequest {
    pub fn parse(
        query: &str,
        path_prefix: Option<&str>,
        authority_scope: KnowledgeRetrieveAuthorityScope,
        limit: usize,
    ) -> Result<Self, KnowledgeRetrieveError> {
        guard_query(query).map_err(|_| KnowledgeRetrieveError::SensitiveQuery)?;
        if path_prefix.is_some_and(|path| guard_query(path).is_err()) {
            return Err(KnowledgeRetrieveError::SensitivePathPrefix);
        }
        let phrase = query.trim();
        if phrase.is_empty() {
            return Err(KnowledgeRetrieveError::EmptyQuery);
        }
        let path_prefix = normalize_knowledge_path_prefix(path_prefix)?;
        Ok(Self {
            phrase: phrase.to_lowercase(),
            terms: significant_terms(phrase),
            path_prefix,
            authority_scope,
            limit: limit.clamp(1, MAX_LIMIT),
        })
    }
}

/// One labeled published generation. Caller owns source-scope selection and
/// label assignment (`current` / `worktree:<id>` / `ref:<name>`). Lane order
/// is source precedence.
#[derive(Clone, Copy)]
pub struct KnowledgeRetrieveLane<'a> {
    pub generation: &'a PublishedGeneration,
    pub label: &'a str,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct KnowledgeRetrieveFilteredCounts {
    pub current: usize,
    pub intent: usize,
    pub history_only: usize,
    pub suppressed: usize,
    pub review_required: usize,
    pub unknown: usize,
}

impl KnowledgeRetrieveFilteredCounts {
    fn saturating_add_from(&mut self, other: &Self) {
        self.current = self.current.saturating_add(other.current);
        self.intent = self.intent.saturating_add(other.intent);
        self.history_only = self.history_only.saturating_add(other.history_only);
        self.suppressed = self.suppressed.saturating_add(other.suppressed);
        self.review_required = self.review_required.saturating_add(other.review_required);
        self.unknown = self.unknown.saturating_add(other.unknown);
    }

    fn any(self) -> bool {
        self.current > 0
            || self.intent > 0
            || self.history_only > 0
            || self.suppressed > 0
            || self.review_required > 0
            || self.unknown > 0
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KnowledgeRetrieveDerived {
    pub authority_rule_version: u32,
    pub policy_version: u32,
    pub secret_policy_version: u32,
    pub bridge_coverage: DerivedCoverage,
    pub authority_coverage: DerivedCoverage,
}

impl KnowledgeRetrieveDerived {
    fn capture(generation: &PublishedGeneration) -> Self {
        Self {
            authority_rule_version: generation.authority.versions.authority_rule_version,
            policy_version: generation.authority.versions.policy_version,
            secret_policy_version: generation.authority.versions.secret_policy_version,
            bridge_coverage: generation.bridge.coverage.clone(),
            authority_coverage: generation.authority.coverage.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnowledgeRetrieveAuthority {
    pub lifecycle: KnowledgeLifecycle,
    pub authority_domain: AuthorityDomain,
    pub code_evidence: CodeEvidenceDisplay,
    pub voice: KnowledgeVoice,
    pub finding_ids: Vec<String>,
    pub finding_ids_omitted: usize,
    pub provenance_ids: Vec<String>,
    pub provenance_ids_omitted: usize,
    pub coverage: DerivedCoverage,
}

/// Typed code-relationship evidence for one hit. Protocol formats
/// [`Self::preview_token`] for the frozen MCP `bridge_previews` field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnowledgeRelationshipEvidence {
    pub link_id: String,
    pub kind: BridgeEvidenceKind,
    pub resolution: BridgeResolution,
}

impl KnowledgeRelationshipEvidence {
    pub fn preview_token(&self) -> String {
        format!(
            "{}:{}:{}",
            self.link_id,
            bridge_evidence_kind_label(&self.kind),
            bridge_resolution_preview(&self.resolution)
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnowledgeRetrieveHit {
    pub source_label: String,
    pub path: String,
    pub line: u32,
    pub line_range: Range<u32>,
    pub heading_path: Vec<String>,
    pub preview: String,
    pub content_hash: String,
    pub publication_generation: u64,
    pub content_generation: u64,
    pub authority: KnowledgeRetrieveAuthority,
    pub relationship_evidence: Vec<KnowledgeRelationshipEvidence>,
    pub relationship_evidence_omitted: usize,
    pub(crate) source_precedence: usize,
    pub(crate) exact_phrase: bool,
    pub(crate) heading_match: bool,
    pub(crate) distinct_term_count: usize,
    pub(crate) unit_start: u32,
    pub(crate) unit_len: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnowledgeRetrieveSource {
    pub label: String,
    pub envelope: Option<SourceResponseEnvelope>,
    pub withheld_count: usize,
    pub filtered: KnowledgeRetrieveFilteredCounts,
    pub readiness: Option<KnowledgeLaneReadiness>,
    pub degraded: bool,
    pub derived: KnowledgeRetrieveDerived,
    pub publication_generation: u64,
    pub content_generation: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnowledgeRetrieveResult {
    pub hits: Vec<KnowledgeRetrieveHit>,
    pub sources: Vec<KnowledgeRetrieveSource>,
    pub withheld_count: usize,
    pub withheld_reasons: Vec<KnowledgeWithheldReason>,
    pub truncated: bool,
    pub overflow: usize,
    pub filtered: KnowledgeRetrieveFilteredCounts,
    pub absence: Option<KnowledgeRetrieveAbsence>,
    pub degraded: bool,
}

struct LaneHits {
    source: KnowledgeRetrieveSource,
    hits: Vec<KnowledgeRetrieveHit>,
}

/// Retrieve typed knowledge hits from already-selected published lanes.
///
/// Ranking tuple is unchanged from Gate I / SIFT-WS0: exact phrase, heading
/// match, distinct-term coverage, source precedence, then path/line/unit
/// start. `limit` applies once across every lane.
pub fn retrieve_knowledge(
    lanes: &[KnowledgeRetrieveLane<'_>],
    request: &KnowledgeRetrieveRequest,
) -> KnowledgeRetrieveResult {
    let sources: Vec<LaneHits> = lanes
        .iter()
        .map(|lane| extract_lane(lane.generation, lane.label, request))
        .collect();
    compose(sources, request)
}

pub(crate) fn significant_terms(query: &str) -> Vec<String> {
    let mut terms = BTreeSet::new();
    for token in query
        .split(|character: char| !character.is_alphanumeric() && character != '_')
        .map(str::trim)
        .filter(|token| !token.is_empty())
    {
        let token = token.to_lowercase();
        if token.len() < 2 || is_stopword(&token) {
            continue;
        }
        terms.insert(token);
        if terms.len() == 32 {
            break;
        }
    }
    terms.into_iter().collect()
}

pub(crate) fn normalize_knowledge_path_prefix(
    input: Option<&str>,
) -> Result<Option<String>, KnowledgeRetrieveError> {
    let Some(raw) = input.map(str::trim).filter(|raw| !raw.is_empty()) else {
        return Ok(None);
    };
    let replaced = raw.replace('\\', "/");
    if replaced.starts_with('/')
        || replaced.starts_with("//")
        || replaced.as_bytes().get(1) == Some(&b':')
    {
        return Err(KnowledgeRetrieveError::InvalidPathPrefix);
    }
    let mut components = Vec::new();
    for component in replaced.split('/') {
        match component {
            "" | "." => {}
            ".." => {
                return Err(KnowledgeRetrieveError::InvalidPathPrefix);
            }
            component => components.push(component),
        }
    }
    Ok((!components.is_empty()).then(|| components.join("/")))
}

/// Bound a matched line to a readable window around the match (SIFT-WS1).
pub(crate) fn window_excerpt(line: &str, phrase: &str, terms: &[String]) -> String {
    let chars: Vec<char> = line.chars().collect();
    if chars.len() <= EXCERPT_MAX_CHARS {
        return line.to_string();
    }

    let lower: Vec<char> = line.chars().flat_map(|c| c.to_lowercase()).collect();
    let lower_str: String = lower.iter().collect();
    let find_chars = |needle: &str| -> Option<usize> {
        if needle.is_empty() {
            return None;
        }
        lower_str
            .find(needle)
            .map(|byte| lower_str[..byte].chars().count())
    };
    let hint = find_chars(phrase)
        .or_else(|| terms.iter().find_map(|term| find_chars(term)))
        .unwrap_or(0)
        .min(chars.len().saturating_sub(1));

    let match_len = phrase.chars().count().max(1);
    let half = EXCERPT_MAX_CHARS.saturating_sub(match_len) / 2;
    let mut start = hint.saturating_sub(half);
    let mut end = (start + EXCERPT_MAX_CHARS).min(chars.len());
    start = end.saturating_sub(EXCERPT_MAX_CHARS);

    if start > 0 {
        let limit = (start + 32).min(end);
        if let Some(offset) = (start..limit).find(|index| chars[*index].is_whitespace())
            && offset < hint
        {
            start = offset + 1;
        }
    }
    if end < chars.len() {
        let floor = end.saturating_sub(32).max(hint + match_len);
        if let Some(offset) = (floor..end)
            .rev()
            .find(|index| chars[*index].is_whitespace())
        {
            end = offset;
        }
    }

    let mut out = String::new();
    if start > 0 {
        out.push('…');
    }
    out.extend(chars[start..end].iter());
    if end < chars.len() {
        out.push('…');
    }
    out
}

fn is_stopword(term: &str) -> bool {
    matches!(
        term,
        "a" | "an"
            | "and"
            | "are"
            | "as"
            | "at"
            | "be"
            | "by"
            | "for"
            | "from"
            | "how"
            | "in"
            | "is"
            | "it"
            | "not"
            | "of"
            | "on"
            | "or"
            | "that"
            | "the"
            | "this"
            | "to"
            | "was"
            | "what"
            | "when"
            | "where"
            | "which"
            | "why"
            | "with"
    )
}

fn extract_lane(
    generation: &PublishedGeneration,
    label: &str,
    request: &KnowledgeRetrieveRequest,
) -> LaneHits {
    let derived = KnowledgeRetrieveDerived::capture(generation);
    let degraded = response_is_degraded(generation);
    let empty = |readiness: Option<KnowledgeLaneReadiness>,
                 envelope,
                 withheld,
                 hits: Vec<KnowledgeRetrieveHit>| {
        let unreadable = readiness.is_some();
        LaneHits {
            source: KnowledgeRetrieveSource {
                label: label.to_string(),
                envelope,
                withheld_count: withheld,
                filtered: KnowledgeRetrieveFilteredCounts::default(),
                readiness,
                degraded: degraded || unreadable,
                derived: derived.clone(),
                publication_generation: generation.publication_generation,
                content_generation: generation.content_generation,
            },
            hits,
        }
    };

    match generation.health.status {
        PublishedIndexStatus::Loading => {
            return empty(
                Some(KnowledgeLaneReadiness::IndexScoutingOrVerifying),
                None,
                0,
                Vec::new(),
            );
        }
        PublishedIndexStatus::Empty if generation.manifest.is_none() => {
            return empty(
                Some(KnowledgeLaneReadiness::NoValidSource),
                None,
                0,
                Vec::new(),
            );
        }
        _ => {}
    }

    let Some(envelope) = generation.source_response_envelope() else {
        return empty(
            Some(KnowledgeLaneReadiness::EnvelopeUnavailable),
            None,
            0,
            Vec::new(),
        );
    };
    if !source_envelope_is_safe(&envelope) {
        return empty(
            Some(KnowledgeLaneReadiness::EvidenceWithheld),
            None,
            1,
            Vec::new(),
        );
    }
    if request.terms.is_empty() {
        return empty(None, Some(envelope), 0, Vec::new());
    }

    let path_scope = match request.path_prefix.as_deref() {
        Some(prefix) => PathScope::prefix(prefix),
        None => PathScope::any(),
    };
    let headings = heading_paths(generation);
    let mut deduplicated: BTreeMap<(String, u32, String), KnowledgeRetrieveHit> = BTreeMap::new();
    let mut withheld_count = 0usize;
    let mut filtered = KnowledgeRetrieveFilteredCounts::default();

    for (record_index, record) in generation.authority.records.iter().enumerate() {
        if !path_scope.matches(&record.unit.path) {
            continue;
        }
        let Some(file) = generation.live.files.get(&record.unit.path) else {
            continue;
        };
        let Some(unit_bytes) = bounded_slice(
            &file.content,
            record.unit.byte_range.start,
            record.unit.byte_range.end,
        ) else {
            continue;
        };
        let Ok(unit_text) = std::str::from_utf8(unit_bytes) else {
            continue;
        };
        let heading_path = headings
            .get(&(
                record.unit.path.clone(),
                record.unit.byte_range.start,
                record.unit.byte_range.end,
            ))
            .cloned()
            .unwrap_or_default();
        let Some(matched) = match_unit(
            &file.content,
            record.unit.byte_range.start,
            unit_text,
            &heading_path,
            request,
        ) else {
            continue;
        };

        if !voice_allowed(record.voice, request.authority_scope) {
            note_filtered_voice(&mut filtered, record.voice);
            continue;
        }

        let authority = authority_view(generation, record_index, record);
        let (relationship_evidence, relationship_evidence_omitted) =
            relationship_evidence(generation, &record.unit);
        let candidate = KnowledgeRetrieveHit {
            source_label: label.to_string(),
            source_precedence: 0,
            path: record.unit.path.clone(),
            line: matched.line,
            line_range: matched.line_range,
            heading_path,
            preview: matched.preview,
            content_hash: record.unit.content_hash.clone(),
            publication_generation: generation.publication_generation,
            content_generation: generation.content_generation,
            authority,
            relationship_evidence,
            relationship_evidence_omitted,
            exact_phrase: matched.exact_phrase,
            heading_match: matched.heading_match,
            distinct_term_count: matched.distinct_term_count,
            unit_start: record.unit.byte_range.start,
            unit_len: record
                .unit
                .byte_range
                .end
                .saturating_sub(record.unit.byte_range.start),
        };

        let heading = candidate.heading_path.join(" > ");
        let bridge = candidate
            .relationship_evidence
            .iter()
            .map(KnowledgeRelationshipEvidence::preview_token)
            .collect::<Vec<_>>()
            .join(" | ");
        let finding_ids = candidate.authority.finding_ids.join(",");
        let provenance_ids = candidate.authority.provenance_ids.join(",");
        let visible_fields = [
            candidate.path.as_str(),
            heading.as_str(),
            candidate.preview.as_str(),
            candidate.content_hash.as_str(),
            finding_ids.as_str(),
            provenance_ids.as_str(),
            bridge.as_str(),
        ];
        if guard_hit(&candidate, &visible_fields).is_err() {
            withheld_count = withheld_count.saturating_add(1);
            continue;
        }

        let key = (
            candidate.path.clone(),
            candidate.line,
            candidate.preview.clone(),
        );
        match deduplicated.get(&key) {
            Some(existing)
                if existing.unit_len < candidate.unit_len
                    || (existing.unit_len == candidate.unit_len
                        && existing.heading_path.len() >= candidate.heading_path.len()) => {}
            _ => {
                deduplicated.insert(key, candidate);
            }
        }
    }

    for entry in &generation.live.manifest_entries {
        let Some(path) = entry.path.normalized_utf8.as_deref() else {
            continue;
        };
        if !path_scope.matches(path) {
            continue;
        }
        if matches!(
            &entry.disposition,
            crate::domain::FileDisposition::MetadataOnly {
                reason: crate::domain::MetadataOnlyReason::SensitivePath { .. }
                    | crate::domain::MetadataOnlyReason::SensitiveContent { .. },
            }
        ) {
            withheld_count = withheld_count.saturating_add(1);
        }
    }

    let mut hits: Vec<KnowledgeRetrieveHit> = deduplicated.into_values().collect();
    hits.sort_by(rank_hits);

    LaneHits {
        source: KnowledgeRetrieveSource {
            label: label.to_string(),
            envelope: Some(envelope),
            withheld_count,
            filtered,
            readiness: None,
            degraded,
            derived,
            publication_generation: generation.publication_generation,
            content_generation: generation.content_generation,
        },
        hits,
    }
}

fn compose(lanes: Vec<LaneHits>, request: &KnowledgeRetrieveRequest) -> KnowledgeRetrieveResult {
    let mut hits: Vec<KnowledgeRetrieveHit> = Vec::new();
    let mut withheld_count = 0usize;
    let mut filtered = KnowledgeRetrieveFilteredCounts::default();
    let mut sources = Vec::with_capacity(lanes.len());
    for (precedence, lane) in lanes.into_iter().enumerate() {
        withheld_count = withheld_count.saturating_add(lane.source.withheld_count);
        filtered.saturating_add_from(&lane.source.filtered);
        hits.extend(lane.hits.into_iter().map(|mut hit| {
            hit.source_precedence = precedence;
            hit
        }));
        sources.push(lane.source);
    }

    hits.sort_by(rank_hits);
    let overflow = hits.len().saturating_sub(request.limit);
    hits.truncate(request.limit);
    let truncated = overflow > 0;
    let degraded = sources.iter().any(|source| source.degraded);

    let absence = if hits.is_empty() {
        Some(if request.terms.is_empty() {
            KnowledgeRetrieveAbsence::QueryTooWeak
        } else if withheld_count > 0 {
            KnowledgeRetrieveAbsence::EvidenceWithheld
        } else if filtered.any() {
            KnowledgeRetrieveAbsence::EvidenceNoncurrent
        } else if degraded {
            KnowledgeRetrieveAbsence::NoEvidenceDegraded
        } else {
            KnowledgeRetrieveAbsence::NoEvidenceComplete
        })
    } else {
        None
    };

    KnowledgeRetrieveResult {
        hits,
        sources,
        withheld_count,
        withheld_reasons: if withheld_count == 0 {
            Vec::new()
        } else {
            vec![KnowledgeWithheldReason::PolicyWithheld]
        },
        truncated,
        overflow,
        filtered,
        absence,
        degraded,
    }
}

fn rank_hits(left: &KnowledgeRetrieveHit, right: &KnowledgeRetrieveHit) -> std::cmp::Ordering {
    right
        .exact_phrase
        .cmp(&left.exact_phrase)
        .then_with(|| right.heading_match.cmp(&left.heading_match))
        .then_with(|| right.distinct_term_count.cmp(&left.distinct_term_count))
        .then_with(|| left.source_precedence.cmp(&right.source_precedence))
        .then_with(|| left.path.cmp(&right.path))
        .then_with(|| left.line.cmp(&right.line))
        .then_with(|| left.unit_start.cmp(&right.unit_start))
}

fn source_envelope_is_safe(envelope: &SourceResponseEnvelope) -> bool {
    let branch = envelope.source_version.branch.as_deref().unwrap_or("");
    let commit = envelope.source_version.commit.as_deref().unwrap_or("");
    guard_hit(
        envelope,
        &[envelope.source.source_id.as_str(), branch, commit],
    )
    .is_ok()
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct UnitMatch {
    line: u32,
    line_range: Range<u32>,
    preview: String,
    exact_phrase: bool,
    heading_match: bool,
    distinct_term_count: usize,
}

fn match_unit(
    file_bytes: &[u8],
    unit_start: u32,
    unit_text: &str,
    heading_path: &[String],
    request: &KnowledgeRetrieveRequest,
) -> Option<UnitMatch> {
    let unit_lower = unit_text.to_lowercase();
    let exact_phrase = unit_lower.contains(&request.phrase);
    let distinct_term_count = request
        .terms
        .iter()
        .filter(|term| unit_lower.contains(term.as_str()))
        .count();
    if !exact_phrase && distinct_term_count == 0 {
        return None;
    }

    let heading_lower = heading_path.join(" ").to_lowercase();
    let heading_match = heading_lower.contains(&request.phrase)
        || request
            .terms
            .iter()
            .any(|term| heading_lower.contains(term));
    let base_line = 1u32.saturating_add(
        file_bytes
            .get(..usize::try_from(unit_start).ok()?)?
            .iter()
            .filter(|byte| **byte == b'\n')
            .count() as u32,
    );
    let mut best: Option<(bool, usize, usize, String)> = None;
    for (offset, raw_line) in unit_text.lines().enumerate() {
        let line = raw_line.trim_end_matches('\r');
        let lower = line.to_lowercase();
        let phrase_here = lower.contains(&request.phrase);
        let terms_here = request
            .terms
            .iter()
            .filter(|term| lower.contains(term.as_str()))
            .count();
        if !phrase_here && terms_here == 0 {
            continue;
        }
        let replace = best.as_ref().is_none_or(|current| {
            (phrase_here, terms_here, std::cmp::Reverse(offset))
                > (current.0, current.1, std::cmp::Reverse(current.2))
        });
        if replace {
            best = Some((phrase_here, terms_here, offset, line.to_string()));
        }
    }
    let (_, _, offset, excerpt) = best?;
    let line = base_line.saturating_add(u32::try_from(offset).unwrap_or(u32::MAX));
    let unit_line_count = u32::try_from(unit_text.lines().count().max(1)).unwrap_or(u32::MAX);
    Some(UnitMatch {
        line,
        line_range: base_line..base_line.saturating_add(unit_line_count),
        preview: window_excerpt(&excerpt, &request.phrase, &request.terms),
        exact_phrase,
        heading_match,
        distinct_term_count,
    })
}

fn heading_paths(generation: &PublishedGeneration) -> HashMap<(String, u32, u32), Vec<String>> {
    let mut headings = HashMap::new();
    let Some(source) = generation.source.as_deref() else {
        return headings;
    };
    for (path, file) in &generation.live.files {
        if file.language != LanguageId::Markdown {
            continue;
        }
        for unit in project_markdown_sections(source, path, &file.content_hash, &file.symbols) {
            headings.insert(
                (path.clone(), unit.byte_range.start, unit.byte_range.end),
                unit.heading_path,
            );
        }
    }
    headings
}

fn authority_view(
    generation: &PublishedGeneration,
    record_index: usize,
    record: &KnowledgeAuthorityRecord,
) -> KnowledgeRetrieveAuthority {
    let mut finding_ids: Vec<String> = generation
        .authority
        .finding_index
        .iter()
        .filter(|(_, index)| usize::try_from(**index).ok() == Some(record_index))
        .map(|(id, _)| id.clone())
        .collect();
    finding_ids.sort();
    finding_ids.dedup();
    let finding_ids_omitted = finding_ids.len().saturating_sub(MAX_IDS_PER_HIT);
    finding_ids.truncate(MAX_IDS_PER_HIT);

    let mut provenance_ids = BTreeSet::new();
    provenance_ids.extend(record.code_evidence.consistent_rule_ids.iter().cloned());
    provenance_ids.extend(
        record
            .code_evidence
            .deterministic_conflict_ids
            .iter()
            .cloned(),
    );
    provenance_ids.extend(record.code_evidence.suspected_conflict_ids.iter().cloned());
    provenance_ids.extend(record.code_evidence.implementation_gap_ids.iter().cloned());
    provenance_ids.extend(record.code_evidence.review_signal_ids.iter().cloned());
    let mut provenance_ids: Vec<String> = provenance_ids.into_iter().collect();
    let provenance_ids_omitted = provenance_ids.len().saturating_sub(MAX_IDS_PER_HIT);
    provenance_ids.truncate(MAX_IDS_PER_HIT);

    KnowledgeRetrieveAuthority {
        lifecycle: record.lifecycle,
        authority_domain: record.authority_domain,
        code_evidence: record.code_evidence.display,
        voice: record.voice,
        finding_ids,
        finding_ids_omitted,
        provenance_ids,
        provenance_ids_omitted,
        coverage: record.code_evidence.coverage.clone(),
    }
}

fn relationship_evidence(
    generation: &PublishedGeneration,
    unit: &KnowledgeAnchor,
) -> (Vec<KnowledgeRelationshipEvidence>, usize) {
    let mut items: Vec<KnowledgeRelationshipEvidence> = generation
        .bridge
        .forward
        .iter()
        .filter(|link| {
            link.evidence.source == unit.source
                && link.evidence.path == unit.path
                && link.evidence.content_hash == unit.content_hash
                && unit.byte_range.start <= link.evidence.byte_range.start
                && link.evidence.byte_range.end <= unit.byte_range.end
        })
        .map(|link| KnowledgeRelationshipEvidence {
            link_id: link.id.0.clone(),
            kind: link.evidence_kind.clone(),
            resolution: link.resolution.clone(),
        })
        .collect();
    items.sort_by_key(|item| item.preview_token());
    items.dedup_by(|left, right| left.preview_token() == right.preview_token());

    let class_of = |preview: &str| -> usize {
        if preview.contains(":exact:") {
            0
        } else if preview.contains(":declared_set:") {
            1
        } else if preview.contains(":ambiguous:") {
            2
        } else {
            3
        }
    };
    let total = items.len();
    let mut by_class: [Vec<KnowledgeRelationshipEvidence>; 4] = Default::default();
    for item in items {
        let class = class_of(&item.preview_token());
        by_class[class].push(item);
    }
    let present = by_class.iter().filter(|class| !class.is_empty()).count();
    let mut selected: Vec<KnowledgeRelationshipEvidence> = Vec::new();
    if present > 0 {
        for class in by_class.iter_mut() {
            if !class.is_empty() && selected.len() < MAX_BRIDGE_PREVIEWS_PER_HIT {
                selected.push(class.remove(0));
            }
        }
        for class in by_class.iter_mut() {
            while !class.is_empty() && selected.len() < MAX_BRIDGE_PREVIEWS_PER_HIT {
                selected.push(class.remove(0));
            }
        }
    }
    let omitted = total.saturating_sub(selected.len());
    (selected, omitted)
}

fn voice_allowed(voice: KnowledgeVoice, scope: KnowledgeRetrieveAuthorityScope) -> bool {
    match scope {
        KnowledgeRetrieveAuthorityScope::Default => matches!(
            voice,
            KnowledgeVoice::Current
                | KnowledgeVoice::Intent
                | KnowledgeVoice::NeedsReview
                | KnowledgeVoice::Unknown
        ),
        KnowledgeRetrieveAuthorityScope::Current => matches!(
            voice,
            KnowledgeVoice::Current | KnowledgeVoice::NeedsReview | KnowledgeVoice::Unknown
        ),
        KnowledgeRetrieveAuthorityScope::Intent => voice == KnowledgeVoice::Intent,
        KnowledgeRetrieveAuthorityScope::History => {
            matches!(
                voice,
                KnowledgeVoice::HistoryOnly | KnowledgeVoice::Suppressed
            )
        }
        KnowledgeRetrieveAuthorityScope::All => true,
    }
}

fn note_filtered_voice(counts: &mut KnowledgeRetrieveFilteredCounts, voice: KnowledgeVoice) {
    match voice {
        KnowledgeVoice::Current => counts.current = counts.current.saturating_add(1),
        KnowledgeVoice::Intent => counts.intent = counts.intent.saturating_add(1),
        KnowledgeVoice::HistoryOnly => counts.history_only = counts.history_only.saturating_add(1),
        KnowledgeVoice::Suppressed => counts.suppressed = counts.suppressed.saturating_add(1),
        KnowledgeVoice::NeedsReview => {
            counts.review_required = counts.review_required.saturating_add(1)
        }
        KnowledgeVoice::Unknown => counts.unknown = counts.unknown.saturating_add(1),
    }
}

fn bounded_slice(bytes: &[u8], start: u32, end: u32) -> Option<&[u8]> {
    let start = usize::try_from(start).ok()?;
    let end = usize::try_from(end).ok()?;
    (start <= end && end <= bytes.len()).then(|| &bytes[start..end])
}

fn response_is_degraded(generation: &PublishedGeneration) -> bool {
    generation
        .manifest
        .as_ref()
        .is_some_and(|manifest| manifest.coverage == CoverageStatus::Degraded)
        || !matches!(generation.freshness.as_ref(), FreshnessStatus::Current)
        || !matches!(generation.bridge.coverage, DerivedCoverage::Complete)
        || !matches!(generation.authority.coverage, DerivedCoverage::Complete)
}

fn bridge_evidence_kind_label(kind: &BridgeEvidenceKind) -> &'static str {
    match kind {
        BridgeEvidenceKind::RepositoryLink => "repository_link",
        BridgeEvidenceKind::ExactPathToken => "exact_path",
        BridgeEvidenceKind::ExactCodeSpanSymbol => "exact_code_span",
        BridgeEvidenceKind::DeclaredOwnershipSelector => "declared_set",
        BridgeEvidenceKind::SupportedStructuredValue { .. } => "structured_value",
    }
}

fn bridge_resolution_preview(resolution: &BridgeResolution) -> String {
    match resolution {
        BridgeResolution::ResolvedExact(anchor) => format!("exact:{}", anchor.id.label()),
        BridgeResolution::ResolvedDeclaredSet { matched_count, .. } => {
            format!("declared_set:{matched_count}")
        }
        BridgeResolution::Ambiguous {
            candidate_count,
            bounded_samples,
        } => format!(
            "ambiguous:{candidate_count}:samples={}",
            bounded_samples.len()
        ),
        BridgeResolution::Missing => "missing".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::live_index::LiveIndex;

    fn request(query: &str) -> KnowledgeRetrieveRequest {
        KnowledgeRetrieveRequest::parse(query, None, KnowledgeRetrieveAuthorityScope::Default, 10)
            .expect("valid request")
    }

    fn load(files: &[(&str, &str)]) -> (tempfile::TempDir, Arc<PublishedGeneration>) {
        let dir = tempfile::tempdir().expect("tempdir");
        for (path, body) in files {
            let full = dir.path().join(path);
            if let Some(parent) = full.parent() {
                std::fs::create_dir_all(parent).expect("parent dir");
            }
            std::fs::write(&full, body).expect("doc fixture");
        }
        let index = LiveIndex::load(dir.path()).expect("load corpus");
        let generation = index.published_source_set().current_generation();
        (dir, generation)
    }

    fn retrieve_one(
        generation: &PublishedGeneration,
        request: &KnowledgeRetrieveRequest,
    ) -> KnowledgeRetrieveResult {
        retrieve_knowledge(
            &[KnowledgeRetrieveLane {
                generation,
                label: "current",
            }],
            request,
        )
    }

    #[test]
    fn path_prefix_uses_slash_boundary_and_excludes_srcx() {
        let (_dir, generation) = load(&[
            (
                "src/note.md",
                "# Src\nsrc lattice persistence boundary lives here.\n",
            ),
            (
                "srcx/note.md",
                "# Srcx\nsrcx lattice persistence boundary lives here.\n",
            ),
        ]);
        let mut scoped = request("lattice persistence boundary");
        scoped.path_prefix = Some("src".to_string());
        let result = retrieve_one(&generation, &scoped);
        let paths: Vec<&str> = result.hits.iter().map(|hit| hit.path.as_str()).collect();
        assert!(paths.contains(&"src/note.md"), "src/ must match: {paths:?}");
        assert!(
            paths.iter().all(|path| *path != "srcx/note.md"),
            "src/ must not match srcx/: {paths:?}"
        );
    }

    #[test]
    fn sensitive_visible_fields_collapse_to_policy_withheld() {
        let (_dir, generation) = load(&[(
            "docs/recovery.md",
            "# Recovery\nSafe checkpoint evidence.\n",
        )]);
        let canary = ["runtime", "-", "canary", "-", "segment"].concat();
        let mut source_version = generation
            .source_version
            .as_ref()
            .expect("source version")
            .as_ref()
            .clone();
        source_version.branch = Some(format!("token={canary}"));
        let unsafe_generation = PublishedGeneration {
            publication_generation: generation.publication_generation,
            content_generation: generation.content_generation,
            project_generation: generation.project_generation,
            source: generation.source.clone(),
            source_version: Some(Arc::new(source_version)),
            freshness: Arc::clone(&generation.freshness),
            manifest: generation.manifest.clone(),
            code_signals: Arc::clone(&generation.code_signals),
            bridge: Arc::clone(&generation.bridge),
            authority: Arc::clone(&generation.authority),
            live: Arc::clone(&generation.live),
            health: Arc::clone(&generation.health),
            outline: Arc::clone(&generation.outline),
        };
        let result = retrieve_one(&unsafe_generation, &request("checkpoint evidence"));
        assert!(
            result.hits.is_empty(),
            "unsafe source provenance must not emit hits"
        );
        assert_eq!(result.withheld_count, 1);
        assert_eq!(
            result.withheld_reasons,
            vec![KnowledgeWithheldReason::PolicyWithheld]
        );
        assert_eq!(
            result.absence,
            Some(KnowledgeRetrieveAbsence::EvidenceWithheld)
        );
        assert_eq!(
            result.sources[0].readiness,
            Some(KnowledgeLaneReadiness::EvidenceWithheld)
        );
    }

    #[test]
    fn in_scope_catalog_sensitive_files_count_as_policy_withheld() {
        let (_dir, generation) = load(&[
            ("docs/ok.md", "# Ok\ncheckpoint evidence is safe.\n"),
            (".env", "SECRET_KEY=checkpoint-evidence-must-not-leak\n"),
        ]);
        let result = retrieve_one(&generation, &request("checkpoint evidence"));
        assert!(
            result.hits.iter().any(|hit| hit.path == "docs/ok.md"),
            "admitted knowledge must still hit"
        );
        assert!(
            result.withheld_count > 0,
            "in-scope policy-withheld catalog files must be counted"
        );
        assert_eq!(
            result.withheld_reasons,
            vec![KnowledgeWithheldReason::PolicyWithheld]
        );
    }

    #[test]
    fn limit_sets_truncated_without_changing_rank_order() {
        let (_dir, generation) = load(&[
            ("docs/a.md", "# A\nalpha persistence boundary first.\n"),
            ("docs/b.md", "# B\nalpha persistence boundary second.\n"),
        ]);
        let mut limited = request("alpha persistence boundary");
        limited.limit = 1;
        let result = retrieve_one(&generation, &limited);
        assert_eq!(result.hits.len(), 1);
        assert!(result.truncated);
        assert_eq!(result.overflow, 1);
    }
}
