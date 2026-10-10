//! Full content selection through shared admission, rendering, and session engines.
use super::embed_query::{Budget, EmbeddedQuerySnapshot, validate_path};
use super::embed_session::QuerySession;
use super::guidance::{file_read, read_contract, read_gate, source};
use crate::embed::parity::read::*;
use crate::embed::parity::{QueryObservation, QueryOutput, QueryRefusalKind};
use crate::live_index::{IndexedFile, search::ContentContext};

fn authority_refusal(refusal: super::authority::AuthorityRefusal) -> QueryRefusalKind {
    match refusal {
        super::authority::AuthorityRefusal::PublicationIdentityMismatch { .. } => {
            QueryRefusalKind::StalePublication
        }
        super::authority::AuthorityRefusal::PhaseNotCurrent { .. }
        | super::authority::AuthorityRefusal::PhysicalRootReplaced
        | super::authority::AuthorityRefusal::BindingRevoked { .. } => {
            QueryRefusalKind::SourceUnavailable
        }
        _ => QueryRefusalKind::AdmissionUnavailable,
    }
}

fn input(
    request: &FileContentRequest,
) -> Result<read_contract::GetFileContentInput, QueryRefusalKind> {
    let mut value: read_contract::GetFileContentInput =
        serde_json::from_value(serde_json::to_value(request).expect("read request serializes"))
            .map_err(|_| QueryRefusalKind::InvalidRequest)?;
    read_contract::normalize_file_content_aliases(&mut value)
        .map_err(|_| QueryRefusalKind::InvalidRequest)?;
    Ok(value)
}
pub(super) fn validate(request: &FileContentRequest) -> Result<(), QueryRefusalKind> {
    validate_path(&request.path, false)?;
    if request.max_tokens == Some(0) {
        return Err(QueryRefusalKind::InvalidRequest);
    }
    let input = input(request)?;
    read_contract::file_content_options_from_input(&input)
        .map_err(|_| QueryRefusalKind::InvalidRequest)?;
    Ok(())
}
pub(super) fn validate_page(request: &SourcePageRequest) -> Result<(), QueryRefusalKind> {
    validate_path(&request.path, false)?;
    if request.offset > 0 && request.expected_publication.is_none() {
        return Err(QueryRefusalKind::InvalidRequest);
    }
    Ok(())
}
fn refuse_scope(path: &str) -> Result<(), QueryRefusalKind> {
    if read_gate::hard_scope_refusal(path).is_some() {
        return Err(QueryRefusalKind::AdmissionUnavailable);
    }
    Ok(())
}
pub(super) fn admit(
    snapshot: &EmbeddedQuerySnapshot,
    path: &str,
    bytes: Option<&[u8]>,
    budget: &mut Budget,
) -> Result<(), QueryRefusalKind> {
    refuse_scope(path)?;
    let live = &snapshot.generation.live;
    let mut record = |meta| budget.withheld = Some(meta);
    if read_gate::refuse_by_policy_with(live, path, &mut record).is_some() {
        // Recorded content demotion has no positions in the manifest. Recover
        // only redacted findings from an original-anchor bounded re-read, using
        // the same rule-set comparison as the MCP read gate. Failure to enrich
        // keeps the policy refusal and never authorizes rendering source bytes.
        if budget.withheld.is_none()
            && let Some(crate::domain::FileDisposition::MetadataOnly {
                reason: crate::domain::MetadataOnlyReason::SensitiveContent { rule_ids, .. },
            }) = live.capture_file_disposition(path)
        {
            budget.withheld = Some(WithheldMeta::unscanned(path));
            if let Ok(Some(bytes)) = snapshot.authority.read_regular_beneath_expected(
                snapshot.authority_publication,
                std::path::Path::new(path),
                crate::knowledge::SECRET_SCAN_MAX_BYTES,
            ) {
                let (_, findings) =
                    read_gate::recorded_finding_evidence_from_bytes(path, &bytes, rule_ids);
                if !findings.is_empty() {
                    budget.withheld = Some(WithheldMeta::from_content_findings(path, &findings));
                }
            }
        }
        return Err(QueryRefusalKind::AdmissionUnavailable);
    }
    let mut record = |meta| budget.withheld = Some(meta);
    if bytes.is_some_and(|bytes| {
        read_gate::classify_admitted_bytes_with(live, path, bytes, &mut record).is_some()
    }) {
        return Err(QueryRefusalKind::AdmissionUnavailable);
    }
    Ok(())
}
fn selected_range(file: &IndexedFile, context: &ContentContext) -> Result<(), QueryRefusalKind> {
    if let Some(name) = context.around_symbol.as_deref() {
        file_read::resolve_around_symbol_range(file, name, context.symbol_line).map_err(
            |error| match error {
                file_read::AroundSymbolResolutionError::Ambiguous(_) => {
                    QueryRefusalKind::AmbiguousSymbol
                }
                _ => QueryRefusalKind::NotFound,
            },
        )?;
    }
    selected_content_range(&file.content, context)
}

/// Check the exact admitted file before reusing indexed source or session output.
/// A changed observation is withheld until the source worker publishes it.
fn verify_indexed_content(
    snapshot: &EmbeddedQuerySnapshot,
    path: &str,
    indexed: &IndexedFile,
) -> Result<(), QueryRefusalKind> {
    use std::io::Read;
    let matches = snapshot
        .authority
        .with_regular_file_beneath_expected(
            snapshot.authority_publication,
            std::path::Path::new(path),
            |opened| {
                let Ok(before) = opened.metadata() else {
                    return Ok(false);
                };
                if before.len() != indexed.content.len() as u64 {
                    return Ok(false);
                }
                let mut buffer = [0u8; 8192];
                for expected in indexed.content.chunks(buffer.len()) {
                    let target = &mut buffer[..expected.len()];
                    if opened.read_exact(target).is_err() || target != expected {
                        return Ok(false);
                    }
                }
                let Ok(after) = opened.metadata() else {
                    return Ok(false);
                };
                Ok(after.len() == before.len() && after.modified().ok() == before.modified().ok())
            },
        )
        .map_err(|_| QueryRefusalKind::SourceUnavailable)?;
    if matches == Some(true) {
        Ok(())
    } else {
        Err(QueryRefusalKind::StalePublication)
    }
}

fn selected_content_range(
    content: &[u8],
    context: &ContentContext,
) -> Result<(), QueryRefusalKind> {
    let text = String::from_utf8_lossy(content);
    let lines: Vec<&str> = text.lines().collect();
    let count = lines.len() as u32;
    if context.start_line.is_some_and(|line| line > count)
        || context.around_line.is_some_and(|line| line > count)
    {
        return Err(QueryRefusalKind::InvalidRequest);
    }
    if let Some(chunk) = context.chunk_index {
        let max_lines = context.max_lines.ok_or(QueryRefusalKind::InvalidRequest)?;
        if max_lines == 0 || chunk == 0 || chunk > count.div_ceil(max_lines) {
            return Err(QueryRefusalKind::InvalidRequest);
        }
    }
    if let Some(query) = &context.around_match {
        let matches = file_read::find_case_insensitive_match_lines(&lines, query);
        if matches
            .get(context.match_occurrence.unwrap_or(1).saturating_sub(1) as usize)
            .is_none()
        {
            return Err(QueryRefusalKind::NotFound);
        }
    }
    Ok(())
}
pub(super) fn page(
    snapshot: &EmbeddedQuerySnapshot,
    request: &SourcePageRequest,
    budget: &mut Budget,
) -> Result<QueryOutput, QueryRefusalKind> {
    if request
        .expected_publication
        .as_ref()
        .is_some_and(|expected| expected != &snapshot.serving_publication_identity)
    {
        return Err(QueryRefusalKind::StalePublication);
    }
    admit(snapshot, &request.path, None, budget)?;
    let file = snapshot
        .generation
        .live
        .get_file(&request.path)
        .ok_or(QueryRefusalKind::NotFound)?;
    let offset = usize::try_from(request.offset).map_err(|_| QueryRefusalKind::InvalidRequest)?;
    if offset > file.content.len() {
        return Err(QueryRefusalKind::InvalidRequest);
    }
    let hash = crate::hash::digest_hex(&file.content);
    budget.required(request.path.len() + hash.len())?;
    if offset < file.content.len() && budget.remaining_bytes() == 0 {
        return Err(QueryRefusalKind::BudgetTooSmall);
    }
    let bytes = budget.source(&file.content[offset..]);
    let end = offset + bytes.len();
    Ok(QueryOutput::SourcePage(SourcePage {
        path: request.path.clone(),
        content_hash: hash,
        byte_start: request.offset,
        total_bytes: file.content.len() as u64,
        bytes,
        next_offset: (end < file.content.len()).then_some(end as u64),
    }))
}
pub(super) fn content(
    snapshot: &EmbeddedQuerySnapshot,
    request: &FileContentRequest,
    session: Option<&QuerySession>,
    budget: &mut Budget,
    observations: &mut Vec<QueryObservation>,
) -> Result<QueryOutput, QueryRefusalKind> {
    let input = input(request)?;
    let options = read_contract::file_content_options_from_input(&input)
        .map_err(|_| QueryRefusalKind::InvalidRequest)?;
    refuse_scope(&input.path)?;
    let indexed = snapshot.generation.live.get_file(&input.path);
    if indexed.is_none()
        && !snapshot
            .authority
            .verify_exact_spelling_expected(
                snapshot.authority_publication,
                std::path::Path::new(&input.path),
            )
            .map_err(authority_refusal)?
    {
        return Err(QueryRefusalKind::NotFound);
    }
    if input.estimate == Some(true) {
        let (size, lines, authority) = if let Some(file) = indexed {
            (
                file.content.len() as u64,
                Some(file.content.iter().filter(|&&b| b == b'\n').count() as u64),
                ReadAuthority::PublishedGeneration,
            )
        } else {
            let size = snapshot
                .authority
                .regular_file_size_beneath_expected(
                    snapshot.authority_publication,
                    std::path::Path::new(&input.path),
                )
                .map_err(authority_refusal)?
                .ok_or(QueryRefusalKind::NotFound)?;
            (size, None, ReadAuthority::DiskObservation)
        };
        budget.required(input.path.len() + 32)?;
        return Ok(QueryOutput::FileReadEstimate(FileReadEstimate {
            path: input.path,
            approximate_tokens: size / 4,
            approximate_lines: lines,
            authority,
        }));
    }
    admit(snapshot, &input.path, None, budget)?;
    let (rendered, hash, authority, raw_len, nul_warning) = if let Some(file) = indexed {
        verify_indexed_content(snapshot, &input.path, file)?;
        selected_range(file, &options.content_context)?;
        if let Some(session) = session
            && let Some((handle, body)) =
                session.file_content_cache_hit(snapshot, request, budget.limits())
        {
            let content_hash = crate::hash::digest_hex(&file.content);
            budget.charge_bytes(
                input.path.len() + content_hash.len() + "PublishedGeneration".len(),
            )?;
            let rendered = budget.text(&body)?;
            budget.reused_handle = Some(handle);
            return Ok(QueryOutput::FileContent(FileContent {
                path: input.path,
                content_hash,
                approximate_tokens: source::approx_tokens_from_bytes(rendered.len()),
                rendered,
                cache_hit: true,
                authority: ReadAuthority::PublishedGeneration,
            }));
        }
        let output = file_read::file_content_from_indexed_file_with_context(
            file,
            options.content_context.clone(),
        );
        (
            output,
            crate::hash::digest_hex(&file.content),
            ReadAuthority::PublishedGeneration,
            file.content.len(),
            crate::domain::index::nul_byte_warning_line(&crate::domain::index::scan_nul_bytes(
                &file.content,
            )),
        )
    } else {
        let bytes = snapshot
            .authority
            .read_regular_beneath_expected(
                snapshot.authority_publication,
                std::path::Path::new(&input.path),
                crate::knowledge::SECRET_SCAN_MAX_BYTES,
            )
            .map_err(authority_refusal)?
            .ok_or(QueryRefusalKind::NotFound)?;
        admit(snapshot, &input.path, Some(&bytes), budget)?;
        if options.content_context.around_symbol.is_some() {
            return Err(QueryRefusalKind::NotFound);
        }
        selected_content_range(&bytes, &options.content_context)?;
        let body = file_read::render_file_content_bytes(
            &input.path,
            &bytes,
            options.content_context.clone(),
        );
        let hash = crate::hash::digest_hex(&bytes);
        observations.push(QueryObservation {
            kind: "disk_observation".into(),
            identity: input.path.clone(),
            content_hash: hash.clone(),
        });
        (
            body,
            hash,
            ReadAuthority::DiskObservation,
            bytes.len(),
            crate::domain::index::nul_byte_warning_line(&crate::domain::index::scan_nul_bytes(
                &bytes,
            )),
        )
    };
    let annotation = match (
        &options.content_context.mode_name,
        options.content_context.mode_explicit,
    ) {
        (Some(mode), true) => format!("── mode: {mode} (explicit) ──\n"),
        _ => String::new(),
    };
    let combined = format!("{annotation}{rendered}");
    let file_capped = combined.len() > file_read::GET_FILE_CONTENT_MAX_BYTES;
    let capped = file_read::cap_file_content_output(combined);
    let warned = match nul_warning {
        Some(warning) if capped.is_empty() => warning,
        Some(warning) => format!("{capped}\n{warning}"),
        None => capped,
    };
    let (mut rendered, token_capped) = source::enforce_token_budget_flagged(
        warned.clone(),
        Some(file_read::resolve_read_max_tokens(
            input.max_tokens,
            raw_len,
        )),
    );
    if input.force_refresh == Some(true)
        && let Some(prior) = session.and_then(|session| {
            session.prior_file_content_fetch(snapshot, request, budget.limits())
        })
    {
        rendered = file_read::append_dedup_hint_footer(
            rendered,
            "file",
            prior.fetched_at.elapsed().as_secs(),
            prior.approx_tokens,
        );
    }
    let full = FileContent {
        path: input.path,
        content_hash: hash,
        approximate_tokens: source::approx_tokens_from_bytes(warned.len()),
        rendered: warned,
        cache_hit: false,
        authority,
    };
    budget.charge_bytes(
        full.path.len()
            + full.content_hash.len()
            + match authority {
                ReadAuthority::PublishedGeneration => "PublishedGeneration".len(),
                ReadAuthority::DiskObservation => "DiskObservation".len(),
            },
    )?;
    budget.cache_output = Some(QueryOutput::FileContent(full.clone()));
    budget.cache_truncated = file_capped;
    budget.truncated |= file_capped || token_capped;
    let mut served = full;
    served.rendered = budget.text(&rendered)?;
    served.approximate_tokens = source::approx_tokens_from_bytes(served.rendered.len());
    Ok(QueryOutput::FileContent(served))
}
