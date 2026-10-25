//! Native `detect_impact` over one captured publication.
//!
//! The change seed, graph walk and risk ordering are the shared computation
//! the MCP handler calls; every git object and working-tree byte passes the
//! shared read gate. Lists carry the same per-list cap and full totals as the
//! MCP payload, further bounded by the caller's query limits.

use super::embed_changes::{gated_text, open_repository};
use super::embed_query::{Budget, EmbeddedQuerySnapshot};
use super::guidance::impact::{DETECT_IMPACT_MAX_RETURNED, ImpactRequest, compute_detect_impact};
use crate::embed::parity::detect_impact::{
    DetectImpactRequest, DetectImpactResult, ImpactBlastEntry, ImpactCallSite, ImpactChangedSymbol,
    ImpactEvidenceStatus, ImpactPage, ImpactPagination, ImpactPathStep, ImpactResolution,
    ImpactRiskSummary, ImpactScope, ImpactSourceFilter,
};
use crate::embed::parity::{QueryOutput, QueryRefusalKind};

pub(super) fn validate(request: &DetectImpactRequest) -> Result<(), QueryRefusalKind> {
    if [&request.base_branch, &request.since]
        .into_iter()
        .any(|reference| reference.as_deref().is_some_and(|r| r.trim().is_empty()))
    {
        return Err(QueryRefusalKind::InvalidRequest);
    }
    if let Some(paths) = &request.paths
        && paths.iter().any(|path| path.trim().is_empty())
    {
        return Err(QueryRefusalKind::InvalidRequest);
    }
    Ok(())
}

fn normalize_explicit_paths(
    root: &std::path::Path,
    raw_paths: &[String],
) -> Result<Vec<String>, QueryRefusalKind> {
    let mut seen = std::collections::BTreeSet::new();
    for raw in raw_paths {
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.contains('\0') {
            return Err(QueryRefusalKind::InvalidRequest);
        }
        let requested = std::path::Path::new(trimmed);
        let absolute = if requested.is_absolute() {
            requested.to_path_buf()
        } else {
            root.join(requested)
        };
        let relative = crate::paths::normalize_event_path(&absolute, root)
            .ok_or(QueryRefusalKind::InvalidRequest)?;
        let normalized = crate::live_index::query::normalize_path_query(&relative);
        super::embed_query::validate_path(&normalized, false)?;
        super::guidance::freshen::safe_repo_path_for_freshen(root, &normalized)
            .map_err(|_| QueryRefusalKind::InvalidRequest)?;
        seen.insert(normalized);
    }
    Ok(seen.into_iter().collect())
}

fn page(total: usize, returned: usize) -> ImpactPage {
    ImpactPage {
        total: total as u64,
        returned: returned as u64,
        truncated: total > returned,
    }
}

pub(super) fn project(
    snapshot: &EmbeddedQuerySnapshot,
    request: &DetectImpactRequest,
    budget: &mut Budget,
) -> Result<QueryOutput, QueryRefusalKind> {
    let live = snapshot.generation.live.as_ref();
    let repo = open_repository(&snapshot.root).ok_or(QueryRefusalKind::GitUnavailable)?;
    let explicit_paths = match &request.paths {
        Some(raw) => Some(normalize_explicit_paths(&snapshot.root, raw)?),
        None => None,
    };
    let report = compute_detect_impact(
        live,
        &repo,
        &ImpactRequest {
            base_branch: request.base_branch.as_deref(),
            since: request.since.as_deref(),
            depth: request.depth,
            files_scope: request.scope == ImpactScope::Files,
            include_untracked: request.include_untracked,
            include_data: request.include_data,
            paths: explicit_paths.as_deref(),
        },
        &mut |git_ref, path| gated_text(live, &repo, git_ref, path),
        &mut |path| gated_text(live, &repo, "", path),
    )
    .map_err(|_| QueryRefusalKind::GitUnavailable)?;
    budget.check()?;
    let risk_counts = report.risk_counts();
    let hint =
        "non-source changed paths are excluded by default; pass include_data=true to include them";
    budget.required(
        hint.len()
            + report.base_disclosure.as_ref().map_or(0, String::len)
            + report.staleness_note.as_ref().map_or(0, String::len),
    )?;

    let mut changed_files = Vec::new();
    for path in report.changed_files.iter().take(DETECT_IMPACT_MAX_RETURNED) {
        if !budget.row(path.len()) {
            break;
        }
        changed_files.push(path.clone());
    }
    let mut changed_symbols = Vec::new();
    for symbol in report
        .changed_symbols
        .iter()
        .take(DETECT_IMPACT_MAX_RETURNED)
    {
        let kind = symbol.kind.to_string();
        if !budget.row(symbol.name.len() + symbol.path.len() + kind.len()) {
            break;
        }
        changed_symbols.push(ImpactChangedSymbol {
            name: symbol.name.clone(),
            path: symbol.path.clone(),
            kind,
        });
    }
    let mut blast_radius = Vec::new();
    let mut evidence_candidates = Vec::new();
    for entry in report.blast_entries.iter().take(DETECT_IMPACT_MAX_RETURNED) {
        let evidence: Option<Vec<ImpactPathStep>> = entry.evidence.as_ref().map(|steps| {
            steps
                .iter()
                .map(|step| ImpactPathStep {
                    caller: ImpactChangedSymbol {
                        name: step.caller.name.clone(),
                        path: step.caller.path.clone(),
                        kind: step.caller.kind.to_string(),
                    },
                    callee: ImpactChangedSymbol {
                        name: step.callee.name.clone(),
                        path: step.callee.path.clone(),
                        kind: step.callee.kind.to_string(),
                    },
                    call_site: ImpactCallSite {
                        path: step.call_site_path.clone(),
                        byte_range: [step.byte_range.0 as usize, step.byte_range.1 as usize],
                        line_range: super::guidance::impact::impact_evidence_line_range(
                            step.line_range,
                        ),
                    },
                    resolution: match step.resolution {
                        crate::live_index::graph::ResolutionReason::UniqueName => {
                            ImpactResolution::UniqueName
                        }
                        crate::live_index::graph::ResolutionReason::QualifiedModule => {
                            ImpactResolution::QualifiedModule
                        }
                        crate::live_index::graph::ResolutionReason::QualifiedOwner => {
                            ImpactResolution::QualifiedOwner
                        }
                    },
                })
                .collect()
        });
        let has_evidence = evidence.is_some();
        let row = ImpactBlastEntry {
            symbol: entry.symbol.clone(),
            hop: entry.hop,
            risk: entry.risk.as_str().to_owned(),
            evidence_status: if has_evidence {
                ImpactEvidenceStatus::Truncated
            } else {
                ImpactEvidenceStatus::Unavailable
            },
            evidence: None,
        };
        let base_size = serde_json::to_vec(&row).map_or(usize::MAX, |json| json.len());
        if !budget.row(base_size) {
            break;
        }
        blast_radius.push(row);
        evidence_candidates.push(evidence);
    }
    let pagination = ImpactPagination {
        changed_files: page(report.changed_files.len(), changed_files.len()),
        changed_symbols: page(report.changed_symbols.len(), changed_symbols.len()),
        blast_radius: page(report.blast_entries.len(), blast_radius.len()),
    };
    // The per-list cap is a truncation like any other; never report it as complete.
    budget.truncated |= pagination.changed_files.truncated
        || pagination.changed_symbols.truncated
        || pagination.blast_radius.truncated;
    let count = |risk: &str| u64::from(risk_counts.get(risk).copied().unwrap_or(0));
    let mut result = DetectImpactResult {
        changed_files,
        changed_symbols,
        blast_radius,
        risk_summary: ImpactRiskSummary {
            critical: count("critical"),
            high: count("high"),
            medium: count("medium"),
            low: count("low"),
        },
        pagination,
        source_filter: ImpactSourceFilter {
            applied: !report.include_data,
            excluded_paths: report.source_filtered_out as u64,
            hint: hint.to_owned(),
        },
        requested_depth: report.requested_depth,
        effective_depth: report.effective_depth,
        base_branch: report.base_disclosure,
        staleness_note: report.staleness_note,
        rendered: String::new(),
    };
    result.rendered = detect_impact_text(&result);
    let base_rendered_bytes = result.rendered.len();
    budget.charge_bytes(base_rendered_bytes)?;

    let mut used_serialized_evidence_bytes = 0usize;
    let mut included_evidence_bytes = 0usize;
    for (row, candidate) in result.blast_radius.iter_mut().zip(&evidence_candidates) {
        let Some(evidence) = candidate.as_ref() else {
            continue;
        };
        let serialized_bytes = serde_json::to_vec(&evidence).map_or(usize::MAX, |json| json.len());
        let decoded_string_bytes = impact_evidence_string_bytes(evidence);
        let total_serialized = used_serialized_evidence_bytes.checked_add(serialized_bytes);
        let within_evidence_cap = total_serialized.is_some_and(|total| {
            total <= super::guidance::impact::DETECT_IMPACT_MAX_EVIDENCE_BYTES
        });
        if within_evidence_cap {
            row.evidence_status = ImpactEvidenceStatus::Available;
            row.evidence = Some(evidence.clone());
            used_serialized_evidence_bytes =
                total_serialized.unwrap_or(used_serialized_evidence_bytes);
            included_evidence_bytes = included_evidence_bytes.saturating_add(decoded_string_bytes);
        } else {
            budget.truncated = true;
        }
    }
    result.rendered = detect_impact_text(&result);
    let rendered_evidence_bytes = result.rendered.len().saturating_sub(base_rendered_bytes);
    let evidence_output_bytes = included_evidence_bytes.saturating_add(rendered_evidence_bytes);
    if evidence_output_bytes > budget.remaining_bytes() {
        // Match MCP's shared-cap selection when the reply fits. If the caller's
        // own output limit is smaller, omit complete chains and disclose that
        // evidence was truncated without dropping blast rows or totals.
        for (row, candidate) in result.blast_radius.iter_mut().zip(&evidence_candidates) {
            if candidate.is_some() {
                row.evidence = None;
                row.evidence_status = ImpactEvidenceStatus::Truncated;
            }
        }
        budget.truncated = true;
        result.rendered = detect_impact_text(&result);
        let rendered_evidence_bytes = result.rendered.len().saturating_sub(base_rendered_bytes);
        let remaining_evidence_bytes = result
            .blast_radius
            .iter()
            .filter_map(|row| row.evidence.as_deref())
            .map(impact_evidence_string_bytes)
            .fold(0usize, usize::saturating_add);
        budget.charge_bytes(remaining_evidence_bytes.saturating_add(rendered_evidence_bytes))?;
    } else {
        budget.charge_bytes(evidence_output_bytes)?;
    }
    Ok(QueryOutput::DetectImpact(result))
}

fn impact_evidence_string_bytes(steps: &[ImpactPathStep]) -> usize {
    usize::try_from(super::embed_usage::decoded_bytes(steps)).unwrap_or(usize::MAX)
}

/// MCP `detect_impact`'s text: the shared summary over the same payload the
/// MCP handler builds.
pub(super) fn detect_impact_text(
    value: &crate::embed::parity::detect_impact::DetectImpactResult,
) -> String {
    let page = |page: &crate::embed::parity::detect_impact::ImpactPage| {
        serde_json::json!({
            "total": page.total,
            "returned": page.returned,
            "truncated": page.truncated,
        })
    };
    let payload = serde_json::json!({
        "changed_files": value.changed_files,
        "changed_symbols": value
            .changed_symbols
            .iter()
            .map(|s| serde_json::json!({"name": s.name, "path": s.path, "kind": s.kind}))
            .collect::<Vec<_>>(),
        "blast_radius": value
            .blast_radius
            .iter()
            .map(|b| {
                let mut entry = serde_json::json!({
                    "symbol": b.symbol,
                    "hop": b.hop,
                    "risk": b.risk,
                    "evidence_status": b.evidence_status,
                });
                if let Some(evidence) = &b.evidence {
                    entry["evidence"] = serde_json::json!(evidence);
                }
                entry
            })
            .collect::<Vec<_>>(),
        "risk_summary": {
            "critical": value.risk_summary.critical,
            "high": value.risk_summary.high,
            "medium": value.risk_summary.medium,
            "low": value.risk_summary.low,
        },
        "pagination": {
            "changed_files": page(&value.pagination.changed_files),
            "changed_symbols": page(&value.pagination.changed_symbols),
            "blast_radius": page(&value.pagination.blast_radius),
        },
        "source_filter": {
            "applied": value.source_filter.applied,
            "excluded_paths": value.source_filter.excluded_paths,
            "hint": value.source_filter.hint,
        },
    });
    crate::index_lifecycle::guidance::changes::detect_impact_result(
        &payload,
        value.requested_depth,
        value.effective_depth,
        value.base_branch.as_deref(),
        value.staleness_note.as_deref(),
    )
}
