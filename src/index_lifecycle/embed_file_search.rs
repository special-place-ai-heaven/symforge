//! Typed projections of shared file resolution, coupling and ranking plans.
use super::embed_query::{Budget, EmbeddedQuerySnapshot, validate_path};
use super::guidance::{file_search, search_contract};
use crate::embed::parity::search::*;
use crate::embed::parity::{QueryOutput, QueryPolicy, QueryRefusalKind};
use crate::live_index::{SearchFilesResolveView, SearchFilesView};

fn input(request: &FileSearchRequest) -> search_contract::SearchFilesInput {
    search_contract::SearchFilesInput {
        project: None,
        projects: None,
        query: request.query.clone(),
        limit: request.limit,
        current_file: request.current_file.clone(),
        changed_with: request.changed_with.clone(),
        resolve: request.resolve,
        estimate: request.estimate,
        max_tokens: request.max_tokens,
        debug_ranking: request.debug_ranking,
        rank_by: request.rank_by.map(|rank| {
            match rank {
                FileRanking::Path => "path",
                FileRanking::Frecency => "frecency",
                FileRanking::PathCochange => "path+cochange",
            }
            .to_owned()
        }),
        anchor_path: request.anchor_path.clone(),
        include_vendor: request.include_vendor,
        include_personal_tooling: request.include_personal_tooling,
        path_prefix: request.path_prefix.clone(),
    }
}
pub(super) fn validate(request: &FileSearchRequest) -> Result<(), QueryRefusalKind> {
    if request.query.trim().is_empty() && request.changed_with.is_none()
        || request.resolve == Some(true) && request.query.trim().is_empty()
    {
        return Err(QueryRefusalKind::InvalidRequest);
    }
    if request
        .limit
        .is_some_and(|value| value == 0 || value > 10_000)
        || request.max_tokens == Some(0)
    {
        return Err(QueryRefusalKind::InvalidRequest);
    }
    crate::knowledge::guard_query(&request.query)
        .map_err(|_| QueryRefusalKind::AdmissionUnavailable)?;
    for path in [
        &request.current_file,
        &request.changed_with,
        &request.anchor_path,
    ]
    .into_iter()
    .flatten()
    {
        validate_path(path, false)?;
    }
    if let Some(path) = &request.path_prefix {
        validate_path(path, true)?;
    }
    Ok(())
}
fn evidence(row: crate::capability::CapabilityEvidence) -> RankingEvidence {
    use crate::capability::{
        CapabilityCost as Cost, CapabilityFreshness as Freshness, CapabilityName as Name,
        CapabilitySafety as Safety, CapabilityStatus as Status,
    };
    RankingEvidence {
        capability: match row.capability {
            Name::FrecencyRanking => RankingCapability::FrecencyRanking,
            Name::CoChangeRanking => RankingCapability::CoChangeRanking,
            Name::RankingDiagnostics => RankingCapability::RankingDiagnostics,
            Name::WorktreeRouting => RankingCapability::WorktreeRouting,
        },
        status: match row.status {
            Status::Applied => RankingStatus::Applied,
            Status::Ready => RankingStatus::Ready,
            Status::Preparing => RankingStatus::Preparing,
            Status::Unavailable => RankingStatus::Unavailable,
            Status::DisabledByPolicy => RankingStatus::DisabledByPolicy,
            Status::FallbackUsed => RankingStatus::FallbackUsed,
            Status::Stale => RankingStatus::Stale,
        },
        freshness: match row.freshness {
            Freshness::Current => RankingFreshness::Current,
            Freshness::Stale => RankingFreshness::Stale,
            Freshness::Empty => RankingFreshness::Empty,
            Freshness::Unknown => RankingFreshness::Unknown,
        },
        cost: match row.cost {
            Cost::Free => RankingCost::Free,
            Cost::Low => RankingCost::Low,
            Cost::Bounded => RankingCost::Bounded,
            Cost::Expensive => RankingCost::Expensive,
        },
        safety: match row.safety {
            Safety::ReadOnly => RankingSafety::ReadOnly,
            Safety::WriteRequiresConsent => RankingSafety::WriteRequiresConsent,
            Safety::OperatorDiagnostics => RankingSafety::OperatorDiagnostics,
        },
        detail: row.detail,
    }
}
fn hit(row: crate::live_index::SearchFilesHit) -> FileSearchHit {
    use crate::live_index::SearchFilesTier as Tier;
    FileSearchHit {
        path: row.path,
        tier: match row.tier {
            Tier::CoChange => FileMatchTier::CoChange,
            Tier::StrongPath => FileMatchTier::StrongPath,
            Tier::Basename => FileMatchTier::Basename,
            Tier::LoosePath => FileMatchTier::LoosePath,
            Tier::MetadataOnly => FileMatchTier::MetadataOnly,
        },
        coupling_score: row.coupling_score,
        shared_commits: row.shared_commits,
        metadata_reason: row.metadata_reason,
    }
}
fn project_hits(
    rows: Vec<crate::live_index::SearchFilesHit>,
    budget: &mut Budget,
) -> Vec<FileSearchHit> {
    let mut hits = Vec::new();
    for row in rows {
        let bytes = row.path.len()
            + row.metadata_reason.as_ref().map_or(0, String::len)
            + match row.tier {
                crate::live_index::SearchFilesTier::CoChange => 9,
                crate::live_index::SearchFilesTier::StrongPath => 11,
                crate::live_index::SearchFilesTier::Basename => 8,
                crate::live_index::SearchFilesTier::LoosePath => 10,
                crate::live_index::SearchFilesTier::MetadataOnly => 13,
            };
        if !budget.row(bytes) {
            break;
        }
        hits.push(hit(row));
    }
    hits
}
pub(super) fn execute(
    snapshot: &EmbeddedQuerySnapshot,
    request: &FileSearchRequest,
    policy: QueryPolicy,
    budget: &mut Budget,
) -> Result<QueryOutput, QueryRefusalKind> {
    if request.estimate == Some(true) {
        budget.required(0)?;
        return Ok(QueryOutput::SearchEstimate(SearchEstimate {
            approximate_tokens: u64::from(request.limit.unwrap_or(20)) * 10 + 30,
        }));
    }
    budget.cap_tokens(request.max_tokens);
    let mut input = input(request);
    input.limit = Some(
        input
            .limit
            .unwrap_or(20)
            .min(budget.remaining_rows() as u32),
    );
    let mut result = FileSearchResult {
        hits: Vec::new(),
        total_matches: 0,
        overflow_count: 0,
        suppressed_by_noise: 0,
        resolution: None,
        cochange: None,
        ranking: Vec::new(),
        ranking_explanation: None,
    };
    if request.resolve == Some(true) {
        let (view, noise) = file_search::resolve_files(&snapshot.generation.live, &input);
        result.suppressed_by_noise = noise as u64;
        result.resolution = Some(match view {
            SearchFilesResolveView::Resolved { path } => {
                budget.required(path.len() + "Resolved".len())?;
                result.total_matches = 1;
                FileResolution::Resolved {
                    path,
                    metadata_reason: None,
                }
            }
            SearchFilesResolveView::ResolvedMetadataOnly { path, reason } => {
                budget.required(path.len() + reason.len() + "Resolved".len())?;
                result.total_matches = 1;
                FileResolution::Resolved {
                    path,
                    metadata_reason: Some(reason),
                }
            }
            SearchFilesResolveView::Ambiguous {
                matches,
                overflow_count,
                ..
            } => {
                budget.charge_bytes("Ambiguous".len())?;
                result.total_matches = (matches.len() + overflow_count) as u64;
                result.overflow_count = overflow_count as u64;
                budget.truncated |= overflow_count > 0;
                let mut candidates = Vec::new();
                for path in matches {
                    if !budget.row(path.len()) {
                        break;
                    }
                    candidates.push(path);
                }
                FileResolution::Ambiguous {
                    candidates,
                    overflow_count: overflow_count as u64,
                }
            }
            SearchFilesResolveView::NotFound { .. } => {
                budget.required("NotFound".len())?;
                FileResolution::NotFound
            }
            SearchFilesResolveView::EmptyHint => return Err(QueryRefusalKind::InvalidRequest),
        });
    } else if let Some(path) = &request.changed_with {
        use crate::live_index::git_temporal::GitTemporalState;
        let temporal = &snapshot.generation.code_signals.temporal;
        let commits = temporal.stats.total_commits_analyzed as u64;
        let state = match &temporal.state {
            GitTemporalState::Unavailable(reason) => CoChangeState::Unavailable {
                reason: reason.clone(),
            },
            GitTemporalState::Ready => {
                if let Some(history) = temporal.files.get(path) {
                    let (strong, weak) = file_search::changed_rows(history);
                    let (rows, state) = if strong.is_empty() {
                        if weak.is_empty() {
                            (Vec::new(), CoChangeState::NoCoupling)
                        } else {
                            (weak, CoChangeState::Weak)
                        }
                    } else {
                        (strong, CoChangeState::Strong)
                    };
                    result.total_matches = rows.len() as u64;
                    result.hits = project_hits(rows, budget);
                    state
                } else {
                    CoChangeState::NoHistory
                }
            }
            _ => CoChangeState::Loading,
        };
        let bytes = match &state {
            CoChangeState::Unavailable { reason } => "Unavailable".len() + reason.len(),
            CoChangeState::NoCoupling => "NoCoupling".len(),
            CoChangeState::NoHistory => "NoHistory".len(),
            CoChangeState::Strong => "Strong".len(),
            CoChangeState::Weak => "Weak".len(),
            CoChangeState::Loading => "Loading".len(),
        };
        budget.charge_bytes(bytes)?;
        result.cochange = Some(CoChangeSummary {
            state,
            analyzed_commits: commits,
            low_history_confidence: commits < 50,
            deprecated_changed_with: true,
        });
    } else {
        let ranked = file_search::rank_files(
            &snapshot.generation,
            Some(&snapshot.root),
            snapshot.project_state.as_ref(),
            &input,
            policy.allow_derived_state_preparation && snapshot.project_state.is_some(),
        );
        budget.check()?;
        if ranked.ranking_diagnostics.explain {
            result.ranking_explanation =
                Some(budget.text(&file_search::search_files_ranking_explanation(
                    &ranked.view,
                    input.rank_by.as_deref(),
                    ranked.cochange_evidence.as_ref(),
                    ranked.frecency_evidence.as_ref(),
                ))?);
        }
        for row in [
            ranked.cochange_evidence,
            ranked.frecency_evidence,
            ranked.ranking_diagnostics.evidence,
        ]
        .into_iter()
        .flatten()
        {
            let row = evidence(row);
            // All fixed enum labels fit within 96 decoded bytes.
            budget.charge_bytes(96 + row.detail.as_ref().map_or(0, String::len))?;
            result.ranking.push(row);
        }
        result.suppressed_by_noise = ranked.hidden_noise_count as u64;
        match ranked.view {
            SearchFilesView::Found {
                hits,
                total_matches,
                overflow_count,
                ..
            } => {
                result.total_matches = total_matches as u64;
                result.overflow_count = overflow_count as u64;
                budget.truncated |= overflow_count > 0;
                result.hits = project_hits(hits, budget);
            }
            SearchFilesView::NotFound { .. } => {}
            SearchFilesView::EmptyQuery => return Err(QueryRefusalKind::InvalidRequest),
        }
    }
    Ok(QueryOutput::FileSearch(result))
}
