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
    DetectImpactRequest, DetectImpactResult, ImpactBlastEntry, ImpactChangedSymbol, ImpactPage,
    ImpactPagination, ImpactRiskSummary, ImpactScope, ImpactSourceFilter,
};
use crate::embed::parity::{QueryOutput, QueryRefusalKind};

pub(super) fn validate(request: &DetectImpactRequest) -> Result<(), QueryRefusalKind> {
    if [&request.base_branch, &request.since]
        .into_iter()
        .any(|reference| reference.as_deref().is_some_and(|r| r.trim().is_empty()))
    {
        return Err(QueryRefusalKind::InvalidRequest);
    }
    Ok(())
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
    for (symbol, hop, risk) in report.blast_entries.iter().take(DETECT_IMPACT_MAX_RETURNED) {
        let risk = risk.as_str();
        if !budget.row(symbol.len() + risk.len()) {
            break;
        }
        blast_radius.push(ImpactBlastEntry {
            symbol: symbol.clone(),
            hop: *hop,
            risk: risk.to_owned(),
        });
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
    Ok(QueryOutput::DetectImpact(result))
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
            .map(|b| serde_json::json!({"symbol": b.symbol, "hop": b.hop, "risk": b.risk}))
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
