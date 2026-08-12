//! Frozen Gate I `search_knowledge` extraction, ranking, safety, and formatting.
//!
//! Every response is derived from one caller-captured [`PublishedGeneration`].
//! This module never reloads the live index while formatting a result.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::ops::Range;
use std::sync::Arc;

use crate::domain::{CoverageStatus, FreshnessStatus, LanguageId, SourceLocation};
use crate::knowledge::{guard_hit, guard_query, project_markdown_sections};
use crate::live_index::knowledge_authority::{KnowledgeAuthorityRecord, KnowledgeVoice};
use crate::live_index::knowledge_bridge::{
    BridgeEvidenceKind, BridgeResolution, CodeAnchorId, DerivedCoverage, KnowledgeAnchor,
};
use crate::live_index::{PublishedGeneration, PublishedIndexStatus, PublishedSourceSet};

use super::search_tools::{KnowledgeAuthorityScope, KnowledgeSourceScope, SearchKnowledgeInput};

const DEFAULT_LIMIT: usize = 10;
const MAX_LIMIT: usize = 100;
const MIN_PROVENANCE_TOKENS: u64 = 64;
const MAX_IDS_PER_HIT: usize = 8;
const MAX_BRIDGE_PREVIEWS_PER_HIT: usize = 4;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NormalizedQuery {
    phrase: String,
    terms: Vec<String>,
    path_prefix: Option<String>,
    authority_scope: KnowledgeAuthorityScope,
    limit: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct AuthorityDisplay {
    lifecycle: String,
    authority_domain: String,
    code_evidence: String,
    voice: String,
    finding_ids: Vec<String>,
    finding_ids_omitted: usize,
    provenance_ids: Vec<String>,
    provenance_ids_omitted: usize,
    coverage: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct KnowledgeHit {
    path: String,
    line: u32,
    line_range: Range<u32>,
    heading_path: Vec<String>,
    excerpt: String,
    content_hash: String,
    publication_generation: u64,
    content_generation: u64,
    authority: AuthorityDisplay,
    bridge_previews: Vec<String>,
    bridge_previews_omitted: usize,
    exact_phrase: bool,
    heading_match: bool,
    distinct_term_count: usize,
    unit_start: u32,
    unit_len: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct FilteredCounts {
    current: usize,
    intent: usize,
    history_only: usize,
    suppressed: usize,
    review_required: usize,
    unknown: usize,
}

pub(crate) fn validate_input(input: &SearchKnowledgeInput) -> Result<NormalizedQuery, String> {
    // Security must run before tokenization, path routing, proxying, analytics,
    // cache lookup, or CCR creation. The rejection never echoes the query.
    guard_query(&input.query)
        .map_err(|_| "Error: sensitive query rejected by repository safety policy.".to_string())?;
    if input
        .path_prefix
        .as_deref()
        .is_some_and(|path| guard_query(path).is_err())
    {
        return Err(
            "Error: sensitive path_prefix rejected by repository safety policy.".to_string(),
        );
    }

    let phrase = input.query.trim();
    if phrase.is_empty() {
        return Err("Error: query must be non-empty.".to_string());
    }
    if input.project.is_some() && input.projects.is_some() {
        return Err("Error: project and projects are mutually exclusive.".to_string());
    }
    if input.projects.as_ref().is_some_and(Vec::is_empty) {
        return Err("Error: projects must not be empty.".to_string());
    }
    if let Some(max_tokens) = input.max_tokens
        && max_tokens < MIN_PROVENANCE_TOKENS
    {
        return Err(format!(
            "Error: max_tokens is too small for provenance; minimum is {MIN_PROVENANCE_TOKENS}."
        ));
    }
    if let Some(projects) = input.projects.as_ref()
        && projects.iter().any(|project| project == "*")
        && projects.len() != 1
    {
        return Err("Error: projects wildcard must be the sole selector.".to_string());
    }
    let path_prefix = normalize_path_prefix(input.path_prefix.as_deref())?;
    let terms = significant_terms(phrase);
    let authority_scope = input
        .authority_scope
        .unwrap_or(KnowledgeAuthorityScope::Default);
    let limit = usize::try_from(input.limit.unwrap_or(DEFAULT_LIMIT as u32))
        .unwrap_or(MAX_LIMIT)
        .clamp(1, MAX_LIMIT);

    Ok(NormalizedQuery {
        phrase: phrase.to_lowercase(),
        terms,
        path_prefix,
        authority_scope,
        limit,
    })
}

/// Select the published generations a source scope addresses, deterministically.
///
/// `current` is the single current-worktree lane. `local_refs`/`worktrees`
/// filter the captured set by source location, excluding the current lane.
/// `all` lists the current lane first (it ranks ahead of a divergent ref but
/// never hides it), then every other lane in `SourceId` order.
pub(crate) fn select_scoped_sources(
    source_set: &PublishedSourceSet,
    scope: KnowledgeSourceScope,
) -> Vec<Arc<PublishedGeneration>> {
    let current_id = &source_set.current_source_id;
    let is_git_ref = |generation: &PublishedGeneration| {
        matches!(
            generation.source.as_deref().map(|source| &source.location),
            Some(SourceLocation::GitRef { .. })
        )
    };
    let is_worktree = |generation: &PublishedGeneration| {
        matches!(
            generation.source.as_deref().map(|source| &source.location),
            Some(SourceLocation::WorkingTree { .. })
        )
    };
    match scope {
        KnowledgeSourceScope::Current => source_set
            .sources
            .get(current_id)
            .map(Arc::clone)
            .into_iter()
            .collect(),
        KnowledgeSourceScope::LocalRefs => source_set
            .sources
            .iter()
            .filter(|(id, generation)| *id != current_id && is_git_ref(generation))
            .map(|(_, generation)| Arc::clone(generation))
            .collect(),
        KnowledgeSourceScope::Worktrees => source_set
            .sources
            .iter()
            .filter(|(id, generation)| *id != current_id && is_worktree(generation))
            .map(|(_, generation)| Arc::clone(generation))
            .collect(),
        KnowledgeSourceScope::All => {
            let mut selected: Vec<Arc<PublishedGeneration>> = source_set
                .sources
                .get(current_id)
                .map(Arc::clone)
                .into_iter()
                .collect();
            selected.extend(
                source_set
                    .sources
                    .iter()
                    .filter(|(id, _)| *id != current_id)
                    .map(|(_, generation)| Arc::clone(generation)),
            );
            selected
        }
    }
}

/// Per-source identity header for a composed multi-source response (L-R06):
/// identity, captured working-tree state, publication/content generations,
/// freshness, manifest coverage, and manifest digest.
fn render_source_scope_identity(generation: &PublishedGeneration) -> String {
    let location = match generation.source.as_deref().map(|source| &source.location) {
        Some(SourceLocation::WorkingTree { worktree_id }) => format!("worktree:{worktree_id}"),
        Some(SourceLocation::GitRef { name }) => format!("ref:{name}"),
        None => "unbound".to_string(),
    };
    let source_id = generation
        .source
        .as_deref()
        .map(|source| source.source_id.as_str().to_string())
        .unwrap_or_else(|| "unbound".to_string());
    let working_tree = generation
        .source_version
        .as_deref()
        .map(|version| format!("{:?}", version.working_tree))
        .unwrap_or_else(|| "Unknown".to_string());
    let digest = generation
        .manifest
        .as_deref()
        .map(|manifest| manifest.digest.as_str())
        .unwrap_or("none");
    format!(
        "{location} source_id={source_id} publication_generation={} content_generation={} working_tree={working_tree} freshness={:?} coverage={:?} manifest_digest={digest}",
        generation.publication_generation,
        generation.content_generation,
        generation.freshness,
        source_coverage(generation),
    )
}

/// Manifest-declared coverage for one source, defaulting to `Degraded` when no
/// manifest is published so a composed response never claims a false Complete.
pub(crate) fn source_coverage(generation: &PublishedGeneration) -> CoverageStatus {
    generation
        .manifest
        .as_deref()
        .map(|manifest| manifest.coverage)
        .unwrap_or(CoverageStatus::Degraded)
}

/// Overall coverage of a composed response equals the worst included source
/// (L-R06): a single degraded source degrades the whole envelope.
pub(crate) fn worst_source_coverage(selected: &[Arc<PublishedGeneration>]) -> CoverageStatus {
    if selected
        .iter()
        .any(|generation| source_coverage(generation) == CoverageStatus::Degraded)
    {
        CoverageStatus::Degraded
    } else {
        CoverageStatus::Complete
    }
}

/// Compose `search_knowledge` across the scope-selected sources of one captured
/// source set (Gate L L-G06). `current` keeps its exact single-source output. A
/// multi-source scope that selects no sources returns a typed empty readiness
/// result rather than a false complete-absence claim.
pub(crate) fn search_scoped(
    source_set: &PublishedSourceSet,
    input: &SearchKnowledgeInput,
) -> String {
    if let Err(error) = validate_input(input) {
        return error;
    }
    let scope = input.source_scope.unwrap_or(KnowledgeSourceScope::Current);
    if matches!(scope, KnowledgeSourceScope::Current) {
        return search_current(&source_set.current_generation(), input);
    }
    let selected = select_scoped_sources(source_set, scope);
    if selected.is_empty() {
        return format!(
            "Readiness: no_sources_in_scope; source_scope '{}' selected no sources.",
            source_scope_label(scope)
        );
    }
    let sections: Vec<String> = selected
        .iter()
        .map(|generation| {
            format!(
                "== source: {} ==\n{}",
                render_source_scope_identity(generation),
                search_current(generation, input)
            )
        })
        .collect();
    format!(
        "Source scope searched: {}\nSources: {}\nOverall coverage: {:?}\nSecret policy version: {}\n\n{}",
        source_scope_label(scope),
        selected.len(),
        worst_source_coverage(&selected),
        crate::knowledge::SECRET_POLICY_VERSION,
        sections.join("\n\n")
    )
}

pub(crate) fn search_current(
    generation: &PublishedGeneration,
    input: &SearchKnowledgeInput,
) -> String {
    let query = match validate_input(input) {
        Ok(query) => query,
        Err(error) => return error,
    };

    match generation.health.status {
        PublishedIndexStatus::Loading => {
            return "Readiness: index_scouting_or_verifying; retry after the current publication completes."
                .to_string();
        }
        PublishedIndexStatus::Empty if generation.manifest.is_none() => {
            return "Readiness: no_valid_source; run index_folder to rebuild from repository source."
                .to_string();
        }
        _ => {}
    }

    let Some(envelope) = generation.source_response_envelope() else {
        return "Readiness: no_valid_source; source envelope is unavailable and no evidence was served."
            .to_string();
    };
    if !source_envelope_is_safe(&envelope) {
        return render_source_withheld_response(generation, input);
    }

    if query.terms.is_empty() {
        return render_response(
            generation,
            &envelope,
            input,
            &[],
            Some("query_too_weak"),
            0,
            0,
            FilteredCounts::default(),
        );
    }

    let headings = heading_paths(generation);
    let mut deduplicated: BTreeMap<(String, u32, String), KnowledgeHit> = BTreeMap::new();
    let mut withheld_sensitive = 0usize;
    let mut filtered = FilteredCounts::default();

    for (record_index, record) in generation.authority.records.iter().enumerate() {
        if !path_in_scope(&record.unit.path, query.path_prefix.as_deref()) {
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
            &query,
        ) else {
            continue;
        };

        if !voice_allowed(record.voice, query.authority_scope) {
            note_filtered_voice(&mut filtered, record.voice);
            continue;
        }

        let authority = authority_display(generation, record_index, record);
        let (bridge_previews, bridge_previews_omitted) = bridge_previews(generation, &record.unit);
        let candidate = KnowledgeHit {
            path: record.unit.path.clone(),
            line: matched.line,
            line_range: matched.line_range,
            heading_path,
            excerpt: matched.excerpt,
            content_hash: record.unit.content_hash.clone(),
            publication_generation: generation.publication_generation,
            content_generation: generation.content_generation,
            authority,
            bridge_previews,
            bridge_previews_omitted,
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
        let bridge = candidate.bridge_previews.join(" | ");
        let finding_ids = candidate.authority.finding_ids.join(",");
        let provenance_ids = candidate.authority.provenance_ids.join(",");
        let visible_fields = [
            candidate.path.as_str(),
            heading.as_str(),
            candidate.excerpt.as_str(),
            candidate.content_hash.as_str(),
            finding_ids.as_str(),
            provenance_ids.as_str(),
            bridge.as_str(),
        ];
        if guard_hit(&candidate, &visible_fields).is_err() {
            withheld_sensitive = withheld_sensitive.saturating_add(1);
            continue;
        }

        let key = (
            candidate.path.clone(),
            candidate.line,
            candidate.excerpt.clone(),
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

    let mut hits: Vec<KnowledgeHit> = deduplicated.into_values().collect();
    hits.sort_by(|left, right| {
        right
            .exact_phrase
            .cmp(&left.exact_phrase)
            .then_with(|| right.heading_match.cmp(&left.heading_match))
            .then_with(|| right.distinct_term_count.cmp(&left.distinct_term_count))
            .then_with(|| left.path.cmp(&right.path))
            .then_with(|| left.line.cmp(&right.line))
            .then_with(|| left.unit_start.cmp(&right.unit_start))
    });
    let overflow = hits.len().saturating_sub(query.limit);
    hits.truncate(query.limit);

    let no_match = if hits.is_empty() {
        Some(if withheld_sensitive > 0 {
            "evidence_withheld"
        } else if filtered.current > 0
            || filtered.intent > 0
            || filtered.history_only > 0
            || filtered.suppressed > 0
            || filtered.review_required > 0
            || filtered.unknown > 0
        {
            "evidence_noncurrent"
        } else if response_is_degraded(generation) {
            "no_evidence_degraded"
        } else {
            "no_evidence_complete"
        })
    } else {
        None
    };

    render_response(
        generation,
        &envelope,
        input,
        &hits,
        no_match,
        overflow,
        withheld_sensitive,
        filtered,
    )
}

fn source_envelope_is_safe(envelope: &crate::domain::SourceResponseEnvelope) -> bool {
    let branch = envelope.source_version.branch.as_deref().unwrap_or("");
    let commit = envelope.source_version.commit.as_deref().unwrap_or("");
    guard_hit(
        envelope,
        &[envelope.source.source_id.as_str(), branch, commit],
    )
    .is_ok()
}

fn render_source_withheld_response(
    generation: &PublishedGeneration,
    input: &SearchKnowledgeInput,
) -> String {
    let overall_coverage = if response_is_degraded(generation) {
        "degraded"
    } else {
        "complete"
    };
    let path_scope = input.path_prefix.as_deref().unwrap_or("repository");
    format!(
        "Trust: exact repository knowledge evidence | publication={} | content={} | source=withheld | coverage={}\n\
         Secret policy: version {}\n\
         Scope: current + {}\n\
         Source: source=withheld source_id=withheld source_version=withheld publication={} content={} freshness=withheld coverage={} manifest_digest=withheld\n\
         Derived: authority_rule_version={} policy_version={} secret_policy_version={} bridge_coverage={} authority_coverage={} overall_coverage={}\n\
         Counts: overflow=0 withheld_sensitive=1 filtered_current=0 filtered_intent=0 filtered_history_only=0 filtered_suppressed=0 filtered_review_required=0 filtered_unknown=0\n\
         No match: evidence_withheld",
        generation.publication_generation,
        generation.content_generation,
        overall_coverage,
        generation.authority.versions.secret_policy_version,
        path_scope,
        generation.publication_generation,
        generation.content_generation,
        overall_coverage,
        generation.authority.versions.authority_rule_version,
        generation.authority.versions.policy_version,
        generation.authority.versions.secret_policy_version,
        derived_coverage_label(&generation.bridge.coverage),
        derived_coverage_label(&generation.authority.coverage),
        overall_coverage,
    )
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct UnitMatch {
    line: u32,
    line_range: Range<u32>,
    excerpt: String,
    exact_phrase: bool,
    heading_match: bool,
    distinct_term_count: usize,
}

fn match_unit(
    file_bytes: &[u8],
    unit_start: u32,
    unit_text: &str,
    heading_path: &[String],
    query: &NormalizedQuery,
) -> Option<UnitMatch> {
    let unit_lower = unit_text.to_lowercase();
    let exact_phrase = unit_lower.contains(&query.phrase);
    let distinct_term_count = query
        .terms
        .iter()
        .filter(|term| unit_lower.contains(term.as_str()))
        .count();
    if !exact_phrase && distinct_term_count == 0 {
        return None;
    }

    let heading_lower = heading_path.join(" ").to_lowercase();
    let heading_match = heading_lower.contains(&query.phrase)
        || query.terms.iter().any(|term| heading_lower.contains(term));
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
        let phrase_here = lower.contains(&query.phrase);
        let terms_here = query
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
        excerpt,
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

fn authority_display(
    generation: &PublishedGeneration,
    record_index: usize,
    record: &KnowledgeAuthorityRecord,
) -> AuthorityDisplay {
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

    AuthorityDisplay {
        lifecycle: snake_debug(record.lifecycle),
        authority_domain: snake_debug(record.authority_domain),
        code_evidence: snake_debug(record.code_evidence.display),
        voice: snake_debug(record.voice),
        finding_ids,
        finding_ids_omitted,
        provenance_ids,
        provenance_ids_omitted,
        coverage: derived_coverage_label(&record.code_evidence.coverage).to_string(),
    }
}

fn bridge_previews(
    generation: &PublishedGeneration,
    unit: &KnowledgeAnchor,
) -> (Vec<String>, usize) {
    let mut previews: Vec<String> = generation
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
        .map(|link| {
            format!(
                "{}:{}:{}",
                link.id.0,
                bridge_evidence_kind_label(&link.evidence_kind),
                bridge_resolution_preview(&link.resolution)
            )
        })
        .collect();
    previews.sort();
    previews.dedup();
    let omitted = previews.len().saturating_sub(MAX_BRIDGE_PREVIEWS_PER_HIT);
    previews.truncate(MAX_BRIDGE_PREVIEWS_PER_HIT);
    (previews, omitted)
}

#[allow(clippy::too_many_arguments)]
fn render_response(
    generation: &PublishedGeneration,
    envelope: &crate::domain::SourceResponseEnvelope,
    input: &SearchKnowledgeInput,
    hits: &[KnowledgeHit],
    no_match: Option<&str>,
    overflow: usize,
    withheld_sensitive: usize,
    filtered: FilteredCounts,
) -> String {
    let source_version = format!(
        "branch={};commit={};working_tree={}",
        envelope
            .source_version
            .branch
            .as_deref()
            .unwrap_or("not_applicable"),
        envelope
            .source_version
            .commit
            .as_deref()
            .unwrap_or("not_applicable"),
        snake_debug(envelope.source_version.working_tree)
    );
    let source_id = envelope.source.source_id.as_str();
    let source_location = match &envelope.source.location {
        SourceLocation::WorkingTree { .. } => "current",
        SourceLocation::GitRef { .. } => "ref",
    };
    let freshness = freshness_label(&envelope.freshness);
    let coverage = coverage_label(envelope.coverage);
    let overall_coverage = if response_is_degraded(generation) {
        "degraded"
    } else {
        "complete"
    };
    let path_scope = input.path_prefix.as_deref().unwrap_or("repository");

    let mut output = format!(
        "Trust: exact repository knowledge evidence | publication={} | content={} | source={} | coverage={}\n\
         Secret policy: version {}\n\
         Scope: current + {}\n\
         Source: source={} source_id={} source_version={} publication={} content={} freshness={} coverage={} manifest_digest={}\n\
         Derived: authority_rule_version={} policy_version={} secret_policy_version={} bridge_coverage={} authority_coverage={} overall_coverage={}\n\
         Counts: overflow={} withheld_sensitive={} filtered_current={} filtered_intent={} filtered_history_only={} filtered_suppressed={} filtered_review_required={} filtered_unknown={}",
        envelope.publication_generation,
        envelope.content_generation,
        source_location,
        overall_coverage,
        generation.authority.versions.secret_policy_version,
        path_scope,
        source_location,
        source_id,
        source_version,
        envelope.publication_generation,
        envelope.content_generation,
        freshness,
        coverage,
        envelope.manifest_digest,
        generation.authority.versions.authority_rule_version,
        generation.authority.versions.policy_version,
        generation.authority.versions.secret_policy_version,
        derived_coverage_label(&generation.bridge.coverage),
        derived_coverage_label(&generation.authority.coverage),
        overall_coverage,
        overflow,
        withheld_sensitive,
        filtered.current,
        filtered.intent,
        filtered.history_only,
        filtered.suppressed,
        filtered.review_required,
        filtered.unknown,
    );

    if let Some(no_match) = no_match {
        output.push_str(&format!("\nNo match: {no_match}"));
        return output;
    }

    for (index, hit) in hits.iter().enumerate() {
        output.push_str(&format!(
            "\n{}. {}:{} | heading={} | excerpt=\"{}\" | source=current content_hash={} publication={} content={} line_range={}..{} | authority: lifecycle={} domain={} code={} voice={} coverage={} | finding_ids=[{}] omitted={} provenance_ids=[{}] omitted={} | bridge_previews=[{}] omitted={}",
            index + 1,
            hit.path,
            hit.line,
            if hit.heading_path.is_empty() {
                "(no heading)".to_string()
            } else {
                hit.heading_path.join(" > ")
            },
            hit.excerpt,
            hit.content_hash,
            hit.publication_generation,
            hit.content_generation,
            hit.line_range.start,
            hit.line_range.end,
            hit.authority.lifecycle,
            hit.authority.authority_domain,
            hit.authority.code_evidence,
            hit.authority.voice,
            hit.authority.coverage,
            hit.authority.finding_ids.join(","),
            hit.authority.finding_ids_omitted,
            hit.authority.provenance_ids.join(","),
            hit.authority.provenance_ids_omitted,
            hit.bridge_previews.join(" | "),
            hit.bridge_previews_omitted,
        ));
    }
    output
}

fn significant_terms(query: &str) -> Vec<String> {
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

fn normalize_path_prefix(input: Option<&str>) -> Result<Option<String>, String> {
    let Some(raw) = input.map(str::trim).filter(|raw| !raw.is_empty()) else {
        return Ok(None);
    };
    let replaced = raw.replace('\\', "/");
    if replaced.starts_with('/')
        || replaced.starts_with("//")
        || replaced.as_bytes().get(1) == Some(&b':')
    {
        return Err("Error: invalid path_prefix; expected a normalized repository-relative path without traversal."
            .to_string());
    }
    let mut components = Vec::new();
    for component in replaced.split('/') {
        match component {
            "" | "." => {}
            ".." => {
                return Err("Error: invalid path_prefix; expected a normalized repository-relative path without traversal."
                    .to_string());
            }
            component => components.push(component),
        }
    }
    Ok((!components.is_empty()).then(|| components.join("/")))
}

fn path_in_scope(path: &str, prefix: Option<&str>) -> bool {
    let Some(prefix) = prefix else {
        return true;
    };
    path == prefix
        || path
            .strip_prefix(prefix)
            .is_some_and(|suffix| suffix.starts_with('/'))
}

fn voice_allowed(voice: KnowledgeVoice, scope: KnowledgeAuthorityScope) -> bool {
    match scope {
        KnowledgeAuthorityScope::Default => matches!(
            voice,
            KnowledgeVoice::Current
                | KnowledgeVoice::Intent
                | KnowledgeVoice::NeedsReview
                | KnowledgeVoice::Unknown
        ),
        KnowledgeAuthorityScope::Current => matches!(
            voice,
            KnowledgeVoice::Current | KnowledgeVoice::NeedsReview | KnowledgeVoice::Unknown
        ),
        KnowledgeAuthorityScope::Intent => voice == KnowledgeVoice::Intent,
        KnowledgeAuthorityScope::History => {
            matches!(
                voice,
                KnowledgeVoice::HistoryOnly | KnowledgeVoice::Suppressed
            )
        }
        KnowledgeAuthorityScope::All => true,
    }
}

fn note_filtered_voice(counts: &mut FilteredCounts, voice: KnowledgeVoice) {
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

pub(crate) fn source_scope_label(scope: KnowledgeSourceScope) -> &'static str {
    match scope {
        KnowledgeSourceScope::Current => "current",
        KnowledgeSourceScope::Worktrees => "worktrees",
        KnowledgeSourceScope::LocalRefs => "local_refs",
        KnowledgeSourceScope::All => "all",
    }
}

fn coverage_label(coverage: CoverageStatus) -> &'static str {
    match coverage {
        CoverageStatus::Complete => "complete",
        CoverageStatus::Degraded => "degraded",
    }
}

fn freshness_label(freshness: &FreshnessStatus) -> &'static str {
    match freshness {
        FreshnessStatus::Current => "current",
        FreshnessStatus::Verifying => "verifying",
        FreshnessStatus::Degraded { .. } => "degraded_last_valid",
    }
}

fn derived_coverage_label(coverage: &DerivedCoverage) -> &'static str {
    match coverage {
        DerivedCoverage::Complete => "complete",
        DerivedCoverage::Truncated { .. } => "truncated",
    }
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
        BridgeResolution::ResolvedExact(anchor) => {
            format!("exact:{}", code_anchor_label(&anchor.id))
        }
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

fn code_anchor_label(anchor: &CodeAnchorId) -> String {
    match anchor {
        CodeAnchorId::File { path } => format!("file:{path}"),
        CodeAnchorId::Symbol { symbol, start_line } => {
            format!("symbol:{symbol:?}:{start_line}")
        }
    }
}

fn snake_debug(value: impl std::fmt::Debug) -> String {
    let debug = format!("{value:?}");
    let mut output = String::with_capacity(debug.len() + 4);
    let mut previous_lowercase = false;
    for character in debug.chars() {
        if character.is_ascii_uppercase() {
            if previous_lowercase {
                output.push('_');
            }
            output.push(character.to_ascii_lowercase());
            previous_lowercase = false;
        } else {
            previous_lowercase = character.is_ascii_lowercase() || character.is_ascii_digit();
            output.push(character);
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::live_index::LiveIndex;

    fn input(query: &str) -> SearchKnowledgeInput {
        SearchKnowledgeInput {
            query: query.to_string(),
            path_prefix: None,
            source_scope: Some(KnowledgeSourceScope::Current),
            authority_scope: Some(KnowledgeAuthorityScope::Default),
            project: None,
            projects: None,
            limit: Some(10),
            max_tokens: None,
        }
    }

    fn bracketed_field(output: &str, field: &str) -> String {
        let marker = format!("{field}=[");
        let start = output.find(&marker).expect("field marker") + marker.len();
        let end = output[start..].find(']').expect("field terminator") + start;
        output[start..end].to_string()
    }

    fn with_readiness(
        generation: &PublishedGeneration,
        status: PublishedIndexStatus,
        freshness: FreshnessStatus,
        keep_manifest: bool,
    ) -> PublishedGeneration {
        let mut health = generation.health.as_ref().clone();
        health.status = status;
        PublishedGeneration {
            publication_generation: generation.publication_generation,
            content_generation: generation.content_generation,
            project_generation: generation.project_generation,
            source: generation.source.clone(),
            source_version: generation.source_version.clone(),
            freshness: Arc::new(freshness),
            manifest: keep_manifest.then(|| generation.manifest.clone()).flatten(),
            code_signals: Arc::clone(&generation.code_signals),
            bridge: Arc::clone(&generation.bridge),
            authority: Arc::clone(&generation.authority),
            live: Arc::clone(&generation.live),
            health: Arc::new(health),
            outline: Arc::clone(&generation.outline),
        }
    }

    #[test]
    fn captured_generation_stays_coherent_across_later_publication() {
        let project = tempfile::tempdir().expect("project");
        std::fs::create_dir_all(project.path().join("docs")).expect("docs");
        let document = project.path().join("docs").join("recovery.md");
        std::fs::write(
            &document,
            "# Recovery\nOld persistence boundary remains visible.\n",
        )
        .expect("old document");

        let shared = LiveIndex::load(project.path()).expect("load old generation");
        let captured = shared.published_source_set().current_generation();

        std::fs::write(
            &document,
            "# Recovery\nNew persistence boundary is now visible.\n",
        )
        .expect("new document");
        shared
            .reload(project.path())
            .expect("publish new generation");
        let next = shared.published_source_set().current_generation();
        assert_ne!(
            captured.publication_generation, next.publication_generation,
            "fixture must publish a distinct generation"
        );

        let old_output = search_current(&captured, &input("persistence boundary"));
        assert!(
            old_output.contains("Old persistence boundary"),
            "old: {old_output}"
        );
        assert!(
            !old_output.contains("New persistence boundary"),
            "captured response mixed a later publication: {old_output}"
        );

        let new_output = search_current(&next, &input("persistence boundary"));
        assert!(
            new_output.contains("New persistence boundary"),
            "new: {new_output}"
        );
        assert!(
            !new_output.contains("Old persistence boundary"),
            "next call served the prior publication: {new_output}"
        );
    }

    #[test]
    fn stable_finding_and_provenance_ids_survive_derived_only_republication() {
        let project = tempfile::tempdir().expect("project");
        std::fs::create_dir_all(project.path().join("docs")).expect("docs");
        std::fs::write(
            project.path().join("docs").join("decision.md"),
            "# Decision\nCurrent boundary evidence.\n",
        )
        .expect("document");
        std::fs::write(
            project.path().join(".symforge-knowledge.toml"),
            "version = 1\n[[entries]]\nentry_id = \"stale-entry\"\nlifecycle = \"archived\"\njustification_code = \"stale\"\n[entries.target]\npath = \"docs/decision.md\"\ncontent_hash = \"stale-hash\"\n",
        )
        .expect("policy");

        let shared = LiveIndex::load(project.path()).expect("load generation");
        let before = shared.published_source_set().current_generation();
        let before_output = search_current(&before, &input("boundary evidence"));
        let finding_ids = bracketed_field(&before_output, "finding_ids");
        let provenance_ids = bracketed_field(&before_output, "provenance_ids");
        assert!(
            !finding_ids.is_empty(),
            "fixture must emit a finding: {before_output}"
        );
        assert!(
            !provenance_ids.is_empty(),
            "fixture must emit provenance: {before_output}"
        );

        let prepared = shared.prepare_authority_rebuild();
        assert!(
            shared.publish_prepared_authority(prepared),
            "derived-only publication must be accepted"
        );
        let after = shared.published_source_set().current_generation();
        assert_eq!(before.content_generation, after.content_generation);
        assert_ne!(before.publication_generation, after.publication_generation);

        let after_output = search_current(&after, &input("boundary evidence"));
        assert_eq!(finding_ids, bracketed_field(&after_output, "finding_ids"));
        assert_eq!(
            provenance_ids,
            bracketed_field(&after_output, "provenance_ids")
        );
    }

    #[test]
    fn readiness_and_degraded_last_valid_never_claim_complete_absence() {
        let project = tempfile::tempdir().expect("project");
        std::fs::create_dir_all(project.path().join("docs")).expect("docs");
        std::fs::write(
            project.path().join("docs").join("recovery.md"),
            "# Recovery\nRetained evidence remains available.\n",
        )
        .expect("document");
        let shared = LiveIndex::load(project.path()).expect("load generation");
        let current = shared.published_source_set().current_generation();

        let loading = with_readiness(
            &current,
            PublishedIndexStatus::Loading,
            FreshnessStatus::Verifying,
            true,
        );
        let loading_output = search_current(&loading, &input("missing evidence"));
        assert!(
            loading_output.contains("index_scouting_or_verifying")
                && !loading_output.contains("no_evidence_complete"),
            "loading readiness must not masquerade as complete absence: {loading_output}"
        );

        let no_valid = with_readiness(
            &current,
            PublishedIndexStatus::Empty,
            FreshnessStatus::Verifying,
            false,
        );
        let no_valid_output = search_current(&no_valid, &input("missing evidence"));
        assert!(
            no_valid_output.contains("no_valid_source")
                && !no_valid_output.contains("no_evidence_complete"),
            "an absent valid snapshot must return recovery guidance: {no_valid_output}"
        );

        let verifying = with_readiness(
            &current,
            PublishedIndexStatus::Ready,
            FreshnessStatus::Verifying,
            true,
        );
        let degraded_miss = search_current(&verifying, &input("orbital zebra lattice"));
        assert!(
            degraded_miss.contains("freshness=verifying")
                && degraded_miss.contains("overall_coverage=degraded")
                && degraded_miss.contains("no_evidence_degraded")
                && !degraded_miss.contains("no_evidence_complete"),
            "verifying last-valid no-match must remain explicitly degraded: {degraded_miss}"
        );
        let degraded_hit = search_current(&verifying, &input("retained evidence"));
        assert!(
            degraded_hit.contains("docs/recovery.md:2")
                && degraded_hit.contains("freshness=verifying")
                && degraded_hit.contains("overall_coverage=degraded"),
            "last-valid evidence may be served only with degraded provenance: {degraded_hit}"
        );
    }

    #[test]
    fn sensitive_source_version_is_withheld_before_any_hit_is_formatted() {
        let project = tempfile::tempdir().expect("project");
        std::fs::create_dir_all(project.path().join("docs")).expect("docs");
        std::fs::write(
            project.path().join("docs").join("recovery.md"),
            "# Recovery\nSafe checkpoint evidence.\n",
        )
        .expect("document");
        let shared = LiveIndex::load(project.path()).expect("load generation");
        let current = shared.published_source_set().current_generation();
        let canary = ["runtime", "-", "canary", "-", "segment"].concat();
        let mut source_version = current
            .source_version
            .as_ref()
            .expect("source version")
            .as_ref()
            .clone();
        source_version.branch = Some(format!("token={canary}"));
        let unsafe_generation = PublishedGeneration {
            publication_generation: current.publication_generation,
            content_generation: current.content_generation,
            project_generation: current.project_generation,
            source: current.source.clone(),
            source_version: Some(Arc::new(source_version)),
            freshness: Arc::clone(&current.freshness),
            manifest: current.manifest.clone(),
            code_signals: Arc::clone(&current.code_signals),
            bridge: Arc::clone(&current.bridge),
            authority: Arc::clone(&current.authority),
            live: Arc::clone(&current.live),
            health: Arc::clone(&current.health),
            outline: Arc::clone(&current.outline),
        };

        let output = search_current(&unsafe_generation, &input("checkpoint evidence"));
        assert!(
            output.contains("evidence_withheld"),
            "unsafe source provenance must fail closed"
        );
        assert!(
            !output.contains(&canary),
            "unsafe source provenance must never be echoed"
        );
        assert!(
            !output.contains("docs/recovery.md"),
            "no hit may be formatted under unsafe source provenance"
        );
    }

    #[test]
    fn unknown_lifecycle_units_remain_visible_at_default_authority_scope() {
        let project = tempfile::tempdir().expect("project");
        std::fs::create_dir_all(project.path().join("docs")).expect("docs");
        std::fs::write(
            project.path().join("docs").join("notes.md"),
            "# Notes\nXylophone lifecycle canary with no declared status.\n",
        )
        .expect("undeclared document");
        let shared = LiveIndex::load(project.path()).expect("load generation");
        let current = shared.published_source_set().current_generation();

        let default_output = search_current(&current, &input("xylophone lifecycle canary"));
        assert!(
            default_output.contains("docs/notes.md"),
            "default authority_scope must still surface an unevidenced unit: {default_output}"
        );
        assert!(
            default_output.contains("lifecycle=unknown"),
            "the hit must report the lifecycle that was actually observed: {default_output}"
        );
        assert!(
            default_output.contains("voice=unknown"),
            "an unevidenced lifecycle must not be consumed as voice=current: {default_output}"
        );

        let mut history = input("xylophone lifecycle canary");
        history.authority_scope = Some(KnowledgeAuthorityScope::History);
        let history_output = search_current(&current, &history);
        assert!(
            !history_output.contains("docs/notes.md"),
            "history scope must not surface an Unknown-voice unit, or the default-scope hit is unfiltered: {history_output}"
        );
    }
}
