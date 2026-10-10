//! Knowledge-search projection from one admitted immutable source-set capture.

use crate::embed::parity::knowledge::search::{
    SearchKnowledgeCounts, SearchKnowledgeDerived, SearchKnowledgeRequest, SearchKnowledgeResult,
    SearchKnowledgeSource,
};
use crate::embed::parity::{QueryOutput, QueryRefusalKind};
use crate::live_index::knowledge_bridge::DerivedCoverage;
use crate::live_index::knowledge_retrieve::{
    KnowledgeLaneReadiness, KnowledgeRetrieveFilteredCounts, KnowledgeRetrieveSource,
};

use super::embed_query::{Budget, EmbeddedQuerySnapshot};

fn counts(value: KnowledgeRetrieveFilteredCounts) -> SearchKnowledgeCounts {
    SearchKnowledgeCounts {
        current: value.current as u64,
        intent: value.intent as u64,
        history_only: value.history_only as u64,
        suppressed: value.suppressed as u64,
        review_required: value.review_required as u64,
        unknown: value.unknown as u64,
    }
}

fn coverage(value: &DerivedCoverage) -> String {
    match value {
        DerivedCoverage::Complete => "complete",
        DerivedCoverage::Truncated { .. } => "truncated",
        DerivedCoverage::Loading => "loading",
    }
    .to_string()
}

fn source(value: KnowledgeRetrieveSource) -> SearchKnowledgeSource {
    let readiness = value.readiness.map(|readiness| {
        match readiness {
            KnowledgeLaneReadiness::IndexScoutingOrVerifying => "index_scouting_or_verifying",
            KnowledgeLaneReadiness::KnowledgeBridgeLoading => "knowledge_bridge_loading",
            KnowledgeLaneReadiness::NoValidSource => "no_valid_source",
            KnowledgeLaneReadiness::EnvelopeUnavailable => "envelope_unavailable",
            KnowledgeLaneReadiness::EvidenceWithheld => "evidence_withheld",
        }
        .to_string()
    });
    SearchKnowledgeSource {
        label: value.label,
        envelope: value.envelope,
        withheld_count: value.withheld_count as u64,
        filtered: counts(value.filtered),
        readiness,
        degraded: value.degraded,
        derived: SearchKnowledgeDerived {
            authority_rule_version: value.derived.authority_rule_version,
            policy_version: value.derived.policy_version,
            secret_policy_version: value.derived.secret_policy_version,
            bridge_coverage: coverage(&value.derived.bridge_coverage),
            authority_coverage: coverage(&value.derived.authority_coverage),
        },
        publication_generation: value.publication_generation,
        content_generation: value.content_generation,
    }
}

fn selected_project_is_bound(snapshot: &EmbeddedQuerySnapshot, selector: &str) -> bool {
    let Some(root) = snapshot.root.to_str() else {
        return false;
    };
    selector == root || selector == root.replace('\\', "/")
}

fn rendered_hits(rendered: &str) -> usize {
    rendered
        .lines()
        .filter(|line| {
            line.split_once(". ")
                .is_some_and(|(ordinal, _)| ordinal.parse::<usize>().is_ok())
        })
        .count()
}

pub(super) fn project(
    snapshot: &EmbeddedQuerySnapshot,
    input: &SearchKnowledgeRequest,
    budget: &mut Budget,
) -> Result<QueryOutput, QueryRefusalKind> {
    budget.check()?;
    // This handle admits exactly one project. A host federation must route
    // project aliases and multi-project requests to explicitly admitted handles.
    if input
        .project
        .as_deref()
        .is_some_and(|selector| !selected_project_is_bound(snapshot, selector))
        || input.projects.is_some()
    {
        return Err(QueryRefusalKind::UnsupportedOption);
    }
    budget.cap_tokens(input.max_tokens);
    let available_hits = budget.remaining_rows().saturating_sub(1);
    if available_hits == 0 {
        return Err(QueryRefusalKind::BudgetTooSmall);
    }
    let requested_limit = input.limit.unwrap_or(10).clamp(1, 100) as usize;
    let mut bounded = input.clone();
    bounded.limit = Some(requested_limit.min(available_hits) as u32);
    let projection =
        crate::knowledge::search::search_scoped_projection(&snapshot.source_set, &bounded)
            .map_err(|_| QueryRefusalKind::InvalidRequest)?;
    budget.check()?;

    let mut sources = Vec::new();
    let mut hit_count = 0;
    let mut overflow = 0;
    let mut withheld_count = 0;
    let mut filtered = counts(KnowledgeRetrieveFilteredCounts::default());
    let mut absence = None;
    let mut degraded = false;
    let mut engine_truncated = false;
    if let Some(retrieval) = projection.retrieval {
        hit_count = retrieval.hits.len();
        overflow = retrieval.overflow;
        withheld_count = retrieval.withheld_count;
        filtered = counts(retrieval.filtered);
        absence = retrieval.absence.map(|value| value.as_str().to_string());
        degraded = retrieval.degraded;
        engine_truncated = retrieval.truncated;
        sources = retrieval.sources.into_iter().map(source).collect();
    }
    let metadata_bytes = serde_json::to_vec(&sources)
        .map_err(|_| QueryRefusalKind::InvalidRequest)?
        .len()
        .saturating_add(absence.as_ref().map_or(0, String::len));
    let rendered_budget = budget.remaining_bytes().saturating_sub(metadata_bytes);
    if rendered_budget < 256 {
        return Err(QueryRefusalKind::BudgetTooSmall);
    }
    let rendered = crate::knowledge::search::budget_summary(
        &projection.rendered,
        Some((rendered_budget / 4) as u64),
    );
    if rendered.len() > rendered_budget {
        return Err(QueryRefusalKind::BudgetTooSmall);
    }
    let returned = rendered_hits(&rendered);
    budget.required(0)?;
    for _ in 0..returned {
        if !budget.row(0) {
            return Err(QueryRefusalKind::BudgetTooSmall);
        }
    }
    budget.charge_bytes(rendered.len().saturating_add(metadata_bytes))?;
    let truncated = engine_truncated
        || requested_limit > available_hits
        || returned < hit_count
        || rendered.len() < projection.rendered.len();
    budget.truncated |= truncated;
    Ok(QueryOutput::SearchKnowledge(SearchKnowledgeResult {
        rendered,
        source_scope: projection.scope,
        sources,
        hit_count: hit_count as u64,
        rendered_hit_count: returned as u64,
        overflow: overflow as u64,
        withheld_count: withheld_count as u64,
        filtered,
        absence,
        degraded,
        truncated,
    }))
}
