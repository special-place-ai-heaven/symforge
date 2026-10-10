//! Shared file-search planning and ranking evidence.
use super::filters::normalize_path_prefix;
use super::search_contract::SearchFilesInput;
use crate::capability::{
    CapabilityCost, CapabilityEvidence, CapabilityFreshness, CapabilityName, CapabilitySafety,
    CapabilityStatus, CouplingPreparePolicy, RankingDiagnosticsPolicy,
};
use crate::live_index::{
    LiveIndex, SearchFilesCouplingEvidence, SearchFilesCouplingNeighbors, SearchFilesHit,
    SearchFilesResolveView, SearchFilesTier, SearchFilesView,
};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

pub(crate) fn normalize_exact_path(input: &str) -> String {
    let normalized = input
        .trim()
        .replace('\\', "/")
        .trim_start_matches("./")
        .trim_start_matches('/')
        .trim_end_matches('/')
        .to_string();

    if normalized.is_empty() {
        input.trim().to_string()
    } else {
        normalized
    }
}

const MAX_CO_CHANGE_PARTNERS_PER_ANCHOR: u32 = 20;

struct SearchFilesCoChangeResolution {
    neighbors: Option<SearchFilesCouplingNeighbors>,
    evidence: CapabilityEvidence,
}

fn cochange_ranking_evidence(
    status: CapabilityStatus,
    freshness: CapabilityFreshness,
    cost: CapabilityCost,
    detail: impl Into<String>,
) -> CapabilityEvidence {
    CapabilityEvidence::new(CapabilityName::CoChangeRanking, status)
        .with_freshness(freshness)
        .with_cost(cost)
        .with_safety(CapabilitySafety::ReadOnly)
        .with_detail(detail)
}

/// Build the precise `FallbackUsed` co-change detail for the case where the
/// coupling store loaded usable partner rows for the anchor, yet no returned
/// candidate landed in the co-change tier. The four distinguishable reasons:
///
/// 1. chore-anchor excluded (`is_chore_anchor_path` fires before the gate),
/// 2. anchor reached only prefix-tier path confidence (below the basename
///    floor), with a `query="<basename>"` hint to clear it,
/// 3. neighbors exist but none of them appear among the returned candidates
///    (the genuine path-mismatch case),
/// 4. neighbors appear among candidates but were filtered out by a later gate.
///
/// The anchor-confidence reason (1 and 2) is computed once at anchor level via
/// [`crate::live_index::rank_signals::classify_anchor_cochange_rejection`], not per candidate.
fn cochange_fallback_detail(
    query: &str,
    anchor_path: &str,
    neighbors: &SearchFilesCouplingNeighbors,
    candidate_paths: &[&str],
) -> String {
    let partner_count = neighbors.len();
    let anchor_score = crate::live_index::query::anchor_path_match_score(query, anchor_path);
    match crate::live_index::rank_signals::classify_anchor_cochange_rejection(
        anchor_path,
        anchor_score,
    ) {
        Some(crate::live_index::rank_signals::AnchorCoChangeRejection::ChoreAnchor) => {
            format!(
                "anchor_path={anchor_path} loaded {partner_count} usable coupling partner(s), but it is a chore anchor (lockfile/changelog/workflow) excluded from co-change promotion; path ranking returned"
            )
        }
        Some(crate::live_index::rank_signals::AnchorCoChangeRejection::BelowConfidenceFloor) => {
            let anchor_basename = std::path::Path::new(anchor_path)
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or(anchor_path);
            format!(
                "anchor_path={anchor_path} loaded {partner_count} usable coupling partner(s), but the query reached only prefix-tier path confidence for the anchor (below the basename-tier floor); pass query=\"{anchor_basename}\" to clear it; path ranking returned"
            )
        }
        None => {
            let any_partner_in_candidates = candidate_paths
                .iter()
                .any(|path| neighbors.contains_key(*path));
            if any_partner_in_candidates {
                format!(
                    "anchor_path={anchor_path} loaded {partner_count} usable coupling partner(s) present among returned candidates, but a later rank gate filtered them; path ranking returned"
                )
            } else {
                format!(
                    "anchor_path={anchor_path} loaded {partner_count} usable coupling partner(s), but none appear among the returned candidates; path ranking returned"
                )
            }
        }
    }
}

fn cochange_lazy_prepare_evidence(
    root: &Path,
    project_state: Option<&crate::domain::ProjectStateDir>,
    reason: &str,
    fallback_detail: &str,
    allow_prepare: bool,
) -> CapabilityEvidence {
    if !allow_prepare {
        return cochange_ranking_evidence(
            CapabilityStatus::DisabledByPolicy,
            CapabilityFreshness::Unknown,
            CapabilityCost::Free,
            format!("{reason}; host policy disallows derived-state preparation; {fallback_detail}"),
        );
    }
    let Some(project_state) = project_state else {
        return cochange_ranking_evidence(
            CapabilityStatus::Unavailable,
            CapabilityFreshness::Unknown,
            CapabilityCost::Free,
            format!("{reason}; no project-state owner is available; {fallback_detail}"),
        );
    };
    match crate::live_index::coupling::start_lazy_prepare(root, project_state) {
        Ok(crate::live_index::coupling::LazyPrepareOutcome::Started) => cochange_ranking_evidence(
            CapabilityStatus::Preparing,
            CapabilityFreshness::Unknown,
            CapabilityCost::Bounded,
            format!("{reason}; bounded background preparation started; {fallback_detail}"),
        ),
        Ok(crate::live_index::coupling::LazyPrepareOutcome::AlreadyRunning) => {
            cochange_ranking_evidence(
                CapabilityStatus::Preparing,
                CapabilityFreshness::Unknown,
                CapabilityCost::Bounded,
                format!(
                    "{reason}; bounded background preparation already in progress; {fallback_detail}"
                ),
            )
        }
        Err(error) => cochange_ranking_evidence(
            CapabilityStatus::Unavailable,
            CapabilityFreshness::Unknown,
            CapabilityCost::Bounded,
            format!("{reason}; unable to start bounded preparation: {error}; {fallback_detail}"),
        ),
    }
}

fn cochange_stale_prepare_evidence(
    root: &Path,
    project_state: Option<&crate::domain::ProjectStateDir>,
    allow_prepare: bool,
) -> CapabilityEvidence {
    if !allow_prepare {
        return cochange_ranking_evidence(
            CapabilityStatus::DisabledByPolicy,
            CapabilityFreshness::Stale,
            CapabilityCost::Free,
            "coupling store is stale; host policy disallows derived-state preparation; path ranking returned",
        );
    }
    let Some(project_state) = project_state else {
        return cochange_ranking_evidence(
            CapabilityStatus::Unavailable,
            CapabilityFreshness::Stale,
            CapabilityCost::Free,
            "coupling store is stale but no project-state owner is available; path ranking returned",
        );
    };
    match crate::live_index::coupling::start_lazy_prepare(root, project_state) {
        Ok(crate::live_index::coupling::LazyPrepareOutcome::Started) => cochange_ranking_evidence(
            CapabilityStatus::Stale,
            CapabilityFreshness::Stale,
            CapabilityCost::Bounded,
            "coupling store is stale for the current HEAD; bounded background refresh started; path ranking returned",
        ),
        Ok(crate::live_index::coupling::LazyPrepareOutcome::AlreadyRunning) => {
            cochange_ranking_evidence(
                CapabilityStatus::Stale,
                CapabilityFreshness::Stale,
                CapabilityCost::Bounded,
                "coupling store is stale for the current HEAD; bounded background refresh already in progress; path ranking returned",
            )
        }
        Err(error) => cochange_ranking_evidence(
            CapabilityStatus::Unavailable,
            CapabilityFreshness::Stale,
            CapabilityCost::Bounded,
            format!(
                "coupling store is stale for the current HEAD; unable to start bounded refresh: {error}; path ranking returned"
            ),
        ),
    }
}

fn search_files_coupling_neighbors(
    index: &LiveIndex,
    repo_root: Option<&Path>,
    project_state: Option<&crate::domain::ProjectStateDir>,
    anchor_path: &str,
    allow_prepare: bool,
) -> SearchFilesCoChangeResolution {
    let anchor_path = normalize_exact_path(anchor_path);
    if !index.files.contains_key(&anchor_path) {
        return SearchFilesCoChangeResolution {
            neighbors: None,
            evidence: cochange_ranking_evidence(
                CapabilityStatus::FallbackUsed,
                CapabilityFreshness::Unknown,
                CapabilityCost::Free,
                format!("anchor_path={anchor_path} is not indexed; path ranking returned"),
            ),
        };
    }

    match crate::live_index::coupling::coupling_prepare_policy_from_env() {
        CouplingPreparePolicy::Disabled => {
            return SearchFilesCoChangeResolution {
                neighbors: None,
                evidence: cochange_ranking_evidence(
                    CapabilityStatus::DisabledByPolicy,
                    CapabilityFreshness::Unknown,
                    CapabilityCost::Free,
                    "operator policy disabled co-change preparation and ranking; path ranking returned",
                ),
            };
        }
        CouplingPreparePolicy::LazyOnRequest | CouplingPreparePolicy::WarmOnStart => {}
    }

    let store = if let Some(store) = index.coupling_store() {
        Some(store.clone())
    } else if let (Some(root), Some(project_state)) = (repo_root, project_state) {
        match crate::live_index::coupling::open_existing_coupling_store(project_state) {
            Ok(Some(store)) => Some((*store).clone()),
            Ok(None) => {
                return SearchFilesCoChangeResolution {
                    neighbors: None,
                    evidence: cochange_lazy_prepare_evidence(
                        root,
                        Some(project_state),
                        "no coupling store exists for this workspace",
                        "path ranking returned",
                        allow_prepare,
                    ),
                };
            }
            Err(error) => {
                return SearchFilesCoChangeResolution {
                    neighbors: None,
                    evidence: cochange_ranking_evidence(
                        CapabilityStatus::Unavailable,
                        CapabilityFreshness::Unknown,
                        CapabilityCost::Low,
                        format!("unable to open coupling store: {error}; path ranking returned"),
                    ),
                };
            }
        }
    } else {
        return SearchFilesCoChangeResolution {
            neighbors: None,
            evidence: cochange_ranking_evidence(
                CapabilityStatus::Unavailable,
                CapabilityFreshness::Unknown,
                CapabilityCost::Free,
                "no repository root is bound; path ranking returned",
            ),
        };
    };
    let Some(store) = store else {
        return SearchFilesCoChangeResolution {
            neighbors: None,
            evidence: cochange_ranking_evidence(
                CapabilityStatus::Unavailable,
                CapabilityFreshness::Unknown,
                CapabilityCost::Free,
                "no coupling store is available; path ranking returned",
            ),
        };
    };

    if let Some(root) = repo_root {
        match store.cold_built_at() {
            Ok(Some(_)) => {}
            Ok(None) => {
                return SearchFilesCoChangeResolution {
                    neighbors: None,
                    evidence: cochange_lazy_prepare_evidence(
                        root,
                        project_state,
                        "coupling store exists but has not completed a cold build",
                        "path ranking returned",
                        allow_prepare,
                    ),
                };
            }
            Err(error) => {
                return SearchFilesCoChangeResolution {
                    neighbors: None,
                    evidence: cochange_ranking_evidence(
                        CapabilityStatus::Unavailable,
                        CapabilityFreshness::Unknown,
                        CapabilityCost::Low,
                        format!(
                            "unable to inspect coupling store build state: {error}; path ranking returned"
                        ),
                    ),
                };
            }
        }

        let stored_head = match store.last_head() {
            Ok(head) => head,
            Err(error) => {
                return SearchFilesCoChangeResolution {
                    neighbors: None,
                    evidence: cochange_ranking_evidence(
                        CapabilityStatus::Unavailable,
                        CapabilityFreshness::Unknown,
                        CapabilityCost::Low,
                        format!(
                            "unable to inspect coupling store HEAD state: {error}; path ranking returned"
                        ),
                    ),
                };
            }
        };
        let current_head = crate::git::head_sha(root).ok();
        if stored_head != current_head {
            return SearchFilesCoChangeResolution {
                neighbors: None,
                evidence: cochange_stale_prepare_evidence(root, project_state, allow_prepare),
            };
        }
    }

    let rows = store
        .query_with_floor(
            &crate::live_index::coupling::AnchorKey::file(&anchor_path),
            MAX_CO_CHANGE_PARTNERS_PER_ANCHOR,
            crate::live_index::rank_signals::FILE_LEVEL_CO_CHANGE_FLOOR,
        )
        .map_err(|error| error.to_string());
    let rows = match rows {
        Ok(rows) => rows,
        Err(error) => {
            return SearchFilesCoChangeResolution {
                neighbors: None,
                evidence: cochange_ranking_evidence(
                    CapabilityStatus::Unavailable,
                    CapabilityFreshness::Unknown,
                    CapabilityCost::Low,
                    format!("unable to query coupling store: {error}; path ranking returned"),
                ),
            };
        }
    };
    let mut neighbors = HashMap::new();
    for row in rows {
        let Some(partner_path) = row.partner.as_str().strip_prefix("file:") else {
            continue;
        };
        let weighted_score = row.weighted_score as f32;
        if !weighted_score.is_finite() || weighted_score <= 0.0 {
            continue;
        }
        neighbors.insert(
            partner_path.to_string(),
            SearchFilesCouplingEvidence {
                shared_commits: row.shared_commits,
                weighted_score,
            },
        );
    }
    if neighbors.is_empty() {
        SearchFilesCoChangeResolution {
            neighbors: None,
            evidence: cochange_ranking_evidence(
                CapabilityStatus::FallbackUsed,
                CapabilityFreshness::Empty,
                CapabilityCost::Low,
                format!(
                    "ready coupling store has no usable partner rows for anchor_path={anchor_path}; path ranking returned"
                ),
            ),
        }
    } else {
        SearchFilesCoChangeResolution {
            neighbors: Some(neighbors),
            evidence: cochange_ranking_evidence(
                CapabilityStatus::Ready,
                CapabilityFreshness::Current,
                CapabilityCost::Low,
                format!(
                    "ready coupling store loaded usable partner rows for anchor_path={anchor_path}"
                ),
            ),
        }
    }
}

fn frecency_ranking_evidence(
    status: CapabilityStatus,
    detail: impl Into<String>,
) -> CapabilityEvidence {
    CapabilityEvidence::new(CapabilityName::FrecencyRanking, status)
        .with_freshness(CapabilityFreshness::Current)
        .with_safety(CapabilitySafety::ReadOnly)
        .with_detail(detail)
}

const RANKING_DIAGNOSTICS_ENV: &str = "SYMFORGE_DEBUG_RANKING";

pub(crate) fn ranking_diagnostics_policy_from_env() -> RankingDiagnosticsPolicy {
    match std::env::var(RANKING_DIAGNOSTICS_ENV) {
        Ok(value) => match value.trim().to_ascii_lowercase().as_str() {
            "" => RankingDiagnosticsPolicy::CallTimeExplain,
            "1" | "true" | "on" | "yes" | "default-on" => RankingDiagnosticsPolicy::DefaultOn,
            "0" | "false" | "no" | "off" | "disabled" | "disable" => {
                RankingDiagnosticsPolicy::Disabled
            }
            _ => RankingDiagnosticsPolicy::Disabled,
        },
        Err(std::env::VarError::NotPresent) => RankingDiagnosticsPolicy::CallTimeExplain,
        Err(std::env::VarError::NotUnicode(_)) => RankingDiagnosticsPolicy::Disabled,
    }
}

pub(crate) struct SearchFilesRankingDiagnosticsDecision {
    pub explain: bool,
    pub evidence: Option<CapabilityEvidence>,
}

fn ranking_diagnostics_evidence(
    status: CapabilityStatus,
    detail: impl Into<String>,
) -> CapabilityEvidence {
    CapabilityEvidence::new(CapabilityName::RankingDiagnostics, status)
        .with_freshness(CapabilityFreshness::Current)
        .with_safety(CapabilitySafety::ReadOnly)
        .with_detail(detail)
}

fn search_files_debug_ranking_requested(
    input: &SearchFilesInput,
) -> SearchFilesRankingDiagnosticsDecision {
    match ranking_diagnostics_policy_from_env() {
        RankingDiagnosticsPolicy::CallTimeExplain => SearchFilesRankingDiagnosticsDecision {
            explain: input.debug_ranking.unwrap_or(false),
            evidence: None,
        },
        RankingDiagnosticsPolicy::DefaultOn => SearchFilesRankingDiagnosticsDecision {
            explain: true,
            evidence: None,
        },
        RankingDiagnosticsPolicy::Disabled if input.debug_ranking.unwrap_or(false) => {
            SearchFilesRankingDiagnosticsDecision {
                explain: false,
                evidence: Some(ranking_diagnostics_evidence(
                    CapabilityStatus::DisabledByPolicy,
                    "operator policy disabled ranking diagnostics; ranking explanation omitted",
                )),
            }
        }
        RankingDiagnosticsPolicy::Disabled => SearchFilesRankingDiagnosticsDecision {
            explain: false,
            evidence: None,
        },
    }
}

fn search_files_rank_mode_label(rank_by: Option<&str>) -> &'static str {
    match rank_by {
        Some("frecency") => "frecency",
        Some("path+cochange") => "path+cochange",
        _ => "default path",
    }
}

fn search_files_tier_summary(view: &SearchFilesView) -> String {
    let SearchFilesView::Found { hits, .. } = view else {
        return "no returned files".to_string();
    };
    let mut cochange = 0usize;
    let mut strong = 0usize;
    let mut basename = 0usize;
    let mut loose = 0usize;
    let mut metadata = 0usize;
    for hit in hits {
        match hit.tier {
            SearchFilesTier::CoChange => cochange += 1,
            SearchFilesTier::StrongPath => strong += 1,
            SearchFilesTier::Basename => basename += 1,
            SearchFilesTier::LoosePath => loose += 1,
            SearchFilesTier::MetadataOnly => metadata += 1,
        }
    }
    let mut parts = Vec::new();
    if cochange > 0 {
        parts.push(format!("co-change={cochange}"));
    }
    if strong > 0 {
        parts.push(format!("strong path={strong}"));
    }
    if basename > 0 {
        parts.push(format!("basename={basename}"));
    }
    if loose > 0 {
        parts.push(format!("loose path={loose}"));
    }
    if metadata > 0 {
        parts.push(format!("metadata-only={metadata}"));
    }
    if parts.is_empty() {
        "no returned files".to_string()
    } else {
        parts.join(", ")
    }
}

fn search_files_signal_explanation(
    signal: &str,
    evidence: Option<&CapabilityEvidence>,
    not_requested: bool,
) -> String {
    if not_requested {
        return format!("{signal} signal: not requested");
    }
    match evidence {
        Some(evidence) => {
            let mut line = format!("{signal} signal: {}", evidence.status);
            if let Some(detail) = evidence.detail.as_deref().map(str::trim)
                && !detail.is_empty()
            {
                line.push_str(" - ");
                line.push_str(detail.trim_end_matches('.'));
            }
            line
        }
        None => format!("{signal} signal: unavailable - no capability evidence was recorded"),
    }
}

fn search_files_final_ordering_note(
    rank_by: Option<&str>,
    cochange_evidence: Option<&CapabilityEvidence>,
    frecency_evidence: Option<&CapabilityEvidence>,
) -> String {
    match rank_by {
        Some("frecency") => match frecency_evidence.map(|evidence| evidence.status) {
            Some(CapabilityStatus::Applied) => {
                "frecency fusion applied after path ranking; ties remain deterministic by path"
                    .to_string()
            }
            Some(status) => format!(
                "frecency requested but {status}; path-ranked ordering was returned"
            ),
            None => "frecency requested but no evidence was available; path-ranked ordering was returned"
                .to_string(),
        },
        Some("path+cochange") => match cochange_evidence.map(|evidence| evidence.status) {
            Some(CapabilityStatus::Applied) | Some(CapabilityStatus::Ready) => {
                "co-change partners that passed gates were fused with path ranking; remaining files keep path tie-breakers"
                    .to_string()
            }
            Some(status) => format!(
                "path+cochange requested but {status}; path-ranked ordering was returned"
            ),
            None => "path+cochange requested but no evidence was available; path-ranked ordering was returned"
                .to_string(),
        },
        _ => "tiered path relevance ordered the results with deterministic path tie-breakers"
            .to_string(),
    }
}

pub(crate) fn search_files_ranking_explanation(
    view: &SearchFilesView,
    rank_by: Option<&str>,
    cochange_evidence: Option<&CapabilityEvidence>,
    frecency_evidence: Option<&CapabilityEvidence>,
) -> String {
    let rank_mode = search_files_rank_mode_label(rank_by);
    let frecency_not_requested = rank_by != Some("frecency");
    let cochange_not_requested = rank_by != Some("path+cochange");
    [
        "Ranking explanation:".to_string(),
        format!("requested rank mode: {rank_mode}"),
        format!(
            "path signal: applied - returned tier family: {}",
            search_files_tier_summary(view)
        ),
        search_files_signal_explanation("frecency", frecency_evidence, frecency_not_requested),
        search_files_signal_explanation("co-change", cochange_evidence, cochange_not_requested),
        format!(
            "final ordering: {}",
            search_files_final_ordering_note(rank_by, cochange_evidence, frecency_evidence)
        ),
    ]
    .join("\n")
}

fn search_files_total_matches(view: &SearchFilesView) -> usize {
    match view {
        SearchFilesView::Found { total_matches, .. } => *total_matches,
        _ => 0,
    }
}

fn search_files_resolve_candidate_count(view: &SearchFilesResolveView) -> usize {
    match view {
        SearchFilesResolveView::Resolved { .. }
        | SearchFilesResolveView::ResolvedMetadataOnly { .. } => 1,
        SearchFilesResolveView::Ambiguous {
            matches,
            overflow_count,
            ..
        } => matches.len().saturating_add(*overflow_count),
        _ => 0,
    }
}

pub(crate) fn resolve_files(
    index: &LiveIndex,
    input: &SearchFilesInput,
) -> (SearchFilesResolveView, usize) {
    let include_vendor = input.include_vendor.unwrap_or(false);
    let include_personal_tooling = input.include_personal_tooling.unwrap_or(false);
    let path_scope = normalize_path_prefix(input.path_prefix.as_deref());
    let view = {
        let guard = index;
        guard.capture_search_files_resolve_view_with_scope(
            &input.query,
            include_vendor,
            include_personal_tooling,
            &path_scope,
        )
    };
    let hidden_noise_count = if include_vendor && include_personal_tooling {
        0
    } else {
        let guard = index;
        let unfiltered = guard.capture_search_files_resolve_view_with_scope(
            &input.query,
            true,
            true,
            &path_scope,
        );
        search_files_resolve_candidate_count(&unfiltered)
            .saturating_sub(search_files_resolve_candidate_count(&view))
    };

    (view, hidden_noise_count)
}

pub(crate) struct RankedFiles {
    pub view: SearchFilesView,
    pub hidden_noise_count: usize,
    pub cochange_evidence: Option<CapabilityEvidence>,
    pub frecency_evidence: Option<CapabilityEvidence>,
    pub ranking_diagnostics: SearchFilesRankingDiagnosticsDecision,
}
pub(crate) fn rank_files(
    generation: &crate::live_index::store::PublishedGeneration,
    repo_root: Option<&Path>,
    project_state: Option<&crate::domain::ProjectStateDir>,
    input: &SearchFilesInput,
    allow_prepare: bool,
) -> RankedFiles {
    rank_files_with_frecency(
        generation,
        repo_root,
        project_state,
        input,
        allow_prepare,
        |paths, now_ts| match repo_root {
            Some(root) => crate::live_index::frecency::ranking_scores_for_paths(
                root,
                project_state,
                paths,
                now_ts,
            ),
            None => Ok(None),
        },
    )
}

pub(crate) fn rank_files_with_frecency(
    generation: &crate::live_index::store::PublishedGeneration,
    repo_root: Option<&Path>,
    project_state: Option<&crate::domain::ProjectStateDir>,
    input: &SearchFilesInput,
    allow_prepare: bool,
    ranking_scores: impl FnOnce(
        &[&Path],
        i64,
    ) -> Result<
        Option<crate::live_index::frecency::FrecencyRankingSnapshot>,
        String,
    >,
) -> RankedFiles {
    let include_vendor = input.include_vendor.unwrap_or(false);
    let include_personal_tooling = input.include_personal_tooling.unwrap_or(false);
    let rank_by_path_cochange = input.rank_by.as_deref() == Some("path+cochange");
    let rank_by_frecency = input.rank_by.as_deref() == Some("frecency");
    let ranking_diagnostics = search_files_debug_ranking_requested(input);
    let mut cochange_evidence: Option<CapabilityEvidence> = None;
    let mut frecency_evidence: Option<CapabilityEvidence> = None;
    let mut hidden_noise_count = 0usize;
    let cochange_repo_root = if rank_by_path_cochange {
        repo_root.map(Path::to_path_buf)
    } else {
        None
    };
    let cochange_project_state = rank_by_path_cochange
        .then(|| project_state.cloned())
        .flatten();
    let mut view = {
        let guard = Arc::clone(&generation.live);
        let cochange_resolution = if rank_by_path_cochange {
            match input.anchor_path.as_deref() {
                Some(anchor_path) => Some(search_files_coupling_neighbors(
                    &guard,
                    cochange_repo_root.as_deref(),
                    cochange_project_state.as_ref(),
                    anchor_path,
                    allow_prepare,
                )),
                None => Some(SearchFilesCoChangeResolution {
                    neighbors: None,
                    evidence: cochange_ranking_evidence(
                        CapabilityStatus::FallbackUsed,
                        CapabilityFreshness::Unknown,
                        CapabilityCost::Free,
                        "`rank_by=\"path+cochange\"` requires `anchor_path=<path>`; path ranking returned",
                    ),
                }),
            }
        } else {
            None
        };
        if let Some(resolution) = &cochange_resolution {
            cochange_evidence = Some(resolution.evidence.clone());
        }
        let coupling_neighbors = cochange_resolution
            .as_ref()
            .and_then(|resolution| resolution.neighbors.as_ref());
        let coupling_context = input.anchor_path.as_deref().zip(coupling_neighbors);
        // Optional path-prefix scoping, mirroring the `path_scope` axis of
        // search_symbols/search_text: the scope is applied inside the capture
        // (pre-count predicate), so total_matches/overflow_count/hits are all
        // consistently scoped — including the overflow set, not just the
        // visible hits.
        let path_scope = normalize_path_prefix(input.path_prefix.as_deref());
        let view = guard.capture_search_files_view_with_noise(
            &input.query,
            input.limit.unwrap_or(20) as usize,
            input.current_file.as_deref(),
            coupling_context,
            include_vendor,
            include_personal_tooling,
            &path_scope,
        );
        if !(include_vendor && include_personal_tooling) {
            // Same path_scope so hidden_noise_count compares like-for-like
            // (noise hidden WITHIN the requested prefix, not repo-wide).
            let unfiltered = guard.capture_search_files_view_with_noise(
                &input.query,
                input.limit.unwrap_or(20) as usize,
                input.current_file.as_deref(),
                coupling_context,
                true,
                true,
                &path_scope,
            );
            hidden_noise_count = search_files_total_matches(&unfiltered)
                .saturating_sub(search_files_total_matches(&view));
        }
        if rank_by_path_cochange && let Some((anchor_path, neighbors)) = coupling_context {
            let applied_hits = match &view {
                SearchFilesView::Found { hits, .. } => hits
                    .iter()
                    .filter(|hit| hit.tier == SearchFilesTier::CoChange)
                    .count(),
                _ => 0,
            };
            cochange_evidence = if applied_hits > 0 {
                Some(cochange_ranking_evidence(
                    CapabilityStatus::Applied,
                    CapabilityFreshness::Current,
                    CapabilityCost::Low,
                    format!(
                        "anchor_path={anchor_path} loaded {} usable coupling partner(s); rows with shared-commit counts show applied evidence",
                        neighbors.len()
                    ),
                ))
            } else {
                let candidate_paths: Vec<&str> = match &view {
                    SearchFilesView::Found { hits, .. } => {
                        hits.iter().map(|hit| hit.path.as_str()).collect()
                    }
                    _ => Vec::new(),
                };
                Some(cochange_ranking_evidence(
                    CapabilityStatus::FallbackUsed,
                    CapabilityFreshness::Current,
                    CapabilityCost::Low,
                    cochange_fallback_detail(
                        &input.query,
                        anchor_path,
                        neighbors,
                        &candidate_paths,
                    ),
                ))
            };
        }
        view
    };
    // Optional frecency-fusion rerank. Activated when the caller requests
    // `rank_by="frecency"`, independent of the old persistent-collection
    // feature flag. Discovery still never opens a writeable frecency store:
    // this branch only reads existing persistent rows and current-process
    // session rows, then reports explicit evidence for every fallback.
    if rank_by_frecency {
        match crate::live_index::frecency::collection_policy_from_env() {
            crate::capability::FrecencyCollectionPolicy::Disabled => {
                frecency_evidence = Some(frecency_ranking_evidence(
                    CapabilityStatus::DisabledByPolicy,
                    "operator policy disabled frecency collection and ranking; path ranking returned",
                ));
            }
            _ => match &mut view {
                SearchFilesView::Found { hits, .. } if hits.is_empty() => {
                    frecency_evidence = Some(frecency_ranking_evidence(
                        CapabilityStatus::FallbackUsed,
                        "no returned candidates to score; path ranking returned",
                    ));
                }
                SearchFilesView::Found { hits, .. } => {
                    if repo_root.is_some() {
                        let candidate_count = hits.len();
                        let hit_paths: Vec<std::path::PathBuf> = hits
                            .iter()
                            .map(|hit| std::path::PathBuf::from(&hit.path))
                            .collect();
                        let path_refs: Vec<&std::path::Path> =
                            hit_paths.iter().map(|path| path.as_path()).collect();
                        let now_ts = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|duration| duration.as_secs() as i64)
                            .unwrap_or(0);
                        match ranking_scores(&path_refs, now_ts) {
                            Ok(Some(snapshot)) if !snapshot.scores.is_empty() => {
                                let scored_count = snapshot.scores.len();
                                let breakdowns =
                                    crate::live_index::search::score_hits_by_frecency_fusion(
                                        hits,
                                        &snapshot.scores,
                                    );
                                let taken = std::mem::take(hits);
                                *hits = crate::live_index::search::reorder_hits_by_frecency_fusion(
                                    taken,
                                    &breakdowns,
                                );
                                frecency_evidence = Some(frecency_ranking_evidence(
                                    CapabilityStatus::Applied,
                                    format!(
                                        "{scored_count}/{candidate_count} returned candidates had frecency scores from {}; requested frecency ranking applied",
                                        snapshot.source
                                    ),
                                ));
                            }
                            Ok(Some(snapshot)) => {
                                frecency_evidence = Some(frecency_ranking_evidence(
                                    CapabilityStatus::FallbackUsed,
                                    format!(
                                        "{} is empty or has no scores for returned candidates; path ranking returned",
                                        snapshot.source
                                    ),
                                ));
                            }
                            Ok(None) => {
                                frecency_evidence = Some(frecency_ranking_evidence(
                                    CapabilityStatus::FallbackUsed,
                                    "no frecency history found; path ranking returned",
                                ));
                            }
                            Err(reason) => {
                                frecency_evidence = Some(frecency_ranking_evidence(
                                    CapabilityStatus::Unavailable,
                                    format!(
                                        "unable to read frecency history: {reason}; path ranking returned"
                                    ),
                                ));
                            }
                        }
                    } else {
                        frecency_evidence = Some(frecency_ranking_evidence(
                            CapabilityStatus::Unavailable,
                            "no repository root is bound; path ranking returned",
                        ));
                    }
                }
                _ => {
                    frecency_evidence = Some(frecency_ranking_evidence(
                        CapabilityStatus::FallbackUsed,
                        "no returned candidates to score; path ranking returned",
                    ));
                }
            },
        }
    }

    RankedFiles {
        view,
        hidden_noise_count,
        cochange_evidence,
        frecency_evidence,
        ranking_diagnostics,
    }
}
pub(crate) fn changed_rows(
    history: &crate::live_index::git_temporal::GitFileHistory,
) -> (Vec<SearchFilesHit>, Vec<SearchFilesHit>) {
    let hits: Vec<SearchFilesHit> = history
        .co_changes
        .iter()
        .map(|entry| SearchFilesHit {
            tier: SearchFilesTier::CoChange,
            path: entry.path.clone(),
            coupling_score: Some(entry.coupling_score),
            shared_commits: Some(entry.shared_commits),
            metadata_reason: None,
        })
        .collect();
    let weak_hits: Vec<SearchFilesHit> = history
        .weak_co_changes
        .iter()
        .map(|entry| SearchFilesHit {
            tier: SearchFilesTier::CoChange,
            path: entry.path.clone(),
            coupling_score: Some(entry.coupling_score),
            shared_commits: Some(entry.shared_commits),
            metadata_reason: None,
        })
        .collect();

    (hits, weak_hits)
}

/// The `Capability:` evidence line MCP appends to a ranked file search.
pub fn capability_evidence_line(evidence: &CapabilityEvidence) -> String {
    let mut line = format!("Capability: {} {}", evidence.capability, evidence.status);
    if let Some(detail) = evidence.detail.as_deref().map(str::trim)
        && !detail.is_empty()
    {
        let detail = detail.trim_end_matches('.');
        line.push_str(" - ");
        line.push_str(detail);
    }
    line.push('.');
    line
}

/// MCP `search_files`' rendered answer for a ranked path search: the result
/// envelope or the filter summary, the view, the ranking evidence and notes,
/// and the zero-hit untracked diagnostic `untracked_paths` sweeps on demand.
pub(crate) fn render_ranked_files(
    generation: &crate::live_index::store::PublishedGeneration,
    ranked: RankedFiles,
    input: &SearchFilesInput,
    include_vendor: bool,
    include_personal_tooling: bool,
    untracked_paths: impl FnOnce() -> Vec<String>,
) -> String {
    let rank_by_path_cochange = input.rank_by.as_deref() == Some("path+cochange");
    let rank_by_frecency = input.rank_by.as_deref() == Some("frecency");
    let view = ranked.view;
    let hidden_noise_count = ranked.hidden_noise_count;
    let cochange_evidence = ranked.cochange_evidence;
    let frecency_evidence = ranked.frecency_evidence;
    let ranking_diagnostics = ranked.ranking_diagnostics;
    let debug_ranking = ranking_diagnostics.explain;
    let envelope = match &view {
        SearchFilesView::Found {
            hits,
            overflow_count,
            ..
        } => {
            let scope = match input.current_file.as_deref() {
                Some(current_file) => {
                    format!("ranked indexed file paths; current file boost `{current_file}`")
                }
                None => "ranked indexed file paths".to_string(),
            };
            Some(super::search_envelope::format_search_envelope(
                super::search_envelope::search_files_match_type_label(&view),
                // The two composite labels already differ from the bare
                // "current index" sentinel, so they never collapse the
                // envelope; only the plain arm needed measuring. They do
                // still say "current" unconditionally — worth revisiting
                // once every lane derives its own prefix.
                if rank_by_path_cochange {
                    super::search_envelope::SourceAuthority::never_collapse(
                        "current index + optional coupling store",
                    )
                } else if rank_by_frecency {
                    super::search_envelope::SourceAuthority::never_collapse(
                        "current index + optional frecency history",
                    )
                } else {
                    super::search_envelope::SourceAuthority::from_freshness(&generation.freshness)
                },
                super::reference_read::search_parse_state_for_paths(
                    &generation.live,
                    hits.iter().map(|hit| hit.path.as_str()),
                ),
                &super::search_envelope::search_completeness_label(
                    *overflow_count,
                    hidden_noise_count,
                ),
                &super::search_envelope::search_files_scope_summary(
                    scope,
                    include_vendor,
                    include_personal_tooling,
                ),
                &super::changes::search_paths_evidence(hits.iter().map(|hit| hit.path.as_str())),
            ))
        }
        _ => None,
    };
    let output = super::search_render::search_files_result_view(&view);
    let had_envelope = envelope.is_some();
    let mut result = match envelope {
        Some(envelope) => format!("{envelope}\n\n{output}"),
        None => output,
    };
    if !had_envelope {
        super::search_envelope::append_search_files_filter_summary(
            &mut result,
            include_vendor,
            include_personal_tooling,
        );
    }
    if let Some(evidence) = cochange_evidence.as_ref() {
        result.push_str("\n\n");
        result.push_str(&capability_evidence_line(evidence));
    }
    if let Some(evidence) = frecency_evidence.as_ref() {
        result.push_str("\n\n");
        result.push_str(&capability_evidence_line(evidence));
    }
    if let Some(evidence) = ranking_diagnostics.evidence.as_ref() {
        result.push_str("\n\n");
        result.push_str(&capability_evidence_line(evidence));
    }
    if debug_ranking {
        result.push_str("\n\n");
        result.push_str(&search_files_ranking_explanation(
            &view,
            input.rank_by.as_deref(),
            cochange_evidence.as_ref(),
            frecency_evidence.as_ref(),
        ));
    }
    if let Some(note) = super::search_envelope::search_files_hidden_noise_note(
        hidden_noise_count,
        include_vendor,
        include_personal_tooling,
    ) {
        result.push_str("\n\n");
        result.push_str(&note);
    }
    if matches!(view, SearchFilesView::NotFound { .. }) {
        let matching_untracked_paths = untracked_paths();
        super::search::append_untracked_file_diagnostic(&mut result, &matching_untracked_paths);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_resolution_applies_scope_before_ambiguity() {
        let root = tempfile::tempdir().unwrap();
        for path in ["src/a", "src/b", "vendor"] {
            std::fs::create_dir_all(root.path().join(path)).unwrap();
            std::fs::write(root.path().join(path).join("lib.rs"), "pub fn item() {}\n").unwrap();
        }
        let shared = LiveIndex::load(root.path()).unwrap();
        let input: SearchFilesInput = serde_json::from_value(serde_json::json!({
            "query":"lib.rs", "resolve":true, "path_prefix":"src/a"
        }))
        .unwrap();
        let (resolved, _) = resolve_files(&shared.read(), &input);
        assert_eq!(
            resolved,
            SearchFilesResolveView::Resolved {
                path: "src/a/lib.rs".into()
            }
        );
    }
}
