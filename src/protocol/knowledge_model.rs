//! Deterministic repository-mental-model projections over one published generation.

use std::collections::{BTreeMap, BTreeSet};

use crate::domain::FileDisposition;
use crate::knowledge::guard_hit;
use crate::live_index::PublishedGeneration;
use crate::live_index::graph::SymbolId;
use crate::live_index::knowledge_authority::{
    AuthorityDomain, KnowledgeAuthorityRecord, KnowledgeVoice,
};
use crate::live_index::knowledge_bridge::{
    BridgeResolution, CodeAnchorId, DerivedCoverage, KnowledgeCard, KnowledgeLinkResolution,
    KnowledgeRole, RoleEvidence,
};

type CardCandidate<'a> = (
    &'a KnowledgeCard,
    &'a RoleEvidence,
    Option<&'a KnowledgeAuthorityRecord>,
);

pub(crate) fn render_code_knowledge_context(
    published: &PublishedGeneration,
    target: &CodeAnchorId,
    include_when_empty: bool,
) -> Option<String> {
    let mut selected = BTreeSet::new();
    if let Some(indices) = published.bridge.reverse_exact.get(target) {
        selected.extend(indices.iter().copied());
    }
    let target_path = match target {
        CodeAnchorId::File { path } => path.as_str(),
        CodeAnchorId::Symbol { symbol, .. } => symbol.path.as_str(),
    };
    for index in &published.bridge.ownership_selectors {
        let Some(link) = published.bridge.forward.get(*index as usize) else {
            continue;
        };
        let Some(selector) = evidence_text(published, &link.evidence) else {
            continue;
        };
        if ownership_selector_matches(selector.trim(), target_path) {
            selected.insert(*index);
        }
    }

    let mut ambiguous = 0usize;
    for (index, link) in published.bridge.forward.iter().enumerate() {
        if let BridgeResolution::Ambiguous {
            bounded_samples, ..
        } = &link.resolution
            && bounded_samples.iter().any(|sample| &sample.id == target)
        {
            ambiguous += 1;
            selected.insert(u32::try_from(index).unwrap_or(u32::MAX));
        }
    }
    let missing = 0_usize;
    if selected.is_empty() && ambiguous == 0 && missing == 0 && !include_when_empty {
        return None;
    }

    let mut links = selected
        .iter()
        .filter_map(|index| {
            published
                .bridge
                .forward
                .get(*index as usize)
                .map(|link| (*index, link))
        })
        .collect::<Vec<_>>();
    links.sort_by(|left, right| {
        (
            left.1.evidence.path.as_str(),
            left.1.evidence.byte_range.start,
            &left.1.id,
        )
            .cmp(&(
                right.1.evidence.path.as_str(),
                right.1.evidence.byte_range.start,
                &right.1.id,
            ))
    });
    let total = links.len();
    let shown = total.min(5);
    let overflow = total.saturating_sub(shown);

    let source = published.source.as_deref();
    let mut lines = vec!["Knowledge evidence:".to_string()];
    lines.push(format!(
        "  source={} publication={} content={} target={}",
        source.map_or("unknown", |value| value.source_id.as_str()),
        published.publication_generation,
        published.content_generation,
        code_anchor_id_label(target),
    ));
    lines.push(format!(
        "  counts total={} shown={} overflow={} ambiguous={} missing={}",
        total, shown, overflow, ambiguous, missing,
    ));
    lines.push(format!(
        "  coverage bridge={} authority={}",
        derived_coverage_label(&published.bridge.coverage),
        derived_coverage_label(&published.authority.coverage),
    ));

    for (ordinal, (index, link)) in links.into_iter().take(5).enumerate() {
        let authority = authority_for_evidence(published, &link.evidence);
        lines.push(format!(
            "  {}. {}:{} bytes={}..{} content_hash={} source={} generation={} link_id={} bridge_index={} resolution={} lifecycle={} voice={}",
            ordinal + 1,
            link.evidence.path,
            link.evidence.line_range.start,
            link.evidence.byte_range.start,
            link.evidence.byte_range.end,
            link.evidence.content_hash,
            link.evidence.source.source_id.as_str(),
            link.evidence.content_generation,
            link.id.0,
            index,
            bridge_resolution_label(&link.resolution),
            authority.map_or("unknown".to_string(), |record| snake_debug(
                &record.lifecycle
            )),
            authority.map_or("unknown", |record| voice_label(record.voice)),
        ));
    }

    let rendered = lines.join("\n");
    match guard_hit(&rendered, &[rendered.as_str()]) {
        Ok(safe) => Some(safe.into_inner().clone()),
        Err(failure) => Some(format!(
            "Knowledge evidence withheld by secret policy v{} ({} finding(s)).",
            failure.policy_version, failure.finding_count
        )),
    }
}

/// Preserve the indivisible provenance/count contract for an explicit
/// `sections=["knowledge"]` read. Generic line truncation can otherwise keep a
/// long symbol identity while dropping coverage, which is neither a complete
/// anchor nor a complete provenance-only fallback.
pub(crate) fn render_budgeted_code_knowledge_only(
    published: &PublishedGeneration,
    rendered: Option<&str>,
    max_tokens: Option<u64>,
) -> String {
    let source = published.source.as_deref();
    let fallback = format!(
        "Knowledge evidence:\n  source={} publication={} content={} target=unresolved\n  counts total=0 shown=0 overflow=0 ambiguous=0 missing=0\n  coverage bridge={} authority={}",
        source.map_or("unknown", |value| value.source_id.as_str()),
        published.publication_generation,
        published.content_generation,
        derived_coverage_label(&published.bridge.coverage),
        derived_coverage_label(&published.authority.coverage),
    );
    let rendered = rendered.unwrap_or(&fallback);
    let normal = format!(
        "Trust: exact source evidence | publication {} | content {} | current\nScope: requested code anchor\nEvidence: reverse knowledge bridge\n\n{rendered}",
        published.publication_generation, published.content_generation,
    );
    let Some(max_tokens) = max_tokens.filter(|value| *value > 0) else {
        return normal;
    };
    if normal.len() <= (max_tokens as usize).saturating_mul(4) {
        return normal;
    }

    let counts = rendered
        .lines()
        .find(|line| line.trim_start().starts_with("counts "))
        .map(str::trim_start)
        .unwrap_or("counts total=0 shown=0 overflow=0 ambiguous=0 missing=0");
    let coverage = rendered
        .lines()
        .find(|line| line.trim_start().starts_with("coverage "))
        .map(str::trim_start)
        .unwrap_or("coverage bridge=unknown authority=unknown");
    let compact = format!(
        "Trust: exact source evidence | current | output_coverage=degraded\nIdentity: source={} publication={} content={}\nScope: requested code anchor\nEvidence: reverse knowledge bridge\n\nKnowledge evidence:\n  {counts}\n  {coverage}",
        source.map_or("unknown", |value| value.source_id.as_str()),
        published.publication_generation,
        published.content_generation,
    );
    if compact.len() <= (max_tokens as usize).saturating_mul(4) {
        compact
    } else {
        let minimum_tokens = compact.len().div_ceil(4);
        format!(
            "Error: max_tokens is too small for atomic knowledge provenance; minimum_tokens={minimum_tokens}."
        )
    }
}

/// Apply a caller budget without allowing the code portion of a combined
/// context response to consume the entire budget and erase the required
/// knowledge provenance. If generic truncation cannot retain the indivisible
/// source/count/coverage block, the response degrades to that block alone.
pub(crate) fn enforce_budgeted_code_context_with_knowledge(
    published: &PublishedGeneration,
    assembled: String,
    rendered: Option<&str>,
    max_tokens: Option<u64>,
) -> (String, bool) {
    let (budgeted, truncated) =
        crate::protocol::format::enforce_token_budget_flagged(assembled, max_tokens);
    if !truncated || rendered.is_none() {
        return (budgeted, truncated);
    }

    let complete_provenance =
        budgeted
            .split_once("Knowledge evidence:")
            .is_some_and(|(_, section)| {
                section.contains("source=")
                    && section.contains("publication=")
                    && section.contains("content=")
                    && section.contains("counts total=")
                    && section.contains("overflow=")
                    && section.contains("ambiguous=")
                    && section.contains("missing=")
                    && section.contains("coverage bridge=")
                    && section.contains("authority=")
            });
    if complete_provenance {
        (budgeted, true)
    } else {
        (
            render_budgeted_code_knowledge_only(published, rendered, max_tokens),
            true,
        )
    }
}

pub(crate) fn resolve_symbol_code_anchor(
    published: &PublishedGeneration,
    path: &str,
    name: &str,
    kind: Option<&str>,
    symbol_line: Option<u32>,
) -> Option<CodeAnchorId> {
    let file = published.live.files.get(path)?;
    let mut matches = file.symbols.iter().filter(|symbol| {
        symbol.name == name
            && kind.is_none_or(|kind| symbol.kind.to_string().eq_ignore_ascii_case(kind))
            && symbol_line.is_none_or(|line| symbol.line_range.0.saturating_add(1) == line)
    });
    let symbol = matches.next()?;
    if matches.next().is_some() {
        return None;
    }
    Some(CodeAnchorId::Symbol {
        symbol: SymbolId {
            path: path.to_string(),
            name: symbol.name.clone(),
            kind: symbol.kind,
        },
        start_line: symbol.line_range.0.saturating_add(1),
    })
}

fn evidence_text<'a>(
    published: &'a PublishedGeneration,
    evidence: &crate::live_index::knowledge_bridge::KnowledgeAnchor,
) -> Option<&'a str> {
    let file = published.live.files.get(&evidence.path)?;
    let start = usize::try_from(evidence.byte_range.start).ok()?;
    let end = usize::try_from(evidence.byte_range.end).ok()?;
    std::str::from_utf8(file.content.get(start..end)?).ok()
}

fn ownership_selector_matches(selector: &str, path: &str) -> bool {
    let mut pattern = selector.trim_start_matches('/').replace('\\', "/");
    if pattern.ends_with('/') {
        pattern.push_str("**");
    }
    if !pattern.contains('/') {
        pattern = format!("**/{pattern}");
    }
    let Ok(glob) = globset::GlobBuilder::new(&pattern)
        .literal_separator(true)
        .backslash_escape(false)
        .build()
    else {
        return false;
    };
    glob.compile_matcher().is_match(path)
}

fn authority_for_evidence<'a>(
    published: &'a PublishedGeneration,
    evidence: &crate::live_index::knowledge_bridge::KnowledgeAnchor,
) -> Option<&'a KnowledgeAuthorityRecord> {
    published.authority.records.iter().find(|record| {
        record.unit.source == evidence.source
            && record.unit.path == evidence.path
            && record.unit.content_hash == evidence.content_hash
            && record.unit.byte_range.start <= evidence.byte_range.start
            && record.unit.byte_range.end >= evidence.byte_range.end
    })
}

fn code_anchor_id_label(target: &CodeAnchorId) -> String {
    match target {
        CodeAnchorId::File { path } => format!("file:{path}"),
        CodeAnchorId::Symbol { symbol, start_line } => format!(
            "symbol:{}:{}:{}:{}",
            symbol.path, symbol.name, symbol.kind, start_line
        ),
    }
}

fn bridge_resolution_label(resolution: &BridgeResolution) -> String {
    match resolution {
        BridgeResolution::ResolvedExact(_) => "resolved_exact".to_string(),
        BridgeResolution::ResolvedDeclaredSet { matched_count, .. } => {
            format!("resolved_declared_set({matched_count})")
        }
        BridgeResolution::Ambiguous {
            candidate_count,
            bounded_samples,
        } => format!(
            "ambiguous({candidate_count},samples=[{}])",
            bounded_samples
                .iter()
                .map(|sample| code_anchor_id_label(&sample.id))
                .collect::<Vec<_>>()
                .join(";")
        ),
        BridgeResolution::Missing => "missing".to_string(),
    }
}

pub(crate) fn render_repository_knowledge_map(published: &PublishedGeneration) -> String {
    let record_by_unit = published
        .authority
        .records
        .iter()
        .map(|record| (record.unit.id.clone(), record))
        .collect::<BTreeMap<_, _>>();
    let mut current = BTreeMap::<KnowledgeRole, Vec<CardCandidate<'_>>>::new();
    let mut intent = BTreeMap::<KnowledgeRole, Vec<CardCandidate<'_>>>::new();
    let mut present_roles = BTreeSet::new();

    for card in &published.bridge.cards {
        let record = record_by_unit.get(&card.anchor.id).copied();
        for (role, evidence) in &card.roles {
            present_roles.insert(*role);
            match record.map(|record| record.authority_domain) {
                Some(AuthorityDomain::CurrentImplementation | AuthorityDomain::Operations) => {
                    current
                        .entry(*role)
                        .or_default()
                        .push((card, evidence, record));
                }
                Some(
                    AuthorityDomain::NormativeIntent
                    | AuthorityDomain::Decision
                    | AuthorityDomain::Governance,
                ) => {
                    intent
                        .entry(*role)
                        .or_default()
                        .push((card, evidence, record));
                }
                Some(AuthorityDomain::HistoricalRecord | AuthorityDomain::Unknown) | None => {}
            }
        }
    }

    let source = published.source.as_deref();
    let source_version = published.source_version.as_deref();
    let manifest = published.manifest.as_deref();
    let mut lines = vec!["Repository knowledge:".to_string()];
    lines.push(format!(
        "  source={} repository={} publication={} content={} project={}",
        source.map_or("unknown", |value| value.source_id.as_str()),
        source.map_or("unknown", |value| value.repository_id.as_str()),
        published.publication_generation,
        published.content_generation,
        published.project_generation,
    ));
    lines.push(format!(
        "  version branch={} commit={} working_tree={}",
        source_version
            .and_then(|value| value.branch.as_deref())
            .unwrap_or("unknown"),
        source_version
            .and_then(|value| value.commit.as_deref())
            .unwrap_or("unknown"),
        source_version.map_or("unknown".to_string(), |value| snake_debug(
            &value.working_tree
        )),
    ));

    render_role_lane(&mut lines, "Current roles", &mut current);
    render_role_lane(&mut lines, "Intent roles", &mut intent);

    let all_roles = [
        KnowledgeRole::Architecture,
        KnowledgeRole::OwnershipGovernance,
        KnowledgeRole::DecisionInvariant,
        KnowledgeRole::SchemaContract,
        KnowledgeRole::Operations,
        KnowledgeRole::TestingSecurity,
        KnowledgeRole::PlanHandoff,
        KnowledgeRole::Other,
    ];
    let missing = all_roles
        .iter()
        .filter(|role| !present_roles.contains(role))
        .map(role_label)
        .collect::<Vec<_>>();
    lines.push(format!(
        "Missing roles: {}",
        if missing.is_empty() {
            "none".to_string()
        } else {
            format!("{} (unknown/no declared evidence)", missing.join(", "))
        }
    ));

    let voice_count = |voice| {
        published
            .authority
            .records
            .iter()
            .filter(|record| record.voice == voice)
            .count()
    };
    lines.push(format!(
        "Hygiene: needs_review={} history_only={} suppressed={}",
        voice_count(KnowledgeVoice::NeedsReview),
        voice_count(KnowledgeVoice::HistoryOnly),
        voice_count(KnowledgeVoice::Suppressed),
    ));

    let ambiguous = published
        .bridge
        .forward
        .iter()
        .filter(|link| matches!(link.resolution, BridgeResolution::Ambiguous { .. }))
        .count();
    let missing_code = published
        .bridge
        .forward
        .iter()
        .filter(|link| matches!(link.resolution, BridgeResolution::Missing))
        .count();
    let missing_knowledge = published
        .bridge
        .knowledge_links
        .iter()
        .filter(|link| matches!(link.resolution, KnowledgeLinkResolution::Missing))
        .count();
    let unknown_records = published
        .authority
        .records
        .iter()
        .filter(|record| {
            record.authority_domain == AuthorityDomain::Unknown
                || record.voice == KnowledgeVoice::Unknown
        })
        .count();
    let (withheld, unreadable) = manifest.map_or((0, 0), |manifest| {
        manifest
            .entries
            .iter()
            .fold((0, 0), |(withheld, unreadable), entry| {
                match entry.disposition {
                    FileDisposition::MetadataOnly { .. } | FileDisposition::HardSkip { .. } => {
                        (withheld + 1, unreadable)
                    }
                    FileDisposition::Unreadable { .. }
                    | FileDisposition::UnstableDuringRead
                    | FileDisposition::AbortedCircuitBreaker => (withheld, unreadable + 1),
                    FileDisposition::Indexed { .. } => (withheld, unreadable),
                }
            })
    });
    lines.push(format!(
        "Uncertainty: unknown_records={} ambiguous_anchors={} missing_anchors={} withheld={} unreadable={}",
        unknown_records,
        ambiguous,
        missing_code.saturating_add(missing_knowledge),
        withheld,
        unreadable,
    ));
    lines.push(format!(
        "Coverage: manifest={} bridge={} authority={} temporal_complete_to_root={} temporal_limitations={} freshness={}",
        manifest.map_or("unknown".to_string(), |value| snake_debug(&value.coverage)),
        derived_coverage_label(&published.bridge.coverage),
        derived_coverage_label(&published.authority.coverage),
        published.code_signals.coverage.complete_to_root,
        published.code_signals.coverage.limitations.len(),
        snake_debug(published.freshness.as_ref()),
    ));
    lines.push(format!(
        "Digests: manifest={} policy={}",
        manifest.map_or("unknown", |value| value.digest.as_str()),
        published.authority.policy_digest,
    ));

    let rendered = lines.join("\n");
    match guard_hit(&rendered, &[rendered.as_str()]) {
        Ok(safe) => safe.into_inner().clone(),
        Err(failure) => format!(
            "Repository knowledge withheld by secret policy v{} ({} finding(s)).",
            failure.policy_version, failure.finding_count
        ),
    }
}

fn render_role_lane(
    lines: &mut Vec<String>,
    title: &str,
    roles: &mut BTreeMap<KnowledgeRole, Vec<CardCandidate<'_>>>,
) {
    let role_count = roles.len();
    let card_count = roles.values().map(Vec::len).sum::<usize>();
    lines.push(format!(
        "{title}: roles={} cards={} shown={} overflow={}",
        role_count,
        card_count,
        role_count,
        card_count.saturating_sub(role_count),
    ));
    if roles.is_empty() {
        lines.push("  none".to_string());
        return;
    }
    for (role, candidates) in roles {
        candidates.sort_by(|left, right| card_candidate_key(left).cmp(&card_candidate_key(right)));
        let Some((card, evidence, record)) = candidates.first() else {
            continue;
        };
        let voice = record.map_or("unknown", |record| voice_label(record.voice));
        // Was `role_evidence_anchor(evidence)`. RoleEvidence no longer carries
        // an anchor because it was always a clone of this same `card.anchor`,
        // so the rendered line is byte-identical.
        let evidence_anchor = &card.anchor;
        lines.push(format!(
            "  {} {}:{} unit_id={}#{}:{} bytes={}..{} content_hash={} source={} generation={} voice={} evidence={} evidence_anchor={}#{}:{}@{}..{} overflow={}",
            role_label(role),
            card.anchor.path,
            card.anchor.line_range.start,
            card.anchor.id.path,
            card.anchor.id.content_hash,
            card.anchor.id.start_byte,
            card.anchor.byte_range.start,
            card.anchor.byte_range.end,
            card.anchor.content_hash,
            card.anchor.source.source_id.as_str(),
            card.anchor.content_generation,
            voice,
            role_evidence_label(evidence),
            evidence_anchor.id.path,
            evidence_anchor.id.content_hash,
            evidence_anchor.id.start_byte,
            evidence_anchor.byte_range.start,
            evidence_anchor.byte_range.end,
            candidates.len().saturating_sub(1),
        ));
    }
}

fn card_candidate_key<'a>(candidate: &CardCandidate<'a>) -> (u8, &'a str, u32, u32) {
    (
        match candidate.1 {
            RoleEvidence::DeclaredSpan => 0,
            RoleEvidence::HeadingRule { .. } => 1,
            RoleEvidence::PathConvention { .. } => 2,
        },
        candidate.0.anchor.path.as_str(),
        candidate.0.anchor.byte_range.start,
        candidate.0.anchor.byte_range.end,
    )
}

fn role_evidence_label(evidence: &RoleEvidence) -> String {
    match evidence {
        RoleEvidence::DeclaredSpan => "declared_span".to_string(),
        RoleEvidence::HeadingRule { rule_id, .. } => format!("heading:{rule_id}"),
        RoleEvidence::PathConvention { rule_id, .. } => format!("path:{rule_id}"),
    }
}

fn role_label(role: &KnowledgeRole) -> &'static str {
    match role {
        KnowledgeRole::Architecture => "architecture",
        KnowledgeRole::OwnershipGovernance => "ownership_governance",
        KnowledgeRole::DecisionInvariant => "decision_invariant",
        KnowledgeRole::SchemaContract => "schema_contract",
        KnowledgeRole::Operations => "operations",
        KnowledgeRole::TestingSecurity => "testing_security",
        KnowledgeRole::PlanHandoff => "plan_handoff",
        KnowledgeRole::Other => "other",
    }
}

fn voice_label(voice: KnowledgeVoice) -> &'static str {
    match voice {
        KnowledgeVoice::Current => "current",
        KnowledgeVoice::Intent => "intent",
        KnowledgeVoice::NeedsReview => "needs_review",
        KnowledgeVoice::Unknown => "unknown",
        KnowledgeVoice::HistoryOnly => "history_only",
        KnowledgeVoice::Suppressed => "suppressed",
    }
}

fn derived_coverage_label(coverage: &DerivedCoverage) -> String {
    match coverage {
        DerivedCoverage::Complete => "complete".to_string(),
        DerivedCoverage::Truncated { breaches } => format!(
            "truncated({})",
            breaches
                .iter()
                .map(|breach| format!("{}:{}", snake_debug(&breach.kind), breach.omitted))
                .collect::<Vec<_>>()
                .join(",")
        ),
    }
}

fn snake_debug(value: &impl std::fmt::Debug) -> String {
    let debug = format!("{value:?}");
    let mut output = String::with_capacity(debug.len());
    for (index, character) in debug.chars().enumerate() {
        if character.is_ascii_uppercase() {
            if index > 0 {
                output.push('_');
            }
            output.push(character.to_ascii_lowercase());
        } else {
            output.push(character);
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;
    use crate::live_index::LiveIndex;

    #[test]
    fn repository_map_reports_current_intent_missing_roles_hygiene_and_coverage() {
        let root = TempDir::new().unwrap();
        fs::write(
            root.path().join("README.md"),
            "# Architecture\nCurrent system shape.\n",
        )
        .unwrap();
        fs::create_dir_all(root.path().join("docs/design")).unwrap();
        // The operations ROLE comes from the "runbook" path token and the intent
        // DOMAIN from the "design" path token. An Operations heading here would
        // now assign the operations domain instead: heading evidence outranks
        // path conventions, which would move this card into the current lane.
        fs::write(
            root.path().join("docs/design/runbook.md"),
            "# Proposed operating model\nProposed operating model.\n",
        )
        .unwrap();
        fs::write(
            root.path().join("docs/contributors.md"),
            "# Contributors\nAlice and Bob contributed.\n",
        )
        .unwrap();

        let shared = LiveIndex::load(root.path()).unwrap();
        let published = shared.published_generation();
        let rendered = render_repository_knowledge_map(&published);

        assert!(rendered.contains("Repository knowledge:"));
        assert!(rendered.contains(&format!(
            "publication={} content={}",
            published.publication_generation, published.content_generation
        )));
        assert!(rendered.contains("Current roles:"));
        assert!(rendered.contains("architecture README.md:1"));
        assert!(rendered.contains("cards=1 shown=1 overflow=0"));
        assert!(rendered.contains("unit_id=README.md#"));
        assert!(rendered.contains("bytes="));
        assert!(rendered.contains("content_hash="));
        assert!(rendered.contains("evidence_anchor=README.md#"));
        assert!(rendered.contains("Intent roles:"));
        assert!(rendered.contains("operations docs/design/runbook.md:1"));
        assert!(rendered.contains("Missing roles:"));
        assert!(rendered.contains("ownership_governance"));
        assert!(rendered.contains("Hygiene:"));
        assert!(rendered.contains("Coverage:"));
        assert!(!rendered.contains("ownership_governance docs/contributors.md"));
    }

    #[test]
    fn repository_map_is_byte_stable_and_caps_each_role_to_one_card() {
        let root = TempDir::new().unwrap();
        fs::write(
            root.path().join("README.md"),
            "# Architecture\nCurrent system shape.\n## Architecture\nSecond shape.\n",
        )
        .unwrap();

        let shared = LiveIndex::load(root.path()).unwrap();
        let published = shared.published_generation();
        let first = render_repository_knowledge_map(&published);
        let second = render_repository_knowledge_map(&published);

        assert_eq!(first, second);
        assert_eq!(first.matches("architecture README.md:").count(), 1);
        assert!(first.contains("overflow=1"));
    }

    #[test]
    fn code_context_caps_exact_and_declared_backlinks_with_complete_counts() {
        let root = TempDir::new().unwrap();
        fs::create_dir_all(root.path().join("src")).unwrap();
        fs::write(root.path().join("src/lib.rs"), "pub fn launch() {}\n").unwrap();
        fs::create_dir_all(root.path().join("docs")).unwrap();
        for ordinal in 0..6 {
            fs::write(
                root.path().join(format!("docs/link-{ordinal}.md")),
                format!("# Link {ordinal}\n[code](../src/lib.rs)\n"),
            )
            .unwrap();
        }
        fs::create_dir_all(root.path().join(".github")).unwrap();
        fs::write(
            root.path().join(".github/CODEOWNERS"),
            "/src/*.rs @runtime-team\n",
        )
        .unwrap();

        let shared = LiveIndex::load(root.path()).unwrap();
        let published = shared.published_generation();
        let target = CodeAnchorId::File {
            path: "src/lib.rs".to_string(),
        };
        let rendered = render_code_knowledge_context(&published, &target, true).unwrap();

        assert!(rendered.contains("Knowledge evidence:"));
        assert!(rendered.contains(&format!(
            "publication={} content={}",
            published.publication_generation, published.content_generation
        )));
        assert!(rendered.contains("total=7 shown=5 overflow=2"));
        assert_eq!(rendered.matches(" link_id=").count(), 5);
        assert!(rendered.contains("lifecycle="));
        assert!(rendered.contains("voice="));
        assert!(rendered.contains("bridge=complete authority=complete"));

        let absent = render_code_knowledge_context(
            &published,
            &CodeAnchorId::File {
                path: "assets/other.bin".to_string(),
            },
            false,
        );
        assert!(absent.is_none());
    }

    #[test]
    fn code_context_renders_ambiguous_bridge_state_with_bounded_exact_samples() {
        let root = TempDir::new().unwrap();
        fs::create_dir_all(root.path().join("src/a")).unwrap();
        fs::create_dir_all(root.path().join("src/b")).unwrap();
        fs::create_dir_all(root.path().join("docs")).unwrap();
        fs::write(root.path().join("src/a/lib.rs"), "pub fn launch() {}\n").unwrap();
        fs::write(root.path().join("src/b/lib.rs"), "pub fn launch() {}\n").unwrap();
        fs::write(root.path().join("docs/link.md"), "# Link\n`launch`\n").unwrap();

        let shared = LiveIndex::load(root.path()).unwrap();
        let published = shared.published_generation();
        let target =
            resolve_symbol_code_anchor(&published, "src/a/lib.rs", "launch", Some("fn"), Some(1))
                .expect("target");
        let rendered = render_code_knowledge_context(&published, &target, true).unwrap();

        assert!(rendered.contains("ambiguous=1"), "{rendered}");
        assert!(rendered.contains("resolution=ambiguous(2"), "{rendered}");
        assert!(
            rendered.contains("samples=[symbol:src/a/lib.rs:launch"),
            "{rendered}"
        );
        assert!(rendered.contains("bytes="), "{rendered}");
        assert!(rendered.contains("content_hash="), "{rendered}");
    }
}
