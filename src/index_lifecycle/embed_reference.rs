//! Bounded reference and dependency projections with shared source evidence.
use std::collections::HashSet;

use super::embed_query::{Budget, EmbeddedQuerySnapshot, validate_kind, validate_path};
use super::guidance::{reference_read as shared, search_envelope, source};
use crate::embed::parity::reference::*;
use crate::embed::parity::{QueryObservation, QueryOutput, QueryRefusalKind};
use crate::live_index::{FindReferencesView, ReferenceFileView, qualified_usages};

pub(super) fn validate(request: &ReferenceSearchRequest) -> Result<(), QueryRefusalKind> {
    if request.name.trim().is_empty()
        || request.symbol_line == Some(0)
        || request.max_tokens == Some(0)
        || request.limit == Some(0)
        || request.max_per_file == Some(0)
        || (request.project.is_some() && request.projects.is_some())
        || request.projects.as_ref().is_some_and(Vec::is_empty)
    {
        return Err(QueryRefusalKind::InvalidRequest);
    }
    if crate::knowledge::guard_query(&request.name).is_err() {
        return Err(QueryRefusalKind::AdmissionUnavailable);
    }
    if let Some(path) = &request.path {
        validate_path(path, false)?;
    }
    validate_kind(request.symbol_kind.as_deref())?;
    if request
        .mode
        .as_deref()
        .is_some_and(|value| !matches!(value, "references" | "implementations"))
        || request
            .direction
            .as_deref()
            .is_some_and(|value| !matches!(value, "trait" | "type" | "auto"))
        || request.kind.as_deref().is_some_and(|value| {
            !matches!(
                value,
                "call" | "import" | "type_usage" | "macro_use" | "value_use" | "all"
            )
        })
    {
        return Err(QueryRefusalKind::UnsupportedOption);
    }
    Ok(())
}

pub(super) fn validate_dependents(
    request: &DependentSearchRequest,
) -> Result<(), QueryRefusalKind> {
    validate_path(&request.path, false)?;
    if request.limit == Some(0) || request.max_per_file == Some(0) || request.max_tokens == Some(0)
    {
        return Err(QueryRefusalKind::InvalidRequest);
    }
    if request
        .format
        .as_deref()
        .is_some_and(|format| !matches!(format, "text" | "mermaid" | "dot"))
    {
        return Err(QueryRefusalKind::UnsupportedOption);
    }
    Ok(())
}

fn bound(
    snapshot: &EmbeddedQuerySnapshot,
    project: Option<&str>,
    projects: Option<&[String]>,
) -> bool {
    let selected = |project: &str| {
        snapshot
            .root
            .to_str()
            .is_some_and(|root| project == root || project == root.replace('\\', "/"))
    };
    project.is_none_or(selected)
        && projects.is_none_or(|projects| projects.len() == 1 && selected(&projects[0]))
}

fn evidence_row(budget: &mut Budget, bytes: usize) -> bool {
    // The rendered response is itself one row. Nested evidence records must
    // consume the remaining rows rather than hiding inside a file container.
    if budget.remaining_rows() <= 1 {
        budget.truncated = true;
        return false;
    }
    budget.row(bytes)
}

fn file_row(
    file: &mut ReferenceFileView,
    max_per_file: usize,
    budget: &mut Budget,
) -> Result<Option<ReferenceFile>, QueryRefusalKind> {
    let mut row = ReferenceFile {
        path: file.file_path.clone(),
        hits: file
            .hits
            .iter()
            .take(max_per_file)
            .map(|hit| {
                hit.context_lines
                    .iter()
                    .map(|line| ReferenceContextLine {
                        line: line.line_number,
                        text: line.text.clone(),
                        is_reference_line: line.is_reference_line,
                        enclosing_annotation: line.enclosing_annotation.clone(),
                    })
                    .collect()
            })
            .collect(),
        caller_declarations: file
            .caller_declarations
            .iter()
            .map(|caller| CallerDeclaration {
                definition_byte_range: caller.definition_byte_range,
                definition_line_range: caller.definition_line_range,
                byte_range: caller.byte_range,
                line: caller.line_number,
                signature_start_line: caller.signature_start_line,
                is_callable: caller.is_callable,
                node_kind: caller.node_kind.clone(),
                scope: caller.scope.clone(),
                name: caller
                    .identity
                    .as_ref()
                    .map(|identity| identity.name.clone()),
                name_byte_range: caller.identity.as_ref().map(|identity| identity.byte_range),
                text: caller.text.clone(),
            })
            .collect(),
        caller_declaration_count: file.caller_declaration_count as u64,
        caller_declarations_omitted: file.caller_declarations_omitted as u64,
        caller_header_unavailable_count: file.caller_header_unavailable_count as u64,
    };
    let caller_count = row.caller_declarations.len();
    let mut keep = true;
    row.caller_declarations.retain(|caller| {
        keep = keep && evidence_row(budget, super::embed_usage::decoded_bytes(caller) as usize);
        keep
    });
    let mut keep = true;
    row.hits.retain(|hit| {
        keep = keep && evidence_row(budget, super::embed_usage::decoded_bytes(hit) as usize);
        keep
    });
    row.caller_declarations_omitted += (caller_count - row.caller_declarations.len()) as u64;
    if row.hits.is_empty() && row.caller_declarations.is_empty() {
        return Ok(None);
    }
    budget.charge_bytes(row.path.len())?;
    file.hits.truncate(row.hits.len());
    file.caller_declarations
        .truncate(row.caller_declarations.len());
    file.caller_declarations_omitted = row.caller_declarations_omitted as usize;
    Ok(Some(row))
}

fn capture(
    snapshot: &EmbeddedQuerySnapshot,
    request: &ReferenceSearchRequest,
    limits: &shared::OutputLimits,
) -> Result<FindReferencesView, QueryRefusalKind> {
    let live = &snapshot.generation.live;
    if let Some(path) = &request.path {
        let file = live.get_file(path).ok_or(QueryRefusalKind::NotFound)?;
        match crate::live_index::query::resolve_symbol_selector(
            file,
            &request.name,
            request.symbol_kind.as_deref(),
            request.symbol_line,
        ) {
            crate::live_index::query::SymbolSelectorMatch::NotFound => {
                return Err(QueryRefusalKind::NotFound);
            }
            crate::live_index::query::SymbolSelectorMatch::Ambiguous(_) => {
                return Err(QueryRefusalKind::AmbiguousSymbol);
            }
            _ => {}
        }
        return live
            .capture_find_references_view_for_symbol(
                path,
                &request.name,
                request.symbol_kind.as_deref(),
                request.symbol_line,
                request.kind.as_deref(),
                limits.total_hits,
            )
            .map_err(|_| QueryRefusalKind::InvalidRequest);
    }
    let kind = shared::find_references_kind_filter(request.kind.as_deref());
    let mut refs = if request.symbol_kind.is_some() || request.symbol_line.is_some() {
        let mut refs = Vec::new();
        for (path, file) in live.all_files() {
            for symbol in &file.symbols {
                if symbol.name != request.name
                    || request
                        .symbol_line
                        .is_some_and(|line| symbol.line_range.0 + 1 != line)
                    || request.symbol_kind.as_deref().is_some_and(|kind| {
                        !crate::live_index::search::kind_filter_matches(kind, &symbol.kind)
                    })
                {
                    continue;
                }
                if let Ok(mut selected) = live.find_exact_references_for_symbol(
                    path,
                    &request.name,
                    request.symbol_kind.as_deref(),
                    Some(symbol.line_range.0 + 1),
                    kind,
                ) {
                    refs.append(&mut selected);
                }
            }
        }
        refs
    } else {
        live.find_references_for_name(&request.name, kind, false)
    };
    refs.sort_by_key(|(path, reference)| (*path, reference.byte_range));
    refs.dedup_by_key(|(path, reference)| (*path, reference.byte_range));
    let mut view = live.build_find_references_view(&refs, limits.total_hits, &request.name);
    if shared::should_collect_qualified_usages(request)
        && request.symbol_kind.is_none()
        && request.symbol_line.is_none()
    {
        let seen = refs
            .iter()
            .map(|(path, reference)| ((*path).to_owned(), reference.byte_range))
            .collect::<HashSet<_>>();
        let usages = qualified_usages::collect_qualified_usages(
            &request.name,
            live.all_files().filter_map(|(path, file)| {
                (file.language == crate::domain::LanguageId::Rust).then_some(
                    qualified_usages::QualifiedFileContent {
                        file_path: path,
                        content: &file.content,
                    },
                )
            }),
        );
        shared::merge_qualified_usages_into_view(&mut view, usages, seen);
    }
    Ok(view)
}

fn finish(
    mut result: ReferenceSearchResult,
    request: &ReferenceSearchRequest,
    budget: &mut Budget,
) -> Result<QueryOutput, QueryRefusalKind> {
    if request.estimate == Some(true) {
        let tokens = source::approx_tokens_from_bytes(result.rendered.len());
        result.estimated_tokens = Some(tokens);
        result.files.clear();
        result.target_candidates.clear();
        result.implementations.clear();
        result.rendered = format!("Estimated tokens: {tokens}");
    } else {
        budget.cache_output = Some(QueryOutput::ReferenceSearch(result.clone()));
        budget.cache_truncated = budget.truncated;
        let (text, truncated) =
            source::enforce_token_budget_flagged(result.rendered, request.max_tokens);
        result.rendered = if truncated || budget.truncated {
            source::downgrade_full_completeness_after_truncation(&text)
        } else {
            text
        };
        budget.truncated |= truncated;
    }
    result.rendered = budget.text(&result.rendered)?;
    Ok(QueryOutput::ReferenceSearch(result))
}

pub(super) fn references(
    snapshot: &EmbeddedQuerySnapshot,
    request: &ReferenceSearchRequest,
    budget: &mut Budget,
    observations: &mut Vec<QueryObservation>,
) -> Result<QueryOutput, QueryRefusalKind> {
    if !bound(
        snapshot,
        request.project.as_deref(),
        request.projects.as_deref(),
    ) {
        return Err(QueryRefusalKind::AdmissionUnavailable);
    }
    if let Some(path) = &request.path {
        super::embed_read::admit(snapshot, path, None, budget)?;
    }
    let live = &snapshot.generation.live;
    let mut result = ReferenceSearchResult {
        total_references: 0,
        total_files: 0,
        files: Vec::new(),
        target_candidate_count: 0,
        target_candidates: Vec::new(),
        target_parse_coverage_complete: true,
        implementations: Vec::new(),
        rendered: String::new(),
        estimated_tokens: None,
    };
    if request.mode.as_deref() == Some("implementations") {
        let mut view =
            live.capture_implementations_view(&request.name, request.direction.as_deref());
        let cap = request.limit.unwrap_or(200).min(500);
        let limits = shared::OutputLimits::new(cap, cap);
        result.total_references = view.entries.len() as u64;
        result.total_files = view
            .entries
            .iter()
            .map(|entry| entry.file_path.as_str())
            .collect::<HashSet<_>>()
            .len() as u64;
        if request.estimate != Some(true) {
            for entry in view.entries.iter().take(limits.total_hits) {
                let row = Implementation {
                    trait_name: entry.trait_name.clone(),
                    implementor: entry.implementor.clone(),
                    path: entry.file_path.clone(),
                    line: entry.line,
                };
                if !evidence_row(budget, super::embed_usage::decoded_bytes(&row) as usize) {
                    break;
                }
                result.implementations.push(row);
            }
            budget.truncated |= result.implementations.len() < view.entries.len();
            if result.implementations.is_empty() && !view.entries.is_empty() {
                return Err(QueryRefusalKind::BudgetTooSmall);
            }
            view.entries.truncate(result.implementations.len());
        }
        let body = shared::implementations_result_view(&view, &request.name, &limits);
        result.rendered = if view.entries.is_empty() {
            body
        } else {
            format!(
                "{}\n\n{body}",
                search_envelope::format_search_envelope(
                    shared::find_references_match_type_label(request, "implementations"),
                    search_envelope::SourceAuthority::from_freshness(
                        &snapshot.generation.freshness
                    ),
                    shared::implementations_parse_state_for_paths(live, &view),
                    &shared::implementations_completeness_label(&view, &limits),
                    &shared::find_references_scope_summary(request, "implementations"),
                    &shared::implementations_evidence(&view),
                )
            )
        };
        result.rendered = shared::with_withheld_note(live, None, result.rendered);
        return finish(result, request, budget);
    }
    let limits = shared::OutputLimits::new(
        request.limit.unwrap_or(20),
        request.max_per_file.unwrap_or(10),
    );
    let mut view = capture(snapshot, request, &limits)?;
    result.total_references = view.total_refs as u64;
    result.total_files = view.total_files as u64;
    result.target_candidate_count = view.target_candidate_count as u64;
    result.target_parse_coverage_complete = view.target_indexed_file_parse_coverage_complete;
    if request.estimate != Some(true) {
        for candidate in &view.target_candidates {
            let row = ReferenceTargetCandidate {
                path: candidate.path.clone(),
                name: candidate.name.clone(),
                kind: candidate.kind.clone(),
                line_range: candidate.line_range,
                byte_range: candidate.byte_range,
                header: candidate.header.clone(),
            };
            if !evidence_row(budget, super::embed_usage::decoded_bytes(&row) as usize) {
                break;
            }
            result.target_candidates.push(row);
        }
        let mut bounded_files = Vec::new();
        for mut file in std::mem::take(&mut view.files)
            .into_iter()
            .take(limits.max_files)
        {
            if let Some(row) = file_row(&mut file, limits.max_per_file, budget)? {
                result.files.push(row);
                bounded_files.push(file);
            }
        }
        view.files = bounded_files;
        let shown: usize = result.files.iter().map(|file| file.hits.len()).sum();
        budget.truncated |= shown < view.total_refs
            || result.files.len() < view.total_files
            || result.target_candidates.len() < view.target_candidates.len();
        view.target_candidates
            .truncate(result.target_candidates.len());
    }
    let candidates = live.oversized_metadata_only_files();
    let disclosure = shared::tier2_reference_disclosure_with(
        &candidates,
        &request.name,
        true,
        |path, max_bytes| {
            if budget.check().is_err() {
                return None;
            }
            let bytes = snapshot
                .authority
                .read_regular_beneath_expected(
                    snapshot.authority_publication,
                    std::path::Path::new(path),
                    max_bytes,
                )
                .ok()??;
            if super::embed_read::admit(snapshot, path, Some(&bytes), budget).is_err() {
                return None;
            }
            observations.push(QueryObservation {
                kind: "disk_observation".into(),
                identity: path.into(),
                content_hash: crate::hash::digest_hex(&bytes),
            });
            Some(bytes)
        },
    );
    budget.check()?;
    let mut body = if request.compact.unwrap_or(false) {
        shared::find_references_compact_view(&view, &request.name, &limits)
    } else {
        shared::find_references_result_view(&view, &request.name, &limits)
    };
    if request.path.is_none() && !view.files.is_empty() {
        let paths = shared::symbol_candidate_paths(live, &request.name);
        if paths.len() > 1 {
            let preview = paths.iter().take(5).map(String::as_str).collect::<Vec<_>>();
            let suffix = if paths.len() > preview.len() {
                format!(", … (+{} more)", paths.len() - preview.len())
            } else {
                String::new()
            };
            body = format!(
                "Note: {} definitions named \"{}\" exist across the repo ({}{}). These references are matched by name and MAY MIX usages of different same-named definitions. Pass `path` (and `symbol_line` if needed) to scope references to one definition.\n\n{}",
                paths.len(),
                request.name,
                preview.join(", "),
                suffix,
                body
            );
        }
    }
    if view.total_refs == 0
        && let Ok(text) = crate::live_index::search::search_text_with_options(
            live,
            Some(&request.name),
            None,
            false,
            &crate::live_index::search::TextSearchOptions::for_current_code_search(),
        )
        && !text.files.is_empty()
    {
        body.push_str(&format!("\n\nNote: no indexed references found, but search_text found {} file(s) containing \"{}\". The index may miss qualified-path calls (e.g., module::{}()). Use search_text(query=\"{}\") for full coverage.",text.files.len(),request.name,request.name,request.name));
    }
    if let Some(disclosure) = &disclosure {
        body.push_str("\n\n");
        body.push_str(disclosure);
    }
    result.rendered = if view.files.is_empty() {
        body
    } else {
        let mut completeness =
            shared::find_references_completeness_label(&view, &limits, request.kind.as_deref());
        if disclosure.is_some() {
            completeness.push_str("; Tier-2 exclusions apply (see below)");
        }
        format!(
            "{}\n\n{body}",
            search_envelope::format_search_envelope(
                shared::find_references_match_type_label(request, "references"),
                search_envelope::SourceAuthority::from_freshness(&snapshot.generation.freshness),
                shared::search_parse_state_for_paths(
                    live,
                    view.files.iter().map(|file| file.file_path.as_str())
                ),
                &completeness,
                &shared::find_references_scope_summary(request, "references"),
                &shared::find_references_evidence(&view),
            )
        )
    };
    result.rendered = shared::with_withheld_note(live, None, result.rendered);
    finish(result, request, budget)
}

pub(super) fn dependents(
    snapshot: &EmbeddedQuerySnapshot,
    request: &DependentSearchRequest,
    budget: &mut Budget,
    observations: &mut Vec<QueryObservation>,
) -> Result<QueryOutput, QueryRefusalKind> {
    if !bound(snapshot, request.project.as_deref(), None) {
        return Err(QueryRefusalKind::AdmissionUnavailable);
    }
    if let Some(name) = request
        .name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
    {
        let redirected = ReferenceSearchRequest {
            name: name.into(),
            path: Some(request.path.clone()),
            limit: request.limit,
            max_per_file: request.max_per_file,
            compact: request.compact,
            estimate: request.estimate,
            max_tokens: request.max_tokens,
            ..Default::default()
        };
        validate(&redirected)?;
        let QueryOutput::ReferenceSearch(result) =
            references(snapshot, &redirected, budget, observations)?
        else {
            unreachable!()
        };
        let guidance = format!(
            "find_dependents is file-level only (it lists files that import \"{}\"). You supplied name=\"{}\", which is a symbol-level query. Use find_references(name=\"{}\") for who calls/uses that symbol, or call find_dependents with only path=\"{}\" for the file dependency graph.",
            request.path, name, name, request.path
        );
        budget.charge_bytes(
            guidance.len() + super::embed_usage::decoded_bytes(&redirected) as usize,
        )?;
        budget.cache_output = None;
        return Ok(QueryOutput::DependentSearch(DependentSearchResult {
            files: Vec::new(),
            rendered: guidance,
            estimated_tokens: result.estimated_tokens,
            references: Some(result),
            redirected_request: Some(redirected),
        }));
    }
    super::embed_read::admit(snapshot, &request.path, None, budget)?;
    let live = &snapshot.generation.live;
    let mut view = live.capture_find_dependents_view(&request.path);
    let total_files = view.files.len();
    let limits = shared::OutputLimits::new(
        request.limit.unwrap_or(20),
        request.max_per_file.unwrap_or(5),
    );
    let mut files = Vec::new();
    if request.estimate != Some(true) {
        let mut bounded_files = Vec::new();
        for mut file in std::mem::take(&mut view.files)
            .into_iter()
            .take(limits.max_files)
        {
            let mut row = DependentFile {
                path: file.file_path.clone(),
                lines: file
                    .lines
                    .iter()
                    .take(limits.max_per_file)
                    .map(|line| DependentLine {
                        line: line.line_number,
                        text: line.line_content.clone(),
                        kind: line.kind.clone(),
                        name: line.name.clone(),
                    })
                    .collect(),
            };
            let mut keep = true;
            row.lines.retain(|line| {
                keep =
                    keep && evidence_row(budget, super::embed_usage::decoded_bytes(line) as usize);
                keep
            });
            budget.truncated |= row.lines.len() < file.lines.len();
            if row.lines.is_empty() {
                continue;
            }
            budget.charge_bytes(row.path.len())?;
            file.lines.truncate(row.lines.len());
            bounded_files.push(file);
            files.push(row);
        }
        budget.truncated |= files.len() < total_files;
        view.files = bounded_files;
    }
    let rendered = match request.format.as_deref().unwrap_or("text") {
        "mermaid" => shared::find_dependents_mermaid(&view, &request.path, &limits),
        "dot" => shared::find_dependents_dot(&view, &request.path, &limits),
        _ if request.compact.unwrap_or(false) => shared::with_withheld_note(
            live,
            None,
            shared::find_dependents_compact_view(&view, &request.path, &limits),
        ),
        _ => shared::with_withheld_note(
            live,
            None,
            shared::find_dependents_result_view(&view, &request.path, &limits),
        ),
    };
    let mut result = DependentSearchResult {
        files,
        references: None,
        rendered,
        estimated_tokens: None,
        redirected_request: None,
    };
    if request.estimate == Some(true) {
        let tokens = source::approx_tokens_from_bytes(result.rendered.len());
        result.estimated_tokens = Some(tokens);
        result.files.clear();
        result.rendered = format!("Estimated tokens: {tokens}");
    } else {
        budget.cache_output = Some(QueryOutput::DependentSearch(result.clone()));
        budget.cache_truncated = budget.truncated;
        let (text, truncated) =
            source::enforce_token_budget_flagged(result.rendered, request.max_tokens);
        result.rendered = text;
        budget.truncated |= truncated;
    }
    result.rendered = budget.text(&result.rendered)?;
    Ok(QueryOutput::DependentSearch(result))
}
