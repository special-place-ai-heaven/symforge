//! Deterministic federation over an explicit host-granted source list.
use crate::embed::parity::federation::*;
use crate::embed::parity::host::{OperationControl, OperationStop};
use crate::embed::parity::{API_VERSION, QueryLimits, QueryRefusalKind, QueryRequest, QueryUsage};
use std::collections::{BTreeMap, BTreeSet};

fn stop(control: Option<&OperationControl>) -> Option<FederationStop> {
    control
        .and_then(|control| control.check().err())
        .map(|stop| match stop {
            OperationStop::Cancelled => FederationStop::Cancelled,
            OperationStop::DeadlineExceeded => FederationStop::DeadlineExceeded,
        })
}
fn stop_refusal(stop: FederationStop) -> FederationRefusalKind {
    match stop {
        FederationStop::Cancelled => FederationRefusalKind::Cancelled,
        FederationStop::DeadlineExceeded => FederationRefusalKind::DeadlineExceeded,
    }
}
fn selected<'a>(
    selection: &'a SourceSelection,
    aliases: &BTreeMap<&'a str, usize>,
) -> Result<BTreeSet<&'a str>, FederationRefusalKind> {
    let requested = match selection {
        SourceSelection::All => aliases.keys().copied().collect(),
        SourceSelection::One(label) => BTreeSet::from([label.as_str()]),
        SourceSelection::Many(labels) => labels.iter().map(String::as_str).collect(),
    };
    if requested.is_empty() {
        return Err(FederationRefusalKind::UnknownSource);
    }
    if requested.iter().any(|label| !aliases.contains_key(label)) {
        return Err(FederationRefusalKind::UnknownSource);
    }
    Ok(requested)
}

/// Every source is supplied by the host. Project names never open or discover
/// roots. A stopped result retains the receipts of sources already completed.
pub fn query_sources(
    sources: &[AdmittedQuerySource<'_>],
    selection: &SourceSelection,
    request: &QueryRequest,
    limits: QueryLimits,
    control: Option<&OperationControl>,
) -> Result<FederatedQueryClaim, FederationRefusal> {
    query_sources_using(
        sources,
        selection,
        request,
        limits,
        control,
        |source, request, limits| {
            source
                .handle
                .query_with_policy(request, limits, source.policy, source.session, control)
        },
    )
}

fn query_sources_using(
    sources: &[AdmittedQuerySource<'_>],
    selection: &SourceSelection,
    request: &QueryRequest,
    limits: QueryLimits,
    control: Option<&OperationControl>,
    mut execute: impl FnMut(
        &AdmittedQuerySource<'_>,
        &QueryRequest,
        QueryLimits,
    ) -> Result<
        crate::embed::parity::QueryClaim,
        crate::embed::parity::QueryRefusal,
    >,
) -> Result<FederatedQueryClaim, FederationRefusal> {
    let canonical_argument_hash = crate::hash::digest_hex(
        &serde_json::to_vec(&(
            "symforge.embed.federation",
            API_VERSION,
            selection,
            request,
            limits,
        ))
        .expect("federation arguments serialize"),
    );
    let refuse = |kind| FederationRefusal {
        kind,
        canonical_argument_hash: canonical_argument_hash.clone(),
    };
    if limits.max_results == 0
        || limits.max_results > 10_000
        || limits.max_bytes == 0
        || limits.max_bytes > 1_048_576
        || sources.len() > 10_000
    {
        return Err(refuse(FederationRefusalKind::InvalidRequest));
    }
    super::embed_query::validate_request(request)
        .map_err(|_| refuse(FederationRefusalKind::InvalidRequest))?;
    let mut aliases = BTreeMap::new();
    for (index, source) in sources.iter().enumerate() {
        if source.label.is_empty()
            || source.label.len() > 1024
            || source.label.trim() != source.label
            || source.label.contains('\0')
            || aliases.insert(source.label, index).is_some()
        {
            return Err(refuse(FederationRefusalKind::InvalidRequest));
        }
        let binding = format!("source-{}", source.handle.identity().raw());
        if source
            .session
            .is_some_and(|session| !session.matches_source_binding(&binding))
        {
            return Err(refuse(FederationRefusalKind::InvalidRequest));
        }
    }
    let requested = selected(selection, &aliases).map_err(refuse)?;
    let mut normalized = request.clone();
    let mut single_source_projects = None;
    let selectors = match &mut normalized {
        QueryRequest::SearchKnowledge(input) => Some((&mut input.project, &mut input.projects)),
        QueryRequest::ReferenceSearch(input) => Some((&mut input.project, &mut input.projects)),
        QueryRequest::SymbolRead(input) => Some((&mut input.project, &mut single_source_projects)),
        QueryRequest::SymbolContext(input) => {
            Some((&mut input.project, &mut single_source_projects))
        }
        QueryRequest::RepoMap(input) => Some((&mut input.project, &mut single_source_projects)),
        QueryRequest::FileContext(input) => Some((&mut input.project, &mut single_source_projects)),
        QueryRequest::DependentSearch(input) => {
            Some((&mut input.project, &mut single_source_projects))
        }
        _ => None,
    };
    if let Some((project, projects)) = selectors {
        let nested_selection = match (project.as_ref(), projects.as_ref()) {
            (Some(_), Some(_)) => return Err(refuse(FederationRefusalKind::InvalidRequest)),
            (Some(project), None) => Some(SourceSelection::One(project.clone())),
            (None, Some(projects))
                if projects.len() == 1 && matches!(projects[0].as_str(), "all" | "*") =>
            {
                Some(SourceSelection::All)
            }
            (None, Some(projects)) => Some(SourceSelection::Many(projects.clone())),
            (None, None) => None,
        };
        if let Some(nested_selection) = &nested_selection {
            let nested = selected(nested_selection, &aliases).map_err(refuse)?;
            if nested != requested {
                return Err(refuse(FederationRefusalKind::InvalidRequest));
            }
        }
        *project = None;
        *projects = None;
    }
    let normalized_request_hash = crate::hash::digest_hex(
        &serde_json::to_vec(&(
            "symforge.embed.federated-source-request",
            API_VERSION,
            &normalized,
        ))
        .expect("adapted query serializes"),
    );
    let mut groups: Vec<(usize, ResolvedQuerySource)> = Vec::new();
    let mut seen = BTreeMap::new();
    for label in requested {
        let index = aliases[label];
        let source = &sources[index];
        let identity = source.handle.identity().raw();
        if let Some(&group) = seen.get(&identity) {
            let (prior_index, resolved): &mut (usize, ResolvedQuerySource) = &mut groups[group];
            let prior = &sources[*prior_index];
            if prior.policy != source.policy
                || prior.session.map(|session| session.identity())
                    != source.session.map(|session| session.identity())
            {
                return Err(refuse(FederationRefusalKind::InvalidRequest));
            }
            resolved.aliases.push(label.to_owned());
        } else {
            seen.insert(identity, groups.len());
            groups.push((
                index,
                ResolvedQuerySource {
                    label: label.to_owned(),
                    binding_identity: format!("source-{identity}"),
                    producing_runtime_identity: super::embed_query::process_instance_identity()
                        .to_owned(),
                    aliases: vec![label.to_owned()],
                },
            ));
        }
    }
    if let Some(stopped) = stop(control) {
        return Err(refuse(stop_refusal(stopped)));
    }
    let resolved_sources = groups
        .iter()
        .map(|(_, source)| source.clone())
        .collect::<Vec<_>>();
    let mut results = Vec::new();
    let mut usage = QueryUsage::default();
    let mut truncated = false;
    let mut completion = FederationCompletion::Complete;
    for (index, resolved) in &groups {
        if let Some(stopped) = stop(control) {
            if results.is_empty() {
                return Err(refuse(stop_refusal(stopped)));
            }
            completion = FederationCompletion::Stopped(stopped);
            truncated = true;
            break;
        }
        // One result envelope is a semantic row. Source authority identities
        // belong to provenance; the user-facing alias is returned payload.
        let envelope_bytes = resolved.label.len() as u64;
        let rows = u64::from(limits.max_results).saturating_sub(usage.rows);
        let bytes = u64::from(limits.max_bytes).saturating_sub(usage.decoded_bytes);
        if rows < 2 || bytes <= envelope_bytes {
            if results.is_empty() {
                return Err(refuse(FederationRefusalKind::BudgetTooSmall));
            }
            truncated = true;
            completion = FederationCompletion::Truncated;
            break;
        }
        let source = &sources[*index];
        let child_limits = QueryLimits {
            max_results: (rows - 1) as u32,
            max_bytes: (bytes - envelope_bytes) as u32,
        };
        let outcome = execute(source, &normalized, child_limits);
        let outcome = match outcome {
            Ok(claim) => {
                usage.rows += claim.usage().rows;
                usage.decoded_bytes += claim.usage().decoded_bytes;
                truncated |= claim.truncated();
                FederatedSourceOutcome::Claim(claim)
            }
            Err(refusal) => {
                if matches!(
                    refusal.kind(),
                    QueryRefusalKind::Cancelled | QueryRefusalKind::DeadlineExceeded
                ) {
                    let stopped = if refusal.kind() == QueryRefusalKind::Cancelled {
                        FederationStop::Cancelled
                    } else {
                        FederationStop::DeadlineExceeded
                    };
                    completion = FederationCompletion::Stopped(stopped);
                    truncated = true;
                }
                FederatedSourceOutcome::Refusal(refusal)
            }
        };
        usage.rows += 1;
        usage.decoded_bytes += envelope_bytes;
        results.push(FederatedSourceResult {
            source: resolved.clone(),
            normalized_request_hash: normalized_request_hash.clone(),
            outcome,
            policy: source.policy,
        });
        if matches!(completion, FederationCompletion::Stopped(_)) {
            break;
        }
    }
    let omitted_sources = (groups.len() - results.len()) as u32;
    if truncated && completion == FederationCompletion::Complete {
        completion = FederationCompletion::Truncated;
    }
    Ok(FederatedQueryClaim {
        canonical_argument_hash,
        resolved_sources,
        results,
        usage,
        truncated,
        omitted_sources,
        completion,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embed::parity::QueryPolicy;
    use crate::embed::{EmbeddedSourceSpec, ProcessIndexRuntime, SourceRuntimePhase};
    use std::time::{Duration, Instant};

    #[test]
    fn first_invoked_source_stop_keeps_its_attributable_refusal() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("lib.rs"), "pub fn loaded() {}\n").unwrap();
        let runtime = ProcessIndexRuntime::acquire().unwrap();
        let handle = runtime
            .open_embedded_source(EmbeddedSourceSpec::current_worktree(
                root.path().to_path_buf(),
            ))
            .unwrap();
        let until = Instant::now() + Duration::from_secs(20);
        while handle.runtime_view().phase != SourceRuntimePhase::Current {
            assert!(Instant::now() < until);
            std::thread::sleep(Duration::from_millis(5));
        }
        let session = handle.new_query_session().unwrap();
        let sources = [AdmittedQuerySource {
            label: "first",
            handle: &handle,
            session: Some(&session),
            policy: QueryPolicy::default(),
        }];
        let control = OperationControl::new(Duration::from_secs(20)).unwrap();
        let mut invoked = 0;
        let outcome = query_sources_using(
            &sources,
            &SourceSelection::All,
            &QueryRequest::File {
                path: "lib.rs".into(),
                start_line: None,
                end_line: None,
            },
            QueryLimits::default(),
            Some(&control),
            |source, request, limits| {
                invoked += 1;
                // Cancel after federation has invoked its first source. The actual
                // native query must refuse, and the coordinator must preserve it.
                control.cancel();
                source.handle.query_with_policy(
                    request,
                    limits,
                    source.policy,
                    source.session,
                    Some(&control),
                )
            },
        )
        .expect("an invoked source has attributable stopped evidence");
        assert_eq!(invoked, 1);
        assert_eq!(
            outcome.completion,
            FederationCompletion::Stopped(FederationStop::Cancelled)
        );
        assert_eq!(outcome.results.len(), 1);
        assert_eq!(outcome.results[0].source.label, "first");
        let FederatedSourceOutcome::Refusal(refusal) = &outcome.results[0].outcome else {
            panic!("refusal")
        };
        assert_eq!(refusal.kind(), QueryRefusalKind::Cancelled);
        assert_eq!(
            outcome.usage,
            QueryUsage {
                rows: 1,
                decoded_bytes: 5
            }
        );
        assert_eq!(session.revision(), 0);

        let before_invocation = query_sources(
            &sources,
            &SourceSelection::All,
            &QueryRequest::File {
                path: "lib.rs".into(),
                start_line: None,
                end_line: None,
            },
            QueryLimits::default(),
            Some(&control),
        )
        .unwrap_err();
        assert_eq!(before_invocation.kind, FederationRefusalKind::Cancelled);
    }
}
