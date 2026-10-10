//! Rich symbol reads use the same selector, renderer, and inspection views as MCP.
use super::embed_query::{Budget, EmbeddedQuerySnapshot, symbol_row, validate_kind, validate_path};
use super::embed_session::QuerySession;
use super::guidance::{source, symbol_read};
use crate::embed::parity::symbol::*;
use crate::embed::parity::{QueryOutput, QueryRefusalKind};
use crate::live_index::query::{SymbolSelectorMatch, resolve_symbol_selector};

pub(super) fn validate(request: &SymbolReadRequest) -> Result<(), QueryRefusalKind> {
    if request.max_tokens == Some(0) || request.symbol_line == Some(0) {
        return Err(QueryRefusalKind::InvalidRequest);
    }
    if let Some(targets) = &request.targets {
        if targets.is_empty() || targets.len() > 10_000 {
            return Err(QueryRefusalKind::InvalidRequest);
        }
        for target in targets {
            validate_path(&target.path, false)?;
            validate_kind(target.kind.as_deref())?;
            if target.symbol_line == Some(0)
                || target
                    .name
                    .as_ref()
                    .is_some_and(|name| name.trim().is_empty())
                || (target.name.is_none() && target.start_byte.is_none())
                || target
                    .start_byte
                    .zip(target.end_byte)
                    .is_some_and(|(start, end)| start > end)
            {
                return Err(QueryRefusalKind::InvalidRequest);
            }
        }
    } else {
        if request.name.trim().is_empty() {
            return Err(QueryRefusalKind::InvalidRequest);
        }
        if !request.path.is_empty() {
            validate_path(&request.path, false)?;
        }
        validate_kind(request.kind.as_deref())?;
    }
    Ok(())
}

pub(super) fn validate_inspect(request: &InspectMatchRequest) -> Result<(), QueryRefusalKind> {
    validate_path(&request.path, false)?;
    if request.line == 0 || request.max_tokens == Some(0) {
        return Err(QueryRefusalKind::InvalidRequest);
    }
    Ok(())
}

fn entry(path: String) -> SymbolReadEntry {
    SymbolReadEntry {
        path,
        symbol: None,
        start_byte: None,
        end_byte: None,
        content_hash: None,
        source: None,
        refusal: None,
        withheld: None,
        candidates: Vec::new(),
    }
}

fn target(
    snapshot: &EmbeddedQuerySnapshot,
    target: &SymbolTarget,
    estimate: bool,
    budget: &mut Budget,
) -> Result<(SymbolReadEntry, String), QueryRefusalKind> {
    let mut row = entry(target.path.clone());
    if let Err(refusal) = super::embed_read::admit(snapshot, &target.path, None, budget) {
        row.refusal = Some(refusal);
        row.withheld = budget.withheld.take();
        return Ok((row, format!("Source withheld: {}", target.path)));
    }
    let Some(file) = snapshot.generation.live.get_file(&target.path) else {
        row.refusal = Some(QueryRefusalKind::NotFound);
        return Ok((
            row,
            super::guidance::file_read::not_found_file(&target.path),
        ));
    };
    if let Err(refusal) =
        super::embed_read::admit(snapshot, &target.path, Some(&file.content), budget)
    {
        row.refusal = Some(refusal);
        row.withheld = budget.withheld.take();
        return Ok((row, format!("Source withheld: {}", target.path)));
    }
    let (start, end, rendered) = if let Some(name) = &target.name {
        let rendered = symbol_read::symbol_detail_from_indexed_file(
            file,
            name,
            target.kind.as_deref(),
            target.symbol_line,
        );
        match resolve_symbol_selector(file, name, target.kind.as_deref(), target.symbol_line) {
            SymbolSelectorMatch::Selected(index, symbol) => {
                row.symbol = Some(symbol_row(file, index, symbol)?);
                (
                    symbol.effective_start() as usize,
                    symbol.byte_range.1 as usize,
                    rendered,
                )
            }
            SymbolSelectorMatch::NotFound => {
                row.refusal = Some(QueryRefusalKind::NotFound);
                return Ok((row, rendered));
            }
            SymbolSelectorMatch::Ambiguous(lines) => {
                row.refusal = Some(QueryRefusalKind::AmbiguousSymbol);
                for (index, symbol) in file.symbols.iter().enumerate() {
                    if symbol.name == *name && lines.contains(&symbol.line_range.0) {
                        row.candidates.push(symbol_row(file, index, symbol)?);
                    }
                }
                return Ok((row, rendered));
            }
        }
    } else {
        let start = target.start_byte.ok_or(QueryRefusalKind::InvalidRequest)? as usize;
        let end = target
            .end_byte
            .map_or(file.content.len(), |end| end as usize);
        let bytes = file
            .content
            .get(start..end)
            .ok_or(QueryRefusalKind::InvalidSpan)?;
        (
            start,
            end,
            symbol_read::code_slice_view(&target.path, bytes),
        )
    };
    let bytes = file
        .content
        .get(start..end)
        .ok_or(QueryRefusalKind::InvalidSpan)?;
    row.start_byte = Some(u32::try_from(start).map_err(|_| QueryRefusalKind::InvalidSpan)?);
    row.end_byte = Some(u32::try_from(end).map_err(|_| QueryRefusalKind::InvalidSpan)?);
    row.content_hash = Some(crate::hash::digest_hex(&file.content));
    if !estimate {
        if bytes.len() > budget.remaining_bytes() {
            return Err(QueryRefusalKind::BudgetTooSmall);
        }
        row.source = Some(bytes.to_vec());
    }
    Ok((row, rendered))
}

pub(super) fn read(
    snapshot: &EmbeddedQuerySnapshot,
    request: &SymbolReadRequest,
    session: Option<&QuerySession>,
    budget: &mut Budget,
) -> Result<QueryOutput, QueryRefusalKind> {
    if request.project.as_ref().is_some_and(|project| {
        snapshot
            .root
            .to_str()
            .is_none_or(|root| project != root && *project != root.replace('\\', "/"))
    }) {
        return Err(QueryRefusalKind::AdmissionUnavailable);
    }
    let estimate = request.estimate == Some(true);
    if !estimate
        && let Some((handle, rendered)) = session
            .and_then(|session| session.symbol_read_cache_hit(snapshot, request, budget.limits()))
    {
        budget.reused_handle = Some(handle);
        return Ok(QueryOutput::SymbolRead(SymbolReadResult {
            entries: Vec::new(),
            rendered: budget.text(&rendered)?,
            cache_hit: true,
            estimated_tokens: None,
        }));
    }
    let mut targets = request.targets.clone().unwrap_or_else(|| {
        vec![SymbolTarget {
            path: request.path.clone(),
            name: Some(request.name.clone()),
            kind: request.kind.clone(),
            symbol_line: request.symbol_line,
            start_byte: None,
            end_byte: None,
        }]
    });
    let mut unresolved = None;
    if request.targets.is_none() && request.path.is_empty() {
        match symbol_read::resolve_symbol_path_by_name(
            &snapshot.generation.live,
            &request.name,
            request.kind.as_deref(),
            request.symbol_line,
        ) {
            symbol_read::SymbolNameLookup::Unique { path, symbol_name } => {
                targets[0].path = path;
                targets[0].name = Some(symbol_name);
            }
            symbol_read::SymbolNameLookup::NotFound(rendered) => {
                let mut row = entry(String::new());
                row.refusal = Some(QueryRefusalKind::NotFound);
                unresolved = Some((row, rendered));
            }
            symbol_read::SymbolNameLookup::Ambiguous(rendered, hits) => {
                let mut row = entry(String::new());
                row.refusal = Some(QueryRefusalKind::AmbiguousSymbol);
                for hit in hits {
                    let Some(file) = snapshot.generation.live.get_file(&hit.path) else {
                        continue;
                    };
                    for (index, symbol) in file.symbols.iter().enumerate() {
                        if symbol.name == hit.name
                            && symbol.line_range.0 + 1 == hit.line
                            && symbol.kind.to_string() == hit.kind
                        {
                            row.candidates.push(symbol_row(file, index, symbol)?);
                        }
                    }
                }
                unresolved = Some((row, rendered));
            }
        }
    }
    let mut entries = Vec::new();
    let mut rendered = Vec::new();
    for target_request in &targets {
        budget.check()?;
        if budget.remaining_rows() == 0 {
            budget.truncated = true;
            break;
        }
        let (mut row, text) = match unresolved.take() {
            Some(value) => value,
            None => target(snapshot, target_request, estimate, budget)?,
        };
        let candidates = std::mem::take(&mut row.candidates);
        budget.required(super::embed_usage::decoded_bytes(&row) as usize)?;
        for candidate in candidates {
            if !budget.row(super::embed_usage::decoded_bytes(&candidate) as usize) {
                break;
            }
            row.candidates.push(candidate);
        }
        entries.push(row);
        rendered.push(text);
    }
    let rendered = rendered.join("\n\n");
    let estimated_tokens = estimate.then(|| source::approx_tokens_from_bytes(rendered.len()));
    let mut full = SymbolReadResult {
        entries,
        rendered,
        cache_hit: false,
        estimated_tokens,
    };
    if estimate {
        full.rendered = format!("Estimated tokens: {}", estimated_tokens.unwrap_or(0));
    } else {
        budget.cache_output = Some(QueryOutput::SymbolRead(full.clone()));
        budget.cache_truncated = budget.truncated;
        let token_limit = super::guidance::file_read::resolve_read_max_tokens(
            request.max_tokens,
            full.rendered.len(),
        );
        let (rendered, truncated) =
            source::enforce_token_budget_flagged(full.rendered, Some(token_limit));
        full.rendered = rendered;
        budget.truncated |= truncated;
    }
    full.rendered = budget.text(&full.rendered)?;
    Ok(QueryOutput::SymbolRead(full))
}

pub(super) fn inspect(
    snapshot: &EmbeddedQuerySnapshot,
    request: &InspectMatchRequest,
    budget: &mut Budget,
) -> Result<QueryOutput, QueryRefusalKind> {
    super::embed_read::admit(snapshot, &request.path, None, budget)?;
    let file = snapshot
        .generation
        .live
        .get_file(&request.path)
        .ok_or(QueryRefusalKind::NotFound)?;
    super::embed_read::admit(snapshot, &request.path, Some(&file.content), budget)?;
    let view = snapshot.generation.live.capture_inspect_match_view(
        &request.path,
        request.line,
        request.context,
        request.sibling_limit,
    );
    let rendered = symbol_read::inspect_match_result_view(&view);
    let crate::live_index::InspectMatchView::Found(found) = view else {
        return Err(QueryRefusalKind::InvalidRequest);
    };
    let convert = |value: crate::live_index::EnclosingSymbolView| InspectionSymbol {
        name: value.name,
        kind: value.kind_label,
        start_line: value.line_range.0,
        end_line: value.line_range.1,
    };
    let mut output = InspectMatchResult {
        path: found.path,
        line: found.line,
        excerpt: found.excerpt,
        enclosing: found.enclosing.map(convert),
        parent_chain: found.parent_chain.into_iter().map(convert).collect(),
        siblings: found
            .siblings
            .into_iter()
            .map(|value| InspectionSymbol {
                name: value.name,
                kind: value.kind_label,
                start_line: value.line_range.0,
                end_line: value.line_range.1,
            })
            .collect(),
        siblings_overflow: found.siblings_overflow as u64,
        rendered: String::new(),
        estimated_tokens: (request.estimate == Some(true))
            .then(|| source::approx_tokens_from_bytes(rendered.len())),
    };
    if request.estimate == Some(true) {
        output.excerpt.clear();
    }
    budget.required(super::embed_usage::decoded_bytes(&output) as usize)?;
    let text = if let Some(tokens) = output.estimated_tokens {
        format!("Estimated tokens: {tokens}")
    } else {
        let mut full = output.clone();
        full.rendered = rendered.clone();
        budget.cache_output = Some(QueryOutput::InspectMatch(full));
        let (rendered, truncated) =
            source::enforce_token_budget_flagged(rendered, request.max_tokens);
        budget.truncated |= truncated;
        rendered
    };
    output.rendered = budget.text(&text)?;
    Ok(QueryOutput::InspectMatch(output))
}
