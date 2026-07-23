//! Read-only repository-knowledge review over one captured publication.

use std::collections::{BTreeMap, BTreeSet};

use crate::domain::{
    CoverageStatus, HistoryCoverage, SourceIdentity, SourceLocation, SourceResponseEnvelope,
};
use crate::knowledge::{guard_hit, guard_query};
use crate::live_index::knowledge_authority::{
    CodeEvidenceDisplay, EvidenceConfidence, KnowledgeAuthorityRecord, KnowledgePolicyTarget,
    KnowledgeVoice, RemediationAction, RemediationPrecondition, TimeEvidence,
};
use crate::live_index::knowledge_bridge::{
    BridgeEvidenceKind, BridgeResolution, CodeAnchor, KnowledgeAnchor, KnowledgeLinkResolution,
    KnowledgeRole,
};
use crate::live_index::store::PublishedIndexStatus;
use crate::live_index::{PublishedGeneration, PublishedSourceSet};

use super::search_tools::{KnowledgeSourceScope, ReviewKnowledgeInput, ReviewKnowledgeMode};

const DEFAULT_LIMIT: usize = 10;
const MAX_LIMIT: usize = 100;
const MIN_PROVENANCE_TOKENS: u64 = 128;

#[derive(Clone, Debug)]
struct NormalizedReview {
    mode: ReviewKnowledgeMode,
    path: Option<String>,
    path_prefix: Option<String>,
    limit: usize,
}

#[derive(Clone, Debug)]
struct ReviewFacts {
    record_index: usize,
    unit_digest: String,
    duplicate_count: usize,
    retained_duplicate: Option<KnowledgeAnchor>,
    protected_roles: Vec<KnowledgeRole>,
    inbound_current_ids: Vec<String>,
    ownership_link_ids: Vec<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct ReviewKnowledgeOutput {
    pub(crate) rendered: String,
    pub(crate) budget_rendered: String,
    pub(crate) source_section: String,
    pub(crate) budget_source_section: String,
    pub(crate) source_key: String,
    pub(crate) review_hash: String,
    pub(crate) result_hash: String,
}

#[derive(Clone, Debug)]
pub(crate) struct CurationReviewAction {
    pub(crate) action_id: String,
    pub(crate) target: KnowledgePolicyTarget,
    pub(crate) proposal_action: RemediationAction,
    pub(crate) proposal_evidence_ids: Vec<String>,
    pub(crate) unmet_preconditions: Vec<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct CurationReviewPlan {
    pub(crate) review_hash: String,
    pub(crate) manifest_digest: String,
    pub(crate) policy_digest: String,
    pub(crate) publication_generation: u64,
    pub(crate) source: SourceIdentity,
    pub(crate) actions: BTreeMap<String, CurationReviewAction>,
}

pub(crate) fn validate_input(input: &ReviewKnowledgeInput) -> Result<(), String> {
    normalize_input(input).map(|_| ())
}

fn normalize_input(input: &ReviewKnowledgeInput) -> Result<NormalizedReview, String> {
    for (label, value) in [
        ("path", input.path.as_deref()),
        ("path_prefix", input.path_prefix.as_deref()),
        ("project", input.project.as_deref()),
    ] {
        if value.is_some_and(|value| guard_query(value).is_err()) {
            return Err(format!(
                "Error: sensitive {label} rejected by repository safety policy."
            ));
        }
    }
    if input
        .projects
        .as_ref()
        .is_some_and(|projects| projects.iter().any(|project| guard_query(project).is_err()))
    {
        return Err(
            "Error: sensitive projects selector rejected by repository safety policy.".to_string(),
        );
    }
    if input.project.is_some() && input.projects.is_some() {
        return Err("Error: project and projects are mutually exclusive.".to_string());
    }
    if input.projects.as_ref().is_some_and(Vec::is_empty) {
        return Err("Error: projects must not be empty.".to_string());
    }
    if let Some(projects) = input.projects.as_ref()
        && projects.iter().any(|project| project == "*")
        && projects.len() != 1
    {
        return Err("Error: projects wildcard must be the sole selector.".to_string());
    }
    if let Some(max_tokens) = input.max_tokens
        && max_tokens < MIN_PROVENANCE_TOKENS
    {
        return Err(format!(
            "Error: max_tokens is too small for provenance; minimum is {MIN_PROVENANCE_TOKENS}."
        ));
    }

    let path = normalize_path(input.path.as_deref(), "path")?;
    let path_prefix = normalize_path(input.path_prefix.as_deref(), "path_prefix")?;
    match input.mode {
        ReviewKnowledgeMode::Document if path.is_none() => {
            return Err("Error: document mode requires one exact path.".to_string());
        }
        ReviewKnowledgeMode::Document if path_prefix.is_some() => {
            return Err("Error: document mode does not accept path_prefix.".to_string());
        }
        _ if path.is_some() && path_prefix.is_some() => {
            return Err("Error: path and path_prefix are mutually exclusive.".to_string());
        }
        _ => {}
    }

    Ok(NormalizedReview {
        mode: input.mode,
        path,
        path_prefix,
        limit: usize::try_from(input.limit.unwrap_or(DEFAULT_LIMIT as u32))
            .unwrap_or(MAX_LIMIT)
            .clamp(1, MAX_LIMIT),
    })
}

pub(crate) fn review_current(
    generation: &PublishedGeneration,
    input: &ReviewKnowledgeInput,
) -> Result<ReviewKnowledgeOutput, String> {
    let normalized = normalize_input(input)?;
    match generation.health.status {
        PublishedIndexStatus::Loading => {
            return Err(
                "Readiness: index_scouting_or_verifying; retry after the current publication completes."
                    .to_string(),
            );
        }
        PublishedIndexStatus::Empty if generation.manifest.is_none() => {
            return Err(
                "Readiness: no_valid_source; run index_folder to rebuild from repository source."
                    .to_string(),
            );
        }
        _ => {}
    }
    let envelope = generation.source_response_envelope().ok_or_else(|| {
        "Readiness: no_valid_source; source envelope is unavailable and no evidence was served."
            .to_string()
    })?;
    if !source_envelope_is_safe(&envelope) {
        return Err(
            "Evidence withheld: source identity or version matched repository safety policy."
                .to_string(),
        );
    }

    let mut selected: Vec<usize> = generation
        .authority
        .records
        .iter()
        .enumerate()
        .filter(|(_, record)| record_in_scope(record, &normalized))
        .map(|(index, _)| index)
        .collect();
    if normalized.mode == ReviewKnowledgeMode::Document && selected.is_empty() {
        return Err("No reviewable knowledge units found for the requested document.".to_string());
    }

    let facts = review_facts(generation, &selected)?;

    selected.sort_by(|left, right| {
        let left_record = &generation.authority.records[*left];
        let right_record = &generation.authority.records[*right];
        let rank = if normalized.mode == ReviewKnowledgeMode::Remediation {
            remediation_rank(&effective_action(left_record, &facts[left]))
                .cmp(&remediation_rank(&effective_action(
                    right_record,
                    &facts[right],
                )))
                .then_with(|| {
                    confidence_rank(effective_confidence(left_record, &facts[left])).cmp(
                        &confidence_rank(effective_confidence(right_record, &facts[right])),
                    )
                })
        } else {
            std::cmp::Ordering::Equal
        };
        rank.then_with(|| {
            anchor_sort_key(&left_record.unit).cmp(&anchor_sort_key(&right_record.unit))
        })
    });

    let summary = render_summary(generation, &selected, &facts);
    let canonical_dossiers = selected
        .iter()
        .map(|index| render_dossier(generation, &facts[index], true))
        .collect::<Vec<_>>()
        .join("\n");
    let canonical_plan = format!(
        "symforge-review-plan-v1\n{}\nmode={}\npath={}\npath_prefix={}\n{}\n{}",
        canonical_source(generation, &envelope),
        snake_debug(normalized.mode),
        normalized.path.as_deref().unwrap_or(""),
        normalized.path_prefix.as_deref().unwrap_or(""),
        summary,
        canonical_dossiers,
    );
    let review_hash = stable_hash("symforge-review-plan-v1", &canonical_plan);
    let source_key = envelope.source.source_id.as_str().to_string();
    let result_hash = combined_result_hash(&[(source_key.clone(), review_hash.clone())]);

    let shown = if normalized.mode == ReviewKnowledgeMode::Summary {
        Vec::new()
    } else {
        selected
            .iter()
            .take(normalized.limit)
            .map(|index| render_dossier(generation, &facts[index], false))
            .collect::<Vec<_>>()
    };
    let shown_count = shown.len();
    let overflow = if normalized.mode == ReviewKnowledgeMode::Summary {
        0
    } else {
        selected.len().saturating_sub(shown_count)
    };
    let source_section = format!(
        "{}\nmode={}\nreview_hash={}\ntotal_dossiers={} shown_dossiers={} overflow={}\n{}{}",
        render_source_header(generation, &envelope),
        snake_debug(normalized.mode),
        review_hash,
        selected.len(),
        shown_count,
        overflow,
        summary,
        if shown.is_empty() {
            String::new()
        } else {
            format!("\n{}", shown.join("\n"))
        },
    );
    let rendered = format!("top_result_hash={result_hash}\n{source_section}");
    let index_entries = if normalized.mode == ReviewKnowledgeMode::Summary {
        Vec::new()
    } else {
        selected
            .iter()
            .take(normalized.limit)
            .map(|index| render_dossier_index(generation, &facts[index]))
            .collect::<Vec<_>>()
    };
    let indexed_count = index_entries.len();
    let budget_source_section = format!(
        "review_hash={review_hash}\nmode={} total_dossiers={} indexed_dossiers={} detail_dossiers=0 overflow={} output_coverage=degraded\n{}\n{}{}",
        snake_debug(normalized.mode),
        selected.len(),
        indexed_count,
        selected.len().saturating_sub(indexed_count),
        render_source_header(generation, &envelope),
        summary,
        if index_entries.is_empty() {
            String::new()
        } else {
            format!("\n{}", index_entries.join("\n"))
        },
    );
    let budget_rendered = format!("top_result_hash={result_hash}\n{budget_source_section}");
    if guard_hit(&rendered, &[rendered.as_str()]).is_err() {
        return Err(
            "Evidence withheld: formatted review matched repository safety policy.".to_string(),
        );
    }
    Ok(ReviewKnowledgeOutput {
        rendered,
        budget_rendered,
        source_section,
        budget_source_section,
        source_key,
        review_hash,
        result_hash,
    })
}

/// Compose `review_knowledge` across the scope-selected sources of one captured
/// source set (Gate L L-G06). `current` keeps its exact single-source output. A
/// multi-source scope with no sources returns a typed empty readiness result.
/// The top result hash aggregates every per-source `(source_id, review_hash)`
/// pair deterministically, so equal generations produce a byte-identical result.
pub(crate) fn review_scoped(
    source_set: &PublishedSourceSet,
    input: &ReviewKnowledgeInput,
) -> Result<ReviewKnowledgeOutput, String> {
    normalize_input(input)?;
    let scope = input.source_scope.unwrap_or(KnowledgeSourceScope::Current);
    if matches!(scope, KnowledgeSourceScope::Current) {
        return review_current(&source_set.current_generation(), input);
    }
    let selected = super::knowledge_search::select_scoped_sources(source_set, scope);
    let scope_label = super::knowledge_search::source_scope_label(scope);
    if selected.is_empty() {
        return Err(format!(
            "Readiness: no_sources_in_scope; source_scope '{scope_label}' selected no sources."
        ));
    }
    let mut pairs: Vec<(String, String)> = Vec::with_capacity(selected.len());
    let mut sections: Vec<String> = Vec::with_capacity(selected.len());
    let mut budget_sections: Vec<String> = Vec::with_capacity(selected.len());
    for generation in &selected {
        let source_key = generation
            .source
            .as_deref()
            .map(|source| source.source_id.as_str().to_string())
            .unwrap_or_else(|| "unbound".to_string());
        match review_current(generation, input) {
            Ok(output) => {
                pairs.push((output.source_key.clone(), output.review_hash.clone()));
                sections.push(output.source_section);
                budget_sections.push(output.budget_source_section);
            }
            Err(readiness) => {
                // An unavailable/empty/degraded lane is a typed per-source
                // outcome, never a whole-scope failure (L-R09). Its readiness is
                // folded into the deterministic result hash so equal generations
                // still produce a byte-identical result.
                pairs.push((source_key.clone(), format!("unavailable:{readiness}")));
                let section = format!("source={source_key}\n{readiness}");
                sections.push(section.clone());
                budget_sections.push(section);
            }
        }
    }
    let result_hash = combined_result_hash(&pairs);
    let source_section = sections.join("\n\n");
    let budget_source_section = budget_sections.join("\n\n");
    let overall_coverage = super::knowledge_search::worst_source_coverage(&selected);
    let header = format!(
        "top_result_hash={result_hash}\nsource_scope={scope_label} sources={} overall_coverage={:?}",
        selected.len(),
        overall_coverage,
    );
    Ok(ReviewKnowledgeOutput {
        rendered: format!("{header}\n\n{source_section}"),
        budget_rendered: format!("{header}\n\n{budget_source_section}"),
        source_section,
        budget_source_section,
        source_key: format!("scope:{scope_label}"),
        review_hash: result_hash.clone(),
        result_hash,
    })
}

pub(crate) fn curation_plan_current(
    generation: &PublishedGeneration,
) -> Result<CurationReviewPlan, String> {
    let review = review_current(
        generation,
        &ReviewKnowledgeInput {
            mode: ReviewKnowledgeMode::Remediation,
            path: None,
            path_prefix: None,
            source_scope: Some(KnowledgeSourceScope::Current),
            project: None,
            projects: None,
            limit: Some(1),
            max_tokens: None,
        },
    )?;
    let selected = (0..generation.authority.records.len()).collect::<Vec<_>>();
    let facts = review_facts(generation, &selected)?;
    let mut actions = BTreeMap::new();
    for index in selected {
        let record = &generation.authority.records[index];
        let facts = &facts[&index];
        let proposal_action = effective_action(record, facts);
        let confidence = effective_confidence(record, facts);
        let action_id = action_id(record, facts, &proposal_action, confidence);
        let unit_hash = policy_unit_hash(generation, &record.unit).ok_or_else(|| {
            "Curation unavailable: reviewed unit bytes are not present in the captured source."
                .to_string()
        })?;
        actions.insert(
            action_id.clone(),
            CurationReviewAction {
                action_id,
                target: KnowledgePolicyTarget {
                    path: record.unit.path.clone(),
                    content_hash: record.unit.content_hash.clone(),
                    unit_byte_range: Some(record.unit.byte_range.clone()),
                    unit_hash: Some(unit_hash),
                },
                proposal_action,
                proposal_evidence_ids: proposal_evidence_ids(record, facts),
                unmet_preconditions: proposal_preconditions(record, facts),
            },
        );
    }
    let manifest_digest = generation
        .manifest
        .as_ref()
        .map(|manifest| manifest.digest.clone())
        .ok_or_else(|| "Curation unavailable: source manifest is absent.".to_string())?;
    let source = generation
        .source
        .as_ref()
        .map(|source| source.as_ref().clone())
        .ok_or_else(|| "Curation unavailable: source identity is absent.".to_string())?;
    Ok(CurationReviewPlan {
        review_hash: review.review_hash,
        manifest_digest,
        policy_digest: generation.authority.policy_digest.clone(),
        publication_generation: generation.publication_generation,
        source,
        actions,
    })
}

fn review_facts(
    generation: &PublishedGeneration,
    selected: &[usize],
) -> Result<BTreeMap<usize, ReviewFacts>, String> {
    let unit_digests: Vec<String> = generation
        .authority
        .records
        .iter()
        .map(|record| unit_digest(generation, &record.unit))
        .collect();
    let mut digest_anchors = BTreeMap::<String, Vec<KnowledgeAnchor>>::new();
    for (index, digest) in unit_digests.iter().enumerate() {
        if digest != "unavailable" {
            digest_anchors
                .entry(digest.clone())
                .or_default()
                .push(generation.authority.records[index].unit.clone());
        }
    }
    for anchors in digest_anchors.values_mut() {
        anchors.sort_by(|left, right| anchor_sort_key(left).cmp(&anchor_sort_key(right)));
        anchors.dedup_by(|left, right| anchor_sort_key(left) == anchor_sort_key(right));
    }
    selected
        .iter()
        .map(|index| {
            let record = &generation.authority.records[*index];
            ensure_record_safe(generation, *index, record)?;
            let digest = unit_digests[*index].clone();
            let duplicates = digest_anchors.get(&digest).cloned().unwrap_or_default();
            let retained_duplicate = duplicates.first().filter(|retained| {
                retained.path != record.unit.path
                    && anchor_sort_key(retained) != anchor_sort_key(&record.unit)
            });
            Ok((
                *index,
                ReviewFacts {
                    record_index: *index,
                    duplicate_count: duplicates.len(),
                    retained_duplicate: retained_duplicate.cloned(),
                    unit_digest: digest,
                    protected_roles: protected_roles(generation, record),
                    inbound_current_ids: inbound_current_links(generation, record),
                    ownership_link_ids: ownership_link_ids(generation, record),
                },
            ))
        })
        .collect()
}

fn render_dossier_index(generation: &PublishedGeneration, facts: &ReviewFacts) -> String {
    let record = &generation.authority.records[facts.record_index];
    let finding_ids = finding_ids(generation, facts.record_index);
    let bridge_records = bridge_records(generation, record);
    let action = effective_action(record, facts);
    let confidence = effective_confidence(record, facts);
    let action_id = action_id(record, facts, &action, confidence);
    let link_ids = bridge_records
        .iter()
        .map(|(_, link)| link.id.0.clone())
        .collect::<Vec<_>>();
    let mut evidence_locations = BTreeSet::from([format!(
        "{}@{}..{}",
        record.unit.path, record.unit.byte_range.start, record.unit.byte_range.end
    )]);
    evidence_locations.extend(bridge_records.iter().map(|(_, link)| {
        format!(
            "{}@{}..{}",
            link.evidence.path, link.evidence.byte_range.start, link.evidence.byte_range.end
        )
    }));
    evidence_locations.extend(exact_code_anchors(record, &bridge_records));
    format!(
        "review_index unit={}@{}..{} finding_ids=[{}] action_id={} link_ids=[{}] evidence_locations=[{}]",
        record.unit.path,
        record.unit.byte_range.start,
        record.unit.byte_range.end,
        join_strings(&finding_ids),
        action_id,
        join_strings(&link_ids),
        evidence_locations.into_iter().collect::<Vec<_>>().join(";"),
    )
}

pub(crate) fn combined_result_hash(plans: &[(String, String)]) -> String {
    let mut canonical = plans.to_vec();
    canonical.sort();
    canonical.dedup();
    let body = canonical
        .iter()
        .map(|(source, review)| format!("{source}\t{review}"))
        .collect::<Vec<_>>()
        .join("\n");
    stable_hash("symforge-review-result-v1", &body)
}

fn record_in_scope(record: &KnowledgeAuthorityRecord, input: &NormalizedReview) -> bool {
    if let Some(path) = input.path.as_deref() {
        return record.unit.path == path;
    }
    path_in_scope(&record.unit.path, input.path_prefix.as_deref())
}

fn render_source_header(
    generation: &PublishedGeneration,
    envelope: &SourceResponseEnvelope,
) -> String {
    let manifest_coverage = match envelope.coverage {
        CoverageStatus::Complete => "complete",
        CoverageStatus::Degraded => "degraded",
    };
    format!(
        "Review knowledge | source_id={} repository_id={} location={} branch={} commit={} working_tree={} publication_generation={} content_generation={} project_generation={} freshness={} manifest_digest={} policy_digest={} manifest_coverage={} bridge_coverage={} authority_coverage={}",
        envelope.source.source_id.as_str(),
        envelope.source.repository_id.as_str(),
        source_location_label(&envelope.source.location),
        envelope.source_version.branch.as_deref().unwrap_or("none"),
        envelope.source_version.commit.as_deref().unwrap_or("none"),
        snake_debug(envelope.source_version.working_tree),
        envelope.publication_generation,
        envelope.content_generation,
        generation.project_generation,
        snake_debug(&envelope.freshness),
        envelope.manifest_digest,
        generation.authority.policy_digest,
        manifest_coverage,
        derived_coverage_label(&generation.bridge.coverage),
        derived_coverage_label(&generation.authority.coverage),
    )
}

fn canonical_source(generation: &PublishedGeneration, envelope: &SourceResponseEnvelope) -> String {
    render_source_header(generation, envelope)
}

fn render_summary(
    generation: &PublishedGeneration,
    selected: &[usize],
    facts: &BTreeMap<usize, ReviewFacts>,
) -> String {
    let mut lifecycle = BTreeMap::<String, usize>::new();
    let mut domain = BTreeMap::<String, usize>::new();
    let mut evidence = BTreeMap::<String, usize>::new();
    let mut voice = BTreeMap::<String, usize>::new();
    let mut oldest: Option<i64> = None;
    let mut review_due = 0usize;
    let mut broken = 0usize;
    let mut conflicting = 0usize;
    let mut duplicate_units = 0usize;
    let mut protected_units = 0usize;
    for index in selected {
        let record = &generation.authority.records[*index];
        *lifecycle.entry(snake_debug(record.lifecycle)).or_default() += 1;
        *domain
            .entry(snake_debug(record.authority_domain))
            .or_default() += 1;
        *evidence
            .entry(snake_debug(record.code_evidence.display))
            .or_default() += 1;
        *voice.entry(snake_debug(record.voice)).or_default() += 1;
        for timestamp in [
            record.timeline.git_first_seen.as_ref(),
            record.timeline.git_last_touch.as_ref(),
        ]
        .into_iter()
        .flatten()
        .filter_map(|time| time.unix_seconds)
        {
            oldest = Some(oldest.map_or(timestamp, |current| current.min(timestamp)));
        }
        review_due += usize::from(record.code_evidence.display == CodeEvidenceDisplay::ReviewDue);
        broken += usize::from(!record.code_evidence.broken_link_indices.is_empty());
        conflicting += usize::from(!record.code_evidence.deterministic_conflict_ids.is_empty());
        duplicate_units += usize::from(facts[index].duplicate_count > 1);
        protected_units += usize::from(!facts[index].protected_roles.is_empty());
    }
    format!(
        "summary.total={} lifecycle=[{}] domain=[{}] evidence=[{}] voice=[{}] oldest_unix_seconds={} review_due={} broken={} conflicting={} duplicate_units={} protected_units={} skipped_suppression_ids=[{}] coverage.manifest={} coverage.bridge={} coverage.authority={}",
        selected.len(),
        render_counts(&lifecycle),
        render_counts(&domain),
        render_counts(&evidence),
        render_counts(&voice),
        oldest.map_or_else(|| "unavailable".to_string(), |value| value.to_string()),
        review_due,
        broken,
        conflicting,
        duplicate_units,
        protected_units,
        join_strings(&generation.authority.skipped_suppression_ids),
        generation
            .manifest
            .as_ref()
            .map(|manifest| snake_debug(manifest.coverage))
            .unwrap_or_else(|| "unavailable".to_string()),
        derived_coverage_label(&generation.bridge.coverage),
        derived_coverage_label(&generation.authority.coverage),
    )
}

fn render_dossier(
    generation: &PublishedGeneration,
    facts: &ReviewFacts,
    canonical: bool,
) -> String {
    let record = &generation.authority.records[facts.record_index];
    let finding_ids = finding_ids(generation, facts.record_index);
    let bridge_records = bridge_records(generation, record);
    let rule_ids = rule_ids(record, &bridge_records);
    let action = effective_action(record, facts);
    let confidence = effective_confidence(record, facts);
    let action_id = action_id(record, facts, &action, confidence);
    let policy_unit_hash =
        policy_unit_hash(generation, &record.unit).unwrap_or_else(|| "unavailable".to_string());
    let exact_code_anchors = exact_code_anchors(record, &bridge_records);
    let proposal_preconditions = proposal_preconditions(record, facts);
    let proposal_evidence_ids = proposal_evidence_ids(record, facts);
    let role_labels = facts
        .protected_roles
        .iter()
        .map(snake_debug)
        .collect::<Vec<_>>();
    let mut lines = vec![format!(
        "dossier unit={} bytes={}..{} lines={}..{} content_hash={} unit_digest={} policy_unit_hash={} source_id={} content_generation={}",
        record.unit.path,
        record.unit.byte_range.start,
        record.unit.byte_range.end,
        record.unit.line_range.start,
        record.unit.line_range.end,
        record.unit.content_hash,
        facts.unit_digest,
        policy_unit_hash,
        record.unit.source.source_id.as_str(),
        record.unit.content_generation,
    )];
    lines.push(format!(
        "finding_ids=[{}] action_id={} rule_ids=[{}]",
        join_strings(&finding_ids),
        action_id,
        join_strings(&rule_ids),
    ));
    lines.push(format!(
        "lifecycle={} lifecycle_provenance={} authority_domain={} authority_provenance={} voice={}",
        snake_debug(record.lifecycle),
        snake_debug(&record.lifecycle_evidence),
        snake_debug(record.authority_domain),
        snake_debug(&record.authority_domain_evidence),
        snake_debug(record.voice),
    ));
    lines.push(format!(
        "code_evidence.display={} code_evidence.consistent_rule_ids=[{}] code_evidence.broken_link_indices=[{}] code_evidence.deterministic_conflict_ids=[{}] code_evidence.suspected_conflict_ids=[{}] code_evidence.implementation_gap_ids=[{}] code_evidence.relevant_code_change_count={} code_evidence.review_signal_ids=[{}] code_evidence.unresolved_semantics={} code_evidence.not_applicable={} code_evidence.coverage={}",
        snake_debug(record.code_evidence.display),
        join_strings(&record.code_evidence.consistent_rule_ids),
        join_u32(&record.code_evidence.broken_link_indices),
        join_strings(&record.code_evidence.deterministic_conflict_ids),
        join_strings(&record.code_evidence.suspected_conflict_ids),
        join_strings(&record.code_evidence.implementation_gap_ids),
        record.code_evidence.relevant_code_change_count,
        join_strings(&record.code_evidence.review_signal_ids),
        record.code_evidence.unresolved_semantics,
        record.code_evidence.not_applicable,
        derived_coverage_label(&record.code_evidence.coverage),
    ));
    lines.push(format!(
        "bridge_records={} [{}]",
        bridge_records.len(),
        bridge_records
            .iter()
            .map(|(index, link)| format!(
                "index={} link_id={} kind={} evidence={}@{}..{} resolution={}",
                index,
                link.id.0,
                snake_debug(&link.evidence_kind),
                link.evidence.path,
                link.evidence.byte_range.start,
                link.evidence.byte_range.end,
                bridge_resolution_label(&link.resolution),
            ))
            .collect::<Vec<_>>()
            .join("; ")
    ));
    lines.push(format!(
        "timeline.coverage={} timeline.filesystem_created={} timeline.filesystem_modified={} timeline.git_first_seen={} timeline.git_last_touch={} timeline.working_tree_changed={} timeline.relevant_code_changes=[{}]",
        history_coverage_label(&record.timeline.coverage),
        time_evidence_label(record.timeline.filesystem_created.as_ref()),
        time_evidence_label(record.timeline.filesystem_modified.as_ref()),
        time_evidence_label(record.timeline.git_first_seen.as_ref()),
        time_evidence_label(record.timeline.git_last_touch.as_ref()),
        record.timeline.working_tree_changed,
        record
            .timeline
            .relevant_code_changes
            .iter()
            .map(|change| format!(
                "rule_id={} anchor={} commit={} unix_seconds={} topologically_after_document={}",
                change.rule_id,
                code_anchor_label(&change.anchor),
                change.commit_id.as_deref().unwrap_or("unavailable"),
                change
                    .unix_seconds
                    .map_or_else(|| "unavailable".to_string(), |value| value.to_string()),
                change
                    .topologically_after_document
                    .map_or_else(|| "unknown".to_string(), |value| value.to_string()),
            ))
            .collect::<Vec<_>>()
            .join("; ")
    ));
    lines.push(format!(
        "exact_code_anchors=[{}] structured_diffs=[] inbound_current_knowledge_links=[{}] source_local_ownership_evidence=[{}]",
        join_strings(&exact_code_anchors),
        join_strings(&facts.inbound_current_ids),
        join_strings(&facts.ownership_link_ids),
    ));
    lines.push(format!(
        "eligibility.protected_roles=[{}] eligibility.duplicate_count={} eligibility.unique_content={} eligibility.inbound_current_count={} eligibility.successor_coverage={}",
        role_labels.join(","),
        facts.duplicate_count,
        facts.duplicate_count <= 1,
        facts.inbound_current_ids.len(),
        successor_coverage(generation, record),
    ));
    lines.push(format!(
        "proposal.action={} proposal.confidence={} proposal.evidence_ids=[{}] proposal.unmet_preconditions=[{}]",
        remediation_action_label(&action),
        snake_debug(confidence),
        join_strings(&proposal_evidence_ids),
        proposal_preconditions.join(","),
    ));
    if canonical {
        lines.join("\n")
    } else {
        format!("\n{}", lines.join("\n"))
    }
}

fn ensure_record_safe(
    generation: &PublishedGeneration,
    index: usize,
    record: &KnowledgeAuthorityRecord,
) -> Result<(), String> {
    let finding_ids = finding_ids(generation, index);
    let mut visible = vec![record.unit.path.as_str(), record.unit.content_hash.as_str()];
    visible.extend(finding_ids.iter().map(String::as_str));
    if guard_hit(record, &visible).is_err() {
        return Err(
            "Evidence withheld: a selected review record matched repository safety policy."
                .to_string(),
        );
    }
    Ok(())
}

fn finding_ids(generation: &PublishedGeneration, record_index: usize) -> Vec<String> {
    generation
        .authority
        .finding_index
        .iter()
        .filter(|(_, index)| **index as usize == record_index)
        .map(|(id, _)| id.clone())
        .collect()
}

fn bridge_records<'a>(
    generation: &'a PublishedGeneration,
    record: &KnowledgeAuthorityRecord,
) -> Vec<(
    usize,
    &'a crate::live_index::knowledge_bridge::KnowledgeCodeLink,
)> {
    generation
        .bridge
        .forward
        .iter()
        .enumerate()
        .filter(|(_, link)| anchor_in_unit(&link.evidence, &record.unit))
        .collect()
}

fn rule_ids(
    record: &KnowledgeAuthorityRecord,
    bridge_records: &[(
        usize,
        &crate::live_index::knowledge_bridge::KnowledgeCodeLink,
    )],
) -> Vec<String> {
    let mut ids = BTreeSet::new();
    ids.extend(record.code_evidence.consistent_rule_ids.iter().cloned());
    ids.extend(
        record
            .code_evidence
            .deterministic_conflict_ids
            .iter()
            .cloned(),
    );
    ids.extend(record.code_evidence.suspected_conflict_ids.iter().cloned());
    ids.extend(record.code_evidence.implementation_gap_ids.iter().cloned());
    ids.extend(record.code_evidence.review_signal_ids.iter().cloned());
    ids.extend(
        record
            .timeline
            .relevant_code_changes
            .iter()
            .map(|change| change.rule_id.clone()),
    );
    for (_, link) in bridge_records {
        if let BridgeEvidenceKind::SupportedStructuredValue { rule_id } = &link.evidence_kind {
            ids.insert(rule_id.clone());
        }
    }
    ids.into_iter().collect()
}

fn protected_roles(
    generation: &PublishedGeneration,
    record: &KnowledgeAuthorityRecord,
) -> Vec<KnowledgeRole> {
    let mut roles = BTreeSet::new();
    for card in &generation.bridge.cards {
        if anchor_in_unit(&card.anchor, &record.unit) {
            for (role, _) in &card.roles {
                if matches!(
                    role,
                    KnowledgeRole::OwnershipGovernance
                        | KnowledgeRole::DecisionInvariant
                        | KnowledgeRole::TestingSecurity
                ) || matches!(
                    record.authority_domain,
                    crate::live_index::knowledge_authority::AuthorityDomain::NormativeIntent
                        | crate::live_index::knowledge_authority::AuthorityDomain::Decision
                        | crate::live_index::knowledge_authority::AuthorityDomain::Governance
                ) {
                    roles.insert(*role);
                }
            }
        }
    }
    roles.into_iter().collect()
}

fn inbound_current_links(
    generation: &PublishedGeneration,
    record: &KnowledgeAuthorityRecord,
) -> Vec<String> {
    let mut ids = BTreeSet::new();
    for link in &generation.bridge.knowledge_links {
        let KnowledgeLinkResolution::ResolvedExact(target) = &link.resolution else {
            continue;
        };
        if !anchor_in_unit(target, &record.unit)
            || !anchor_has_current_voice(generation, &link.evidence)
        {
            continue;
        }
        let key = format!(
            "{}:{}:{}->{}:{}:{}",
            link.evidence.path,
            link.evidence.byte_range.start,
            link.evidence.content_hash,
            target.path,
            target.byte_range.start,
            target.content_hash,
        );
        ids.insert(format!(
            "klink-{}",
            &stable_hash("knowledge-link-v1", &key)[..20]
        ));
    }
    ids.into_iter().collect()
}

fn anchor_has_current_voice(generation: &PublishedGeneration, anchor: &KnowledgeAnchor) -> bool {
    generation.authority.records.iter().any(|record| {
        anchor_in_unit(anchor, &record.unit) && record.voice == KnowledgeVoice::Current
    })
}

fn ownership_link_ids(
    generation: &PublishedGeneration,
    record: &KnowledgeAuthorityRecord,
) -> Vec<String> {
    generation
        .bridge
        .forward
        .iter()
        .filter(|link| {
            matches!(
                link.evidence_kind,
                BridgeEvidenceKind::DeclaredOwnershipSelector
            ) && anchor_in_unit(&link.evidence, &record.unit)
        })
        .map(|link| link.id.0.clone())
        .collect()
}

fn exact_code_anchors(
    record: &KnowledgeAuthorityRecord,
    bridge_records: &[(
        usize,
        &crate::live_index::knowledge_bridge::KnowledgeCodeLink,
    )],
) -> Vec<String> {
    let mut anchors = BTreeSet::new();
    for (_, link) in bridge_records {
        match &link.resolution {
            BridgeResolution::ResolvedExact(anchor) => {
                anchors.insert(code_anchor_label(anchor));
            }
            BridgeResolution::Ambiguous {
                bounded_samples, ..
            } => {
                anchors.extend(bounded_samples.iter().map(code_anchor_label));
            }
            _ => {}
        }
    }
    anchors.extend(
        record
            .timeline
            .relevant_code_changes
            .iter()
            .map(|change| code_anchor_label(&change.anchor)),
    );
    anchors.into_iter().collect()
}

fn proposal_preconditions(record: &KnowledgeAuthorityRecord, facts: &ReviewFacts) -> Vec<String> {
    let mut blockers: BTreeSet<String> = record
        .proposal
        .unmet_preconditions
        .iter()
        .map(remediation_precondition_label)
        .collect();
    let action = effective_action(record, facts);
    let removes_or_combines = matches!(
        action,
        RemediationAction::MergeInto { .. }
            | RemediationAction::Archive
            | RemediationAction::DeletionCandidate { .. }
    );
    let needs_duplicate_coverage = matches!(
        action,
        RemediationAction::MergeInto { .. } | RemediationAction::DeletionCandidate { .. }
    );
    if removes_or_combines && !facts.protected_roles.is_empty() {
        blockers.insert("protected_role".to_string());
    }
    if needs_duplicate_coverage && facts.duplicate_count <= 1 {
        blockers.insert("unique_content_unknown".to_string());
    }
    if removes_or_combines && !facts.inbound_current_ids.is_empty() {
        blockers.insert(format!(
            "inbound_live_links(count={})",
            facts.inbound_current_ids.len()
        ));
    }
    blockers.into_iter().collect()
}

fn successor_coverage(
    generation: &PublishedGeneration,
    record: &KnowledgeAuthorityRecord,
) -> &'static str {
    let Some(successor) = record.successor.as_ref() else {
        return "not_applicable";
    };
    if generation
        .authority
        .records
        .iter()
        .any(|candidate| anchor_in_unit(successor, &candidate.unit))
    {
        "exact"
    } else {
        "missing"
    }
}

fn unit_digest(generation: &PublishedGeneration, anchor: &KnowledgeAnchor) -> String {
    let Some(file) = generation.live.get_file(&anchor.path) else {
        return "unavailable".to_string();
    };
    let Ok(start) = usize::try_from(anchor.byte_range.start) else {
        return "unavailable".to_string();
    };
    let Ok(end) = usize::try_from(anchor.byte_range.end) else {
        return "unavailable".to_string();
    };
    let Some(bytes) = file.content.get(start..end) else {
        return "unavailable".to_string();
    };
    stable_hash(
        "knowledge-unit-v1",
        std::str::from_utf8(bytes).unwrap_or(""),
    )
}

fn policy_unit_hash(generation: &PublishedGeneration, anchor: &KnowledgeAnchor) -> Option<String> {
    let file = generation.live.get_file(&anchor.path)?;
    let start = usize::try_from(anchor.byte_range.start).ok()?;
    let end = usize::try_from(anchor.byte_range.end).ok()?;
    file.content.get(start..end).map(crate::hash::digest_hex)
}

fn action_id(
    record: &KnowledgeAuthorityRecord,
    facts: &ReviewFacts,
    action: &RemediationAction,
    confidence: EvidenceConfidence,
) -> String {
    let canonical = format!(
        "{}:{}:{}:{}:{}",
        record.unit.path,
        record.unit.byte_range.start,
        facts.unit_digest,
        remediation_action_label(action),
        snake_debug(confidence),
    );
    format!(
        "action-{}",
        &stable_hash("knowledge-action-v1", &canonical)[..20]
    )
}

fn effective_action(record: &KnowledgeAuthorityRecord, facts: &ReviewFacts) -> RemediationAction {
    facts
        .retained_duplicate
        .as_ref()
        .map(|retained| RemediationAction::DeletionCandidate {
            retained: retained.clone(),
        })
        .unwrap_or_else(|| record.proposal.action.clone())
}

fn effective_confidence(
    record: &KnowledgeAuthorityRecord,
    facts: &ReviewFacts,
) -> EvidenceConfidence {
    if facts.retained_duplicate.is_some() {
        EvidenceConfidence::StrongCandidate
    } else {
        record.proposal.confidence
    }
}

fn proposal_evidence_ids(record: &KnowledgeAuthorityRecord, facts: &ReviewFacts) -> Vec<String> {
    let mut evidence: BTreeSet<String> = record.proposal.evidence_ids.iter().cloned().collect();
    if facts.retained_duplicate.is_some() {
        evidence.insert(format!("exact-duplicate-{}", &facts.unit_digest[..20]));
    }
    evidence.into_iter().collect()
}

fn time_evidence_label(value: Option<&TimeEvidence>) -> String {
    value.map_or_else(
        || "unavailable".to_string(),
        |value| {
            format!(
                "unix_seconds={} provenance={} coverage={}",
                value
                    .unix_seconds
                    .map_or_else(|| "unavailable".to_string(), |time| time.to_string()),
                snake_debug(value.provenance),
                history_coverage_label(&value.coverage),
            )
        },
    )
}

fn history_coverage_label(coverage: &HistoryCoverage) -> String {
    format!(
        "complete_to_root:{};limitations:[{}]",
        coverage.complete_to_root,
        coverage
            .limitations
            .iter()
            .map(snake_debug)
            .collect::<Vec<_>>()
            .join(",")
    )
}

fn bridge_resolution_label(resolution: &BridgeResolution) -> String {
    match resolution {
        BridgeResolution::ResolvedExact(anchor) => {
            format!("resolved_exact({})", code_anchor_label(anchor))
        }
        BridgeResolution::ResolvedDeclaredSet {
            selector_anchor,
            matched_count,
        } => format!(
            "resolved_declared_set(selector={}@{}..{},matched_count={matched_count})",
            selector_anchor.path, selector_anchor.byte_range.start, selector_anchor.byte_range.end,
        ),
        BridgeResolution::Ambiguous {
            candidate_count,
            bounded_samples,
        } => format!(
            "ambiguous(candidate_count={candidate_count},samples=[{}])",
            bounded_samples
                .iter()
                .map(code_anchor_label)
                .collect::<Vec<_>>()
                .join(",")
        ),
        BridgeResolution::Missing => "missing".to_string(),
    }
}

fn code_anchor_label(anchor: &CodeAnchor) -> String {
    let id = match &anchor.id {
        crate::live_index::knowledge_bridge::CodeAnchorId::File { path } => {
            format!("file:{path}")
        }
        crate::live_index::knowledge_bridge::CodeAnchorId::Symbol { symbol, start_line } => {
            format!(
                "symbol:{}::{}:{}@{start_line}",
                symbol.path, symbol.name, symbol.kind
            )
        }
    };
    format!(
        "{}#{} lines={}..{} content_generation={}",
        id,
        anchor.content_hash,
        anchor.line_range.start,
        anchor.line_range.end,
        anchor.content_generation
    )
}

fn remediation_action_label(action: &RemediationAction) -> String {
    match action {
        RemediationAction::Keep => "keep".to_string(),
        RemediationAction::Update => "update".to_string(),
        RemediationAction::RelabelIntent => "relabel_intent".to_string(),
        RemediationAction::MergeInto { target } => format!("merge(target={})", target.path),
        RemediationAction::MarkSuperseded { successor } => {
            format!("mark_superseded(successor={})", successor.path)
        }
        RemediationAction::Archive => "archive".to_string(),
        RemediationAction::DeletionCandidate { retained } => {
            format!("deletion_candidate(retained={})", retained.path)
        }
        RemediationAction::NeedsReview => "needs_review".to_string(),
    }
}

fn remediation_precondition_label(value: &RemediationPrecondition) -> String {
    match value {
        RemediationPrecondition::ProtectedRole => "protected_role".to_string(),
        RemediationPrecondition::UniqueContentUnknown => "unique_content_unknown".to_string(),
        RemediationPrecondition::InboundLiveLinks { count } => {
            format!("inbound_live_links(count={count})")
        }
        RemediationPrecondition::MissingSuccessor => "missing_successor".to_string(),
        RemediationPrecondition::SuccessorCoverageIncomplete => {
            "successor_coverage_incomplete".to_string()
        }
        RemediationPrecondition::SourceCoverageDegraded => "source_coverage_degraded".to_string(),
        RemediationPrecondition::WorkingTreeDirtyOrUntracked => {
            "working_tree_dirty_or_untracked".to_string()
        }
        RemediationPrecondition::UnsupportedPath => "unsupported_path".to_string(),
        RemediationPrecondition::RequiresUserJudgment => "requires_user_judgment".to_string(),
    }
}

fn remediation_rank(action: &RemediationAction) -> u8 {
    match action {
        RemediationAction::DeletionCandidate { .. } => 0,
        RemediationAction::MarkSuperseded { .. } => 1,
        RemediationAction::Archive => 2,
        RemediationAction::MergeInto { .. } => 3,
        RemediationAction::Update => 4,
        RemediationAction::RelabelIntent => 5,
        RemediationAction::NeedsReview => 6,
        RemediationAction::Keep => 7,
    }
}

fn confidence_rank(confidence: EvidenceConfidence) -> u8 {
    match confidence {
        EvidenceConfidence::Deterministic => 0,
        EvidenceConfidence::StrongCandidate => 1,
        EvidenceConfidence::ReviewSignal => 2,
        EvidenceConfidence::Unresolved => 3,
    }
}

fn anchor_in_unit(anchor: &KnowledgeAnchor, unit: &KnowledgeAnchor) -> bool {
    anchor.source.source_id == unit.source.source_id
        && anchor.path == unit.path
        && anchor.byte_range.start >= unit.byte_range.start
        && anchor.byte_range.end <= unit.byte_range.end
}

fn anchor_sort_key(anchor: &KnowledgeAnchor) -> (&str, u32, u32, &str) {
    (
        anchor.path.as_str(),
        anchor.byte_range.start,
        anchor.byte_range.end,
        anchor.content_hash.as_str(),
    )
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

fn normalize_path(value: Option<&str>, field: &str) -> Result<Option<String>, String> {
    let Some(raw) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    let replaced = raw.replace('\\', "/");
    if replaced.starts_with('/')
        || replaced.starts_with("//")
        || replaced.as_bytes().get(1) == Some(&b':')
    {
        return Err(format!(
            "Error: invalid {field}; expected a normalized repository-relative path without traversal."
        ));
    }
    let mut components = Vec::new();
    for component in replaced.split('/') {
        match component {
            "" | "." => {}
            ".." => {
                return Err(format!(
                    "Error: invalid {field}; expected a normalized repository-relative path without traversal."
                ));
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

fn source_location_label(location: &SourceLocation) -> String {
    match location {
        SourceLocation::WorkingTree { worktree_id } => format!("working_tree:{worktree_id}"),
        SourceLocation::GitRef { name } => format!("git_ref:{name}"),
    }
}

fn derived_coverage_label(
    coverage: &crate::live_index::knowledge_bridge::DerivedCoverage,
) -> String {
    match coverage {
        crate::live_index::knowledge_bridge::DerivedCoverage::Complete => "complete".to_string(),
        crate::live_index::knowledge_bridge::DerivedCoverage::Truncated { breaches } => format!(
            "truncated([{}])",
            breaches
                .iter()
                .map(|breach| format!("{}:{}", snake_debug(breach.kind), breach.omitted))
                .collect::<Vec<_>>()
                .join(",")
        ),
    }
}

fn render_counts(counts: &BTreeMap<String, usize>) -> String {
    counts
        .iter()
        .map(|(label, count)| format!("{label}:{count}"))
        .collect::<Vec<_>>()
        .join(",")
}

fn join_strings(values: &[String]) -> String {
    values.join(",")
}

fn join_u32(values: &[u32]) -> String {
    values
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

fn stable_hash(domain: &str, body: &str) -> String {
    let mut bytes = Vec::with_capacity(domain.len() + body.len() + 1);
    bytes.extend_from_slice(domain.as_bytes());
    bytes.push(0);
    bytes.extend_from_slice(body.as_bytes());
    crate::hash::digest_hex(&bytes)
}

fn snake_debug(value: impl std::fmt::Debug) -> String {
    let debug = format!("{value:?}");
    let mut output = String::with_capacity(debug.len() + 4);
    for (index, character) in debug.chars().enumerate() {
        if character.is_ascii_uppercase() {
            if index > 0 {
                output.push('_');
            }
            output.push(character.to_ascii_lowercase());
        } else if character.is_whitespace() {
            continue;
        } else {
            output.push(character.to_ascii_lowercase());
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use crate::live_index::LiveIndex;
    use crate::protocol::search_tools::{
        KnowledgeSourceScope, ReviewKnowledgeInput, ReviewKnowledgeMode,
    };

    fn input(mode: ReviewKnowledgeMode) -> ReviewKnowledgeInput {
        ReviewKnowledgeInput {
            mode,
            path: None,
            path_prefix: None,
            source_scope: Some(KnowledgeSourceScope::Current),
            project: None,
            projects: None,
            limit: Some(10),
            max_tokens: None,
        }
    }

    #[test]
    fn review_modes_emit_complete_dossiers_and_hashes_independent_of_output_limits() {
        let project = tempfile::tempdir().expect("project");
        std::fs::create_dir_all(project.path().join("docs")).expect("docs");
        std::fs::create_dir_all(project.path().join("src")).expect("src");
        std::fs::write(
            project.path().join("docs").join("architecture.md"),
            "# Architecture\nCurrent implementation links `src/lib.rs`.\n",
        )
        .expect("document");
        std::fs::write(project.path().join("src/lib.rs"), "pub fn anchor() {}\n").expect("code");

        let shared = LiveIndex::load(project.path()).expect("index");
        let generation = shared.published_source_set().current_generation();

        let summary_input = input(ReviewKnowledgeMode::Summary);
        super::validate_input(&summary_input).expect("valid summary input");
        let summary = super::review_current(&generation, &summary_input).expect("summary");
        let mut document_input = input(ReviewKnowledgeMode::Document);
        document_input.path = Some("docs/architecture.md".to_string());
        let document = super::review_current(&generation, &document_input).expect("document");
        let remediation =
            super::review_current(&generation, &input(ReviewKnowledgeMode::Remediation))
                .expect("remediation");

        assert!(summary.rendered.contains("mode=summary"), "{summary:?}");
        assert!(!summary.source_key.is_empty());
        assert!(summary.source_section.contains(&summary.review_hash));
        assert!(
            document
                .rendered
                .contains("code_evidence.consistent_rule_ids=")
        );
        assert!(document.rendered.contains("bridge_records="));
        assert!(document.rendered.contains("timeline.coverage="));
        assert!(document.rendered.contains("proposal.action="));
        assert!(remediation.rendered.contains("mode=remediation"));

        let mut limited = input(ReviewKnowledgeMode::Remediation);
        limited.limit = Some(1);
        limited.max_tokens = Some(256);
        let limited = super::review_current(&generation, &limited).expect("limited");
        assert_eq!(remediation.review_hash, limited.review_hash);
        assert_eq!(remediation.result_hash, limited.result_hash);
    }

    #[test]
    fn review_from_captured_generation_never_mixes_a_later_publication() {
        let project = tempfile::tempdir().expect("project");
        std::fs::create_dir_all(project.path().join("docs")).expect("docs");
        let document = project.path().join("docs/recovery.md");
        std::fs::write(&document, "# Recovery\nOld persistence boundary.\n").expect("old document");

        let shared = LiveIndex::load(project.path()).expect("load old generation");
        let captured = shared.published_source_set().current_generation();
        let request = input(ReviewKnowledgeMode::Remediation);
        let old_before = super::review_current(&captured, &request).expect("old review");

        std::fs::write(&document, "# Recovery\nNew persistence boundary.\n").expect("new document");
        shared
            .reload(project.path())
            .expect("publish new generation");
        let next = shared.published_source_set().current_generation();
        assert_ne!(
            captured.publication_generation, next.publication_generation,
            "fixture must publish a distinct generation"
        );

        let old_after = super::review_current(&captured, &request).expect("captured review");
        let new_review = super::review_current(&next, &request).expect("new review");
        assert_eq!(old_before.rendered, old_after.rendered);
        assert_eq!(old_before.review_hash, old_after.review_hash);
        assert_ne!(old_before.review_hash, new_review.review_hash);
        assert!(old_after.rendered.contains(&format!(
            "publication_generation={}",
            captured.publication_generation
        )));
        assert!(new_review.rendered.contains(&format!(
            "publication_generation={}",
            next.publication_generation
        )));
    }

    #[test]
    fn exact_duplicate_is_only_an_evidence_backed_deletion_candidate_with_retained_anchor() {
        let project = tempfile::tempdir().expect("project");
        std::fs::create_dir_all(project.path().join("docs")).expect("docs");
        for path in ["docs/a-notes.md", "docs/b-notes.md"] {
            std::fs::write(
                project.path().join(path),
                "# Notes\nIdentical bounded unit.\n",
            )
            .expect("duplicate document");
        }
        let shared = LiveIndex::load(project.path()).expect("index");
        let generation = shared.published_source_set().current_generation();
        let output = super::review_current(&generation, &input(ReviewKnowledgeMode::Remediation))
            .expect("remediation");

        assert!(
            output
                .rendered
                .contains("proposal.action=deletion_candidate(retained=docs/a-notes.md)"),
            "exact duplicate proposal: {}",
            output.rendered
        );
        assert!(
            output.rendered.contains("eligibility.duplicate_count=2"),
            "duplicate check: {}",
            output.rendered
        );
        assert!(
            !output.rendered.contains("Identical bounded unit"),
            "review must not inline duplicate prose"
        );
    }
}
